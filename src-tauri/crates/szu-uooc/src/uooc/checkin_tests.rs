use super::{
    checkin::{CheckinSource, CheckinStatus, Job, Outcome},
    progress::{CourseProgress, ProgressEntry},
    tests::{course, ok, root, service_test, Server},
    *,
};
use serde_json::{json, Value};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn payload(signed: bool) -> Value {
    let date = if signed {
        progress::today()
    } else {
        progress::today().pred_opt().unwrap()
    };
    json!({"total_progress_value":20,"video_progress_value":0,"signin":5,"signin_cnt":if signed {2}else{1},"signin_total":25,"signin_progress_value":if signed {8}else{4},"signin_time":format!("{date} 00:00:01")})
}
fn ready(service: &Service) {
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42"), course("43")];
    });
}
fn job(cid: &str) -> Job {
    Job {
        id: format!("checkin-{cid}"),
        cid: cid.into(),
        account: "7".into(),
        epoch: 0,
        source: CheckinSource::Manual,
        cancel: CancellationToken::new(),
    }
}
async fn idle(service: &Service) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while service.runtime.lock().unwrap().checkins.pumping {
        assert!(Instant::now() < deadline, "checkin queue timed out");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn automatic_and_manual_checkins_submit_once_then_confirm_progress_serially() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let signed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let observed = signed.clone();
    let server = Server::new(move |request| {
        assert_eq!(request.headers["user-agent"], user_agent::DESKTOP);
        let cid = request.fields()["cid"].clone();
        seen.lock().unwrap().push(format!(
            "{} {cid}",
            if request.path.starts_with("/home/course/info?") {
                "INFO"
            } else {
                "GET"
            }
        ));
        if request.path == "/home/course/info?cid=42" || request.path == "/home/course/info?cid=43"
        {
            assert_eq!(request.method, "GET");
            assert_eq!(request.fields().len(), 1);
            assert!(!request.headers.contains_key("content-type"));
            assert!(request.headers["referer"].ends_with(&format!("/home/course/{cid}")));
            assert!(
                observed.lock().unwrap().insert(cid),
                "no duplicate submission"
            );
            return ok(json!({"signin": 1}));
        }
        assert_eq!(request.method, "GET");
        assert!(request.path.starts_with("/home/course/progress?"));
        ok(payload(observed.lock().unwrap().contains(&cid)))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.progress.insert(
            "42".into(),
            ProgressEntry {
                data: Some(CourseProgress::parse(&payload(false), progress::today())),
                ..Default::default()
            },
        );
    });
    service.enqueue_automatic_checkins();
    service.enqueue_automatic_checkins();
    service
        .enqueue_checkins(vec!["43".into(), "43".into()], CheckinSource::Manual)
        .unwrap();
    idle(&service).await;
    assert_eq!(
        *calls.lock().unwrap(),
        ["GET 42", "INFO 42", "GET 42", "GET 43", "INFO 43", "GET 43"]
    );
    assert_eq!(
        service.snapshot().checkins["42"].source,
        CheckinSource::Automatic
    );
    assert_eq!(
        service.snapshot().checkins["43"].source,
        CheckinSource::Manual
    );
    assert!(service
        .snapshot()
        .checkins
        .values()
        .all(|s| s.status == CheckinStatus::Succeeded));
    let saved = service.credentials.load_uooc_session().unwrap().unwrap();
    assert!(
        saved.contains("rotated"),
        "updated cookies must be checkpointed at completion"
    );
    service.enqueue_automatic_checkins();
    idle(&service).await;
    assert_eq!(calls.lock().unwrap().len(), 6);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_forward_checkin_fields_can_confirm_success() {
    let before = CourseProgress::parse(&payload(false), progress::today());
    let mut value = payload(false);
    value["video_progress_value"] = json!(100);
    value["total_progress_value"] = json!(80);
    value["score"] = json!(90);
    value["signin_score"] = json!(100);
    assert!(!CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
    value["signin_progress_value"] = json!(8);
    assert!(CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
    value["signin_progress_value"] = json!(3);
    assert!(!CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
    value["signin_cnt"] = json!(2);
    assert!(
        !CourseProgress::parse(&value, progress::today()).confirms_checkin(&before),
        "a decreasing percentage is not success"
    );
    value["signin_progress_value"] = json!(4);
    assert!(CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
    value["signin_cnt"] = json!(1);
    value["signin_time"] = json!(format!("{} 00:00:01", progress::today()));
    assert!(CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
    // The displayed eligibility can change independently; confirm against the date itself.
    value["signin"] = json!(0);
    assert!(CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
    value["signin_time"] = json!("invalid");
    assert!(!CourseProgress::parse(&value, progress::today()).confirms_checkin(&before));
}

#[tokio::test]
async fn a_new_beijing_day_checks_eligible_courses_without_any_video_tasks() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let counted = counts.clone();
    let signed = Arc::new(AtomicBool::new(false));
    let did_sign = signed.clone();
    let server = Server::new(move |request| {
        assert_eq!(request.headers["user-agent"], user_agent::DESKTOP);
        seen.lock().unwrap().push(request.path.clone());
        if request.path == "/home/course/info?cid=42" || request.path == "/home/course/info?cid=43"
        {
            assert_eq!(request.method, "GET");
            assert_eq!(request.fields()["cid"], "42");
            assert!(!did_sign.swap(true, Ordering::SeqCst));
            return ok(json!({"signin": 1}));
        }
        assert_eq!(request.method, "GET");
        if request.path.starts_with(client::LIST) {
            return ok(json!({"data": (42..=48).map(|id| json!({
                "id": id, "status": if id == 46 {35} else {31}, "parent_name": "测试课程"
            })).collect::<Vec<_>>(), "page": {"pages": 1}}));
        }
        if request.path == "/home/member/user" {
            return ok(json!({"id": 7, "name": "test"}));
        }
        assert!(request.path.starts_with("/home/course/progress?"));
        let cid = request.fields()["cid"].clone();
        let mut counts = counted.lock().unwrap();
        let count = counts.entry(cid.clone()).or_default();
        *count += 1;
        ok(match cid.as_str() {
            "42" => {
                let mut value = payload(did_sign.load(Ordering::SeqCst));
                value["video_progress_value"] = json!(100);
                value
            }
            "43" => json!({"signin": 0, "signin_cnt": 1, "signin_total": 25}),
            "44" => json!({"signin": 5, "signin_cnt": 25, "signin_total": 25}),
            "45" => payload(true),
            "47" => json!({"signin": 5, "signin_cnt": 0, "signin_total": 0}),
            "48" => json!({"signin": 5}),
            _ => panic!("ended courses must not be polled"),
        })
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    let yesterday = progress::today().pred_opt().unwrap();
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.checkins.daily_scans.insert(("7".into(), yesterday));
        runtime
            .checkins
            .attempted
            .insert(("7".into(), "42".into(), yesterday));
        runtime.snapshot.settings.dry_run = true;
        runtime.snapshot.progress.insert(
            "42".into(),
            ProgressEntry {
                // Yesterday's displayed "today" status must not suppress a new-day check.
                data: Some(CourseProgress::parse(&payload(true), progress::today())),
                ..Default::default()
            },
        );
    }
    service
        .refresh_session(Some(progress::today()))
        .await
        .unwrap();
    idle(&service).await;
    let snapshot = service.snapshot();
    assert!(snapshot.tasks.is_empty());
    assert_eq!(snapshot.checkins.len(), 1);
    assert_eq!(snapshot.checkins["42"].source, CheckinSource::Automatic);
    assert_eq!(snapshot.checkins["42"].status, CheckinStatus::Succeeded);
    assert_eq!(counts.lock().unwrap()["42"], 3);
    assert!(signed.load(Ordering::SeqCst));
    for cid in ["43", "44", "45", "47", "48"] {
        assert_eq!(counts.lock().unwrap()[cid], 1);
    }
    let total = calls.lock().unwrap().len();
    service
        .refresh_session(Some(progress::today()))
        .await
        .unwrap();
    idle(&service).await;
    assert_eq!(
        calls.lock().unwrap().len(),
        total,
        "same-day background ticks must be idle"
    );
    assert!(service
        .runtime
        .lock()
        .unwrap()
        .checkins
        .attempted
        .contains(&("7".into(), "42".into(), progress::today())));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn a_fresh_baseline_and_unchanged_responses_do_not_claim_checkin() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let posts = Arc::new(AtomicUsize::new(0));
    let submitted = posts.clone();
    let server = Server::new(move |request| {
        if request.path == "/home/course/info?cid=42" || request.path == "/home/course/info?cid=43"
        {
            assert_eq!(request.method, "GET");
            submitted.fetch_add(1, Ordering::SeqCst);
            return ok(json!({"signin": 1}));
        }
        assert_eq!(request.method, "GET");
        observed.fetch_add(1, Ordering::SeqCst);
        ok(payload(false))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    let mut stale = payload(false);
    stale["signin_progress_value"] = json!(0);
    stale["signin_cnt"] = json!(0);
    service.change(|s| {
        s.progress.insert(
            "42".into(),
            ProgressEntry {
                data: Some(CourseProgress::parse(&stale, progress::today())),
                ..Default::default()
            },
        );
    });
    assert_eq!(
        service
            .execute_checkin(
                &job("42"),
                Duration::from_secs(1),
                Duration::from_millis(250)
            )
            .await
            .unwrap(),
        Outcome::Unchanged
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn already_satisfied_and_no_requirement_finish_after_one_get() {
    for value in [
        payload(true),
        json!({"signin":0}),
        json!({"signin":5,"signin_cnt":25,"signin_total":25}),
    ] {
        let root = root();
        let service = service_test(root.clone());
        ready(&service);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let server = Server::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            ok(value.clone())
        });
        *service.client.write().unwrap() =
            Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
        assert_eq!(
            service
                .execute_checkin(&job("42"), Duration::from_millis(5), Duration::from_secs(2))
                .await
                .unwrap(),
            Outcome::AlreadySatisfied
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn cancel_stops_polling_without_touching_video_work() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let release = Arc::new(AtomicBool::new(false));
    let released = release.clone();
    let server = Server::new(move |request| {
        assert_eq!(request.method, "GET");
        observed.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !released.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        ok(payload(false))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    let video = CancellationToken::new();
    service
        .runtime
        .lock()
        .unwrap()
        .workers
        .insert("43".into(), (1, video.clone()));
    service
        .enqueue_checkins(vec!["42".into()], CheckinSource::Manual)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while calls.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    service.cancel_checkins(vec!["42".into()]);
    release.store(true, Ordering::SeqCst);
    idle(&service).await;
    assert_eq!(
        service.snapshot().checkins["42"].status,
        CheckinStatus::Cancelled
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!video.is_cancelled());
    service.runtime.lock().unwrap().workers.clear();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn automatic_failures_do_not_retry_until_requested_and_network_timeout_is_distinct() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let slow = Arc::new(AtomicBool::new(false));
    let delayed = slow.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let server = Server::new(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        if delayed.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(70));
        }
        json!({"code":500})
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.courses = vec![course("42")];
        s.progress.insert(
            "42".into(),
            ProgressEntry {
                data: Some(CourseProgress::parse(&payload(false), progress::today())),
                ..Default::default()
            },
        );
    });
    service.enqueue_automatic_checkins();
    idle(&service).await;
    assert_eq!(
        service.snapshot().checkins["42"].status,
        CheckinStatus::Failed
    );
    // Even if a later refresh reports eligibility, do not retry the same automatic attempt.
    service.change(|s| s.progress.get_mut("42").unwrap().error = None);
    service.enqueue_automatic_checkins();
    idle(&service).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    slow.store(true, Ordering::SeqCst);
    let error = service
        .execute_checkin(
            &job("42"),
            Duration::from_millis(5),
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
    assert!(error.contains("查询超时"));
    assert!(!error.contains("未检测到"));
    assert!(!service.snapshot().progress["42"].refreshing);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn uncertain_signin_responses_are_verified_without_repeating_course_info() {
    for (status, delayed) in [(503, false), (200, false), (200, true)] {
        let root = root();
        let service = service_test(root.clone());
        ready(&service);
        let posts = Arc::new(AtomicUsize::new(0));
        let submitted = posts.clone();
        let gets = Arc::new(AtomicUsize::new(0));
        let fetched = gets.clone();
        let server = Server::bytes(move |request| {
            if request.path == "/home/course/info?cid=42"
                || request.path == "/home/course/info?cid=43"
            {
                assert_eq!(request.method, "GET");
                submitted.fetch_add(1, Ordering::SeqCst);
                if delayed {
                    std::thread::sleep(Duration::from_millis(700));
                }
                // The server accepted the operation but the acknowledgement was lost/invalid.
                return (status, b"incomplete response".to_vec(), String::new());
            }
            assert_eq!(request.method, "GET");
            assert_eq!(request.path, "/home/course/progress?cid=42");
            fetched.fetch_add(1, Ordering::SeqCst);
            (
                200,
                serde_json::to_vec(&ok(payload(submitted.load(Ordering::SeqCst) > 0))).unwrap(),
                String::new(),
            )
        });
        *service.client.write().unwrap() =
            Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
        let result = service
            .execute_checkin(&job("42"), Duration::from_millis(5), Duration::from_secs(1))
            .await;
        assert_eq!(result.unwrap(), Outcome::Changed);
        assert_eq!(posts.load(Ordering::SeqCst), 1);
        assert_eq!(gets.load(Ordering::SeqCst), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn rejected_signin_stops_and_auth_rejection_expires_the_session() {
    for code in [401, 601] {
        let root = root();
        let service = service_test(root.clone());
        ready(&service);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let server = Server::new(move |request| {
            seen.lock().unwrap().push(request.method.clone());
            if request.path == "/home/course/info?cid=42"
                || request.path == "/home/course/info?cid=43"
            {
                assert_eq!(request.method, "GET");
                return json!({"code":code});
            }
            assert_eq!(request.path, "/home/course/progress?cid=42");
            ok(payload(false))
        });
        *service.client.write().unwrap() =
            Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
        service
            .enqueue_checkins(vec!["42".into()], CheckinSource::Manual)
            .unwrap();
        idle(&service).await;
        assert_eq!(*calls.lock().unwrap(), ["GET", "GET"]);
        let snapshot = service.snapshot();
        if code == 401 {
            assert_eq!(snapshot.session, SessionStatus::LoginRequired);
            assert!(snapshot.checkins["42"].message.contains("登录已失效"));
            assert!(service.credentials.load_uooc_session().unwrap().is_none());
        } else {
            assert_eq!(snapshot.checkins["42"].status, CheckinStatus::Failed);
            assert!(snapshot.checkins["42"].message.contains("签到提交失败"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn cancelling_or_switching_account_during_submission_ignores_late_results() {
    for switch_account in [false, true] {
        let root = root();
        let service = service_test(root.clone());
        ready(&service);
        let started = Arc::new(AtomicBool::new(false));
        let seen = started.clone();
        let release = Arc::new(AtomicBool::new(false));
        let released = release.clone();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let counted = calls.clone();
        let server = Server::new(move |request| {
            counted.lock().unwrap().push(request.method.clone());
            if request.path == "/home/course/info?cid=42"
                || request.path == "/home/course/info?cid=43"
            {
                seen.store(true, Ordering::SeqCst);
                let deadline = Instant::now() + Duration::from_secs(2);
                while !released.load(Ordering::SeqCst) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(2));
                }
                return ok(json!({"signin": 1}));
            }
            assert_eq!(request.path, "/home/course/progress?cid=42");
            ok(payload(false))
        });
        *service.client.write().unwrap() =
            Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
        service
            .enqueue_checkins(vec!["42".into()], CheckinSource::Manual)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(
            service.snapshot().checkins["42"].status,
            CheckinStatus::Submitting
        );
        if switch_account {
            let mut runtime = service.runtime.lock().unwrap();
            // Production account changes cancel and replace identity under the same lock.
            // A standalone cancel is allowed to checkpoint the still-current account.
            checkin::cancel_locked(&mut runtime, &[], "账户已变化");
            runtime.epoch += 1;
            runtime.session_cancel.cancel();
            runtime.snapshot.user.as_mut().unwrap().id = "8".into();
            runtime.snapshot.progress.clear();
            runtime.snapshot.checkins.clear();
        } else {
            service.cancel_checkins(vec!["42".into()]);
        }
        release.store(true, Ordering::SeqCst);
        idle(&service).await;
        assert_eq!(*calls.lock().unwrap(), ["GET", "GET"]);
        if switch_account {
            assert!(service.snapshot().checkins.is_empty());
            assert!(service.snapshot().progress.is_empty());
            assert!(service.credentials.load_uooc_session().unwrap().is_none());
        } else {
            assert_eq!(
                service.snapshot().checkins["42"].status,
                CheckinStatus::Cancelled
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn info_markers_are_distinct_from_progress_weights_and_legacy_is_sign() {
    for marker in [
        json!(0),
        json!(1),
        json!(2),
        json!("0"),
        json!("1"),
        json!("2"),
        json!(5),
        Value::Null,
    ] {
        let expected = match marker
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| marker.to_string())
            .as_str()
        {
            "0" => Ok(client::CourseSignin::NotRequired),
            "1" => Ok(client::CourseSignin::Signed),
            "2" => Ok(client::CourseSignin::AlreadySigned),
            _ => Err(Error::Invalid),
        };
        let server = Server::new(move |request| {
            assert_eq!(request.method, "GET");
            assert_eq!(request.path, "/home/course/info?cid=42");
            ok(json!({"signin":marker,"is_sign":1}))
        });
        let client = Client::with_base(session::load(None).unwrap(), &server.base).unwrap();
        assert_eq!(
            client.signin("42", &CancellationToken::new()).await,
            expected
        );
    }
}

#[tokio::test]
async fn info_zero_and_two_refresh_progress_once_without_waiting_for_an_increase() {
    for (marker, outcome) in [(0, Outcome::NotRequired), (2, Outcome::AlreadySigned)] {
        let root = root();
        let service = service_test(root.clone());
        ready(&service);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let server = Server::new(move |request| {
            assert_eq!(request.method, "GET");
            seen.lock().unwrap().push(request.path.clone());
            if request.path == "/home/course/info?cid=42" {
                return ok(json!({"signin":marker}));
            }
            assert_eq!(request.path, "/home/course/progress?cid=42");
            ok(payload(false))
        });
        *service.client.write().unwrap() =
            Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
        assert_eq!(
            service
                .execute_checkin(&job("42"), Duration::from_secs(2), Duration::from_secs(1))
                .await
                .unwrap(),
            outcome
        );
        assert_eq!(
            *calls.lock().unwrap(),
            [
                "/home/course/progress?cid=42",
                "/home/course/info?cid=42",
                "/home/course/progress?cid=42"
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn forbidden_info_does_not_expire_a_valid_login_or_retry_the_entry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = Server::bytes(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        (403, b"not allowed method".to_vec(), String::new())
    });
    let client = Client::with_base(session::load(None).unwrap(), &server.base).unwrap();
    assert_eq!(
        client.signin("42", &CancellationToken::new()).await,
        Err(Error::Forbidden)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn network_errors_and_expired_sessions_have_distinct_checkin_states() {
    for (status, expected_session, message) in [
        (503, SessionStatus::Ready, "请检查网络"),
        (401, SessionStatus::LoginRequired, "登录已失效"),
    ] {
        let root = root();
        let service = service_test(root.clone());
        ready(&service);
        let server = Server::bytes(move |request| {
            assert_eq!(request.method, "GET");
            assert_eq!(request.path, "/home/course/progress?cid=42");
            (status, vec![], String::new())
        });
        *service.client.write().unwrap() =
            Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
        service
            .enqueue_checkins(vec!["42".into()], CheckinSource::Manual)
            .unwrap();
        idle(&service).await;
        let snapshot = service.snapshot();
        assert_eq!(snapshot.session, expected_session);
        assert!(snapshot.checkins["42"].message.contains(message));
        assert_ne!(snapshot.checkins["42"].status, CheckinStatus::Succeeded);
        assert!(!snapshot.progress["42"].refreshing);
        if status == 401 {
            assert!(service.credentials.load_uooc_session().unwrap().is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn account_changes_reject_late_checkin_results() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let started = Arc::new(AtomicBool::new(false));
    let seen = started.clone();
    let server = Server::new(move |_| {
        seen.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(70));
        ok(payload(true))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service
        .enqueue_checkins(vec!["42".into()], CheckinSource::Manual)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.epoch += 1;
        runtime.session_cancel.cancel();
        runtime.snapshot.user.as_mut().unwrap().id = "8".into();
        runtime.snapshot.progress.clear();
        runtime.snapshot.checkins.clear();
    }
    idle(&service).await;
    assert!(service.snapshot().progress.is_empty());
    assert!(service.snapshot().checkins.is_empty());
    assert!(service.credentials.load_uooc_session().unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn video_auth_expiry_cancels_checkin_and_invalidates_older_refreshes() {
    let root = root();
    let service = service_test(root.clone());
    let checkin_cancel = CancellationToken::new();
    let old_session = service.runtime.lock().unwrap().session_cancel.clone();
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.snapshot.session = SessionStatus::Ready;
        runtime.snapshot.progress.insert(
            "43".into(),
            ProgressEntry {
                refreshing: true,
                ..Default::default()
            },
        );
        runtime
            .workers
            .insert("42".into(), (3, CancellationToken::new()));
        runtime.checkins.active = Some(Job {
            cancel: checkin_cancel.clone(),
            ..job("43")
        });
    }
    service.finish("42", 3, Err(Error::Unauthorized), &service.client());
    assert!(checkin_cancel.is_cancelled());
    assert!(old_session.is_cancelled());
    assert_eq!(service.snapshot().session, SessionStatus::LoginRequired);
    assert_eq!(service.runtime.lock().unwrap().epoch, 1);
    assert!(!service.snapshot().progress["43"].refreshing);
    std::fs::remove_dir_all(root).unwrap();
}
