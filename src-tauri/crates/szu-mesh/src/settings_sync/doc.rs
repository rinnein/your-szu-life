//! Loro 同步文档：跨设备合并、检查点加解密与“本地值 ↔ 文档值”的应用。
//!
//! 文档只保存 JSON 文本；真正的凭据仍由各模块的存储持有。检查点落盘前用设备
//! 主密钥加密，因此 Loro snapshot 中的明文不会直接出现在磁盘上。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use loro::{ExportMode, LoroDoc, LoroMap, ToJson, VersionVector};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use super::items::{
    item_def, paper_row_key, question_row_key, ItemContext, ITEM_DEFS, OJ_ACCOUNTS, UOOC_QA_BANK,
    UOOC_SESSION,
};

const ROOT: &str = "settings";
const DOC_MAGIC: &[u8; 8] = b"SZUSYNC1";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocValues {
    pub single: BTreeMap<String, Value>,
    pub oj: BTreeMap<String, Value>,
    pub questions: BTreeMap<String, Value>,
    pub papers: BTreeMap<String, Value>,
    /// UOOC 会话 marker（`{"kind":"session"|"logout", ...}`）。
    pub session: Option<Value>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyStats {
    pub applied: usize,
    pub bank_rows: usize,
}

pub struct SyncDoc {
    doc: LoroDoc,
    baseline: Mutex<DocValues>,
    saved_vv: Mutex<Vec<u8>>,
    path: PathBuf,
}

impl SyncDoc {
    pub async fn load(app: &AppHandle, items: &ItemContext) -> Result<Self, String> {
        let path = checkpoint_path(app)?;
        if szu_core::storage::vault::vault().is_none() {
            return Err("凭据存储不可用，无法加载同步检查点".into());
        }
        let doc = LoroDoc::new();
        doc.set_peer_id(peer_id()?).map_err(|error| error.to_string())?;

        let mut seeded = false;
        match fs::read(&path) {
            Ok(bytes) => match open_checkpoint(&bytes) {
                Ok((snapshot, pending)) => {
                    doc.import(&snapshot).map_err(|error| error.to_string())?;
                    if let Some(pending) = pending {
                        doc.import(&pending).map_err(|error| error.to_string())?;
                    }
                }
                Err(error) => {
                    log::error!("同步检查点损坏，已隔离并重新播种: {error}");
                    quarantine(&path);
                    seeded = true;
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => seeded = true,
            Err(error) => return Err(format!("无法读取同步检查点: {error}")),
        }

        let saved = doc.oplog_vv().encode();
        let state = Self {
            doc,
            baseline: Mutex::new(DocValues::default()),
            saved_vv: Mutex::new(saved),
            path,
        };
        if seeded {
            state.seed(app, items).await?;
            if let Err(error) = state.save() {
                log::warn!("同步检查点保存失败: {error}");
            }
        }
        *state.baseline.lock().map_err(|_| "同步基线锁失败")? = state.values()?;
        Ok(state)
    }

    fn settings(&self) -> LoroMap {
        self.doc.get_map(ROOT)
    }

    fn oj_map(&self) -> Result<LoroMap, String> {
        self.settings()
            .ensure_mergeable_map(OJ_ACCOUNTS)
            .map_err(|error| error.to_string())
    }

    fn bank_map(&self) -> Result<LoroMap, String> {
        self.settings()
            .ensure_mergeable_map(UOOC_QA_BANK)
            .map_err(|error| error.to_string())
    }

    fn put_single(&self, key: &str, value: &Value) -> Result<(), String> {
        let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
        self.settings()
            .insert(key, text)
            .map_err(|error| error.to_string())
    }

    pub fn values(&self) -> Result<DocValues, String> {
        let root = self.doc.get_deep_value().to_json_value();
        let mut values = DocValues::default();
        let Some(settings) = root.get(ROOT).and_then(Value::as_object) else {
            return Ok(values);
        };
        for (key, value) in settings {
            match key.as_str() {
                OJ_ACCOUNTS => {
                    let Some(map) = value.as_object() else { continue };
                    for (site, raw) in map {
                        if let Some(text) = raw.as_str() {
                            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                                values.oj.insert(site.clone(), parsed);
                            }
                        }
                    }
                }
                UOOC_QA_BANK => {
                    let Some(map) = value.as_object() else { continue };
                    for (row, raw) in map {
                        let Some(text) = raw.as_str() else { continue };
                        let Ok(parsed) = serde_json::from_str::<Value>(text) else {
                            continue;
                        };
                        if row.starts_with("q:") {
                            values.questions.insert(row.clone(), parsed);
                        } else if row.starts_with("p:") {
                            values.papers.insert(row.clone(), parsed);
                        }
                    }
                }
                UOOC_SESSION => {
                    if let Some(text) = value.as_str() {
                        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                            values.session = Some(parsed);
                        }
                    }
                }
                _ => {
                    if let Some(text) = value.as_str() {
                        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                            values.single.insert(key.clone(), parsed);
                        }
                    }
                }
            }
        }
        Ok(values)
    }

    /// 首次运行：用本机业务值播种文档，之后两端以文档为准。
    async fn seed(&self, _app: &AppHandle, items: &ItemContext) -> Result<(), String> {
        let disabled = disabled_items();
        for def in ITEM_DEFS.iter().filter(|def| !def.collection) {
            if matches!(def.id, OJ_ACCOUNTS | UOOC_SESSION)
                || disabled.iter().any(|id| id == def.id)
            {
                continue;
            }
            if let Some(value) = items.read(def.id).await? {
                self.put_single(def.id, &value)?;
            }
        }
        if !disabled.iter().any(|id| id == OJ_ACCOUNTS) {
            if let Some(Value::Object(accounts)) = items.read(OJ_ACCOUNTS).await? {
                let oj = self.oj_map()?;
                for (site, value) in accounts {
                    let text =
                        serde_json::to_string(&value).map_err(|error| error.to_string())?;
                    oj.insert(&site, text).map_err(|error| error.to_string())?;
                }
            }
        }
        if !disabled.iter().any(|id| id == UOOC_QA_BANK) {
            let bank = self.bank_map()?;
            let snapshot = items.bank_snapshot().await?;
            for question in snapshot.questions {
                let key = question_row_key(&question);
                let value = serde_json::to_value(&question).map_err(|error| error.to_string())?;
                let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
                bank.insert(&key, text).map_err(|error| error.to_string())?;
            }
            for paper in snapshot.papers {
                let key = paper_row_key(&paper);
                let value = serde_json::to_value(&paper).map_err(|error| error.to_string())?;
                let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
                bank.insert(&key, text).map_err(|error| error.to_string())?;
            }
        }
        if !disabled.iter().any(|id| id == UOOC_SESSION) {
            if let Some(Value::String(raw)) = items.read(UOOC_SESSION).await? {
                self.write_session(&json!({ "kind": "session", "data": raw }))?;
            }
        }
        Ok(())
    }

    pub fn write_session(&self, marker: &Value) -> Result<(), String> {
        let text = serde_json::to_string(marker).map_err(|error| error.to_string())?;
        self.settings()
            .insert(UOOC_SESSION, text)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// 把本机业务值并入文档；返回提交到文档的项/行数量。
    ///
    /// `only` 为 `Some` 时只比较指定同步项，供“修改后立即同步”使用，避免
    /// 修改 OJ 账号时也去扫描整个题库。
    pub async fn collect_local(
        &self,
        app: &AppHandle,
        items: &ItemContext,
        only: Option<&BTreeSet<String>>,
    ) -> Result<usize, String> {
        let disabled = disabled_items();
        let mut pushed = 0usize;
        let mut baseline = self.baseline.lock().map_err(|_| "同步基线锁失败")?.clone();

        for def in ITEM_DEFS.iter().filter(|def| !def.collection) {
            if !wanted(only, def.id)
                || disabled.iter().any(|id| id == def.id)
                || matches!(def.id, OJ_ACCOUNTS | UOOC_SESSION)
            {
                continue;
            }
            let local = items.read(def.id).await?;
            if local.as_ref() == baseline.single.get(def.id) {
                continue;
            }
            if let Some(value) = local {
                self.put_single(def.id, &value)?;
                baseline.single.insert(def.id.to_owned(), value);
                pushed += 1;
            }
            // 单值项取值为空时不删除共享值，避免“本机未配置”覆盖其它设备。
        }

        if wanted(only, OJ_ACCOUNTS) && !disabled.iter().any(|id| id == OJ_ACCOUNTS) {
            let local = items.read(OJ_ACCOUNTS).await?;
            let local = match local {
                Some(Value::Object(map)) => map,
                _ => serde_json::Map::new(),
            };
            let oj = self.oj_map()?;
            for (site, value) in &local {
                if baseline.oj.get(site) == Some(value) {
                    continue;
                }
                let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
                oj.insert(site, text).map_err(|error| error.to_string())?;
                baseline.oj.insert(site.clone(), value.clone());
                pushed += 1;
            }
            let removed: Vec<String> = baseline
                .oj
                .keys()
                .filter(|site| !local.contains_key(*site))
                .cloned()
                .collect();
            for site in removed {
                oj.delete(&site).map_err(|error| error.to_string())?;
                baseline.oj.remove(&site);
                pushed += 1;
            }
        }

        if wanted(only, UOOC_QA_BANK) && !disabled.iter().any(|id| id == UOOC_QA_BANK) {
            let bank = self.bank_map()?;
            let snapshot = items.bank_snapshot().await?;
            for question in snapshot.questions {
                let key = question_row_key(&question);
                let value = serde_json::to_value(&question).map_err(|error| error.to_string())?;
                if baseline.questions.get(&key) == Some(&value) {
                    continue;
                }
                let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
                bank.insert(&key, text).map_err(|error| error.to_string())?;
                baseline.questions.insert(key, value);
                pushed += 1;
            }
            for paper in snapshot.papers {
                let key = paper_row_key(&paper);
                let value = serde_json::to_value(&paper).map_err(|error| error.to_string())?;
                if baseline.papers.get(&key) == Some(&value) {
                    continue;
                }
                let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
                bank.insert(&key, text).map_err(|error| error.to_string())?;
                baseline.papers.insert(key, value);
                pushed += 1;
            }
        }

        if wanted(only, UOOC_SESSION) && !disabled.iter().any(|id| id == UOOC_SESSION) {
            pushed += self.consume_pending_login(items).await?;
        }

        *self.baseline.lock().map_err(|_| "同步基线锁失败")? = baseline;
        let _ = app;
        Ok(pushed)
    }

    /// 显式登录由 UOOC 模块写入本机偏好；只在这里把会话提交到共享文档。
    async fn consume_pending_login(&self, items: &ItemContext) -> Result<usize, String> {
        let pending = szu_core::preferences::load::<String>(
            szu_core::preferences::UOOC_LOGIN_PENDING,
        )
        .ok()
        .flatten()
        .filter(|value| !value.is_empty());
        let Some(pending) = pending else {
            return Ok(0);
        };
        let current = szu_core::storage::account_fingerprint()?;
        if current.as_deref() != Some(pending.as_str()) {
            return Ok(0);
        }
        let mut pushed = 0;
        if let Some(Value::String(raw)) = items.read(UOOC_SESSION).await? {
            self.write_session(&json!({ "kind": "session", "data": raw }))?;
            pushed = 1;
        }
        let _ = szu_core::preferences::save(
            szu_core::preferences::UOOC_LOGIN_PENDING,
            &String::new(),
        );
        Ok(pushed)
    }

    /// 把文档值应用到各模块的正常保存入口；返回实际应用的数量。
    pub async fn apply_remote(
        &self,
        app: &AppHandle,
        items: &ItemContext,
    ) -> Result<ApplyStats, String> {
        let disabled = disabled_items();
        let values = self.values()?;
        let mut baseline = self.baseline.lock().map_err(|_| "同步基线锁失败")?.clone();
        let mut stats = ApplyStats::default();

        for def in ITEM_DEFS.iter().filter(|def| !def.collection) {
            if disabled.iter().any(|id| id == def.id)
                || matches!(def.id, OJ_ACCOUNTS | UOOC_SESSION)
            {
                continue;
            }
            let Some(value) = values.single.get(def.id).cloned() else {
                continue;
            };
            if baseline.single.get(def.id) == Some(&value) {
                continue;
            }
            items.write(def.id, value.clone()).await?;
            stats.applied += 1;
            // 保存入口可能规范化值（trim 等）：以本地实际值回写文档，避免反复。
            let after = items.read(def.id).await?;
            if let Some(after) = after {
                if after != value {
                    self.put_single(def.id, &after)?;
                }
                baseline.single.insert(def.id.to_owned(), after);
            } else {
                // 写入成功但读回为空：保留文档值作为基线，避免下一轮反复应用。
                baseline.single.insert(def.id.to_owned(), value);
            }
        }

        if !disabled.iter().any(|id| id == OJ_ACCOUNTS) && values.oj != baseline.oj {
            let map = Value::Object(
                values
                    .oj
                    .iter()
                    .map(|(site, value)| (site.clone(), value.clone()))
                    .collect(),
            );
            items.write(OJ_ACCOUNTS, map).await?;
            stats.applied += 1;
            baseline.oj = values.oj.clone();
            if let Some(Value::Object(after)) = items.read(OJ_ACCOUNTS).await? {
                let after: BTreeMap<String, Value> = after.into_iter().collect();
                if after != values.oj {
                    self.replace_oj(&after)?;
                    baseline.oj = after;
                }
            }
        }

        if !disabled.iter().any(|id| id == UOOC_QA_BANK) {
            let questions: Vec<_> = values
                .questions
                .iter()
                .filter(|(key, value)| baseline.questions.get(*key) != Some(*value))
                .map(|(_, value)| serde_json::from_value(value.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "题库题目格式无效".to_string())?;
            let papers: Vec<_> = values
                .papers
                .iter()
                .filter(|(key, value)| baseline.papers.get(*key) != Some(*value))
                .map(|(_, value)| serde_json::from_value(value.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "题库试卷格式无效".to_string())?;
            let touched_questions: Vec<String> = values
                .questions
                .keys()
                .filter(|key| baseline.questions.get(*key) != values.questions.get(*key))
                .cloned()
                .collect();
            let touched_papers: Vec<String> = values
                .papers
                .keys()
                .filter(|key| baseline.papers.get(*key) != values.papers.get(*key))
                .cloned()
                .collect();
            if !questions.is_empty() {
                stats.bank_rows += questions.len();
                items.apply_questions(questions).await?;
                stats.applied += 1;
            }
            if !papers.is_empty() {
                stats.bank_rows += papers.len();
                items.apply_papers(papers).await?;
                stats.applied += 1;
            }
            if !touched_questions.is_empty() || !touched_papers.is_empty() {
                let after = items.bank_snapshot().await?;
                let bank = self.bank_map()?;
                let after_questions: BTreeMap<String, Value> = after
                    .questions
                    .iter()
                    .filter(|question| touched_questions.contains(&question_row_key(question)))
                    .map(|question| {
                        (
                            question_row_key(question),
                            serde_json::to_value(question).unwrap_or(Value::Null),
                        )
                    })
                    .collect();
                let after_papers: BTreeMap<String, Value> = after
                    .papers
                    .iter()
                    .filter(|paper| touched_papers.contains(&paper_row_key(paper)))
                    .map(|paper| {
                        (
                            paper_row_key(paper),
                            serde_json::to_value(paper).unwrap_or(Value::Null),
                        )
                    })
                    .collect();
                for (key, value) in after_questions {
                    if values.questions.get(&key) != Some(&value) {
                        let text =
                            serde_json::to_string(&value).map_err(|error| error.to_string())?;
                        bank.insert(&key, text).map_err(|error| error.to_string())?;
                    }
                    baseline.questions.insert(key, value);
                }
                for (key, value) in after_papers {
                    if values.papers.get(&key) != Some(&value) {
                        let text =
                            serde_json::to_string(&value).map_err(|error| error.to_string())?;
                        bank.insert(&key, text).map_err(|error| error.to_string())?;
                    }
                    baseline.papers.insert(key, value);
                }
            }
        }

        if !disabled.iter().any(|id| id == UOOC_SESSION) {
            self.apply_session(app, items, values.session.as_ref()).await?;
        }

        *self.baseline.lock().map_err(|_| "同步基线锁失败")? = baseline;
        Ok(stats)
    }

    fn replace_oj(&self, map: &BTreeMap<String, Value>) -> Result<(), String> {
        let oj = self.oj_map()?;
        let existing: Vec<String> = oj.keys().map(|key| key.to_string()).collect();
        for site in existing {
            if !map.contains_key(&site) {
                oj.delete(&site).map_err(|error| error.to_string())?;
            }
        }
        for (site, value) in map {
            let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
            oj.insert(site, text).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    async fn apply_session(
        &self,
        app: &AppHandle,
        items: &ItemContext,
        marker: Option<&Value>,
    ) -> Result<(), String> {
        let Some(marker) = marker else {
            return Ok(());
        };
        match marker.get("kind").and_then(Value::as_str) {
            Some("logout") => {
                let local = items.read(UOOC_SESSION).await?.is_some();
                if local || szu_uooc::uooc::is_logged_in(app) {
                    szu_uooc::uooc::apply_synced_logout(app)?;
                }
            }
            Some("session") => {
                let data = marker
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or("UOOC 会话标记缺少内容")?;
                let local = items.read(UOOC_SESSION).await?;
                if matches!(&local, Some(Value::String(current)) if current == data) {
                    return Ok(());
                }
                // 两端都已登录时不互相覆盖。
                if szu_uooc::uooc::is_logged_in(app) {
                    return Ok(());
                }
                items
                    .write(UOOC_SESSION, Value::String(data.to_owned()))
                    .await?;
            }
            _ => log::warn!("未知的 UOOC 会话标记，已忽略"),
        }
        Ok(())
    }

    pub fn oplog_vv(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    pub fn export_updates(&self, from: &VersionVector) -> Result<Vec<u8>, String> {
        self.doc
            .export(ExportMode::updates(from))
            .map_err(|error| error.to_string())
    }

    pub fn export_all_updates(&self) -> Result<Vec<u8>, String> {
        self.doc
            .export(ExportMode::all_updates())
            .map_err(|error| error.to_string())
    }

    pub fn import(&self, bytes: &[u8]) -> Result<(), String> {
        self.doc.import(bytes).map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        self.doc.commit();
        let vv = self.doc.oplog_vv().encode();
        {
            let saved = self.saved_vv.lock().map_err(|_| "同步检查点锁失败")?;
            if *saved == vv {
                return Ok(());
            }
        }
        let snapshot = self
            .doc
            .export(ExportMode::Snapshot)
            .map_err(|error| error.to_string())?;
        let pending = self
            .doc
            .export_pending_updates()
            .map_err(|error| error.to_string())?;
        let plain = frame(&snapshot, pending.as_deref());
        let vault = szu_core::storage::vault::vault().ok_or("凭据存储不可用")?;
        let encrypted = vault.encrypt_bytes(&plain)?;
        write_atomic(&self.path, &encrypted)?;
        *self.saved_vv.lock().map_err(|_| "同步检查点锁失败")? = vv;
        Ok(())
    }
}

fn checkpoint_path(app: &AppHandle) -> Result<PathBuf, String> {
    let fingerprint = szu_core::storage::account_fingerprint()?
        .ok_or_else(|| "尚未登录，无法确定同步检查点".to_string())?;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定应用数据目录".to_string())?
        .join("mesh")
        .join("sync")
        .join(format!("{fingerprint}.loro.enc")))
}

fn peer_id() -> Result<u64, String> {
    let device = szu_core::preferences::load::<String>(szu_core::preferences::SYNC_DEVICE_ID)
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "local".to_owned());
    let digest = Sha256::digest(device.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    Ok(u64::from_be_bytes(bytes).max(1))
}

fn open_checkpoint(bytes: &[u8]) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let vault = szu_core::storage::vault::vault().ok_or("凭据存储不可用")?;
    let plain = vault.decrypt_bytes(bytes)?;
    unframe(&plain)
}

fn frame(snapshot: &[u8], pending: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 8 + snapshot.len() + 8 + pending.map_or(0, |p| p.len()));
    out.extend_from_slice(DOC_MAGIC);
    out.extend_from_slice(&(snapshot.len() as u64).to_be_bytes());
    out.extend_from_slice(snapshot);
    let len = pending.map_or(0, |p| p.len() as u64);
    out.extend_from_slice(&len.to_be_bytes());
    if let Some(pending) = pending {
        out.extend_from_slice(pending);
    }
    out
}

fn unframe(bytes: &[u8]) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    if bytes.len() < 16 || &bytes[..8] != DOC_MAGIC {
        return Err("同步检查点格式无效".into());
    }
    let snapshot_len = u64::from_be_bytes(
        bytes[8..16]
            .try_into()
            .map_err(|_| "同步检查点长度无效".to_string())?,
    ) as usize;
    let snapshot_end = 16usize
        .checked_add(snapshot_len)
        .filter(|end| *end + 8 <= bytes.len())
        .ok_or("同步检查点长度无效")?;
    let pending_len = u64::from_be_bytes(
        bytes[snapshot_end..snapshot_end + 8]
            .try_into()
            .map_err(|_| "同步检查点长度无效".to_string())?,
    ) as usize;
    let pending_start = snapshot_end + 8;
    let pending_end = pending_start
        .checked_add(pending_len)
        .filter(|end| *end == bytes.len())
        .ok_or("同步检查点长度无效")?;
    let snapshot = bytes[16..snapshot_end].to_vec();
    let pending = (pending_len > 0).then(|| bytes[pending_start..pending_end].to_vec());
    Ok((snapshot, pending))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("同步检查点目录无效")?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建同步检查点目录: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("无法保存同步检查点: {error}"))
}

fn quarantine(path: &Path) {
    if !path.exists() {
        return;
    }
    let stamp = chrono::Utc::now().timestamp_millis();
    let target = path.with_extension(format!("enc.corrupt-{stamp}"));
    match fs::rename(path, &target) {
        Ok(()) => log::warn!("同步检查点已隔离到 {}", target.display()),
        Err(error) => log::error!("同步检查点隔离失败: {error}"),
    }
}

/// 是否应处理某个同步项；`None` 表示完整同步。
fn wanted(only: Option<&BTreeSet<String>>, id: &str) -> bool {
    only.map_or(true, |items| items.contains(id))
}

/// 本机停用的同步项。
pub fn disabled_items() -> Vec<String> {
    szu_core::preferences::load::<Vec<String>>(szu_core::preferences::SYNC_DISABLED_ITEMS)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// 某同步项是否被本机停用。
pub fn is_disabled(id: &str) -> bool {
    disabled_items().iter().any(|item| item == id)
}

/// 停用/恢复某个同步项；只写本机偏好，不进入同步数据。
pub fn set_disabled(id: &str, disabled: bool) -> Result<(), String> {
    if item_def(id).is_none() {
        return Err("未知的同步项".into());
    }
    let mut items = disabled_items();
    let present = items.iter().any(|item| item == id);
    if disabled && !present {
        items.push(id.to_owned());
    } else if !disabled && present {
        items.retain(|item| item != id);
    }
    items.sort();
    items.dedup();
    szu_core::preferences::save(szu_core::preferences::SYNC_DISABLED_ITEMS, &items)
        .map_err(|error| format!("保存停用项失败: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_frame_round_trips() {
        let framed = frame(b"snapshot", Some(b"pending"));
        let (snapshot, pending) = unframe(&framed).unwrap();
        assert_eq!(snapshot, b"snapshot");
        assert_eq!(pending.as_deref(), Some(b"pending".as_slice()));
    }

    #[test]
    fn checkpoint_frame_without_pending() {
        let framed = frame(b"snapshot", None);
        let (snapshot, pending) = unframe(&framed).unwrap();
        assert_eq!(snapshot, b"snapshot");
        assert!(pending.is_none());
    }

    #[test]
    fn checkpoint_rejects_truncated_bytes() {
        assert!(unframe(b"short").is_err());
    }

    #[test]
    fn item_filter_none_means_full_and_some_restricts() {
        let mut only = BTreeSet::new();
        only.insert("oj.accounts".to_owned());
        assert!(wanted(None, "oj.accounts"));
        assert!(wanted(Some(&only), "oj.accounts"));
        assert!(!wanted(Some(&only), "uooc.session"));
    }
}

#[cfg(test)]
mod loro_tests {
    use loro::{ExportMode, LoroDoc, ToJson};

    fn doc(peer: u64) -> LoroDoc {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).unwrap();
        doc
    }

    #[test]
    fn independent_oj_sites_merge() {
        let a = doc(1);
        let b = doc(2);
        a.get_map("settings")
            .ensure_mergeable_map("oj.accounts")
            .unwrap()
            .insert("site-a", r#"{"username":"a"}"#)
            .unwrap();
        b.get_map("settings")
            .ensure_mergeable_map("oj.accounts")
            .unwrap()
            .insert("site-b", r#"{"username":"b"}"#)
            .unwrap();
        let sa = a.export(ExportMode::Snapshot).unwrap();
        let sb = b.export(ExportMode::Snapshot).unwrap();
        a.import(&sb).unwrap();
        b.import(&sa).unwrap();
        let ja = a.get_deep_value().to_json_value();
        let jb = b.get_deep_value().to_json_value();
        assert_eq!(ja, jb);
        assert!(ja["settings"]["oj.accounts"]["site-a"].is_string());
        assert!(ja["settings"]["oj.accounts"]["site-b"].is_string());
    }

    #[test]
    fn same_oj_site_uses_last_write_wins() {
        let a = doc(1);
        let b = doc(2);
        a.get_map("settings")
            .ensure_mergeable_map("oj.accounts")
            .unwrap()
            .insert("site", r#"{"username":"a"}"#)
            .unwrap();
        b.get_map("settings")
            .ensure_mergeable_map("oj.accounts")
            .unwrap()
            .insert("site", r#"{"username":"b"}"#)
            .unwrap();
        let sa = a.export(ExportMode::Snapshot).unwrap();
        let sb = b.export(ExportMode::Snapshot).unwrap();
        a.import(&sb).unwrap();
        b.import(&sa).unwrap();
        let ja = a.get_deep_value().to_json_value();
        let jb = b.get_deep_value().to_json_value();
        assert_eq!(ja, jb);
        let value = ja["settings"]["oj.accounts"]["site"].as_str().unwrap();
        assert!(value == r#"{"username":"a"}"# || value == r#"{"username":"b"}"#);
    }

    #[test]
    fn bank_rows_merge_per_key() {
        let a = doc(1);
        let b = doc(2);
        a.get_map("settings")
            .ensure_mergeable_map("uooc.qa_bank")
            .unwrap()
            .insert("q:1", r#"{"stem":"one"}"#)
            .unwrap();
        b.get_map("settings")
            .ensure_mergeable_map("uooc.qa_bank")
            .unwrap()
            .insert("q:2", r#"{"stem":"two"}"#)
            .unwrap();
        let sa = a.export(ExportMode::Snapshot).unwrap();
        let sb = b.export(ExportMode::Snapshot).unwrap();
        a.import(&sb).unwrap();
        b.import(&sa).unwrap();
        let ja = a.get_deep_value().to_json_value();
        assert_eq!(ja, b.get_deep_value().to_json_value());
        assert!(ja["settings"]["uooc.qa_bank"]["q:1"].is_string());
        assert!(ja["settings"]["uooc.qa_bank"]["q:2"].is_string());
    }
}
