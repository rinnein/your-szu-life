use super::*;
use crate::storage::testing::MemoryCredentialStore;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

pub(super) fn root() -> PathBuf {
    std::env::temp_dir().join(format!("szu-uooc-test-{}", uuid::Uuid::new_v4()))
}
pub(super) fn course(cid: &str) -> Course {
    Course {
        id: cid.into(),
        title: format!("课程 {cid}"),
        semester: "秋季".into(),
        organization: "大学".into(),
        weeks: "5/7".into(),
        end_date: String::new(),
        status: "31".into(),
    }
}
pub(super) fn service_test(root: PathBuf) -> Arc<Service> {
    Service::new(None, Arc::new(MemoryCredentialStore::default()), root).unwrap()
}

#[test]
fn list_uses_status_and_cycle_id_not_weeks_or_parent() {
    let rows = json!({"data": [
        {"id":1588976494_u64,"parent":1140106027,"status":"31","parent_name":"人工智能","name":"2026秋季","weeks":"5/7","end_time":"2026/10/14"},
        {"id":669673711,"parent":1327274410,"status":31,"parent_name":"艺术的星空","weeks":"99/17"},
        {"id":2000000001_u64,"parent":1234567890,"status":"33","parent_name":"复习模式课程","name":"2026秋季","weeks":"16/16","end_time":"2026/12/31"},
        {"id":1677434615,"status":"35","weeks":"10/10"},
        {"id":422912032,"status":"34","weeks":"15/15"},
        {"id":1065104563,"status":"31","parent_name":"创新领航讲座","weeks":"未知"},
        {"id":999,"status":40}, {"id":1000,"status":0}
    ],"page":{"pages":"2"}});
    let (courses, pages) = model::courses_page(&rows).unwrap();
    assert_eq!(pages, 2);
    assert_eq!(
        courses.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["1588976494", "669673711", "2000000001", "1065104563"]
    );
    assert_eq!(courses[0].title, "人工智能");
    assert_eq!(courses[0].semester, "2026秋季");
    assert_eq!(courses[0].end_date, "2026/10/14");
    assert_eq!(courses[0].status, "31");
    assert_eq!(courses[2].title, "复习模式课程");
    assert_eq!(courses[2].status, "33");
    assert_eq!(
        serde_json::to_value(&courses[0]).unwrap()["endDate"],
        "2026/10/14"
    );
    assert_eq!(
        serde_json::to_value(&courses[2]).unwrap()["status"],
        "33"
    );
}

#[test]
fn course_schedule_metadata_refresh_is_compatible_with_saved_tasks() {
    let legacy: Course = serde_json::from_value(
        json!({"id":"42","title":"课程","semester":"秋季","organization":"大学","weeks":"5/7"}),
    )
    .unwrap();
    assert!(legacy.end_date.is_empty());
    let task = |account: &str| CourseTask {
        course: legacy.clone(),
        account_id: account.into(),
        desired_running: false,
        status: TaskStatus::Paused,
        message: "keep".into(),
        videos: vec![],
        generation: 9,
    };
    let mut snapshot = Snapshot {
        tasks: vec![task("7"), task("8")],
        ..Snapshot::default()
    };
    let current = Course {
        weeks: "6/7".into(),
        end_date: "2026/10/14".into(),
        ..legacy.clone()
    };
    snapshot.update_courses(
        User {
            id: "7".into(),
            name: "test".into(),
        },
        vec![current.clone()],
    );
    assert_eq!(snapshot.tasks[0].course, current);
    assert_eq!(snapshot.tasks[0].status, TaskStatus::Paused);
    assert!(!snapshot.tasks[0].desired_running);
    assert_eq!(snapshot.tasks[0].generation, 9);
    assert_eq!(snapshot.tasks[1].course, legacy);
}

#[tokio::test]
async fn duration_cache_single_flight_persists_and_does_not_cache_failures() {
    let root = root();
    let path = root.join("quiz_qa_bank.db");
    let cache = Arc::new(DurationCache::new(path.clone()));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut jobs = vec![];
    for _ in 0..12 {
        let cache = cache.clone();
        let calls = calls.clone();
        jobs.push(tokio::spawn(async move {
            cache
                .get_or_probe("10", "20", &CancellationToken::new(), || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(123.5)
                })
                .await
                .unwrap()
        }));
    }
    for job in jobs {
        assert_eq!(job.await.unwrap(), 123.5);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(cache);
    let cache = DurationCache::new(path.clone());
    // A new URL, new runner (including dry-run), or account is not part of the key.
    assert_eq!(
        cache
            .get_or_probe("10", "20", &CancellationToken::new(), || async {
                panic!("cached media must not be requested again")
            })
            .await
            .unwrap(),
        123.5
    );
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(cache
            .get_or_probe("10", "21", &CancellationToken::new(), || async {
                Ok(value)
            })
            .await
            .is_err());
    }
    assert!(cache
        .get_or_probe("10", "21", &CancellationToken::new(), || async {
            Err("failed".into())
        })
        .await
        .is_err());
    let cancel = CancellationToken::new();
    assert!(cache
        .get_or_probe("10", "21", &cancel, || async {
            cancel.cancel();
            Ok(90.0)
        })
        .await
        .is_err());
    assert_eq!(
        cache
            .get_or_probe("10", "21", &CancellationToken::new(), || async { Ok(99.0) })
            .await
            .unwrap(),
        99.0
    );
    let raw = std::fs::read(&path).unwrap();
    assert!(!raw.windows(5).any(|w| w == b"https") && !raw.windows(6).any(|w| w == b"cookie"));
    std::fs::write(&path, b"corrupt cache").unwrap();
    // 该场景模拟“进程重启后发现文件损坏”：清掉进程内句柄缓存，
    // 让打开逻辑重新走一次损坏隔离流程。
    crate::migrations::db::reset_registry();
    let cache = DurationCache::new(path);
    assert_eq!(
        cache
            .get_or_probe("10", "20", &CancellationToken::new(), || async { Ok(80.0) })
            .await
            .unwrap(),
        80.0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cookies_keep_session_and_host_only_metadata_and_reject_other_domains() {
    let jar = session::load(None).unwrap();
    {
        let mut inner = jar.lock().unwrap();
        inner
            .insert_raw(
                &cookie_store::RawCookie::parse("sid=test-only; Path=/; Secure; HttpOnly").unwrap(),
                &client::ORIGIN.parse().unwrap(),
            )
            .unwrap();
        inner
            .insert_raw(
                &cookie_store::RawCookie::parse("other=test-only; Path=/").unwrap(),
                &"https://example.org/".parse().unwrap(),
            )
            .unwrap();
    }
    let restored = session::load(Some(&session::serialize(&jar).unwrap())).unwrap();
    let cookies: Vec<_> = restored.lock().unwrap().iter_any().cloned().collect();
    assert_eq!(cookies.len(), 1);
    assert_eq!(
        cookies[0].domain,
        cookie_store::CookieDomain::HostOnly("www.uooc.net.cn".into())
    );
    assert_eq!(cookies[0].http_only(), Some(true));
    assert!(!cookies[0].is_persistent());
}

#[derive(Clone)]
pub(super) struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
    pub headers: HashMap<String, String>,
}
impl Request {
    pub fn fields(&self) -> HashMap<String, String> {
        let url = if self.body.is_empty() {
            format!("http://test{}", self.path)
        } else {
            format!("http://test/?{}", self.body)
        };
        reqwest::Url::parse(&url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }
}
pub(super) struct Server {
    pub base: String,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub fn new(handler: impl Fn(Request) -> Value + Send + Sync + 'static) -> Self {
        Self::bytes(move |request| {
            (
                200,
                serde_json::to_vec(&handler(request)).unwrap(),
                "Content-Type: application/json\r\nSet-Cookie: rotated=test-only; Path=/\r\n"
                    .into(),
            )
        })
    }
    pub fn bytes(
        handler: impl Fn(Request) -> (u16, Vec<u8>, String) + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handler = Arc::new(handler);
        let worker = thread::spawn(move || {
            let mut children = vec![];
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let handler = handler.clone();
                        children.push(thread::spawn(move || {
                            if let Some(request) = read_request(&mut stream) {
                                let (status, bytes, extra) = handler(request);
                                let header = format!("HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", bytes.len());
                                let _ = stream.write_all(header.as_bytes()); let _ = stream.write_all(&bytes);
                            }
                        }));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(2)),
                }
            }
            for child in children {
                child.join().unwrap();
            }
        });
        Self {
            base,
            stop,
            thread: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut bytes = vec![];
    let mut chunk = [0; 4096];
    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]);
            let length = header
                .lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return Some(Request {
                    method: header.lines().next()?.split_whitespace().next()?.into(),
                    path: header.lines().next()?.split_whitespace().nth(1)?.into(),
                    body: String::from_utf8_lossy(&bytes[end + 4..end + 4 + length]).into(),
                    headers: header
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().into()))
                        .collect(),
                });
            }
        }
        if bytes.len() > 65536 {
            return None;
        }
    }
}
pub(super) fn ok(data: Value) -> Value {
    json!({"code":1,"data":data})
}

#[tokio::test]
async fn list_pagination_auth_and_subsection_parameters_are_preserved() {
    let requests = Arc::new(Mutex::new(vec![]));
    let observed = requests.clone();
    let server = Server::new(move |request| {
        let fields = request.fields();
        assert_eq!(request.headers["user-agent"], user_agent::DESKTOP);
        observed.lock().unwrap().push(request.path.clone());
        if request.path.starts_with(client::LIST) {
            assert_eq!(fields["type"], "learn");
            assert_eq!(fields["keyword"], "");
            return ok(
                json!({"data":[{"id":1,"parent":999,"status":"31"},{"id": fields["page"].parse::<u64>().unwrap(),"status":31}],"page":{"pages":2}}),
            );
        }
        assert_eq!(fields["cid"], "42");
        assert_eq!(fields["section_id"], "11");
        assert_eq!(fields["subsection_id"], "111");
        if request.path.starts_with("/home/learn/getUnitLearn") {
            assert_eq!(fields["catalog_id"], "111");
            ok(json!([]))
        } else {
            assert_eq!(fields["resource_id"], "1001");
            assert_eq!(fields["video_length"], "99.00");
            ok(json!({"finished":"1"}))
        }
    });
    let client = Client::with_base(session::load(None).unwrap(), &server.base).unwrap();
    let cancel = CancellationToken::new();
    assert_eq!(
        client
            .courses(&cancel)
            .await
            .unwrap()
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
    let video = Video {
        cid: "42".into(),
        chapter: "1".into(),
        section: "11".into(),
        subsection: Some("111".into()),
        resource: "1001".into(),
        title: String::new(),
        source: String::new(),
        position: 0.0,
        task_type: "video".into(),
        task_id: None,
    };
    client.unit(&video, &cancel).await.unwrap();
    assert!(client.mark(&video, 99.0, 50.0, &cancel).await.unwrap());
    assert_eq!(requests.lock().unwrap().len(), 4);
    let server = Server::new(|_| json!({"code":401,"msg":"not logged in"}));
    let client = Client::with_base(session::load(None).unwrap(), &server.base).unwrap();
    assert_eq!(client.courses(&cancel).await, Err(Error::Unauthorized));
}

#[tokio::test]
async fn login_redirects_and_download_redirects_are_distinguished_without_following() {
    for (location, expected) in [
        ("/user/login", Error::Unauthorized),
        ("/app/download", Error::UnexpectedRedirect),
        (
            "https://other.example/user/login",
            Error::UnexpectedRedirect,
        ),
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let server = Server::bytes(move |request| {
            assert_eq!(request.headers["user-agent"], user_agent::DESKTOP);
            observed.fetch_add(1, Ordering::SeqCst);
            (302, vec![], format!("Location: {location}\r\n"))
        });
        let client = Client::with_base(session::load(None).unwrap(), &server.base).unwrap();
        assert_eq!(
            client.courses(&CancellationToken::new()).await,
            Err(expected)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

pub(super) async fn wait_finished(service: &Service) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !service.runtime.lock().unwrap().workers.is_empty() {
        assert!(Instant::now() < deadline, "tasks timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn dry_run_reuses_duration_and_never_marks_then_real_run_shares_global_limit() {
    let root = root();
    let service = service_test(root.clone());
    let concurrent = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(Mutex::new(vec![]));
    let (current, max, observed) = (concurrent.clone(), peak.clone(), calls.clone());
    let server = Server::new(move |request| {
        let fields = request.fields();
        if request.path.starts_with("/home/course/progress") {
            return ok(json!({"video_progress_value":0}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([{"id":1,"children":[{"id":11,"children":[{"id":111}]}]}]));
        }
        if request.path.starts_with("/home/learn/getUnitLearn") {
            let sub = fields.contains_key("subsection_id");
            return ok(
                json!([{"id":if sub {102}else{101},"title":"test video","finished":0,"video_pos":"5.5","video_play_list":[{"source":"https://example.invalid/video.mp4?signature=changed"}]}]),
            );
        }
        assert!(request.path.starts_with("/home/learn/markVideoLearn"));
        let active = current.fetch_add(1, Ordering::SeqCst) + 1;
        max.fetch_max(active, Ordering::SeqCst);
        observed.lock().unwrap().push(fields["cid"].clone());
        thread::sleep(Duration::from_millis(80));
        current.fetch_sub(1, Ordering::SeqCst);
        ok(json!({"finished":1}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42"), course("43")];
    });
    for cid in ["42", "43"] {
        for rid in ["101", "102"] {
            service
                .durations
                .get_or_probe(cid, rid, &CancellationToken::new(), || async { Ok(120.0) })
                .await
                .unwrap();
        }
    }
    service
        .set_settings(Settings {
            concurrency: 2,
            dry_run: true,
            ..Settings::default()
        })
        .unwrap();
    service.start(vec!["42".into(), "43".into()]).unwrap();
    wait_finished(&service).await;
    assert!(calls.lock().unwrap().is_empty());
    assert!(service
        .snapshot()
        .tasks
        .iter()
        .all(|t| t.status == TaskStatus::DryRun));
    service
        .set_settings(Settings {
            concurrency: 2,
            ..Settings::default()
        })
        .unwrap();
    service
        .control(vec!["42".into(), "43".into()], TaskAction::Resume)
        .unwrap();
    wait_finished(&service).await;
    assert_eq!(calls.lock().unwrap().len(), 4);
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    assert!(service
        .snapshot()
        .tasks
        .iter()
        .all(|t| t.status == TaskStatus::Completed));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn persisted_intent_and_generation_prevent_paused_and_old_account_work_from_resuming() {
    let root = root();
    let service = service_test(root.clone());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
        s.tasks = vec![CourseTask {
            course: course("42"),
            account_id: "7".into(),
            desired_running: true,
            status: TaskStatus::Running,
            message: String::new(),
            videos: vec![],
            generation: 3,
        }];
    });
    let token = CancellationToken::new();
    service
        .runtime
        .lock()
        .unwrap()
        .workers
        .insert("42".into(), (3, token.clone()));
    service.control(vec![], TaskAction::Pause).unwrap();
    assert!(token.is_cancelled());
    service.task_change("42", 3, |t| t.status = TaskStatus::Completed);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::Paused);
    let reopened = service_test(root.clone());
    assert!(!reopened.snapshot().tasks[0].desired_running);
    reopened.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "8".into(),
            name: "new account".into(),
        });
        s.courses = vec![course("42")];
        s.tasks[0].desired_running = true;
    });
    reopened.reconcile();
    assert_eq!(reopened.snapshot().tasks[0].status, TaskStatus::Stopped);
    assert!(reopened.runtime.lock().unwrap().workers.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn startup_refresh_is_coalesced_and_runs_even_without_a_saved_cookie() {
    let root = root();
    let service = service_test(root.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let server = Server::new(move |request| {
        assert!(request.path.starts_with(client::LIST));
        observed.fetch_add(1, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(50));
        json!({"code":401})
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    let (a, b) = tokio::join!(service.refresh(), service.refresh());
    a.unwrap();
    b.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.snapshot().session, SessionStatus::LoginRequired);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn failed_cache_write_is_retried_without_reparsing_media() {
    let root = root();
    std::fs::create_dir_all(&root).unwrap();
    let parent = root.join("blocked");
    std::fs::write(&parent, b"not a directory").unwrap();
    let path = parent.join("quiz_qa_bank.db");
    let cache = DurationCache::new(path.clone());
    assert!(cache
        .get_or_probe("1", "2", &CancellationToken::new(), || async { Ok(100.0) })
        .await
        .is_err());
    std::fs::remove_file(&parent).unwrap();
    assert_eq!(
        cache
            .get_or_probe("1", "2", &CancellationToken::new(), || async {
                panic!("must reuse the successful parse")
            })
            .await
            .unwrap(),
        100.0
    );
    assert_eq!(
        DurationCache::new(path)
            .get_or_probe("1", "2", &CancellationToken::new(), || async {
                panic!("must be durable")
            })
            .await
            .unwrap(),
        100.0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_android_service_callbacks_cannot_stop_a_new_run() {
    let root = root();
    let service = service_test(root.clone());
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.background_generation = 2;
        runtime.background_active = true;
        runtime.snapshot.tasks.push(CourseTask {
            course: course("42"),
            account_id: "7".into(),
            desired_running: true,
            status: TaskStatus::Running,
            message: String::new(),
            videos: vec![],
            generation: 9,
        });
        runtime
            .workers
            .insert("42".into(), (9, CancellationToken::new()));
    }
    service.system_pause_generation("old service", Some(1));
    service
        .control_generation(vec![], TaskAction::Stop, Some(1))
        .unwrap();
    assert!(service.snapshot().tasks[0].desired_running);
    assert_eq!(service.runtime.lock().unwrap().workers.len(), 1);
    service.system_pause_generation("system timeout", Some(2));
    assert!(service.snapshot().tasks[0].desired_running);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::SystemPaused);
    assert!(service.runtime.lock().unwrap().workers.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn startup_restores_only_running_courses_after_identity_and_course_checks() {
    let root = root();
    let task = |cid: &str, desired| CourseTask {
        course: course(cid),
        account_id: "7".into(),
        desired_running: desired,
        status: TaskStatus::Paused,
        message: String::new(),
        videos: vec![],
        generation: 0,
    };
    db::UoocStateDatabase::open(&root.join("state.sqlite"))
        .unwrap()
        .save(&SavedState {
            settings: Settings::default(),
            tasks: vec![task("42", true), task("43", false), task("44", true)],
            quizzes: Default::default(),
            discussion: Default::default(),
            qa_api_key: None,
            initial_qa_bootstrapped_accounts: Default::default(),
        })
        .unwrap();
    let service = service_test(root.clone());
    let progress = Arc::new(AtomicUsize::new(0));
    let calls = progress.clone();
    let server = Server::new(move |request| {
        if request.path.starts_with(client::LIST) {
            return ok(
                json!({"data":[{"id":42,"status":31},{"id":43,"status":31},{"id":44,"status":35}],"page":{"pages":1}}),
            );
        }
        if request.path.starts_with("/home/member/user") {
            return ok(json!({"id":7,"name":"test"}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([]));
        }
        assert!(request.path.starts_with("/home/course/progress"));
        assert!(["42", "43"].contains(&request.fields()["cid"].as_str()));
        calls.fetch_add(1, Ordering::SeqCst);
        ok(json!({"video_progress_value":100}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.refresh().await.unwrap();
    wait_finished(&service).await;
    assert!(progress.load(Ordering::SeqCst) >= 2);
    let tasks = service.snapshot().tasks;
    assert_eq!(tasks[0].status, TaskStatus::Completed);
    assert_eq!(tasks[1].status, TaskStatus::Paused);
    assert_eq!(tasks[2].status, TaskStatus::Stopped);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn gated_discovery_finishes_earlier_videos_without_reading_later_sections() {
    let root = root();
    let service = service_test(root.clone());
    let marks = Arc::new(AtomicUsize::new(0));
    let observed = marks.clone();
    let server = Server::new(move |request| {
        if request.path.starts_with("/home/course/progress") {
            return ok(json!({"video_progress_value":0}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([{"id":1,"children":[{"id":11},{"id":12},{"id":13}]}]));
        }
        if request.path.starts_with("/home/learn/getUnitLearn") {
            let section = request.fields()["section_id"].clone();
            assert_ne!(section, "13");
            if section == "12" {
                return json!({"code":600});
            }
            return ok(
                json!([{"id":101,"finished":observed.load(Ordering::SeqCst),"title":"earlier video","video_url":{"hd":{"source":"https://example.invalid/video.mp4"}}}]),
            );
        }
        observed.fetch_add(1, Ordering::SeqCst);
        ok(json!({"finished":1}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.settings.qa_server_url = server.base.clone();
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });
    service
        .durations
        .get_or_probe("42", "101", &CancellationToken::new(), || async {
            Ok(120.0)
        })
        .await
        .unwrap();
    service.start(vec!["42".into()]).unwrap();
    wait_finished(&service).await;
    assert_eq!(marks.load(Ordering::SeqCst), 1);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::Blocked);
    assert_eq!(
        service.snapshot().tasks[0].videos[0].status,
        TaskStatus::Completed
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn tail_requires_server_confirmation_and_pause_stops_future_reports() {
    let root = root();
    let service = service_test(root.clone());
    let marks = Arc::new(AtomicUsize::new(0));
    let observed = marks.clone();
    let server = Server::new(move |request| {
        if request.path.starts_with("/home/course/progress") {
            return ok(json!({"video_progress_value": 0}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([{"id": 1, "children": [{"id": 11}]}]));
        }
        if request.path.starts_with("/home/learn/getUnitLearn") {
            return ok(
                json!([{"id": 101, "finished": 0, "video_pos": "2", "video_url": {"hd": {"source": "https://example.invalid/video.mp4"}}}]),
            );
        }
        assert_eq!(request.fields()["video_pos"], "2.00");
        observed.fetch_add(1, Ordering::SeqCst);
        ok(json!({"finished": 0}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.settings.qa_server_url = server.base.clone();
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });
    service
        .set_settings(Settings {
            report_interval_sec: 1,
            ..Settings::default()
        })
        .unwrap();
    service
        .durations
        .get_or_probe("42", "101", &CancellationToken::new(), || async { Ok(3.0) })
        .await
        .unwrap();
    service.start(vec!["42".into()]).unwrap();
    wait_finished(&service).await;
    assert_eq!(marks.load(Ordering::SeqCst), 3);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::Failed);
    service
        .control(vec!["42".into()], TaskAction::Resume)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while marks.load(Ordering::SeqCst) < 4 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    service.control(vec![], TaskAction::Pause).unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(marks.load(Ordering::SeqCst), 4);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::Paused);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn nested_catalog_quizzes_are_identified_by_icon_list_without_calling_unit() {
    let root = root();
    let service = service_test(root.clone());
    let server = Server::new(move |request| {
        if request.path.starts_with("/home/course/progress") {
            return ok(json!({"video_progress_value": 100}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([
                {
                    "id": 2000,
                    "name": "绪论",
                    "children": [
                        {
                            "id": 2001,
                            "name": "第1节 绪论",
                            "finished": 1,
                            "icon_list": [{"id": 3001, "type": "10", "type_name": "视频"}]
                        },
                        {
                            "id": 2002,
                            "name": "绪论测验",
                            "task_id": 141304783,
                            "finished": 1,
                            "icon_list": [{"id": 3002, "type": "80", "type_name": "测验"}]
                        }
                    ]
                }
            ]));
        }
        if request.path.starts_with("/home/learn/getUnitLearn") {
            let section = request.fields()["section_id"].clone();
            assert_ne!(section, "2002", "测验节点不应调用 getUnitLearn 视频接口");
            return ok(json!([
                {
                    "id": 101,
                    "finished": 1,
                    "title": "第1节 绪论",
                    "video_pos": "100",
                    "video_url": {"hd": {"source": "https://example.invalid/video.mp4"}}
                }
            ]));
        }
        ok(json!({"code": 1}))
    });
    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });
    service.start(vec!["42".into()]).unwrap();
    wait_finished(&service).await;
    let tasks = service.snapshot().tasks;
    assert_eq!(tasks.len(), 1);
    let videos = &tasks[0].videos;
    assert_eq!(videos.len(), 2);
    let quiz_item = videos.iter().find(|v| v.task_type.as_deref() == Some("quiz")).expect("需包含识别出的测验项");
    assert_eq!(quiz_item.resource_id, "3002");
    assert_eq!(quiz_item.title, "绪论测验");
    assert_eq!(quiz_item.task_id.as_deref(), Some("141304783"));
    assert_eq!(quiz_item.status, TaskStatus::Completed);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn deleting_tasks_cancels_workers_and_removes_from_snapshot() {
    let root = root();
    let service = service_test(root.clone());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42"), course("43")];
        s.tasks = vec![
            CourseTask {
                course: course("42"),
                account_id: "7".into(),
                desired_running: true,
                status: TaskStatus::Paused,
                message: String::new(),
                generation: 1,
                videos: vec![],
            },
            CourseTask {
                course: course("43"),
                account_id: "7".into(),
                desired_running: false,
                status: TaskStatus::Completed,
                message: String::new(),
                generation: 1,
                videos: vec![],
            },
        ];
    });

    assert_eq!(service.snapshot().tasks.len(), 2);

    // 删除单个任务 "42"
    service.control(vec!["42".into()], TaskAction::Delete).unwrap();
    let tasks = service.snapshot().tasks;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].course.id, "43");

    // 删除剩余任务 "43"
    service.control(vec!["43".into()], TaskAction::Delete).unwrap();
    assert!(service.snapshot().tasks.is_empty());

    // 重新打开服务，确认持久化存储中任务也已移除
    let reopened = service_test(root.clone());
    assert!(reopened.snapshot().tasks.is_empty());

    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn build_local_bank_discovers_all_course_quizzes_via_task_list_logic() {
    let root = root();
    let service = service_test(root.clone());
    let server = Server::new(move |request| {
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([
                {
                    "id": 1,
                    "name": "绪论",
                    "children": [
                        {
                            "id": 101,
                            "name": "绪论测验",
                            "task_id": 901,
                            "finished": 1,
                            "icon_list": [{"id": 501, "type": "80", "type_name": "测验"}]
                        }
                    ]
                },
                {
                    "id": 2,
                    "name": "第一章",
                    "children": [
                        {
                            "id": 102,
                            "name": "第一章测验",
                            "task_id": 902,
                            "finished": 1,
                            "icon_list": [{"id": 502, "type": "80", "type_name": "测验"}]
                        }
                    ]
                }
            ]));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            let tid = request.fields().get("tid").cloned().unwrap_or_default();
            return json!({
                "code": 1,
                "data": {
                    "questions": [
                        {
                            "id": format!("q_{tid}"),
                            "question": format!("题目内容 {tid}"),
                            "type": "10",
                            "options_app": [
                                {"key": "A", "value": "正确答案"}
                            ],
                            "answer": "A"
                        }
                    ]
                }
            });
        }
        ok(json!({"code": 1}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.settings.qa_server_url = server.base.clone();
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });

    let count = service.build_local_bank().await.unwrap();
    assert_eq!(count, 2);

    let bank = service.get_qa_db().await.unwrap().load_all_bank_questions().await.unwrap();
    assert_eq!(bank.len(), 2);
    assert!(bank.values().any(|q| q.tid == "901"));
    assert!(bank.values().any(|q| q.tid == "902"));

    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn build_local_bank_creates_new_tid_on_server_even_if_not_found_in_course_and_logs() {
    let root = root();
    let service = service_test(root.clone());

    let upload_batch_calls = Arc::new(AtomicUsize::new(0));
    let upload_batch_calls_clone = upload_batch_calls.clone();
    let upload_bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
    let upload_bodies_clone = upload_bodies.clone();

    // Mock uooc-qa-share server
    let qa_server = Server::new(move |request| {
        if request.path == "/api/courses/42" && request.method == "GET" {
            // Server has no task groups for this course yet
            return json!({
                "course": { "cid": "42", "name": "测试课程 42" },
                "taskGroups": []
            });
        }
        if request.path == "/api/courses/42/tasks/901" && request.method == "GET" {
            // Returns the exact error: "Task group not found in this course"
            return json!({
                "error": "Task group not found in this course"
            });
        }
        if request.path == "/api/upload/batch" && request.method == "POST" {
            upload_batch_calls_clone.fetch_add(1, Ordering::SeqCst);
            upload_bodies_clone.lock().unwrap().push(request.body.clone());
            return json!({
                "ok": true,
                "results": [{ "question_id": "q_901", "action": "added_question" }]
            });
        }
        json!({ "error": "not found" })
    });

    // Mock UOOC server
    let server = Server::new(move |request| {
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([
                {
                    "id": 1,
                    "name": "第一章",
                    "children": [
                        {
                            "id": 101,
                            "name": "测验一",
                            "task_id": 901,
                            "finished": 1,
                            "icon_list": [{"id": 501, "type": "80", "type_name": "测验"}]
                        }
                    ]
                }
            ]));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            return json!({
                "code": 1,
                "data": {
                    "questions": [
                        {
                            "id": "q_901",
                            "question": "测试题目 901",
                            "type": "10",
                            "options_app": [{"key": "A", "value": "选项A"}],
                            "answer": "A"
                        }
                    ]
                }
            });
        }
        ok(json!({"code": 1}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
        s.settings.qa_server_url = qa_server.base.clone();
    });

    let count = service.build_local_bank().await.unwrap();
    assert_eq!(count, 1);

    // Verify upload/batch was called to create the new tid on the server
    assert_eq!(upload_batch_calls.load(Ordering::SeqCst), 1);
    let bodies = upload_bodies.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    assert_eq!(payload["tid"], "901");
    assert_eq!(payload["cid"], "42");
    assert_eq!(payload["questions"][0]["question_id"], "q_901");

    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn build_local_bank_fetches_exam_view_for_completed_quiz_and_records_standard_answers() {
    let root = root();
    let service = service_test(root.clone());

    let exam_view_called = Arc::new(AtomicUsize::new(0));
    let exam_view_called_clone = exam_view_called.clone();

    // Mock UOOC server
    let server = Server::new(move |request| {
        if request.path.starts_with("/home/learn/getCatalogList") {
            return ok(json!([
                {
                    "id": 1,
                    "name": "第一章",
                    "children": [
                        {
                            "id": 101,
                            "name": "已考测验",
                            "task_id": 999,
                            "finished": 1,
                            "icon_list": [{"id": 501, "type": "80", "type_name": "测验"}]
                        }
                    ]
                }
            ]));
        }
        // /exam/getTaskPaper only returns questions, without answers
        if request.path.starts_with("/exam/getTaskPaper") {
            return json!({
                "code": 1,
                "data": {
                    "questions": [
                        {
                            "id": "q_999",
                            "question": "我国第一部现代白话文小说是？",
                            "type": "10",
                            "options_app": [
                                {"key": "A", "value": "狂人日记"},
                                {"key": "B", "value": "阿Q正传"}
                            ],
                            "answer": ""
                        }
                    ]
                }
            });
        }
        // /exam/view returns the completed exam with user's submitted answer and standard answer!
        if request.path.starts_with("/exam/view") {
            exam_view_called_clone.fetch_add(1, Ordering::SeqCst);
            return json!({
                "code": 1,
                "data": {
                    "questions": [
                        {
                            "id": "q_999",
                            "stem": "我国第一部现代白话文小说是？",
                            "type": "single",
                            "options_app": [
                                {"key": "A", "value": "狂人日记"},
                                {"key": "B", "value": "阿Q正传"}
                            ],
                            "user_answer": "A",
                            "standard_answer": "A",
                            "is_right": 1
                        }
                    ]
                }
            });
        }
        ok(json!({"code": 1}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.settings.qa_server_url = server.base.clone();
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });

    let count = service.build_local_bank().await.unwrap();
    assert_eq!(count, 1);

    // Verify /exam/view was called because the quiz is finished
    assert!(exam_view_called.load(Ordering::SeqCst) >= 1);

    let bank = service.get_qa_db().await.unwrap().load_all_bank_questions().await.unwrap();
    assert_eq!(bank.len(), 1);
    let question = bank.get("999:q_999").expect("question should exist");
    // Standard answer MUST NOT be empty!
    assert!(!question.correct_answers.is_empty());
    assert_eq!(question.correct_answers, vec!["狂人日记".to_string()]);

    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn test_uooc_exam_lifecycle_list_paper_draft_commit_heartbeat() {
    let root = root();
    let service = service_test(root.clone());

    let saved_called = Arc::new(AtomicUsize::new(0));
    let saved_called_clone = saved_called.clone();
    let committed_called = Arc::new(AtomicUsize::new(0));
    let committed_called_clone = committed_called.clone();
    let heartbeat_called = Arc::new(AtomicUsize::new(0));
    let heartbeat_called_clone = heartbeat_called.clone();

    let server = Server::new(move |request| {
        if request.path.starts_with("/home/task/examList") {
            return ok(json!({
                "data": [
                    {
                        "id": 888,
                        "title": "期末考试测试",
                        "start_time": "2026-10-01 09:00:00",
                        "end_time": "2026-10-30 18:00:00",
                        "limit_time": 90,
                        "status": 1,
                        "chance": 1,
                        "submit_count": 0,
                        "task_id": 999
                    }
                ],
                "page": { "pages": 1 }
            }));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            return json!({
                "code": 1,
                "data": {
                    "questions": [
                        {
                            "id": "q_exam_1",
                            "question": "测试考题：1+1等于几？",
                            "type": "10",
                            "options_app": [
                                { "key": "A", "value": "1" },
                                { "key": "B", "value": "2" }
                            ],
                            "answer": "B"
                        }
                    ]
                }
            });
        }
        if request.path.starts_with("/exam/taskInfo") {
            return ok(json!({
                "title": "期末考试测试",
                "limit_time": 90,
                "status": 1
            }));
        }
        if request.path.starts_with("/exam/save") {
            saved_called_clone.fetch_add(1, Ordering::SeqCst);
            return ok(json!({ "msg": "草稿已保存" }));
        }
        if request.path.starts_with("/exam/commit") {
            committed_called_clone.fetch_add(1, Ordering::SeqCst);
            return ok(json!({ "msg": "考试提交成功" }));
        }
        if request.path.starts_with("/home/cheatrecord/getAlert") {
            heartbeat_called_clone.fetch_add(1, Ordering::SeqCst);
            return ok(json!([]));
        }
        ok(json!({}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });

    // 1. 测试考试列表
    let exams = service.list_exams(Some("42")).await.unwrap();
    assert_eq!(exams.len(), 1);
    assert_eq!(exams[0].title, "期末考试测试");
    assert_eq!(exams[0].limit_time, 90);
    assert_eq!(exams[0].task_id, "999");

    // 2. 测试试卷获取
    let paper = service.get_exam_paper("42", "999").await.unwrap();
    assert_eq!(paper.questions.len(), 1);
    assert_eq!(paper.questions[0].text, "测试考题：1+1等于几？");
    assert_eq!(paper.limit_time, 90);

    // 3. 测试草稿暂存
    let mut answers = super::quiz::model::Answers::new();
    answers.insert("q_exam_1".into(), vec!["B".into()]);
    service.save_exam_draft("42", "999", answers.clone()).await.unwrap();
    assert_eq!(saved_called.load(Ordering::SeqCst), 1);

    // 4. 测试防作弊心跳保持
    let heartbeat_ok = service.heartbeat_exam("42", "999").await.unwrap();
    assert!(heartbeat_ok);
    assert_eq!(heartbeat_called.load(Ordering::SeqCst), 1);

    // 5. 测试正式交卷
    let submit_msg = service.submit_exam("42", "999", answers, Some(paper.questions)).await.unwrap();
    assert_eq!(submit_msg, "考试提交成功");
    assert_eq!(committed_called.load(Ordering::SeqCst), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_uooc_homework_and_eval_lifecycle() {
    let root = root();
    let service = service_test(root.clone());
    let eval_submitted = Arc::new(AtomicUsize::new(0));
    let eval_submitted_clone = eval_submitted.clone();

    let server = Server::new(move |request| {
        if request.path.starts_with("/home/task/existEval") {
            return ok(json!({ "data": true }));
        }
        if request.path.starts_with("/home/task/homeworkList") {
            return ok(json!({
                "data": [
                    {
                        "id": 101,
                        "title": "课后作业第1次",
                        "start_time": "2026-10-01 00:00:00",
                        "end_time": "2026-10-25 23:59:59",
                        "status": 1,
                        "chance": 2,
                        "submit_count": 0,
                        "task_id": 202
                    }
                ],
                "page": { "pages": 1 }
            }));
        }
        if request.path.starts_with("/home/task/evalList") {
            return ok(json!({
                "data": [
                    {
                        "tid": 202,
                        "title": "课后作业第1次互评",
                        "sub_list": [
                            {
                                "eval_id": 501,
                                "student_task_id": 202,
                                "content": "这是同学A提交的作业内容",
                                "status": 0
                            }
                        ]
                    }
                ]
            }));
        }
        if request.path.starts_with("/home/task/submitEvalScore") {
            eval_submitted_clone.fetch_add(1, Ordering::SeqCst);
            return ok(json!({ "msg": "互评打分成功" }));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            return json!({
                "code": 1,
                "data": {
                    "questions": [
                        {
                            "id": "q_hw_1",
                            "question": "作业简答题：请简述什么是HTTP协议？",
                            "type": "40"
                        }
                    ]
                }
            });
        }
        if request.path.starts_with("/exam/taskInfo") {
            return ok(json!({
                "title": "课后作业第1次",
                "limit_time": 0,
                "status": 1
            }));
        }
        ok(json!({}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });

    // 1. 验证作业列表
    let homeworks = service.list_homeworks(Some("42")).await.unwrap();
    assert_eq!(homeworks.len(), 1);
    assert_eq!(homeworks[0].title, "课后作业第1次");
    assert!(homeworks[0].has_eval);

    // 2. 验证试卷主观题类型解析
    let paper = service.get_exam_paper("42", "202").await.unwrap();
    assert_eq!(paper.questions.len(), 1);
    assert_eq!(paper.questions[0].kind, super::quiz::model::Kind::Subjective);

    // 3. 验证互评任务拉取
    let evals = service.list_evals(Some("42")).await.unwrap();
    assert_eq!(evals.len(), 1);
    assert_eq!(evals[0].title, "课后作业第1次互评");
    assert_eq!(evals[0].submissions.len(), 1);
    assert_eq!(evals[0].submissions[0].content, "这是同学A提交的作业内容");

    // 4. 验证互评提交
    let res = service
        .submit_eval("42", "202", "501", 95.0, "作业完成得非常规范详细")
        .await
        .unwrap();
    assert_eq!(res, "互评打分成功");
    assert_eq!(eval_submitted.load(Ordering::SeqCst), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_uooc_completed_quiz_loads_via_view_api() {
    let root = root();
    let service = service_test(root.clone());
    let view_called = Arc::new(AtomicUsize::new(0));
    let view_called_clone = view_called.clone();

    let server = Server::new(move |request| {
        if request.path.starts_with("/exam/taskInfo") {
            return ok(json!({
                "title": "已完成章节测验",
                "status": 2
            }));
        }
        if request.path.starts_with("/exam/view") {
            view_called_clone.fetch_add(1, Ordering::SeqCst);
            return json!({
                "code": 1,
                "data": {
                    "title": "已完成章节测验",
                    "questions": [
                        {
                            "id": "q_completed_1",
                            "question": "OSI七层模型的第一层是？",
                            "type": "10",
                            "options": [
                                { "id": "A", "value": "物理层" },
                                { "id": "B", "value": "数据链路层" }
                            ],
                            "answer": "A",
                            "user_answer": "A"
                        }
                    ],
                    "score": 100
                }
            });
        }
        ok(json!({}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });

    let paper = service.get_exam_paper("42", "task_view_999").await.unwrap();
    assert_eq!(paper.title, "已完成章节测验");
    assert!(paper.is_submitted);
    assert_eq!(paper.questions.len(), 1);
    assert_eq!(paper.questions[0].text, "OSI七层模型的第一层是？");
    assert_eq!(view_called.load(Ordering::SeqCst), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_default_qa_server_url_and_normalization() {
    assert_eq!(crate::uooc::model::DEFAULT_QA_SERVER_URL, "https://uooc.rinne.in");
    let settings = Settings::default();
    assert_eq!(settings.qa_server_url, crate::uooc::model::DEFAULT_QA_SERVER_URL);

    // Empty URL falls back to default constant
    assert_eq!(
        model::normalize_qa_url("").unwrap(),
        crate::uooc::model::DEFAULT_QA_SERVER_URL
    );
    assert_eq!(
        model::normalize_qa_url("   ").unwrap(),
        crate::uooc::model::DEFAULT_QA_SERVER_URL
    );

    // Trims trailing slashes
    assert_eq!(
        model::normalize_qa_url("https://uooc.rinne.in/").unwrap(),
        "https://uooc.rinne.in"
    );
    assert_eq!(
        model::normalize_qa_url("http://127.0.0.1:8080///").unwrap(),
        "http://127.0.0.1:8080"
    );
}

#[tokio::test]
async fn test_review_mode_course_lifecycle_and_exam_visibility() {
    let root = root();
    let service = service_test(root.clone());

    let server = Server::new(move |request| {
        if request.path.starts_with("/home/task/examList") {
            return ok(json!({
                "data": [
                    {
                        "id": 9999,
                        "title": "复习课程期末大考",
                        "start_time": "2026-10-01 09:00:00",
                        "end_time": "2026-10-30 18:00:00",
                        "limit_time": 120,
                        "status": 1,
                        "chance": 1,
                        "submit_count": 0,
                        "task_id": 9999
                    }
                ],
                "page": { "pages": 1 }
            }));
        }
        ok(json!({}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());

    let mut review_course = course("333");
    review_course.status = "33".into();
    review_course.title = "复习模式课程".into();

    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![review_course.clone()];
    });

    // 1. 处于复习模式的课程不能启动视频任务
    let start_result = service.start(vec!["333".into()]);
    assert!(start_result.is_err());
    assert!(start_result.unwrap_err().contains("处于复习模式"));

    // 2. 处于复习模式的课程能够正常获取考试任务
    let exams = service.list_exams(None).await.unwrap();
    assert_eq!(exams.len(), 1);
    assert_eq!(exams[0].title, "复习课程期末大考");
    assert_eq!(exams[0].cid, "333");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_uooc_exam_list_real_unpaginated_payload_shape() {
    let root = root();
    let service = service_test(root.clone());

    let server = Server::new(move |request| {
        if request.path.starts_with("/home/task/examList") {
            return json!({
                "code": 1,
                "msg": "",
                "data": [
                    {
                        "id": 1231294128_u64,
                        "type": "30",
                        "chapter_id": "0",
                        "section_id": "0",
                        "name": "人工智能行业与创新创业对接范式",
                        "end_time": "2026-10-24 22:00:00",
                        "time_limit": "60",
                        "start_time": "2026-10-19 15:00:00",
                        "allow_view_paper": "0",
                        "allow_view_score": "1",
                        "is_task": "1",
                        "answer_hint": "0",
                        "answer_review": "0",
                        "eval_status": "0",
                        "class_ids": [],
                        "order": 0,
                        "chapter_name": "综合测验",
                        "upid": 0,
                        "status_code": 0,
                        "commit_time": "",
                        "do_time": "",
                        "status": "未提交",
                        "expire": 0,
                        "tick": 3600,
                        "state": {
                            "view": false,
                            "do": true,
                            "eval": false
                        }
                    }
                ]
            });
        }
        ok(json!({}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());

    let mut ai_course = course("1588976494");
    ai_course.title = "人工智能".into();

    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![ai_course];
    });

    // 1. 指定 cid 获取考试列表
    let exams = service.list_exams(Some("1588976494")).await.unwrap();
    assert_eq!(exams.len(), 1);
    let exam = &exams[0];
    assert_eq!(exam.id, "1231294128");
    assert_eq!(exam.task_id, "1231294128");
    assert_eq!(exam.title, "人工智能行业与创新创业对接范式");
    assert_eq!(exam.cid, "1588976494");
    assert_eq!(exam.course_name, "人工智能");
    assert_eq!(exam.start_time, "2026-10-19 15:00:00");
    assert_eq!(exam.end_time, "2026-10-24 22:00:00");
    assert_eq!(exam.limit_time, 60);
    assert_eq!(exam.status, 0);
    assert_eq!(exam.status_text, "未提交");
    assert!(exam.can_do);
    assert!(!exam.can_view);
    assert_eq!(exam.chapter_text, Some("综合测验".to_string()));

    // 2. 未指定 cid 获取所有在修课程考试列表
    let all_exams = service.list_exams(None).await.unwrap();
    assert_eq!(all_exams.len(), 1);
    assert_eq!(all_exams[0].id, "1231294128");

    // 3. 当指定未在 snapshot 中的 cid 时也能兜底查询
    let direct_exams = service.list_exams(Some("999999")).await.unwrap();
    assert_eq!(direct_exams.len(), 1);
    assert_eq!(direct_exams[0].cid, "999999");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_uooc_exam_601_auto_course_evaluation_and_retry() {
    let root = root();
    let service = service_test(root.clone());

    let eval_comment_called = Arc::new(AtomicUsize::new(0));
    let eval_comment_called_clone = eval_comment_called.clone();
    let know_review_called = Arc::new(AtomicUsize::new(0));
    let know_review_called_clone = know_review_called.clone();

    let save_attempts = Arc::new(AtomicUsize::new(0));
    let save_attempts_clone = save_attempts.clone();
    let commit_attempts = Arc::new(AtomicUsize::new(0));
    let commit_attempts_clone = commit_attempts.clone();

    let server = Server::new(move |request| {
        if request.path.starts_with("/home/course/setKnowReview") {
            know_review_called_clone.fetch_add(1, Ordering::SeqCst);
            return ok(json!({ "msg": "已知晓" }));
        }
        if request.path.starts_with("/home/course/comment") {
            return ok(json!({ "has_comment": false }));
        }
        if request.path.starts_with("/home/course/addcomment") {
            eval_comment_called_clone.fetch_add(1, Ordering::SeqCst);
            return ok(json!({ "msg": "评价成功" }));
        }
        if request.path.starts_with("/exam/save") {
            let attempt = save_attempts_clone.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                // 首次调用返回 601 验证错误：提示需要先完成课程评价
                return json!({
                    "code": 601,
                    "msg": "服务器验证错误",
                    "data": ["请先完成该课程的教学评价"]
                });
            }
            return ok(json!({ "msg": "草稿已保存" }));
        }
        if request.path.starts_with("/exam/commit") {
            let attempt = commit_attempts_clone.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                return json!({
                    "code": 601,
                    "msg": "服务器验证错误",
                    "data": ["必须先完成课程教学评价方可提交试卷"]
                });
            }
            return ok(json!({ "msg": "考试提交成功" }));
        }
        ok(json!({}))
    });

    *service.client.write().unwrap() =
        Arc::new(Client::with_base(session::load(None).unwrap(), &server.base).unwrap());

    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
    });

    let mut answers = super::quiz::model::Answers::new();
    answers.insert("q1".into(), vec!["A".into()]);

    // 1. 测试 save_exam_draft 遇到 601 时自动完成评价并重试成功
    service.save_exam_draft("42", "task_exam_1", answers.clone()).await.unwrap();
    assert_eq!(save_attempts.load(Ordering::SeqCst), 2); // 第一次 601，第二次成功
    assert!(know_review_called.load(Ordering::SeqCst) >= 1);
    assert!(eval_comment_called.load(Ordering::SeqCst) >= 1);

    // 2. 测试 submit_exam 遇到 601 时自动完成评价并重试成功
    let msg = service.submit_exam("42", "task_exam_1", answers, None).await.unwrap();
    assert_eq!(msg, "考试提交成功");
    assert_eq!(commit_attempts.load(Ordering::SeqCst), 2); // 第一次 601，第二次成功

    // 3. 测试 Error::message 对 601 的格式化输出
    let err_eval = super::client::Error::Server(
        601,
        "服务器验证错误".into(),
        vec!["请先完成该课程的教学评价".into()],
    );
    assert!(err_eval.message().contains("请先完成课程评价"));
    assert!(err_eval.message().contains("601"));

    let err_field = super::client::Error::Server(
        601,
        "服务器验证错误".into(),
        vec!["cid不能为空".into()],
    );
    assert!(err_field.message().contains("服务器验证错误：cid不能为空"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn learned_courses_page_includes_ended_courses_for_shared_bank_checks() {
    let rows = json!({"data": [
        {"id":42,"status":"31","parent_name":"开课中"},
        {"id":43,"status":"33","parent_name":"复习模式"},
        {"id":77,"status":"35","parent_name":"已结课"},
        {"id":78,"status":34,"parent_name":"其他状态"},
        {"id":79,"status":0}
    ],"page":{"pages":1}});

    let (active, _) = model::courses_page(&rows).unwrap();
    assert_eq!(
        active.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["42", "43"]
    );

    let (learned, _) = model::learned_courses_page(&rows).unwrap();
    assert_eq!(
        learned.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["42", "43", "77", "78", "79"]
    );
}

#[tokio::test]
async fn course_share_need_asks_server_by_cid() {
    let server = Server::new(|request| {
        match request.path.as_str() {
            "/api/courses/missing" => json!({"error": "Course not found", "taskGroups": []}),
            "/api/courses/full" => json!({"course": {"cid": "full"}, "taskGroups": []}),
            "/api/courses/need" => json!({"course": {"cid": "need"}, "taskGroups": []}),
            _ if request.path.starts_with("/api/automation/tasks") => {
                let fields = request.fields();
                if fields.get("cid").map(String::as_str) != Some("need") {
                    return json!({"ok": true, "tasks": []});
                }
                json!({"ok": true, "tasks": [
                    {"taskType": "answer_request", "cid": "need", "tid": "901"},
                    {"taskType": "answer_request", "cid": "need", "tid": "901"},
                    {"taskType": "answer_request", "cid": "need", "tid": "902"},
                    {"taskType": "stem_request", "cid": "need", "tid": "903"}
                ]})
            }
            _ => json!({"ok": true}),
        }
    });

    assert_eq!(
        super::quiz::qa::course_share_need(&server.base, Some("key"), "missing")
            .await
            .unwrap(),
        super::quiz::qa::CourseShareNeed::EntireCourse
    );
    assert_eq!(
        super::quiz::qa::course_share_need(&server.base, Some("key"), "full")
            .await
            .unwrap(),
        super::quiz::qa::CourseShareNeed::Satisfied
    );
    assert_eq!(
        super::quiz::qa::course_share_need(&server.base, Some("key"), "need")
            .await
            .unwrap(),
        super::quiz::qa::CourseShareNeed::TaskGroups(vec!["901".into(), "902".into()])
    );
}

#[tokio::test]
async fn download_qa_vectors_never_pulls_other_courses() {
    use std::sync::Mutex;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    let server = Server::new(move |request| {
        observed.lock().unwrap().push(request.path.clone());
        if request.path == "/api/courses/42" {
            return json!({
                "course": {"cid": "42"},
                "taskGroups": [{"tid": "100"}]
            });
        }
        if request.path.starts_with("/api/automation/tasks") {
            return json!({"ok": true, "tasks": [
                {"cid": "42", "tid": "101"},
                {"cid": "99", "tid": "102"}
            ]});
        }
        if let Some(tid) = request
            .path
            .strip_prefix("/api/courses/42/tasks/")
        {
            return json!({"questions": [{
                "questionId": format!("q_{tid}"),
                "type": "single",
                "stem": format!("题目 {tid}"),
                "answers": [{"values": ["A"], "isCorrect": true}]
            }]});
        }
        json!({"ok": true})
    });

    let dir = tempfile::tempdir().unwrap();
    let qa_db = super::quiz::db::QaDatabase::open(&dir.path().join("qa.db"))
        .await
        .unwrap();
    let status = super::quiz::qa::download_qa_vectors(
        &qa_db,
        &server.base,
        Some("key"),
        &["42".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(status.cached_count, 2);

    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|path| path == "/api/courses/42/tasks/100"));
    assert!(seen.iter().any(|path| path == "/api/courses/42/tasks/101"));
    assert!(!seen.iter().any(|path| path.contains("102")));
    assert!(!seen.iter().any(|path| path.contains("/api/courses/99")));

    let _ = std::fs::remove_dir_all(dir.path());
}

#[tokio::test]
async fn build_local_bank_only_builds_ended_course_when_server_needs_it() {
    use std::sync::Mutex;

    let root = root();
    let service = service_test(root.clone());

    let qa_requests = Arc::new(Mutex::new(Vec::new()));
    let qa_seen = qa_requests.clone();
    let upload_bodies = Arc::new(Mutex::new(Vec::new()));
    let upload_seen = upload_bodies.clone();

    let qa_server = Server::new(move |request| {
        qa_seen.lock().unwrap().push(request.path.clone());
        match request.path.as_str() {
            "/api/courses/42" => json!({
                "course": {"cid": "42", "name": "开课课程"},
                "taskGroups": []
            }),
            "/api/courses/77" => json!({"error": "Course not found", "taskGroups": []}),
            "/api/automation/tasks" => json!({"ok": true, "tasks": [], "count": 0}),
            "/api/automation/match" => json!({"ok": true, "missingOnServer": []}),
            "/api/upload/batch" => {
                upload_seen.lock().unwrap().push(request.body.clone());
                json!({"ok": true, "results": []})
            }
            _ => json!({"ok": true, "questions": []}),
        }
    });

    let uooc_server = Server::new(|request| {
        if request.path.starts_with(super::client::LIST) {
            return ok(json!({"data": [
                {"id":42,"status":"31","parent_name":"开课课程"},
                {"id":77,"status":"35","parent_name":"已结课课程"}
            ],"page":{"pages":1}}));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            let cid = request.fields().get("cid").cloned().unwrap_or_default();
            let (id, task_id) = if cid == "77" { (2, 902) } else { (1, 901) };
            return ok(json!([{
                "id": id,
                "name": "章节",
                "children": [{
                    "id": id * 10,
                    "name": "测验",
                    "task_id": task_id,
                    "finished": 1,
                    "icon_list": [{"id": task_id, "type": "80", "type_name": "测验"}]
                }]
            }]));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            let tid = request.fields().get("tid").cloned().unwrap_or_default();
            return json!({
                "code": 1,
                "data": {"questions": [{
                    "id": format!("q_{tid}"),
                    "question": format!("题目 {tid}"),
                    "type": "10",
                    "options_app": [{"key": "A", "value": "选项"}],
                    "answer": "A"
                }]}
            });
        }
        ok(json!({"code": 1}))
    });

    *service.client.write().unwrap() = Arc::new(
        Client::with_base(session::load(None).unwrap(), &uooc_server.base).unwrap(),
    );
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User { id: "7".into(), name: "test".into() });
        s.courses = vec![course("42")];
        s.settings.qa_server_url = qa_server.base.clone();
    });

    let count = service.build_local_bank().await.unwrap();
    assert_eq!(count, 2);

    let bank = service
        .get_qa_db()
        .await
        .unwrap()
        .load_all_bank_questions()
        .await
        .unwrap();
    assert!(bank.values().any(|q| q.cid == "42" && q.tid == "901"));
    assert!(bank.values().any(|q| q.cid == "77" && q.tid == "902"));

    let qa_requests = qa_requests.lock().unwrap();
    // 开课中课程允许拉取题库
    assert!(qa_requests.iter().any(|p| p == "/api/courses/42/tasks/901"));
    // 已结课课程不拉取服务端题库，只上传缺失内容
    assert!(!qa_requests.iter().any(|p| p.starts_with("/api/courses/77/tasks/")));

    let uploaded_tids: Vec<String> = upload_bodies
        .lock()
        .unwrap()
        .iter()
        .map(|body| {
            serde_json::from_str::<serde_json::Value>(body).unwrap()["tid"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert!(uploaded_tids.contains(&"902".to_string()));

    let _ = std::fs::remove_dir_all(root);
}
