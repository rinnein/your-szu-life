use super::{model::*, service, CampusNetworkStore};
use tauri::State;

#[tauri::command]
pub(crate) fn get_network_overview(
    store: State<'_, CampusNetworkStore>,
) -> Result<NetworkOverviewView, String> {
    service::get_network_overview(&store)
}

#[tauri::command]
pub(crate) fn get_dorm_network_settings(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
) -> Result<DormNetworkSettingsView, String> {
    service::get_dorm_network_settings(app, &store)
}

#[tauri::command]
pub(crate) fn save_dorm_network_settings(
    app: tauri::AppHandle,
    input: DormNetworkSettingsInput,
    store: State<'_, CampusNetworkStore>,
) -> Result<DormNetworkSettingsView, String> {
    service::save_dorm_network_settings(app, input, &store)
}

#[tauri::command]
pub(crate) fn get_dorm_login_status(
    store: State<'_, CampusNetworkStore>,
) -> Result<DormLoginStatusView, String> {
    service::get_dorm_login_status(&store)
}

#[tauri::command]
pub(crate) fn trigger_dorm_login(app: tauri::AppHandle) -> Result<(), String> {
    service::trigger_dorm_login(app)
}

#[tauri::command]
pub(crate) fn get_dorm_login_logs(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
) -> Result<Vec<DormLoginLogView>, String> {
    service::get_dorm_login_logs(app, &store)
}

#[tauri::command]
pub(crate) fn consume_dorm_login_failure_notice(
    app: tauri::AppHandle,
    store: State<'_, CampusNetworkStore>,
) -> Result<Option<DormFailureNotice>, String> {
    service::consume_dorm_login_failure_notice(app, &store)
}

#[tauri::command]
pub(crate) async fn logout_campus_network(
    app: tauri::AppHandle,
) -> Result<NetworkLogoutResultView, String> {
    tauri::async_runtime::spawn_blocking(move || service::logout_campus_network(app))
        .await
        .map_err(|_| "校园网注销任务异常结束，请检查当前网络状态".to_owned())?
}

#[tauri::command]
pub(crate) fn get_staff_network_settings(
    app: tauri::AppHandle,
) -> Result<StaffNetworkSettingsView, String> {
    super::staff_login::get_settings(app)
}
#[tauri::command]
pub(crate) fn save_staff_network_settings(
    app: tauri::AppHandle,
    input: StaffNetworkSettingsInput,
) -> Result<StaffNetworkSettingsView, String> {
    super::staff_login::save_settings(app, input)
}
#[tauri::command]
pub(crate) fn get_staff_login_status(
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
pub(crate) async fn apply_staff_network_mode(
    app: tauri::AppHandle,
    confirmation_token: Option<String>,
) -> Result<StaffApplyResultView, String> {
    tauri::async_runtime::spawn_blocking(move || super::staff_login::apply(app, confirmation_token))
        .await
        .map_err(|_| "教工区模式应用任务异常结束，请检查当前网络状态".to_owned())?
}
#[tauri::command]
pub(crate) fn consume_staff_login_failure_notice(
    app: tauri::AppHandle,
) -> Result<Option<DormFailureNotice>, String> {
    super::staff_login::consume_failure(app)
}
