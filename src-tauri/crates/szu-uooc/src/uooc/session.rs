#[cfg(not(target_os = "android"))]
use super::client::LOGIN;
use super::client::ORIGIN;
#[cfg(not(target_os = "android"))]
use cookie_store::RawCookie;
use cookie_store::{CookieDomain, CookieStore};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

#[cfg(not(target_os = "android"))]
const URLS: [&str; 5] = [
    LOGIN,
    ORIGIN,
    "https://www.uooc.net.cn/home",
    "https://www.uooc.net.cn/home/member/user",
    "https://www.uooc.net.cn/home/course/list",
];
fn allowed_domain(domain: &str) -> bool {
    matches!(
        domain.trim_start_matches('.'),
        "www.uooc.net.cn" | "uooc.net.cn"
    )
}
fn allowed(cookie: &cookie_store::Cookie<'_>) -> bool {
    match &cookie.domain {
        CookieDomain::HostOnly(host) => host == "www.uooc.net.cn",
        CookieDomain::Suffix(domain) => allowed_domain(domain),
        _ => false,
    }
}
pub fn load(raw: Option<&str>) -> Result<Arc<CookieStoreMutex>, String> {
    let stored = szu_core::webview_cookies::load_jar(raw.unwrap_or(""))?;
    if let Some((snapshot, scope)) = szu_core::webview_cookies::native_snapshot(&stored) {
        if scope.kind != szu_core::webview_cookies::Kind::Uooc {
            return Err("UOOC 会话范围无效".into());
        }
        snapshot
            .validate(&szu_core::webview_cookies::Scope::uooc())
            .map_err(String::from)?;
    }
    let mut jar = CookieStore::default();
    for cookie in stored
        .lock()
        .map_err(|_| "UOOC 会话不可用")?
        .iter_any()
        .filter(|c| allowed(c) && !c.is_expired())
    {
        let _ = jar.insert(cookie.clone(), &ORIGIN.parse().unwrap());
    }
    let jar = Arc::new(CookieStoreMutex::new(jar));
    szu_core::webview_cookies::copy_snapshot(&stored, &jar);
    Ok(jar)
}
pub fn serialize(jar: &CookieStoreMutex) -> Result<String, String> {
    szu_core::webview_cookies::serialize_jar(jar)
}

pub fn capture(window: &tauri::WebviewWindow) -> Result<Arc<CookieStoreMutex>, String> {
    #[cfg(not(target_os = "android"))]
    let mut jar = CookieStore::default();
    #[cfg(target_os = "android")]
    {
        let scope = szu_core::webview_cookies::Scope::uooc();
        return szu_core::webview_cookies::project(
            szu_core::webview_cookies::capture(window, &scope)?,
            scope,
        );
    }
    #[cfg(not(target_os = "android"))]
    for url in URLS {
        for cookie in window
            .cookies_for_url(url.parse().unwrap())
            .map_err(|_| "无法读取 UOOC Cookie")?
        {
            if !allowed_domain(cookie.domain().unwrap_or("www.uooc.net.cn")) {
                continue;
            }
            let raw = RawCookie::parse(cookie.to_string()).map_err(|_| "UOOC Cookie 格式无效")?;
            let _ = jar.insert_raw(&raw, &url.parse().unwrap());
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(Arc::new(CookieStoreMutex::new(jar)))
    }
}

pub fn install(
    window: &tauri::WebviewWindow,
    jar: &CookieStoreMutex,
    delete: bool,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let scope = szu_core::webview_cookies::Scope::uooc();
        if delete {
            return szu_core::webview_cookies::clear(window, &scope);
        }
        if let Some(envelope) = szu_core::webview_cookies::SessionEnvelope::decode(&serialize(jar)?)? {
            return szu_core::webview_cookies::replace(window, &scope, &envelope.webview);
        }
        let records = jar
            .lock()
            .map_err(|_| "UOOC 会话不可用")?
            .iter_any()
            .filter(|c| allowed(c))
            .map(|c| szu_core::webview_cookies::Patch {
                cookie: szu_core::webview_cookies::CookieRecord::from_stored(c)
                    .expect("allowed cookie domain"),
                remove: false,
            })
            .collect::<Vec<_>>();
        return szu_core::webview_cookies::apply(window, &scope, &records);
    }
    #[cfg(not(target_os = "android"))]
    {
        let cookies: Vec<RawCookie<'static>> = jar
            .lock()
            .map_err(|_| "UOOC 会话不可用")?
            .iter_any()
            .filter(|c| allowed(c))
            .map(|cookie| {
                let mut raw: RawCookie<'static> = cookie.clone().into();
                if delete {
                    raw.set_max_age(time::Duration::ZERO);
                    raw.set_expires(time::OffsetDateTime::UNIX_EPOCH);
                }
                raw
            })
            .collect();
        #[cfg(not(target_os = "android"))]
        for raw in cookies {
            let cookie = tauri::webview::cookie::Cookie::parse(raw.to_string())
                .map_err(|_| "UOOC Cookie 格式无效")?
                .into_owned();
            window
                .set_cookie(cookie)
                .map_err(|_| "无法恢复 UOOC Cookie")?;
        }
        Ok(())
    }
}
