//! 同步引擎：整项配置用版本向量合并，题库用「基线哈希 + 行级」合并。
//!
//! 规则由需求决定：
//! - 两端改过同一项且内容不同 → 该项/该行**冻结**，不再参与后续同步；
//! - 只有一端改过 → 直接把改动推给对端；
//! - 内容相同 → 合并版本向量即可。
//!
//! 引擎只与同网络（同账号）的设备通信，请求经 EasyTier 数据面的本机桥接端口、
//! 带上 mesh token。每次同步只交换元数据，值按需拉取。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::items::{
    content_hash, item_def, paper_hash, paper_row_key, question_hash, question_row_key, ItemContext,
    ITEM_DEFS, UOOC_QA_BANK,
};
use super::state::{ItemState, RowState, SyncDatabase, VersionVector};
use crate::mesh::{api::TOKEN_HEADER, MeshStore};
use szu_core::storage::SharedCredentialStore;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const ROW_BATCH: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemMeta {
    pub id: String,
    pub hash: String,
    pub vector: VersionVector,
    pub conflict: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPayload {
    pub hash: String,
    pub vector: VersionVector,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub conflict: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowDigest {
    pub key: String,
    pub hash: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BankDigest {
    pub questions: Vec<RowDigest>,
    pub papers: Vec<RowDigest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStateResponse {
    pub items: Vec<ItemMeta>,
    pub bank: BankDigest,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankRowsRequest {
    pub questions: Vec<String>,
    pub papers: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankRowsResponse {
    pub questions: Vec<szu_uooc::uooc::quiz::qa::LocalBankQuestion>,
    pub papers: Vec<szu_uooc::uooc::quiz::qa::SubmittedPaperRecord>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankApplyRequest {
    pub questions: Vec<szu_uooc::uooc::quiz::qa::LocalBankQuestion>,
    pub papers: Vec<szu_uooc::uooc::quiz::qa::SubmittedPaperRecord>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankApplyResponse {
    pub applied: usize,
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub peers: usize,
    pub applied: usize,
    pub pushed: usize,
    pub conflicts: usize,
    pub bank_rows: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncItemView {
    pub id: String,
    pub label: String,
    pub collection: bool,
    /// `conflict` / `synced` / `waiting`
    pub state: String,
}

/// 一次比较的结论。`Adopt` 表示拉取对端，`Push` 表示把本机推过去。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeDecision {
    Same,
    Adopt,
    Push,
    Conflict,
}

/// 纯函数：根据内容哈希与版本向量决定合并方向。
pub fn decide(
    local_hash: &str,
    local_vector: &VersionVector,
    remote_hash: &str,
    remote_vector: &VersionVector,
) -> MergeDecision {
    if remote_hash == local_hash {
        return MergeDecision::Same;
    }
    // 一侧没有这一项：有内容的一侧直接覆盖，不算冲突。
    if local_hash.is_empty() {
        return MergeDecision::Adopt;
    }
    if remote_hash.is_empty() {
        return MergeDecision::Push;
    }
    if remote_vector == local_vector {
        return MergeDecision::Conflict;
    }
    if remote_vector.covers(local_vector) {
        return MergeDecision::Adopt;
    }
    if local_vector.covers(remote_vector) {
        return MergeDecision::Push;
    }
    MergeDecision::Conflict
}

struct PeerEndpoint {
    label: String,
    port: u16,
    token: Arc<str>,
}

pub struct SyncStore {
    root: OnceLock<PathBuf>,
    credentials: OnceLock<SharedCredentialStore>,
    db: tokio::sync::OnceCell<SyncDatabase>,
    pass: tokio::sync::Mutex<()>,
    last_summary: std::sync::Mutex<SyncSummary>,
}

impl SyncStore {
    pub fn new() -> Self {
        Self {
            root: OnceLock::new(),
            credentials: OnceLock::new(),
            db: tokio::sync::OnceCell::new(),
            pass: tokio::sync::Mutex::new(()),
            last_summary: std::sync::Mutex::new(SyncSummary::default()),
        }
    }

    pub fn initialize(
        &self,
        app: &AppHandle,
        credentials: SharedCredentialStore,
    ) -> Result<(), String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法确定应用数据目录".to_string())?;
        let _ = self.root.set(data_dir.join("mesh"));
        let _ = self.credentials.set(credentials);
        Ok(())
    }

    async fn db(&self) -> Result<&SyncDatabase, String> {
        let root = self
            .root
            .get()
            .ok_or_else(|| "设备同步模块未初始化".to_string())?;
        self.db
            .get_or_try_init(|| async { SyncDatabase::open(root).await })
            .await
    }

    fn items(&self, app: &AppHandle) -> ItemContext {
        ItemContext {
            app: app.clone(),
            credentials: self
                .credentials
                .get()
                .cloned()
                .unwrap_or_else(|| Arc::new(szu_core::storage::PlatformCredentialStore)),
        }
    }

    pub fn last_summary(&self) -> SyncSummary {
        self.last_summary
            .lock()
            .map(|summary| summary.clone())
            .unwrap_or_default()
    }

    fn set_summary(&self, summary: SyncSummary) {
        if let Ok(mut current) = self.last_summary.lock() {
            *current = summary;
        }
    }

    /// 一次完整同步：逐个对端交换整项配置与题库增量。
    pub async fn run_pass(&self, app: &AppHandle) -> Result<SyncSummary, String> {
        let _guard = self.pass.lock().await;
        let mut summary = SyncSummary::default();
        let peers = peer_endpoints(app);
        summary.peers = peers.len();
        let client = szu_core::netproxy::overlay_client_builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("同步客户端创建失败: {error}"))?;
        if let Err(error) = self.refresh_local_items(app).await {
            summary.errors.push(error);
        }
        for peer in peers {
            if let Err(error) = self.reconcile_peer(app, &client, &peer, &mut summary).await {
                summary
                    .errors
                    .push(format!("与 {} 同步失败: {error}", peer.label));
            }
        }
        if summary.applied > 0 || summary.bank_rows > 0 {
            szu_core::onboarding::adopt_synced_items();
        }
        self.set_summary(summary.clone());
        Ok(summary)
    }

    /// 检测本机内容是否被改动；改动则推进自己的计数器。
    async fn refresh_local_items(&self, app: &AppHandle) -> Result<(), String> {
        let items = self.items(app);
        let db = self.db().await?;
        let states = db.load_items().await?;
        let device_id = device_id();
        for def in ITEM_DEFS.iter().filter(|def| !def.collection) {
            let value = items.read(def.id).await?;
            let hash = value.as_ref().map(content_hash).unwrap_or_default();
            let Some(mut state) = states.get(def.id).cloned() else {
                // 本机从未有过这一项：不要为「空值」创建版本向量，否则对端会把
                // 我们的空状态当成一次并发修改而冻结。
                if value.is_none() {
                    continue;
                }
                let mut vector = VersionVector::default();
                vector.bump(&device_id);
                db.save_item(
                    def.id,
                    &ItemState {
                        vector,
                        hash,
                        conflict: false,
                    },
                )
                .await?;
                continue;
            };
            if state.conflict {
                continue;
            }
            if state.vector.0.is_empty() || state.hash != hash {
                state.vector.bump(&device_id);
                state.hash = hash;
                db.save_item(def.id, &state).await?;
            }
        }
        Ok(())
    }

    async fn local_item(
        &self,
        app: &AppHandle,
        id: &str,
    ) -> Result<(Option<Value>, ItemState), String> {
        let value = self.items(app).read(id).await?;
        let hash = value.as_ref().map(content_hash).unwrap_or_default();
        let state = self
            .db()
            .await?
            .load_items()
            .await?
            .get(id)
            .cloned()
            .unwrap_or(ItemState {
                vector: VersionVector::default(),
                hash,
                conflict: false,
            });
        Ok((value, state))
    }

    async fn reconcile_peer(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        peer: &PeerEndpoint,
        summary: &mut SyncSummary,
    ) -> Result<(), String> {
        let state = fetch_state(client, peer).await?;
        let remote_items: BTreeMap<&str, &ItemMeta> =
            state.items.iter().map(|item| (item.id.as_str(), item)).collect();

        for def in ITEM_DEFS.iter().filter(|def| !def.collection) {
            let (_, local) = self.local_item(app, def.id).await?;
            if local.conflict {
                continue;
            }
            let Some(remote) = remote_items.get(def.id) else {
                if self.push_item(app, client, peer, def.id).await? {
                    summary.pushed += 1;
                }
                continue;
            };
            if remote.conflict {
                continue;
            }
            match decide(&local.hash, &local.vector, &remote.hash, &remote.vector) {
                MergeDecision::Same => {
                    self.save_item_state(app, def.id, &local, &remote.vector, remote.hash.clone())
                        .await?;
                }
                MergeDecision::Adopt => {
                    if self.pull_item(app, client, peer, def.id, remote).await? {
                        summary.applied += 1;
                    }
                }
                MergeDecision::Push => {
                    if self.push_item(app, client, peer, def.id).await? {
                        summary.pushed += 1;
                    }
                }
                MergeDecision::Conflict => {
                    self.freeze_item(app, def.id, &local).await?;
                    summary.conflicts += 1;
                }
            }
        }

        self.reconcile_bank(app, client, peer, &state.bank, summary)
            .await
    }

    async fn save_item_state(
        &self,
        _app: &AppHandle,
        id: &str,
        local: &ItemState,
        remote: &VersionVector,
        hash: String,
    ) -> Result<(), String> {
        let mut next = local.clone();
        next.vector.merge(remote);
        next.hash = hash;
        self.db().await?.save_item(id, &next).await
    }

    async fn freeze_item(&self, _app: &AppHandle, id: &str, local: &ItemState) -> Result<(), String> {
        let mut next = local.clone();
        next.conflict = true;
        self.db().await?.save_item(id, &next).await
    }

    async fn pull_item(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        peer: &PeerEndpoint,
        id: &str,
        remote: &ItemMeta,
    ) -> Result<bool, String> {
        let payload = client
            .get(format!("http://127.0.0.1:{}/v1/sync/item/{id}", peer.port))
            .header(TOKEN_HEADER, peer.token.as_ref())
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<ItemPayload>()
            .await
            .map_err(|error| format!("对端同步项格式无效: {error}"))?;
        let Some(value) = payload.value.clone() else {
            return Ok(false);
        };
        self.items(app).write(id, value).await?;
        let (_, mut local) = self.local_item(app, id).await?;
        local.vector.merge(&payload.vector);
        local.vector.merge(&remote.vector);
        // 以对端内容作为基线；如果本地写入后做了规范化（哈希变化），下一次
        // 本地刷新会把它当成一次本地修改推回去。
        local.hash = payload.hash;
        self.db().await?.save_item(id, &local).await?;
        Ok(true)
    }

    async fn push_item(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        peer: &PeerEndpoint,
        id: &str,
    ) -> Result<bool, String> {
        let (value, local) = self.local_item(app, id).await?;
        if local.conflict || value.is_none() {
            return Ok(false);
        }
        let payload = ItemPayload {
            hash: local.hash.clone(),
            vector: local.vector.clone(),
            value,
            conflict: false,
        };
        let response = client
            .post(format!("http://127.0.0.1:{}/v1/sync/item/{id}", peer.port))
            .header(TOKEN_HEADER, peer.token.as_ref())
            .json(&payload)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<ItemPayload>()
            .await
            .map_err(|error| format!("对端同步结果格式无效: {error}"))?;
        if response.conflict {
            self.freeze_item(app, id, &local).await?;
            return Ok(false);
        }
        self.save_item_state(app, id, &local, &response.vector, local.hash.clone())
            .await?;
        Ok(true)
    }

    async fn reconcile_bank(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        peer: &PeerEndpoint,
        remote: &BankDigest,
        summary: &mut SyncSummary,
    ) -> Result<(), String> {
        let items = self.items(app);
        let snapshot = items.bank_snapshot().await?;
        let db = self.db().await?;
        let rows = db.load_rows().await?;
        let local_questions: HashMap<String, _> = snapshot
            .questions
            .iter()
            .map(|question| (question_row_key(question), question))
            .collect();
        let local_papers: HashMap<String, _> = snapshot
            .papers
            .iter()
            .map(|paper| (paper_row_key(paper), paper))
            .collect();

        let mut want_questions = Vec::new();
        let mut want_papers = Vec::new();
        let mut push_questions = Vec::new();
        let mut push_papers = Vec::new();
        let mut matched = Vec::new();

        let mut row_conflicts = Vec::new();
        classify_rows(
            &remote.questions,
            &local_questions,
            &rows,
            question_hash,
            &mut want_questions,
            &mut push_questions,
            &mut row_conflicts,
            &mut matched,
        );
        classify_rows(
            &remote.papers,
            &local_papers,
            &rows,
            paper_hash,
            &mut want_papers,
            &mut push_papers,
            &mut row_conflicts,
            &mut matched,
        );
        // 两边一致的行也要落基线：下一次任一侧改动才能按三路规则判断冲突。
        for (key, hash) in matched {
            let conflict = rows.get(&key).is_some_and(|row| row.conflict);
            db.save_row(&key, &RowState { hash, conflict }).await?;
        }
        for key in row_conflicts {
            let hash = rows.get(&key).map(|row| row.hash.clone()).unwrap_or_default();
            db.save_row(&key, &RowState { hash, conflict: true }).await?;
            summary.conflicts += 1;
        }

        if !want_questions.is_empty() || !want_papers.is_empty() {
            let fetched = fetch_rows(client, peer, &want_questions, &want_papers).await?;
            let applied_questions = fetched.questions.len();
            let applied_papers = fetched.papers.len();
            items.apply_questions(fetched.questions.clone()).await?;
            items.apply_papers(fetched.papers.clone()).await?;
            summary.bank_rows += applied_questions + applied_papers;

            // 应用后本地可能因为合并（答案并集、上传标记）产生新内容：以应用后的
            // 实际哈希作为基线，并把差异推回对端，避免反复来回。
            let after = items.bank_snapshot().await?;
            let after_questions: HashMap<String, _> = after
                .questions
                .iter()
                .map(|question| (question_row_key(question), question))
                .collect();
            let after_papers: HashMap<String, _> = after
                .papers
                .iter()
                .map(|paper| (paper_row_key(paper), paper))
                .collect();
            for question in &fetched.questions {
                let key = question_row_key(question);
                if let Some(current) = after_questions.get(&key) {
                    let hash = question_hash(current);
                    db.save_row(&key, &RowState { hash: hash.clone(), conflict: false })
                        .await?;
                    let agreed = remote
                        .questions
                        .iter()
                        .find(|digest| digest.key == key)
                        .map(|digest| digest.hash.as_str());
                    if agreed != Some(hash.as_str()) {
                        push_questions.push((**current).clone());
                    }
                }
            }
            for paper in &fetched.papers {
                let key = paper_row_key(paper);
                if let Some(current) = after_papers.get(&key) {
                    let hash = paper_hash(current);
                    db.save_row(&key, &RowState { hash: hash.clone(), conflict: false })
                        .await?;
                    let agreed = remote
                        .papers
                        .iter()
                        .find(|digest| digest.key == key)
                        .map(|digest| digest.hash.as_str());
                    if agreed != Some(hash.as_str()) {
                        push_papers.push((*current).clone());
                    }
                }
            }
        }

        if !push_questions.is_empty() || !push_papers.is_empty() {
            let mut conflicts = Vec::new();
            for chunk in push_questions.chunks(ROW_BATCH) {
                let response = apply_rows(client, peer, chunk, &[]).await?;
                conflicts.extend(response.conflicts);
            }
            for chunk in push_papers.chunks(ROW_BATCH) {
                let response = apply_rows(client, peer, &[], chunk).await?;
                conflicts.extend(response.conflicts);
            }
            for key in conflicts {
                let hash = db
                    .load_rows()
                    .await?
                    .get(&key)
                    .map(|row| row.hash.clone())
                    .unwrap_or_default();
                db.save_row(&key, &RowState { hash, conflict: true }).await?;
                summary.conflicts += 1;
            }
        }
        Ok(())
    }

    // ---- 对等接口：由 `mesh::api` 调用 ----

    pub async fn peer_state(&self, app: &AppHandle) -> Result<SyncStateResponse, String> {
        self.refresh_local_items(app).await?;
        let db = self.db().await?;
        let states = db.load_items().await?;
        let items = ITEM_DEFS
            .iter()
            .filter(|def| !def.collection)
            .filter_map(|def| {
                states.get(def.id).map(|state| ItemMeta {
                    id: def.id.to_owned(),
                    hash: state.hash.clone(),
                    vector: state.vector.clone(),
                    conflict: state.conflict,
                })
            })
            .collect();
        let snapshot = self.items(app).bank_snapshot().await?;
        let rows = db.load_rows().await?;
        let bank = BankDigest {
            questions: snapshot
                .questions
                .iter()
                .map(|question| RowDigest {
                    key: question_row_key(question),
                    hash: question_hash(question),
                })
                .filter(|digest| !rows.get(&digest.key).is_some_and(|row| row.conflict))
                .collect(),
            papers: snapshot
                .papers
                .iter()
                .map(|paper| RowDigest {
                    key: paper_row_key(paper),
                    hash: paper_hash(paper),
                })
                .filter(|digest| !rows.get(&digest.key).is_some_and(|row| row.conflict))
                .collect(),
        };
        Ok(SyncStateResponse { items, bank })
    }

    pub async fn peer_item(&self, app: &AppHandle, id: &str) -> Result<ItemPayload, String> {
        if item_def(id).is_none() || id == UOOC_QA_BANK {
            return Err("未知的同步项".into());
        }
        let (value, state) = self.local_item(app, id).await?;
        Ok(ItemPayload {
            hash: state.hash,
            vector: state.vector,
            value,
            conflict: state.conflict,
        })
    }

    pub async fn peer_apply_item(
        &self,
        app: &AppHandle,
        id: &str,
        payload: ItemPayload,
    ) -> Result<ItemPayload, String> {
        if item_def(id).is_none() || id == UOOC_QA_BANK {
            return Err("未知的同步项".into());
        }
        let (_, mut local) = self.local_item(app, id).await?;
        if local.conflict {
            return Ok(ItemPayload {
                hash: local.hash,
                vector: local.vector,
                value: None,
                conflict: true,
            });
        }
        match decide(&local.hash, &local.vector, &payload.hash, &payload.vector) {
            MergeDecision::Same => {
                local.vector.merge(&payload.vector);
                self.db().await?.save_item(id, &local).await?;
                return Ok(ItemPayload {
                    hash: local.hash,
                    vector: local.vector,
                    value: None,
                    conflict: false,
                });
            }
            MergeDecision::Conflict => {
                local.conflict = true;
                self.db().await?.save_item(id, &local).await?;
                return Ok(ItemPayload {
                    hash: local.hash,
                    vector: local.vector,
                    value: None,
                    conflict: true,
                });
            }
            MergeDecision::Push => {
                return Ok(ItemPayload {
                    hash: local.hash,
                    vector: local.vector,
                    value: None,
                    conflict: false,
                });
            }
            MergeDecision::Adopt => {}
        }
        if payload.vector.covers(&local.vector) {
            let Some(value) = payload.value else {
                return Err("同步项缺少内容".into());
            };
            self.items(app).write(id, value).await?;
            local.vector.merge(&payload.vector);
            local.hash = payload.hash.clone();
            self.db().await?.save_item(id, &local).await?;
            return Ok(ItemPayload {
                hash: local.hash,
                vector: local.vector,
                value: None,
                conflict: false,
            });
        }
        // 本地更新：告诉调用方它应该来拉取。
        Ok(ItemPayload {
            hash: local.hash,
            vector: local.vector,
            value: None,
            conflict: false,
        })
    }

    pub async fn peer_bank_rows(
        &self,
        app: &AppHandle,
        request: BankRowsRequest,
    ) -> Result<BankRowsResponse, String> {
        let snapshot = self.items(app).bank_snapshot().await?;
        let questions: HashSet<_> = request.questions.into_iter().collect();
        let papers: HashSet<_> = request.papers.into_iter().collect();
        Ok(BankRowsResponse {
            questions: snapshot
                .questions
                .into_iter()
                .filter(|question| questions.contains(&question_row_key(question)))
                .collect(),
            papers: snapshot
                .papers
                .into_iter()
                .filter(|paper| papers.contains(&paper_row_key(paper)))
                .collect(),
        })
    }

    pub async fn peer_apply_bank(
        &self,
        app: &AppHandle,
        request: BankApplyRequest,
    ) -> Result<BankApplyResponse, String> {
        let items = self.items(app);
        let db = self.db().await?;
        let rows = db.load_rows().await?;
        let snapshot = items.bank_snapshot().await?;
        let current_questions: HashMap<String, _> = snapshot
            .questions
            .iter()
            .map(|question| (question_row_key(question), question))
            .collect();
        let current_papers: HashMap<String, _> = snapshot
            .papers
            .iter()
            .map(|paper| (paper_row_key(paper), paper))
            .collect();

        let mut apply_questions = Vec::new();
        let mut apply_papers = Vec::new();
        let mut conflicts = Vec::new();

        for question in request.questions {
            let key = question_row_key(&question);
            match current_questions.get(&key) {
                Some(current) => {
                    let local = question_hash(current);
                    if local == question_hash(&question) {
                        db.save_row(
                            &key,
                            &RowState {
                                hash: local,
                                conflict: rows.get(&key).is_some_and(|row| row.conflict),
                            },
                        )
                        .await?;
                    } else if rows.get(&key).is_some_and(|row| row.conflict) {
                        conflicts.push(key);
                    } else if rows.get(&key).is_none()
                        || rows.get(&key).map(|row| row.hash.as_str()) == Some(local.as_str())
                    {
                        apply_questions.push(question);
                    } else {
                        db.save_row(&key, &RowState { hash: local, conflict: true })
                            .await?;
                        conflicts.push(key);
                    }
                }
                None => apply_questions.push(question),
            }
        }
        for paper in request.papers {
            let key = paper_row_key(&paper);
            match current_papers.get(&key) {
                Some(current) => {
                    let local = paper_hash(current);
                    if local == paper_hash(&paper) {
                        db.save_row(
                            &key,
                            &RowState {
                                hash: local,
                                conflict: rows.get(&key).is_some_and(|row| row.conflict),
                            },
                        )
                        .await?;
                    } else if rows.get(&key).is_some_and(|row| row.conflict) {
                        conflicts.push(key);
                    } else if rows.get(&key).is_none()
                        || rows.get(&key).map(|row| row.hash.as_str()) == Some(local.as_str())
                    {
                        apply_papers.push(paper);
                    } else {
                        db.save_row(&key, &RowState { hash: local, conflict: true })
                            .await?;
                        conflicts.push(key);
                    }
                }
                None => apply_papers.push(paper),
            }
        }

        let applied = apply_questions.len() + apply_papers.len();
        let touched_questions: HashSet<_> = apply_questions.iter().map(question_row_key).collect();
        let touched_papers: HashSet<_> = apply_papers.iter().map(paper_row_key).collect();
        items.apply_questions(apply_questions).await?;
        items.apply_papers(apply_papers).await?;

        if applied > 0 {
            let after = items.bank_snapshot().await?;
            for question in after
                .questions
                .iter()
                .filter(|question| touched_questions.contains(&question_row_key(question)))
            {
                db.save_row(
                    &question_row_key(question),
                    &RowState {
                        hash: question_hash(question),
                        conflict: false,
                    },
                )
                .await?;
            }
            for paper in after
                .papers
                .iter()
                .filter(|paper| touched_papers.contains(&paper_row_key(paper)))
            {
                db.save_row(
                    &paper_row_key(paper),
                    &RowState {
                        hash: paper_hash(paper),
                        conflict: false,
                    },
                )
                .await?;
            }
        }
        Ok(BankApplyResponse { applied, conflicts })
    }

    pub async fn item_views(&self) -> Vec<SyncItemView> {
        let Ok(db) = self.db().await else {
            return Vec::new();
        };
        let Ok(items) = db.load_items().await else {
            return Vec::new();
        };
        let rows = db.load_rows().await.unwrap_or_default();
        ITEM_DEFS
            .iter()
            .map(|def| {
                let conflict = if def.collection {
                    rows.values().any(|row| row.conflict)
                } else {
                    items.get(def.id).is_some_and(|item| item.conflict)
                };
                let touched = if def.collection {
                    !rows.is_empty()
                } else {
                    items
                        .get(def.id)
                        .is_some_and(|item| !item.hash.is_empty() && !item.vector.0.is_empty())
                };
                let state = if conflict {
                    "conflict"
                } else if touched {
                    "synced"
                } else {
                    "waiting"
                };
                SyncItemView {
                    id: def.id.to_owned(),
                    label: def.label.to_owned(),
                    collection: def.collection,
                    state: state.to_owned(),
                }
            })
            .collect()
    }
}

/// 把远端 digest 与本地行做三路比较：拉取、推送、或冻结。
fn classify_rows<T>(
    remote: &[RowDigest],
    local: &HashMap<String, &T>,
    rows: &BTreeMap<String, RowState>,
    hash_of: impl Fn(&T) -> String,
    want: &mut Vec<String>,
    push: &mut Vec<T>,
    conflicts: &mut Vec<String>,
    matched: &mut Vec<(String, String)>,
) where
    T: Clone,
{
    for digest in remote {
        match local.get(&digest.key) {
            None => want.push(digest.key.clone()),
            Some(row) => {
                let hash = hash_of(row);
                if hash == digest.hash {
                    matched.push((digest.key.clone(), hash));
                    continue;
                }
                if rows.get(&digest.key).is_some_and(|state| state.conflict) {
                    continue;
                }
                match rows.get(&digest.key) {
                    Some(base) if base.hash == digest.hash => push.push((*row).clone()),
                    Some(base) if base.hash == hash => want.push(digest.key.clone()),
                    Some(_) => conflicts.push(digest.key.clone()),
                    None => want.push(digest.key.clone()),
                }
            }
        }
    }
}

async fn fetch_state(
    client: &reqwest::Client,
    peer: &PeerEndpoint,
) -> Result<SyncStateResponse, String> {
    client
        .get(format!("http://127.0.0.1:{}/v1/sync/state", peer.port))
        .header(TOKEN_HEADER, peer.token.as_ref())
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<SyncStateResponse>()
        .await
        .map_err(|error| format!("对端状态格式无效: {error}"))
}

async fn fetch_rows(
    client: &reqwest::Client,
    peer: &PeerEndpoint,
    questions: &[String],
    papers: &[String],
) -> Result<BankRowsResponse, String> {
    let mut result = BankRowsResponse::default();
    for chunk in questions.chunks(ROW_BATCH) {
        let response = client
            .post(format!("http://127.0.0.1:{}/v1/sync/bank/rows", peer.port))
            .header(TOKEN_HEADER, peer.token.as_ref())
            .json(&BankRowsRequest {
                questions: chunk.to_vec(),
                papers: Vec::new(),
            })
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<BankRowsResponse>()
            .await
            .map_err(|error| format!("对端题库格式无效: {error}"))?;
        result.questions.extend(response.questions);
    }
    for chunk in papers.chunks(ROW_BATCH) {
        let response = client
            .post(format!("http://127.0.0.1:{}/v1/sync/bank/rows", peer.port))
            .header(TOKEN_HEADER, peer.token.as_ref())
            .json(&BankRowsRequest {
                questions: Vec::new(),
                papers: chunk.to_vec(),
            })
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json::<BankRowsResponse>()
            .await
            .map_err(|error| format!("对端题库格式无效: {error}"))?;
        result.papers.extend(response.papers);
    }
    Ok(result)
}

async fn apply_rows(
    client: &reqwest::Client,
    peer: &PeerEndpoint,
    questions: &[szu_uooc::uooc::quiz::qa::LocalBankQuestion],
    papers: &[szu_uooc::uooc::quiz::qa::SubmittedPaperRecord],
) -> Result<BankApplyResponse, String> {
    client
        .post(format!("http://127.0.0.1:{}/v1/sync/bank/apply", peer.port))
        .header(TOKEN_HEADER, peer.token.as_ref())
        .json(&BankApplyRequest {
            questions: questions.to_vec(),
            papers: papers.to_vec(),
        })
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<BankApplyResponse>()
        .await
        .map_err(|error| format!("对端题库应用结果格式无效: {error}"))
}

fn peer_endpoints(app: &AppHandle) -> Vec<PeerEndpoint> {
    let Some(mesh) = app.try_state::<Arc<MeshStore>>() else {
        return Vec::new();
    };
    let Some(node) = mesh.node() else {
        return Vec::new();
    };
    let Some(token) = mesh.token() else {
        return Vec::new();
    };
    mesh.devices()
        .into_iter()
        .filter(|device| !device.is_self && device.reachable)
        .filter_map(|device| {
            let ip: Ipv4Addr = device.overlay_ip.as_deref()?.parse().ok()?;
            let port = mesh.bridge_port(&node, ip)?;
            Some(PeerEndpoint {
                label: device.name,
                port,
                token: token.clone(),
            })
        })
        .collect()
}

fn device_id() -> String {
    szu_core::preferences::load::<String>(szu_core::preferences::SYNC_DEVICE_ID)
        .ok()
        .flatten()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| "local".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(entries: &[(&str, u64)]) -> VersionVector {
        VersionVector(
            entries
                .iter()
                .map(|(device, value)| ((*device).to_owned(), *value))
                .collect(),
        )
    }

    #[test]
    fn identical_content_only_merges_vectors() {
        assert_eq!(
            decide("h1", &vector(&[("a", 1)]), "h1", &vector(&[("b", 2)])),
            MergeDecision::Same
        );
    }

    #[test]
    fn newer_remote_is_adopted() {
        assert_eq!(
            decide("h1", &vector(&[("a", 1)]), "h2", &vector(&[("a", 1), ("b", 1)])),
            MergeDecision::Adopt
        );
    }

    #[test]
    fn newer_local_is_pushed() {
        assert_eq!(
            decide("h2", &vector(&[("a", 2), ("b", 1)]), "h1", &vector(&[("a", 1)])),
            MergeDecision::Push
        );
    }

    #[test]
    fn concurrent_edits_freeze_the_item() {
        assert_eq!(
            decide("h1", &vector(&[("a", 2)]), "h2", &vector(&[("b", 1)])),
            MergeDecision::Conflict
        );
        assert_eq!(
            decide("h1", &vector(&[("a", 1)]), "h2", &vector(&[("a", 1)])),
            MergeDecision::Conflict
        );
    }

    #[test]
    fn missing_side_is_not_a_conflict() {
        assert_eq!(
            decide("", &vector(&[]), "h", &vector(&[("a", 1)])),
            MergeDecision::Adopt
        );
        assert_eq!(
            decide("h", &vector(&[("a", 1)]), "", &vector(&[("b", 1)])),
            MergeDecision::Push
        );
        assert_eq!(
            decide("", &VersionVector::default(), "", &VersionVector::default()),
            MergeDecision::Same
        );
    }

    #[test]
    fn empty_local_state_adopts_the_remote() {
        assert_eq!(
            decide("", &VersionVector::default(), "h", &vector(&[("a", 1)])),
            MergeDecision::Adopt
        );
    }
}
