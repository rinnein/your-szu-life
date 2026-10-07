use super::{
    cache::Cache,
    captcha,
    model::*,
    protocol::{self, Failure},
    session,
};
use serde_json::json;
fn task() -> Task {
    Task {
        id: "t".into(),
        course: Course {
            id: "new".into(),
            ..Default::default()
        },
        old_course: Some(Course {
            id: "old".into(),
            ..Default::default()
        }),
        threshold: 2,
        group: None,
        priority: 0,
        gear: Gear::Ultra,
        failures: 0,
        attempts: 0,
        phase: Phase::Selecting,
        message: String::new(),
        next_at: 0,
        started_at: None,
        enabled: true,
        pending: None,
        one_shot: false,
        goal_observed: false,
    }
}
fn pending(t: &mut Task, phase: Phase, dropping: bool) {
    t.phase = phase;
    t.pending = Some(PendingWrite {
        class_id: if phase == Phase::Selecting {
            "new"
        } else {
            "old"
        }
        .into(),
        dropping,
        phase,
        sent_at: 1,
        accepted: true,
    });
}
#[test]
fn downgrade_only_at_exact_boundaries() {
    let mut t = task();
    for _ in 0..4 {
        t.failed();
    }
    assert_eq!(t.gear, Gear::Ultra);
    t.failed();
    assert_eq!((t.gear, t.failures), (Gear::High, 0));
    for _ in 0..29 {
        t.failed();
    }
    assert_eq!(t.gear, Gear::High);
    t.failed();
    assert_eq!((t.gear, t.failures), (Gear::Medium, 0));
    for _ in 0..100 {
        t.failed();
    }
    assert_eq!(t.gear, Gear::Medium);
    t.switch_gear(Gear::Ultra);
    assert_eq!(t.failures, 0);
}
#[test]
fn swap_waits_for_confirmed_drop_and_restores_on_full() {
    let mut t = task();
    pending(&mut t, Phase::Dropping, true);
    t.confirmed(true);
    assert!(t.pending.is_some());
    t.confirmed(false);
    assert_eq!(t.phase, Phase::Selecting);
    assert_eq!(t.gear, Gear::Ultra);
    pending(&mut t, Phase::Selecting, false);
    t.rejected(true, false, "已满".into());
    assert_eq!(t.phase, Phase::Restoring);
    assert_eq!(t.gear, Gear::Ultra);
    pending(&mut t, Phase::Restoring, false);
    t.confirmed(false);
    assert!(t.pending.is_some());
    t.confirmed(true);
    assert_eq!(t.phase, Phase::Monitoring);
    assert!(t.enabled);
}
#[test]
fn successful_new_course_finishes_swap() {
    let mut t = task();
    pending(&mut t, Phase::Selecting, false);
    t.confirmed(true);
    assert_eq!(t.phase, Phase::Completed);
    assert!(!t.enabled);
}
#[test]
fn durable_unresolved_write_survives_roundtrip() {
    let mut t = task();
    pending(&mut t, Phase::Dropping, true);
    let raw = serde_json::to_string(&t).unwrap();
    let restored: Task = serde_json::from_str(&raw).unwrap();
    assert_eq!(restored.pending.unwrap().phase, Phase::Dropping);
}
#[test]
fn errors_do_not_conflate_initialization_auth_and_network() {
    for (status, body, expected) in [
        (503, "初始化中", Failure::Network),
        (200, "系统初始化中", Failure::Initializing),
        (403, "Forbidden", Failure::Forbidden),
        (401, "", Failure::Session),
        (429, "", Failure::Throttled),
        (200, "{\"code\":\"-1\",\"msg\":\"人数已满\"}", Failure::Full),
        (404, "missing", Failure::Protocol),
    ] {
        assert_eq!(
            protocol::classify(status, "", body).unwrap_err().kind,
            expected
        );
    }
    assert_eq!(
        protocol::classify(302, "https://webvpn.szu.edu.cn/users/auth/cas", "")
            .unwrap_err()
            .kind,
        Failure::Needs2fa
    );
}
#[test]
fn hostname_boundary_is_strict() {
    for u in [
        "https://webvpn.szu.edu.cn/",
        "https://bkxk.webvpn.szu.edu.cn/",
    ] {
        assert!(session::vpn_url(&u.parse().unwrap()));
    }
    for u in [
        "https://webvpn.szu.edu.cn.evil.test/",
        "http://webvpn.szu.edu.cn/",
        "https://evilwebvpn.szu.edu.cn/",
        "https://user@webvpn.szu.edu.cn/",
    ] {
        assert!(!session::vpn_url(&u.parse().unwrap()));
    }
}
#[test]
fn callback_preserves_encoded_target() {
    let target = "https://bkxk.webvpn.szu.edu.cn/a?x=1&y=2";
    let url = session::auth_url(target).unwrap();
    let service = url
        .query_pairs()
        .find(|(k, _)| k == "service")
        .unwrap()
        .1
        .into_owned();
    let service: reqwest::Url = service.parse().unwrap();
    assert_eq!(
        service.query_pairs().find(|(k, _)| k == "url").unwrap().1,
        target
    );
}
#[test]
fn coordinates_require_four_distinct_in_bounds_points() {
    assert!(captcha::coordinates(&[[1, 20], [2, 30], [3, 40], [4, 50]]).is_ok());
    assert!(captcha::coordinates(&[[1, 20]; 4]).is_err());
    assert!(captcha::coordinates(&[[250, 20], [2, 30], [3, 40], [4, 50]]).is_err());
    assert!(captcha::coordinates(&[]).is_err());
}
#[test]
fn selection_uses_szu_form_and_lab_case() {
    let c = Context {
        student: "test".into(),
        batch: "batch".into(),
        campus: "01".into(),
        term: "term".into(),
    };
    let course = Course {
        id: "class".into(),
        category: "FANKC".into(),
        test_ids: vec!["lab1".into(), "lab2".into()],
        ..Default::default()
    };
    let add = protocol::selection(&c, &course, false);
    assert_eq!(add["data"]["testTeachingClassID"], "lab1,lab2");
    let drop = protocol::selection(&c, &course, true);
    assert_eq!(drop["data"]["operationType"], "2");
    assert!(drop["data"].get("testTeachingClassID").is_none());
}
#[test]
fn sqlite_isolates_accounts_and_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let db = Cache::open(&temp.path().join("cache.sqlite")).unwrap();
    db.put("a", "catalog", "batch1", &json!([1])).unwrap();
    db.put("b", "catalog", "batch1", &json!([2])).unwrap();
    assert_eq!(
        db.get::<serde_json::Value>("a", "catalog", "batch1")
            .unwrap(),
        Some(json!([1]))
    );
    assert!(db
        .get::<serde_json::Value>("a", "catalog", "batch2")
        .unwrap()
        .is_none());
}

#[test]
fn school_password_matches_reference_vectors() {
    assert_eq!(captcha::password("abc"), "N0QyMEFBM0M2ODQ0MTdGRg==");
    assert_eq!(captcha::password("test-password-123"),"QzVERjFGRUQ4OUI5NEZGRjc3QzUwODU3RUMwOUVCMzFDOTUwQjNGMjM0QjI5M0FFNjgxMUIzMDJGQUYwQUNFMDJCQTQzMzI3NUU0QUJERTQ=");
    assert_eq!(
        captcha::password("测试A😀"),
        "N0EyQzMzOUMwMzExOTk3OTA4MjJFMDRGMzM2ODQxMUI="
    );
}

#[test]
fn deployed_contract_parser_uses_named_functions() {
    let c = super::contract::parse(
        r#"function deleteVolunteerResult(e){return ajax(BaseUrl+"/sys/xsxkapp/elective/deleteVolunteer.do",e)} function queryProcess(){return ajax(BaseUrl+"/sys/xsxkapp/elective/process.do")} "#,
    );
    assert_eq!(c.drop_path.as_deref(), Some("elective/deleteVolunteer.do"));
    assert_eq!(c.process_path.as_deref(), Some("elective/process.do"));
}

#[test]
fn cookie_roundtrip_preserves_host_only_and_path() {
    let jar = session::jar("").unwrap();
    let url = "https://bkxk.webvpn.szu.edu.cn/xsxkapp/".parse().unwrap();
    jar.lock()
        .unwrap()
        .parse("test=value; Path=/xsxkapp/; HttpOnly; Secure", &url)
        .unwrap();
    let raw = session::serialize(&jar).unwrap();
    let loaded = session::jar(&raw).unwrap();
    let guard = loaded.lock().unwrap();
    let cookie = guard.iter_any().next().unwrap();
    assert!(
        matches!(&cookie.domain,cookie_store::CookieDomain::HostOnly(host) if host=="bkxk.webvpn.szu.edu.cn")
    );
    assert_eq!(
        guard
            .get_request_values(&"https://other.webvpn.szu.edu.cn/xsxkapp/".parse().unwrap())
            .count(),
        0
    );
    assert_eq!(
        guard
            .get_request_values(&"https://bkxk.webvpn.szu.edu.cn/elsewhere".parse().unwrap())
            .count(),
        0
    );
}
#[test]
fn single_attempt_stops_on_full() {
    let mut t = task();
    t.old_course = None;
    t.one_shot = true;
    pending(&mut t, Phase::Selecting, false);
    t.rejected(true, false, "已满".into());
    assert!(!t.enabled);
    assert!(t.pending.is_none());
}

#[test]
fn admission_rejection_keeps_authenticated_state_and_is_endpoint_specific() {
    let body = r#"{"data":null,"msg":"抱歉,未评教,学籍信息不对,无法选课!","code":"2","timestamp":"1791366161400"}"#;
    let error = protocol::classify_student(200, "", body).unwrap_err();
    assert_eq!(error.kind, Failure::AdmissionRestricted);
    let mut snapshot = Snapshot::default();
    snapshot.authenticated();
    snapshot.admission_failure(&error);
    assert!(snapshot.logged_in);
    assert_eq!(snapshot.authentication, Authentication::Authenticated);
    assert_eq!(snapshot.admission, Admission::Restricted);
    assert_eq!(
        snapshot.admission_message,
        "抱歉,未评教,学籍信息不对,无法选课!"
    );
    assert!(!snapshot.can_select());
    assert_eq!(
        protocol::classify(200, "", body).unwrap_err().kind,
        Failure::Business
    );
    assert_eq!(
        protocol::classify_student(401, "", body).unwrap_err().kind,
        Failure::Session
    );
}
#[test]
fn initialization_and_network_errors_do_not_revoke_a_verified_login() {
    let mut s = Snapshot::default();
    s.authenticated();
    s.admission_failure(&protocol::Error::new(Failure::Initializing, "系统初始化中"));
    assert!(s.logged_in);
    assert_eq!(s.admission, Admission::Initializing);
    s.admission_failure(&protocol::Error::new(Failure::Network, "超时"));
    assert!(s.logged_in);
    assert_eq!(s.admission, Admission::Unavailable);
    s.admission_failure(&protocol::Error::new(Failure::Session, "登录失效"));
    assert!(!s.logged_in);
    assert_eq!(s.authentication, Authentication::Expired);
}
#[test]
fn shared_cookie_delta_imports_browser_changes_without_overwriting_newer_native_values() {
    use super::school_cookies;
    let jar = session::jar("").unwrap();
    let url = "http://bkxk.szu.edu.cn/xsxkapp/".parse().unwrap();
    jar.lock()
        .unwrap()
        .parse("route=one; Path=/; HttpOnly", &url)
        .unwrap();
    let before = school_cookies::records(&jar, Mode::Campus);
    let browser = session::jar(&session::serialize(&jar).unwrap()).unwrap();
    browser
        .lock()
        .unwrap()
        .parse("route=two; Path=/; HttpOnly", &url)
        .unwrap();
    let changed = school_cookies::records(&browser, Mode::Campus);
    school_cookies::merge_browser_delta(&jar, Mode::Campus, &before, &changed, &before);
    assert_eq!(
        jar.lock()
            .unwrap()
            .get_request_values(&url)
            .next()
            .unwrap()
            .1,
        "two"
    );
    jar.lock()
        .unwrap()
        .parse("route=native-new; Path=/; HttpOnly", &url)
        .unwrap();
    school_cookies::merge_browser_delta(&jar, Mode::Campus, &before, &changed, &before);
    assert_eq!(
        jar.lock()
            .unwrap()
            .get_request_values(&url)
            .next()
            .unwrap()
            .1,
        "native-new"
    );
    assert!(!school_cookies::allowed(
        Mode::Campus,
        "evil.szu.edu.cn",
        "route"
    ));
    assert!(!school_cookies::allowed(
        Mode::Campus,
        "szu.edu.cn.evil.test",
        "JSESSIONID"
    ));
}
#[test]
fn cache_catalog_search_is_account_scoped_and_merges_existing_pages() {
    let dir = tempfile::tempdir().unwrap();
    let db = Cache::open(&dir.path().join("cache.sqlite")).unwrap();
    let context = Context {
        student: "a".into(),
        term: "term".into(),
        batch: "batch".into(),
        campus: "01".into(),
    };
    let query = Query {
        category: "TJKC".into(),
        keyword: String::new(),
        page: 0,
    };
    let courses = vec![Course {
        id: "one".into(),
        code: "C1".into(),
        name: "缓存课程".into(),
        ..Default::default()
    }];
    let scope = serde_json::to_string(&(context.clone(), query.clone())).unwrap();
    db.put("a", "catalog", &scope, &(&courses, false, 123_i64))
        .unwrap();
    let result = db.catalog("a", &context, &query).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["coverage"], "partial");
    assert_eq!(db.catalog("b", &context, &query).unwrap()["total"], 0);
}
#[test]
fn official_selection_satisfies_swap_without_an_additional_drop() {
    let mut t = task();
    t.phase = Phase::Monitoring;
    let selected = vec![t.course.clone(), t.old_course.clone().unwrap()];
    super::model::reconcile_membership(&mut t, &selected);
    assert_eq!(t.phase, Phase::Completed);
    assert!(t.pending.is_none());
    assert!(!t.enabled);
}
#[test]
fn official_changes_pause_only_the_affected_swap_and_pending_results_stay_pending() {
    let mut t = task();
    t.phase = Phase::Monitoring;
    super::model::reconcile_membership(&mut t, &[]);
    assert!(!t.enabled);
    let mut t = task();
    pending(&mut t, Phase::Restoring, false);
    let new = t.course.clone();
    super::model::reconcile_membership(&mut t, &[new.clone()]);
    assert!(t.pending.is_some());
    assert!(t.goal_observed);
    let old = t.old_course.clone().unwrap();
    super::model::reconcile_membership(&mut t, &[new, old]);
    assert_eq!(t.phase, Phase::Completed);
    assert!(!t.enabled);
}
