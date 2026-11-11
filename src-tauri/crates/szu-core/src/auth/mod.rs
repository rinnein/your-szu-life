#[cfg(target_os = "android")]
pub mod android_window;
pub mod commands;
#[cfg(target_os = "linux")]
mod linux_cookies;
pub mod model;
mod probe;
mod recovery;
mod refresh;
mod scripts;
pub mod service;
pub mod session;
#[cfg(any(target_os = "android", test))]
mod session_policy;
mod state;
pub mod webview;

pub use state::AuthStore;

pub(super) const AUTH_LOGIN_URL: &str = "https://authserver.szu.edu.cn/authserver/login";
pub(super) const AUTH_PROFILE_URL: &str =
    "https://authserver.szu.edu.cn/personalInfo/personCenter/index.html#/accountsecurity";
pub(super) const AUTH_MOBILE_PROFILE_URL: &str =
    "https://authserver.szu.edu.cn/personalInfo/personalMobile/index.html#/home";
pub(super) const AUTH_USER_CONF_URL: &str =
    "https://authserver.szu.edu.cn/personalInfo/common/getUserConf";
pub(super) const AUTH_HOST: &str = "authserver.szu.edu.cn";

/// Credentials are read only inside Rust and never exposed by an IPC command.
pub fn current_network_credentials(
    app: &tauri::AppHandle,
) -> Result<(String, String), String> {
    use tauri::Manager;
    app.try_state::<AuthStore>()
        .ok_or("账户状态不可用")?
        .network_credentials()
}

/// No auth mutex or password crosses this boundary.
pub fn authenticated_campus_card_number(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    app.try_state::<AuthStore>()
        .ok_or("认证状态不可用")?
        .authenticated_campus_card_number()
}

/// 内部 CAS 会话投影，保留完整原生快照及 HTTP 可见身份；不经 IPC/MCP 导出。
pub fn reusable_sso_jar(
    app: &tauri::AppHandle,
) -> Result<std::sync::Arc<reqwest_cookie_store::CookieStoreMutex>, String> {
    use tauri::Manager;
    let session = app
        .try_state::<AuthStore>()
        .ok_or("认证状态不可用")?
        .credentials
        .load_session()?
        .ok_or("请先完成应用统一认证")?;
    crate::webview_cookies::project(
        crate::webview_cookies::Snapshot {
            cookies: session.cookies,
            http_keys: session.http_keys,
        },
        crate::webview_cookies::Scope::auth(),
    )
}

#[cfg(test)]
mod tests;
