use super::{
    http::{portal_url, PortalHttp},
    model::*,
    portal::*,
    staff::{probe_staff_portal, unsigned, valid_ip, StaffPortalProbe},
};
use serde_json::Value;
#[cfg(not(target_os = "android"))]
use std::process::Command;
use std::{thread, time::Duration};

pub(super) const DORM_PORTAL_HOST: &str = "172.30.255.42";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NetworkSnapshot {
    pub(crate) connected: bool,
    pub(crate) ssid: Option<String>,
    pub(crate) ip: Option<String>,
    pub(crate) transport: Option<String>,
    pub(crate) capabilities_available: Option<bool>,
    pub(crate) wifi_permission: Option<String>,
    pub(crate) location_enabled: Option<bool>,
    pub(crate) has_private_172_ipv4: bool,
    pub(crate) wifi_connected: bool,
    pub(crate) wifi_ip: Option<String>,
    pub(crate) wifi_has_private_172_ipv4: bool,
}

#[cfg(not(target_os = "android"))]
pub(super) fn command_text(command: &str, args: &[&str]) -> Option<String> {
    Command::new(command)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn network_snapshot() -> NetworkSnapshot {
    #[cfg(target_os = "linux")]
    {
        let ssid =
            command_text("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"]).and_then(|text| {
                text.lines().find_map(|line| {
                    let (active, ssid) = line.split_once(':')?;
                    (active == "yes" && !ssid.is_empty()).then(|| ssid.to_owned())
                })
            });
        let ip = command_text("hostname", &["-I"]).and_then(|text| {
            text.split_whitespace()
                .find(|value| valid_ipv4(value))
                .map(str::to_owned)
        });
        return NetworkSnapshot {
            connected: ip.is_some() || ssid.is_some(),
            ssid,
            ip,
            ..Default::default()
        };
    }
    #[cfg(target_os = "macos")]
    {
        let ssid = command_text("networksetup", &["-getairportnetwork", "en0"])
            .and_then(|text| {
                text.split_once(": ")
                    .map(|(_, value)| value.trim().to_owned())
            })
            .filter(|value| !value.is_empty() && !value.contains("not associated"));
        let ip = command_text("ipconfig", &["getifaddr", "en0"])
            .and_then(|text| text.trim().parse::<std::net::Ipv4Addr>().ok())
            .map(|value| value.to_string());
        return NetworkSnapshot {
            connected: ip.is_some() || ssid.is_some(),
            ssid,
            ip,
            ..Default::default()
        };
    }
    #[cfg(target_os = "windows")]
    {
        let ssid = command_text("netsh", &["wlan", "show", "interfaces"]).and_then(|text| {
            text.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim().eq_ignore_ascii_case("SSID") && !value.trim().is_empty())
                    .then(|| value.trim().to_owned())
            })
        });
        return NetworkSnapshot {
            connected: ssid.is_some(),
            ssid,
            ip: None,
            ..Default::default()
        };
    }
    #[cfg(target_os = "android")]
    {
        super::android::network_snapshot()
    }
}

impl NetworkSnapshot {
    pub(super) fn effective_connected(&self) -> bool {
        self.connected || self.wifi_connected || self.ip.is_some() || self.wifi_ip.is_some()
    }

    pub(super) fn fingerprint(&self) -> String {
        format!(
            "{:?}|{:?}|{}|{:?}|{}|{}|{:?}|{}",
            self.ssid,
            self.ip,
            self.connected,
            self.transport,
            self.has_private_172_ipv4,
            self.wifi_connected,
            self.wifi_ip,
            self.wifi_has_private_172_ipv4
        )
    }
}

pub(super) fn network_snapshot_result() -> Result<NetworkSnapshot, String> {
    #[cfg(target_os = "android")]
    {
        return super::android::network_snapshot_result();
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(network_snapshot())
    }
}

pub(super) trait NetworkEnvironment: Sync {
    fn snapshot(&self) -> NetworkSnapshot;
    fn now_ms(&self) -> u64;
    fn sleep(&self, duration: Duration);
}
pub(super) struct SystemNetworkEnvironment;
impl NetworkEnvironment for SystemNetworkEnvironment {
    fn snapshot(&self) -> NetworkSnapshot {
        network_snapshot()
    }
    fn now_ms(&self) -> u64 {
        super::now_ms()
    }
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DormPortalContext {
    pub(super) ip: String,
    pub(super) ipv6: String,
    pub(super) mac: String,
    pub(super) vlan: String,
    pub(super) ac_ip: String,
    pub(super) ac_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DormPortalProbe {
    pub(crate) reachable: bool,
    pub(crate) authenticated: bool,
    pub(super) recognized: bool,
    pub(super) state: PortalOnlineState,
    pub(crate) session: Option<DormPortalSessionInfo>,
    pub(super) context: Option<DormPortalContext>,
}

fn gateway_text(value: Option<&Value>, name: &str) -> Option<String> {
    let text = value?.get(name)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn dorm_context(html: &str, status: Option<&Value>) -> Option<DormPortalContext> {
    let text = |key| extract_portal_value(html, key).or_else(|| gateway_text(status, key));
    let ip = extract_terminal_ip(html)
        .and_then(|value| valid_ip(&value))
        .filter(|ip| ip.is_ipv4())
        .or_else(|| {
            ["v4ip", "v46ip", "ss5"]
                .into_iter()
                .filter_map(text)
                .filter_map(|value| valid_ip(&value))
                .find(|ip| ip.is_ipv4())
        })?
        .to_string();
    let mac = ["olmac", "ss4"]
        .into_iter()
        .filter_map(text)
        .find_map(|value| {
            let value = value.replace([':', '-'], "");
            (value.len() == 12 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| value.to_ascii_lowercase())
        })
        .unwrap_or_else(|| "000000000000".into());
    let vlan = text("vlanid")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value <= 4095)
        .unwrap_or(1)
        .to_string();
    Some(DormPortalContext {
        ip,
        mac,
        vlan,
        ipv6: text("v6ip")
            .and_then(|value| valid_ip(&value))
            .filter(|ip| ip.is_ipv6())
            .map(|ip| ip.to_string())
            .unwrap_or_default(),
        ac_ip: text("wlanacip")
            .and_then(|value| valid_ip(&value))
            .map(|ip| ip.to_string())
            .unwrap_or_default(),
        ac_name: text("wlanacname").unwrap_or_default(),
    })
}

pub(super) fn portal_probe_from_html(html: &str, status_authenticated: bool) -> DormPortalProbe {
    let session = extract_portal_session_info(html);
    let authenticated = !has_login_portal_marker(html)
        && (has_online_portal_marker(html)
            || (status_authenticated && has_online_session_fields(&session)));
    DormPortalProbe {
        reachable: true,
        recognized: has_login_portal_marker(html) || has_online_portal_marker(html),
        state: if authenticated {
            PortalOnlineState::Online
        } else if has_login_portal_marker(html) && !status_authenticated {
            PortalOnlineState::Offline
        } else {
            PortalOnlineState::Unknown
        },
        authenticated,
        context: dorm_context(html, None),
        session: (authenticated && has_online_session_fields(&session)).then_some(session),
    }
}

pub(super) fn dorm_probe_from_responses(html: &str, status: Option<&Value>) -> DormPortalProbe {
    let result = status
        .and_then(|value| value.get("result"))
        .and_then(unsigned);
    let mut probe = portal_probe_from_html(html, result == Some(1));
    probe.context = dorm_context(html, status);
    if matches!(result, Some(0 | 1)) {
        probe.recognized = true;
        probe.state = if result == Some(0) {
            PortalOnlineState::Offline
        } else if has_login_portal_marker(html) {
            PortalOnlineState::Unknown
        } else {
            PortalOnlineState::Online
        };
        probe.authenticated = probe.state == PortalOnlineState::Online;
    }
    if !probe.authenticated {
        probe.session = None;
        return probe;
    }
    let mut session = probe.session.unwrap_or_default();
    let text = |key| gateway_text(status, key);
    session.campus_card_number = session.campus_card_number.or_else(|| text("uid"));
    session.online_username = session.online_username.or_else(|| text("NID"));
    session.login_ip = session.login_ip.or_else(|| text("lip"));
    session.login_started_at = session.login_started_at.or_else(|| text("stime"));
    session.login_expires_at = session.login_expires_at.or_else(|| text("etime"));
    session.online_flow = session
        .online_flow
        .or_else(|| status?.get("olflow").and_then(unsigned));
    session.online_duration = session
        .online_duration
        .or_else(|| status?.get("oltime").and_then(unsigned));
    probe.session = has_online_session_fields(&session).then_some(session);
    probe
}

pub(super) fn probe_dorm_portal(http: &dyn PortalHttp, now: u64) -> DormPortalProbe {
    let root = http.get(&portal_url(DORM_PORTAL_HOST, 80, "/"));
    let html = root
        .as_ref()
        .ok()
        .filter(|response| (200..300).contains(&response.status))
        .map(|response| decode_portal_html(&response.body, response.content_type.as_deref()))
        .unwrap_or_default();
    let callback = format!("dr{}", 1000 + now % 9000);
    let mut url = portal_url(DORM_PORTAL_HOST, 80, "/drcom/chkstatus");
    url.query_pairs_mut()
        .append_pair("callback", &callback)
        .append_pair("_", &now.to_string());
    let response = http.get(&url);
    let status = response
        .as_ref()
        .ok()
        .filter(|response| (200..300).contains(&response.status))
        .and_then(|response| {
            parse_jsonp_callback(&String::from_utf8_lossy(&response.body), &callback).ok()
        });
    let mut probe = dorm_probe_from_responses(&html, status.as_ref());
    probe.reachable = root.is_ok() || response.is_ok();
    if !probe.reachable {
        probe.state = PortalOnlineState::Unavailable;
    }
    probe
}

#[derive(Debug, Clone, Default)]
pub(super) struct NetworkObservation {
    pub(super) snapshot: NetworkSnapshot,
    pub(super) dorm: DormPortalProbe,
    pub(super) staff: StaffPortalProbe,
}
#[derive(Debug, Clone, Default)]
pub(super) struct RegionEvidence {
    staff: Option<(String, String)>,
}

pub(super) fn classify_network_overview(
    observation: &NetworkObservation,
    evidence: &mut RegionEvidence,
) -> NetworkOverviewView {
    let NetworkObservation {
        snapshot,
        dorm,
        staff,
    } = observation;
    let fingerprint = snapshot.fingerprint();
    let source_ip = staff
        .config
        .as_ref()
        .map(|config| &config.ip)
        .or_else(|| dorm.context.as_ref().map(|context| &context.ip));
    if evidence.staff.as_ref().is_some_and(|(network, ip)| {
        network != &fingerprint || source_ip.is_some_and(|current| current != ip)
    }) {
        evidence.staff = None;
    }
    if staff.state == PortalOnlineState::Online {
        if let Some(config) = &staff.config {
            evidence.staff = Some((fingerprint, config.ip.clone()));
        }
    }
    let region = if staff.state == PortalOnlineState::Online
        || evidence.staff.is_some()
        || snapshot.ssid.as_deref() == Some("SZU_WLAN")
    {
        NetworkRegion::Staff
    } else if dorm.recognized {
        NetworkRegion::Dormitory
    } else {
        NetworkRegion::None
    };
    let state = match region {
        NetworkRegion::Staff => match staff.state {
            PortalOnlineState::Online => NetworkConnectionState::Connected,
            PortalOnlineState::Offline => NetworkConnectionState::NotLoggedIn,
            _ => NetworkConnectionState::Unknown,
        },
        NetworkRegion::Dormitory => match dorm.state {
            PortalOnlineState::Online => NetworkConnectionState::Connected,
            PortalOnlineState::Offline => NetworkConnectionState::NotLoggedIn,
            _ => NetworkConnectionState::Unknown,
        },
        NetworkRegion::None
            if !snapshot.effective_connected() && !dorm.reachable && !staff.reachable =>
        {
            NetworkConnectionState::NoNetwork
        }
        NetworkRegion::None => NetworkConnectionState::Unknown,
    };
    NetworkOverviewView {
        connected: snapshot.effective_connected(),
        portal_session: (region == NetworkRegion::Dormitory
            && state == NetworkConnectionState::Connected)
            .then(|| dorm.session.clone())
            .flatten(),
        staff_session: (region == NetworkRegion::Staff
            && state == NetworkConnectionState::Connected)
            .then(|| staff.session.clone())
            .flatten(),
        region,
        state,
        ssid: snapshot.ssid.clone(),
        portal_reachable: dorm.reachable,
        staff_host_reachable: staff.reachable,
    }
}

pub(super) fn probe_network_overview(
    http: &dyn PortalHttp,
    snapshot: NetworkSnapshot,
    now: u64,
) -> NetworkObservation {
    // Always consult Srun even when Dr.COM is reachable or reports an online user.
    let staff = probe_staff_portal(http, now);
    let dorm = probe_dorm_portal(http, now);
    NetworkObservation {
        snapshot,
        staff,
        dorm,
    }
}
