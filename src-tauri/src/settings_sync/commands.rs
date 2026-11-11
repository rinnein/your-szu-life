use std::sync::Arc;

use tauri::{AppHandle, State};

use super::engine::{SyncItemView, SyncStore};
use super::SyncStatusView;

#[tauri::command]
pub(crate) async fn get_sync_status(
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
) -> Result<SyncStatusView, String> {
    Ok(super::status(&app, &store).await)
}

#[tauri::command]
pub(crate) async fn get_sync_items(
    store: State<'_, Arc<SyncStore>>,
) -> Result<Vec<SyncItemView>, String> {
    Ok(store.item_views().await)
}

#[tauri::command]
pub(crate) async fn trigger_sync(
    app: AppHandle,
    store: State<'_, Arc<SyncStore>>,
) -> Result<SyncStatusView, String> {
    let store = store.inner().clone();
    super::trigger(&app, &store).await
}
