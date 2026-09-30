#[cfg(target_os = "android")]
pub(crate) mod android_cookies;
pub(crate) mod commands;
pub(crate) mod model;
mod probe;
mod recovery;
mod refresh;
mod scripts;
mod service;
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

#[cfg(test)]
mod tests;
