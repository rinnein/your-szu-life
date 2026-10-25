use super::*;
use crate::storage::testing::MemoryCredentialStore;
use std::{
    io::{Read, Write},
    net::TcpListener,
};
fn setup() -> (tempfile::TempDir, Arc<Service>) {
    let dir = tempfile::tempdir().unwrap();
    let service = Service::new(
        None,
        Arc::new(MemoryCredentialStore::default()),
        dir.path().join("state.json"),
    )
    .unwrap();
    (dir, service)
}
fn input(url: &str) -> SiteInput {
    SiteInput {
        id: None,
        name: "测试 OJ".into(),
        url: url.into(),
        username: "student".into(),
        password: Some("fixture-password".into()),
        clear_credentials: false,
        reminders: true,
    }
}
fn fixture() -> Value {
    json!({"id":12,"group":{"id":3},"title":"练习","description":"<p>说明</p>","begin_time":"2026-10-04T00:00:00+08:00","end_time":"2026-10-05T00:00:00+08:00","now_time":"2026-10-04T12:00:00+08:00","problem_json":{"0":[{"id":1,"title":"相加","description":"<p>计算</p>","time_limit":1.5,"memory_limit":128,"sample_input":["  1 2\n"],"sample_output":["3"],"languages":{"cpp":true},"hint":"提示"}],"1":[{"id":2,"description":"选择","choice":["甲","乙"],"multiple":false}],"2":[{"id":3,"description":"[=] 和 [=]"}],"3":[{"id":4,"description":"解释"}],"4":[{"id":5,"title":"补全","languages":["py"],"code_template":{"py":"print([=])"}}]},"problem_score":{"0":{"1":20}}})
}
fn contest(id: &str, begin: i64, end: i64, now: i64) -> Contest {
    Contest {
        id: id.into(),
        group_id: "1".into(),
        title: "测验".into(),
        description: String::new(),
        begin,
        end,
        now,
        is_exam: false,
    }
}
// A bounded HTTP double; no test touches the user's OJ account or keyring.
fn server(responses: Vec<(u16, Value)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut captured = vec![];
        for (status, value) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(4)))
                .unwrap();
            let mut raw = vec![];
            let mut buffer = [0; 4096];
            loop {
                let n = stream.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buffer[..n]);
                if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&raw[..end]);
                    let len = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            captured.push(String::from_utf8_lossy(&raw).into_owned());
            let body = value.to_string();
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        captured
    });
    (url, handle)
}
#[test]
fn nine_defaults_preserve_addresses_and_restrictions() {
    let rows = defaults();
    assert_eq!(rows.len(), 9);
    assert_eq!(rows[2].url, "http://172.31.221.56/");
    assert!(rows[2].name.contains("老师"));
    assert!(rows[8].name.contains("关闭学生"));
}
#[test]
fn url_validation_preserves_subpath() {
    assert_eq!(
        client::normalize("http://example.test/oj")
            .unwrap()
            .as_str(),
        "http://example.test/oj/"
    );
    for url in [
        "file:///etc/passwd",
        "https://user:pw@example.test/",
        "http://a/?x=1",
        "http://a/#x",
    ] {
        assert!(client::normalize(url).is_err())
    }
}
#[test]
fn credentials_survive_service_restart_through_encrypted_vault() {
    // 回归：OJ 账号原为单一 JSON 条目，改为加密文档后必须仍能跨重启还原账号与会话。
    // 直接驱动 vault 与文档层，不依赖全局单例。
    use crate::storage::{
        entries::{self, VaultDocument},
        vault::{Vault, KEY_LEN},
    };
    let dir = tempfile::tempdir().unwrap();
    let key = [11u8; KEY_LEN];
    let raw = r#"{"szu-1":{"username":"student","password":"fixture-password","url":"http://example.test/","cookies":"session=abc"}}"#;

    let vault = Vault::open(dir.path().to_path_buf(), key);
    vault
        .update(|document| entries::set_oj_raw(document, raw))
        .unwrap();

    // 重新打开，模拟应用重启。
    let reopened = Vault::open(dir.path().to_path_buf(), key);
    let loaded = entries::load_oj_accounts(&reopened).unwrap().unwrap();
    let parsed: std::collections::BTreeMap<String, super::model::Secret> =
        serde_json::from_str(&loaded).unwrap();
    assert_eq!(parsed["szu-1"].cookies, "session=abc");
    assert_eq!(parsed["szu-1"].password, "fixture-password");

    // 文档必须与设置导出共用结构：账号与会话分列两个键。
    let document: VaultDocument = reopened.document().unwrap();
    assert!(document.entries.contains_key(entries::OJ_ACCOUNTS));
    assert!(document.entries.contains_key(entries::OJ_SESSIONS));
}

#[test]
fn configuration_enables_without_network_and_never_returns_password() {
    let (_d, s) = setup();
    let id = s.save_site(input("http://example.test/")).unwrap();
    let snapshot = s.snapshot();
    assert_eq!(snapshot.sites[0].site.id, id);
    assert!(snapshot.sites[0].enabled);
    assert!(!serde_json::to_string(&snapshot)
        .unwrap()
        .contains("fixture-password"));
}
#[test]
fn changing_url_requires_password_and_cancels_old_session() {
    let (_d, s) = setup();
    let id = s.save_site(input("http://example.test/")).unwrap();
    let e = s.entry(&id).unwrap();
    let mut i = input("http://new.test/");
    i.id = Some(id.clone());
    i.password = None;
    assert!(s.save_site(i).is_err());
    let mut i = input("http://new.test/");
    i.id = Some(id.clone());
    s.save_site(i).unwrap();
    assert!(e.client.cancel.is_cancelled());
    assert_ne!(e.epoch, s.entry(&id).unwrap().epoch);
}
#[test]
fn account_changes_reject_old_drafts() {
    let (_d, s) = setup();
    let id = s.save_site(input("http://example.test/")).unwrap();
    let e = s.entry(&id).unwrap();
    let mut i = input("http://example.test/");
    i.id = Some(id.clone());
    i.username = "other".into();
    s.save_site(i).unwrap();
    assert!(s
        .save_draft(DraftInput {
            site_id: id,
            contest_id: "1".into(),
            problem_key: "0:1".into(),
            account: e.username.clone(),
            epoch: e.epoch,
            draft: Draft::default()
        })
        .is_err());
}
#[test]
fn language_drafts_restore_independently() {
    let (_d, s) = setup();
    let id = s.save_site(input("http://example.test/")).unwrap();
    let e = s.entry(&id).unwrap();
    for (language, code) in [("cpp", "int main(){}"), ("py", "print(1)")] {
        s.save_draft(DraftInput {
            site_id: id.clone(),
            contest_id: "1".into(),
            problem_key: "0:1".into(),
            account: e.username.clone(),
            epoch: e.epoch,
            draft: Draft {
                language: language.into(),
                code: code.into(),
                revision: 1,
                ..Default::default()
            },
        })
        .unwrap();
    }
    assert_eq!(
        s.draft_for_language(&id, "1", "0:1", "cpp")
            .unwrap()
            .unwrap()
            .code,
        "int main(){}"
    );
}
#[test]
fn parse_all_problem_types_without_solutions() {
    let raw = fixture();
    let p = protocol::problems(&raw).unwrap();
    assert_eq!(p.len(), 5);
    assert_eq!(p[0].time_limit, 1.5);
    assert_eq!(p[2].blanks, 2);
    assert!(!p[1].multiple);
    assert_eq!(p[4].templates["py"], "print([=])");
}
#[test]
fn malformed_samples_fail_instead_of_silently_dropping() {
    let mut raw = fixture();
    raw["problem_json"]["0"][0]["sample_output"] = json!([]);
    assert!(protocol::problems(&raw).is_err());
}
#[test]
fn companion_converts_seconds_and_preserves_whitespace() {
    let raw = fixture();
    let c = protocol::contest(&raw).unwrap();
    let p = protocol::problems(&raw).unwrap();
    let result = protocol::companion(&defaults()[0], &c, &p[0], "batch", 2, true).unwrap();
    assert_eq!(result["timeLimit"], 1500);
    assert_eq!(result["tests"][0]["input"], "  1 2\n");
    assert_eq!(result["tests"][0]["output"], "3\n");
    assert_eq!(result["batch"]["size"], 2);
}
#[test]
fn noncode_answers_use_szu_contract() {
    let p = protocol::problems(&fixture()).unwrap();
    let drafts = BTreeMap::from([
        (
            "1:2".into(),
            Draft {
                answer: json!([1]),
                ..Default::default()
            },
        ),
        (
            "2:3".into(),
            Draft {
                answer: json!(["a", "b"]),
                ..Default::default()
            },
        ),
        (
            "3:4".into(),
            Draft {
                answer: json!("解释"),
                ..Default::default()
            },
        ),
    ]);
    let body = protocol::noncode_body("12", &p, &drafts).unwrap();
    assert_eq!(body["choice_submission"]["2"], json!([1]));
    assert_eq!(body["qa_submission_list"][0]["qa_problem_id"], 4);
    assert!(body.get("code").is_none());
}
#[test]
fn initial_baseline_does_not_notify_old_contests() {
    let mut known = Known::default();
    assert!(monitor::changes(&mut known, &[contest("1", 100, 9_000_000, 0)], 0).is_empty());
    assert!(known.initialized);
}
#[test]
fn new_start_due_are_deduplicated_and_deadline_changes_reschedule() {
    let mut known = Known::default();
    monitor::changes(&mut known, &[contest("1", 100, 4_000_000, 0)], 0);
    assert_eq!(
        monitor::changes(&mut known, &[contest("1", 100, 4_000_000, 100)], 100).len(),
        1
    );
    assert!(monitor::changes(&mut known, &[contest("1", 100, 4_000_000, 200)], 200).is_empty());
    assert_eq!(
        monitor::changes(
            &mut known,
            &[contest("1", 100, 4_000_000, 500_000)],
            500_000
        )
        .len(),
        1
    );
    assert!(monitor::changes(
        &mut known,
        &[contest("1", 100, 4_000_000, 600_000)],
        600_000
    )
    .is_empty());
    assert_eq!(
        monitor::changes(
            &mut known,
            &[contest("1", 100, 4_100_000, 600_000)],
            600_000
        )
        .len(),
        1
    );
}
#[test]
fn identifiers_are_scoped_by_site_and_account() {
    assert_ne!(scope("a", "user"), scope("b", "user"));
    assert_ne!(scope("a", "user"), scope("a", "other"));
}
#[test]
fn persistence_keeps_reminder_state() {
    let (_d, s) = setup();
    {
        let mut r = s.runtime.lock().unwrap();
        let k = r.data.known.entry(scope("szu-67", "student")).or_default();
        monitor::changes(k, &[contest("1", 0, 2000, 1000)], 1000);
        s.persist(&r).unwrap();
    }
    let data = persistence::read(&s.path).unwrap();
    assert_eq!(data.known[&scope("szu-67", "student")].events.len(), 1);
}
#[tokio::test]
async fn paginated_list_uses_all_pages() {
    let (url, handle) = server(vec![
        (200, json!({"count":2,"results":[{"id":1}]})),
        (200, json!({"count":2,"results":[{"id":2}]})),
    ]);
    let client = Client::new(&url, "").unwrap();
    assert_eq!(client.list("groups/", &[]).await.unwrap().len(), 2);
    let requests = handle.join().unwrap();
    assert!(requests[1].contains("offset=1"));
}
#[tokio::test]
async fn incomplete_pagination_is_an_error() {
    let (url, handle) = server(vec![
        (200, json!({"count":2,"results":[{"id":1}]})),
        (200, json!({"count":2,"results":[]})),
    ]);
    assert!(Client::new(&url, "")
        .unwrap()
        .list("groups/", &[])
        .await
        .is_err());
    handle.join().unwrap();
}
#[tokio::test]
async fn forbidden_is_not_session_expiry() {
    let (url, handle) = server(vec![(403, json!({}))]);
    let error = Client::new(&url, "")
        .unwrap()
        .get("groups/", &[])
        .await
        .unwrap_err();
    assert_eq!(error.status, Some(403));
    assert!(!error.uncertain);
    handle.join().unwrap();
}
#[tokio::test]
async fn login_failure_includes_bounded_raw_response() {
    let (url, handle) = server(vec![
        (403, json!({"detail":"账号被拒绝","trace":"fixture"})),
        (200, json!({"captcha_key":"key","captcha_image":"aW1hZ2U="})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().captcha = Some(("token".into(), "server-key".into(), "image".into()));
    let error = s
        .login(&id, &e, Some(("token".into(), "ABCD".into())))
        .await
        .unwrap_err();
    assert!(error.contains("HTTP 403"));
    assert!(error.contains("账号被拒绝"));
    assert!(error.contains("原始响应"));
    handle.join().unwrap();
}
#[tokio::test]
async fn failing_post_is_not_retried() {
    let (url, handle) = server(vec![(503, json!({}))]);
    let error = Client::new(&url, "")
        .unwrap()
        .request(
            reqwest::Method::POST,
            "code-submission/",
            &[],
            Some(&json!({"code":"x"})),
        )
        .await
        .unwrap_err();
    assert!(error.uncertain);
    assert_eq!(handle.join().unwrap().len(), 1);
}
#[tokio::test]
async fn cancelled_account_cannot_send() {
    let client = Client::new("http://127.0.0.1:1/", "").unwrap();
    client.cancel.cancel();
    assert!(client
        .get("groups/", &[])
        .await
        .unwrap_err()
        .message
        .contains("变化"));
}
#[test]
fn bundled_ocr_runs_real_model() {
    let output = ocr::recognize(include_bytes!("fixtures/captcha.png")).unwrap();
    assert_eq!(output.to_uppercase(), "HIUZ");
}
#[tokio::test]
async fn uncertain_submission_is_persisted_and_cannot_repeat() {
    let mut raw = fixture();
    raw["problem_json"] = json!({"0":raw["problem_json"]["0"].clone()});
    let (url, handle) = server(vec![
        (200, raw),
        (200, json!({"count":0,"results":[]})),
        (503, json!({})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    s.save_draft(DraftInput {
        site_id: id.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        contest_id: "12".into(),
        problem_key: "0:1".into(),
        draft: Draft {
            language: "cpp".into(),
            code: "int main(){}".into(),
            revision: 1,
            ..Default::default()
        },
    })
    .unwrap();
    let request = || SubmitInput {
        site_id: id.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        contest_id: "12".into(),
        problem_key: Some("0:1".into()),
    };
    assert!(s.submit(request()).await.is_err());
    assert_eq!(persistence::read(&s.path).unwrap().uncertain.len(), 1);
    assert!(s.submit(request()).await.unwrap_err().contains("待确认"));
    let requests = handle.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[2].starts_with("POST /api/code-submission/"));
}
#[tokio::test]
async fn manual_captcha_keeps_credentials_native_and_uses_csrf() {
    let (url, handle) = server(vec![
        (200, json!({"id":9})),
        (200, json!({"id":9,"username":"student"})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.client
        .jar
        .lock()
        .unwrap()
        .parse("csrftoken=fixture-csrf; Path=/", &e.client.base)
        .unwrap();
    e.state.lock().unwrap().captcha = Some(("token".into(), "server-key".into(), "image".into()));
    s.login(&id, &e, Some(("token".into(), "ABCD".into())))
        .await
        .unwrap();
    assert_eq!(e.state.lock().unwrap().session, Session::Ready);
    let requests = handle.join().unwrap();
    assert!(requests[0]
        .to_ascii_lowercase()
        .contains("x-csrftoken: fixture-csrf"));
    assert!(requests[0].contains("\"captcha_key\":\"server-key\""));
    assert!(!serde_json::to_string(&s.snapshot())
        .unwrap()
        .contains("fixture-password"));
}
#[tokio::test]
async fn companion_sender_posts_complete_batch_without_oj_cookies() {
    let mut raw = fixture();
    let first = raw["problem_json"]["0"][0].clone();
    let mut second = first.clone();
    second["id"] = json!(8);
    raw["problem_json"] = json!({"0":[first,second]});
    let (url, origin) = server(vec![(200, raw), (200, json!({"count":0,"results":[]}))]);
    let (receiver, received) = server(vec![(200, json!({})), (200, json!({}))]);
    let port = reqwest::Url::parse(&receiver).unwrap().port().unwrap();
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    e.client
        .jar
        .lock()
        .unwrap()
        .parse("sessionid=private-fixture; Path=/", &e.client.base)
        .unwrap();
    let response = s.send_companion(&id, "12", vec![], port).await.unwrap();
    assert_eq!(response["sent"], 2);
    origin.join().unwrap();
    let requests = received.join().unwrap();
    let values: Vec<Value> = requests
        .iter()
        .map(|r| serde_json::from_str(r.split_once("\r\n\r\n").unwrap().1).unwrap())
        .collect();
    for request in &requests {
        assert!(request.starts_with("POST / HTTP/1.1"));
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
    }
    assert_eq!(values[0]["batch"]["id"], values[1]["batch"]["id"]);
    assert_eq!(values[0]["batch"]["size"], 2);
}
#[test]
fn letter_labels_follow_each_type_and_survive_numeric_ids() {
    for (index, expected) in [
        (0, "A"),
        (25, "Z"),
        (26, "AA"),
        (27, "AB"),
        (51, "AZ"),
        (52, "BA"),
        (701, "ZZ"),
        (702, "AAA"),
    ] {
        assert_eq!(protocol::display_label(index), expected);
    }
    let mut value = fixture();
    let mut second = value["problem_json"]["0"][0].clone();
    second["id"] = json!(99);
    value["problem_json"]["0"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let problems = protocol::problems(&value).unwrap();
    assert_eq!(problems[0].display_label, "A");
    assert_eq!(problems[1].display_label, "B");
    assert_eq!(problems[1].id, "99");
    assert_eq!(problems[2].display_label, "A");
}
#[test]
fn compiler_defaults_dependencies_and_wire_format() {
    assert_eq!(
        CompileOptions::for_language("c", None)
            .unwrap()
            .unwrap()
            .version,
        "c17"
    );
    assert_eq!(
        CompileOptions::for_language("cpp", None)
            .unwrap()
            .unwrap()
            .version,
        "c++20"
    );
    let invalid = CompileOptions {
        version: "c++20".into(),
        enable_asan: false,
        enable_lsan: true,
    };
    assert!(CompileOptions::for_language("cpp", Some(&invalid)).is_err());
    let valid = CompileOptions {
        enable_asan: true,
        ..invalid
    };
    assert_eq!(
        valid.wire(),
        json!({"version":"c++20","enable_asan":true,"enable_lsan":true})
    );
    assert!(CompileOptions::for_language("c", Some(&valid)).is_err());
    assert!(CompileOptions::for_language("py", Some(&valid))
        .unwrap()
        .is_none());
}
#[test]
fn legacy_options_and_language_specific_persistence_remain_readable() {
    let draft: Draft = serde_json::from_value(
        json!({"code":"","language":"cpp","answer":"","options":null,"revision":1}),
    )
    .unwrap();
    assert_eq!(
        CompileOptions::for_language(&draft.language, draft.options.as_ref())
            .unwrap()
            .unwrap()
            .version,
        "c++20"
    );
    let old: CompileOptions =
        serde_json::from_value(json!({"version":"c++14","enable_asan":true,"enable_lsan":false}))
            .unwrap();
    assert_eq!(
        CompileOptions::for_language("cpp", Some(&old))
            .unwrap()
            .unwrap()
            .version,
        "c++14"
    );
    let (_dir, s) = setup();
    let id = s.save_site(input("http://oj.test/")).unwrap();
    let e = s.entry(&id).unwrap();
    s.save_draft(DraftInput {
        site_id: id.clone(),
        contest_id: "12".into(),
        problem_key: "0:1".into(),
        account: e.username.clone(),
        epoch: e.epoch,
        draft: Draft {
            options: Some(old.clone()),
            revision: 2,
            ..draft
        },
    })
    .unwrap();
    assert_eq!(
        s.draft_for_language(&id, "12", "0:1", "cpp")
            .unwrap()
            .unwrap()
            .options,
        Some(old)
    );
}
#[test]
fn single_and_bulk_companion_names_never_include_chinese() {
    let value = fixture();
    let c = protocol::contest(&value).unwrap();
    let p = protocol::problems(&value).unwrap();
    let single = protocol::companion(&defaults()[0], &c, &p[0], "one", 1, false).unwrap();
    let bulk = protocol::companion(&defaults()[0], &c, &p[0], "all", 1, true).unwrap();
    assert_eq!(single["name"], "A");
    assert_eq!(bulk["name"], "A");
    assert_eq!(single["tests"], bulk["tests"]);
    assert_eq!(single["batch"]["size"], 1);
}
#[tokio::test]
async fn python_submission_omits_compiler_options() {
    let mut raw = fixture();
    let mut p = raw["problem_json"]["0"][0].clone();
    p["languages"] = json!({"py":true});
    raw["problem_json"] = json!({"0":[p]});
    let (url, handle) = server(vec![
        (200, raw),
        (200, json!({"count":0,"results":[]})),
        (200, json!({"id":10})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    s.save_draft(DraftInput {
        site_id: id.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        contest_id: "12".into(),
        problem_key: "0:1".into(),
        draft: Draft {
            language: "py".into(),
            code: "print(1)".into(),
            revision: 1,
            ..Default::default()
        },
    })
    .unwrap();
    s.submit(SubmitInput {
        site_id: id,
        account: e.username.clone(),
        epoch: e.epoch,
        contest_id: "12".into(),
        problem_key: Some("0:1".into()),
    })
    .await
    .unwrap();
    let requests = handle.join().unwrap();
    let body: Value = serde_json::from_str(requests[2].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert!(body.get("options").is_none());
}
#[test]
fn instance_preferences_persist_across_problems_and_reload() {
    let (dir, s) = setup();
    let id = s.save_site(input("http://oj.test/")).unwrap();
    let e = s.entry(&id).unwrap();
    let preferences = s
        .save_preferences(PreferencesInput {
            site_id: id.clone(),
            account: e.username.clone(),
            epoch: e.epoch,
            expected_revision: 0,
            language: "cpp".into(),
            options: Some(CompileOptions {
                version: "c++17".into(),
                enable_asan: true,
                enable_lsan: true,
            }),
        })
        .unwrap();
    assert_eq!(preferences.revision, 1);
    let data = persistence::read(&dir.path().join("state.json")).unwrap();
    assert_eq!(data.preferences[&id].language, "cpp");
    assert_eq!(
        data.preferences[&id].options().unwrap().unwrap().version,
        "c++17"
    );
    s.save_draft(DraftInput {
        site_id: id.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        contest_id: "12".into(),
        problem_key: "0:99".into(),
        draft: Draft {
            language: "c".into(),
            revision: 50,
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(
        Service::preferences_locked(&s.runtime.lock().unwrap(), &id, &e.username).language,
        "cpp"
    );
    assert!(s
        .save_preferences(PreferencesInput {
            site_id: id,
            account: e.username.clone(),
            epoch: e.epoch,
            expected_revision: 0,
            language: "c".into(),
            options: None
        })
        .is_err());
}
#[test]
fn preferences_are_instance_scoped_and_keep_each_language_options() {
    let (_dir, s) = setup();
    let a = s.save_site(input("http://one.test/")).unwrap();
    let b = s.save_site(input("http://two.test/")).unwrap();
    let e = s.entry(&a).unwrap();
    s.save_preferences(PreferencesInput {
        site_id: a.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        expected_revision: 0,
        language: "cpp".into(),
        options: Some(CompileOptions {
            version: "c++14".into(),
            enable_asan: true,
            enable_lsan: false,
        }),
    })
    .unwrap();
    let p = s
        .save_preferences(PreferencesInput {
            site_id: a.clone(),
            account: e.username.clone(),
            epoch: e.epoch,
            expected_revision: 1,
            language: "py".into(),
            options: None,
        })
        .unwrap();
    assert_eq!(p.language, "py");
    assert_eq!(p.options_by_language["cpp"].version, "c++14");
    assert_eq!(
        Service::preferences_locked(&s.runtime.lock().unwrap(), &b, "student").language,
        "cpp"
    );
}
#[test]
fn companion_urls_are_resolved_only_with_exact_instance_and_problem() {
    let sites = defaults();
    assert_eq!(
        companion::target(
            &sites,
            "http://172.31.221.67/#/contest/12/?problem=99&type=0"
        )
        .unwrap(),
        ("szu-67".into(), "12".into(), "0:99".into())
    );
    for url in [
        "http://evil.test/#/contest/12/?problem=99&type=0",
        "http://172.31.221.67/#/contest/12/?problem=99&type=3",
        "http://172.31.221.67/#/contest/12/?problem=99&type=0&extra=1",
        "http://172.31.221.67/#//evil.test/contest/12/?problem=99&type=0",
    ] {
        assert!(companion::target(&sites, url).is_err());
    }
}
#[tokio::test]
async fn cph_submission_uses_instance_options_and_caches_ide_code() {
    let mut raw = fixture();
    raw["problem_json"] = json!({"0":raw["problem_json"]["0"].clone()});
    let (url, handle) = server(vec![
        (200, raw),
        (200, json!({"count":0,"results":[]})),
        (200, json!({"id":123})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    s.save_preferences(PreferencesInput {
        site_id: id.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        expected_revision: 0,
        language: "cpp".into(),
        options: Some(CompileOptions {
            version: "c++17".into(),
            enable_asan: true,
            enable_lsan: true,
        }),
    })
    .unwrap();
    s.save_draft(DraftInput {
        site_id: id.clone(),
        account: e.username.clone(),
        epoch: e.epoch,
        contest_id: "12".into(),
        problem_key: "0:1".into(),
        draft: Draft {
            language: "cpp".into(),
            code: "app-code".into(),
            revision: 1,
            ..Default::default()
        },
    })
    .unwrap();
    s.companion_status(
        27121,
        CompanionStatus {
            connected: true,
            active: true,
            message: String::new(),
        },
    );
    let source_url = format!("{url}#/contest/12/?problem=1&type=0");
    let result = s
        .companion_submit(CompanionSubmission {
            url: source_url.clone(),
            source_code: "ide-code".into(),
        })
        .await
        .unwrap();
    assert_eq!(result.submission["id"], 123);
    assert_eq!(
        s.draft_for_language(&id, "12", "0:1", "cpp")
            .unwrap()
            .unwrap()
            .code,
        "ide-code"
    );
    let stored = persistence::read(&s.path).unwrap();
    let draft = &stored.drafts[&draft_key(&id, &e.username, "12", "0:1", "cpp")];
    assert_eq!(draft.code, "ide-code");
    assert_eq!(draft.options.as_ref().unwrap().version, "c++17");
    assert!(draft.revision > 1);
    assert!(s
        .companion_submit(CompanionSubmission {
            url: source_url,
            source_code: "ide-code".into()
        })
        .await
        .unwrap_err()
        .contains("刚刚"));
    let requests = handle.join().unwrap();
    let body: Value = serde_json::from_str(requests[2].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["code"], "ide-code");
    assert_eq!(body["language"], "cpp");
    assert_eq!(
        body["options"],
        json!({"version":"c++17","enable_asan":true,"enable_lsan":true})
    );
}
#[test]
fn saving_unchanged_companion_configuration_keeps_active_connection() {
    let (_dir, s) = setup();
    s.companion_status(
        27121,
        CompanionStatus {
            connected: true,
            active: true,
            message: String::new(),
        },
    );
    s.configure_companion(true, 27121).unwrap();
    assert!(s.snapshot().companion_status.active);
    s.configure_companion(false, 27121).unwrap();
    assert!(!s.snapshot().companion_status.active);
}

#[tokio::test]
async fn ide_code_is_cached_even_when_the_site_cannot_load_the_paper() {
    let (url, handle) = server(vec![(503, json!({}))]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    let code = "  int main() {} \n\n";
    let result = s
        .submit_with_code(
            SubmitInput {
                site_id: id.clone(),
                contest_id: "12".into(),
                problem_key: Some("0:1".into()),
                account: e.username.clone(),
                epoch: e.epoch,
            },
            Some(code.into()),
        )
        .await;
    assert!(result.is_err());
    let stored = persistence::read(&s.path).unwrap();
    assert_eq!(
        stored.drafts[&draft_key(&id, &e.username, "12", "0:1", "cpp")].code,
        code
    );
    assert_eq!(handle.join().unwrap().len(), 1);
}

#[tokio::test]
async fn records_filter_problem_after_pagination_and_scope_uncertainty() {
    let (url, handle) = server(vec![
        (
            200,
            json!({"count":3,"results":[{"id":1,"problem":1,"status":0},{"id":2,"problem":2,"status":-1}]}),
        ),
        (
            200,
            json!({"count":3,"results":[{"id":3,"problem":{"id":"1"},"status":1}]}),
        ),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    {
        let mut r = s.runtime.lock().unwrap();
        for pk in ["0:1", "0:2", "4:1"] {
            r.data.uncertain.insert(
                operation_key(&id, &e.username, "12", Some(pk)),
                json!({"time":0,"problemKey":pk}),
            );
        }
    }
    let records = s.records(&id, "12", 0, Some("1")).await.unwrap();
    assert_eq!(records["records"].as_array().unwrap().len(), 2);
    assert_eq!(records["records"][1]["id"], 3);
    assert_eq!(records["uncertain"].as_array().unwrap().len(), 1);
    let requests = handle.join().unwrap();
    assert!(requests.iter().all(|r| r.contains("problem=1")));
    assert!(requests[1].contains("offset=2"));
}

#[test]
fn noncode_records_never_expose_other_problem_answers() {
    let row = json!({"id":1,"time":"now","choice_submission":{"1":[0],"2":[1]},"fill_in_submission":{"1":["secret"]},"status":0});
    let value = protocol::record_for_problem(&row, 1, "1").unwrap();
    assert_eq!(value["answer"], json!([0]));
    assert_eq!(value["problem"], "1");
    assert!(value.get("choice_submission").is_none());
    assert!(value.get("status").is_none());
    assert!(protocol::record_for_problem(&row, 1, "3").is_none());
}

#[test]
fn sample_spaces_tabs_blank_lines_and_crlf_are_preserved() {
    let mut raw = fixture();
    let text = "\n  1 \t\r\n2  \r\n\r\n";
    raw["problem_json"]["0"][0]["sample_input"] = json!([text]);
    raw["problem_json"]["0"][0]["sample_output"] = json!(["  \t"]);
    let problems = protocol::problems(&raw).unwrap();
    assert_eq!(problems[0].samples[0].input, text);
    assert_eq!(problems[0].samples[0].output, "  \t");
    let payload = protocol::companion(
        &defaults()[0],
        &protocol::contest(&raw).unwrap(),
        &problems[0],
        "batch",
        1,
        false,
    )
    .unwrap();
    assert_eq!(payload["tests"][0]["input"], text);
    assert_eq!(payload["tests"][0]["output"], "  \t\n");
}

#[tokio::test]
async fn completion_uses_all_visible_problem_ids_not_just_a_score_count() {
    let mut raw = fixture();
    raw["problem_json"] = json!({"0": [raw["problem_json"]["0"][0].clone(), {"id":2,"title":"B"}]});
    let (url, handle) = server(vec![
        (200, raw),
        (
            200,
            json!({"data":{"score":5,"submission_info":{"1":{"accepted":true},"2":{"accepted":true},"99":{"accepted":true}}}}),
        ),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    assert_eq!(
        {
            let result = s.progress(&id, "12", false, None, None).await.unwrap();
            json!({"accepted":result["accepted"],"total":result["total"]})
        },
        json!({"accepted":2,"total":2})
    );
    assert!(handle.join().unwrap()[1].starts_with("GET /api/contests/12/self_rank/"));
}
#[tokio::test]
async fn ranking_respects_visibility_and_uses_student_rank_pagination() {
    let mut raw = fixture();
    raw["show_ranking"] = json!(false);
    let (url, handle) = server(vec![(200, raw)]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    assert!(s
        .ranking(&id, "12", 0)
        .await
        .unwrap_err()
        .contains("未开放"));
    assert_eq!(handle.join().unwrap().len(), 1);
    let (url, handle) = server(vec![
        (200, fixture()),
        (
            200,
            json!({"count":21,"results":[{"user":{"username":"fixture","name":"测试","secret":"not-exported"},"score":1,"penalty":"00:20:00","submission_info":{"1":{"accepted":true,"time_usage":0,"error":1}}}]}),
        ),
    ]);
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    let result = s.ranking(&id, "12", 20).await.unwrap();
    assert_eq!(result["rows"][0]["rank"], 21);
    assert_eq!(result["rows"][0]["problems"]["1"]["seconds"], 0.0);
    assert!(!result.to_string().contains("secret"));
    let requests = handle.join().unwrap();
    assert!(requests[1].starts_with("GET /api/contests/12/rank/?offset=20&limit=20"));
}

#[tokio::test]
async fn progress_cache_is_shared_across_callers_and_manual_refresh_bypasses_it() {
    let mut raw = fixture();
    raw["problem_json"] = json!({"0":raw["problem_json"]["0"].clone()});
    let (url, handle) = server(vec![
        (200, raw.clone()),
        (
            200,
            json!({"data":{"submission_info":{"1":{"accepted":true}}}}),
        ),
        (200, raw),
        (200, json!({"data":{"submission_info":{}}})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    let (first, second) = tokio::join!(
        s.progress(&id, "12", false, Some(&e.username), Some(e.epoch)),
        s.progress(&id, "12", false, Some(&e.username), Some(e.epoch))
    );
    assert_eq!(first.as_ref().unwrap()["accepted"], 1);
    assert_eq!(first, second);
    assert_eq!(
        s.progress(&id, "12", false, None, None).await.unwrap()["accepted"],
        1
    );
    assert!(s
        .progress(&id, "12", false, Some("wrong-account"), None)
        .await
        .is_err());
    assert!(s
        .progress(&id, "12", false, None, Some(e.epoch + 1))
        .await
        .is_err());
    assert_eq!(
        s.progress(&id, "12", true, None, None).await.unwrap()["accepted"],
        0
    );
    assert_eq!(handle.join().unwrap().len(), 4);
}
#[tokio::test]
async fn terminal_judging_invalidates_only_its_contest_and_only_when_status_changes() {
    let mut raw = fixture();
    raw["problem_json"] = json!({"0":raw["problem_json"]["0"].clone()});
    let judged = json!({"id":11,"contest":{"id":12},"problem":1,"status":0});
    let (url, handle) = server(vec![
        (200, raw.clone()),
        (200, json!({"data":{"submission_info":{}}})),
        (200, judged.clone()),
        (200, raw),
        (
            200,
            json!({"data":{"submission_info":{"1":{"accepted":true}}}}),
        ),
        (200, judged),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    assert_eq!(
        s.progress(&id, "12", false, None, None).await.unwrap()["accepted"],
        0
    );
    e.state.lock().unwrap().progress.insert(
        "99".into(),
        (
            chrono::Utc::now().timestamp_millis(),
            json!({"accepted":1,"total":1}),
        ),
    );
    s.result(&id, "11", 0, Some("1")).await.unwrap();
    assert!(!e.state.lock().unwrap().progress.contains_key("12"));
    assert!(e.state.lock().unwrap().progress.contains_key("99"));
    assert_eq!(
        s.progress(&id, "12", false, None, None).await.unwrap()["accepted"],
        1
    );
    s.result(&id, "11", 0, Some("1")).await.unwrap();
    assert!(e.state.lock().unwrap().progress.contains_key("12"));
    assert_eq!(handle.join().unwrap().len(), 6);
}
#[tokio::test]
async fn expired_progress_refreshes_and_account_replacement_discards_the_cache() {
    let mut raw = fixture();
    raw["problem_json"] = json!({"0":raw["problem_json"]["0"].clone()});
    let (url, handle) = server(vec![
        (200, raw),
        (200, json!({"data":{"submission_info":{}}})),
    ]);
    let (_dir, s) = setup();
    let id = s.save_site(input(&url)).unwrap();
    let e = s.entry(&id).unwrap();
    e.state.lock().unwrap().session = Session::Ready;
    e.state.lock().unwrap().progress.insert(
        "12".into(),
        (
            chrono::Utc::now().timestamp_millis() - 3_600_000,
            json!({"accepted":1,"total":1}),
        ),
    );
    assert_eq!(
        s.progress(&id, "12", false, None, None).await.unwrap()["accepted"],
        0
    );
    let mut changed = input(&url);
    changed.id = Some(id.clone());
    changed.username = "other".into();
    s.save_site(changed).unwrap();
    assert!(s
        .entry(&id)
        .unwrap()
        .state
        .lock()
        .unwrap()
        .progress
        .is_empty());
    assert_eq!(handle.join().unwrap().len(), 2);
}
