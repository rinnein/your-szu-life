use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use super::devices::MeshDeviceView;
use super::{status, MeshStatusView, MeshStore};

#[tauri::command]
pub(crate) fn get_mesh_status(store: State<'_, Arc<MeshStore>>) -> MeshStatusView {
    status(&store)
}

#[tauri::command]
pub(crate) fn list_mesh_devices(store: State<'_, Arc<MeshStore>>) -> Vec<MeshDeviceView> {
    store.devices()
}

#[tauri::command]
pub(crate) async fn set_mesh_enabled(
    app: AppHandle,
    store: State<'_, Arc<MeshStore>>,
    enabled: bool,
) -> Result<MeshStatusView, String> {
    let store = store.inner().clone();
    let campus_access = super::campus_access_enabled();
    super::set_switches(&app, &store, enabled, campus_access).await?;
    Ok(status(&store))
}

#[tauri::command]
pub(crate) async fn set_mesh_campus_access(
    app: AppHandle,
    store: State<'_, Arc<MeshStore>>,
    enabled: bool,
) -> Result<MeshStatusView, String> {
    let store = store.inner().clone();
    let sync_enabled = super::enabled();
    super::set_switches(&app, &store, sync_enabled, enabled).await?;
    Ok(status(&store))
}

#[tauri::command]
pub(crate) fn set_mesh_device_name(
    app: AppHandle,
    store: State<'_, Arc<MeshStore>>,
    name: String,
) -> Result<MeshStatusView, String> {
    let name = super::normalize_device_name(&name)?;
    crate::preferences::save(crate::preferences::SYNC_DEVICE_NAME, &name)
        .map_err(|error| format!("保存设备名称失败: {error}"))?;
    let view = status(&store);
    let _ = app.emit(super::STATUS_EVENT, view.clone());
    Ok(view)
}
