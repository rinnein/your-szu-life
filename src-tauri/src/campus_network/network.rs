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

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn command_text(command: &str, args: &[&str]) -> Option<String> {
    let mut process = Command::new(command);
    process.args(args);
    process
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "windows")]
fn windows_wifi_ssid() -> Option<String> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::WiFi::{
        wlan_interface_state_connected, wlan_intf_opcode_current_connection, WlanCloseHandle,
        WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanQueryInterface,
        WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO_LIST,
    };

    unsafe {
        let mut negotiated_version = 0;
        let mut client_handle = 0 as HANDLE;
        if WlanOpenHandle(2, std::ptr::null(), &mut negotiated_version, &mut client_handle) != 0 {
            return None;
        }

        let mut if_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(client_handle, std::ptr::null(), &mut if_list) != 0
            || if_list.is_null()
        {
            WlanCloseHandle(client_handle, std::ptr::null());
            return None;
        }

        let count = (*if_list).dwNumberOfItems as usize;
        let interfaces = std::slice::from_raw_parts((*if_list).InterfaceInfo.as_ptr(), count);
        let mut found_ssid = None;

        for info in interfaces {
            if info.isState == wlan_interface_state_connected {
                let mut data_size = 0;
                let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
                let mut opcode_type = 0;
                if WlanQueryInterface(
                    client_handle,
                    &info.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    std::ptr::null(),
                    &mut data_size,
                    &mut data,
                    &mut opcode_type,
                ) == 0
                    && !data.is_null()
                {
                    let conn_attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                    let ssid_len =
                        conn_attrs.wlanAssociationAttributes.dot11Ssid.uSSIDLength as usize;
                    if ssid_len > 0 && ssid_len <= 32 {
                        let ssid_bytes =
                            &conn_attrs.wlanAssociationAttributes.dot11Ssid.ucSSID[..ssid_len];
                        let ssid_str = String::from_utf8_lossy(ssid_bytes).trim().to_string();
                        if !ssid_str.is_empty() {
                            found_ssid = Some(ssid_str);
                        }
                    }
                    WlanFreeMemory(data);
                }
            }
            if found_ssid.is_some() {
                break;
            }
        }

        WlanFreeMemory(if_list as *const _);
        WlanCloseHandle(client_handle, std::ptr::null());
        found_ssid
    }
}

#[cfg(target_os = "windows")]
fn windows_local_ip() -> Option<String> {
    for target in &["172.30.255.42:80", "1.1.1.1:80", "223.5.5.5:80"] {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect(target).is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    let ip = addr.ip();
                    if let std::net::IpAddr::V4(ipv4) = ip {
                        if !ipv4.is_unspecified() && !ipv4.is_loopback() {
                            return Some(ipv4.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn is_private_172(value: &str) -> bool {
    value
        .trim()
        .parse::<std::net::Ipv4Addr>()
        .map_or(false, |ip| {
            let octets = ip.octets();
            octets[0] == 172 && (16..=31).contains(&octets[1])
        })
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
        let ssid = windows_wifi_ssid();
        let ip = windows_local_ip();
        let has_private_172 = ip.as_deref().map_or(false, is_private_172);
        let wifi_connected = ssid.is_some();
        return NetworkSnapshot {
            connected: ip.is_some() || ssid.is_some(),
            ssid: ssid.clone(),
            ip: ip.clone(),
            transport: if wifi_connected {
                Some("wifi".into())
            } else if ip.is_some() {
                Some("ethernet".into())
            } else {
                None
            },
            has_private_172_ipv4: has_private_172,
            wifi_connected,
            wifi_ip: if wifi_connected { ip } else { None },
            wifi_has_private_172_ipv4: wifi_connected && has_private_172,
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
