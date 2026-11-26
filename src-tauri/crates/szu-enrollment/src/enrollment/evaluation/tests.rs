//! 评教模块单测：档位规则、提交内容与本地 mock 服务的端到端链路。

use super::jxpj::JxpjClient;
use super::model::*;
use super::*;
use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

fn choice(label: &str, low: &str, high: &str, normal: &str) -> ChoiceOption {
    ChoiceOption {
        label: label.to_string(),
        value_low: low.to_string(),
        value_high: high.to_string(),
        value_normal: normal.to_string(),
    }
}

fn four_choices() -> Vec<ChoiceOption> {
    vec![
        choice("非常优秀", "90", "100", "92"),
        choice("良好", "60", "90", "65"),
        choice("一般", "30", "60", "30"),
        choice("待改进", "0", "30", "10"),
    ]
}

#[test]
fn picks_second_highest_tier_and_keeps_original_index() {
    let choices = four_choices();
    let index = choose_choice(&choices).expect("four choices");
    assert_eq!(index, 1);
    assert_eq!(choices[index].label, "良好");
    assert_eq!(choice_score(&choices[index]), 65.0);
}

#[test]
fn five_tiers_pick_second_highest_and_three_pick_middle() {
    let five = vec![
        choice("优秀", "90", "100", "95"),
        choice("良好", "80", "90", "85"),
        choice("中等", "70", "80", "75"),
        choice("及格", "60", "70", "65"),
        choice("不及格", "0", "60", "30"),
    ];
    assert_eq!(choose_choice(&five), Some(1));

    let three = vec![
        choice("满意", "80", "100", "90"),
        choice("一般", "50", "80", "60"),
        choice("不满意", "0", "50", "20"),
    ];
    assert_eq!(choose_choice(&three), Some(1));
}

#[test]
fn two_option_scale_takes_the_highest() {
    let two = vec![
        choice("是", "60", "100", "80"),
        choice("否", "0", "60", "30"),
    ];
    assert_eq!(choose_choice(&two), Some(0));
    assert_eq!(choose_choice(&[]), None);
}

#[test]
fn plan_label_and_score_follow_score_type() {
    let target = EvaluationTarget {
        id: "36229-24510-47-1187".into(),
        target_id: 1187,
        target_name: "常规课学生过程性评价指标-3".into(),
        term_name: "26-27学年第1学期".into(),
        expired_date: Some("2099-01-01".into()),
        status: TargetStatus::Pending,
        message: None,
        score_type: 4,
        comment_required: 2,
        stay_seconds: 0,
        records: vec![EvaluationRecord {
            sc_id: 2242443,
            course_name: "离散数学".into(),
            course_no: "1500210001".into(),
            employee_name: "教师甲".into(),
            class_name: "202620271150021000101".into(),
            current_times: 0,
            limited_times: 1,
            expired_date: Some("2099-01-01".into()),
            participate: true,
        }],
        questions: vec![EvaluationQuestion {
            sub_id: 1237,
            sub_name: "教师是否教态大方得体".into(),
            kind: "options".into(),
            choices: four_choices(),
        }],
    };

    let plan = build_plan(&target, "无").expect("plan");
    assert_eq!(plan.target_id, 1187);
    assert_eq!(plan.choices.len(), 1);
    assert_eq!(plan.choices[0].choice_label, "2242443_1237_良好_1");
    assert_eq!(plan.choices[0].score, Some(65.0));
    assert_eq!(plan.comments[0].comment, "无");

    // 单选模式（scoreType != 4）不写 score。
    let radio = EvaluationTarget {
        score_type: 1,
        ..target
    };
    let plan = build_plan(&radio, "无").expect("radio plan");
    assert_eq!(plan.choices[0].score, None);
}

#[test]
fn skips_expired_unparticipating_and_already_evaluated_records() {
    let record = |sc_id: i64, participate: bool, current: i64, expired: &str| EvaluationRecord {
        sc_id,
        course_name: "课程".into(),
        course_no: "1".into(),
        employee_name: "教师".into(),
        class_name: "班级".into(),
        current_times: current,
        limited_times: 1,
        expired_date: Some(expired.into()),
        participate,
    };
    let target = EvaluationTarget {
        id: "x".into(),
        target_id: 1,
        target_name: "指标".into(),
        term_name: "学期".into(),
        expired_date: Some("2099-01-01".into()),
        status: TargetStatus::Pending,
        message: None,
        score_type: 4,
        comment_required: 0,
        stay_seconds: 0,
        records: vec![
            record(1, true, 0, "2099-01-01"),
            record(2, false, 0, "2099-01-01"),
            record(3, true, 0, "2000-01-01"),
            record(4, true, 1, "2099-01-01"),
        ],
        questions: vec![EvaluationQuestion {
            sub_id: 10,
            sub_name: "题目".into(),
            kind: "options".into(),
            choices: four_choices(),
        }],
    };
    let ids: Vec<i64> = target
        .actionable_records()
        .map(|record| record.sc_id)
        .collect();
    assert_eq!(ids, vec![1]);
    assert_eq!(expired_count(&target), 1);
}

#[test]
fn unsupported_question_type_is_flagged() {
    let row = jxpj::TargetRow {
        id: "36229-1-47-1187".into(),
        target_name: "指标".into(),
        term_name: "学期".into(),
        pending: 1,
    };
    let detail = jxpj::Detail {
        target_id: 1187,
        target_name: "指标".into(),
        score_type: 4,
        comment_required: 0,
        stay_seconds: 0,
        records: vec![jxpj::Record {
            sc_id: 1,
            course_name: "课程".into(),
            course_no: "1".into(),
            employee_name: "教师".into(),
            class_name: "班级".into(),
            current_times: 0,
            limited_times: 1,
            expired_date: Some("2099-01-01".into()),
            participate: true,
        }],
        questions: vec![jxpj::Question {
            sub_id: 2,
            sub_name: "填空".into(),
            kind: "text".into(),
            choices: Vec::new(),
        }],
    };
    let target = build_target(&row, detail);
    assert_eq!(target.status, TargetStatus::Unsupported);
    assert!(target.message.is_some());
}

#[test]
fn empty_comment_falls_back_to_default() {
    assert_eq!(resolve_comment(Some("   ")), "无");
    assert_eq!(resolve_comment(Some(" 好课 ")), "好课");
    // 偏好数据库未初始化时读取失败，同样回落默认值。
    assert_eq!(resolve_comment(None), "无");
}

// ---------- 本地 mock 服务（raw TcpListener，与 lms 测试同一套路） ----------

struct Server {
    base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start<H>(host: &str, make_handler: impl FnOnce(&str) -> H) -> Self
    where
        H: Fn(&str, &str, &str) -> (u16, Vec<(String, String)>, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind(format!("{host}:0")).expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let base = format!("http://{address}");
        let handler: Arc<dyn Fn(&str, &str, &str) -> (u16, Vec<(String, String)>, String) + Send + Sync> =
            Arc::from(make_handler(&base));
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let handler = handler.clone();
                        workers.push(std::thread::spawn(move || {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                            let Some((method, path, body)) = read_request(&mut stream) else {
                                return;
                            };
                            let (status, headers, body) = handler(&method, &path, &body);
                            write_response(&mut stream, status, &headers, &body);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Self {
            base,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<(String, String, String)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        let Some(header_end) = find(&buffer, b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
        let length = head
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                if !key.eq_ignore_ascii_case("content-length") {
                    return None;
                }
                value.trim().parse::<usize>().ok()
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        while buffer.len() < body_start + length {
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        let body_end = (body_start + length).min(buffer.len());
        let body = String::from_utf8_lossy(&buffer[body_start..body_end]).to_string();
        let request_line = head.lines().next()?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        return Some((method, path, body));
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn write_response(
    stream: &mut std::net::TcpStream,
    status: u16,
    headers: &[(String, String)],
    body: &str,
) {
    let reason = match status {
        200 => "OK",
        302 => "Found",
        _ => "Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.as_bytes().len()
    );
    for (key, value) in headers {
        response.push_str(&format!("{key}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(body);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn empty_jar() -> Arc<reqwest_cookie_store::CookieStoreMutex> {
    Arc::new(reqwest_cookie_store::CookieStoreMutex::new(
        cookie_store::CookieStore::default(),
    ))
}

const ACCOUNT_JSON: &str = r#"{"code":200,"message":"操作成功","payload":{"data":{
    "loginName":"2025150041","termId":47,"termName":"26-27学年第1学期",
    "roleModuleList":[{"name":"学生评价录入","url":"/studenteval/index"}]}}}"#;

const QUERY_JSON: &str = r#"{"code":200,"message":"操作成功","payload":{"data":{"total":2,
    "pageNum":1,"pageSize":100,"dataList":[
    {"id":"36229-24510-47-1187","targetName":"常规课学生过程性评价指标-3","termName":"26-27学年第1学期","noEvalCount":1,"hasEvalCount":0,"totalEvalCount":1},
    {"id":"36229-24508-47-1187","targetName":"常规课学生过程性评价指标-3","termName":"26-27学年第1学期","noEvalCount":0,"hasEvalCount":1,"totalEvalCount":1}
    ]}}}"#;

const DETAIL_JSON: &str = r#"{"code":200,"message":"操作成功","payload":{"data":[{
    "targetId":1187,"targetName":"常规课学生过程性评价指标-3","scoreType":4,
    "commentRequired":2,"staySeconds":0,
    "studentCourseData":[{
        "scId":2242443,"courseName":"离散数学","courseNo":"1500210001",
        "employeeName":"教师甲","classesName":"202620271150021000101",
        "currentTimes":0,"limitedTimes":1,"expiredDate":"2099-01-01","participate":true
    }],
    "subItemList":[
        {"id":1237,"subName":"教师是否教态大方得体","subitemType":"options","choices":[
            {"valueLow":"90","valueHigh":"100","label":"非常优秀","valueNormal":"92"},
            {"valueLow":"60","valueHigh":"90","label":"良好","valueNormal":"65"},
            {"valueLow":"30","valueHigh":"60","label":"一般","valueNormal":"30"},
            {"valueLow":"0","valueHigh":"30","label":"待改进","valueNormal":"10"}
        ]},
        {"id":1238,"subName":"是否清楚教学目标","subitemType":"options","choices":[
            {"valueLow":"90","valueHigh":"100","label":"非常清楚","valueNormal":"92"},
            {"valueLow":"60","valueHigh":"90","label":"比较清楚","valueNormal":"65"},
            {"valueLow":"30","valueHigh":"60","label":"一般","valueNormal":"30"},
            {"valueLow":"0","valueHigh":"30","label":"不清楚","valueNormal":"10"}
        ]}
    ]
}]}}"#;

fn jxpj_mock(
    auth_base: &str,
    requests: Arc<Mutex<Vec<(String, String, String)>>>,
) -> (Server, Arc<Mutex<Vec<(String, String, String)>>>) {
    let log = requests.clone();
    let auth_base = auth_base.to_string();
    let server = Server::start("127.0.0.1", move |jxpj_base| {
        let jxpj_base = jxpj_base.to_string();
        let log = log.clone();
        let auth_base = auth_base.clone();
        let ticket_seen = Arc::new(AtomicBool::new(false));
        move |method: &str, path: &str, body: &str| {
            log.lock()
                .expect("log")
                .push((method.to_string(), path.to_string(), body.to_string()));
            let json = |body: &str| {
                (
                    200,
                    vec![(
                        "Content-Type".to_string(),
                        "application/json;charset=UTF-8".to_string(),
                    )],
                    body.to_string(),
                )
            };
            match (method, path) {
                ("GET", path) if path.starts_with("/sso/szulogin?ticket=") => {
                    ticket_seen.store(true, Ordering::SeqCst);
                    (
                        302,
                        vec![
                            (
                                "Location".to_string(),
                                format!("{jxpj_base}/sso/szulogin"),
                            ),
                            (
                                "Set-Cookie".to_string(),
                                "JSESSIONID=test; Path=/education-jxcp-service-web; HttpOnly"
                                    .to_string(),
                            ),
                        ],
                        String::new(),
                    )
                }
                ("GET", "/sso/szulogin") if !ticket_seen.load(Ordering::SeqCst) => {
                    let login = format!(
                        "{auth_base}/authserver/login?service=http%3A%2F%2Fjxpj%2Fsso%2Fszulogin"
                    );
                    (
                        200,
                        vec![(
                            "Content-Type".to_string(),
                            "text/html;charset=UTF-8".to_string(),
                        )],
                        format!(
                            "<script languge='javascript'>window.location.href='{login}'</script>"
                        ),
                    )
                }
                ("GET", "/sso/szulogin") => (
                    302,
                    vec![("Location".to_string(), format!("{jxpj_base}/"))],
                    String::new(),
                ),
                ("GET", "/") => (
                    200,
                    vec![(
                        "Content-Type".to_string(),
                        "text/html;charset=UTF-8".to_string(),
                    )],
                    "<!DOCTYPE html><html></html>".to_string(),
                ),
                ("GET", "/sso/login") => json(ACCOUNT_JSON),
                ("POST", path) if path.starts_with("/studentEval/query?") => json(QUERY_JSON),
                ("GET", path) if path.starts_with("/studentEval/querySearchTargetById/") => {
                    json(DETAIL_JSON)
                }
                ("POST", path) if path.starts_with("/studentEval/saveScore/") => json(
                    r#"{"code":200,"message":"操作成功","payload":{"data":true}}"#,
                ),
                _ => (404, Vec::new(), "not found".to_string()),
            }
        }
    });
    (server, requests)
}

#[test]
fn end_to_end_login_overview_and_submit() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    // auth 服务需要 jxpj 的地址来生成回跳；jxpj 的 handler 也需要 auth 的地址，
    // 因此先起 auth、再起 jxpj，auth 的 handler 通过共享槽读取 jxpj base。
    let slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let auth_slot = slot.clone();
    let auth = Server::start("127.0.0.2", move |_auth_base| {
        let slot = auth_slot.clone();
        move |_method: &str, path: &str, _body: &str| {
            let jxpj_base = slot.lock().expect("slot").clone().unwrap_or_default();
            if path.starts_with("/authserver/login") {
                (
                    302,
                    vec![(
                        "Location".to_string(),
                        format!("{jxpj_base}/sso/szulogin?ticket=ST-1"),
                    )],
                    String::new(),
                )
            } else {
                (404, Vec::new(), String::new())
            }
        }
    });
    let (web, _) = jxpj_mock(&auth.base, requests.clone());
    *slot.lock().expect("slot") = Some(web.base.clone());

    let client = JxpjClient::new(&web.base, &auth.base, empty_jar()).expect("client");
    let overview = fetch_with(&client).expect("overview");
    assert_eq!(overview.pending_targets, 1);
    assert_eq!(overview.pending_records, 1);
    assert_eq!(overview.term.as_ref().map(|term| term.id), Some(47));
    let target = &overview.targets[0];
    assert_eq!(target.status, TargetStatus::Pending);
    assert_eq!(target.target_id, 1187);
    assert_eq!(target.questions.len(), 2);
    assert_eq!(target.questions[0].choices.len(), 4);

    let mut events = Vec::new();
    let outcome = run_with(&client, &overview, None, true, &mut |event| events.push(event))
        .expect("dry run");
    assert_eq!(outcome.summary.submitted_targets, 0);
    assert_eq!(outcome.plans.len(), 1);
    assert_eq!(outcome.plans[0].choices.len(), 2);
    assert_eq!(outcome.plans[0].choices[0].choice_label, "2242443_1237_良好_1");
    assert_eq!(outcome.plans[0].choices[0].score, Some(65.0));
    assert_eq!(outcome.plans[0].comments[0].comment, "无");

    let mut events = Vec::new();
    let outcome = run_with(&client, &overview, Some("课程很好"), false, &mut |event| {
        events.push(event)
    })
    .expect("submit");
    assert_eq!(outcome.summary.submitted_targets, 1);
    assert_eq!(outcome.summary.submitted_records, 1);
    assert!(outcome.summary.failures.is_empty());

    let log = requests.lock().expect("log");
    let submit = log
        .iter()
        .find(|(method, path, _)| method == "POST" && path == "/studentEval/saveScore/1187")
        .expect("submit request");
    let body: serde_json::Value = serde_json::from_str(&submit.2).expect("body json");
    assert_eq!(body["choiceList"][0]["choiceLabel"], "2242443_1237_良好_1");
    assert_eq!(body["choiceList"][0]["score"], 65.0);
    assert_eq!(body["commentList"][0]["comment"], "课程很好");
    // 同一次会话里不应再次登录（只 bootstrap 一次）。
    let logins = log
        .iter()
        .filter(|(method, path, _)| method == "GET" && path == "/sso/login")
        .count();
    assert_eq!(logins, 1);
}

