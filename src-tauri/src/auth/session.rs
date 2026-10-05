use super::{
    AUTH_HOST, AUTH_LOGIN_URL, AUTH_MOBILE_PROFILE_URL, AUTH_PROFILE_URL, AUTH_USER_CONF_URL,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{webview::cookie::Cookie, Manager};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

/// Kept exclusively in the system credential store. It is intentionally not
/// serializable through a Tauri command or event payload.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct AuthSessionSecret {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) format_version: u8,
    pub(crate) student_id: String,
    pub(crate) cookies: Vec<StoredAuthCookie>,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

impl AuthSessionSecret {
    pub(super) fn has_android_cookie_metadata(&self) -> bool {
        matches!(self.format_version, 2 | 3)
    }

    pub(super) fn can_restore(&self) -> bool {
        !cfg!(target_os = "android") || self.has_android_cookie_metadata()
    }

    pub(super) fn restorable_snapshot(
        &self,
        expected_id: &str,
        now: OffsetDateTime,
    ) -> Result<Option<Self>, SessionRestoreError> {
        if self.student_id != expected_id {
            return Err(SessionRestoreError::AccountMismatch);
        }
        if !self.can_restore() {
            return Err(SessionRestoreError::Unsupported);
        }
        let mut cookies = Vec::new();
        for original in &self.cookies {
            let mut cookie = original.clone();
            cookie.domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
            // Older desktop snapshots lost Soup's host/domain distinction.
            // Narrow an ambiguous auth-host cookie to that exact host. A later
            // HTTP update for the same cookie takes precedence over the old copy.
            #[cfg(target_os = "linux")]
            if self.format_version < 3 && cookie.domain == AUTH_HOST {
                cookie.host_only = true;
            }
            if !cookie_in_auth_scope(&cookie)
                || cookie.to_webview_cookie().is_err()
                || cookie
                    .expires
                    .as_deref()
                    .is_some_and(|date| OffsetDateTime::parse(date, &Rfc2822).is_err())
            {
                return Err(SessionRestoreError::InvalidCookie);
            }
            if !cookie.value.is_empty() && !cookie.is_definitely_expired(now) {
                cookies.push(cookie);
            }
        }
        let mut seen = HashSet::new();
        cookies.reverse();
        cookies.retain(|cookie| {
            seen.insert((
                cookie.name.clone(),
                cookie.domain.clone(),
                cookie.path.clone(),
                cookie.host_only,
            ))
        });
        cookies.reverse();
        Ok((!cookies.is_empty()).then(|| Self {
            format_version: if cfg!(target_os = "linux") {
                3
            } else {
                self.format_version
            },
            student_id: self.student_id.clone(),
            cookies,
        }))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SessionRestoreError {
    AccountMismatch,
    Unsupported,
    InvalidCookie,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StoredAuthCookie {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    #[serde(default)]
    pub(crate) host_only: bool,
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
        if !is_auth_cookie_domain(&domain) || cookie.name().is_empty() {
            return None;
        }
        Some(Self {
            name: cookie.name().to_owned(),
            value: cookie.value().to_owned(),
            domain,
            host_only: cookie.domain().is_none(),
            path: cookie.path().unwrap_or("/").to_owned(),
            expires: cookie
                .max_age()
                .map(|age| OffsetDateTime::now_utc().saturating_add(age))
                .or_else(|| cookie.expires_datetime())
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
        if self.host_only && self.domain != AUTH_HOST {
            return Err("认证会话主机范围无效".into());
        }
        let mut raw = format!("{}={}; Path={}", self.name, self.value, self.path);
        // A Domain attribute creates a distinct cookie in Android Chromium.
        // Never turn an original host cookie into a domain cookie on restore.
        if !self.host_only {
            raw.push_str("; Domain=");
            raw.push_str(&self.domain);
        }
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

/// AndroidX returns canonical domains: a leading dot means a domain cookie.
/// This differs from HTTP Set-Cookie semantics. Inspect the original attribute
/// before the cookie crate strips its leading dot.
#[cfg(any(target_os = "android", test))]
pub(super) fn from_android_cookie_info(raw: &str) -> Result<StoredAuthCookie, String> {
    let domain = raw
        .split(';')
        .skip(1)
        .filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| key.eq_ignore_ascii_case("domain"))
        .map(|(_, value)| value.trim())
        .ok_or("Android Cookie 缺少域信息")?;
    let cookie = Cookie::parse(raw.to_owned()).map_err(|_| "Android Cookie 格式无效")?;
    if cookie.path().is_none() {
        return Err("Android Cookie 缺少路径信息".into());
    }
    let mut stored = StoredAuthCookie::from_webview_cookie(cookie.into_owned())
        .ok_or("Android Cookie 范围无效")?;
    stored.host_only = !domain.starts_with('.');
    stored.to_webview_cookie()?;
    Ok(stored)
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
    let mut url: tauri::Url = format!("https://{AUTH_HOST}/")
        .parse()
        .expect("fixed auth origin");
    url.set_path(&cookie.path);
    Ok(url)
}

const AUTH_COOKIE_URLS: [&str; 4] = [
    AUTH_LOGIN_URL,
    AUTH_PROFILE_URL,
    AUTH_MOBILE_PROFILE_URL,
    AUTH_USER_CONF_URL,
];

pub(super) fn cookie_in_auth_scope(cookie: &StoredAuthCookie) -> bool {
    is_auth_cookie_domain(&cookie.domain)
        && (!cookie.host_only || cookie.domain.trim_start_matches('.') == AUTH_HOST)
        && is_valid_cookie_path(&cookie.path)
        && AUTH_COOKIE_URLS.iter().any(|url| {
            let url: tauri::Url = url.parse().expect("fixed official URL");
            url.path() == cookie.path
                || (url.path().starts_with(&cookie.path)
                    && (cookie.path.ends_with('/')
                        || url.path().as_bytes().get(cookie.path.len()) == Some(&b'/')))
        })
}

#[cfg(target_os = "android")]
fn read_android_auth_cookies() -> Result<Vec<StoredAuthCookie>, String> {
    let mut stored = Vec::new();
    let mut seen = HashSet::new();
    for raw in super::android_cookies::read_cookies(&AUTH_COOKIE_URLS)? {
        let cookie = from_android_cookie_info(&raw)?;
        let key = (
            cookie.name.clone(),
            cookie.domain.clone(),
            cookie.path.clone(),
            cookie.host_only,
        );
        if seen.insert(key) {
            stored.push(cookie);
        }
    }
    Ok(stored)
}

pub(super) fn capture_auth_session(
    window: &tauri::WebviewWindow,
    student_id: String,
) -> Result<AuthSessionSecret, String> {
    #[cfg(target_os = "android")]
    let stored = {
        let _ = window;
        read_android_auth_cookies()?
    };
    #[cfg(target_os = "linux")]
    let stored = super::linux_cookies::read(window)?;
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let stored = {
        let mut cookies = Vec::new();
        for url in AUTH_COOKIE_URLS {
            cookies.extend(
                window
                    .cookies_for_url(url.parse().expect("fixed official URL"))
                    .map_err(|error| error.to_string())?,
            );
        }
        collect_auth_cookies(cookies)
    };
    if stored.is_empty() {
        return Err("未读取到官方认证会话".into());
    }
    Ok(AuthSessionSecret {
        format_version: if cfg!(target_os = "linux") { 3 } else { 2 },
        student_id,
        cookies: stored,
    })
}

#[cfg(any(not(any(target_os = "android", target_os = "linux")), test))]
pub(super) fn collect_auth_cookies(
    cookies: impl IntoIterator<Item = Cookie<'static>>,
) -> Vec<StoredAuthCookie> {
    let mut seen = HashSet::new();
    let mut stored = Vec::new();
    for cookie in cookies {
        let Some(cookie) = StoredAuthCookie::from_webview_cookie(cookie) else {
            continue;
        };
        let key = (
            cookie.name.clone(),
            cookie.domain.clone(),
            cookie.path.clone(),
            cookie.host_only,
        );
        if seen.insert(key) {
            stored.push(cookie);
        }
    }
    stored
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
pub(super) fn install_auth_cookie(
    window: &tauri::WebviewWindow,
    cookie: &StoredAuthCookie,
) -> Result<(), String> {
    let mut native = cookie.to_webview_cookie()?;
    if cookie.host_only {
        native.set_domain(cookie.domain.clone());
    }
    window.set_cookie(native).map_err(|error| error.to_string())
}

#[cfg(target_os = "android")]
fn android_cookie_deletions() -> Result<Vec<serde_json::Value>, String> {
    read_android_auth_cookies()?.into_iter().map(|cookie| {
        let mut expired = cookie.to_webview_cookie()?;
        expired.set_max_age(time::Duration::ZERO);
        Ok(serde_json::json!({"url": auth_cookie_url(&cookie)?.as_str(), "header": expired.to_string()}))
    }).collect()
}

/// Install persisted cookies without navigating or clearing the local app
/// document. Startup refresh uses this path so the workbench remains visible.
pub(super) fn install_auth_session_cookies(
    app: &tauri::AppHandle,
    session: &AuthSessionSecret,
) -> Result<(), String> {
    if !session.can_restore() {
        return Err("旧版认证会话需要重新登录".into());
    }
    #[cfg(target_os = "android")]
    {
        let _ = app;
        // Replace the official-origin jar, including stale cookies omitted by
        // Set-Cookie deletions. Other SZU services' host cookies are untouched.
        let mut writes = android_cookie_deletions()?;
        for cookie in &session.cookies {
            if !cookie.is_definitely_expired(OffsetDateTime::now_utc()) {
                writes.push(serde_json::json!({"url": auth_cookie_url(cookie)?.as_str(), "header": cookie.to_webview_cookie()?.to_string()}));
            }
        }
        super::android_cookies::write_cookies(&writes)
    }
    #[cfg(target_os = "linux")]
    {
        let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
        let cookies = session
            .cookies
            .iter()
            .filter(|cookie| !cookie.is_definitely_expired(OffsetDateTime::now_utc()))
            .cloned()
            .collect();
        super::linux_cookies::replace(&window, cookies)
    }
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    {
        let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
        for cookie in &session.cookies {
            if !cookie.is_definitely_expired(OffsetDateTime::now_utc()) {
                install_auth_cookie(&window, cookie)?;
            }
        }
        Ok(())
    }
}

pub(super) fn clear_webview_session(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if let Some(window) = app.get_webview_window("main") {
            super::linux_cookies::replace(&window, Vec::new())?;
        }
        return Ok(());
    }
    #[cfg(target_os = "android")]
    {
        super::android_cookies::write_cookies(&android_cookie_deletions()?)?;
        if !read_android_auth_cookies()?.is_empty() {
            return Err("Android 认证会话未能清除".into());
        }
    }
    #[cfg(not(target_os = "linux"))]
    if let Some(window) = app.get_webview_window("main") {
        window
            .clear_all_browsing_data()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(target_os = "linux"))]
    Ok(())
}
