use super::{
    model::Mode,
    protocol::{Error, Failure, Result},
};
use cookie_store::{CookieDomain, CookieStore, RawCookie};
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub const VPN: &str = "https://webvpn.szu.edu.cn/";
pub const EHALL: &str = "https://ehall.szu.edu.cn/jwapp/sys/xywccx/*default/index.do";
pub const VPN_NAMES: [&str; 3] = ["_webvpn_key", "webvpn_username", "webvpn_username_NS_Sig"];
#[derive(Default, Serialize, Deserialize)]
pub struct Secrets {
    pub student: String,
    pub vpn: String,
    pub campus: String,
    pub webvpn: String,
    pub ehall: String,
    #[serde(default)]
    pub academic_context: Option<serde_json::Value>,
    pub campus_token: String,
    pub webvpn_token: String,
}
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
    let jar = if raw.is_empty() {
        CookieStore::default()
    } else {
        cookie_store::serde::json::load(raw.as_bytes())
            .map_err(|_| Error::new(Failure::Protocol, "认证数据损坏，请重新认证"))?
    };
    Ok(Arc::new(CookieStoreMutex::new(jar)))
}
pub fn serialize(jar: &CookieStoreMutex) -> Result<String> {
    let mut out = Vec::new();
    cookie_store::serde::json::save_incl_expired_and_nonpersistent(
        &*jar
            .lock()
            .map_err(|_| Error::new(Failure::Protocol, "会话锁失败"))?,
        &mut out,
    )
    .map_err(|_| Error::new(Failure::Protocol, "无法保存认证数据"))?;
    String::from_utf8(out).map_err(|_| Error::new(Failure::Protocol, "认证编码无效"))
}
pub fn merge(target: &CookieStoreMutex, source: &CookieStoreMutex) {
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
}
pub fn client(jar: Arc<CookieStoreMutex>) -> Result<reqwest::blocking::Client> {
    #[cfg(target_os = "android")]
    crate::platform_tls::initialize_platform_verifier().map_err(Error::from)?;
    reqwest::blocking::Client::builder()
        .cookie_provider(jar)
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
    let mut jar = CookieStore::default();
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
        let _ = window;
        for header in crate::auth::android_cookies::read_cookies(urls).map_err(Error::from)? {
            let mut c = RawCookie::parse(header)
                .map_err(|_| Error::new(Failure::Protocol, "Cookie格式无效"))?
                .into_owned();
            let domain = c.domain().unwrap_or("").to_owned();
            let host = domain.trim_start_matches('.');
            if !allowed(host, c.name()) {
                continue;
            }
            if !domain.starts_with('.') {
                c.unset_domain();
            }
            let url = format!("https://{host}/")
                .parse()
                .map_err(|_| Error::new(Failure::Protocol, "Cookie域无效"))?;
            let _ = jar.insert_raw(&c, &url);
        }
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
    serialize(&CookieStoreMutex::new(jar))
}
pub fn school_jar(secrets: &Secrets, mode: Mode, fresh: bool) -> Result<Arc<CookieStoreMutex>> {
    let raw = match mode {
        Mode::Campus => &secrets.campus,
        Mode::Webvpn => &secrets.webvpn,
    };
    let school = jar(if fresh { "" } else { raw })?;
    if mode == Mode::Webvpn {
        merge(&school, jar(&secrets.vpn)?.as_ref());
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
    serialize(&CookieStoreMutex::new(target))
}
