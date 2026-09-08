use super::{
    http::{portal_url, CheckedHttp, PortalHttp, REQUEST_TIMEOUT},
    model::*,
    network::*,
    operations::LogoutGuard,
    portal::parse_jsonp_callback,
    settings::{disable_auto_connection, dorm_settings},
    staff::{probe_staff_portal, staff_url, StaffPortalConfig},
    state::NetworkEvents,
    CampusNetworkStore,
};
use reqwest::Url;
use sha1::{Digest, Sha1};
use std::time::Duration;

#[derive(Debug, Clone)]
pub(super) enum LogoutTarget {
    Dorm(DormPortalContext),
    Staff {
        config: StaffPortalConfig,
        username: String,
        domain: String,
    },
}
impl LogoutTarget {
    pub(super) fn from_observation(
        region: &NetworkRegion,
        observation: &NetworkObservation,
    ) -> Option<Self> {
        match region {
            NetworkRegion::Dormitory => Some(Self::Dorm(observation.dorm.context.clone()?)),
            NetworkRegion::Staff => Some(Self::Staff {
                config: observation.staff.config.clone()?,
                username: observation.staff.session.as_ref()?.username.clone(),
                domain: observation.staff.domain.clone(),
            }),
            NetworkRegion::None => None,
        }
    }
    pub(super) fn request(&self, now: u64) -> (Url, String) {
        let callback = match self {
            Self::Dorm(_) => format!("dr{}", 1000 + now % 9000),
            Self::Staff { .. } => format!("szuLifeLogout_{now}"),
        };
        let mut url = match self {
            Self::Dorm(context) => {
                let mut url = portal_url(DORM_PORTAL_HOST, 801, "/eportal/portal/logout");
                url.query_pairs_mut()
                    .append_pair("callback", &callback)
                    .append_pair("login_method", "1")
                    // These are the portal script's fixed placeholders, NOT user credentials.
                    .append_pair("user_account", "drcom")
                    .append_pair("user_password", "123")
                    .append_pair("ac_logout", "0")
                    .append_pair("register_mode", "1")
                    .append_pair("wlan_user_ip", &context.ip)
                    .append_pair("wlan_user_ipv6", &context.ipv6)
                    .append_pair("wlan_vlan_id", &context.vlan)
                    .append_pair("wlan_user_mac", &context.mac)
                    .append_pair("wlan_ac_ip", &context.ac_ip)
                    .append_pair("wlan_ac_name", &context.ac_name)
                    .append_pair("jsVersion", "4.1.3")
                    .append_pair("lang", "zh")
                    .append_pair("v", &(now % 10000 + 500).to_string());
                url
            }
            Self::Staff {
                config,
                username,
                domain,
            } if config.mac_auth => {
                let time = (now / 1000).to_string();
                let sign = format!(
                    "{:x}",
                    Sha1::digest(format!("{time}{username}{}1{time}", config.ip).as_bytes())
                );
                let mut url = staff_url("/cgi-bin/rad_user_dm");
                url.query_pairs_mut()
                    .append_pair("callback", &callback)
                    .append_pair("ip", &config.ip)
                    .append_pair("username", username)
                    .append_pair("time", &time)
                    .append_pair("unbind", "1")
                    .append_pair("sign", &sign);
                let _ = domain;
                url
            }
            Self::Staff {
                config,
                username,
                domain,
            } => {
                let account = if domain.is_empty() {
                    username.clone()
                } else {
                    format!("{username}@{}", domain.trim_start_matches('@'))
                };
                let mut url = staff_url("/cgi-bin/srun_portal");
                url.query_pairs_mut()
                    .append_pair("callback", &callback)
                    .append_pair("action", "logout")
                    .append_pair("username", &account)
                    .append_pair("ip", &config.ip)
                    .append_pair("ac_id", &config.acid);
                url
            }
        };
        url.query_pairs_mut().append_pair("_", &now.to_string());
        (url, callback)
    }
    pub(super) fn state(&self, observation: &NetworkObservation) -> PortalOnlineState {
        match self {
            Self::Dorm(_) => observation.dorm.state,
            Self::Staff { .. } => observation.staff.state,
        }
    }
    pub(super) fn same_identity(&self, observation: &NetworkObservation) -> Option<bool> {
        match self {
            Self::Dorm(target) => observation
                .dorm
                .context
                .as_ref()
                .map(|current| current.ip == target.ip),
            Self::Staff { config, .. } => observation
                .staff
                .config
                .as_ref()
                .map(|current| current.ip == config.ip && current.acid == config.acid),
        }
    }
    pub(super) fn refresh(
        &self,
        http: &dyn PortalHttp,
        observation: &mut NetworkObservation,
        now: u64,
    ) {
        match self {
            Self::Dorm(_) => observation.dorm = probe_dorm_portal(http, now),
            Self::Staff { .. } => observation.staff = probe_staff_portal(http, now),
        }
    }
}

fn result(
    outcome: NetworkLogoutOutcome,
    region: NetworkRegion,
    message: &str,
) -> NetworkLogoutResultView {
    NetworkLogoutResultView {
        outcome,
        region,
        message: format!("{message}；自动连接已关闭。"),
    }
}

fn network_changed(
    store: &CampusNetworkStore,
    guard: &LogoutGuard,
    environment: &dyn NetworkEnvironment,
    events: &dyn NetworkEvents,
    region: NetworkRegion,
) -> NetworkLogoutResultView {
    guard.commit(|| store.forget_region_evidence());
    store.apply_observation(
        guard.token(),
        &NetworkObservation {
            snapshot: environment.snapshot(),
            ..Default::default()
        },
        events,
    );
    result(
        NetworkLogoutOutcome::Unknown,
        region,
        "网络已变化，已停止本次操作，未转向其他网络注销",
    )
}

/// All IO is injectable. Tests never invoke a real logout or the system credential store.
pub(super) fn perform_network_logout(
    store: &CampusNetworkStore,
    http: &dyn PortalHttp,
    environment: &dyn NetworkEnvironment,
    events: &dyn NetworkEvents,
) -> Result<NetworkLogoutResultView, String> {
    let guard = store.operations.begin_logout()?;
    let disabled = guard
        .commit(|| {
            let saved = disable_auto_connection(store);
            let staff_saved = super::staff_login::settings::disable(store, events);
            store.change_dorm_status(
                |status| {
                    status.state = DormLoginState::Disabled;
                    status.next_probe_at_ms = None;
                    status.consecutive_failures = 0;
                },
                events,
            );
            if let Ok(settings) = dorm_settings(store) {
                events.settings_changed(&settings);
            }
            saved.and(staff_saved.map(|_| ())).map(|_| ())
        })
        .ok_or("注销操作已失效")?;
    disabled?;

    let original = environment.snapshot();
    let region = store
        .network_overview
        .lock()
        .map(|view| view.region.clone())
        .unwrap_or_default();
    // A request already admitted by the login worker must settle before logout is sent.
    if !guard.wait_for_login(REQUEST_TIMEOUT + Duration::from_secs(2)) {
        return Ok(result(
            NetworkLogoutOutcome::Unknown,
            region,
            "之前的登录请求尚未结束，未发送注销请求，请稍后重试",
        ));
    }
    let allowed = || guard.current() && environment.snapshot() == original;
    let checked = CheckedHttp {
        inner: http,
        allowed: &allowed,
    };
    if !allowed() {
        return Ok(network_changed(store, &guard, environment, events, region));
    }
    let mut observation = probe_network_overview(&checked, original.clone(), environment.now_ms());
    if !allowed() {
        return Ok(network_changed(store, &guard, environment, events, region));
    }
    let overview = store
        .apply_observation(guard.token(), &observation, events)
        .ok_or("注销操作已失效")?;
    let region = overview.region;
    if overview.state == NetworkConnectionState::NotLoggedIn {
        return Ok(result(
            NetworkLogoutOutcome::AlreadyOffline,
            region,
            "当前设备已处于未登录状态",
        ));
    }
    if overview.state != NetworkConnectionState::Connected {
        return Ok(result(
            NetworkLogoutOutcome::Unknown,
            region,
            "无法确认当前设备的在线会话，未发送注销请求",
        ));
    }
    let Some(target) = LogoutTarget::from_observation(&region, &observation) else {
        return Ok(result(
            NetworkLogoutOutcome::Unknown,
            region,
            "门户未提供完整终端信息，未发送注销请求",
        ));
    };
    let report = disconnect_target(
        &target,
        &checked,
        environment,
        &mut observation,
        &allowed,
        |observation| {
            store.apply_observation(guard.token(), observation, events);
        },
    );
    if report.changed {
        return Ok(network_changed(store, &guard, environment, events, region));
    }
    if report.state == PortalOnlineState::Offline {
        return Ok(result(
            NetworkLogoutOutcome::Disconnected,
            region,
            "已确认当前设备断开校园网",
        ));
    }
    let last_state = report.state;
    let acknowledged = report.acknowledged;
    Ok(if last_state == PortalOnlineState::Online {
        result(
            NetworkLogoutOutcome::Failed,
            region,
            if acknowledged {
                "门户已接受注销，但复查后设备仍在线"
            } else {
                "注销未完成，复查后设备仍在线"
            },
        )
    } else {
        result(
            NetworkLogoutOutcome::Unknown,
            region,
            "暂时无法确认是否已断开，请稍后检查网络状态",
        )
    })
}

pub(super) struct TargetLogoutReport {
    pub(super) acknowledged: bool,
    pub(super) state: PortalOnlineState,
    pub(super) changed: bool,
}
/// Used by explicit logout and mode application. Does not change preferences or acquire another operation lock.
pub(super) fn disconnect_target(
    target: &LogoutTarget,
    http: &dyn PortalHttp,
    environment: &dyn NetworkEnvironment,
    observation: &mut NetworkObservation,
    allowed: &dyn Fn() -> bool,
    mut observed: impl FnMut(&NetworkObservation),
) -> TargetLogoutReport {
    let mut report = TargetLogoutReport {
        acknowledged: false,
        state: PortalOnlineState::Unknown,
        changed: false,
    };
    if !allowed() {
        report.changed = true;
        return report;
    }
    let (url, callback) = target.request(environment.now_ms());
    report.acknowledged = http
        .get(&url)
        .ok()
        .filter(|response| (200..300).contains(&response.status))
        .and_then(|response| {
            parse_jsonp_callback(&String::from_utf8_lossy(&response.body), &callback).ok()
        })
        .is_some_and(|value| match target {
            LogoutTarget::Dorm(_) => value
                .get("result")
                .is_some_and(|v| v.as_u64() == Some(1) || v.as_str() == Some("ok")),
            LogoutTarget::Staff { .. } => value.get("error").and_then(|v| v.as_str()) == Some("ok"),
        });
    for _ in 0..3 {
        if !allowed() {
            report.changed = true;
            break;
        }
        environment.sleep(Duration::from_secs(1));
        if !allowed() {
            report.changed = true;
            break;
        }
        target.refresh(http, observation, environment.now_ms());
        if !allowed() || target.same_identity(observation) == Some(false) {
            report.changed = true;
            break;
        }
        observed(observation);
        report.state = if target.same_identity(observation) == Some(true) {
            target.state(observation)
        } else {
            PortalOnlineState::Unknown
        };
        if report.state == PortalOnlineState::Offline {
            break;
        }
    }
    report
}

#[cfg(test)]
mod tests;
