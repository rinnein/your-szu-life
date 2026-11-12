//! 同步引擎：Loro 文档负责合并，引擎负责发现对端、交换增量与把结果应用到业务。
//!
//! 协议 v2 只交换两样东西：本机 `oplog_vv` 与自对端版本向量以来的 Loro updates。
//! 不再有“冲突冻结”：同一 key 的并发写入由 Loro 的 LWW 规则确定胜者，不同 key
//! （不同 OJ 站点、不同题库行）独立合并。

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use loro::VersionVector;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::doc::{disabled_items, SyncDoc};
use super::items::{ItemContext, ITEM_DEFS, OJ_ACCOUNTS, UOOC_SESSION};
use crate::mesh::{api::TOKEN_HEADER, MeshStore};
use szu_core::storage::SharedCredentialStore;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub const PROTOCOL: u32 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub peers: usize,
    pub applied: usize,
    pub pushed: usize,
    pub bank_rows: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncItemView {
    pub id: String,
    pub label: String,
    pub collection: bool,
    /// `synced` / `waiting` / `disabled` / `loggedOut`
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStateResponse {
    pub protocol: u32,
    /// base64(`VersionVector::encode()`)
    pub vv: String,
}

struct PeerEndpoint {
    label: String,
    port: u16,
    token: Arc<str>,
}

struct CachedDoc {
    fingerprint: String,
    doc: Arc<SyncDoc>,
}

pub struct SyncStore {
    credentials: OnceLock<SharedCredentialStore>,
    /// 按账户指纹缓存；切换账号时丢弃并重新加载检查点。
    doc: tokio::sync::Mutex<Option<CachedDoc>>,
    /// 完整同步的本地串行锁；不跨出站 HTTP 持有。
    running: tokio::sync::Mutex<()>,
    /// 文档读写/应用的短临界区；对端 HTTP handler 也使用它。
    pass: tokio::sync::Mutex<()>,
    last_summary: Mutex<SyncSummary>,
}

impl SyncStore {
    pub fn new() -> Self {
        Self {
            credentials: OnceLock::new(),
            doc: tokio::sync::Mutex::new(None),
            running: tokio::sync::Mutex::new(()),
            pass: tokio::sync::Mutex::new(()),
            last_summary: Mutex::new(SyncSummary::default()),
        }
    }

    pub fn initialize(
        &self,
        _app: &AppHandle,
        credentials: SharedCredentialStore,
    ) -> Result<(), String> {
        let _ = self.credentials.set(credentials);
        Ok(())
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

    async fn doc(&self, app: &AppHandle) -> Result<Arc<SyncDoc>, String> {
        let fingerprint = szu_core::storage::account_fingerprint()?
            .ok_or_else(|| "尚未登录，无法启用设备同步".to_string())?;
        let mut cached = self.doc.lock().await;
        if let Some(current) = cached.as_ref() {
            if current.fingerprint == fingerprint {
                return Ok(current.doc.clone());
            }
        }
        let credentials = self
            .credentials
            .get()
            .cloned()
            .unwrap_or_else(|| Arc::new(szu_core::storage::PlatformCredentialStore));
        let doc = Arc::new(
            SyncDoc::load(
                app,
                &ItemContext {
                    app: app.clone(),
                    credentials,
                },
            )
            .await?,
        );
        *cached = Some(CachedDoc {
            fingerprint,
            doc: doc.clone(),
        });
        Ok(doc)
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

    /// 一次完整同步：先把本机改动并入文档，再逐对端交换增量并应用。
    pub async fn run_pass(&self, app: &AppHandle) -> Result<SyncSummary, String> {
        // 只串行本机的完整轮次；出站 HTTP 不持有 `pass`，避免两台设备同时
        // 发起同步时互相等待对端 handler 造成的死锁。
        let _running = self.running.lock().await;
        let doc = self.doc(app).await?;
        let items = self.items(app);
        let mut summary = SyncSummary::default();
        {
            let _pass = self.pass.lock().await;
            summary.pushed = doc.collect_local(app, &items).await?;
        }

        let peers = peer_endpoints(app);
        summary.peers = peers.len();
        let client = szu_core::netproxy::overlay_client_builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("同步客户端创建失败: {error}"))?;
        for peer in peers {
            if let Err(error) = self
                .exchange(app, &doc, &items, &client, &peer, &mut summary)
                .await
            {
                summary
                    .errors
                    .push(format!("与 {} 同步失败: {error}", peer.label));
            }
        }
        {
            let _pass = self.pass.lock().await;
            if let Err(error) = doc.save() {
                summary.errors.push(format!("保存同步检查点失败: {error}"));
            }
        }
        if summary.applied > 0 || summary.bank_rows > 0 {
            szu_core::onboarding::adopt_synced_items();
        }
        self.set_summary(summary.clone());
        Ok(summary)
    }

    async fn exchange(
        &self,
        app: &AppHandle,
        doc: &SyncDoc,
        items: &ItemContext,
        client: &reqwest::Client,
        peer: &PeerEndpoint,
        summary: &mut SyncSummary,
    ) -> Result<(), String> {
        let state = fetch_state(client, peer).await?;
        if state.protocol != PROTOCOL {
            return Err(format!(
                "对端协议版本 {} 不兼容，请先升级两端应用",
                state.protocol
            ));
        }
        let remote_vv = decode_vv(&state.vv)?;
        let (updates, local_vv) = {
            let _pass = self.pass.lock().await;
            (doc.export_updates(&remote_vv)?, encode_vv(&doc.oplog_vv()))
        };
        let response = client
            .post(format!(
                "http://127.0.0.1:{}/v2/sync/exchange",
                peer.port
            ))
            .header(TOKEN_HEADER, peer.token.as_ref())
            .header("x-sync-vv", local_vv)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(updates)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let bytes = response
            .bytes()
            .await
            .map_err(|error| error.to_string())?;
        let _pass = self.pass.lock().await;
        if !bytes.is_empty() {
            doc.import(&bytes)?;
        }
        // 出站 HTTP 期间用户可能又改了本地配置：应用远端前再采集一次，
        // 让这些改动进入 Loro 后再按合并结果回写业务存储。
        let _ = doc.collect_local(app, items).await?;
        let stats = doc.apply_remote(app, items).await?;
        summary.applied += stats.applied;
        summary.bank_rows += stats.bank_rows;
        Ok(())
    }

    // ---- 对等接口：由 `mesh::api` 调用 ----

    pub async fn peer_state(&self, app: &AppHandle) -> Result<SyncStateResponse, String> {
        let _pass = self.pass.lock().await;
        let doc = self.doc(app).await?;
        let _ = doc.collect_local(app, &self.items(app)).await?;
        Ok(SyncStateResponse {
            protocol: PROTOCOL,
            vv: encode_vv(&doc.oplog_vv()),
        })
    }

    /// 对端请求：导入对方增量，返回对方缺少的增量与本机版本向量。
    pub async fn peer_exchange(
        &self,
        app: &AppHandle,
        from: VersionVector,
        updates: &[u8],
    ) -> Result<(Vec<u8>, String), String> {
        let _pass = self.pass.lock().await;
        let doc = self.doc(app).await?;
        let items = self.items(app);
        doc.collect_local(app, &items).await?;
        if !updates.is_empty() {
            doc.import(updates)?;
        }
        let out = doc.export_updates(&from)?;
        let _ = doc.apply_remote(app, &items).await?;
        doc.save()?;
        Ok((out, encode_vv(&doc.oplog_vv())))
    }

    /// 记录全局退出：文档中的会话 marker 变为 `logout`，下一次同步广播给对端。
    pub async fn mark_global_logout(&self, app: &AppHandle) -> Result<(), String> {
        let _pass = self.pass.lock().await;
        let doc = self.doc(app).await?;
        doc.write_session(&serde_json::json!({ "kind": "logout" }))?;
        doc.save()?;
        Ok(())
    }

    pub fn set_item_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        super::doc::set_disabled(id, !enabled)
    }

    pub async fn item_views(&self, app: &AppHandle) -> Vec<SyncItemView> {
        let disabled = disabled_items();
        let fallback = || {
            ITEM_DEFS
                .iter()
                .map(|def| SyncItemView {
                    id: def.id.to_owned(),
                    label: def.label.to_owned(),
                    collection: def.collection,
                    state: if disabled.iter().any(|id| id == def.id) {
                        "disabled"
                    } else {
                        "waiting"
                    }
                    .to_owned(),
                })
                .collect::<Vec<_>>()
        };
        // 关闭同步时不初始化/播种 Loro 检查点，避免在用户未启用同步时生成凭据副本。
        if !crate::mesh::enabled() {
            return fallback();
        }
        let _pass = self.pass.lock().await;
        let Ok(doc) = self.doc(app).await else {
            return fallback();
        };
        let Ok(values) = doc.values() else {
            return fallback();
        };
        ITEM_DEFS
            .iter()
            .map(|def| {
                let state = if disabled.iter().any(|id| id == def.id) {
                    "disabled"
                } else if def.id == UOOC_SESSION {
                    match values
                        .session
                        .as_ref()
                        .and_then(|marker| marker.get("kind"))
                        .and_then(serde_json::Value::as_str)
                    {
                        Some("logout") => "loggedOut",
                        Some("session") => "synced",
                        _ => "waiting",
                    }
                } else if def.collection {
                    if values.questions.is_empty() && values.papers.is_empty() {
                        "waiting"
                    } else {
                        "synced"
                    }
                } else if def.id == OJ_ACCOUNTS {
                    if values.oj.is_empty() {
                        "waiting"
                    } else {
                        "synced"
                    }
                } else if values.single.contains_key(def.id) {
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

/// base64 + Loro 版本向量解码；供 API 与测试复用。
pub fn decode_vv(encoded: &str) -> Result<VersionVector, String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| format!("版本向量编码无效: {error}"))?;
    VersionVector::decode(&bytes).map_err(|error| error.to_string())
}

fn encode_vv(vv: &VersionVector) -> String {
    STANDARD.encode(vv.encode())
}

async fn fetch_state(
    client: &reqwest::Client,
    peer: &PeerEndpoint,
) -> Result<SyncStateResponse, String> {
    client
        .get(format!("http://127.0.0.1:{}/v2/sync/state", peer.port))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_vector_base64_round_trips() {
        let mut vv = VersionVector::default();
        vv.set_last(loro::ID::new(7, 3));
        let encoded = encode_vv(&vv);
        assert_eq!(decode_vv(&encoded).unwrap(), vv);
    }

    #[test]
    fn invalid_version_vector_is_rejected() {
        assert!(decode_vv("not-base64!!").is_err());
    }
}
