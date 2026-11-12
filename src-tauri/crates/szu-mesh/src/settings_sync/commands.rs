use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use super::engine::{SyncItemView, SyncStore};
use super::SyncStatusView;

#[tauri::command]
pub async fn get_sync_status(
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
) -> Result<SyncStatusView, String> {
    Ok(super::status(&app, &store).await)
}

#[tauri::command]
pub async fn get_sync_items(
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
) -> Result<Vec<SyncItemView>, String> {
    Ok(store.item_views(&app).await)
}

#[tauri::command]
pub async fn trigger_sync(
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
) -> Result<SyncStatusView, String> {
    let store = store.inner().clone();
    super::trigger(&app, &store).await
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UoocLogoutMode {
    /// 所有同账号设备一起退出，并写入同步文档的 logout 标记。
    Global,
    /// 只退出本机，并停用本机的 UOOC 登录态同步。
    Local,
}

/// 设备同步开启时的 UOOC 退出入口。
///
/// 普通退出仍由 `logout_uooc` 负责；这里在正常退出之后处理同步契约：
/// 全局退出写 `logout` 标记并立即广播，本机退出只把该项加入本机停用列表。
#[tauri::command]
pub async fn logout_uooc_with_mode(
    window: tauri::WebviewWindow,
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
    mode: UoocLogoutMode,
) -> Result<(), String> {
    if !crate::mesh::enabled() {
        return Err("请先开启设备同步".into());
    }
    szu_uooc::uooc::logout(&app, window).await?;
    let store = store.inner().clone();
    match mode {
        UoocLogoutMode::Global => {
            store.mark_global_logout(&app).await?;
            let app = app.clone();
            let syncing = store.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = syncing.run_pass(&app).await {
                    log::warn!("全局退出后立即同步失败: {error}");
                }
                super::emit_status(&app, &syncing).await;
            });
        }
        UoocLogoutMode::Local => {
            store.set_item_enabled(super::items::UOOC_SESSION, false)?;
        }
    }
    super::emit_status(&app, &store).await;
    Ok(())
}

/// 恢复/停用某个同步项。目前用于“仅本机退出”后重新开启 UOOC 登录态同步。
#[tauri::command]
pub async fn set_sync_item_enabled(
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
    id: String,
    enabled: bool,
) -> Result<SyncStatusView, String> {
    let store = store.inner().clone();
    store.set_item_enabled(&id, enabled)?;
    let view = super::status(&app, &store).await;
    let _ = app.emit(super::STATUS_EVENT, view.clone());
    Ok(view)
}
