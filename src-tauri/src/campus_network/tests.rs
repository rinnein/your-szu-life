use super::{login::backoff_delay, logs::sanitized_log, model::*, network::*, portal::*};
use std::time::Duration;

#[test]
fn custom_network_settings_expose_saved_card_number_without_password() {
    use crate::{
        campus_network::{settings, staff_login, CampusNetworkStore},
        storage::{testing::MemoryCredentialStore, CredentialStore},
    };
    use std::sync::Arc;

    let credentials = Arc::new(MemoryCredentialStore::default());
    credentials
        .save_dorm_credentials(&DormCustomCredentials {
            campus_card_number: "001234".into(),
            password: "dorm-secret".into(),
        })
        .unwrap();
    credentials
        .save_staff_credentials(&DormCustomCredentials {
            campus_card_number: "654321".into(),
            password: "staff-secret".into(),
        })
        .unwrap();
    let store = CampusNetworkStore::new(credentials.clone());

    let dorm = settings::dorm_settings_view(&store, &DormNetworkSettings::default()).unwrap();
    assert_eq!(dorm.custom_campus_card_number.as_deref(), Some("001234"));
    assert!(dorm.custom_configured);
    assert!(!serde_json::to_string(&dorm)
        .unwrap()
        .contains("dorm-secret"));

    let staff =
        staff_login::settings::cached_view(&store, &StaffNetworkSettings::default()).unwrap();
    assert_eq!(staff.custom_campus_card_number.as_deref(), Some("654321"));
    assert!(staff.custom_configured);
    assert!(!serde_json::to_string(&staff)
        .unwrap()
        .contains("staff-secret"));

    let empty = CampusNetworkStore::new(Arc::new(MemoryCredentialStore::default()));
    let dorm_empty = settings::dorm_settings_view(&empty, &DormNetworkSettings::default()).unwrap();
    let staff_empty =
        staff_login::settings::cached_view(&empty, &StaffNetworkSettings::default()).unwrap();
    assert_eq!(dorm_empty.custom_campus_card_number, None);
    assert_eq!(staff_empty.custom_campus_card_number, None);
}

#[test]
fn parses_android_network_snapshot_and_keeps_connectivity_without_ssid() {
    let snapshot = super::android::parse_network_snapshot_json(
        r#"{"connected":true,"ssid":"SZU_WLAN","ip":"172.24.111.161","transport":"wifi","capabilitiesAvailable":false,"wifiPermission":"denied","locationEnabled":false}"#,
    )
    .expect("valid Android snapshot");
    assert_eq!(snapshot.connected, true);
    assert_eq!(snapshot.ssid.as_deref(), Some("SZU_WLAN"));
    assert_eq!(snapshot.ip.as_deref(), Some("172.24.111.161"));
    assert_eq!(snapshot.transport.as_deref(), Some("wifi"));
    assert_eq!(snapshot.capabilities_available, Some(false));
    assert_eq!(snapshot.wifi_permission.as_deref(), Some("denied"));
    assert_eq!(snapshot.location_enabled, Some(false));
    assert!(!snapshot.has_private_172_ipv4);
    assert!(!snapshot.wifi_connected);

    let campus = super::android::parse_network_snapshot_json(
        r#"{"connected":true,"ip":"10.0.0.2","transport":"cellular","wifiConnected":true,"wifiIp":"172.30.1.2","wifiHasPrivate172Ipv4":true}"#,
    )
    .expect("valid campus snapshot");
    assert!(campus.wifi_connected);
    assert_eq!(campus.wifi_ip.as_deref(), Some("172.30.1.2"));
    assert!(campus.wifi_has_private_172_ipv4);

    let wifi_only = super::android::parse_network_snapshot_json(
        r#"{"connected":false,"wifiConnected":true,"wifiIp":"192.168.1.10"}"#,
    )
    .expect("valid Wi-Fi-only snapshot");
    assert!(wifi_only.connected);
    assert!(wifi_only.effective_connected());
    assert!(wifi_only.wifi_connected);

    let wifi_ip_only =
        super::android::parse_network_snapshot_json(r#"{"connected":false,"wifiIp":"172.30.9.8"}"#)
            .expect("valid Wi-Fi IP snapshot");
    assert!(wifi_ip_only.connected);
    assert!(wifi_ip_only.effective_connected());

    let without_ssid = super::android::parse_network_snapshot_json(
        r#"{"connected":true,"ssid":"<unknown ssid>","ip":"172.24.111.161"}"#,
    )
    .expect("valid Android snapshot without SSID");
    assert!(without_ssid.connected);
    assert!(without_ssid.ssid.is_none());
    assert_eq!(without_ssid.ip.as_deref(), Some("172.24.111.161"));

    let cellular = super::android::parse_network_snapshot_json(
        r#"{"connected":true,"ip":"10.123.45.67","transport":"cellular","wifiConnected":false}"#,
    )
    .expect("valid cellular snapshot");
    assert_eq!(cellular.ssid.as_deref(), Some("移动数据"));
    assert!(cellular.is_cellular());
    assert!(cellular.effective_connected());

    let cellular_with_unknown_ssid = super::android::parse_network_snapshot_json(
        r#"{"connected":true,"ssid":"<unknown ssid>","ip":"10.123.45.67","transport":"cellular","wifiConnected":false}"#,
    )
    .expect("valid cellular snapshot with unknown ssid");
    assert_eq!(cellular_with_unknown_ssid.ssid.as_deref(), Some("移动数据"));
    assert!(cellular_with_unknown_ssid.is_cellular());
}

#[test]
fn rejects_invalid_android_snapshot_values_without_panicking() {
    assert!(super::android::parse_network_snapshot_json("not-json").is_none());
    let snapshot = super::android::parse_network_snapshot_json(
        r#"{"connected":false,"ssid":" ","ip":"not-an-ip"}"#,
    )
    .expect("valid snapshot envelope");
    assert!(!snapshot.connected);
    assert!(snapshot.ssid.is_none());
    assert!(snapshot.ip.is_none());
}

#[test]
fn extracts_terminal_ip_by_portal_priority_and_rejects_invalid_values() {
    let html = "<script>v4ip='000.000.000.000';v46ip='172.24.75.190';ss5='172.24.1.1';</script>";
    assert_eq!(extract_terminal_ip(html).as_deref(), Some("172.24.75.190"));
    assert_eq!(
        extract_terminal_ip("<script>v4ip='bad';v46ip='0.0.0.0';</script>"),
        None
    );
}

#[test]
fn parses_portal_jsonp_and_classifies_success_and_already_online() {
    let success = parse_jsonp_response("dr1003({\"result\":1,\"msg\":\"ok\"});").unwrap();
    assert_eq!(success["result"], 1);
    let online = parse_jsonp_response("dr1003({\"result\":0,\"ret_code\":2});").unwrap();
    assert_eq!(online["ret_code"], 2);
    assert!(parse_jsonp_response("not-jsonp").is_err());
}

#[test]
fn parses_portal_online_session_fields_and_gb2312_html() {
    let html = "<meta charset='gb2312'><script>NID='张三';uid='123456';oltime='4294967295';olflow=4294967295;lip='172.24.111.161';stime='2026-09-05 09:06:33';etime='2026-09-07 19:30:49';</script><!--Dr.COMWebLoginID_1.htm-->";
    let session = extract_portal_session_info(html);
    assert_eq!(session.online_username.as_deref(), Some("张三"));
    assert_eq!(session.campus_card_number.as_deref(), Some("123456"));
    assert_eq!(session.online_duration, Some(4_294_967_295));
    assert_eq!(session.online_flow, Some(4_294_967_295));
    assert_eq!(session.login_ip.as_deref(), Some("172.24.111.161"));
    assert!(has_online_portal_marker(html));
    assert!(!has_login_portal_marker(html));

    let (encoded, _, _) =
        encoding_rs::GB18030.encode("<meta charset='gb2312'><title>注销页</title>");
    let decoded = decode_portal_html(encoded.as_ref(), None);
    assert!(decoded.contains("注销页"));
}

#[test]
fn online_portal_session_drives_connected_state_without_cached_login_status() {
    let snapshot = NetworkSnapshot {
        connected: true,
        ssid: Some("SZU_CTC&CMCC".into()),
        ip: Some("172.24.111.161".into()),
        ..Default::default()
    };
    let session = DormPortalSessionInfo {
        online_username: Some("张三".into()),
        campus_card_number: Some("123456".into()),
        online_duration: Some(120),
        online_flow: Some(4096),
        login_ip: Some("172.24.111.161".into()),
        login_started_at: Some("2026-09-05 09:06:33".into()),
        login_expires_at: Some("2026-09-07 19:30:49".into()),
        charset: Some("gb2312".into()),
    };
    let overview = classify_network_overview(
        &NetworkObservation {
            snapshot,
            dorm: DormPortalProbe {
                reachable: true,
                recognized: true,
                authenticated: true,
                state: PortalOnlineState::Online,
                session: Some(session.clone()),
                ..Default::default()
            },
            ..Default::default()
        },
        &mut RegionEvidence::default(),
    );
    assert_eq!(overview.state, NetworkConnectionState::Connected);
    assert_eq!(overview.portal_session, Some(session));
}

#[test]
fn classifies_network_regions_and_connection_states() {
    use super::{
        staff::{apply_staff_info, parse_staff_config, StaffPortalProbe},
        testing::*,
    };
    let mut staff = StaffPortalProbe {
        reachable: true,
        config: parse_staff_config(&staff_html(TEST_IP, true)),
        ..Default::default()
    };
    apply_staff_info(&mut staff, &staff_online(TEST_IP));
    let mut observation = NetworkObservation {
        snapshot: snapshot(),
        dorm: dorm_probe_from_responses(&dorm_html(TEST_IP, true), None),
        staff,
    };
    let mut evidence = RegionEvidence::default();
    let overview = classify_network_overview(&observation, &mut evidence);
    assert_eq!(overview.region, NetworkRegion::Staff);
    assert_eq!(overview.state, NetworkConnectionState::Connected);
    assert!(overview.portal_session.is_none());
    assert_eq!(overview.staff_session.unwrap().username, "001234");

    apply_staff_info(
        &mut observation.staff,
        &serde_json::json!({"error":"not_online_error"}),
    );
    let offline = classify_network_overview(&observation, &mut evidence);
    assert_eq!(offline.region, NetworkRegion::Staff);
    assert_eq!(offline.state, NetworkConnectionState::NotLoggedIn);
    assert!(offline.staff_session.is_none());
    observation.staff.state = PortalOnlineState::Unknown;
    let unknown = classify_network_overview(&observation, &mut evidence);
    assert_eq!(unknown.region, NetworkRegion::Staff);
    assert_eq!(unknown.state, NetworkConnectionState::Unknown);

    // A new network invalidates the old staff proof, even if Dr.COM is also reachable.
    observation.snapshot.ssid = Some("new-network".into());
    let moved = classify_network_overview(&observation, &mut evidence);
    assert_eq!(moved.region, NetworkRegion::Dormitory);
    assert_eq!(moved.state, NetworkConnectionState::Connected);
    assert!(moved.staff_session.is_none());
}

#[test]
fn config_and_host_reachability_alone_never_override_dorm_or_create_staff_region() {
    use super::{
        staff::{parse_staff_config, StaffPortalProbe},
        testing::*,
    };
    let mut observation = NetworkObservation {
        snapshot: snapshot(),
        staff: StaffPortalProbe {
            reachable: true,
            state: PortalOnlineState::Offline,
            config: parse_staff_config(&staff_html(TEST_IP, true)),
            ..Default::default()
        },
        dorm: dorm_probe_from_responses(&dorm_html(TEST_IP, false), None),
    };
    let overview = classify_network_overview(&observation, &mut RegionEvidence::default());
    assert_eq!(overview.region, NetworkRegion::Dormitory);
    assert_eq!(overview.state, NetworkConnectionState::NotLoggedIn);
    observation.dorm = DormPortalProbe {
        reachable: true,
        ..Default::default()
    };
    let overview = classify_network_overview(&observation, &mut RegionEvidence::default());
    assert_eq!(overview.region, NetworkRegion::None);
    assert_eq!(overview.state, NetworkConnectionState::Unknown);
    observation.snapshot.ssid = Some("SZU_WLAN".into());
    let overview = classify_network_overview(&observation, &mut RegionEvidence::default());
    assert_eq!(overview.region, NetworkRegion::Staff);
    assert_eq!(overview.state, NetworkConnectionState::NotLoggedIn);
    let connected_without_portal = classify_network_overview(
        &NetworkObservation {
            snapshot: NetworkSnapshot {
                connected: true,
                ssid: None,
                ip: Some("192.0.2.10".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        &mut RegionEvidence::default(),
    );
    assert_eq!(connected_without_portal.region, NetworkRegion::None);
    assert_eq!(
        connected_without_portal.state,
        NetworkConnectionState::Unknown
    );

    let wifi_without_portal = classify_network_overview(
        &NetworkObservation {
            snapshot: NetworkSnapshot {
                connected: false,
                wifi_connected: true,
                wifi_ip: Some("172.30.10.2".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        &mut RegionEvidence::default(),
    );
    assert!(wifi_without_portal.connected);
    assert_eq!(wifi_without_portal.state, NetworkConnectionState::Unknown);

    let cellular_overview = classify_network_overview(
        &NetworkObservation {
            snapshot: NetworkSnapshot {
                connected: true,
                ssid: Some("移动数据".into()),
                transport: Some("cellular".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        &mut RegionEvidence::default(),
    );
    assert_eq!(cellular_overview.region, NetworkRegion::None);
    assert_eq!(cellular_overview.state, NetworkConnectionState::Connected);
    assert_eq!(cellular_overview.ssid.as_deref(), Some("移动数据"));
    assert!(!cellular_overview.portal_reachable);
    assert!(!cellular_overview.staff_host_reachable);

    let offline = classify_network_overview(
        &NetworkObservation::default(),
        &mut RegionEvidence::default(),
    );
    assert_eq!(offline.region, NetworkRegion::None);
    assert_eq!(offline.state, NetworkConnectionState::NoNetwork);
}

#[test]
fn changed_portal_source_ip_invalidates_staff_proof_even_when_local_fingerprint_is_unchanged() {
    use super::{
        staff::{apply_staff_info, parse_staff_config, StaffPortalProbe},
        testing::*,
    };
    let mut observation = NetworkObservation {
        snapshot: snapshot(),
        staff: StaffPortalProbe {
            config: parse_staff_config(&staff_html(TEST_IP, true)),
            ..Default::default()
        },
        dorm: dorm_probe_from_responses(&dorm_html(TEST_IP, true), None),
    };
    apply_staff_info(&mut observation.staff, &staff_online(TEST_IP));
    let mut evidence = RegionEvidence::default();
    assert_eq!(
        classify_network_overview(&observation, &mut evidence).region,
        NetworkRegion::Staff
    );
    observation.staff.config.as_mut().unwrap().ip = "172.28.0.9".into();
    observation.staff.state = PortalOnlineState::Offline;
    observation.staff.session = None;
    assert_eq!(
        classify_network_overview(&observation, &mut evidence).region,
        NetworkRegion::Dormitory
    );
}

#[test]
fn unrecognized_http_page_is_not_a_dorm_portal_and_explicit_offline_clears_identity() {
    assert!(!dorm_probe_from_responses("<html>hello</html>", None).recognized);
    let probe = dorm_probe_from_responses(
        include_str!("fixtures/dorm-online.html"),
        Some(&serde_json::json!({"result":0})),
    );
    assert_eq!(probe.state, PortalOnlineState::Offline);
    assert!(!probe.authenticated);
    assert!(probe.session.is_none());
}

#[test]
fn backoff_has_the_requested_boundaries() {
    assert_eq!(backoff_delay(1), Duration::from_secs(10));
    assert_eq!(backoff_delay(5), Duration::from_secs(10));
    assert_eq!(backoff_delay(6), Duration::from_secs(120));
    assert_eq!(backoff_delay(10), Duration::from_secs(120));
    assert_eq!(backoff_delay(11), Duration::from_secs(600));
}

#[test]
fn response_bodies_never_enter_logs_even_when_they_contain_identity_or_signed_urls() {
    let log = DormLoginLogView {
        stage: "logout".into(),
        outcome: "disconnected".into(),
        result: Some(1),
        message: Some("已确认断开".into()),
        response: Some(serde_json::json!({
            "user_name":"001234", "user_password":"test-only-secret", "user_mac":"0000-0000-0001",
            "nested":{"cookie":"private", "url":"https://net.szu.edu.cn/?sign=test-only-sign"}
        })),
        ..Default::default()
    };
    let clean = sanitized_log(log);
    assert!(clean.response.is_none());
    assert_eq!(clean.result, Some(1));
    let wire = serde_json::to_string(&clean).unwrap();
    for secret in [
        "001234",
        "test-only-secret",
        "0000-0000-0001",
        "test-only-sign",
        "private",
    ] {
        assert!(!wire.contains(secret));
    }
}

#[test]
fn dorm_settings_default_to_disabled_current_account() {
    let settings = DormNetworkSettings::default();
    assert!(!settings.enabled);
    assert_eq!(settings.credential_mode, DormCredentialMode::CurrentAccount);
    assert!(settings.pending_failure_notice.is_none());
}

#[test]
fn credential_modes_are_independent_and_do_not_require_auth_for_custom_credentials() {
    use super::{settings::resolve_credentials, CampusNetworkStore};
    use crate::storage::{testing::MemoryCredentialStore, CredentialStore};
    use std::sync::Arc;
    let credentials = Arc::new(MemoryCredentialStore::default());
    credentials
        .save_dorm_credentials(&DormCustomCredentials {
            campus_card_number: "654321".into(),
            password: "custom-test-only".into(),
        })
        .unwrap();
    let store = CampusNetworkStore::new(credentials);
    let settings = DormNetworkSettings {
        credential_mode: DormCredentialMode::Custom,
        ..Default::default()
    };
    assert_eq!(
        resolve_credentials(&store, &settings, || panic!(
            "custom mode must never read auth state"
        ))
        .unwrap(),
        ("654321".into(), "custom-test-only".into())
    );
    let settings = DormNetworkSettings::default();
    assert_eq!(
        resolve_credentials(&store, &settings, || Ok((
            "001234".into(),
            "account-test-only".into()
        )))
        .unwrap(),
        ("001234".into(), "account-test-only".into())
    );
    assert!(resolve_credentials(&store, &settings, || Err("no current account".into())).is_err());
    assert!(resolve_credentials(&store, &settings, || Ok(("123".into(), "bad".into()))).is_err());
}

#[test]
fn snapshot_connection_is_published_before_portal_probe_finishes() {
    use super::{state::NetworkEvents, CampusNetworkStore};
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Capture(Mutex<Option<NetworkOverviewView>>);
    impl NetworkEvents for Capture {
        fn overview_changed(&self, overview: &NetworkOverviewView) {
            *self.0.lock().unwrap() = Some(overview.clone());
        }
    }

    let store = CampusNetworkStore::new(Arc::new(MemoryCredentialStore::default()));
    let events = Capture::default();
    store.apply_snapshot_connection(
        &NetworkSnapshot {
            connected: false,
            wifi_connected: true,
            wifi_ip: Some("172.30.10.2".into()),
            ssid: None,
            ..Default::default()
        },
        &events,
    );
    let overview = events.0.lock().unwrap().clone().expect("connection event");
    assert!(overview.connected);
    assert_eq!(overview.state, NetworkConnectionState::Unknown);
    assert_eq!(overview.region, NetworkRegion::None);

    store.apply_snapshot_connection(
        &NetworkSnapshot {
            connected: true,
            transport: Some("cellular".into()),
            ssid: Some("移动数据".into()),
            wifi_connected: false,
            ..Default::default()
        },
        &events,
    );
    let overview = events.0.lock().unwrap().clone().expect("cellular connection event");
    assert!(overview.connected);
    assert_eq!(overview.state, NetworkConnectionState::Connected);
    assert_eq!(overview.region, NetworkRegion::None);
    assert_eq!(overview.ssid.as_deref(), Some("移动数据"));
    assert!(!overview.portal_reachable);
    assert!(!overview.staff_host_reachable);
}

#[test]
fn probe_network_overview_skips_http_when_cellular() {
    struct PanicHttp;
    impl super::http::PortalHttp for PanicHttp {
        fn get(&self, _url: &reqwest::Url) -> Result<super::http::HttpResponse, super::http::HttpError> {
            panic!("cellular snapshot must never trigger HTTP request");
        }
    }
    let snapshot = NetworkSnapshot {
        connected: true,
        transport: Some("cellular".into()),
        ssid: Some("移动数据".into()),
        wifi_connected: false,
        ..Default::default()
    };
    let observation = super::network::probe_network_overview(&PanicHttp, snapshot, 1000);
    assert!(observation.snapshot.is_cellular());
    assert!(!observation.dorm.reachable);
    assert!(!observation.staff.reachable);
}

#[test]
fn monitor_probe_requests_are_coalesced_and_cleared_when_finished() {
    use super::CampusNetworkStore;
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;
    let store = CampusNetworkStore::new(Arc::new(MemoryCredentialStore::default()));
    let (_generation, receiver) = store.reserve_monitor().expect("monitor reservation");
    assert!(store.request_probe().expect("queue first probe"));
    assert!(!store.request_probe().expect("coalesce second probe"));
    assert!(matches!(
        receiver.recv_timeout(Duration::from_millis(10)),
        Ok(super::monitor::MonitorCommand::ProbeNow)
    ));
    store.probe_finished();
    assert!(store.request_probe().expect("queue probe after completion"));
}

#[test]
fn monitor_restart_budget_is_bounded_per_minute() {
    use super::CampusNetworkStore;
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;
    let store = CampusNetworkStore::new(Arc::new(MemoryCredentialStore::default()));
    assert!(store.reserve_monitor_restart());
    assert!(store.reserve_monitor_restart());
    assert!(store.reserve_monitor_restart());
    assert!(!store.reserve_monitor_restart());
}

#[test]
fn stale_monitor_generation_cannot_clear_a_restarted_monitor() {
    use super::CampusNetworkStore;
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;
    let store = CampusNetworkStore::new(Arc::new(MemoryCredentialStore::default()));
    let (first_generation, _first_receiver) = store.reserve_monitor().expect("first monitor");
    store.finish_monitor(first_generation);
    let (second_generation, _second_receiver) = store.reserve_monitor().expect("second monitor");
    store.finish_monitor(first_generation);
    assert!(store.monitor_running());
    store.finish_monitor(second_generation);
    assert!(!store.monitor_running());
}

#[test]
fn monitor_is_reserved_once_and_login_gate_is_released_on_drop() {
    use super::CampusNetworkStore;
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;
    let store = CampusNetworkStore::new(Arc::new(MemoryCredentialStore::default()));
    let (_generation, receiver) = store.reserve_monitor().expect("first monitor reservation");
    assert!(store.reserve_monitor().is_none());
    let login = store.begin_login().unwrap();
    assert!(store.login_in_flight());
    assert!(store.begin_login().is_err());
    drop(login);
    assert!(!store.login_in_flight());
    let login = store.begin_login().unwrap();
    drop(login);
    drop(store);
    assert!(matches!(
        receiver.recv_timeout(Duration::from_millis(10)),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn result_policy_keeps_success_already_online_and_business_failure_distinct() {
    use super::login::{apply_login_status, login_result_from_response};
    for (response, expected_ok, already_online) in [
        (serde_json::json!({"result":1,"msg":"ok"}), true, false),
        (serde_json::json!({"result":0,"ret_code":2}), true, true),
        (
            serde_json::json!({"result":0,"ret_code":1,"msg":"denied"}),
            false,
            false,
        ),
    ] {
        let result = login_result_from_response(&response, 200);
        assert_eq!(result.ok, expected_ok);
        assert_eq!(result.already_online, already_online);
        let mut status = DormLoginStatusView {
            consecutive_failures: 7,
            ..Default::default()
        };
        apply_login_status(&mut status, &result);
        assert_eq!(
            status.state,
            if expected_ok {
                DormLoginState::Online
            } else {
                DormLoginState::Paused
            }
        );
        assert_eq!(status.last_result, Some(result));
        if expected_ok {
            assert_eq!(status.next_probe_at_ms, None);
            assert_eq!(status.consecutive_failures, 0);
        }
    }
}

#[test]
fn transport_failures_back_off_and_paused_or_busy_logins_are_not_retried() {
    use super::{login::apply_transport_failure, monitor::login_due};
    let mut status = DormLoginStatusView {
        consecutive_failures: 5,
        ..Default::default()
    };
    assert_eq!(
        apply_transport_failure(&mut status, false, 1000),
        (6, 121000)
    );
    assert_eq!(status.state, DormLoginState::Monitoring);
    assert!(!login_due(&status, 120000, false));
    assert!(login_due(&status, 121000, false));
    assert!(!login_due(&status, 121000, true));
    status.state = DormLoginState::Paused;
    assert!(!login_due(&status, 999999, false));
    status.consecutive_failures = 10;
    assert_eq!(
        apply_transport_failure(&mut status, true, 1000),
        (11, 601000)
    );
    assert_eq!(status.state, DormLoginState::Monitoring);
}
