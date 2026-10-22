use super::{
    http::{portal_url, PortalHttp},
    model::*,
    portal::*,
    staff::{probe_staff_portal, unsigned, valid_ip, StaffPortalProbe},
};
use serde_json::Value;
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

fn is_private_172(value: &str) -> bool {
    value
        .trim()
        .parse::<std::net::Ipv4Addr>()
        .map_or(false, |ip| {
            let octets = ip.octets();
            octets[0] == 172 && (16..=31).contains(&octets[1])
        })
}

pub(crate) fn is_staff_ssid(ssid: &str) -> bool {
    ssid.trim().trim_matches('"') == "SZU_WLAN"
}

pub(crate) fn is_dorm_ssid(ssid: &str) -> bool {
    ssid.trim().trim_matches('"') == "SZU_CTC&CMCC"
}

#[cfg(not(target_os = "android"))]
pub(super) fn network_snapshot() -> NetworkSnapshot {
    use netdev::prelude::*;

    let raw_ssid = ssid::get_ssid()
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty());

    let interfaces = get_interfaces();
    let default_iface = get_default_interface().ok();

    let mut primary_ip = None;
    let mut transport = None;
    let mut wifi_connected = false;
    let mut wifi_ip = None;
    let mut is_ethernet = false;

    if let Some(def) = &default_iface {
        if let Some(net) = def.ipv4.iter().find(|n| {
            !n.addr().is_loopback() && !n.addr().is_unspecified() && !n.addr().is_link_local()
        }) {
            primary_ip = Some(net.addr().to_string());
        }
        match def.if_type {
            InterfaceType::Wireless80211 => {
                transport = Some("wifi".to_string());
                wifi_connected = true;
                wifi_ip = primary_ip.clone();
            }
            InterfaceType::Ethernet | InterfaceType::GigabitEthernet => {
                transport = Some("ethernet".to_string());
                is_ethernet = true;
            }
            InterfaceType::Wwan | InterfaceType::Wwanpp => {
                transport = Some("cellular".to_string());
            }
            InterfaceType::Tunnel | InterfaceType::Ppp => {
                transport = Some("vpn".to_string());
            }
            _ => {
                let lower_name = def.name.to_ascii_lowercase();
                if lower_name.starts_with("wl") || lower_name.contains("wifi") {
                    transport = Some("wifi".to_string());
                    wifi_connected = true;
                    wifi_ip = primary_ip.clone();
                } else if lower_name.starts_with("eth") || lower_name.starts_with("en") {
                    transport = Some("ethernet".to_string());
                    is_ethernet = true;
                } else {
                    transport = Some("other".to_string());
                }
            }
        }
    }

    for iface in &interfaces {
        if iface.if_type == InterfaceType::Loopback {
            continue;
        }
        let is_up = iface.oper_state == OperState::Up || !iface.ipv4.is_empty();
        if !is_up {
            continue;
        }

        let first_v4 = iface
            .ipv4
            .iter()
            .find(|n| {
                !n.addr().is_loopback()
                    && !n.addr().is_unspecified()
                    && !n.addr().is_link_local()
            })
            .map(|n| n.addr().to_string());

        let lower_name = iface.name.to_ascii_lowercase();
        let is_wifi_iface = iface.if_type == InterfaceType::Wireless80211
            || lower_name.starts_with("wl")
            || lower_name.contains("wifi");

        let is_eth_iface = matches!(
            iface.if_type,
            InterfaceType::Ethernet | InterfaceType::GigabitEthernet
        ) || lower_name.starts_with("eth")
            || lower_name.starts_with("en");

        if is_wifi_iface {
            if first_v4.is_some() || raw_ssid.is_some() {
                wifi_connected = true;
                if wifi_ip.is_none() {
                    wifi_ip = first_v4.clone();
                }
            }
        }

        if is_eth_iface && first_v4.is_some() {
            is_ethernet = true;
        }

        if primary_ip.is_none() && first_v4.is_some() {
            primary_ip = first_v4;
            if is_wifi_iface {
                transport = Some("wifi".to_string());
            } else if is_eth_iface {
                transport = Some("ethernet".to_string());
            }
        }
    }

    if raw_ssid.is_some() {
        wifi_connected = true;
        if transport.is_none() {
            transport = Some("wifi".to_string());
        }
    }

    // UDP routing lookup fallback if interface IP not resolved
    if primary_ip.is_none() {
        for target in &["172.30.255.42:80", "1.1.1.1:80", "223.5.5.5:80"] {
            if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
                if socket.connect(target).is_ok() {
                    if let Ok(addr) = socket.local_addr() {
                        let ip = addr.ip();
                        if let std::net::IpAddr::V4(ipv4) = ip {
                            if !ipv4.is_unspecified() && !ipv4.is_loopback() {
                                primary_ip = Some(ipv4.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let has_private_172 = primary_ip.as_deref().map_or(false, is_private_172);
    let wifi_has_private_172 = wifi_ip.as_deref().map_or(false, is_private_172);

    // If cellular network, set SSID to "移动数据"
    // If wired network, set SSID to "有线连接"
    let ssid = if raw_ssid.is_some() {
        raw_ssid
    } else if transport.as_deref() == Some("cellular") && !wifi_connected {
        Some("移动数据".to_string())
    } else if transport.as_deref() == Some("ethernet") || (is_ethernet && !wifi_connected) {
        Some("有线连接".to_string())
    } else {
        None
    };

    let connected = primary_ip.is_some() || ssid.is_some() || wifi_connected;

    NetworkSnapshot {
        connected,
        ssid,
        ip: primary_ip,
        transport,
        has_private_172_ipv4: has_private_172,
        wifi_connected,
        wifi_ip,
        wifi_has_private_172_ipv4: wifi_has_private_172,
        ..Default::default()
    }
}

#[cfg(target_os = "android")]
pub(super) fn network_snapshot() -> NetworkSnapshot {
    super::android::network_snapshot()
}

impl NetworkSnapshot {
    pub(crate) fn is_cellular(&self) -> bool {
        (self.transport.as_deref() == Some("cellular") || self.ssid.as_deref() == Some("移动数据"))
            && !self.wifi_connected
    }

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
    if snapshot.is_cellular() {
        return NetworkOverviewView {
            connected: true,
            portal_session: None,
            staff_session: None,
            region: NetworkRegion::None,
            state: NetworkConnectionState::Connected,
            ssid: snapshot.ssid.clone(),
            portal_reachable: false,
            staff_host_reachable: false,
        };
    }
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
        || snapshot.ssid.as_deref().map_or(false, is_staff_ssid)
    {
        NetworkRegion::Staff
    } else if dorm.recognized || snapshot.ssid.as_deref().map_or(false, is_dorm_ssid) {
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
    if snapshot.is_cellular() {
        return NetworkObservation {
            snapshot,
            staff: StaffPortalProbe::default(),
            dorm: DormPortalProbe::default(),
        };
    }
    let (staff, dorm) = match snapshot.ssid.as_deref() {
        Some(ssid) if is_staff_ssid(ssid) => {
            // Fast pre-judgment for Staff area Wi-Fi: probe staff portal first
            let staff = probe_staff_portal(http, now);
            let dorm = if staff.state == PortalOnlineState::Online {
                DormPortalProbe::default()
            } else {
                probe_dorm_portal(http, now)
            };
            (staff, dorm)
        }
        Some(ssid) if is_dorm_ssid(ssid) => {
            // Fast pre-judgment for Dormitory Wi-Fi: probe dorm portal first
            let dorm = probe_dorm_portal(http, now);
            let staff = if dorm.recognized {
                StaffPortalProbe::default()
            } else {
                probe_staff_portal(http, now)
            };
            (staff, dorm)
        }
        _ => {
            // Non-designated SSID, external Wi-Fi, or wired: probe both portals via HTTP
            let staff = probe_staff_portal(http, now);
            let dorm = probe_dorm_portal(http, now);
            (staff, dorm)
        }
    };
    NetworkObservation {
        snapshot,
        staff,
        dorm,
    }
}
