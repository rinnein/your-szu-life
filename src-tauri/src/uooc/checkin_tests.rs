use super::{
    checkin::{CheckinStatus, Job},
    checkin_view::{Async, Factory, Page},
    progress::{Attendance, CourseProgress, ProgressEntry},
    tests::{course, ok, root, service_test, Server},
    *,
};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn payload(signed: bool) -> Value {
    let date = if signed {
        progress::today()
    } else {
        progress::today().pred_opt().unwrap()
    };
    json!({"total_progress_value":20,"video_progress_value":0,"signin":5,"signin_cnt":if signed {2}else{1},"signin_total":25,"signin_time":format!("{date} 00:00:01")})
}
#[derive(Default)]
struct FakeData {
    signed: Mutex<HashSet<String>>,
    opened: Mutex<Vec<(String, bool)>>,
    active: AtomicUsize,
    peak: AtomicUsize,
    closed: AtomicUsize,
    fail_open: AtomicBool,
    ready: AtomicBool,
    sign_on_sync: AtomicBool,
}
struct FakeFactory(Arc<FakeData>);
struct FakePage {
    data: Arc<FakeData>,
    cid: String,
}
impl Drop for FakePage {
    fn drop(&mut self) {
        self.data.active.fetch_sub(1, Ordering::SeqCst);
        self.data.closed.fetch_add(1, Ordering::SeqCst);
    }
}
impl Page for FakePage {
    fn ready(&self) -> Async<'_, Result<bool, String>> {
        Box::pin(async { Ok(self.data.ready.load(Ordering::SeqCst)) })
    }
    fn synchronize(&self, _service: Arc<Service>, _job: Job) -> Async<'_, Result<(), String>> {
        Box::pin(async {
            if self.data.sign_on_sync.load(Ordering::SeqCst) {
                self.data.signed.lock().unwrap().insert(self.cid.clone());
            }
            Ok(())
        })
    }
}
impl Factory for FakeFactory {
    fn open(&self, _service: Arc<Service>, job: Job) -> Async<'_, Result<Box<dyn Page>, String>> {
        Box::pin(async move {
            self.0
                .opened
                .lock()
                .unwrap()
                .push((job.cid.clone(), job.visible));
            if self.0.fail_open.load(Ordering::SeqCst) {
                return Err("模拟加载失败".into());
            }
            let count = self.0.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.0.peak.fetch_max(count, Ordering::SeqCst);
            Ok(Box::new(FakePage {
                data: self.0.clone(),
                cid: job.cid,
            }) as Box<dyn Page>)
        })
    }
}
fn fixture() -> (Arc<Service>, Arc<FakeData>, Server, PathBuf) {
    let root = root();
    let service = service_test(root.clone());
    let data = Arc::new(FakeData::default());
    data.ready.store(true, Ordering::SeqCst);
    data.sign_on_sync.store(true, Ordering::SeqCst);
    let mock = data.clone();
    let server = Server::new(move |request| {
        assert!(
            request.path.starts_with("/home/course/progress"),
            "签到测试不得直接提交签到或视频上报"
        );
        let signed = mock
            .signed
            .lock()
            .unwrap()
            .contains(&request.fields()["cid"]);
        ok(payload(signed))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    *service.checkin_factory.lock().unwrap() = Some(Arc::new(FakeFactory(data.clone())));
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42"), course("43")];
    });
    (service, data, server, root)
}
async fn idle(service: &Service) {
    let deadline = Instant::now() + Duration::from_secs(35);
    while service.runtime.lock().unwrap().checkins.pumping {
        assert!(Instant::now() < deadline, "签到队列未结束");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn checkin_queue_is_serial_deduplicated_and_preserves_automatic_manual_modes() {
    let (service, data, _server, root) = fixture();
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
        .enqueue_checkins(
            vec!["43".into(), "43".into()],
            true,
            "feature-parent".into(),
        )
        .unwrap();
    idle(&service).await;
    assert_eq!(
        *data.opened.lock().unwrap(),
        vec![("42".into(), false), ("43".into(), true)]
    );
    assert_eq!(data.peak.load(Ordering::SeqCst), 1);
    assert_eq!(data.closed.load(Ordering::SeqCst), 2);
    assert!(service
        .snapshot()
        .checkins
        .values()
        .all(|s| s.status == CheckinStatus::Succeeded));
    assert!(service
        .snapshot()
        .progress
        .values()
        .all(|p| p.data.as_ref().unwrap().attendance == Attendance::Today));
    service.change(|s| {
        s.courses = vec![course("42")];
        s.progress.get_mut("42").unwrap().data =
            Some(CourseProgress::parse(&payload(false), progress::today()));
    });
    service.enqueue_automatic_checkins();
    idle(&service).await;
    assert_eq!(data.opened.lock().unwrap().len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn checkin_rechecks_eligibility_and_does_not_retry_failed_automatic_attempts() {
    let (service, data, _server, root) = fixture();
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
    data.fail_open.store(true, Ordering::SeqCst);
    service.enqueue_automatic_checkins();
    idle(&service).await;
    assert_eq!(
        service.snapshot().checkins["42"].status,
        CheckinStatus::Failed
    );
    service.enqueue_automatic_checkins();
    idle(&service).await;
    assert_eq!(data.opened.lock().unwrap().len(), 1);
    data.signed.lock().unwrap().insert("42".into());
    service
        .enqueue_checkins(vec!["42".into()], true, "main".into())
        .unwrap();
    idle(&service).await;
    assert_eq!(
        service.snapshot().checkins["42"].status,
        CheckinStatus::Skipped
    );
    assert_eq!(data.opened.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn closing_manual_window_cancels_only_checkin_and_releases_the_page() {
    let (service, data, _server, root) = fixture();
    data.ready.store(false, Ordering::SeqCst);
    let video_token = CancellationToken::new();
    service
        .runtime
        .lock()
        .unwrap()
        .workers
        .insert("43".into(), (1, video_token.clone()));
    service
        .enqueue_checkins(vec!["42".into()], true, "feature-parent".into())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while data.active.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    checkin::window_closed(&service, "feature-parent");
    idle(&service).await;
    assert_eq!(
        service.snapshot().checkins["42"].status,
        CheckinStatus::Cancelled
    );
    assert_eq!(data.closed.load(Ordering::SeqCst), 1);
    assert!(!video_token.is_cancelled());
    service.runtime.lock().unwrap().workers.clear();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn page_readiness_and_elapsed_time_alone_are_not_signin_success() {
    let (service, data, _server, root) = fixture();
    data.sign_on_sync.store(false, Ordering::SeqCst);
    let job = Job {
        id: "test-job".into(),
        cid: "42".into(),
        account: "7".into(),
        epoch: 0,
        visible: false,
        parent: "main".into(),
        cancel: CancellationToken::new(),
    };
    let page = FakeFactory(data.clone())
        .open(service.clone(), job.clone())
        .await
        .unwrap();
    let before = CourseProgress::parse(&payload(false), progress::today());
    assert!(tokio::time::timeout(
        Duration::from_millis(60),
        service.verify_checkin(
            &job,
            &before,
            &*page,
            Duration::ZERO,
            Duration::from_millis(5)
        )
    )
    .await
    .is_err());
    data.sign_on_sync.store(true, Ordering::SeqCst);
    assert!(service
        .verify_checkin(
            &job,
            &before,
            &*page,
            Duration::ZERO,
            Duration::from_millis(5)
        )
        .await
        .unwrap());
    drop(page);
    assert_eq!(data.closed.load(Ordering::SeqCst), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn course_window_urls_and_closing_callbacks_stay_bound_to_the_course() {
    assert!(checkin_view::is_course_url(
        &checkin_view::course_url("1588976494").unwrap(),
        "1588976494"
    ));
    for url in [
        "http://www.uooc.net.cn/home/course/42",
        "https://www.uooc.net.cn.evil.test/home/course/42",
        "https://www.uooc.net.cn/user/login",
        "https://www.uooc.net.cn/home/course/43",
    ] {
        assert!(!checkin_view::is_course_url(&url.parse().unwrap(), "42"));
    }
    assert!(checkin_view::course_url("../42").is_err());
    let root = root();
    let service = service_test(root.clone());
    let job = Job {
        id: "uooc-checkin-test".into(),
        cid: "42".into(),
        account: "7".into(),
        epoch: 0,
        visible: true,
        parent: "main".into(),
        cancel: CancellationToken::new(),
    };
    {
        let mut r = service.runtime.lock().unwrap();
        r.checkins.active = Some(job.clone());
        r.checkins.closing.insert(job.id.clone());
    }
    checkin::window_closed(&service, &job.id);
    assert!(!job.cancel.is_cancelled());
    checkin::window_closed(&service, "main");
    assert!(job.cancel.is_cancelled());
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
            id: "checkin-expiry".into(),
            cid: "43".into(),
            account: "7".into(),
            epoch: 0,
            visible: false,
            parent: "main".into(),
            cancel: checkin_cancel.clone(),
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
