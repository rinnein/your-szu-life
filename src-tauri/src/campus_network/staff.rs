use super::{
    http::{allowed_url, PortalHttp},
    model::{PortalOnlineState, StaffPortalSessionInfo},
    portal::{config_literals, decode_portal_html, meta_refresh_target, parse_jsonp_callback},
};
use reqwest::Url;
use serde_json::Value;
use std::{collections::HashSet, net::IpAddr};

const STAFF_ORIGIN: &str = "https://net.szu.edu.cn/";
const MAX_REDIRECTS: usize = 4;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaffPortalConfig {
    pub(super) ip: String,
    pub(super) acid: String,
    pub(super) mac_auth: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct StaffPortalProbe {
    pub(super) reachable: bool,
    pub(super) state: PortalOnlineState,
    pub(super) config: Option<StaffPortalConfig>,
    pub(super) session: Option<StaffPortalSessionInfo>,
    pub(super) domain: String,
    pub(super) page_url: Option<Url>,
    pub(super) nas_ip: String,
}

pub(super) fn valid_ip(value: &str) -> Option<IpAddr> {
    let ip: IpAddr = value.trim().parse().ok()?;
    (!ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast()).then_some(ip)
}

pub(super) fn unsigned(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|number| *number <= MAX_SAFE_INTEGER)
}

fn text(value: &Value, key: &str) -> Option<String> {
    let text = value.get(key)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub(super) fn parse_staff_config(html: &str) -> Option<StaffPortalConfig> {
    let values = config_literals(html)?;
    let ip = valid_ip(values.get("ip")?.as_str()?)?.to_string();
    let acid = values.get("acid")?;
    let acid = acid
        .as_str()
        .map(str::to_owned)
        .or_else(|| unsigned(acid).map(|n| n.to_string()))?;
    if acid.is_empty() || acid.len() > 16 || !acid.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(StaffPortalConfig {
        ip,
        acid,
        mac_auth: values.get("portal.MacAuth")?.as_bool()?,
    })
}

pub(super) fn staff_url(path: &str) -> Url {
    Url::parse(STAFF_ORIGIN)
        .unwrap()
        .join(path)
        .expect("static staff URL")
}

pub(super) fn staff_redirect(base: &Url, target: &str) -> Option<Url> {
    let url = base.join(target).ok()?;
    (allowed_url(&url) && url.scheme() == "https" && url.host_str() == Some("net.szu.edu.cn"))
        .then_some(url)
}

fn read_config(http: &dyn PortalHttp, probe: &mut StaffPortalProbe) -> Option<StaffPortalConfig> {
    let mut url = staff_url("/");
    let mut visited = HashSet::new();
    for _ in 0..=MAX_REDIRECTS {
        if !visited.insert(url.clone()) {
            return None;
        }
        let response = http.get(&url).ok()?;
        probe.reachable = true;
        probe.state = PortalOnlineState::Unknown;
        if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            url = staff_redirect(&url, response.location.as_deref()?)?;
            continue;
        }
        if !(200..300).contains(&response.status) {
            return None;
        }
        let html = decode_portal_html(&response.body, response.content_type.as_deref());
        if let Some(config) = parse_staff_config(&html) {
            probe.page_url = Some(url.clone());
            probe.nas_ip = config_literals(&html)
                .and_then(|values| values.get("nas").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_default();
            if !probe.nas_ip.is_empty() && valid_ip(&probe.nas_ip).is_none() {
                return None;
            }
            return Some(config);
        }
        url = staff_redirect(&url, &meta_refresh_target(&html)?)?;
    }
    None
}

pub(super) fn staff_info_url(config: &StaffPortalConfig, now: u64) -> (Url, String) {
    let callback = format!("szuLifeInfo_{now}");
    let mut url = staff_url("/cgi-bin/rad_user_info");
    url.query_pairs_mut()
        .append_pair("callback", &callback)
        .append_pair("ip", &config.ip)
        .append_pair("_", &now.to_string());
    (url, callback)
}

pub(super) fn apply_staff_info(probe: &mut StaffPortalProbe, value: &Value) {
    probe.session = None;
    probe.domain.clear();
    probe.state = PortalOnlineState::Unknown;
    let Some(config) = probe.config.as_ref() else {
        return;
    };
    if value.get("error").and_then(Value::as_str) == Some("not_online_error") {
        probe.state = PortalOnlineState::Offline;
        return;
    }
    if value.get("error").and_then(Value::as_str) != Some("ok") {
        return;
    }
    let Some(username) = text(value, "user_name") else {
        return;
    };
    if username.len() > 256 || username.chars().any(char::is_control) {
        return;
    }
    let Some(ip) = valid_ip(&config.ip) else {
        return;
    };
    let online_ip = text(value, "online_ip").and_then(|ip| valid_ip(&ip));
    let online_ipv6 = text(value, "online_ip6")
        .and_then(|ip| valid_ip(&ip))
        .filter(IpAddr::is_ipv6);
    if if ip.is_ipv4() { online_ip } else { online_ipv6 } != Some(ip) {
        return;
    }
    probe.domain = text(value, "domain").unwrap_or_default();
    probe.session = Some(StaffPortalSessionInfo {
        username,
        real_name: text(value, "real_name"),
        login_ip: online_ip.unwrap_or(ip).to_string(),
        login_ipv6: online_ipv6.map(|ip| ip.to_string()),
        user_mac: text(value, "user_mac"),
        used_bytes: value.get("sum_bytes").and_then(unsigned),
        used_seconds: value.get("sum_seconds").and_then(unsigned),
        login_started_at_ms: value
            .get("add_time")
            .and_then(unsigned)
            .and_then(|seconds| seconds.checked_mul(1000))
            .filter(|n| *n <= MAX_SAFE_INTEGER),
    });
    probe.state = PortalOnlineState::Online;
}

pub(super) fn probe_staff_portal(http: &dyn PortalHttp, now: u64) -> StaffPortalProbe {
    let mut probe = StaffPortalProbe::default();
    let Some(config) = read_config(http, &mut probe) else {
        return probe;
    };
    let (url, callback) = staff_info_url(&config, now);
    probe.config = Some(config);
    let Ok(response) = http.get(&url) else {
        return probe;
    };
    if !(200..300).contains(&response.status) {
        return probe;
    }
    if let Ok(value) = parse_jsonp_callback(&String::from_utf8_lossy(&response.body), &callback) {
        apply_staff_info(&mut probe, &value);
    }
    probe
}

#[cfg(test)]
mod tests;
