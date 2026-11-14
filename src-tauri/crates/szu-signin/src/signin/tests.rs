//! 签到模块测试：纯函数（geo/matching/scheduler/room/photo）+ QW 客户端
//! mock。涉及 szu-lms / 房间实际网络的服务级链路由真机验收覆盖。

use super::model::{TimetableCache, TimetableSession};
use super::qw;
use super::scheduler;
use super::Service;
use szu_core::storage::testing::MemoryCredentialStore;
use std::sync::Arc;

fn test_credentials() -> Arc<MemoryCredentialStore> {
    Arc::new(MemoryCredentialStore::default())
}

// ---------------------------------------------------------------------------
// 课表与本地日期
// ---------------------------------------------------------------------------

fn session(date: &str, start_h: u32, end_h: u32) -> TimetableSession {
    use chrono::{NaiveDate, TimeZone as _};
    let base = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
    let start = chrono::Local
        .from_local_datetime(&base.and_hms_opt(start_h, 0, 0).unwrap())
        .single()
        .unwrap();
    let end = chrono::Local
        .from_local_datetime(&base.and_hms_opt(end_h, 0, 0).unwrap())
        .single()
        .unwrap();
    TimetableSession {
        date: date.to_string(),
        course_name: "概率论与数理统计".into(),
        building_name: Some("致理楼".into()),
        building_name_en: None,
        classroom: None,
        start_ms: start.timestamp_millis(),
        end_ms: end.timestamp_millis(),
        semester: None,
    }
}

#[tokio::test]
async fn timetable_cache_roundtrip_and_today_filter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let service = Service::with_bases(
        None,
        test_credentials(),
        path.clone(),
        "http://127.0.0.1:1/".into(),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    assert!(service.timetable_snapshot().is_empty());
    let cache = TimetableCache {
        fetched_at_ms: chrono::Utc::now().timestamp_millis(),
        semester: Some("2026-2027-1".into()),
        sessions: vec![
            session("2026-11-13", 8, 10),
            session("2026-11-13", 14, 16),
            session("2026-11-14", 10, 12),
        ],
    };
    let database = super::db::SigninDatabase::open(&path).await.unwrap();
    database.save_timetable(&cache).await.unwrap();
    // 重新打开服务验证内存缓存从磁盘恢复。
    let reopened = Service::with_bases(
        None,
        test_credentials(),
        path,
        "http://127.0.0.1:1/".into(),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    assert_eq!(reopened.timetable_snapshot().sessions.len(), 3);
    let today = super::local_date(chrono::Utc::now().timestamp_millis());
    let count_on_today = reopened
        .timetable_snapshot()
        .sessions_on(&today)
        .len();
    let _ = count_on_today; // 取决于运行日期，只验证不 panic。
    // today_sessions 与 local_date 一致。
    let now = chrono::Utc::now().timestamp_millis();
    assert_eq!(reopened.today_sessions(now).len(), count_on_today);
}

#[tokio::test]
async fn resolve_radar_point_prefers_memory_then_timetable_then_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let service = Service::with_bases(
        None,
        test_credentials(),
        path.clone(),
        "http://127.0.0.1:1/".into(),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    // 1. 兜底：无记忆、无课表 → 致理楼。
    let point = service
        .resolve_radar_point("42", "概率论与数理统计")
        .await
        .unwrap();
    assert_eq!(point.source, "fallback");
    assert_eq!(point.label, "致理楼");
    assert_eq!(point.fence_radius_m, Some(130));
    // 2. 课表：写入课表后命中楼栋坐标。
    let cache = TimetableCache {
        fetched_at_ms: chrono::Utc::now().timestamp_millis(),
        semester: None,
        sessions: vec![session("2026-11-13", 8, 10)],
    };
    let database = super::db::SigninDatabase::open(&path).await.unwrap();
    database.save_timetable(&cache).await.unwrap();
    *service.timetable.lock().unwrap() = cache;
    let point = service
        .resolve_radar_point("42", "概率论与数理统计")
        .await
        .unwrap();
    assert_eq!(point.source, "timetable");
    assert_eq!(point.label, "致理楼");
    // 3. 记忆：保存上次成功点后优先返回。
    super::db::SigninDatabase::open(&path)
        .await
        .unwrap()
        .save_last_point(
            "42",
            &super::model::LastPoint {
                latitude: 22.5,
                longitude: 113.9,
                label: "浏览器定位".into(),
                source: "geolocation".into(),
            },
        )
        .await
        .unwrap();
    let point = service
        .resolve_radar_point("42", "概率论与数理统计")
        .await
        .unwrap();
    assert_eq!(point.source, "last_success");
    assert!((point.latitude - 22.5).abs() < 1e-9);
}

#[test]
fn szu_course_key_is_normalized_and_prefixed() {
    assert_eq!(super::szu_course_key("  概率论 课程 "), "szu:概率论课程");
    assert_ne!(super::szu_course_key("课程A"), super::szu_course_key("课程B"));
}

// ---------------------------------------------------------------------------
// QW 客户端（mock）
// ---------------------------------------------------------------------------

/// 最小 HTTP mock：cas/status + CAS 登录链语义。
/// `fail_first_probes` 控制 cas/status 前几次探测返回未登录（验证
/// 「探测失败 → 走登录链 → 落地复探成功」的建立路径）。
struct MockServer {
    _listener: std::net::TcpListener,
    pub base: String,
}

impl MockServer {
    fn start(fail_first_probes: usize) -> Self {
        use std::io::{Read as _, Write as _};
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Arc;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let kept = listener.try_clone().expect("复制监听句柄");
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let probes = Arc::new(AtomicUsize::new(0));
        let callback_hit = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || {
            let callback_hit = callback_hit.clone();
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => break,
                };
                let mut buf = vec![0u8; 4096];
                let _ = stream.read(&mut buf);
                let request_line = String::from_utf8_lossy(&buf);
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let (status, extra_headers, body) = if path.starts_with("/attendance/cas/status") {
                    let hit = probes.fetch_add(1, Ordering::SeqCst);
                    if hit < fail_first_probes {
                        // 未登录的真实形态：success:true（请求成功）+ logged_in:false。
                        (
                            200,
                            Vec::new(),
                            br#"{"success": true, "logged_in": false}"#.to_vec(),
                        )
                    } else {
                        (
                            200,
                            Vec::new(),
                            br#"{"success": true, "logged_in": true, "user": {"username": "2025150041", "name": "Test"}}"#.to_vec(),
                        )
                    }
                } else if path.starts_with("/attendance/index.html") {
                    // 登录链入口：会话未建立时 302 到回跳；callback 建立会话后
                    // 再次访问即为 200 落地（真实流程语义）。
                    if callback_hit.load(Ordering::SeqCst) {
                        (200, Vec::new(), b"<html>attendance</html>".to_vec())
                    } else {
                        (
                            302,
                            vec![(
                                "Location".to_string(),
                                "/attendance/cas/callback?redirect=%2Fattendance%2Findex.html&ticket=ST-test".to_string(),
                            )],
                            Vec::new(),
                        )
                    }
                } else if path.starts_with("/attendance/cas/callback") {
                    callback_hit.store(true, Ordering::SeqCst);
                    (
                        302,
                        vec![("Location".to_string(), "/attendance/index.html".to_string())],
                        Vec::new(),
                    )
                } else {
                    (200, Vec::new(), br#"{"success": false}"#.to_vec())
                };
                let mut head = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (key, value) in extra_headers {
                    head.push_str(&format!("{key}: {value}\r\n"));
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        Self {
            _listener: kept,
            base,
        }
    }
}

/// 真实形态的网关 mock：会话只由 callback 的
/// `Set-Cookie: cas_session=…; Path=/attendance` 建立，cas/status、登录入口
/// 与业务接口都要求它。只有把会话 cookie 真正落盘并在新客户端上恢复，
/// 课表拉取才能成功（`refresh_timetable` 每次都会重建客户端）。
struct SessionMockServer {
    _listener: std::net::TcpListener,
    pub base: String,
}

impl SessionMockServer {
    fn start() -> Self {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let kept = listener.try_clone().expect("复制监听句柄");
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => break,
                };
                let mut buf = vec![0u8; 4096];
                let _ = stream.read(&mut buf);
                let request = String::from_utf8_lossy(&buf);
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let session = request.contains("cas_session=test");
                let (status, extra_headers, body) = if path.starts_with("/attendance/cas/status") {
                    (
                        200,
                        Vec::new(),
                        format!(
                            r#"{{"success": true, "logged_in": {session}, "user": {}}}"#,
                            if session {
                                r#"{"username": "2025150041", "name": "Test"}"#
                            } else {
                                "null"
                            }
                        )
                        .into_bytes(),
                    )
                } else if path.starts_with("/attendance/index.html") {
                    if session {
                        (200, Vec::new(), b"<html>attendance</html>".to_vec())
                    } else {
                        (
                            302,
                            vec![(
                                "Location".to_string(),
                                "/attendance/cas/callback?redirect=%2Fattendance%2Findex.html&ticket=ST-test".to_string(),
                            )],
                            Vec::new(),
                        )
                    }
                } else if path.starts_with("/attendance/cas/callback") {
                    // 真实服务端的会话 cookie：Path 限定在 /attendance。
                    (
                        302,
                        vec![
                            ("Location".to_string(), "/attendance/index.html".to_string()),
                            (
                                "Set-Cookie".to_string(),
                                "cas_session=test; Path=/attendance; HttpOnly".to_string(),
                            ),
                        ],
                        Vec::new(),
                    )
                } else if path.starts_with("/attendance/api/v1/student-schedule") {
                    if session {
                        (
                            200,
                            Vec::new(),
                            r#"{"success":true,"data":{"schedule":[{"date":"2026-11-14","course_name":"概率论与数理统计","start_time":"08:30","end_time":"10:10"}]}}"#
                                .as_bytes()
                                .to_vec(),
                        )
                    } else {
                        (
                            401,
                            Vec::new(),
                            br#"{"success":false,"message":"session expired"}"#.to_vec(),
                        )
                    }
                } else {
                    (200, Vec::new(), br#"{"success": false}"#.to_vec())
                };
                let mut head = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (key, value) in extra_headers {
                    head.push_str(&format!("{key}: {value}\r\n"));
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        Self {
            _listener: kept,
            base,
        }
    }
}

#[tokio::test]
async fn refresh_timetable_reuses_persisted_path_scoped_session() {
    let dir = tempfile::tempdir().unwrap();
    let server = SessionMockServer::start();
    let service = Service::with_bases(
        None,
        test_credentials(),
        dir.path().join("state.sqlite"),
        format!("{}/", server.base),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    let cache = service
        .refresh_timetable(true)
        .await
        .expect("会话落盘后应能拉取课表");
    assert_eq!(cache.sessions.len(), 1);
    assert_eq!(cache.sessions[0].course_name, "概率论与数理统计");
}

#[tokio::test]
async fn qw_bootstrap_reports_established_on_success() {    let server = MockServer::start(0);
    let client = qw::QwClient::new(&server.base, "https://authserver.szu.edu.cn/", None, None).unwrap();
    match client.bootstrap().await {
        Ok(qw::Bootstrap::Established { student_id, name }) => {
            // cas/status 的 user.username 即学号，签到 payload 需要。
            assert_eq!(student_id, "2025150041");
            assert_eq!(name, "Test");
        }
        other => panic!("应判为已登录，实际 {other:?}"),
    }
}

#[tokio::test]
async fn qw_bootstrap_establishes_session_via_cas_chain() {
    // 首次探测未登录：走 index.html 登录链（302 回跳）落地后复探成功。
    let server = MockServer::start(1);
    let client = qw::QwClient::new(&server.base, "https://authserver.szu.edu.cn/", None, None).unwrap();
    match client.bootstrap().await {
        Ok(qw::Bootstrap::Established { .. }) => {}
        other => panic!("链建立后应判为已登录，实际 {other:?}"),
    }
}

#[tokio::test]
async fn qw_bootstrap_reports_needs_auth_on_failure() {
    // 服务端始终不给会话：链走完复探仍失败 → 如实报 NeedsAuth。
    let server = MockServer::start(usize::MAX);
    let client = qw::QwClient::new(&server.base, "https://authserver.szu.edu.cn/", None, None).unwrap();
    match client.bootstrap().await {
        Ok(qw::Bootstrap::NeedsAuth) => {}
        other => panic!("应判为需要认证，实际 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 探测计划与待签视图
// ---------------------------------------------------------------------------

#[test]
fn checkin_view_defaults_to_incomplete_and_empty() {
    let dir = tempfile::tempdir().unwrap();
    let service = Service::with_bases(
        None,
        test_credentials(),
        dir.path().join("state.sqlite"),
        "http://127.0.0.1:1/".into(),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    let view = service.checkin_view();
    assert!(view.items.is_empty());
    assert!(!view.complete);
}

#[tokio::test]
async fn probe_without_app_keeps_view_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let service = Service::with_bases(
        None,
        test_credentials(),
        dir.path().join("state.sqlite"),
        "http://127.0.0.1:1/".into(),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    service.probe().await;
    let view = service.checkin_view();
    assert!(!view.complete, "无应用句柄时探测应失败并保持不完整");
    assert!(view.items.is_empty());
}

#[test]
fn window_targets_cover_only_course_window_plus_margin() {
    use szu_lms::lms::rollcall::RollcallItem;
    const MIN: i64 = 60 * 1000;
    const MARGIN: i64 = super::ROOM_WINDOW_MARGIN_MS;
    let session = super::model::TimetableSession {
        date: "2026-11-14".into(),
        course_name: "概率论与数理统计".into(),
        building_name: None,
        building_name_en: None,
        classroom: None,
        start_ms: 9 * 60 * MIN,
        end_ms: 11 * 60 * MIN,
        semester: None,
    };
    let rollcall = RollcallItem {
        rollcall_id: 1,
        is_number: false,
        is_radar: true,
        course_id: 42,
        course_title: "概率论与数理统计".into(),
        student_rollcall_id: 1,
        kind: None,
        status: Some("on_call".into()),
    };
    let sessions = vec![&session];
    // 窗口内（开始前 10 分钟整、课中、结束后 10 分钟整）命中。
    for now in [
        9 * 60 * MIN - MARGIN,
        10 * 60 * MIN,
        11 * 60 * MIN + MARGIN,
    ] {
        let targets = super::window_targets(&sessions, &[rollcall.clone()], now);
        assert_eq!(targets, vec![(42, 1)], "now={now}");
    }
    // 窗口外沿一分钟：不命中。
    for now in [9 * 60 * MIN - MARGIN - MIN, 11 * 60 * MIN + MARGIN + MIN] {
        assert!(super::window_targets(&sessions, &[rollcall.clone()], now).is_empty());
    }
    // 已完成（on_call_fine）的点名不作为目标。
    let done = RollcallItem {
        status: Some("on_call_fine".into()),
        ..rollcall.clone()
    };
    assert!(super::window_targets(
        &sessions,
        &[done],
        10 * 60 * MIN
    )
    .is_empty());
}

#[tokio::test]
async fn ensure_qw_persists_session_and_reports_ready() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(0);
    let service = Service::with_bases(
        None,
        test_credentials(),
        dir.path().join("state.sqlite"),
        format!("{}/", server.base),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    let status = service.qw_status(true).await;
    assert_eq!(status.state, super::model::QwSessionState::Ready);
    // 会话 jar 落盘（vault 凭据），与 lms/jxxk 同一签发链路。
    let saved = service.credentials.load_qw_session().unwrap();
    assert!(saved.is_some(), "bootstrap 成功后应保存会话 cookie");
    // 二次查询走 Ready 快路径，不再触发网络。
    let again = service.qw_status(false).await;
    assert_eq!(again.state, super::model::QwSessionState::Ready);
}

#[tokio::test]
async fn refresh_timetable_reports_needs_auth_when_sso_dead() {
    // 统一认证 SSO 失效（登录链无法建立会话）：课表拉取应如实提示重新认证，
    // 而不是继续发请求报出误导性的「会话已失效」。
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(usize::MAX);
    let service = Service::with_bases(
        None,
        test_credentials(),
        dir.path().join("state.sqlite"),
        format!("{}/", server.base),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    let error = service
        .refresh_timetable(true)
        .await
        .expect_err("未认证应失败");
    assert!(error.contains("统一认证"), "应提示重新认证: {error}");
    assert!(!error.contains("会话已失效"), "不应误导为会话失效: {error}");
}

#[tokio::test]
async fn ensure_qw_reports_needs_auth_without_cas() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(usize::MAX);
    let service = Service::with_bases(
        None,
        test_credentials(),
        dir.path().join("state.sqlite"),
        format!("{}/", server.base),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    let status = service.qw_status(true).await;
    assert_eq!(status.state, super::model::QwSessionState::NeedsAuth);
}

#[tokio::test]
async fn ensure_qw_reports_unavailable_when_session_save_fails() {
    // 会话没落盘时不能声称 Ready：下一次请求会从 vault 重建客户端并 401，
    // 表现为误导性的「会话已失效」。
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start(0);
    let credentials = test_credentials();
    credentials
        .fail_session_save
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let service = Service::with_bases(
        None,
        credentials,
        dir.path().join("state.sqlite"),
        format!("{}/", server.base),
        "http://127.0.0.2:1/".into(),
    )
    .unwrap();
    let status = service.qw_status(true).await;
    assert_eq!(status.state, super::model::QwSessionState::Unavailable);
    assert!(
        status.message.contains("保存"),
        "应提示会话保存失败: {}",
        status.message
    );
    let error = service
        .refresh_timetable(true)
        .await
        .expect_err("落盘失败不应声称就绪");
    assert!(
        error.contains("保存"),
        "刷新课表应返回保存失败，而不是会话失效: {error}"
    );
}

#[test]
fn probe_window_gate_matches_scheduler_windows() {
    let min = 60 * 1000;
    let start = 9 * 60 * min;
    let end = 11 * 60 * min;
    let sessions = [(start, end)];
    // 窗口内与课中：自动探测放行。
    assert!(scheduler::probe_window_active(&sessions, start));
    assert!(scheduler::probe_window_active(&sessions, start + 20 * min));
    assert!(scheduler::probe_window_active(&sessions, end));
    // 开课前 10 分钟（计划在未来）与结束后 10 分钟：自动探测拦截。
    assert!(!scheduler::probe_window_active(&sessions, start - 10 * min));
    assert!(!scheduler::probe_window_active(&sessions, end + 10 * min));
    // 无课：始终拦截。
    assert!(!scheduler::probe_window_active(&[], start));
}

#[test]
fn scheduler_probe_covers_windows_and_course_time() {
    let min = 60 * 1000;
    let start = 9 * 60 * min;
    let end = 11 * 60 * min;
    // 窗口内立即探测。
    let plan = scheduler::next_probe(&[(start, end)], start).unwrap();
    assert_eq!(plan.cadence_ms, 30 * 1000);
    // 课中 5min。
    let plan = scheduler::next_probe(&[(start, end)], start + 20 * min).unwrap();
    assert_eq!(plan.cadence_ms, 5 * 60 * 1000);
    // 课后无计划。
    assert!(scheduler::next_probe(&[(start, end)], end + 10 * min).is_none());
}


// ---------------------------------------------------------------------------
// 真实链路诊断（只读；必须显式授权）
// ---------------------------------------------------------------------------

/// 极简 eprintln 日志器：让 bootstrap 的逐跳日志在测试里可见。
struct EprintlnLogger;

impl log::Log for EprintlnLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// 走真实「未登录 → CAS 链 → 会话建立 → 课表」全流程：
/// fresh jar（无任何 qw cookie）+ 应用 vault 里的 authserver SSO。
/// 只读：绝不写 keyring / vault。运行：
/// `SZU_READ_SAVED_SESSION=1 cargo test -p szu-signin qw_live -- --ignored --nocapture`
#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "Read-only live chain diagnostic; requires explicit user authorization"]
async fn qw_live_cas_chain_diagnostic() {
    assert!(
        std::env::var("SZU_READ_SAVED_SESSION").as_deref() == Ok("1"),
        "set SZU_READ_SAVED_SESSION=1 to explicitly enable this live diagnostic"
    );
    let _ = log::set_boxed_logger(Box::new(EprintlnLogger));
    log::set_max_level(log::LevelFilter::Info);

    let Some(session) = szu_core::storage::load_saved_session_readonly() else {
        panic!("真实统一认证会话不可读（keyring 或 vault 缺失）");
    };
    eprintln!(
        "vault 会话: student_id={} cookies={} 条",
        session.student_id,
        session.cookies.len()
    );

    let sso = szu_core::webview_cookies::project(
        szu_core::webview_cookies::Snapshot {
            cookies: session.cookies,
            http_keys: session.http_keys,
        },
        szu_core::webview_cookies::Scope::auth(),
    )
    .expect("SSO jar 投影失败");

    // fresh jar：不加载 vault 的 qw 会话 —— 模拟「qw 未登录，仅 authserver SSO」。
    let client = qw::QwClient::new(qw::QW_ORIGIN, qw::QW_CAS_LOGIN, None, Some(&sso))
        .expect("客户端构建失败");
    match client.bootstrap().await {
        Ok(qw::Bootstrap::Established { student_id, name }) => {
            eprintln!("bootstrap: Established student_id={student_id} name={name}");
        }
        Ok(qw::Bootstrap::NeedsAuth) => {
            panic!("bootstrap: NeedsAuth —— 链未能从 authserver SSO 建立 qw 会话");
        }
        Err(error) => panic!("bootstrap: Err({error})"),
    }

    let sessions = qw::student_schedule(&client)
        .await
        .expect("课表拉取失败");
    eprintln!("student-schedule: {} 个课次", sessions.len());
    assert!(!sessions.is_empty(), "课表不应为空");
    // 刚由 callback 建立的全新会话：后续用它验证「落盘 → 重建」。
    let fresh_cookies = client.cookies().expect("fresh 会话序列化失败");

    // ---- 第二轮：完全复刻 app 参数（vault 里的旧 qw 会话 + SSO）----
    let data_root = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| std::path::PathBuf::from(home).join(".local/share"))
        })
        .expect("XDG 数据目录");
    let vault = szu_core::storage::vault::Vault::open(
        data_root.join("in.rinne.your-szu-life").join("credentials"),
        {
            let key = szu_core::storage::vault::read_keyring_secret(
                szu_core::storage::SERVICE_NAME,
                szu_core::storage::vault::VAULT_KEY_ENTRY,
            )
            .expect("keyring 读取失败")
            .filter(|bytes| bytes.len() == szu_core::storage::vault::KEY_LEN)
            .expect("vault 主密钥不可用");
            let mut fixed = [0u8; szu_core::storage::vault::KEY_LEN];
            fixed.copy_from_slice(&key);
            fixed
        },
    );
    let saved_qw: Option<String> = szu_core::storage::vault::Vault::load(
        &vault,
        szu_core::storage::entries::QW_SESSION,
    )
    .expect("vault 读取失败");
    eprintln!(
        "vault qw 会话: {}",
        saved_qw.as_ref().map(|c| format!("{} 字节", c.len())).unwrap_or_else(|| "无".into())
    );
    let client = qw::QwClient::new(
        qw::QW_ORIGIN,
        qw::QW_CAS_LOGIN,
        saved_qw.as_deref(),
        Some(&sso),
    )
    .expect("客户端构建失败（复刻 app 参数）");
    match client.bootstrap().await {
        Ok(qw::Bootstrap::Established { student_id, name }) => {
            eprintln!("bootstrap(app 参数): Established student_id={student_id} name={name}");
        }
        Ok(qw::Bootstrap::NeedsAuth) => {
            panic!("bootstrap(app 参数): NeedsAuth —— 链未能建立会话");
        }
        Err(error) => panic!("bootstrap(app 参数): Err({error})"),
    }
    let sessions = qw::student_schedule(&client)
        .await
        .expect("课表拉取失败（复刻 app 参数）");
    eprintln!("student-schedule(app 参数): {} 个课次", sessions.len());
    assert!(!sessions.is_empty(), "课表不应为空（复刻 app 参数）");

    // ---- 第三轮：复刻 refresh_timetable 的「cookies() 落盘 → 重建客户端」----
    // 真机上失败的就是这一段：会话 cookie 的 Path=/attendance 被过滤掉，
    // 重建出来的客户端没有 cas_session，接口一律 401。用第一轮刚建立的全新
    // 会话（而不是 vault 里可能已有的旧 cookie）来验证。
    let saved = fresh_cookies;
    eprintln!("cookies() 序列化: {} 字节", saved.len());
    let rebuilt = qw::QwClient::new(
        qw::QW_ORIGIN,
        qw::QW_CAS_LOGIN,
        Some(&saved),
        Some(&sso),
    )
    .expect("客户端构建失败（重建）");
    let sessions = qw::student_schedule(&rebuilt)
        .await
        .expect("课表拉取失败（重建客户端）");
    eprintln!("student-schedule(重建客户端): {} 个课次", sessions.len());
    assert!(!sessions.is_empty(), "课表不应为空（重建客户端）");
}
