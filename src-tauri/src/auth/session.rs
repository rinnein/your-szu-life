use super::{
    AUTH_HOST, AUTH_LOGIN_URL, AUTH_MOBILE_PROFILE_URL, AUTH_PROFILE_URL, AUTH_USER_CONF_URL,
};
use crate::webview_cookies::CookieRecord;
#[cfg(target_os = "android")]
use crate::webview_cookies::{Scope, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(any(not(any(target_os = "android", target_os = "linux")), test))]
use tauri::webview::cookie::Cookie;
use tauri::Manager;
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

/// Kept exclusively in the system credential store. It is intentionally not
/// serializable through a Tauri command or event payload.
#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct AuthSessionSecret {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) format_version: u8,
    pub(crate) student_id: String,
    pub(crate) cookies: Vec<CookieRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) http_keys: Option<Vec<crate::webview_cookies::Identity>>,
}

impl std::fmt::Debug for AuthSessionSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthSessionSecret")
            .field("format_version", &self.format_version)
            .field("cookie_count", &self.cookies.len())
            .finish()
    }
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

impl AuthSessionSecret {
    pub(super) fn has_android_cookie_metadata(&self) -> bool {
        matches!(self.format_version, 2..=4)
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
        cookies.retain(|cookie| seen.insert(cookie.identity()));
        cookies.reverse();
        crate::webview_cookies::Snapshot {
            cookies: cookies.clone(),
            http_keys: self.http_keys.as_ref().map(|keys| {
                keys.iter()
                    .filter(|k| cookies.iter().any(|c| c.identity() == **k))
                    .cloned()
                    .collect()
            }),
        }
        .validate(&crate::webview_cookies::Scope::auth())
        .map_err(|_| SessionRestoreError::InvalidCookie)?;
        Ok((!cookies.is_empty()).then(|| Self {
            format_version: if cfg!(target_os = "linux") {
                3
            } else {
                self.format_version
            },
            student_id: self.student_id.clone(),
            http_keys: self.http_keys.as_ref().map(|keys| {
                keys.iter()
                    .filter(|k| cookies.iter().any(|c| c.identity() == **k))
                    .cloned()
                    .collect()
            }),
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

/// AndroidX returns canonical domains: a leading dot means a domain cookie.
/// This differs from HTTP Set-Cookie semantics. Inspect the original attribute
/// before the cookie crate strips its leading dot.
#[cfg(test)]
pub(super) fn from_android_cookie_info(raw: &str) -> Result<CookieRecord, String> {
    let stored = CookieRecord::from_android(raw).map_err(String::from)?;
    if !is_auth_cookie_domain(&stored.domain) || (stored.host_only && stored.domain != AUTH_HOST) {
        return Err("Android Cookie 范围无效".into());
    }

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

const AUTH_COOKIE_URLS: [&str; 4] = [
    AUTH_LOGIN_URL,
    AUTH_PROFILE_URL,
    AUTH_MOBILE_PROFILE_URL,
    AUTH_USER_CONF_URL,
];

pub(super) fn cookie_in_auth_scope(cookie: &CookieRecord) -> bool {
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

pub(super) fn capture_auth_session(
    window: &tauri::WebviewWindow,
    student_id: String,
) -> Result<AuthSessionSecret, String> {
    #[cfg(target_os = "android")]
    let captured = {
        let _ = window;
        crate::webview_cookies::capture(window, &Scope::auth())?
    };
    #[cfg(target_os = "android")]
    let (stored, http_keys) = (captured.cookies, captured.http_keys);
    #[cfg(not(target_os = "android"))]
    let http_keys = None;
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
        format_version: if cfg!(target_os = "android") {
            4
        } else if cfg!(target_os = "linux") {
            3
        } else {
            2
        },
        student_id,
        cookies: stored,
        http_keys,
    })
}

#[cfg(any(not(any(target_os = "android", target_os = "linux")), test))]
pub(super) fn collect_auth_cookies(
    cookies: impl IntoIterator<Item = Cookie<'static>>,
) -> Vec<CookieRecord> {
    let mut seen = HashSet::new();
    let mut stored = Vec::new();
    for cookie in cookies {
        let cookie = CookieRecord::from_cookie(cookie, AUTH_HOST);
        if !is_auth_cookie_domain(&cookie.domain) {
            continue;
        }
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
    cookie: &CookieRecord,
) -> Result<(), String> {
    let mut native = cookie.to_webview_cookie()?;
    if cookie.host_only {
        native.set_domain(cookie.domain.clone());
    }
    window.set_cookie(native).map_err(|error| error.to_string())
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
        let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
        crate::webview_cookies::replace(
            &window,
            &Scope::auth(),
            &Snapshot {
                cookies: session.cookies.clone(),
                http_keys: session.http_keys.clone(),
            },
        )
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
        Ok(())
    }
    #[cfg(target_os = "android")]
    {
        let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
        return crate::webview_cookies::clear(&window, &Scope::auth());
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    if let Some(window) = app.get_webview_window("main") {
        window
            .clear_all_browsing_data()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    Ok(())
}
