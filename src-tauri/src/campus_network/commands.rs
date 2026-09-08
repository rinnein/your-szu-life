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
