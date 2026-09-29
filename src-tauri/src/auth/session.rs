use super::{
    AUTH_HOST, AUTH_LOGIN_URL, AUTH_MOBILE_PROFILE_URL, AUTH_PROFILE_URL, AUTH_USER_CONF_URL,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(target_os = "android")]
use std::time::Duration;
use tauri::{webview::cookie::Cookie, Manager};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

/// Kept exclusively in the system credential store. It is intentionally not
/// serializable through a Tauri command or event payload.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct AuthSessionSecret {
    pub(crate) student_id: String,
    pub(crate) cookies: Vec<StoredAuthCookie>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StoredAuthCookie {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) expires: Option<String>,
    pub(crate) http_only: Option<bool>,
    pub(crate) secure: Option<bool>,
    pub(crate) same_site: Option<String>,
    pub(crate) partitioned: Option<bool>,
}

impl StoredAuthCookie {
    pub(super) fn from_webview_cookie(cookie: Cookie<'static>) -> Option<Self> {
        let domain = cookie
            .domain()
            .unwrap_or(AUTH_HOST)
            .trim()
            .to_ascii_lowercase();
        if !is_auth_cookie_domain(&domain) || cookie.name().is_empty() || cookie.value().is_empty()
        {
            return None;
        }
        Some(Self {
            name: cookie.name().to_owned(),
            value: cookie.value().to_owned(),
            domain,
            path: cookie.path().unwrap_or("/").to_owned(),
            expires: cookie
                .expires_datetime()
                .and_then(|expires| expires.format(&Rfc2822).ok()),
            http_only: cookie.http_only(),
            secure: cookie.secure(),
            same_site: cookie.same_site().map(|site| site.to_string()),
            partitioned: cookie.partitioned(),
        })
    }

    pub(super) fn to_webview_cookie(&self) -> Result<Cookie<'static>, String> {
        if !is_auth_cookie_domain(&self.domain)
            || self.name.is_empty()
            || self.name.contains([';', '\r', '\n'])
            || self.value.contains([';', '\r', '\n'])
            || !is_valid_cookie_path(&self.path)
        {
            return Err("认证会话格式无效".into());
        }
        let mut raw = format!(
            "{}={}; Domain={}; Path={}",
            self.name, self.value, self.domain, self.path
        );
        if self.secure == Some(true) {
            raw.push_str("; Secure");
        }
        if self.http_only == Some(true) {
            raw.push_str("; HttpOnly");
        }
        if let Some(same_site) = &self.same_site {
            raw.push_str("; SameSite=");
            raw.push_str(same_site);
        }
        if self.partitioned == Some(true) {
            raw.push_str("; Partitioned");
        }
        if let Some(expires) = &self.expires {
            raw.push_str("; Expires=");
            raw.push_str(expires);
        }
        Cookie::parse(raw)
            .map(Cookie::into_owned)
            .map_err(|_| "认证会话格式无效".into())
    }

    pub(super) fn is_definitely_expired(&self, now: OffsetDateTime) -> bool {
        self.expires
            .as_deref()
            .and_then(|value| OffsetDateTime::parse(value, &Rfc2822).ok())
            .is_some_and(|expires| expires <= now)
    }
}

pub(super) fn is_auth_cookie_domain(domain: &str) -> bool {
    // `cookies_for_url` already returns only cookies applicable to the official
    // host. Keep the persisted allow-list equally narrow so a corrupted secret
    // cannot make us inject unrelated SZU subdomain cookies on restore.
    matches!(
        domain.trim_start_matches('.').trim_end_matches('.'),
        AUTH_HOST | "szu.edu.cn"
    )
}

pub(super) fn is_valid_cookie_path(path: &str) -> bool {
    path.starts_with('/') && !path.contains([';', '\r', '\n'])
}

#[cfg(target_os = "android")]
pub(super) fn auth_cookie_url(cookie: &StoredAuthCookie) -> Result<tauri::Url, String> {
    if !is_auth_cookie_domain(&cookie.domain) || !is_valid_cookie_path(&cookie.path) {
        return Err("认证会话格式无效".into());
    }
    format!("https://{AUTH_HOST}{}", cookie.path)
        .parse::<tauri::Url>()
        .map_err(|_| "认证会话格式无效".into())
}

pub(super) fn capture_auth_session(
    window: &tauri::WebviewWindow,
    student_id: String,
) -> Result<AuthSessionSecret, String> {
    let mut cookies = Vec::new();
    for url in [
        AUTH_LOGIN_URL,
        AUTH_PROFILE_URL,
        AUTH_MOBILE_PROFILE_URL,
        AUTH_USER_CONF_URL,
    ] {
        cookies.extend(
            window
                .cookies_for_url(url.parse().expect("fixed official URL"))
                .map_err(|error| error.to_string())?,
        );
    }
    let stored = collect_auth_cookies(cookies);
    if stored.is_empty() {
        return Err("未读取到官方认证会话".into());
    }
    Ok(AuthSessionSecret {
        student_id,
        cookies: stored,
    })
}

pub(super) fn collect_auth_cookies(
    cookies: impl IntoIterator<Item = Cookie<'static>>,
) -> Vec<StoredAuthCookie> {
    let mut seen = HashSet::new();
    let mut stored = Vec::new();
    for cookie in cookies {
        let Some(cookie) = StoredAuthCookie::from_webview_cookie(cookie) else {
            continue;
        };
        let key = format!("{}\u{0}{}\u{0}{}", cookie.name, cookie.domain, cookie.path);
        if seen.insert(key) {
            stored.push(cookie);
        }
    }
    stored
}

#[cfg(not(target_os = "android"))]
pub(super) fn install_auth_cookie(
    window: &tauri::WebviewWindow,
    cookie: &StoredAuthCookie,
) -> Result<(), String> {
    window
        .set_cookie(cookie.to_webview_cookie()?)
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "android")]
pub(super) fn install_auth_cookie(
    window: &tauri::WebviewWindow,
    cookie: &StoredAuthCookie,
) -> Result<(), String> {
    use jni::objects::JValue;
    use std::sync::mpsc;

    let cookie_header = cookie.to_webview_cookie()?.to_string();
    let target_url = auth_cookie_url(cookie)?.to_string();
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |environment, _, _| {
                let result = (|| -> Result<(), String> {
                    let manager = environment
                        .call_static_method(
                            "android/webkit/CookieManager",
                            "getInstance",
                            "()Landroid/webkit/CookieManager;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let url = environment
                        .new_string(target_url)
                        .map_err(|error| error.to_string())?;
                    let header = environment
                        .new_string(cookie_header)
                        .map_err(|error| error.to_string())?;
                    environment
                        .call_method(
                            &manager,
                            "setCookie",
                            "(Ljava/lang/String;Ljava/lang/String;)V",
                            &[JValue::Object(&url), JValue::Object(&header)],
                        )
                        .map_err(|error| error.to_string())?;
                    environment
                        .call_method(&manager, "flush", "()V", &[])
                        .map_err(|error| error.to_string())?;
                    Ok(())
                })();
                let _ = sender.send(result);
            });
        })
        .map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "写入 Android 认证会话超时".to_string())?
}

/// Install persisted cookies without navigating or clearing the local app
/// document. Startup refresh uses this path so the workbench remains visible.
pub(super) fn install_auth_session_cookies(
    app: &tauri::AppHandle,
    session: &AuthSessionSecret,
) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
    for cookie in &session.cookies {
        install_auth_cookie(&window, cookie)?;
    }
    Ok(())
}

pub(super) fn clear_webview_session(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .clear_all_browsing_data()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
