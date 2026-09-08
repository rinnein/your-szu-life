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
pub(crate) fn begin_authentication(app: tauri::AppHandle) -> Result<(), String> {
    service::begin_authentication(app)
}

#[tauri::command]
pub(crate) fn logout(app: tauri::AppHandle, store: State<'_, AuthStore>) -> Result<(), String> {
    service::logout(app, &store)
}
