use super::client::{Bootstrap, Client};
use super::model::*;
use super::session::parse_global_data;
use crate::storage::testing::MemoryCredentialStore;
use crate::storage::CredentialStore;
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

/// 最小 HTTP mock：raw TcpListener，与 uooc 测试同一套路。
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
}

struct Server {
    pub base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// `host` 用不同的回环地址（127.0.0.1 / 127.0.0.2）区分 lms 与 authserver
    /// 两个 cookie 域。handler 工厂先拿到自身 base，便于构造绝对跳转地址。
    pub fn start<H>(
        host: &str,
        make_handler: impl FnOnce(&str) -> H,
    ) -> Self
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
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .ok()?;
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

fn html_page() -> String {
    r#"<html><body><script>
    var globalData = {
        user: { id: 12345, name: "张三", userNo: "2020010000", orgId: 1, role: "Student" },
        dept: { id: 9, name: "xx学院", code: "02011000" }
    }
    </script></body></html>"#
        .into()
}

fn todos_body() -> Value {
    json!({
        "todo_list": [
            {
                "course_code": "202620271150037000307",
                "course_id": 31746,
                "course_name": "数据库系统",
                "course_type": 1,
                "end_time": "2026-11-03T15:59:00Z",
                "id": 223783,
                "is_locked": false,
                "is_student": true,
                "prerequisites": [],
                "title": "ADVANCED SQL",
                "type": "homework"
            }
        ]
    })
}

fn notifications_body() -> Value {
    json!({
        "notifications": [
            {
                "id": "6ae409c838a59c86eb247633",
                "type": "activity_expiring",
                "top": false,
                "timestamp": 1793329607457i64,
                "payload": {
                    "activity_id": 248039,
                    "activity_title": "第五次作业",
                    "activity_type": "homework",
                    "course_id": 32176,
                    "course_name": "概率论与数理统计",
                    "created_at": "2026-10-30T03:06:47Z",
                    "end_time": "2026-10-31T15:59:00Z",
                    "uploads": []
                },
                "unread": true
            }
        ],
        "unread_count": 1
    })
}

/// 起 lms + authserver 两个 mock 站点并返回客户端。
/// `with_castgc` 决定 authserver 侧是否有有效的 CAS 会话。
async fn make_client(with_castgc: bool) -> (Client, Server, Server) {
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
    let auth_base = auth_server.base.clone();
    let lms_server = Server::start("127.0.0.1", move |lms_base| {
        let lms_base = lms_base.to_string();
        let auth_login = format!("{auth_base}/authserver/login");
        Box::new(move |request: Request| {
            if request.path.starts_with("/api/todos") {
                if request.has_cookie("LMS_SESSION") {
                    (200, vec![], serde_json::to_vec(&todos_body()).unwrap())
                } else {
                    (302, vec![("Location".into(), lms_base.to_string())], Vec::new())
                }
            } else if request.path.starts_with("/ntf/users/12345/notifications") {
                (
                    200,
                    vec![],
                    serde_json::to_vec(&notifications_body()).unwrap(),
                )
            } else if request.has_cookie("LMS_SESSION") || request.path.contains("ticket=") {
                (
                    200,
                    vec![(
                        "Set-Cookie".into(),
                        "LMS_SESSION=established-session; Path=/".into(),
                    )],
                    html_page().into_bytes(),
                )
            } else {
                (
                    302,
                    vec![(
                        "Location".into(),
                        format!("{auth_login}?service={lms_base}/"),
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
        &lms_server.base,
        &format!("{}/authserver/login", auth_server.base),
        None,
        &sso_cookies,
    )
    .unwrap();
    (client, lms_server, auth_server)
}

#[tokio::test]
async fn bootstrap_establishes_session_via_cas_redirect_chain() {
    let (client, _lms, _auth) = make_client(true).await;
    match client.bootstrap().await.unwrap() {
        Bootstrap::Established(html) => {
            let profile = parse_global_data(&html).expect("profile");
            assert_eq!(profile.uid, 12345);
            assert_eq!(profile.name, "张三");
        }
        other => panic!("expected established session, got {other:?}"),
    }
    let todos = client.get_json("api/todos", &[]).await.unwrap();
    assert_eq!(todos["todo_list"][0]["title"], "ADVANCED SQL");
    let notifications = client
        .get_json(
            "ntf/users/12345/notifications",
            &[
                ("limit", "5".to_string()),
                ("additionalFields", "unread_count".to_string()),
                ("removed", "only_mobile".to_string()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(notifications["unread_count"], 1);
    // 保存会话时只保留畅课域 cookie，authserver 的 CAS 会话不得被复制。
    let cookies = client.cookies().unwrap();
    assert!(cookies.contains("LMS_SESSION"));
    assert!(!cookies.contains("CASTGC"));
}

#[tokio::test]
async fn bootstrap_without_cas_reports_needs_auth() {
    let (client, _lms, _auth) = make_client(false).await;
    assert!(matches!(
        client.bootstrap().await.unwrap(),
        Bootstrap::NeedsAuth
    ));
}

#[tokio::test]
async fn expired_lms_session_is_re_established_on_api_call() {
    // 初始客户端不带 LMS_SESSION：首次 /api/todos 返回 302（session_expired），
    // 引导逻辑应经 CAS 重建会话后成功。
    let (client, _lms, _auth) = make_client(true).await;
    let bootstrap = client.bootstrap().await.unwrap();
    assert!(matches!(bootstrap, Bootstrap::Established(_)));
    let todos = client.get_json("api/todos", &[]).await.unwrap();
    assert_eq!(todos["todo_list"].as_array().unwrap().len(), 1);
}

fn temp_db_path(tag: &str) -> std::path::PathBuf {
    static COUNTER: Mutex<u32> = Mutex::new(0);
    let mut counter = COUNTER.lock().unwrap();
    *counter += 1;
    std::env::temp_dir().join(format!(
        "lms-test-{}-{}-{tag}.sqlite",
        std::process::id(),
        *counter
    ))
}

fn test_store() -> Arc<MemoryCredentialStore> {
    Arc::new(MemoryCredentialStore::default())
}

#[tokio::test]
async fn service_refresh_persists_cache_and_session() {
    let (client, _lms, _auth) = make_client(true).await;
    let credentials = test_store();
    let path = temp_db_path("service");
    let service = super::Service::new(None, credentials.clone(), path.clone()).unwrap();
    service.refresh_with_client(&client).await;
    let snapshot = service.snapshot();
    assert_eq!(snapshot.session, SessionState::Ready);
    assert_eq!(snapshot.todos.len(), 1);
    assert_eq!(snapshot.todos[0].title, "ADVANCED SQL");
    assert_eq!(snapshot.notifications.len(), 1);
    assert_eq!(snapshot.unread_count, 1);
    assert_eq!(snapshot.profile.as_ref().unwrap().uid, 12345);
    assert!(snapshot.fetched_at.is_some());
    // 会话 jar 与缓存都应落盘。
    assert!(credentials
        .load_lms_session()
        .unwrap()
        .unwrap()
        .contains("LMS_SESSION"));
    let reopened = super::Service::new(None, credentials, path).unwrap();
    let restored = reopened.snapshot();
    assert_eq!(restored.session, SessionState::Ready);
    assert_eq!(restored.todos.len(), 1);
}

#[tokio::test]
async fn service_without_cas_marks_needs_auth_and_keeps_cache() {
    let (client, _lms, _auth) = make_client(false).await;
    let credentials = test_store();
    let service = super::Service::new(None, credentials, temp_db_path("needs-auth")).unwrap();
    service.refresh_with_client(&client).await;
    let snapshot = service.snapshot();
    assert_eq!(snapshot.session, SessionState::NeedsAuth);
    assert!(snapshot.todos.is_empty());
    assert!(snapshot.profile.is_none());
}

#[tokio::test]
async fn trusted_redirects_cover_szu_subdomains_only() {
    // 真实链路经 identity.szu.edu.cn（Keycloak broker）中转，须放行深大子域。
    let (client, _lms, _auth) = make_client(false).await;
    let trusted = |raw: &str| client.is_trusted_redirect(&raw.parse().unwrap());
    assert!(trusted("https://lms.szu.edu.cn/"));
    assert!(trusted("https://authserver.szu.edu.cn/authserver/login"));
    assert!(trusted("https://identity.szu.edu.cn/auth/realms/szu/protocol/cas/login"));
    assert!(!trusted("https://evil.example.com/"));
    assert!(!trusted("https://szu.edu.cn.evil.com/"));
}
