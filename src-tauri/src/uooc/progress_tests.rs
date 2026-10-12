use super::{
    progress::{Attendance, CourseProgress},
    tests::{course, ok, root, service_test, Server},
    *,
};
use chrono::NaiveDate;
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

#[test]
fn progress_uses_server_values_and_keeps_unknowns_unknown() {
    let data = CourseProgress::parse(
        &json!({"total_progress_value":"42.5","total_progress":"99%", "video_progress":"100%", "video_cnt":"36","video_total":36,
        "test_progress_value":"bad", "homework_progress_value":-1,"homework_progress":"7.5%"}),
        progress::today(),
    );
    assert_eq!(data.total_percent, Some(42.5));
    assert!(data.video_complete());
    assert_eq!(data.video.count, Some(36));
    assert_eq!(data.test.percent, None);
    assert_eq!(data.test.count, None);
    assert_eq!(data.homework.percent, Some(7.5));
    assert_eq!(data.attendance, Attendance::Unknown);
}

#[test]
fn weights_and_details_come_from_each_courses_response() {
    let value = json!({
        "signin":"5","video":"30","homework":"10","test":"20","discuz":"5","finalExam":"30",
        "signin_progress_value":50,"signin_cnt":"10","signin_total":"20",
        "video_progress_value":50,"video_cnt":28,"video_total":57,"video_score":"30",
        "homework_progress_value":0,"homework_cnt":0,"homework_total":0,
        "test_progress_value":100,"test_cnt":"13","test_total":"13",
        "discuss_progress_value":60,"discuss_cnt":3,"discuss_score":"1.5","discuss_avg_score":"30",
        "exam_cnt":"1","exam_total":2,"exam_score":"12","exam_avg_score":"40",
        "score":"57.5","weighted_score":"51.5","total_progress_value":7,
        "discuss_publish_cnt":1,"discuss_reply_cnt":2,"video_mins":"30.5",
        "discuss":99,"exam":99
    });
    let p = CourseProgress::parse(&value, progress::today());
    assert_eq!(
        [
            p.signin.weight,
            p.video.weight,
            p.homework.weight,
            p.test.weight,
            p.discussion.weight,
            p.exam.weight
        ],
        [
            Some(5.0),
            Some(30.0),
            Some(10.0),
            Some(20.0),
            Some(5.0),
            Some(30.0)
        ]
    );
    assert_eq!(p.weight_total, Some(100.0));
    assert_eq!(p.weighted_percent, Some(55.5));
    assert!(!p.weighted_partial);
    assert_eq!(p.total_percent, Some(7.0));
    assert_eq!(p.exam.percent, Some(50.0));
    assert_eq!(p.discussion.total, None);
    assert_eq!(p.homework.total, Some(0));
    assert_eq!(p.discussion.score, Some(1.5));
    assert_eq!(p.discussion_completion_ratio(), Some(0.3));
    assert_eq!(p.exam.average_score, Some(40.0));
    assert_eq!(p.weighted_score, Some(51.5));
    assert_eq!(p.discussion_posts, Some(1));
    assert_eq!(p.discussion_replies, Some(2));
    assert_eq!(p.video_minutes, Some(30.5));
    assert!(
        !p.video_complete(),
        "a full score must not stand in for video completion"
    );

    let other = CourseProgress::parse(
        &json!({"signin":0,"video":80,"homework":0,"test":20,"discuz":0,"finalExam":0,"video_progress_value":50,"test_progress_value":100}),
        progress::today(),
    );
    assert_eq!(other.weighted_percent, Some(60.0));
    assert!(
        !other.weighted_partial,
        "zero-weight unknown categories do not affect completion"
    );
}

#[test]
fn absent_discussion_and_exam_completion_remains_partial_not_a_grade() {
    let p = CourseProgress::parse(
        &json!({
            "signin":5,"video":30,"homework":10,"test":20,"discuz":5,"finalExam":30,
            "signin_progress_value":50,"video_progress_value":50,"homework_progress_value":0,"test_progress_value":100,
            "discuss_cnt":9,"discuss_avg_score":100,"discuss_score":5,
            "exam_cnt":0,"exam_total":0,"exam_avg_score":100,"exam_score":30
        }),
        progress::today(),
    );
    assert_eq!(p.weighted_percent, Some(37.5));
    assert!(p.weighted_partial);
    assert_eq!(p.discussion.percent, None);
    assert_eq!(p.exam.percent, None);
}

#[test]
fn missing_invalid_or_zero_weights_never_become_a_hardcoded_rubric() {
    let mut value = json!({"signin":0,"video":100,"homework":0,"test":0,"discuz":0,"finalExam":0,"video_progress_value":70});
    assert_eq!(
        CourseProgress::parse(&value, progress::today()).weighted_percent,
        Some(70.0)
    );
    value["video"] = json!(0);
    let zero = CourseProgress::parse(&value, progress::today());
    assert_eq!(zero.weight_total, Some(0.0));
    assert_eq!(zero.weighted_percent, None);
    value["video"] = json!(30);
    assert_eq!(
        CourseProgress::parse(&value, progress::today()).weighted_percent,
        None
    );
    value["video"] = json!("invalid");
    let invalid = CourseProgress::parse(&value, progress::today());
    assert_eq!(invalid.video.weight, None);
    assert_eq!(invalid.weight_total, None);
    value.as_object_mut().unwrap().remove("discuz");
    assert_eq!(
        CourseProgress::parse(&value, progress::today())
            .discussion
            .weight,
        None
    );
}

#[test]
fn signin_handles_beijing_day_boundaries_targets_and_invalid_fields() {
    let day = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    for (weight, count, total, last, want) in [
        (
            json!(0),
            json!(0),
            json!(25),
            json!(null),
            Attendance::NotRequired,
        ),
        (
            json!(5),
            json!(0),
            json!(0),
            json!(null),
            Attendance::NotRequired,
        ),
        (
            json!(5),
            json!(25),
            json!(25),
            json!(null),
            Attendance::Complete,
        ),
        (
            json!("5"),
            json!("1"),
            json!("25"),
            json!("2026-10-01 00:01:00"),
            Attendance::Today,
        ),
        (
            json!(5),
            json!(1),
            json!(25),
            json!("2026-09-30T16:01:00Z"),
            Attendance::Today,
        ),
        (
            json!(5),
            json!(1),
            json!(25),
            json!("2026-09-30 23:59:59"),
            Attendance::Needed,
        ),
        (
            json!(5),
            json!(0),
            json!(25),
            json!(null),
            Attendance::Needed,
        ),
        (json!(5), json!(0), json!(25), json!(""), Attendance::Needed),
        (
            json!(5),
            json!(1),
            json!(25),
            json!("invalid"),
            Attendance::Unknown,
        ),
        (
            json!(5),
            json!(1),
            json!(25),
            json!("2026-10-02 01:00:00"),
            Attendance::Unknown,
        ),
        (
            json!(5),
            json!(null),
            json!(25),
            json!(null),
            Attendance::Unknown,
        ),
        (
            json!(5),
            json!(-1),
            json!(25),
            json!(null),
            Attendance::Unknown,
        ),
    ] {
        let p = CourseProgress::parse(
            &json!({"signin":weight,"signin_cnt":count,"signin_total":total,"signin_time":last}),
            day,
        );
        assert_eq!(p.attendance, want);
    }
    assert_eq!(
        CourseProgress::parse(&json!({"signin":5,"signin_cnt":0,"signin_total":25}), day)
            .attendance,
        Attendance::Unknown
    );
}

fn ready(service: &Service, ids: &[&str]) {
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = ids.iter().map(|id| course(id)).collect();
    });
}

#[tokio::test]
async fn concurrent_progress_requests_coalesce_and_keep_the_global_limit() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service, &["42", "43", "44", "45", "46"]);
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let (a, p, c) = (active.clone(), peak.clone(), counts.clone());
    let server = Server::new(move |request| {
        let id = request.fields()["cid"].clone();
        *c.lock().unwrap().entry(id).or_default() += 1;
        let count = a.fetch_add(1, Ordering::SeqCst) + 1;
        p.fetch_max(count, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        a.fetch_sub(1, Ordering::SeqCst);
        ok(json!({"total_progress_value":35,"video_progress_value":70}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    let mut jobs = tokio::task::JoinSet::new();
    for id in ["42", "42", "42", "43", "44", "45", "46"] {
        let service = service.clone();
        jobs.spawn(async move {
            service
                .fetch_progress(id, &CancellationToken::new())
                .await
                .unwrap()
        });
    }
    while let Some(result) = jobs.join_next().await {
        assert_eq!(result.unwrap().total_percent, Some(35.0));
    }
    assert_eq!(counts.lock().unwrap()["42"], 1);
    assert_eq!(peak.load(Ordering::SeqCst), 3);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn errors_preserve_progress_and_old_account_responses_cannot_repopulate_it() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service, &["42"]);
    let fail = Arc::new(AtomicBool::new(false));
    let slow = Arc::new(AtomicBool::new(false));
    let (failure, delay) = (fail.clone(), slow.clone());
    let server = Server::new(move |_| {
        if delay.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(60));
        }
        if failure.load(Ordering::SeqCst) {
            json!({"code":500})
        } else {
            ok(json!({"total_progress_value":35}))
        }
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service
        .fetch_progress("42", &CancellationToken::new())
        .await
        .unwrap();
    fail.store(true, Ordering::SeqCst);
    assert!(service
        .fetch_progress("42", &CancellationToken::new())
        .await
        .is_err());
    assert_eq!(
        service.snapshot().progress["42"]
            .data
            .as_ref()
            .unwrap()
            .total_percent,
        Some(35.0)
    );
    assert!(service.snapshot().progress["42"].error.is_some());
    fail.store(false, Ordering::SeqCst);
    slow.store(true, Ordering::SeqCst);
    let other = service.clone();
    let request =
        tokio::spawn(async move { other.fetch_progress("42", &CancellationToken::new()).await });
    tokio::time::sleep(Duration::from_millis(10)).await;
    {
        let mut r = service.runtime.lock().unwrap();
        r.epoch += 1;
        r.session_cancel.cancel();
        r.session_cancel = CancellationToken::new();
        r.snapshot.user.as_mut().unwrap().id = "8".into();
        r.snapshot.progress.clear();
    }
    assert_eq!(request.await.unwrap(), Err(Error::Cancelled));
    assert!(service.snapshot().progress.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn adding_existing_tasks_is_idempotent_for_every_status() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service, &["42"]);
    for status in [
        TaskStatus::Queued,
        TaskStatus::Running,
        TaskStatus::Paused,
        TaskStatus::Stopped,
        TaskStatus::Failed,
        TaskStatus::Completed,
        TaskStatus::DryRun,
        TaskStatus::Blocked,
        TaskStatus::SystemPaused,
        TaskStatus::AwaitingLogin,
    ] {
        service.change(|s| {
            s.tasks = vec![CourseTask {
                course: course("42"),
                account_id: "7".into(),
                desired_running: true,
                status: status.clone(),
                message: "keep".into(),
                generation: 9,
                videos: vec![VideoState {
                    resource_id: "101".into(),
                    title: "video".into(),
                    status: TaskStatus::Completed,
                    position: 79.0,
                    duration: Some(80.0),
                    message: String::new(),
                }],
            }]
        });
        let before = serde_json::to_value(&service.snapshot().tasks).unwrap();
        service.start(vec!["42".into(), "42".into()]).unwrap();
        assert_eq!(
            serde_json::to_value(&service.snapshot().tasks).unwrap(),
            before
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn aborted_progress_future_clears_its_loading_state() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service, &["42"]);
    let server = Server::new(|_| {
        std::thread::sleep(Duration::from_millis(100));
        ok(json!({"total_progress_value": 35}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    let other = service.clone();
    let request =
        tokio::spawn(async move { other.fetch_progress("42", &CancellationToken::new()).await });
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !service
        .snapshot()
        .progress
        .get("42")
        .is_some_and(|entry| entry.refreshing)
    {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    request.abort();
    let _ = request.await;
    assert!(!service.snapshot().progress["42"].refreshing);
    assert!(service.snapshot().progress["42"].error.is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn finished_units_are_visible_after_resume_without_parsing_media() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service, &["42"]);
    let server = Server::new(|request| {
        if request.path.starts_with("/home/course/progress") {
            return ok(json!({"video_progress_value":0}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([{"id":1,"children":[{"id":11}]}]));
        }
        assert!(request.path.starts_with("/home/learn/getUnitLearn"));
        ok(
            json!([{"id":101,"finished":1,"title":"already complete","video_pos":"79","video_url":{"hd":{"source":"https://example.invalid/must-not-be-requested.mp4"}}}]),
        )
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.start(vec!["42".into()]).unwrap();
    tests::wait_finished(&service).await;
    assert_eq!(
        service.snapshot().tasks[0].videos[0].status,
        TaskStatus::Completed
    );
    assert_eq!(service.snapshot().tasks[0].videos[0].duration, None);
    service
        .control(vec!["42".into()], TaskAction::Resume)
        .unwrap();
    tests::wait_finished(&service).await;
    assert_eq!(service.snapshot().tasks[0].videos.len(), 1);
    assert_eq!(service.snapshot().tasks[0].videos[0].position, 79.0);
    assert_eq!(
        service_test(root.clone()).snapshot().tasks[0].videos.len(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}
