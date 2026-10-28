use super::{model::*, probe::*, scripts::*, session::*, AUTH_HOST};

#[test]
fn login_service_uses_actual_ua_and_encodes_the_complete_profile_url() {
    for ua in [
        "Mozilla/5.0 (Linux; Android 14) Mobile",
        "iPhone",
        "iPad",
        "iPod",
        "MOBILE",
    ] {
        let url = authentication_url(Some(ua));
        assert!(is_auth_login_url(url.as_str()));
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![("service".into(), super::AUTH_MOBILE_PROFILE_URL.into())]
        );
        assert!(url.fragment().is_none());
        assert!(url.as_str().contains("%23%2Fhome"));
    }
    for ua in [
        None,
        Some(""),
        Some("unknown"),
        Some("Mozilla/5.0 (X11; Linux x86_64) Chrome/130"),
        Some("Mozilla/5.0 (Windows NT 10.0)"),
    ] {
        let url = authentication_url(ua);
        assert_eq!(
            url.query_pairs()
                .find(|(name, _)| name == "service")
                .unwrap()
                .1,
            super::AUTH_PROFILE_URL
        );
    }
}

#[test]
fn profile_pages_are_not_authentication_evidence_and_foreign_hosts_are_rejected() {
    for url in [super::AUTH_PROFILE_URL, super::AUTH_MOBILE_PROFILE_URL] {
        assert!(is_auth_profile_url(url));
        let page_response = serde_json::json!({"ok": true, "status": 200, "url": url, "text": "<html>profile shell</html>"});
        assert!(!auth_probe_succeeded(&page_response.to_string()));
    }
    assert!(!is_auth_profile_url(
        "https://authserver.szu.edu.cn.evil.example/personalInfo/personalMobile/index.html#/home"
    ));
    assert!(!is_auth_profile_url(
        "http://authserver.szu.edu.cn/personalInfo/personalMobile/index.html#/home"
    ));
}

#[test]
fn cookie_collection_deduplicates_shared_cookies_but_keeps_mobile_and_common_paths() {
    let root = session_cookie();
    let mut mobile = root.clone();
    mobile.path = "/personalInfo/personalMobile".into();
    let mut common = root.clone();
    common.path = "/personalInfo/common".into();
    let foreign = tauri::webview::cookie::Cookie::parse("other=x; Domain=evil.example; Path=/")
        .unwrap()
        .into_owned();
    let cookies = collect_auth_cookies([
        root.to_webview_cookie().unwrap(),
        root.to_webview_cookie().unwrap(),
        mobile.to_webview_cookie().unwrap(),
        common.to_webview_cookie().unwrap(),
        foreign,
    ]);
    assert_eq!(cookies.len(), 3);
    assert_eq!(
        cookies
            .iter()
            .map(|cookie| cookie.path.as_str())
            .collect::<Vec<_>>(),
        vec!["/", "/personalInfo/personalMobile", "/personalInfo/common"]
    );
}

#[test]
fn android_host_cookie_round_trip_does_not_create_a_domain_shadow() {
    let host = from_android_cookie_info(
        "MOD_AUTH_CAS=host-session; domain=authserver.szu.edu.cn; path=/; httponly",
    )
    .unwrap();
    let domain = from_android_cookie_info(
        "MOD_AUTH_CAS=domain-session; domain=.authserver.szu.edu.cn; path=/",
    )
    .unwrap();
    assert!(host.host_only);
    assert!(!domain.host_only);
    assert_eq!(host.path, "/");
    assert_eq!(host.http_only, Some(true));
    let restored = host.to_webview_cookie().unwrap();
    assert!(restored.domain().is_none());
    assert!(!restored.to_string().contains("Domain="));
    assert_eq!(
        domain.to_webview_cookie().unwrap().domain(),
        Some(AUTH_HOST)
    );
    let distinct = collect_auth_cookies([restored, domain.to_webview_cookie().unwrap()]);
    assert_eq!(distinct.len(), 2);
}

#[test]
fn android_cookie_metadata_preserves_paths_and_rejects_lossy_headers() {
    let login = from_android_cookie_info(
        "JSESSIONID=login; domain=authserver.szu.edu.cn; path=/authserver; secure; httponly",
    )
    .unwrap();
    let profile = from_android_cookie_info("JSESSIONID=profile; domain=authserver.szu.edu.cn; path=/personalInfo; secure; httponly; samesite=none").unwrap();
    assert_ne!(login.path, profile.path);
    assert_eq!(profile.secure, Some(true));
    assert_eq!(profile.same_site.as_deref(), Some("None"));
    assert!(from_android_cookie_info("JSESSIONID=lossy").is_err());
    assert!(from_android_cookie_info("JSESSIONID=lossy; domain=authserver.szu.edu.cn").is_err());
    assert!(
        from_android_cookie_info("JSESSIONID=foreign; domain=other.szu.edu.cn; path=/").is_err()
    );
}

#[test]
fn legacy_android_snapshots_require_reauthentication_without_guessing_scope() {
    let legacy: AuthSessionSecret =
        serde_json::from_str(r#"{"student_id":"1234567890","cookies":[]}"#).unwrap();
    assert!(!legacy.has_android_cookie_metadata());
    let current = AuthSessionSecret {
        format_version: 2,
        ..legacy
    };
    assert!(current.has_android_cookie_metadata());
    assert_eq!(
        serde_json::from_str::<AuthSessionSecret>(&serde_json::to_string(&current).unwrap())
            .unwrap(),
        current
    );
}

fn session_cookie() -> StoredAuthCookie {
    StoredAuthCookie {
        name: "JSESSIONID".into(),
        value: "opaque-session".into(),
        domain: AUTH_HOST.into(),
        host_only: false,
        path: "/".into(),
        expires: Some("Sun, 13 Sep 2026 00:00:00 +0000".into()),
        http_only: Some(true),
        secure: Some(true),
        same_site: Some("Lax".into()),
        partitioned: None,
    }
}

#[test]
fn validates_fields_and_reuses_verified_password() {
    let input = AccountProfileInput {
        student_id: "1234567890".into(),
        campus_card_number: "123456".into(),
        name: None,
        password: None,
    };
    assert!(input.validate(Some("secret")).is_ok());
    assert!(input.validate(None).is_err());
}

#[test]
fn cookie_session_round_trip_keeps_sensitive_attributes_internal() {
    let cookie = session_cookie();
    let restored = cookie.to_webview_cookie().expect("valid cookie");
    assert_eq!(restored.name(), "JSESSIONID");
    assert_eq!(restored.domain(), Some(AUTH_HOST));
    assert_eq!(restored.http_only(), Some(true));
    assert_eq!(restored.secure(), Some(true));
    let secret = AuthSessionSecret {
        format_version: 2,
        student_id: "1234567890".into(),
        cookies: vec![cookie],
    };
    let public = serde_json::to_string(&AccountProfileView {
        student_id: secret.student_id,
        campus_card_number: "123456".into(),
        name: None,
        has_password: true,
    })
    .unwrap();
    assert!(!public.contains("opaque-session"));
    assert!(!public.contains("password"));
}

#[test]
fn session_cookie_rejects_external_domains_and_expired_sessions() {
    assert!(!is_auth_cookie_domain("example.com"));
    assert!(!is_auth_cookie_domain("other.szu.edu.cn"));
    assert!(is_auth_cookie_domain(".szu.edu.cn"));
    let mut expired = session_cookie();
    expired.expires = Some("Sun, 13 Sep 2020 00:00:00 +0000".into());
    assert!(expired.is_definitely_expired(time::OffsetDateTime::now_utc()));
}

#[test]
fn restoration_keeps_session_cookies_and_drops_only_expired_records() {
    let mut persistent = session_cookie();
    persistent.expires = None;
    let mut expired = persistent.clone();
    expired.name = "expired".into();
    expired.expires = Some("Sun, 13 Sep 2020 00:00:00 +0000".into());
    let session = AuthSessionSecret {
        format_version: 3,
        student_id: "1234567890".into(),
        cookies: vec![persistent, expired],
    };
    let restored = session
        .restorable_snapshot("1234567890", time::OffsetDateTime::now_utc())
        .unwrap()
        .unwrap();
    assert_eq!(restored.cookies.len(), 1);
    assert_eq!(restored.cookies[0].name, "JSESSIONID");
    assert!(restored.cookies[0].expires.is_none());
    assert_eq!(
        session.restorable_snapshot("9999999999", time::OffsetDateTime::now_utc()),
        Err(SessionRestoreError::AccountMismatch)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn legacy_linux_cookie_scope_is_narrowed_and_latest_update_wins() {
    let mut old = session_cookie();
    old.expires = None;
    old.host_only = false;
    let mut fresh = old.clone();
    fresh.host_only = true;
    fresh.value = "fresh-test-value".into();
    let session = AuthSessionSecret {
        format_version: 2,
        student_id: "1234567890".into(),
        cookies: vec![old, fresh],
    };
    let restored = session
        .restorable_snapshot("1234567890", time::OffsetDateTime::now_utc())
        .unwrap()
        .unwrap();
    assert_eq!(restored.format_version, 3);
    assert_eq!(restored.cookies.len(), 1);
    assert!(restored.cookies[0].host_only);
    assert_eq!(restored.cookies[0].value, "fresh-test-value");
}

#[test]
fn auth_cookie_serialization_requires_a_safe_matching_path() {
    let mut cookie = session_cookie();
    cookie.path = "personalInfo".into();
    assert!(cookie.to_webview_cookie().is_err());
    cookie.path = "/personalInfo".into();
    assert!(cookie.to_webview_cookie().is_ok());
    cookie.domain = "other.szu.edu.cn".into();
    assert!(cookie.to_webview_cookie().is_err());
}

#[test]
fn profile_and_pending_must_match_the_verified_uid() {
    let profile = StoredAccountProfile {
        student_id: "1234567890".into(),
        campus_card_number: "123456".into(),
        name: None,
        password: "secret".into(),
    };
    let pending = PendingAuthCredentials {
        student_id: "1234567890".into(),
        password: "secret".into(),
        name: None,
    };
    assert_eq!(profile.student_id, pending.student_id);
    assert_ne!(profile.student_id, "9999999999");
}

#[test]
fn reauthentication_prefill_uses_saved_profile_or_pending_credentials() {
    let profile = StoredAccountProfile {
        student_id: "1234567890".into(),
        campus_card_number: "123456".into(),
        name: None,
        password: "saved-password".into(),
    };
    let pending = PendingAuthCredentials {
        student_id: "9999999999".into(),
        password: "pending-password".into(),
        name: None,
    };
    assert_eq!(
        login_form_prefill(Some(&profile), Some(&pending)),
        Some(LoginFormPrefill {
            identifier: "1234567890".into(),
            password: "saved-password".into(),
        })
    );
    assert_eq!(
        login_form_prefill(None, Some(&pending)),
        Some(LoginFormPrefill {
            identifier: "9999999999".into(),
            password: "pending-password".into(),
        })
    );
}

#[test]
fn only_a_submitted_password_for_the_verified_existing_account_can_replace_the_secret() {
    let mut capture = CapturedCredentials {
        identifier: "1234567890".into(),
        password: "corrected-test-password".into(),
    };
    assert_eq!(
        verified_password_update("1234567890", Some("123456"), "1234567890", Some(&capture)),
        Some("corrected-test-password")
    );
    assert_eq!(
        verified_password_update("1234567890", Some("123456"), "9999999999", Some(&capture)),
        None
    );
    // QR success has no confirmed password submission, even if a prior form
    // captured a password before the user switched methods.
    assert_eq!(
        verified_password_update("1234567890", Some("123456"), "1234567890", None),
        None
    );
    capture.identifier = "123456".into();
    assert!(
        verified_password_update("1234567890", Some("123456"), "1234567890", Some(&capture))
            .is_some()
    );
    capture.password.clear();
    assert!(
        verified_password_update("1234567890", Some("123456"), "1234567890", Some(&capture))
            .is_none()
    );
}

#[test]
fn remote_credential_capture_capability_allows_webview_redirects() {
    let capability = include_str!("../../capabilities/authserver-credential-capture.json");
    assert!(capability.contains(r#""urls": ["https://*"]"#));
}

#[test]
fn login_script_serializes_credentials_and_native_flow_identity() {
    let credentials = LoginFormPrefill {
        identifier: "1234567890".into(),
        password: "quotes\" and \n newline".into(),
    };
    let script = auth_login_capture_script(true, Some(&credentials), 7, 11, true);
    let options = script.rsplit_once(")(").unwrap().1.trim_end_matches(')');
    let options: serde_json::Value = serde_json::from_str(options).unwrap();
    assert_eq!(options["generation"], "7");
    assert_eq!(options["documentId"], "11");
    assert_eq!(options["prefill"]["password"], credentials.password);
    assert_eq!(options["automatic"], true);
    assert!(!script.contains("export default"));
    assert!(auth_login_capture_script(false, None, 1, 1, false).contains("\"prefill\":null"));
}

#[test]
fn auth_flow_guards_are_one_shot_and_resettable() {
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;

    let store = super::AuthStore::new(Arc::new(MemoryCredentialStore::default()));
    let generation = store.try_start_auth_probe().expect("probe starts");
    assert!(store.try_start_auth_probe().is_none());
    assert!(store.auth_probe_is_current(generation));
    store.cancel_auth_probe();
    assert!(!store.auth_probe_is_current(generation));
    let next_generation = store.try_start_auth_probe().expect("probe restarts");
    store.finish_auth_probe(next_generation);
    assert!(store.claim_auth_completion());
    assert!(!store.claim_auth_completion());
    store.reset_auth_flow_guards();
    assert!(store.claim_auth_completion());
}

#[test]
fn session_refresh_guard_is_single_flight_and_cancellable() {
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;

    let store = super::AuthStore::new(Arc::new(MemoryCredentialStore::default()));
    let generation = store.try_start_session_refresh().expect("refresh starts");
    assert!(store.try_start_session_refresh().is_none());
    assert!(store.session_refresh_is_current(generation));
    store.cancel_session_refresh();
    assert!(!store.session_refresh_is_current(generation));
    store
        .session_refresh_last_started_ms
        .store(0, std::sync::atomic::Ordering::Release);
    let next_generation = store.try_start_session_refresh().expect("refresh restarts");
    store.finish_session_refresh(next_generation);
    assert!(store.try_start_session_refresh().is_none());
    store.reset_auth_flow_guards();
    assert!(store.try_start_session_refresh().is_some());
}

#[test]
fn extracts_the_public_name_from_verified_user_conf() {
    let current_response = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\"},\"data\":{\"cn\":\" 张三 \"}}"}"#;
    assert_eq!(
        probe_identity(current_response),
        Some(VerifiedUserIdentity {
            student_id: "1234567890".into(),
            name: Some("张三".into()),
        })
    );

    let nested = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\",\"userInfo\":{\"realName\":\"李四\"}}}"}"#;
    assert_eq!(
        probe_identity(nested),
        Some(VerifiedUserIdentity {
            student_id: "1234567890".into(),
            name: Some("李四".into()),
        })
    );

    let absent = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\"}}"}"#;
    assert_eq!(
        probe_identity(absent).and_then(|identity| identity.name),
        None
    );
}

#[test]
fn accepts_only_strict_successful_probe_results() {
    let valid = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\"}}"}"#;
    let login_redirect = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/authserver/login","text":"{\"datas\":{\"uid\":\"1234567890\"}}"}"#;
    let invalid_uid = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"bad\"}}"}"#;
    assert!(auth_probe_succeeded(valid));
    assert!(!auth_probe_succeeded(login_redirect));
    assert!(auth_probe_reached_verified_endpoint(invalid_uid));
    assert!(!auth_probe_succeeded(invalid_uid));
}

#[test]
fn auth_login_url_requires_the_exact_official_path() {
    assert!(is_auth_login_url(
        "https://authserver.szu.edu.cn/authserver/login?service=app"
    ));
    assert!(!is_auth_login_url(
        "https://authserver.szu.edu.cn/authserver/login-redirect"
    ));
}

#[test]
fn auth_state_uses_camel_case_data_fields_for_the_frontend() {
    let awaiting = serde_json::to_value(AuthState::AwaitingProfile {
        name: None,
        student_id: Some("1234567890".into()),
    })
    .unwrap();
    assert_eq!(awaiting["awaitingProfile"]["studentId"], "1234567890");
    assert!(awaiting["awaitingProfile"].get("student_id").is_none());

    let authenticated = serde_json::to_value(AuthState::Authenticated {
        name: None,
        student_id: "1234567890".into(),
    })
    .unwrap();
    assert_eq!(authenticated["authenticated"]["studentId"], "1234567890");
    assert!(authenticated["authenticated"].get("student_id").is_none());
}
#[test]
fn public_auth_state_has_distinct_terminal_failures() {
    assert_ne!(
        AuthState::CredentialCaptureFailed,
        AuthState::SessionCaptureFailed
    );
    assert_ne!(
        AuthState::CredentialStorageFailed,
        AuthState::AccountMismatch
    );
}

#[test]
fn credential_snapshot_reads_legacy_storage_without_sharing_auth_locks() {
    use crate::storage::{testing::MemoryCredentialStore, CredentialStore};
    use std::sync::Arc;
    let credentials = Arc::new(MemoryCredentialStore::default());
    credentials.seed("profile",r#"{"student_id":"1234567890","campus_card_number":"001234","password":"test-only","name":"测试"}"#);
    let store = super::AuthStore::new(credentials.clone());
    assert_eq!(
        store.network_credentials().unwrap(),
        ("001234".into(), "test-only".into())
    );
    *store.profile.lock().unwrap() = Some(StoredAccountProfile {
        campus_card_number: "654321".into(),
        password: "cached-test".into(),
        ..Default::default()
    });
    assert_eq!(
        store.network_credentials().unwrap(),
        ("654321".into(), "cached-test".into())
    );
    assert!(store.profile.try_lock().is_ok());
    assert!(store.state.try_lock().is_ok());
    *store.profile.lock().unwrap() = None;
    credentials.delete_profile().unwrap();
    assert!(store.network_credentials().is_err());
}

#[test]
fn finishing_authentication_clears_transient_secrets_and_resumes_the_local_flow() {
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;
    let store = super::AuthStore::new(Arc::new(MemoryCredentialStore::default()));
    *store.state.lock().unwrap() = AuthState::Authenticating;
    *store.attempt.lock().unwrap() = AuthAttempt::InitialSetup;
    *store.captured.lock().unwrap() = Some(CapturedCredentials {
        identifier: "123456".into(),
        password: "test-only".into(),
    });
    *store.login_prefill.lock().unwrap() = Some(LoginFormPrefill {
        identifier: "123456".into(),
        password: "test-only".into(),
    });
    let local: tauri::Url = "http://tauri.localhost/life/campus-network".parse().unwrap();
    *store.resume_url.lock().unwrap() = Some(local.clone());
    let terminal = AuthState::Authenticated {
        student_id: "1234567890".into(),
        name: Some("测试".into()),
    };
    store.finish_attempt(terminal.clone());
    assert_eq!(*store.state.lock().unwrap(), terminal);
    assert_eq!(*store.attempt.lock().unwrap(), AuthAttempt::None);
    assert!(store.captured.lock().unwrap().is_none());
    assert!(store.login_prefill.lock().unwrap().is_none());
    assert_eq!(super::webview::application_resume_url(&store), local);
    assert!(store.resume_url.lock().unwrap().is_none());
    let fallback = super::webview::application_resume_url(&store);
    #[cfg(any(target_os = "windows", target_os = "android"))]
    assert_eq!(fallback.as_str(), "http://tauri.localhost/");
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    assert_eq!(fallback.as_str(), "tauri://localhost/");
}

#[test]
fn payments_account_snapshot_requires_verified_matching_complete_profile_and_is_read_only() {
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;
    let store = super::AuthStore::new(Arc::new(MemoryCredentialStore::default()));
    *store.profile.lock().unwrap() = Some(StoredAccountProfile {
        student_id: "1234567890".into(),
        campus_card_number: "001234".into(),
        password: "test-only-password".into(),
        name: None,
    });
    for state in [
        AuthState::Checking,
        AuthState::Unauthenticated,
        AuthState::Authenticating,
        AuthState::SessionExpired,
    ] {
        *store.state.lock().unwrap() = state;
        assert!(store.authenticated_campus_card_number().is_err());
    }
    let authenticated = AuthState::Authenticated {
        student_id: "1234567890".into(),
        name: None,
    };
    *store.state.lock().unwrap() = authenticated.clone();
    let auth_return: tauri::Url = "http://tauri.localhost/settings".parse().unwrap();
    *store.resume_url.lock().unwrap() = Some(auth_return.clone());
    assert_eq!(store.authenticated_campus_card_number().unwrap(), "001234");
    assert_eq!(*store.state.lock().unwrap(), authenticated);
    assert_eq!(*store.resume_url.lock().unwrap(), Some(auth_return));
    assert_eq!(
        store.profile.lock().unwrap().as_ref().unwrap().password,
        "test-only-password"
    );
    assert!(store.state.try_lock().is_ok());
    assert!(store.profile.try_lock().is_ok());
    store.profile.lock().unwrap().as_mut().unwrap().student_id = "9999999999".into();
    assert!(store.authenticated_campus_card_number().is_err());
    store.profile.lock().unwrap().as_mut().unwrap().student_id = "1234567890".into();
    for invalid in ["", "12345", "1234567", "12345x"] {
        store
            .profile
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .campus_card_number = invalid.into();
        assert!(store.authenticated_campus_card_number().is_err());
    }
}
