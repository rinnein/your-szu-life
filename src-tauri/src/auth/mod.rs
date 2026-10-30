#[cfg(target_os = "android")]
pub(crate) mod android_cookies;
pub(crate) mod commands;
#[cfg(target_os = "linux")]
mod linux_cookies;
pub(crate) mod mcp;
pub(crate) mod model;
mod probe;
mod recovery;
mod refresh;
mod scripts;
pub(crate) mod service;
pub(crate) mod session;
mod state;
pub(crate) mod webview;

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
pub(crate) fn current_network_credentials(
    app: &tauri::AppHandle,
) -> Result<(String, String), String> {
    use tauri::Manager;
    app.try_state::<AuthStore>()
        .ok_or("账户状态不可用")?
        .network_credentials()
}

/// No auth mutex or password crosses this boundary.
pub(crate) fn authenticated_campus_card_number(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    app.try_state::<AuthStore>()
        .ok_or("认证状态不可用")?
        .authenticated_campus_card_number()
}

/// 只读导出可复用的 authserver 会话 cookie（Set-Cookie 字符串），供依赖 CAS SSO
/// 的模块（如畅课）建立下级服务会话。不含任何密码；调用方必须把它们加到
/// authserver 域的请求 URL 上使用。
pub(crate) fn reusable_sso_cookies(app: &tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri::Manager;
    let store = app
        .try_state::<AuthStore>()
        .ok_or("认证状态不可用")?
        .credentials
        .load_session()?
        .ok_or("请先完成应用统一认证")?;
    Ok(session_cookies_as_strings(&store))
}

fn session_cookies_as_strings(
    session: &session::AuthSessionSecret,
) -> Vec<String> {
    session
        .cookies
        .iter()
        .filter_map(|cookie| cookie.to_webview_cookie().ok().map(|c| c.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;
