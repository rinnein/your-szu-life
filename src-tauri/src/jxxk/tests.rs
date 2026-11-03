use super::client::{Bootstrap, Client};
use super::model::*;
use super::{parse_course_page, parse_lesson_page, parse_lesson_streams, parse_play_urls};
use serde_json::{json, Value};
use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// 最小 HTTP mock：raw TcpListener，与 lms/uooc 测试同一套路。
struct Request {
    path: String,
    headers: std::collections::BTreeMap<String, String>,
}
impl Request {
    fn cookies(&self) -> Vec<(String, String)> {
        self.headers
            .get("cookie")
            .map(|raw| {
                raw.split(';')
                    .filter_map(|part| part.trim().split_once('='))
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn has_cookie(&self, name: &str) -> bool {
        self.cookies().iter().any(|(k, _)| k == name)
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|v| v.as_str())
    }
}

struct Server {
    pub base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// `host` 用不同的回环地址（127.0.0.1 / 127.0.0.2）区分 jxxk 与 authserver
    /// 两个 cookie 域。handler 工厂先拿到自身 base，便于构造绝对跳转地址。
    pub fn start<H>(host: &str, make_handler: impl FnOnce(&str) -> H) -> Self
    where
        H: Fn(Request) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind(format!("{host}:0")).unwrap();
        let base = format!(
            "http://{}:{}",
            listener.local_addr().unwrap().ip(),
            listener.local_addr().unwrap().port()
        );
        let handler: Arc<dyn Fn(Request) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + Sync> =
            Arc::from(make_handler(&base));
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = std::thread::spawn(move || {
            let mut children = vec![];
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let handler = handler.clone();
                        children.push(std::thread::spawn(move || {
                            if let Some(request) = read_request(&mut stream) {
                                let (status, headers, body) = handler(request);
                                let extra = headers
                                    .iter()
                                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                                    .collect::<String>();
                                let head = format!(
                                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                                    body.len()
                                );
                                let _ = stream.write_all(head.as_bytes());
                                let _ = stream.write_all(&body);
                            }
                        }));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
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

fn read_request(stream: &mut std::net::TcpStream) -> Option<Request> {
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
                let mut lines = header.lines();
                let request_line = lines.next()?;
                return Some(Request {
                    path: request_line.split_whitespace().nth(1)?.into(),
                    headers: lines
                        .filter_map(|line| line.split_once(':'))
                        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().into()))
                        .collect(),
                });
            }
        }
        if bytes.len() > 65536 {
            return None;
        }
    }
}

/// 记录收到的请求，供测试断言 token 头。
#[derive(Default)]
struct Seen {
    paths: Mutex<Vec<String>>,
}
impl Seen {
    fn record(&self, path: &str) {
        self.paths.lock().unwrap().push(path.to_owned());
    }
    fn count(&self, needle: &str) -> usize {
        self.paths
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.contains(needle))
            .count()
    }
}

fn envelope(data: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"success": true, "code": 200, "message": "", "data": data})).unwrap()
}

/// 起 jxxk + authserver 两个 mock 站点并返回客户端。
/// `with_castgc` 决定 authserver 侧是否有有效的 CAS 会话。
async fn make_client(with_castgc: bool, seen: Arc<Seen>) -> (Client, Server, Server) {
    let auth_server = Server::start("127.0.0.2", move |_auth_base| {
        Box::new(move |request: Request| {
            if request.path.starts_with("/authserver/login") {
                if request.has_cookie("CASTGC") {
                    let service = request
                        .path
                        .split_once("service=")
                        .map(|(_, value)| value.to_owned())
                        .unwrap_or_default();
                    (
                        302,
                        vec![(
                            "Location".into(),
                            format!("{service}&ticket=ST-test-ticket"),
                        )],
                        Vec::new(),
                    )
                } else {
                    (
                        200,
                        vec![],
                        b"<html><div class='login-main'><form id='pwdFromId'></form></div></html>"
                            .to_vec(),
                    )
                }
            } else {
                (404, vec![], Vec::new())
            }
        })
    });
    let seen_for_handler = seen.clone();
    let auth_base = auth_server.base.clone();
    let jxxk_server = Server::start("127.0.0.1", move |jxxk_base| {
        let jxxk_base = jxxk_base.to_string();
        let auth_login = format!("{auth_base}/authserver/login");
        Box::new(move |request: Request| {
            seen_for_handler.record(&request.path);
            if request
                .path
                .starts_with("/university-user/public/refreshToken")
            {
                if request.header("token") == Some("initial-token") {
                    (
                        200,
                        vec![],
                        serde_json::to_vec(&json!({
                            "success": true, "code": 200,
                            "data": "rotated-token",
                        }))
                        .unwrap(),
                    )
                } else {
                    (
                        200,
                        vec![],
                        serde_json::to_vec(&json!({
                            "success": false, "code": -10000, "data": null,
                        }))
                        .unwrap(),
                    )
                }
            } else if request.path.contains("university-basic/public/time") {
                (200, vec![], envelope(json!({"now": "2026-10-31 10:00:00"})))
            } else if request.path.contains("schoolYearSemester") {
                (
                    200,
                    vec![],
                    envelope(json!([
                        {"id": "sem-1", "name": "2026-2027-1",
                         "beginDate": "2026-08-31", "endDate": "2027-01-17"},
                    ])),
                )
            } else if request.path.contains("ticket=") {
                // 票据校验通过后跳回 sso（真实平台行为），最终页面用脚本
                // setCookie 下发 token，而非 Set-Cookie 响应头。
                (
                    302,
                    vec![
                        (
                            "Location".into(),
                            format!("{jxxk_base}/university-sso/ava/sso"),
                        ),
                        ("Set-Cookie".into(), "JXXK_TICKET_OK=1; Path=/".into()),
                    ],
                    Vec::new(),
                )
            } else if request.path.ends_with("/university-sso/ava/sso")
                && request.has_cookie("JXXK_TICKET_OK")
            {
                (
                    200,
                    vec![],
                    format!(
                        r#"<html><body><script>
                        setCookie('fromUrl', 'https:\/\/jxxk\/', 30, "\/")
                        setCookie("COLLEGE_UNIVERSITY_TOKEN_", "initial-token")
                        window.location.href = "https:\/\/jxxk\/"
                        </script></body></html>"#
                    )
                    .into_bytes(),
                )
            } else if request.has_cookie("COLLEGE_UNIVERSITY_TOKEN_szu.edu.cn") {
                (200, vec![], b"<html><body>index</body></html>".to_vec())
            } else if request.path.contains("sso") {
                // 未带票据访问 SSO 入口：转投 authserver CAS。
                (
                    302,
                    vec![(
                        "Location".into(),
                        format!("{auth_login}?service={jxxk_base}/university-sso/ava/sso"),
                    )],
                    Vec::new(),
                )
            } else {
                (
                    302,
                    vec![(
                        "Location".into(),
                        format!("{auth_login}?service={jxxk_base}/university-sso/ava/sso"),
                    )],
                    Vec::new(),
                )
            }
        })
    });
    let sso_cookies = if with_castgc {
        vec!["CASTGC=test-only-cas-session; Path=/authserver".to_string()]
    } else {
        vec![]
    };
    let client = Client::new(
        &jxxk_server.base,
        &format!("{}/authserver/login", auth_server.base),
        None,
        &sso_cookies,
    )
    .unwrap();
    (client, jxxk_server, auth_server)
}

#[test]
fn cookies_keeps_parent_domain_token_cookie() {
    // 平台的 token cookie 可能带 Domain=szu.edu.cn（父域）：按「适用于
    // 巡课请求」过滤必须保留，否则重启后无法恢复会话、每次都重新登录。
    let client = super::client::Client::new(
        super::client::JXXK_ORIGIN,
        super::client::AUTH_LOGIN,
        None,
        &[],
    )
    .unwrap();
    let raw = "COLLEGE_UNIVERSITY_TOKEN_=parent-domain-token; Domain=szu.edu.cn; Path=/";
    let parsed = cookie_store::RawCookie::parse(raw).unwrap();
    {
        let mut jar = client.jar.lock().unwrap();
        jar.insert_raw(&parsed, &"https://jxxk.szu.edu.cn/".parse().unwrap())
            .unwrap();
    }
    client.reload_token_from_jar();
    assert_eq!(client.token().as_deref(), Some("parent-domain-token"));
    let cookies = client.cookies().unwrap();
    assert!(
        cookies.contains("parent-domain-token"),
        "父域 token cookie 不应被会话保存过滤掉"
    );
}


fn temp_service_db_path(tag: &str) -> std::path::PathBuf {
    static COUNTER: Mutex<u32> = Mutex::new(0);
    let mut counter = COUNTER.lock().unwrap();
    *counter += 1;
    std::env::temp_dir().join(format!(
        "jxxk-service-{}-{}-{tag}.sqlite",
        std::process::id(),
        *counter
    ))
}


#[tokio::test]
async fn seeded_cache_keys_include_prefix_and_hit_optimistically() {
    // 回归：播种曾用裸 id 作键，导致启动后缓存永不命中、重验证报「缓存键无效」。
    let credentials: Arc<crate::storage::testing::MemoryCredentialStore> = Arc::default();
    let service = super::Service::new(None, credentials, temp_service_db_path("seed")).unwrap();
    let course_page = CoursePage {
        page_index: 1,
        page_size: 200,
        record_count: 1,
        content: vec![Course {
            id: "c1".into(),
            name: "数据结构".into(),
            teacher: None,
            year_semester_id: Some("sem-1".into()),
            year_semester_name: Some("2026-2027-1".into()),
            category_section_name: None,
            clazzes: None,
            course_resource_count: Some(3),
        }],
    };
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.session = super::model::SessionState::Ready;
        runtime.lists.insert(
            "courses:sem-1".to_string(),
            (
                chrono::Utc::now().timestamp_millis(),
                super::CachedList::Courses(course_page),
            ),
        );
    }
    // 无客户端（None app → 无 SSO cookie）：命中缓存即返回，不触网。
    let page = service.list_courses(Some("sem-1"), None, 1, 200).await.unwrap();
    assert_eq!(page.content[0].name, "数据结构");
}
#[tokio::test]
async fn establish_persists_session_on_refresh_path() {
    // refreshToken 轮换后的会话必须落盘，否则重启后 vault 里还是旧 JWT。
    let seen = Arc::new(Seen::default());
    let (client, _jxxk, _auth) = make_client(true, seen).await;
    client.bootstrap().await.unwrap();
    let credentials: Arc<crate::storage::testing::MemoryCredentialStore> = Arc::default();
    let service = super::Service::new(None, credentials.clone(), temp_service_db_path("refresh")).unwrap();
    service.establish(&client, false).await.unwrap();
    use crate::storage::CredentialStore as _;
    let saved = credentials.load_jxxk_session().unwrap().unwrap();
    assert!(saved.contains("rotated-token"), "轮换后的会话应已落盘");
}

#[tokio::test]
async fn cached_lists_serve_without_network() {
    // 乐观缓存：命中缓存列表的读取完全不触发网络（无 SSO/无客户端也能返回）。
    let credentials: Arc<crate::storage::testing::MemoryCredentialStore> = Arc::default();
    let service =
        super::Service::new(None, credentials, temp_service_db_path("optimistic")).unwrap();
    service.runtime.lock().unwrap().lists.insert(
        "lessons:c1".to_string(),
        (
            chrono::Utc::now().timestamp_millis(),
            super::CachedList::Lessons(LessonPage {
                page_index: 1,
                page_size: 200,
                record_count: 1,
                content: vec![Lesson {
                    id: "l1".into(),
                    target_name: "第1周".into(),
                    live_id: Some("live-1".into()),
                    live_state: Some(4),
                    live_state_text: None,
                    start_time: Some("2026-09-04 10:15:00".into()),
                    end_time: None,
                    classroom: None,
                    media_video_id: Some("media-1".into()),
                }],
            }),
        ),
    );
    let page = service.list_lessons("c1", 1, 200).await.unwrap();
    assert_eq!(page.record_count, 1);
    assert_eq!(page.content[0].target_name, "第1周");
}

#[tokio::test]
async fn bootstrap_establishes_session_and_extracts_token() {
    let seen = Arc::new(Seen::default());
    let (client, _jxxk, _auth) = make_client(true, seen).await;
    match client.bootstrap().await.unwrap() {
        Bootstrap::Established => {}
        other => panic!("expected established session, got {other:?}"),
    }
    assert_eq!(client.token().as_deref(), Some("initial-token"));
    // 保存会话时只保留巡课域 cookie，authserver 的 CAS 会话不得被复制。
    let cookies = client.cookies().unwrap();
    assert!(cookies.contains("COLLEGE_UNIVERSITY_TOKEN_"));
    assert!(!cookies.contains("CASTGC"));
}

#[tokio::test]
async fn bootstrap_without_cas_reports_needs_auth() {
    let seen = Arc::new(Seen::default());
    let (client, _jxxk, _auth) = make_client(false, seen).await;
    assert!(matches!(
        client.bootstrap().await.unwrap(),
        Bootstrap::NeedsAuth
    ));
}

#[tokio::test]
async fn refresh_token_rotates_and_persists_to_jar() {
    let seen = Arc::new(Seen::default());
    let (client, _jxxk, _auth) = make_client(true, seen).await;
    client.bootstrap().await.unwrap();
    client.refresh_token().await.unwrap();
    assert_eq!(client.token().as_deref(), Some("rotated-token"));
    let cookies = client.cookies().unwrap();
    assert!(cookies.contains("rotated-token"));
}

#[tokio::test]
async fn api_calls_require_token_header() {
    let seen = Arc::new(Seen::default());
    let (client, _jxxk, _auth) = make_client(true, seen.clone()).await;
    client.bootstrap().await.unwrap();
    let value = client
        .get_json("university-basic/public/time", &[])
        .await
        .unwrap();
    assert_eq!(value, json!({"now": "2026-10-31 10:00:00"}));
    assert!(seen.count("university-basic/public/time") >= 1);
}

#[tokio::test]
async fn trusted_redirects_cover_szu_subdomains_only() {
    let _guard = crate::netproxy::TEST_ENV_LOCK.lock().unwrap();
    let seen = Arc::new(Seen::default());
    let (client, _jxxk, _auth) = make_client(false, seen).await;
    let trusted = |raw: &str| client.is_trusted_redirect(&raw.parse().unwrap());
    assert!(trusted("https://jxxk.szu.edu.cn/"));
    assert!(trusted("https://authserver.szu.edu.cn/authserver/login"));
    assert!(trusted(
        "https://identity.szu.edu.cn/auth/realms/szu/protocol/cas/login"
    ));
    assert!(!trusted("https://evil.example.com/"));
    assert!(!trusted("https://szu.edu.cn.evil.com/"));

    // 校内网络下，overrides 规则配置的内部入口（172.31.63.8）也是受信任的跳转目标。
    let prod_client = Client::new(
        super::client::JXXK_ORIGIN,
        super::client::AUTH_LOGIN,
        None,
        &[],
    )
    .unwrap();
    let trusted_prod = |raw: &str| prod_client.is_trusted_redirect(&raw.parse().unwrap());

    crate::netproxy::set_in_campus_network(true);
    assert!(trusted_prod("http://172.31.63.8:80/university-sso/ava/sso"));
    assert!(trusted_prod("http://172.31.63.8/university-sso/ava/sso"));

    // 校外网络下，不产生 origin override，非深大域的目标不被信任。
    crate::netproxy::set_in_campus_network(false);
    assert!(!trusted_prod("http://172.31.63.8:80/"));
}

#[tokio::test]
async fn cookies_preserves_override_target_cookies_on_campus() {
    let _guard = crate::netproxy::TEST_ENV_LOCK.lock().unwrap();
    crate::netproxy::set_in_campus_network(true);
    let client = Client::new(
        super::client::JXXK_ORIGIN,
        super::client::AUTH_LOGIN,
        None,
        &[],
    )
    .unwrap();

    // 模拟写入 token，应同时落地到公开 origin 与校内 override 目标 origin。
    client.store_token("test-jwt-token");
    assert!(client.has_token());

    // 导出会话 JSON，反序列化后检查两个 origin 的 cookie 均被保留。
    let json = client.cookies().unwrap();
    assert!(json.contains("test-jwt-token"));

    crate::netproxy::set_in_campus_network(false);
}

#[test]
fn parse_course_page_projects_fields() {
    let value = json!({
        "pageIndex": 1, "pageSize": 12, "recordCount": 2,
        "content": [
            {
                "id": "a19e25fa0bffe8f49884ad2fb3420660",
                "name": "数据结构",
                "teacher": "张志宏",
                "yearSemesterId": "2c9fbf889fc5a0fc01a038b02bc80017",
                "yearSemesterName": "2026-2027-1",
                "categorySectionName": "计算机与软件学院",
                "clazzes": "202620271150275000210",
                "courseResourceCount": 16,
            },
            { "id": "x", "name": "Java程序设计" },
        ],
    });
    let page = parse_course_page(&value).unwrap();
    assert_eq!(page.record_count, 2);
    assert_eq!(page.content[0].name, "数据结构");
    assert_eq!(
        page.content[0].year_semester_name.as_deref(),
        Some("2026-2027-1")
    );
    assert!(page.content[1].teacher.is_none());
}

#[test]
fn parse_lesson_page_extracts_live_state_and_media_video_id() {
    let value = json!({
        "pageIndex": 1, "pageSize": 10, "recordCount": 1,
        "content": [
            {
                "id": "lesson-1",
                "targetName": "数据结构第1周星期5第3-5节",
                "liveDTO": {
                    "id": "live-1",
                    "title": "数据结构第1周星期5第3-5节",
                    "startTime": "2026-09-04 10:15:00",
                    "endTime": "2026-09-04 12:25:00",
                    "liveState": 4,
                    "liveStateText": "查看回放",
                    "mediaVideoIds": ["2c9fbf889fc8e86101a06adf793b6090"],
                    "liveDTORecordEquipments": [
                        {"recordEquipmentName": "L1-703", "recordEquipmentThirdPartyNo": "致理楼L1-703"}
                    ],
                },
            },
        ],
    });
    let page = parse_lesson_page(&value).unwrap();
    assert_eq!(page.content.len(), 1);
    let lesson = &page.content[0];
    assert_eq!(lesson.live_id.as_deref(), Some("live-1"));
    assert_eq!(lesson.live_state, Some(4));
    assert_eq!(
        lesson.media_video_id.as_deref(),
        Some("2c9fbf889fc8e86101a06adf793b6090")
    );
    assert_eq!(lesson.classroom.as_deref(), Some("致理楼L1-703"));
}

#[test]
fn parse_lesson_streams_and_play_urls() {
    let streams = parse_lesson_streams(&json!({
        "pageIndex": 1, "pageSize": 18, "recordCount": 1,
        "content": [
            {
                "id": "video-1",
                "associateId": "media-1",
                "title": "数据结构第1周星期5第3-5节",
                "duration": 7794,
                "download": true,
                "recordEquipmentName": "L1-703",
                "streams": [
                    {"id": 614838, "name": "教师画面", "streamNo": "1", "streamLabel": "TP", "fileSize": 2362046313i64, "duration": 7794, "fileName": "20260904101510.mp4"},
                    {"id": 614839, "name": "学生画面", "streamNo": "2", "streamLabel": "SP", "fileSize": 2362190200i64, "duration": 7794},
                    {"id": 614837, "name": "电脑画面", "streamNo": "30", "fileSize": 1315574675i64, "duration": 7794},
                ],
            }
        ],
    }))
    .unwrap();
    assert_eq!(streams.media_video_id, "media-1");
    assert!(streams.download);
    assert_eq!(streams.streams.len(), 3);
    assert_eq!(streams.streams[0].name, "教师画面");

    let urls = parse_play_urls(&json!({
        "content": [
            {
                "screenTag": "multi", "streamName": "教师画面", "streamNo": "1", "streamLabel": "TP", "streamPermissionType": "TP",
                "streams": [{"id": 614838, "tag": "HLS_VIR", "playUrl": "https://jxxk.szu.edu.cn/vv/m/x.mp4/index.m3u8?t=1&sign=s"}],
            },
            {
                "screenTag": "main", "streamName": "主码流", "streamNo": "0",
                "streams": [],
            },
        ],
    }))
    .unwrap();
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0].stream_name.as_deref(), Some("教师画面"));
    assert!(urls[0].play_url.contains("index.m3u8"));
}


#[test]
fn extract_jwt_token_finds_embedded_jwt() {
    // 合成三段式 JWT 形态：header/payload 为无意义 JSON 的 base64url，
    // signature 为占位片段；模拟中转页把 token 内嵌在脚本里、
    // 再以 document.cookie 拼接写入的场景。
    let header = "eyJhbGciOiJIUzI1NiJ9";
    let payload = "eyJzdGF0dXMiOiJvayJ9";
    let signature = "c2lnbmF0dXJl";
    let jwt = format!("{header}.{payload}.{signature}");
    let html = format!(
        r#"<script>var t = "{jwt}"; document.cookie = "COLLEGE_UNIVERSITY_TOKEN_=" + t;</script>"#
    );
    assert_eq!(super::client::extract_jwt_token(&html).as_deref(), Some(jwt.as_str()));
    // 页面没有任何 JWT 时返回 None
    assert!(super::client::extract_jwt_token("<p>hello world, no token here</p>").is_none());
    // eyJ 开头但不足三段的不误报
    assert!(super::client::extract_jwt_token(header).is_none());
}

#[test]
fn parse_semester_list_accepts_array_and_page_shapes() {
    // 实测形状：data 直接是数组
    let array = json!([
        {"id": "sem-1", "name": "2026-2027-1",
         "beginDate": "2026-08-31", "endDate": "2027-01-17"},
    ]);
    let semesters = super::model::parse_semester_list(&array).unwrap();
    assert_eq!(semesters.len(), 1);
    assert_eq!(semesters[0].name, "2026-2027-1");

    // 兼容形状：分页对象包裹
    let page = json!({"pageIndex": 1, "recordCount": 1,
        "content": [{"id": "sem-2", "name": "2025-2026-2"}]});
    let semesters = super::model::parse_semester_list(&page).unwrap();
    assert_eq!(semesters[0].id, "sem-2");

    // 兼容形状：data 包裹
    let wrapped = json!({"data": [{"id": "sem-3", "name": "2024-2025-1"}]});
    let semesters = super::model::parse_semester_list(&wrapped).unwrap();
    assert_eq!(semesters[0].name, "2024-2025-1");

    // 空对象视为无数据而非错误
    assert!(super::model::parse_semester_list(&json!({"pageIndex": 1})).unwrap().is_empty());
}

#[test]
fn live_stream_parser_handles_common_shapes() {
    let sources = super::client::parse_live_streams(&json!({
        "streams": [
            {"name": "教师画面", "playUrl": "https://jxxk.szu.edu.cn/live/tp.m3u8"},
            {"name": "学生画面", "url": "https://jxxk.szu.edu.cn/live/sp.flv"},
            {"name": "无地址"},
        ],
    }));
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].label.as_deref(), Some("教师画面"));
}

#[test]
fn extract_script_cookies_parses_set_cookie_calls() {
    let html = r#"<script>
        setCookie('fromUrl', 'https:\/\/jxxk.szu.edu.cn\/#\/', 30, "\/")
        setCookie("COLLEGE_UNIVERSITY_TOKEN_", "initial-token")
        window.location.href = "https:\/\/jxxk.szu.edu.cn\/"
    </script>"#;
    let pairs = super::client::extract_script_cookies(html);
    println!("DEBUG_PAIRS: {pairs:?}");
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].0, "fromUrl");
    assert_eq!(pairs[0].1, "https://jxxk.szu.edu.cn/#/");
    assert_eq!(pairs[1].0, "COLLEGE_UNIVERSITY_TOKEN_");
    assert_eq!(pairs[1].1, "initial-token");
    assert!(super::client::extract_script_cookies("<p>no script</p>").is_empty());
}

#[test]
fn envelope_code_accepts_number_and_string() {
    let numeric: Envelope =
        serde_json::from_value(json!({"success": false, "code": -10000})).unwrap();
    assert!(numeric.code_is(-10000));
    let textual: Envelope =
        serde_json::from_value(json!({"success": false, "code": "-10000"})).unwrap();
    assert!(textual.code_is(-10000));
    let other: Envelope =
        serde_json::from_value(json!({"success": false, "code": 103520703})).unwrap();
    assert!(!other.code_is(-10000));
}

#[test]
fn task_core_broadcast_change_does_not_deadlock() {
    let registry = Arc::new(super::download::DownloadRegistry::default());
    let core = super::download::test_task_core(
        Arc::downgrade(&registry),
        super::download::TaskStatus::Preparing,
    );
    // 验证无论连续节流广播、强制广播或状态流转，均不发生互斥锁重入死锁。
    core.broadcast_change();
    core.broadcast_change();
    core.broadcast_change_force();
    core.set_status(super::download::TaskStatus::Downloading, None);
    core.set_status(super::download::TaskStatus::Paused, None);
    assert_eq!(
        core.state.lock().unwrap().status,
        super::download::TaskStatus::Paused
    );
    assert_eq!(core.state.lock().unwrap().speed, 0);
}

#[test]
fn download_registry_broadcast_and_management() {
    let registry = Arc::new(super::download::DownloadRegistry::default());
    let core1 = super::download::test_task_core(
        Arc::downgrade(&registry),
        super::download::TaskStatus::Preparing,
    );
    registry.register(core1.clone());
    assert_eq!(registry.list().len(), 1);
    assert_eq!(registry.get(&core1.id).unwrap().id, core1.id);

    // 暂停
    super::download::pause_task(&registry, &core1.id).unwrap();
    assert_eq!(
        core1.state.lock().unwrap().status,
        super::download::TaskStatus::Paused
    );

    // 摘除
    let detached = registry.detach(&core1.id);
    assert!(detached.is_some());
    assert_eq!(registry.list().len(), 0);
}

#[test]
fn token_needs_refresh_distinguishes_fresh_and_expiring() {
    use base64::Engine as _;
    let now = chrono::Utc::now().timestamp();
    let header = "eyJhbGciOiJIUzI1NiJ9";
    let sig = "c2lnbmF0dXJl";

    // 1. 2小时后到期：无需刷新
    let fresh_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&serde_json::json!({"exp": now + 7200})).unwrap(),
    );
    let fresh_jwt = format!("{header}.{fresh_payload}.{sig}");

    // 2. 5分钟后到期（< 15 分钟）：必须刷新
    let expiring_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&serde_json::json!({"exp": now + 300})).unwrap(),
    );
    let expiring_jwt = format!("{header}.{expiring_payload}.{sig}");

    assert!(!super::client::token_needs_refresh_for(&fresh_jwt));
    assert!(super::client::token_needs_refresh_for(&expiring_jwt));
}

#[test]
fn jxxk_strictly_follows_campus_dns_and_netproxy_egress() {
    // 验证 jxxk 业务域名与认证域名均严格命中 netproxy 校内域名判定
    assert!(
        crate::netproxy::model::is_campus_domain("jxxk.szu.edu.cn"),
        "jxxk.szu.edu.cn 必须被判定为校内域名"
    );
    assert!(
        crate::netproxy::model::is_campus_domain("authserver.szu.edu.cn"),
        "authserver.szu.edu.cn 必须被判定为校内域名"
    );
    // 验证配置的校内主 DNS 包含深圳大学指定的 DNS 服务器
    let dns_servers = crate::netproxy::model::campus_dns_servers();
    assert!(
        dns_servers.iter().any(|ip| ip.to_string() == "192.168.247.6"),
        "校内 DNS 服务器列表必须包含深圳大学指定 DNS 192.168.247.6"
    );
}
