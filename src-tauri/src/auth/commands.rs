use super::{model::*, service, AuthStore};

#[tauri::command]
pub(crate) async fn capture_auth_credentials(
    window: tauri::WebviewWindow,
    capture: CapturedCredentials,
    generation: String,
    document_id: String,
) -> Result<(), String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let app = window.app_handle().clone();
        let store = app.state::<AuthStore>();
        service::capture_auth_credentials(window, capture, &generation, &document_id, &store)
    })
    .await
    .map_err(|_| "凭据捕获任务失败".to_string())?
}

#[tauri::command]
pub(crate) async fn control_auth_recovery(
    window: tauri::WebviewWindow,
    generation: String,
    document_id: String,
    operation: super::recovery::RecoveryOperation,
) -> Result<super::recovery::RecoveryStatus, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let app = window.app_handle().clone();
        let store = app.state::<AuthStore>();
        service::control_auth_recovery(window, &generation, &document_id, operation, &store)
    })
    .await
    .map_err(|_| "认证恢复任务失败".to_string())?
}

#[tauri::command]
pub(crate) async fn get_auth_state(app: tauri::AppHandle) -> Result<AuthState, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<AuthStore>();
        service::get_auth_state(app.clone(), &store)
    })
    .await
    .map_err(|_| "认证状态读取失败".to_string())?
}

#[tauri::command]
pub(crate) async fn refresh_auth_session(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || service::refresh_auth_session(app))
        .await
        .map_err(|_| "会话刷新任务失败".to_string())?
}

#[tauri::command]
pub(crate) async fn get_account_profile(
    app: tauri::AppHandle,
) -> Result<Option<AccountProfileView>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        service::get_account_profile(&app.state::<AuthStore>())
    })
    .await
    .map_err(|_| "账户资料读取失败".to_string())?
}

#[tauri::command]
pub(crate) async fn get_user_data(app: tauri::AppHandle) -> Result<Option<UserDataView>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || service::get_user_data(&app.state::<AuthStore>()))
        .await
        .map_err(|_| "账户资料读取失败".to_string())?
}

#[tauri::command]
pub(crate) async fn confirm_profile(
    app: tauri::AppHandle,
    input: AccountProfileInput,
) -> Result<(), String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        service::confirm_profile(app.clone(), input, &app.state::<AuthStore>())
    })
    .await
    .map_err(|_| "账户资料保存失败".to_string())?
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
