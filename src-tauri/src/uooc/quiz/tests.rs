use super::*;
use crate::uooc::{
    ai,
    model::*,
    tests::{course, ok, root, service_test, wait_finished, Server},
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
const HTML: &str = r#"<form method="post" action="/exam/commit"><input type="hidden" name="task_id" value="99"><input type="hidden" name="nonce" value="private-form-token"><div class="queContainer" id="q1"><div class="ti-q">2+2?</div><label class="ti-a"><input type="radio" name="answers[1]" value="a">4</label><label class="ti-a"><input type="radio" name="answers[1]" value="b">5</label></div></form>"#;
fn json_response(v: Value) -> (u16, Vec<u8>, String) {
    (
        200,
        serde_json::to_vec(&v).unwrap(),
        "Content-Type: application/json\r\n".into(),
    )
}
fn completion(answer: &str) -> Value {
    json!({"id":"test","object":"chat.completion","created":0,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":json!({"answers":[answer]}).to_string()},"finish_reason":"stop"}]})
}
fn ready(service: &Service) {
    service.change(|s| {
        s.session = SessionStatus::Ready;
        s.user = Some(User {
            id: "7".into(),
            name: "test".into(),
        });
        s.courses = vec![course("42")];
        // 单测不得访问真实题库服务：指向必然拒绝连接的本地端口，保持流程走本地题库与 AI。
        s.settings.qa_server_url = "http://127.0.0.1:1".into();
    });
}
#[test]
fn parses_verified_form_without_exposing_hidden_fields_or_executing_script() {
    let url = "https://www.uooc.net.cn/exam/99".parse().unwrap();
    let p = protocol::parse(HTML, &url, "99").unwrap();
    assert!(p.action.is_some());
    let public = serde_json::to_string(&p.questions).unwrap();
    assert!(!public.contains("private-form-token"));
    assert!(!public.contains("nonce"));
    let values = Answers::from([("q1".into(), vec!["a".into()])]);
    let fields = p.answer_fields(&values).unwrap();
    assert!(fields.contains(&("answers[1]".into(), "a".into())));
    assert!(fields.contains(&("nonce".into(), "private-form-token".into())));
    for altered in [
        HTML.replace("/exam/commit", "https://evil.test/commit"),
        HTML.replace("/exam/commit", "/exam/delete"),
        HTML.replace("method=\"post\"", "method=\"get\""),
        HTML.replace("<form ", "<form onsubmit=\"ajax()\" "),
        HTML.replace("value=\"99\"", "value=\"98\""),
    ] {
        assert!(protocol::parse(&altered, &url, "99")
            .unwrap()
            .action
            .is_none());
    }
    assert!(validate_answers(
        &p.questions,
        &Answers::from([("q1".into(), vec!["c".into()])]),
        true
    )
    .is_err());
    assert!(validate_answers(
        &p.questions,
        &Answers::from([("q1".into(), vec!["a".into(), "b".into()])]),
        true
    )
    .is_err());
}
#[tokio::test]
async fn approval_flow_is_atomic_and_never_leaks_credentials_to_ai() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let passed = Arc::new(AtomicBool::new(false));
    let marked = passed.clone();
    let submits = Arc::new(AtomicUsize::new(0));
    let posted = submits.clone();
    let server = Server::bytes(move |request| {
        if request.path == "/chat/completions" {
            assert!(!request.headers.contains_key("cookie"));
            assert_eq!(request.headers["authorization"], "Bearer test-ai-key");
            assert!(!request.body.contains("private-form-token"));
            return json_response(completion("a"));
        }
        if request.path.starts_with("/home/course/progress?") {
            return json_response(ok(
                json!({"video_progress_value":100,"test":20,"test_progress_value":if marked.load(Ordering::SeqCst){100}else{0}}),
            ));
        }
        if request.path.starts_with("/home/task/testList?") {
            return json_response(ok(
                json!({"data":[{"res_id":21,"chapter_id":10,"section_id":11}],"page":{"pages":1}}),
            ));
        }
        if request.path.starts_with("/home/learn/getUnitLearn?") {
            return json_response(ok(
                json!([{"id":21,"type":80,"task_id":99,"title":"测验","finished":if marked.load(Ordering::SeqCst){1}else{0}}]),
            ));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            return (404, b"".to_vec(), String::new());
        }
        if request.path == "/exam/99" {
            return (
                200,
                HTML.as_bytes().to_vec(),
                "Content-Type: text/html\r\n".into(),
            );
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return json_response(ok(json!([])));
        }
        if request.path.starts_with("/exam/view") {
            return json_response(ok(json!({})));
        }
        if request.path == "/exam/commit" {
            assert_eq!(request.method, "POST");
            assert_eq!(request.fields()["nonce"], "private-form-token");
            assert_eq!(request.fields()["answers[1]"], "a");
            posted.fetch_add(1, Ordering::SeqCst);
            marked.store(true, Ordering::SeqCst);
            return json_response(json!({"code":1,"msg":"测验通过"}));
        }
        panic!("unexpected route {}", request.path)
    });
    *service.client.write().unwrap() = Arc::new(
        Client::with_base(crate::uooc::session::load(None).unwrap(), &server.base).unwrap(),
    );
    service.ai.lock().unwrap().main = ai::Provider {
        base_url: server.base.clone(),
        model: "test".into(),
        key: "test-ai-key".into(),
    };
    service.ai.lock().unwrap().default_mode = Mode::Approval;
    service.start(vec!["42".into()]).unwrap();
    wait_finished(&service).await;
    let q = service.snapshot().quizzes["7/42"].clone();
    assert_eq!(q.stage, Stage::AwaitingApproval);
    assert_eq!(q.answers["q1"], ["a"]);
    assert_eq!(submits.load(Ordering::SeqCst), 0);
    assert_eq!(
        service.snapshot().tasks[0].status,
        TaskStatus::AwaitingDecision
    );
    assert!(service
        .quiz_edit("42".into(), q.revision - 1, q.answers.clone())
        .is_err());
    service
        .quiz_edit("42".into(), q.revision, q.answers)
        .unwrap();
    let q = service.snapshot().quizzes["7/42"].clone();
    service
        .quiz_action("42".into(), q.revision, Action::Submit)
        .await
        .unwrap();
    assert!(service
        .quiz_action("42".into(), q.revision, Action::Submit)
        .await
        .is_err());
    wait_finished(&service).await;
    assert!(passed.load(Ordering::SeqCst));
    assert_eq!(submits.load(Ordering::SeqCst), 1);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::Completed);
    let saved = serde_json::to_string(&service.state_db.load().unwrap().unwrap()).unwrap();
    assert!(!saved.contains("test-ai-key"));
    assert!(!saved.contains("private-form-token"));
    service.change(|s| s.user.as_mut().unwrap().id = "8".into());
    assert!(service.snapshot().quizzes.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn supervised_dry_run_never_calls_ai_or_submits_without_user_choice() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let calls = Arc::new(Mutex::new(vec![]));
    let observed = calls.clone();
    let server = Server::bytes(move |request| {
        assert_eq!(request.method, "GET");
        observed.lock().unwrap().push(request.path.clone());
        if request.path.starts_with("/home/course/progress?") {
            return json_response(ok(json!({"video_progress_value":100,"test":20})));
        }
        if request.path.starts_with("/home/task/testList?") {
            return json_response(ok(
                json!({"data":[{"res_id":21,"chapter_id":10,"section_id":11}],"page":{"pages":1}}),
            ));
        }
        if request.path.starts_with("/home/learn/getUnitLearn?") {
            return json_response(ok(
                json!([{"id":21,"type":80,"task_id":99,"title":"测验","finished":0}]),
            ));
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return (404, b"".to_vec(), String::new());
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            return (404, b"".to_vec(), String::new());
        }
        assert_eq!(request.path, "/exam/99");
        (200, HTML.as_bytes().to_vec(), String::new())
    });
    *service.client.write().unwrap() = Arc::new(
        Client::with_base(crate::uooc::session::load(None).unwrap(), &server.base).unwrap(),
    );
    service.quiz_policy("42".into(), Mode::Supervised).unwrap();
    service.change(|s| s.settings.dry_run = true);
    service.start(vec!["42".into()]).unwrap();
    wait_finished(&service).await;
    let q = service.snapshot().quizzes["7/42"].clone();
    assert_eq!(q.stage, Stage::AwaitingChoice);
    assert!(q.answers.is_empty());
    service
        .quiz_edit(
            "42".into(),
            q.revision,
            Answers::from([("q1".into(), vec!["a".into()])]),
        )
        .unwrap();
    let q = service.snapshot().quizzes["7/42"].clone();
    assert!(service
        .quiz_action("42".into(), q.revision, Action::Submit)
        .await
        .is_err());
    assert!(calls.lock().unwrap().iter().all(|p| !p.contains("commit")));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn jev_choice_and_multiple_validation_keeps_uncertainty_visible() {
    let mut q = protocol::parse(
        HTML,
        &"https://www.uooc.net.cn/exam/99".parse().unwrap(),
        "99",
    )
    .unwrap()
    .questions
    .remove(0);
    let result = json!({"answers":{"choice":{"choice":"a","confidence":0.95,"probabilities":{"a":0.99,"b":0.01}}}});
    assert_eq!(ai::interpret_jev(&q, &result, 0.8).unwrap().answers, ["a"]);
    assert!(ai::interpret_jev(&q, &result, 0.99).is_err());
    q.kind = Kind::Multiple;
    let result = json!({"answers":{"option_0":{"noul":0.95},"option_1":{"noul":0.05}}});
    assert_eq!(ai::interpret_jev(&q, &result, 0.8).unwrap().answers, ["a"]);
    assert!(ai::interpret_jev(
        &q,
        &json!({"answers":{"option_0":{"noul":0.5},"option_1":{"noul":0.1}}}),
        0.8
    )
    .is_err());
}
#[test]
fn provider_normalization_and_secret_view_are_safe() {
    assert_eq!(
        ai::normalize_url("https://example.org/api/v3/chat/completions/").unwrap(),
        "https://example.org/api/v3"
    );
    assert_eq!(
        ai::normalize_url("http://192.168.1.20:8000/v1/").unwrap(),
        "http://192.168.1.20:8000/v1"
    );
    for url in [
        "https://user:key@example.org/v1",
        "https://example.org/v1?key=secret",
        "file:///tmp/key",
    ] {
        assert!(ai::normalize_url(url).is_err());
    }
    let mut config = ai::Config::default();
    config.main.key = "private-provider-key".into();
    let view = serde_json::to_string(&config.view()).unwrap();
    assert!(!view.contains("private-provider-key"));
    assert!(view.contains("hasKey"));
}

#[test]
fn cached_question_images_restore_and_corruption_invalidates_approval() {
    let root = root();
    let image = "data:image/png;base64,aGVsbG8=".to_owned();
    let mut q = protocol::parse(
        HTML,
        &"https://www.uooc.net.cn/exam/99".parse().unwrap(),
        "99",
    )
    .unwrap()
    .questions
    .remove(0);
    q.images = vec![image.clone()];
    let states = BTreeMap::from([(
        "7/42".into(),
        QuizState {
            account_id: "7".into(),
            cid: "42".into(),
            signature: "approved".into(),
            stage: Stage::AwaitingApproval,
            questions: vec![q],
            ..Default::default()
        },
    )]);
    let saved = super::saved(&states, &root).unwrap();
    let reference = saved["7/42"].questions[0].images[0].clone();
    assert!(reference.starts_with("cached:"));
    assert!(!serde_json::to_string(&saved).unwrap().contains("base64"));
    let mut restored = saved.clone();
    restore_media(&mut restored, &root);
    assert_eq!(restored["7/42"].questions[0].images, [image]);
    let file = root.join("quiz-media").join(format!(
        "{}.json",
        reference.strip_prefix("cached:").unwrap()
    ));
    std::fs::write(&file, "\"data:image/png;base64,bad\"").unwrap();
    let mut broken = saved;
    restore_media(&mut broken, &root);
    assert_eq!(broken["7/42"].stage, Stage::AwaitingChoice);
    assert!(broken["7/42"].signature.is_empty());
    assert!(!file.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn submission_acknowledgement_is_not_a_pass_result() {
    assert_eq!(
        protocol::verdict(br#"{"code":1,"data":{"finished":1}}"#),
        None
    );
    assert_eq!(
        protocol::verdict(
            "<div class='queContainer'><div class='layui-layer-content'>测验通过</div></div>"
                .as_bytes()
        ),
        None
    );
    assert_eq!(
        protocol::verdict("<div class='layui-layer-content'>请重新提交测验</div>".as_bytes()),
        Some(false)
    );
    assert_eq!(
        protocol::verdict("{\"code\":1,\"msg\":\"测验通过\"}".as_bytes()),
        Some(true)
    );
}

#[tokio::test]
async fn yolo_gate_resumes_video_only_after_confirmed_quiz_completion() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorded = events.clone();
    let video_one = Arc::new(AtomicBool::new(false));
    let first = video_one.clone();
    let quiz_pass = Arc::new(AtomicBool::new(false));
    let passed = quiz_pass.clone();
    let video_two = Arc::new(AtomicBool::new(false));
    let second = video_two.clone();
    let server = Server::bytes(move |request| {
        if request.path == "/chat/completions" {
            return json_response(completion("a"));
        }
        if request.path.starts_with("/home/course/progress?") {
            return json_response(ok(
                json!({"video_progress_value":if second.load(Ordering::SeqCst){100}else{0},"test":20}),
            ));
        }
        if request.path.starts_with("/home/learn/getCatalogList?") {
            return json_response(ok(json!([{"id":10,"children":[{"id":11},{"id":12}]}])));
        }
        if request.path.starts_with("/home/task/testList?") {
            return json_response(ok(
                json!({"data":[{"res_id":21,"chapter_id":10,"section_id":11}],"page":{"pages":1}}),
            ));
        }
        if request.path.starts_with("/home/learn/getUnitLearn?") {
            if request.fields()["section_id"] == "12" {
                if !passed.load(Ordering::SeqCst) {
                    return json_response(json!({"code":600}));
                }
                return json_response(ok(
                    json!([{"id":22,"type":10,"title":"视频二","finished":i32::from(second.load(Ordering::SeqCst)),"video_url":{"main":{"source":"https://example.org/v2.mp4"}}}]),
                ));
            }
            return json_response(ok(json!([
                {"id":20,"type":10,"title":"视频一","finished":i32::from(first.load(Ordering::SeqCst)),"video_url":{"main":{"source":"https://example.org/v1.mp4"}}},
                {"id":21,"type":80,"task_id":99,"title":"测验","finished":i32::from(passed.load(Ordering::SeqCst))}
            ])));
        }
        if request.path == "/home/learn/markVideoLearn" {
            let rid = request.fields()["resource_id"].clone();
            if rid == "20" {
                first.store(true, Ordering::SeqCst);
            } else {
                assert!(passed.load(Ordering::SeqCst));
                second.store(true, Ordering::SeqCst);
            }
            recorded.lock().unwrap().push(format!("video-{rid}"));
            return json_response(ok(json!({"finished":1})));
        }
        if request.path.starts_with("/exam/getTaskPaper") {
            return (404, b"".to_vec(), String::new());
        }
        if request.path == "/exam/99" {
            return (200, HTML.as_bytes().to_vec(), String::new());
        }
        if request.path.starts_with("/home/learn/getCatalogList") {
            return json_response(ok(json!([])));
        }
        if request.path.starts_with("/exam/view") {
            return json_response(ok(json!({})));
        }
        if request.path == "/exam/commit" {
            assert!(first.load(Ordering::SeqCst));
            recorded.lock().unwrap().push("quiz".into());
            passed.store(true, Ordering::SeqCst);
            return json_response(json!({"code":1,"msg":"测验通过"}));
        }
        panic!("unexpected route {}", request.path)
    });
    *service.client.write().unwrap() = Arc::new(
        Client::with_base(crate::uooc::session::load(None).unwrap(), &server.base).unwrap(),
    );
    {
        let mut ai = service.ai.lock().unwrap();
        ai.default_mode = Mode::Yolo;
        ai.main = ai::Provider {
            base_url: server.base.clone(),
            model: "test".into(),
            key: "test-key".into(),
        };
    }
    for rid in ["20", "22"] {
        service
            .durations
            .get_or_probe("42", rid, &CancellationToken::new(), || async { Ok(5.0) })
            .await
            .unwrap();
    }
    service.start(vec!["42".into()]).unwrap();
    wait_finished(&service).await;
    assert_eq!(*events.lock().unwrap(), ["video-20", "quiz", "video-22"]);
    assert_eq!(service.snapshot().tasks[0].status, TaskStatus::Completed);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn jev_uses_systemone_and_requests_only_one_main_model_recheck() {
    let root = root();
    let service = service_test(root.clone());
    ready(&service);
    let calls = Arc::new(AtomicUsize::new(0));
    let n = calls.clone();
    let server = Server::new(move |request| {
        assert!(!request.headers.contains_key("cookie"));
        let body: Value = serde_json::from_str(&request.body).unwrap();
        if request.path == "/chat/completions" {
            let count = n.fetch_add(1, Ordering::SeqCst);
            assert!(count < 2);
            return completion(if count == 0 { "a" } else { "b" });
        }
        assert_eq!(request.path, "/systemone");
        assert_eq!(body["questions"]["choice"]["type"], "choice");
        assert_eq!(body["model"], "jev-latest");
        assert!(body.get("messages").is_none());
        assert!(body["state"].get("review").is_none());
        json!({"answers":{"choice":{"type":"choice","choice":"b","confidence":0.98,"probabilities":{"a":0.01,"b":0.99}}}})
    });
    {
        let mut c = service.ai.lock().unwrap();
        c.main = ai::Provider {
            base_url: server.base.clone(),
            model: "test".into(),
            key: "main-key".into(),
        };
        c.jev = ai::Provider {
            base_url: server.base.clone(),
            model: "jev-latest".into(),
            key: "jev-key".into(),
        };
        c.jev_enabled = true;
    }
    let questions = protocol::parse(
        HTML,
        &"https://www.uooc.net.cn/exam/99".parse().unwrap(),
        "99",
    )
    .unwrap()
    .questions;
    let (answers, reviews, _sessions, error) =
        ai::generate(&service, &questions, &std::collections::BTreeMap::new(), &CancellationToken::new(), None).await;
    assert!(error.is_none());
    assert_eq!(answers["q1"], ["b"]);
    assert_eq!(reviews.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    std::fs::remove_dir_all(root).unwrap();
}
