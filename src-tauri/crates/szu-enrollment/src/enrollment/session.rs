use super::{
    model::Mode,
    protocol::{Error, Failure, Result},
};
#[cfg(not(target_os = "android"))]
use cookie_store::RawCookie;
use cookie_store::{CookieDomain, CookieStore};
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;
pub const VPN: &str = "https://webvpn.szu.edu.cn/";
pub const EHALL: &str = "https://ehall.szu.edu.cn/jwapp/sys/xywccx/*default/index.do";
pub const VPN_NAMES: [&str; 3] = ["_webvpn_key", "webvpn_username", "webvpn_username_NS_Sig"];
pub use szu_core::storage::model::EnrollmentSecrets as Secrets;
pub fn vpn_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|h| h == "webvpn.szu.edu.cn" || h.ends_with(".webvpn.szu.edu.cn"))
        && url.username().is_empty()
        && url.password().is_none()
}
pub fn auth_url(target: &str) -> Result<reqwest::Url> {
    let target = reqwest::Url::parse(target)
        .map_err(|_| Error::new(Failure::Protocol, "无效的 WebVPN 地址"))?;
    if !vpn_url(&target) {
        return Err(Error::new(Failure::Protocol, "仅允许学校 WebVPN 地址"));
    }
    let mut callback =
        reqwest::Url::parse("https://webvpn.szu.edu.cn/users/auth/cas/callback").unwrap();
    callback
        .query_pairs_mut()
        .append_pair("url", target.as_str());
    let mut auth =
        reqwest::Url::parse("https://authserver-443.webvpn.szu.edu.cn/authserver/login").unwrap();
    auth.query_pairs_mut()
        .append_pair("service", callback.as_str());
    Ok(auth)
}
pub fn jar(raw: &str) -> Result<Arc<CookieStoreMutex>> {
    szu_core::webview_cookies::load_jar(raw).map_err(Error::from)
}
pub fn serialize(jar: &CookieStoreMutex) -> Result<String> {
    szu_core::webview_cookies::serialize_jar(jar).map_err(Error::from)
}
pub fn merge(target: &Arc<CookieStoreMutex>, source: &CookieStoreMutex) -> Result<()> {
    let target_owner = target;
    let source_owner = source;
    let source = source.lock().unwrap();
    let mut target = target.lock().unwrap();
    for c in source.iter_unexpired() {
        let host = match &c.domain {
            CookieDomain::HostOnly(h) | CookieDomain::Suffix(h) => h,
            _ => continue,
        };
        if let Ok(url) = format!("https://{host}/").parse() {
            let _ = target.insert(c.clone(), &url);
        }
    }
    drop(target);
    drop(source);
    let scope = szu_core::webview_cookies::Scope::new(
        szu_core::webview_cookies::Kind::Webvpn,
        &[
            VPN,
            "https://bkxk.webvpn.szu.edu.cn/",
            "https://bkxk.webvpn.szu.edu.cn/xsxkapp/sys/xsxkapp/*default/index.do",
        ],
    )
    .map_err(Error::from)?;
    szu_core::webview_cookies::merge_native(target_owner, source_owner, scope).map_err(Error::from)
}
pub fn client(jar: Arc<CookieStoreMutex>) -> Result<reqwest::blocking::Client> {
    #[cfg(target_os = "android")]
    szu_core::platform_tls::initialize_platform_verifier().map_err(Error::from)?;
    szu_core::netproxy::blocking_client_builder()
        .cookie_provider(szu_core::webview_cookies::http_provider(jar))
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .connect_timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 SZULife/0.3")
        .build()
        .map_err(|_| Error::new(Failure::Network, "无法建立网络连接"))
}
pub fn gateway_valid(raw: &str) -> Result<bool> {
    let jar = jar(raw)?;
    let target: reqwest::Url =
        "https://bkxk.webvpn.szu.edu.cn/xsxkapp/sys/xsxkapp/*default/index.do"
            .parse()
            .unwrap();
    let names: Vec<String> = jar
        .lock()
        .unwrap()
        .get_request_values(&target)
        .map(|(n, _)| n.to_string())
        .collect();
    if !VPN_NAMES.iter().all(|n| names.iter().any(|x| x == n)) {
        return Ok(false);
    }
    let response = client(jar)?
        .get(target)
        .send()
        .map_err(|_| Error::new(Failure::Network, "WebVPN 暂时无法连接"))?;
    let status = response.status();
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if status.as_u16() >= 500 {
        return Err(Error::new(Failure::Network, "WebVPN 暂时不可用"));
    }
    if status.is_redirection() {
        return Ok(!location.contains("/users/")
            && !location.contains("authserver")
            && location.contains("xsxkapp"));
    }
    let body = response.text().unwrap_or_default();
    Ok(status.is_success()
        && (body.contains("xsxkapp") || body.contains("loginPwd"))
        && !body.contains("/users/auth/cas"))
}
pub fn capture(window: &tauri::WebviewWindow, urls: &[&str], vpn: bool) -> Result<String> {
    #[cfg(not(target_os = "android"))]
    let mut jar = CookieStore::default();
    #[cfg(not(target_os = "android"))]
    let allowed = |host: &str, name: &str| {
        if vpn {
            VPN_NAMES.contains(&name)
                && (host == "szu.edu.cn"
                    || host == "webvpn.szu.edu.cn"
                    || host.ends_with(".webvpn.szu.edu.cn"))
        } else {
            host == "ehall.szu.edu.cn"
        }
    };
    #[cfg(target_os = "android")]
    {
        let kind = if vpn {
            szu_core::webview_cookies::Kind::Gateway
        } else {
            szu_core::webview_cookies::Kind::Academic
        };
        let scope = szu_core::webview_cookies::Scope::new(kind, urls).map_err(Error::from)?;
        let snapshot = szu_core::webview_cookies::capture(window, &scope).map_err(Error::from)?;
        let jar = szu_core::webview_cookies::project(snapshot, scope).map_err(Error::from)?;
        return serialize(jar.as_ref());
    }
    #[cfg(target_os = "linux")]
    for u in urls {
        for (domain, header) in super::linux_cookies::read(window, u).map_err(Error::from)? {
            let mut raw = RawCookie::parse(header)
                .map_err(|_| Error::new(Failure::Protocol, "Cookie格式无效"))?
                .into_owned();
            if !allowed(domain.trim_start_matches('.'), raw.name()) {
                continue;
            }
            if !domain.starts_with('.') {
                raw.unset_domain();
            }
            let _ = jar.insert_raw(&raw, &u.parse().unwrap());
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    for u in urls {
        let url: reqwest::Url = u.parse().unwrap();
        for c in window
            .cookies_for_url(url.clone())
            .map_err(|_| Error::new(Failure::Protocol, "无法捕获认证Cookie"))?
        {
            if !allowed(c.domain().unwrap_or(url.host_str().unwrap()), c.name()) {
                continue;
            }
            let raw = RawCookie::parse(c.to_string())
                .map_err(|_| Error::new(Failure::Protocol, "Cookie格式无效"))?;
            let _ = jar.insert_raw(&raw, &url);
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        serialize(&CookieStoreMutex::new(jar))
    }
}
pub fn school_jar(secrets: &Secrets, mode: Mode, fresh: bool) -> Result<Arc<CookieStoreMutex>> {
    let raw = match mode {
        Mode::Campus => &secrets.campus,
        Mode::Webvpn => &secrets.webvpn,
    };
    let school = jar(if fresh { "" } else { raw })?;
    if mode == Mode::Webvpn {
        merge(&school, jar(&secrets.vpn)?.as_ref())?;
    }
    Ok(school)
}

pub fn matches_account(raw: &str, student: &str, campus_card: &str) -> Result<bool> {
    let jar = jar(raw)?;
    let target = "https://bkxk.webvpn.szu.edu.cn/".parse().unwrap();
    let jar = jar.lock().unwrap();
    let username = jar
        .get_request_values(&target)
        .find(|(n, _)| *n == "webvpn_username")
        .map(|(_, v)| v.to_owned())
        .unwrap_or_default();
    let decoded = reqwest::Url::parse(&format!("https://localhost/?value={username}"))
        .ok()
        .and_then(|u| u.query_pairs().next().map(|(_, v)| v.into_owned()))
        .unwrap_or_default();
    Ok([username.as_str(), decoded.as_str()].iter().any(|value| {
        !value.is_empty()
            && (*value == student || (!campus_card.is_empty() && *value == campus_card))
    }))
}

#[derive(Clone)]
pub struct SharedSchool {
    pub client: super::client::Client,
    pub generation: u64,
    pub gateway_seed: String,
    pub logged_out: bool,
}

pub fn gateway_only(source: &CookieStoreMutex) -> Result<String> {
    let mut target = CookieStore::default();
    let native = szu_core::webview_cookies::native_snapshot(source);
    let source = source.lock().unwrap();
    for cookie in source
        .iter_unexpired()
        .filter(|c| VPN_NAMES.contains(&c.name()))
    {
        let host = match &cookie.domain {
            CookieDomain::HostOnly(h) | CookieDomain::Suffix(h) => h,
            _ => continue,
        };
        if let Ok(url) = format!("https://{host}/").parse() {
            let _ = target.insert(cookie.clone(), &url);
        }
    }
    let http = serialize(&CookieStoreMutex::new(target))?;
    if let Some((snapshot, _)) = native {
        let scope = szu_core::webview_cookies::Scope::new(
            szu_core::webview_cookies::Kind::Gateway,
            &[VPN, "https://bkxk.webvpn.szu.edu.cn/"],
        )
        .map_err(Error::from)?;
        let keys = snapshot.http_keys.clone();
        let cookies: Vec<_> = snapshot
            .cookies
            .into_iter()
            .filter(|c| scope.allows(c))
            .collect();
        return szu_core::webview_cookies::SessionEnvelope::encode(
            http,
            szu_core::webview_cookies::Snapshot {
                http_keys: keys.map(|keys| {
                    keys.into_iter()
                        .filter(|key| cookies.iter().any(|c| c.identity() == *key))
                        .collect()
                }),
                cookies,
            },
            scope,
        )
        .map_err(Error::from);
    }
    Ok(http)
}

