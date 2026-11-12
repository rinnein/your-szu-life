use super::{model::*, service, CampusNetworkStore};
use tauri::State;

#[tauri::command]
pub fn request_campus_network_permissions() -> Result<(), String> {
    super::android::request_permissions()
}

#[tauri::command]
pub fn get_debug_mode(store: State<'_, CampusNetworkStore>) -> DebugModeView {
    super::diagnostics::mode_view(&store)
}

#[tauri::command]
pub fn set_debug_mode(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
    enabled: bool,
) -> Result<DebugModeView, String> {
    super::diagnostics::set_mode(&app, &store, enabled)
}

#[tauri::command]
pub fn clear_debug_logs(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
) -> Result<(), String> {
    crate::logging::clear_logs(&app)?;
    super::diagnostics::clear_logs(&app, &store)
}

#[tauri::command]
pub fn get_log_info(app: tauri::AppHandle) -> Result<crate::logging::LogInfo, String> {
    crate::logging::get_log_info(&app)
}

#[tauri::command]
pub fn open_log_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    crate::logging::open_log_file(&app, &path)
}

#[tauri::command]
pub fn set_log_level(level: String) -> Result<String, String> {
    crate::logging::set_log_level(&level)
}

#[tauri::command]
pub fn get_network_overview(
    store: State<'_, CampusNetworkStore>,
) -> Result<NetworkOverviewView, String> {
    service::get_network_overview(&store)
}

#[tauri::command]
pub fn refresh_campus_network(app: tauri::AppHandle) -> Result<NetworkOverviewView, String> {
    service::refresh_campus_network(app)
}

#[tauri::command]
pub fn get_dorm_network_settings(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
) -> Result<DormNetworkSettingsView, String> {
    service::get_dorm_network_settings(app, &store)
}

#[tauri::command]
pub fn save_dorm_network_settings(
    app: tauri::AppHandle,
    input: DormNetworkSettingsInput,
    store: State<'_, CampusNetworkStore>,
) -> Result<DormNetworkSettingsView, String> {
    let view = service::save_dorm_network_settings(app, input, &store)?;
    crate::settings_sync_signal::mark_dirty(crate::settings_sync_items::DORM_CREDENTIALS);
    Ok(view)
}

#[tauri::command]
pub fn get_dorm_login_status(
    store: State<'_, CampusNetworkStore>,
) -> Result<DormLoginStatusView, String> {
    service::get_dorm_login_status(&store)
}

#[tauri::command]
pub fn trigger_dorm_login(app: tauri::AppHandle) -> Result<(), String> {
    service::trigger_dorm_login(app)
}

#[tauri::command]
pub fn consume_dorm_login_failure_notice(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
) -> Result<Option<DormFailureNotice>, String> {
    service::consume_dorm_login_failure_notice(app, &store)
}

#[tauri::command]
pub async fn logout_campus_network(
    app: tauri::AppHandle,
) -> Result<NetworkLogoutResultView, String> {
    tauri::async_runtime::spawn_blocking(move || service::logout_campus_network(app))
        .await
        .map_err(|_| "校园网注销任务异常结束，请检查当前网络状态".to_owned())?
}

#[tauri::command]
pub fn get_staff_network_settings(
    app: tauri::AppHandle,
) -> Result<StaffNetworkSettingsView, String> {
    super::staff_login::get_settings(app)
}
#[tauri::command]
pub fn save_staff_network_settings(
    app: tauri::AppHandle,
    input: StaffNetworkSettingsInput,
) -> Result<StaffNetworkSettingsView, String> {
    let view = super::staff_login::save_settings(app, input)?;
    crate::settings_sync_signal::mark_dirty(crate::settings_sync_items::STAFF_CREDENTIALS);
    Ok(view)
}
#[tauri::command]
pub fn get_staff_login_status(
    store: State<'_, CampusNetworkStore>,
) -> Result<StaffLoginStatusView, String> {
    store
        .staff
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "教工区状态锁失败".into())
}
#[tauri::command]
pub async fn apply_staff_network_mode(
    app: tauri::AppHandle,
    confirmation_token: Option<String>,
) -> Result<StaffApplyResultView, String> {
    tauri::async_runtime::spawn_blocking(move || super::staff_login::apply(app, confirmation_token))
        .await
        .map_err(|_| "教工区模式应用任务异常结束，请检查当前网络状态".to_owned())?
}
#[tauri::command]
pub fn consume_staff_login_failure_notice(
    app: tauri::AppHandle,
) -> Result<Option<DormFailureNotice>, String> {
    super::staff_login::consume_failure(app)
}
