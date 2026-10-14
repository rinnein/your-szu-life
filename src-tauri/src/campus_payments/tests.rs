use super::{
    model::{parse_observation, NetworkFeePortalNotice, NoticeCode, PageKind, PageObservation},
    scripts::{cancel_script, page_script},
    state::{
        Action, CampusPaymentsStore, Phase, Session, CAPTCHA_MESSAGE, FORM_ERROR_MESSAGE,
        LOGIN_ERROR_MESSAGE, MANUAL_MESSAGE,
    },
    urls::{
        application_return_url, is_login_url, is_portal_origin, is_target_url,
        same_application_origin,
    },
    PORTAL_URL,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::Url;

fn url(value: &str) -> Url {
    value.parse().unwrap()
}
fn local() -> Url {
    url("http://tauri.localhost/campus-payments/network")
}
fn login() -> Url {
    url("https://self.szu.edu.cn/self/nav_login")
}
fn target() -> Url {
    url(PORTAL_URL)
}
fn page(kind: PageKind, token: &str) -> PageObservation {
    PageObservation {
        kind,
        document_token: token.into(),
    }
}
fn session(now: Instant) -> Session {
    Session::new(1, "001234".into(), local(), now)
}

#[test]
fn credentials_are_only_eligible_for_the_exact_https_login_endpoint() {
    assert!(is_login_url(&login()));
    assert!(is_login_url(&url(
        "https://self.szu.edu.cn:443/self/nav_login"
    )));
    for value in [
        "http://self.szu.edu.cn/self/nav_login",
        "https://self.szu.edu.cn.evil.test/self/nav_login",
        "https://self.szu.edu.cn@evil.test/self/nav_login",
        "https://other.szu.edu.cn/self/nav_login",
        "https://self.szu.edu.cn:444/self/nav_login",
        "https://user:password@self.szu.edu.cn/self/nav_login",
        "https://self.szu.edu.cn/self/nav_login/",
        "https://self.szu.edu.cn/self/nav_login?return=other",
        "https://self.szu.edu.cn/self/nav_login#fragment",
        "https://self.szu.edu.cn/other/nav_login",
        "https://self.szu.edu.cn/self/nav_operators",
    ] {
        assert!(!is_login_url(&url(value)), "unexpected login URL: {value}");
    }
    assert!(is_portal_origin(&target()));
    assert!(is_target_url(&target()));
    assert!(!is_target_url(&login()));
    assert!(!is_target_url(&url("https://evil.test/self/nav_operators")));
}

#[test]
fn return_url_accepts_only_the_payments_route_on_real_application_origins() {
    let development = url("http://localhost:1420/");
    for value in [
        "tauri://localhost/campus-payments/network",
        "http://tauri.localhost/campus-payments/network",
        "http://tauri.localhost/campus-payments/network?discard=1#fragment",
        "http://localhost:1420/campus-payments/network?discard=1#fragment",
    ] {
        let result = application_return_url(&url(value), Some(&development)).unwrap();
        assert_eq!(result.path(), "/campus-payments/network");
        assert_eq!(result.query(), None);
        assert_eq!(result.fragment(), None);
    }
    for value in [
        "http://tauri.localhost/",
        "tauri://localhost/campus-payments",
        "http://tauri.localhost/campus-payments",
        "tauri://localhost/settings",
        "http://tauri.localhost:1420/campus-payments",
        "https://self.szu.edu.cn/campus-payments",
        "https://tauri.localhost/campus-payments",
        "http://localhost:1421/campus-payments",
        "http://127.0.0.1:1420/campus-payments",
        "http://user:secret@localhost:1420/campus-payments",
        "http://tauri.localhost.evil.test/campus-payments",
        "file:///campus-payments",
        "tauri://localhost:80/campus-payments",
    ] {
        assert!(
            application_return_url(&url(value), Some(&development)).is_none(),
            "unexpected return URL: {value}"
        );
    }
    assert!(
        application_return_url(&url("http://localhost:1420/campus-payments/network"), None)
            .is_none()
    );
    assert!(!same_application_origin(
        &url("http://user@tauri.localhost/campus-payments"),
        &local()
    ));
}

#[test]
fn an_existing_official_session_is_handed_off_without_requesting_credentials() {
    let now = Instant::now();
    let mut flow = session(now);
    assert_eq!(
        flow.observe(&page(PageKind::Portal, "existing"), &target(), now),
        Action::HandOff
    );
    assert!(!flow.attempted);
    assert_eq!(flow.message(), MANUAL_MESSAGE);
    assert_eq!(flow.timeout(now + Duration::from_secs(3600)), None);
    assert_eq!(
        flow.observe(&page(PageKind::LoginReady, "later"), &login(), now),
        Action::Wait
    );
}

#[test]
fn submission_is_reserved_before_eval_and_never_retried_on_same_or_reloaded_form() {
    let now = Instant::now();
    let mut flow = session(now);
    assert_eq!(
        flow.observe(&page(PageKind::LoginReady, "first"), &login(), now),
        Action::Submit("first".into())
    );
    assert!(flow.attempted);
    assert_eq!(flow.phase, Phase::Submitting);
    // Includes a lost submit callback: seeing the same form is not permission to retry.
    for kind in [PageKind::LoginReady, PageKind::Submitted, PageKind::Pending] {
        assert_eq!(
            flow.observe(&page(kind, "first"), &login(), now),
            Action::Wait
        );
    }
    assert_eq!(
        flow.observe(&page(PageKind::LoginReady, "reloaded"), &login(), now),
        Action::HandOff
    );
    assert_eq!(flow.message(), LOGIN_ERROR_MESSAGE);
    assert_eq!(
        flow.observe(&page(PageKind::LoginReady, "third"), &login(), now),
        Action::Wait
    );
}

#[test]
fn captcha_errors_invalid_forms_and_manual_takeover_disable_all_automation() {
    let now = Instant::now();
    for (kind, message) in [
        (PageKind::Captcha, CAPTCHA_MESSAGE),
        (PageKind::LoginError, LOGIN_ERROR_MESSAGE),
        (PageKind::InvalidForm, FORM_ERROR_MESSAGE),
        (PageKind::Other, FORM_ERROR_MESSAGE),
        (PageKind::Manual, MANUAL_MESSAGE),
    ] {
        let mut flow = session(now);
        assert_eq!(
            flow.observe(&page(kind, "first"), &login(), now),
            Action::HandOff
        );
        assert_eq!(flow.message(), message);
        assert!(!flow.attempted);
        assert_eq!(flow.timeout(now + Duration::from_secs(60)), None);
        assert_eq!(
            flow.observe(&page(PageKind::LoginReady, "later"), &login(), now),
            Action::Wait
        );
    }
    let mut submitting = session(now);
    submitting.observe(&page(PageKind::LoginReady, "first"), &login(), now);
    // Manual takeover wins even at the login timeout boundary.
    assert_eq!(
        submitting.observe(
            &page(PageKind::Manual, "first"),
            &login(),
            now + Duration::from_secs(20)
        ),
        Action::HandOff
    );
    assert_eq!(submitting.timeout(now + Duration::from_secs(60)), None);
}

#[test]
fn a_foreign_page_or_wrong_login_path_cannot_request_credentials() {
    let now = Instant::now();
    for location in [
        "https://evil.test/self/nav_login",
        "https://self.szu.edu.cn:444/self/nav_login",
        "http://self.szu.edu.cn/self/nav_login",
        "https://self.szu.edu.cn/self/nav_login?extra=1",
        "https://self.szu.edu.cn/other",
    ] {
        let mut flow = session(now);
        assert_eq!(
            flow.observe(&page(PageKind::LoginReady, "first"), &url(location), now),
            Action::HandOff
        );
        assert!(!flow.attempted);
    }
}

#[test]
fn provisional_document_probes_cannot_change_the_flow_or_extend_its_deadline() {
    let now = Instant::now();
    let mut flow = session(now);
    for (seconds, location) in [(1, login()), (2, target()), (3, url("https://other.test/"))] {
        assert_eq!(
            flow.observe(
                &page(PageKind::Stale, "old-document"),
                &location,
                now + Duration::from_secs(seconds)
            ),
            Action::Wait
        );
        assert_eq!(flow.phase, Phase::Opening);
        assert_eq!(flow.phase_since, now);
        assert!(!flow.attempted);
    }
    assert_eq!(
        flow.observe(
            &page(PageKind::Stale, "old-document"),
            &target(),
            now + Duration::from_secs(20)
        ),
        Action::Fail(NoticeCode::LoadTimeout)
    );
}

#[test]
fn an_anonymous_redirect_document_waits_for_login_without_handing_off_or_submitting() {
    let now = Instant::now();
    let mut flow = session(now);
    for seconds in [0, 1, 19] {
        assert_eq!(
            flow.observe(
                &page(PageKind::Pending, "redirect"),
                &target(),
                now + Duration::from_secs(seconds)
            ),
            Action::Wait
        );
        assert_eq!(flow.phase, Phase::Opening);
        assert!(!flow.attempted);
    }
    assert_eq!(
        flow.timeout(now + Duration::from_secs(20)),
        Some(NoticeCode::LoadTimeout)
    );
    assert_eq!(
        flow.observe(
            &page(PageKind::LoginReady, "committed-login"),
            &login(),
            now + Duration::from_secs(19)
        ),
        Action::Submit("committed-login".into())
    );
}

#[test]
fn loading_login_and_form_waits_have_independent_bounded_deadlines() {
    let now = Instant::now();
    let mut loading = session(now);
    assert_eq!(loading.timeout(now + Duration::from_secs(19)), None);
    assert_eq!(
        loading.observe(
            &page(PageKind::Loading, "loading"),
            &login(),
            now + Duration::from_secs(19)
        ),
        Action::Wait
    );
    assert_eq!(loading.phase, Phase::Opening);
    assert_eq!(
        loading.timeout(now + Duration::from_secs(20)),
        Some(NoticeCode::LoadTimeout)
    );

    let mut missing_form = session(now);
    assert_eq!(
        missing_form.observe(
            &page(PageKind::Pending, "loaded"),
            &login(),
            now + Duration::from_secs(2)
        ),
        Action::Wait
    );
    assert_eq!(missing_form.timeout(now + Duration::from_secs(6)), None);
    assert_eq!(
        missing_form.observe(
            &page(PageKind::Pending, "loaded"),
            &login(),
            now + Duration::from_secs(7)
        ),
        Action::Fail(NoticeCode::FormTimeout)
    );

    let mut submitting = session(now);
    submitting.observe(
        &page(PageKind::LoginReady, "first"),
        &login(),
        now + Duration::from_secs(3),
    );
    assert_eq!(submitting.timeout(now + Duration::from_secs(22)), None);
    assert_eq!(
        submitting.timeout(now + Duration::from_secs(23)),
        Some(NoticeCode::LoginTimeout)
    );
}

#[test]
fn navigation_without_a_current_document_observation_does_not_extend_deadlines() {
    let now = Instant::now();
    let mut opening = session(now);
    for seconds in 1..20 {
        opening.observe_location(&target(), now + Duration::from_secs(seconds));
    }
    assert_eq!(
        opening.timeout(now + Duration::from_secs(20)),
        Some(NoticeCode::LoadTimeout)
    );

    let mut submitting = session(now);
    submitting.observe(&page(PageKind::LoginReady, "login"), &login(), now);
    for seconds in 1..20 {
        submitting.observe_location(&login(), now + Duration::from_secs(seconds));
        submitting.observe_location(&target(), now + Duration::from_secs(seconds));
    }
    assert_eq!(
        submitting.timeout(now + Duration::from_secs(20)),
        Some(NoticeCode::LoginTimeout)
    );
}

#[test]
fn successful_login_restores_the_target_only_once_after_leaving_the_login_form() {
    let now = Instant::now();
    let dashboard = url("https://self.szu.edu.cn/self/nav_home");
    let mut flow = session(now);
    flow.observe(&page(PageKind::LoginReady, "login"), &login(), now);
    assert_eq!(
        flow.observe(&page(PageKind::Portal, "dashboard"), &dashboard, now),
        Action::RestoreTarget("dashboard".into())
    );
    assert!(flow.restored_target);
    assert_eq!(
        flow.observe(&page(PageKind::Portal, "dashboard"), &dashboard, now),
        Action::Wait
    );
    assert_eq!(
        flow.observe(&page(PageKind::Portal, "dashboard"), &target(), now),
        Action::HandOff
    );
    assert_eq!(
        flow.observe(&page(PageKind::Portal, "another"), &dashboard, now),
        Action::Wait
    );
    assert_eq!(flow.timeout(now + Duration::from_secs(60)), None);
}

#[test]
fn returning_to_login_after_restoration_never_loops() {
    let now = Instant::now();
    let mut flow = session(now);
    flow.observe(&page(PageKind::LoginReady, "login"), &login(), now);
    flow.observe(
        &page(PageKind::Portal, "home"),
        &url("https://self.szu.edu.cn/self/nav_home"),
        now,
    );
    assert_eq!(
        flow.timeout(now + Duration::from_secs(20)),
        Some(NoticeCode::LoadTimeout)
    );
    assert_eq!(
        flow.observe(&page(PageKind::LoginReady, "again"), &login(), now),
        Action::HandOff
    );
    assert_eq!(flow.message(), LOGIN_ERROR_MESSAGE);
    assert_eq!(
        flow.observe(&page(PageKind::LoginReady, "again"), &login(), now),
        Action::Wait
    );
}

#[test]
fn timeout_returns_release_initial_and_remote_tasks_and_preserve_one_shot_notices() {
    for seen_remote in [false, true] {
        let store = CampusPaymentsStore::default();
        let now = Instant::now();
        let (generation, _) = store.reserve("001234".into(), local(), now).unwrap();
        let mut runtime = store.runtime.lock().unwrap();
        // Native navigation may still report the starting application page.
        assert!(!runtime.finish_local_navigation(&local(), generation));
        let session = runtime.active.as_mut().unwrap();
        session.seen_remote = seen_remote;
        assert_eq!(
            session.timeout(now + Duration::from_secs(20)),
            Some(NoticeCode::LoadTimeout)
        );
        let notice = NetworkFeePortalNotice::from(NoticeCode::LoadTimeout);
        session.hand_off(notice.message);
        runtime.notice = Some(notice);
        assert!(!runtime.finish_local_navigation(&local(), generation + 1));
        assert!(!runtime.finish_local_navigation(&target(), generation));
        assert!(runtime.finish_local_navigation(&local(), generation));
        assert!(runtime.active.is_none());
        assert_eq!(runtime.notice.take().unwrap().code, NoticeCode::LoadTimeout);
        assert!(runtime.notice.take().is_none());
        drop(runtime);
        assert!(store.reserve("001234".into(), local(), now).is_ok());
    }
}

#[test]
fn local_route_changes_cancel_pending_navigation_without_finishing_newer_tasks() {
    let store = CampusPaymentsStore::default();
    let now = Instant::now();
    let (old, _) = store.reserve("001234".into(), local(), now).unwrap();
    assert!(store
        .runtime
        .lock()
        .unwrap()
        .finish_local_navigation(&url("http://tauri.localhost/"), old));
    let (current, _) = store.reserve("001234".into(), local(), now).unwrap();
    let mut runtime = store.runtime.lock().unwrap();
    assert!(!runtime.finish_local_navigation(&url("http://tauri.localhost/settings"), old));
    assert_eq!(runtime.active.as_ref().unwrap().generation, current);
    assert!(!runtime.finish_local_navigation(&local(), current));
    runtime.active.as_mut().unwrap().seen_remote = true;
    assert!(runtime.finish_local_navigation(&local(), current));
    assert!(runtime.active.is_none());
}

#[test]
fn cancelled_generations_cannot_fail_cancel_or_prepare_newer_tasks() {
    let store = CampusPaymentsStore::default();
    let now = Instant::now();
    let (old, _) = store.reserve("001234".into(), local(), now).unwrap();
    assert_eq!(store.cancel_generation(old), Some(local()));
    let (current, _) = store.reserve("654321".into(), local(), now).unwrap();
    assert_ne!(old, current);
    assert!(store.fail(old, NoticeCode::LoadTimeout).is_none());
    assert!(store.cancel_generation(old).is_none());
    assert!(store.prepare_navigation(old, now).is_err());
    assert_eq!(
        store
            .runtime
            .lock()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .generation,
        current
    );
    assert_eq!(store.fail(current, NoticeCode::LoginTimeout), Some(local()));
    let mut runtime = store.runtime.lock().unwrap();
    assert!(runtime.active.is_none());
    assert_eq!(
        runtime.notice.take().unwrap().code,
        NoticeCode::LoginTimeout
    );
    assert!(runtime.notice.take().is_none());
}

#[test]
fn only_one_concurrent_open_can_reserve_the_webview() {
    let store = Arc::new(CampusPaymentsStore::default());
    let attempts: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .reserve("001234".into(), local(), Instant::now())
                    .is_ok()
            })
        })
        .collect();
    assert_eq!(
        attempts
            .into_iter()
            .filter(|attempt| attempt.thread().id() != std::thread::current().id())
            .map(|attempt| attempt.join().unwrap() as usize)
            .sum::<usize>(),
        1
    );
    let opening = store.opening.try_lock().unwrap();
    let generation = store
        .runtime
        .lock()
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .generation;
    store.cancel_generation(generation);
    assert!(
        store.opening.try_lock().is_err(),
        "cancellation must not release in-flight native cleanup"
    );
    drop(opening);
    assert!(store.opening.try_lock().is_ok());
}

#[test]
fn account_changes_require_cleanup_and_failed_preparation_does_not_commit_the_new_account() {
    let store = CampusPaymentsStore::default();
    let now = Instant::now();
    let (first, changed) = store.reserve("001234".into(), local(), now).unwrap();
    assert!(!changed);
    store
        .prepare_navigation(first, now + Duration::from_secs(4))
        .unwrap();
    assert_eq!(
        store
            .runtime
            .lock()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .timeout(now + Duration::from_secs(23)),
        None
    );
    store.cancel_generation(first);
    let (same, changed) = store.reserve("001234".into(), local(), now).unwrap();
    assert!(!changed);
    store.cancel_generation(same);
    let (other, changed) = store.reserve("654321".into(), local(), now).unwrap();
    assert!(changed);
    store.cancel_generation(other); // Cleanup / account validation failed.
    assert!(store.prepare_navigation(other, now).is_err());
    let (_, changed) = store.reserve("654321".into(), local(), now).unwrap();
    assert!(changed);
}

#[test]
fn opening_again_clears_a_previous_one_shot_notice() {
    let store = CampusPaymentsStore::default();
    let now = Instant::now();
    let (generation, _) = store.reserve("001234".into(), local(), now).unwrap();
    store.fail(generation, NoticeCode::FormTimeout);
    assert!(store.runtime.lock().unwrap().notice.is_some());
    store.reserve("001234".into(), local(), now).unwrap();
    assert!(store.runtime.lock().unwrap().notice.is_none());
}

fn script_options(script: &str) -> serde_json::Value {
    let source = include_str!("page.js").replacen("export default ", "", 1);
    let prefix = format!("({source})(");
    serde_json::from_str(
        script
            .strip_prefix(&prefix)
            .unwrap()
            .strip_suffix(')')
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn only_the_one_shot_submit_script_contains_safely_serialized_credentials() {
    let flow = session(Instant::now());
    let password = "quoted\"password\\with\nnewline';window.unwanted=true;//";
    let script = page_script(
        &flow,
        "submit",
        Some("document"),
        Some(("001234", password)),
        None,
    );
    let options = script_options(&script);
    assert_eq!(options["password"], password);
    assert_eq!(options["account"], "001234");
    assert_eq!(options["documentToken"], "document");
    assert!(
        !script.contains(password),
        "raw password must not be interpolated into source"
    );
    for operation in ["probe", "toolbar", "restore", "return"] {
        let expected_url = (operation == "probe").then_some(PORTAL_URL);
        let script = page_script(&flow, operation, Some("document"), None, expected_url);
        let options = script_options(&script);
        assert_eq!(
            options.get("expectedUrl").and_then(|value| value.as_str()),
            expected_url
        );
        assert!(options.get("password").is_none());
        assert!(options.get("account").is_none());
        assert!(!script.contains("001234"));
    }
    let cancel = cancel_script(flow.generation);
    assert!(!cancel.contains(password));
    assert!(cancel.contains("cancelled = true"));
    assert!(cancel.contains("manual = true"));
    assert!(cancel.contains("flow.dispose?.()"));
    assert!(cancel.find("cancelled = true").unwrap() < cancel.find("flow.dispose").unwrap());
}

#[test]
fn synchronous_callback_results_are_bounded_and_accept_engine_string_wrapping() {
    let plain = r#"{"kind":"loginReady","documentToken":"opaque-document"}"#;
    let once = serde_json::to_string(plain).unwrap();
    let twice = serde_json::to_string(&once).unwrap();
    for input in [plain, &once, &twice] {
        let result = parse_observation(input).unwrap();
        assert_eq!(result.kind, PageKind::LoginReady);
        assert_eq!(result.document_token, "opaque-document");
    }
    for input in [
        "undefined",
        "null",
        "[]",
        "{}",
        r#"{"kind":"unknown","documentToken":"x"}"#,
        r#"{"kind":"portal","documentToken":""}"#,
    ] {
        assert!(parse_observation(input).is_none());
    }
    assert!(parse_observation(&"x".repeat(4097)).is_none());
    assert!(parse_observation(&format!(
        r#"{{"kind":"portal","documentToken":"{}"}}"#,
        "x".repeat(129)
    ))
    .is_none());
}

#[test]
fn notices_and_ipc_contracts_keep_credentials_and_remote_permissions_out() {
    for code in [
        NoticeCode::LoadTimeout,
        NoticeCode::LoginTimeout,
        NoticeCode::FormTimeout,
        NoticeCode::NavigationFailed,
        NoticeCode::CredentialsUnavailable,
    ] {
        let value = serde_json::to_value(NetworkFeePortalNotice::from(code)).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 2);
        assert!(object.contains_key("code"));
        assert!(object.contains_key("message"));
    }
    let default_capability: serde_json::Value =
        serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
    assert!(default_capability.get("remote").is_none());
    let remote: serde_json::Value = serde_json::from_str(include_str!(
        "../../capabilities/authserver-credential-capture.json"
    ))
    .unwrap();
    assert_eq!(
        remote["remote"]["urls"],
        serde_json::json!(["https://*"])
    );
    assert_eq!(
        remote["permissions"],
        serde_json::json!([
            "allow-capture-auth-credentials",
            "allow-control-auth-recovery"
        ])
    );
    let commands = include_str!("commands.rs");
    assert!(commands.contains("open_network_fee_portal(window: tauri::WebviewWindow)"));
    assert!(commands.contains("service::local_return_url(&window)?"));
    let service = include_str!("service.rs");
    assert!(!service.contains("resume_url"));
    assert!(!service.contains("AuthState::"));
    assert!(
        include_str!("../app.rs").contains("if !campus_payments::on_page_load(window, payload)")
    );
    for operation in ["confirm_profile", "navigate_to_authentication", "logout"] {
        let auth = include_str!("../auth/service.rs");
        let function = auth
            .split(&format!("pub(super) fn {operation}("))
            .nth(1)
            .unwrap()
            .split("\npub(super) fn ")
            .next()
            .unwrap();
        assert!(function.contains("crate::campus_payments::cancel(&app)"));
    }
}
