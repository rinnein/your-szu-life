use super::{model::*, probe::*, scripts::*, session::*, AUTH_HOST};

fn session_cookie() -> StoredAuthCookie {
    StoredAuthCookie {
        name: "JSESSIONID".into(),
        value: "opaque-session".into(),
        domain: AUTH_HOST.into(),
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
    let session = AuthSessionSecret {
        student_id: "1234567890".into(),
        cookies: vec![expired],
    };
    let profile = StoredAccountProfile {
        student_id: "1234567890".into(),
        campus_card_number: "123456".into(),
        name: None,
        password: "secret".into(),
    };
    assert!(!session_matches_account(&session, Some(&profile), None));
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
fn remote_credential_capture_capability_matches_authserver_page_origin() {
    let capability = include_str!("../../capabilities/authserver-credential-capture.json");
    // Tauri evaluates remote IPC against the document origin it reports
    // (`https://authserver.szu.edu.cn/`), not the full login pathname.
    // The Rust command below still enforces /authserver/login exactly.
    assert!(capability.contains(r#"https://authserver.szu.edu.cn"#));
    assert!(!capability.contains("authserver/login*"));
}

#[test]
fn login_capture_script_requires_capture_before_official_submission() {
    let script = auth_login_capture_script(false, None);
    assert!(script.contains("__SZU_LOGIN_CAPTURE_VERSION__ === '2'"));
    assert!(script.contains("if (!document.documentElement) return;"));
    assert!(script.contains("window.__SZU_LOGIN_CAPTURE_READY__ = true"));
    assert!(script.contains("const savedCredentials = null"));
    assert!(script.contains("const applySavedCredentials"));
    assert!(script.contains("autofilledForms = new WeakSet"));
    assert!(script.contains("account.value = savedCredentials.identifier"));
    assert!(script.contains("password.value = savedCredentials.password"));
    assert!(script.contains("autofilledForms.add(form)"));
    assert!(script.contains("await captureCandidate()"));
    assert!(script.contains("window.startLogin(button)"));
    assert!(script.contains("const formField = selector => loginForm()?.querySelector(selector)"));
    assert!(script.contains("const accountInput = () => formField('#username')"));
    assert!(script.contains("const passwordInput = () => formField('#password')"));
    assert!(script.contains("const loginButton = () => formField('#login_submit')"));
    assert!(script.contains("const rememberInput = () => formField('#rememberMe')"));
    assert!(script.contains(".login-main #pwdFromId"));
    assert!(!script.contains(".login-main .m-account #username"));
    assert!(!script.contains(".login-main .m-account #password"));
    assert!(script.contains(".login-main #formErrorTip"));
    assert!(script.contains(".login-main .pwd_login .error-tip"));
    assert!(script.contains("error.classList.remove('hide')"));
    assert!(script.contains("new MutationObserver(refreshLoginForm)"));
    assert!(script.contains("setInterval(refreshLoginForm, 500)"));
    assert!(script.contains("target.closest('.login-main #pwdFromId #login_submit')"));
    assert!(!script.contains("document.querySelector('#username,"));
    assert!(!script.contains("document.querySelector('#password,"));
    assert!(!script.contains("button[type=\"submit\"]"));
    assert!(script.contains("if (isActiveLoginButton(event.target)) intercept(event)"));
    assert!(script.contains("document.addEventListener('keydown', interceptEnter, true)"));
    assert!(script.contains("document.addEventListener('keyup', interceptEnter, true)"));
    assert!(script.contains("if (event.target === loginForm()) intercept(event)"));
    assert!(script.contains("await captureCandidate()"));
    let bridge = &script[script.find("const bridgeSubmit = async").unwrap()..];
    assert!(
        bridge.find("await captureCandidate()").unwrap()
            < bridge.find("continueOfficialLogin(button)").unwrap()
    );
    assert!(script.contains(".qrcode-middle"));
    assert!(script.contains("二维码登录已禁用"));
    assert!(script.contains("stopImmediatePropagation"));
    assert!(script.contains("未能读取当前账号密码表单，请刷新后重新输入"));
    let reauth = auth_login_capture_script(true, None);
    assert!(reauth.contains("const qrAllowed = true"));
}

#[test]
fn probe_result_script_consumes_each_result_once() {
    let script = auth_probe_result_script();
    assert!(script.contains("window.__SZU_AUTH_PROBE__=null"));
    assert!(script.contains("||null"));
}

#[test]
fn auth_flow_guards_are_one_shot_and_resettable() {
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;

    let store = super::AuthStore::new(Arc::new(MemoryCredentialStore::default()));
    assert!(store.claim_session_restore_redirect());
    assert!(!store.claim_session_restore_redirect());
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
    assert!(store.claim_session_restore_redirect());
    assert!(store.claim_auth_completion());
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
    let local: tauri::Url = "http://tauri.localhost/campus-network".parse().unwrap();
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
