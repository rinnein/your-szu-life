//! Native cookie metadata adapter. Values are never exposed through application IPC.
use super::{
    model::Mode,
    protocol::{Error, Failure, Result},
    session,
};
use cookie_store::{CookieDomain, CookieExpiration, CookieStore, RawCookie};
use reqwest_cookie_store::CookieStoreMutex;
use std::{collections::BTreeMap, sync::Arc};
pub type Key = (String, String, String, bool);
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    pub key: Key,
    pub header: String,
    pub url: String,
}
#[derive(Clone)]
pub struct Patch {
    pub record: Record,
    pub remove: bool,
}
pub type Records = BTreeMap<Key, Record>;
pub fn allowed(mode: Mode, domain: &str, name: &str) -> bool {
    let host = reqwest::Url::parse(mode.origin())
        .unwrap()
        .host_str()
        .unwrap()
        .to_string();
    domain.trim_start_matches('.') == host
        || (["route", "insert_cookie", "JSESSIONID", "_WEU"].contains(&name)
            && (domain.trim_start_matches('.') == "szu.edu.cn"
                || (mode == Mode::Webvpn && domain.trim_start_matches('.') == "webvpn.szu.edu.cn")))
        || (mode == Mode::Webvpn
            && session::VPN_NAMES.contains(&name)
            && matches!(
                domain.trim_start_matches('.'),
                "szu.edu.cn" | "webvpn.szu.edu.cn"
            ))
}
pub fn records(jar: &CookieStoreMutex, mode: Mode) -> Records {
    let jar = jar.lock().unwrap();
    let mut result = Records::new();
    for c in jar.iter_unexpired() {
        let (domain, host_only) = match &c.domain {
            CookieDomain::HostOnly(d) => (d.clone(), true),
            CookieDomain::Suffix(d) => (d.clone(), false),
            _ => continue,
        };
        if !allowed(mode, &domain, c.name()) {
            continue;
        }
        let mut raw: RawCookie<'static> = c.clone().into();
        raw.set_path(c.path.as_ref().to_string());
        raw.set_max_age(None);
        if host_only {
            raw.unset_domain();
        } else {
            raw.set_domain(domain.clone());
        }
        match c.expires {
            CookieExpiration::AtUtc(date) => raw.set_expires(date),
            CookieExpiration::SessionEnd => raw.unset_expires(),
        };
        let key = (
            c.name().to_string(),
            domain.clone(),
            c.path.as_ref().to_string(),
            host_only,
        );
        // Host-only cookies must be installed against their original host, never a parent domain.
        let scheme = if mode == Mode::Campus && domain == "bkxk.szu.edu.cn" {
            "http"
        } else {
            "https"
        };
        result.insert(
            key.clone(),
            Record {
                key,
                header: raw.to_string(),
                url: format!("{scheme}://{domain}{}", c.path.as_ref()),
            },
        );
    }
    result
}
pub fn patch(from: &Records, to: &Records) -> Vec<Patch> {
    let mut result = vec![];
    for (key, r) in from {
        if !to.contains_key(key) {
            result.push(Patch {
                record: r.clone(),
                remove: true,
            });
        }
    }
    for (key, r) in to {
        if from.get(key) != Some(r) {
            result.push(Patch {
                record: r.clone(),
                remove: false,
            });
        }
    }
    result
}
pub fn merge_browser_delta(
    jar: &CookieStoreMutex,
    mode: Mode,
    previous_browser: &Records,
    browser: &Records,
    previous_native: &Records,
) {
    let current = records(jar, mode);
    let changes = patch(previous_browser, browser);
    let mut jar = jar.lock().unwrap();
    for change in changes {
        let key = &change.record.key;
        if current.get(key) != previous_native.get(key) {
            continue;
        }
        if change.remove {
            jar.remove(&key.1, &key.2, &key.0);
        } else if let Ok(url) = change.record.url.parse() {
            if let Ok(raw) = RawCookie::parse(change.record.header) {
                let _ = jar.insert_raw(&raw, &url);
            }
        }
    }
}
pub fn capture(
    window: &tauri::WebviewWindow,
    mode: Mode,
    known: &Records,
) -> Result<Arc<CookieStoreMutex>> {
    let mut jar = CookieStore::default();
    let origin = mode.origin();
    let mut urls = vec![
        format!("{origin}/"),
        format!("{origin}/xsxkapp/sys/xsxkapp/*default/index.do"),
    ];
    urls.extend([
        format!("{origin}/xsxkapp/sys/xsxkapp/student/check/login.do"),
        format!("{origin}/xsxkapp/sys/xsxkapp/student/vcode/image.do"),
        format!("{origin}/xsxkapp/sys/xsxkapp/elective/volunteer.do"),
    ]);
    urls.extend(known.values().map(|record| record.url.clone()));
    if mode == Mode::Webvpn {
        urls.push(session::VPN.into());
    }
    urls.sort();
    urls.dedup();
    for url in urls {
        #[cfg(target_os = "linux")]
        let cookies = super::linux_cookies::read(window, &url).map_err(Error::from)?;
        #[cfg(target_os = "android")]
        let cookies = {
            let _ = window;
            crate::auth::android_cookies::read_cookies(&[url.as_str()])
                .map_err(Error::from)?
                .into_iter()
                .filter_map(|h| {
                    let raw = RawCookie::parse(h.clone()).ok()?;
                    Some((raw.domain()?.to_string(), h))
                })
                .collect::<Vec<_>>()
        };
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let cookies = window
            .cookies_for_url(url.parse().unwrap())
            .map_err(|_| Error::new(Failure::Protocol, "无法读取官网会话"))?
            .into_iter()
            .map(|c| {
                (
                    c.domain()
                        .unwrap_or(reqwest::Url::parse(&url).unwrap().host_str().unwrap())
                        .to_string(),
                    c.to_string(),
                )
            })
            .collect::<Vec<_>>();
        for (domain, header) in cookies {
            let mut raw = RawCookie::parse(header)
                .map_err(|_| Error::new(Failure::Protocol, "官网Cookie格式无效"))?
                .into_owned();
            if !allowed(mode, &domain, raw.name()) {
                continue;
            }
            if !domain.starts_with('.') {
                raw.unset_domain();
            }
            let request = reqwest::Url::parse(&url).unwrap();
            let _ = jar.insert_raw(&raw, &request);
        }
    }
    Ok(Arc::new(CookieStoreMutex::new(jar)))
}
pub fn install(window: &tauri::WebviewWindow, changes: Vec<Patch>) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    return super::linux_cookies::write(window, changes).map_err(Error::from);
    #[cfg(target_os = "android")]
    {
        let _ = window;
        let mut headers = vec![];
        for change in changes {
            let mut raw = RawCookie::parse(change.record.header)
                .map_err(|_| Error::new(Failure::Protocol, "Cookie格式无效"))?
                .into_owned();
            if change.remove {
                raw.set_max_age(time::Duration::ZERO);
                raw.set_expires(time::OffsetDateTime::UNIX_EPOCH);
            }
            headers.push(serde_json::json!({"url":change.record.url,"header":raw.to_string()}));
        }
        crate::auth::android_cookies::write_cookies(&headers).map_err(Error::from)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        for change in changes {
            let mut cookie = tauri::webview::cookie::Cookie::parse(change.record.header)
                .map_err(|_| Error::new(Failure::Protocol, "Cookie格式无效"))?
                .into_owned();
            if change.remove {
                cookie.set_max_age(time::Duration::ZERO);
                cookie.set_expires(time::OffsetDateTime::UNIX_EPOCH);
            }
            if change.record.key.3 {
                cookie.set_domain(change.record.key.1);
            }
            window
                .set_cookie(cookie)
                .map_err(|_| Error::new(Failure::Protocol, "无法恢复官网Cookie"))?;
        }
        Ok(())
    }
}
