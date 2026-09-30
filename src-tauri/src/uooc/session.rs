use super::client::{LOGIN, ORIGIN};
use cookie_store::{CookieDomain, CookieStore, RawCookie};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;

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
pub(super) fn load(raw: Option<&str>) -> Result<Arc<CookieStoreMutex>, String> {
    let mut jar = CookieStore::default();
    if let Some(raw) = raw {
        let stored = cookie_store::serde::json::load(raw.as_bytes())
            .map_err(|_| "UOOC 会话数据损坏，请重新登录")?;
        for cookie in stored.iter_any().filter(|c| allowed(c) && !c.is_expired()) {
            let _ = jar.insert(cookie.clone(), &ORIGIN.parse().unwrap());
        }
    }
    Ok(Arc::new(CookieStoreMutex::new(jar)))
}
pub(super) fn serialize(jar: &CookieStoreMutex) -> Result<String, String> {
    let jar = jar.lock().map_err(|_| "UOOC 会话不可用")?;
    let mut bytes = vec![];
    cookie_store::serde::json::save_incl_expired_and_nonpersistent(&jar, &mut bytes)
        .map_err(|_| "UOOC 会话保存失败")?;
    String::from_utf8(bytes).map_err(|_| "UOOC 会话保存失败".into())
}

pub(super) fn capture(window: &tauri::WebviewWindow) -> Result<Arc<CookieStoreMutex>, String> {
    let mut jar = CookieStore::default();
    #[cfg(target_os = "android")]
    {
        let _ = window;
        for header in crate::auth::android_cookies::read_cookies(&URLS)? {
            let domain = header
                .split(';')
                .skip(1)
                .filter_map(|p| p.trim().split_once('='))
                .find(|(k, _)| k.eq_ignore_ascii_case("domain"))
                .map(|(_, v)| v.trim().to_owned())
                .ok_or("UOOC Cookie 缺少域信息")?;
            let mut cookie = RawCookie::parse(header)
                .map_err(|_| "UOOC Cookie 格式无效")?
                .into_owned();
            if !allowed_domain(&domain) {
                continue;
            }
            // AndroidX canonical domain without a dot denotes a host-only cookie.
            if !domain.starts_with('.') {
                cookie.unset_domain();
            }
            if cookie.path().is_none() {
                return Err("UOOC Cookie 缺少路径信息".into());
            }
            let _ = jar.insert_raw(&cookie, &ORIGIN.parse().unwrap());
        }
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
    Ok(Arc::new(CookieStoreMutex::new(jar)))
}

pub(super) fn install(
    window: &tauri::WebviewWindow,
    jar: &CookieStoreMutex,
    delete: bool,
) -> Result<(), String> {
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
    #[cfg(target_os = "android")]
    {
        let _ = window;
        let data: Vec<_> = cookies.iter().map(|c| serde_json::json!({"url": format!("{ORIGIN}{}", c.path().unwrap_or("/")), "header": c.to_string()})).collect();
        crate::auth::android_cookies::write_cookies(&data)?;
    }
    Ok(())
}
