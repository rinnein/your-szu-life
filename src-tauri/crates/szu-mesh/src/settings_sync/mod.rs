//! 设备设置同步。
//!
//! 只同步 [`items::ITEM_DEFS`] 白名单里的内容；账号资料、统一认证会话与本地
//! 偏好（外观、通知、代理等）永远不进入同步。跨设备合并由 Loro CRDT 负责：
//! 同一 key 的并发写入按 LWW 收敛，不再冻结整项。

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub mod commands;
pub mod doc;
pub mod engine;
pub mod items;

use engine::{SyncItemView, SyncStore, SyncSummary};

pub const STATUS_EVENT: &str = "sync-status-changed";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusView {
    pub enabled: bool,
    pub campus_access: bool,
    pub running: bool,
    pub summary: SyncSummary,
    pub items: Vec<SyncItemView>,
}

pub fn initialize(app: &AppHandle, credentials: szu_core::storage::SharedCredentialStore) {
    let Some(store) = app.try_state::<Arc<SyncStore>>().map(|state| state.inner().clone()) else {
        return;
    };
    if let Err(error) = store.initialize(app, credentials) {
        log::error!("设备同步模块初始化失败: {error}");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // 等节点先把虚拟 IP 与设备列表准备好，再开始交换。
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            if crate::mesh::enabled() {
                if let Err(error) = store.run_pass(&app).await {
                    log::warn!("设备同步失败: {error}");
                }
                emit_status(&app, &store).await;
            }
            let interval = szu_core::activity::interval(
                Some("mesh"),
                Duration::from_secs(60),
                Duration::from_secs(180),
                Duration::from_secs(600),
            );
            tokio::time::sleep(interval).await;
        }
    });
}

pub async fn status(app: &AppHandle, store: &SyncStore) -> SyncStatusView {
    SyncStatusView {
        enabled: crate::mesh::enabled(),
        campus_access: crate::mesh::campus_access_enabled(),
        running: crate::mesh::is_running(app),
        summary: store.last_summary(),
        items: store.item_views(app).await,
    }
}

pub async fn emit_status(app: &AppHandle, store: &SyncStore) {
    let _ = app.emit(STATUS_EVENT, status(app, store).await);
}

/// 手动触发一次同步（向导与设置页的「立即同步」）。
pub async fn trigger(app: &AppHandle, store: &SyncStore) -> Result<SyncStatusView, String> {
    if !crate::mesh::enabled() {
        return Err("请先开启设备同步".into());
    }
    let summary = store.run_pass(app).await?;
    let view = SyncStatusView {
        enabled: true,
        campus_access: crate::mesh::campus_access_enabled(),
        running: crate::mesh::is_running(app),
        summary,
        items: store.item_views(app).await,
    };
    let _ = app.emit(STATUS_EVENT, view.clone());
    Ok(view)
}
