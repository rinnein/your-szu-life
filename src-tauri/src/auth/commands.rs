use super::{model::*, service, AuthStore};
use tauri::State;

#[tauri::command]
pub(crate) fn capture_auth_credentials(
    window: tauri::WebviewWindow,
    capture: CapturedCredentials,
    store: State<'_, AuthStore>,
) -> Result<(), String> {
    service::capture_auth_credentials(window, capture, &store)
}

#[tauri::command]
pub(crate) fn get_auth_state(
    app: tauri::AppHandle,
    store: State<'_, AuthStore>,
) -> Result<AuthState, String> {
    service::get_auth_state(app, &store)
}

#[tauri::command]
pub(crate) fn refresh_auth_session(app: tauri::AppHandle) -> Result<(), String> {
    service::refresh_auth_session(app)
}

#[tauri::command]
pub(crate) fn get_account_profile(
    store: State<'_, AuthStore>,
) -> Result<Option<AccountProfileView>, String> {
    service::get_account_profile(&store)
}

#[tauri::command]
pub(crate) fn get_user_data(store: State<'_, AuthStore>) -> Result<Option<UserDataView>, String> {
    service::get_user_data(&store)
}

#[tauri::command]
pub(crate) fn confirm_profile(
    app: tauri::AppHandle,
    input: AccountProfileInput,
    store: State<'_, AuthStore>,
) -> Result<(), String> {
    service::confirm_profile(app, input, &store)
}

#[tauri::command]
pub(crate) async fn begin_authentication(
    app: tauri::AppHandle,
    user_agent: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        service::begin_authentication(app, user_agent.as_deref())
    })
    .await
    .map_err(|_| "认证任务执行失败".to_string())?
}

#[tauri::command]
pub(crate) async fn logout(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<AuthStore>();
        service::logout(app.clone(), &store)
    })
    .await
    .map_err(|_| "注销任务执行失败".to_string())?
}
