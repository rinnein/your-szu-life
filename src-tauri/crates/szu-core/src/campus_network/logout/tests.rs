use super::*;
use crate::{
    campus_network::{
        http::{HttpError, HttpResponse},
        staff::{apply_staff_info, parse_staff_config, StaffPortalProbe},
        testing::*,
    },
    storage::{testing::MemoryCredentialStore, CredentialStore},
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};

struct Scenario {
    staff: bool,
    online: AtomicBool,
    reject: AtomicBool,
    unbind_reject: AtomicBool,
    uncertain: AtomicBool,
    change_source: AtomicBool,
    change_before_logout: AtomicBool,
    placeholder_mac: AtomicBool,
    config_fail: AtomicBool,
    logouts: AtomicUsize,
    unbinds: AtomicUsize,
    portal_config: Mutex<DormPortalConfig>,
    requests: Mutex<Vec<Url>>,
    environment: Arc<FakeEnvironment>,
    credentials: Arc<MemoryCredentialStore>,
}
impl PortalHttp for Scenario {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError> {
        assert!(
            !self
                .credentials
                .load_dorm_settings()
                .unwrap()
                .unwrap()
                .enabled,
            "auto-connect must be persisted OFF before any HTTP request"
        );
        self.requests.lock().unwrap().push(url.clone());
        assert_ne!(
            url.path(),
            "/eportal/portal/login",
            "logout may never submit login"
        );
        let after =
            self.logouts.load(Ordering::Relaxed) + self.unbinds.load(Ordering::Relaxed) > 0;
        if after && self.uncertain.load(Ordering::Relaxed) {
            return Err(HttpError::Timeout);
        }
        let ip = if after && self.change_source.load(Ordering::Relaxed) {
            "172.28.0.9"
        } else {
            TEST_IP
        };
        let online = self.online.load(Ordering::Relaxed);
        let mac = if self.placeholder_mac.load(Ordering::Relaxed) {
            "000000000000"
        } else {
            "000000000001"
        };
        Ok(match (url.host_str().unwrap(), url.path()) {
            ("net.szu.edu.cn", "/") => html(&staff_html(ip, true)),
            ("net.szu.edu.cn", "/cgi-bin/rad_user_info") => jsonp(
                url,
                if self.staff && online {
                    staff_online(ip)
                } else {
                    serde_json::json!({"error":"not_online_error"})
                },
            ),
            ("172.30.255.42", "/") => {
                html(&dorm_html_with_mac(TEST_IP, self.staff || online, mac))
            }
            ("172.30.255.42", "/drcom/chkstatus") => {
                if self.change_before_logout.load(Ordering::Relaxed) && !after {
                    self.environment.current.lock().unwrap().ssid =
                        Some("different-network".into());
                }
                jsonp(
                    url,
                    serde_json::json!({
                        "result": if self.staff || online { 1 } else { 0 },
                        "v4ip": TEST_IP,
                        "uid": "001234",
                        "olmac": mac,
                        "ss4": "000000000000",
                        "v6ip": "::",
                        "vlanid": "1",
                    }),
                )
            }
            ("172.30.255.42", "/eportal/portal/page/loadConfig") => {
                if self.config_fail.load(Ordering::Relaxed) {
                    return Err(HttpError::Timeout);
                }
                let config = *self.portal_config.lock().unwrap();
                jsonp(
                    url,
                    serde_json::json!({
                        "code": 1,
                        "data": {
                            "login_method": config.login_method.to_string(),
                            "un_bind_mac": if config.un_bind_mac { "1" } else { "0" },
                            "register_mode": config.register_mode.to_string(),
                            "ac_logout": config.ac_logout.to_string(),
                            "isp_unbind_suffix": if config.isp_unbind_suffix { "1" } else { "0" },
                        }
                    }),
                )
            }
            ("172.30.255.42", "/eportal/portal/mac/unbind") => {
                self.unbinds.fetch_add(1, Ordering::Relaxed);
                if self.unbind_reject.load(Ordering::Relaxed) {
                    jsonp(url, serde_json::json!({"result":0,"msg":"解绑失败"}))
                } else {
                    self.online.store(false, Ordering::Relaxed);
                    jsonp(url, serde_json::json!({"result":1,"msg":"解绑成功"}))
                }
            }
            ("net.szu.edu.cn", "/cgi-bin/rad_user_dm")
            | ("172.30.255.42", "/eportal/portal/logout")
            | ("172.30.255.42", "/drcom/logout") => {
                assert_eq!(url.host_str() == Some("net.szu.edu.cn"), self.staff);
                self.logouts.fetch_add(1, Ordering::Relaxed);
                if self.reject.load(Ordering::Relaxed) {
                    jsonp(url, serde_json::json!({"error":"reject","result":0}))
                } else {
                    self.online.store(false, Ordering::Relaxed);
                    jsonp(url, serde_json::json!({"error":"ok","result":1}))
                }
            }
            _ => panic!("unexpected mock endpoint"),
        })
    }
}
fn setup(
    staff: bool,
) -> (
    Arc<CampusNetworkStore>,
    Arc<MemoryCredentialStore>,
    Arc<Scenario>,
) {
    let credentials = Arc::new(MemoryCredentialStore::default());
    credentials
        .save_dorm_settings(&DormNetworkSettings {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let store = Arc::new(CampusNetworkStore::new(credentials.clone()));
    let scenario = Arc::new(Scenario {
        staff,
        online: AtomicBool::new(true),
        reject: AtomicBool::new(false),
        unbind_reject: AtomicBool::new(false),
        uncertain: AtomicBool::new(false),
        change_source: AtomicBool::new(false),
        change_before_logout: AtomicBool::new(false),
        placeholder_mac: AtomicBool::new(false),
        config_fail: AtomicBool::new(false),
        logouts: AtomicUsize::new(0),
        unbinds: AtomicUsize::new(0),
        portal_config: Mutex::new(DormPortalConfig {
            login_method: 1,
            un_bind_mac: true,
            register_mode: 1,
            ac_logout: 0,
            isp_unbind_suffix: false,
        }),
        requests: Mutex::new(Vec::new()),
        environment: Arc::new(FakeEnvironment::default()),
        credentials: credentials.clone(),
    });
    (store, credentials, scenario)
}

#[test]
fn request_builders_match_both_staff_modes_and_dorm_placeholders() {
    let config = parse_staff_config(&staff_html(TEST_IP, true)).unwrap();
    let (url, _) = LogoutTarget::Staff {
        config: config.clone(),
        username: "001234".into(),
        domain: "ignored-by-dm".into(),
    }
    .request(1788828000123, None);
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(url.path(), "/cgi-bin/rad_user_dm");
    assert_eq!(query["username"], "001234");
    assert_eq!(query["ip"], TEST_IP);
    assert_eq!(query["time"], "1788828000");
    assert_eq!(query["unbind"], "1");
    assert_eq!(query["sign"], "fc59e3bc81315d3b96bcca82685e46923289a6b3");
    let (normal, _) = LogoutTarget::Staff {
        config: StaffPortalConfig {
            mac_auth: false,
            acid: "7".into(),
            ..config
        },
        username: "001234".into(),
        domain: "campus".into(),
    }
    .request(1, None);
    let query: std::collections::HashMap<_, _> = normal.query_pairs().into_owned().collect();
    assert_eq!(normal.path(), "/cgi-bin/srun_portal");
    assert_eq!(query["username"], "001234@campus");
    assert_eq!(query["ac_id"], "7");
    assert_eq!(query["action"], "logout");
    assert!(!query.contains_key("password"));
    let portal_config = DormPortalConfig {
        login_method: 1,
        un_bind_mac: true,
        register_mode: 1,
        ac_logout: 0,
        isp_unbind_suffix: false,
    };
    let context = DormPortalContext {
        ip: TEST_IP.into(),
        mac: "000000000001".into(),
        vlan: "18".into(),
        ipv6: "::".into(),
        account: "001234".into(),
        ..Default::default()
    };
    let (dorm, _) = LogoutTarget::Dorm(context.clone()).request(1, Some(&portal_config));
    let query: std::collections::HashMap<_, _> = dorm.query_pairs().into_owned().collect();
    assert_eq!(dorm.port(), Some(801));
    assert_eq!(dorm.query_pairs().next().unwrap().0, "callback");
    assert_eq!(query["callback"], "dr1001");
    assert_eq!(query["user_account"], "drcom");
    assert_eq!(query["user_password"], "123");
    assert_eq!(query["register_mode"], "1");
    assert_eq!(query["wlan_user_ip"], TEST_IP);
    assert_eq!(query["wlan_user_ipv6"], "");
    assert_eq!(query["wlan_vlan_id"], "18");
    assert_eq!(query["wlan_user_mac"], "000000000001");

    let unbind_context = DormPortalContext {
        mac: "3ee1f57672e6".into(),
        ..context.clone()
    };
    let (unbind, unbind_callback) = dorm_unbind_request(&unbind_context, &portal_config, 1).unwrap();
    let unbind_query: std::collections::HashMap<_, _> =
        unbind.query_pairs().into_owned().collect();
    assert_eq!(unbind.path(), "/eportal/portal/mac/unbind");
    assert_eq!(unbind.port(), Some(801));
    assert_eq!(unbind_callback, "dr1001");
    assert_eq!(unbind_query["callback"], "dr1001");
    assert_eq!(unbind_query["user_account"], "001234");
    assert_eq!(unbind_query["wlan_user_mac"], "3EE1F57672E6");
    assert_eq!(unbind_query["wlan_user_ip"], "2887516168");
    assert!(!unbind_query.contains_key("user_password"));
    assert!(!unbind_query.contains_key("login_method"));

    let placeholder_account = DormPortalContext {
        account: "12345678901234567890123456".into(),
        ..unbind_context.clone()
    };
    assert!(dorm_unbind_request(&placeholder_account, &portal_config, 1).is_none());
    let empty_account = DormPortalContext {
        account: String::new(),
        ..unbind_context.clone()
    };
    assert!(dorm_unbind_request(&empty_account, &portal_config, 1).is_none());
    let suffix_config = DormPortalConfig {
        isp_unbind_suffix: true,
        ..portal_config
    };
    let suffix_context = DormPortalContext {
        account: "001234@dx".into(),
        ..unbind_context
    };
    let (suffix, _) = dorm_unbind_request(&suffix_context, &suffix_config, 1).unwrap();
    assert_eq!(
        suffix
            .query_pairs()
            .find(|(key, _)| key == "user_account")
            .map(|(_, value)| value.into_owned()),
        Some("001234".into())
    );

    let (local, _) = LogoutTarget::Dorm(context).request(
        1,
        Some(&DormPortalConfig {
            login_method: 0,
            ..portal_config
        }),
    );
    assert_eq!(local.path(), "/drcom/logout");
    assert_eq!(local.port_or_known_default(), Some(80));
    assert!(!local.query_pairs().any(|(key, _)| key == "user_account"));
}

#[test]
fn both_regions_disconnect_without_reading_credentials_and_preserve_saved_accounts() {
    for staff in [false, true] {
        let (store, credentials, scenario) = setup(staff);
        credentials.seed("profile", r#"{"student_id":"1234567890","campus_card_number":"654321","name":"应用同学","password":"test-only-app-password"}"#);
        credentials
            .save_dorm_credentials(&DormCustomCredentials {
                campus_card_number: "654321".into(),
                password: "test-only-custom-password".into(),
            })
            .unwrap();
        let view = perform_network_logout(
            &store,
            scenario.as_ref(),
            scenario.environment.as_ref(),
            &(),
        )
        .unwrap();
        assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
        if staff {
            assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
            assert_eq!(scenario.unbinds.load(Ordering::Relaxed), 0);
        } else {
            assert_eq!(scenario.logouts.load(Ordering::Relaxed), 0);
            assert_eq!(scenario.unbinds.load(Ordering::Relaxed), 1);
        }
        assert_eq!(credentials.dorm_credential_reads.load(Ordering::Relaxed), 0);
        assert!(!credentials.load_dorm_settings().unwrap().unwrap().enabled);
        assert!(credentials.load_profile().unwrap().is_some());
        assert_eq!(
            credentials
                .load_dorm_credentials()
                .unwrap()
                .unwrap()
                .password,
            "test-only-custom-password"
        );
        let overview = store.network_overview.lock().unwrap();
        assert_eq!(
            overview.region,
            if staff {
                NetworkRegion::Staff
            } else {
                NetworkRegion::Dormitory
            }
        );
        assert_eq!(overview.state, NetworkConnectionState::NotLoggedIn);
        assert!(overview.portal_session.is_none() && overview.staff_session.is_none());
        assert_eq!(
            store.dorm_status.lock().unwrap().state,
            DormLoginState::Disabled
        );
        assert!(store.dorm_status.lock().unwrap().next_probe_at_ms.is_none());
        for url in scenario.requests.lock().unwrap().iter() {
            assert!(
                !url.as_str().contains("172.28.0.99"),
                "must never touch another device"
            );
            assert!(!url.as_str().contains("test-only"));
            assert!(!url.as_str().contains("654321"));
        }
    }
}

#[test]
fn dorm_unbind_failure_falls_back_to_portal_logout() {
    let (store, _, scenario) = setup(false);
    scenario.unbind_reject.store(true, Ordering::Relaxed);
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
    assert_eq!(scenario.unbinds.load(Ordering::Relaxed), 1);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
}

#[test]
fn portal_config_without_unbind_uses_portal_logout() {
    let (store, _, scenario) = setup(false);
    scenario.portal_config.lock().unwrap().un_bind_mac = false;
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
    assert_eq!(scenario.unbinds.load(Ordering::Relaxed), 0);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
}

#[test]
fn config_fetch_failure_uses_portal_logout() {
    let (store, _, scenario) = setup(false);
    scenario.config_fail.store(true, Ordering::Relaxed);
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
    assert_eq!(scenario.unbinds.load(Ordering::Relaxed), 0);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
}

#[test]
fn placeholder_mac_skips_unbind_and_uses_portal_logout() {
    let (store, _, scenario) = setup(false);
    scenario.placeholder_mac.store(true, Ordering::Relaxed);
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
    assert_eq!(scenario.unbinds.load(Ordering::Relaxed), 0);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
}

#[test]
fn login_method_zero_uses_local_portal_logout() {
    let (store, _, scenario) = setup(false);
    {
        let mut config = scenario.portal_config.lock().unwrap();
        config.login_method = 0;
        config.un_bind_mac = false;
    }
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
    assert!(scenario
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|url| url.path() == "/drcom/logout"));
}

#[test]
fn dorm_logout_keeps_verifying_for_six_attempts() {
    let (store, _, scenario) = setup(false);
    scenario.unbind_reject.store(true, Ordering::Relaxed);
    scenario.reject.store(true, Ordering::Relaxed);
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Failed);
    assert_eq!(
        scenario.environment.sleeps.lock().unwrap().as_slice(),
        &[Duration::from_secs(1); LOGOUT_VERIFY_ATTEMPTS]
    );
}

#[test]
fn missing_credentials_do_not_prevent_logout() {
    let (store, credentials, scenario) = setup(true);
    let view = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(view.outcome, NetworkLogoutOutcome::Disconnected);
    assert_eq!(credentials.dorm_credential_reads.load(Ordering::Relaxed), 0);
}

#[test]
fn already_offline_is_idempotent_and_still_persists_disabled() {
    for staff in [false, true] {
        let (store, credentials, scenario) = setup(staff);
        scenario.online.store(false, Ordering::Relaxed);
        if staff {
            let mut probe = StaffPortalProbe {
                config: parse_staff_config(&staff_html(TEST_IP, true)),
                ..Default::default()
            };
            apply_staff_info(&mut probe, &staff_online(TEST_IP));
            store.apply_observation(
                super::super::operations::OperationToken::Passive(store.operations.generation()),
                &NetworkObservation {
                    snapshot: snapshot(),
                    staff: probe,
                    ..Default::default()
                },
                &(),
            );
        }
        let result = perform_network_logout(
            &store,
            scenario.as_ref(),
            scenario.environment.as_ref(),
            &(),
        )
        .unwrap();
        assert_eq!(result.outcome, NetworkLogoutOutcome::AlreadyOffline);
        assert_eq!(scenario.logouts.load(Ordering::Relaxed), 0);
        assert!(!credentials.load_dorm_settings().unwrap().unwrap().enabled);
    }
}

#[test]
fn failed_persistence_stops_before_any_network_request_and_keeps_runtime_disabled() {
    let (store, credentials, scenario) = setup(true);
    credentials
        .fail_settings_save
        .store(true, Ordering::Relaxed);
    assert!(perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &()
    )
    .is_err());
    assert!(scenario.requests.lock().unwrap().is_empty());
    assert!(!dorm_settings(&store).unwrap().enabled);
    assert!(
        credentials.load_dorm_settings().unwrap().unwrap().enabled,
        "failed save must not pretend to persist"
    );
    assert!(!store.operations.busy());
}

#[test]
fn failures_and_unknown_results_never_retry_logout_or_reenable_auto_connect() {
    for uncertain in [false, true] {
        let (store, credentials, scenario) = setup(true);
        scenario.reject.store(!uncertain, Ordering::Relaxed);
        scenario.uncertain.store(uncertain, Ordering::Relaxed);
        let result = perform_network_logout(
            &store,
            scenario.as_ref(),
            scenario.environment.as_ref(),
            &(),
        )
        .unwrap();
        assert_eq!(
            result.outcome,
            if uncertain {
                NetworkLogoutOutcome::Unknown
            } else {
                NetworkLogoutOutcome::Failed
            }
        );
        assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
        assert_eq!(
            scenario.environment.sleeps.lock().unwrap().as_slice(),
            &[Duration::from_secs(1); LOGOUT_VERIFY_ATTEMPTS]
        );
        assert!(!credentials.load_dorm_settings().unwrap().unwrap().enabled);
        assert_eq!(store.dorm_status.lock().unwrap().consecutive_failures, 0);
        if uncertain {
            assert_eq!(
                store.network_overview.lock().unwrap().state,
                NetworkConnectionState::Unknown
            );
        }
    }
}

#[test]
fn network_switch_before_submission_aborts_instead_of_logging_out_the_new_network() {
    let (store, _, scenario) = setup(true);
    scenario.change_before_logout.store(true, Ordering::Relaxed);
    let result = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(result.outcome, NetworkLogoutOutcome::Unknown);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 0);
    assert!(store
        .network_overview
        .lock()
        .unwrap()
        .staff_session
        .is_none());
}

#[test]
fn changed_portal_ip_after_submission_is_not_mistaken_for_success() {
    let (store, credentials, scenario) = setup(true);
    scenario.change_source.store(true, Ordering::Relaxed);
    let result = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(result.outcome, NetworkLogoutOutcome::Unknown);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 1);
    assert!(!credentials.load_dorm_settings().unwrap().unwrap().enabled);
}

struct Signals(mpsc::Sender<()>);
impl NetworkEvents for Signals {
    fn settings_changed(&self, _settings: &DormNetworkSettings) {
        self.0.send(()).unwrap();
    }
}
#[test]
fn logout_disables_immediately_then_waits_for_an_admitted_login_and_rejects_stale_results() {
    let (store, credentials, scenario) = setup(true);
    let login = store.begin_login().unwrap();
    let (sender, receiver) = mpsc::channel();
    let worker_store = store.clone();
    let worker_scenario = scenario.clone();
    let worker = std::thread::spawn(move || {
        perform_network_logout(
            &worker_store,
            worker_scenario.as_ref(),
            worker_scenario.environment.as_ref(),
            &Signals(sender),
        )
    });
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!credentials.load_dorm_settings().unwrap().unwrap().enabled);
    assert_eq!(scenario.logouts.load(Ordering::Relaxed), 0);
    assert!(!login.current());
    assert!(login
        .commit(|| panic!("old login must not overwrite logout"))
        .is_none());
    assert!(store.begin_login().is_err());
    assert!(store.operations.begin_logout().is_err());
    assert!(store
        .operations
        .settings::<()>(|| panic!("cannot enable during logout"))
        .is_err());
    drop(login);
    let result = worker.join().unwrap().unwrap();
    assert_eq!(result.outcome, NetworkLogoutOutcome::Disconnected);
    assert!(!credentials.load_dorm_settings().unwrap().unwrap().enabled);
    assert_eq!(
        store.dorm_status.lock().unwrap().state,
        DormLoginState::Disabled
    );
}

#[test]
fn transient_settings_read_failure_does_not_resume_auto_connect_or_discard_credential_mode() {
    let (store, credentials, scenario) = setup(true);
    credentials
        .save_dorm_settings(&DormNetworkSettings {
            enabled: true,
            credential_mode: DormCredentialMode::Custom,
            ..Default::default()
        })
        .unwrap();
    credentials
        .fail_settings_load
        .store(true, Ordering::Relaxed);
    assert!(perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &()
    )
    .is_err());
    assert!(scenario.requests.lock().unwrap().is_empty());
    credentials
        .fail_settings_load
        .store(false, Ordering::Relaxed);
    let settings = dorm_settings(&store).unwrap();
    assert!(!settings.enabled);
    assert_eq!(settings.credential_mode, DormCredentialMode::Custom);
    let result = perform_network_logout(
        &store,
        scenario.as_ref(),
        scenario.environment.as_ref(),
        &(),
    )
    .unwrap();
    assert_eq!(result.outcome, NetworkLogoutOutcome::Disconnected);
    let restarted = CampusNetworkStore::new(credentials.clone());
    let restored = dorm_settings(&restarted).unwrap();
    assert!(!restored.enabled);
    assert_eq!(restored.credential_mode, DormCredentialMode::Custom);
}
