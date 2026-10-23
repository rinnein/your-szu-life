pub(crate) mod db;
pub(crate) mod model;
pub(crate) mod qa;
pub(crate) mod protocol;
#[cfg(test)]
mod tests;
use self::model::*;
use super::{
    ai,
    client::{Client, Error},
    model::{id, number, text, SessionStatus, TaskStatus, Video},
    Service,
};
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use tauri::Manager;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Context {
    cid: String,
    account: String,
    generation: u64,
}
fn context(service: &Service, cid: &str, generation: u64) -> Result<Context, Error> {
    let r = service.runtime.lock().unwrap();
    let user = r.snapshot.user.as_ref().ok_or(Error::Unauthorized)?;
    if r.workers.get(cid).map(|(g, _)| *g) != Some(generation) {
        return Err(Error::Cancelled);
    }
    Ok(Context {
        cid: cid.into(),
        account: user.id.clone(),
        generation,
    })
}
fn key(account: &str, cid: &str) -> String {
    format!("{account}/{cid}")
}
fn current(service: &Service, ctx: &Context) -> bool {
    let r = service.runtime.lock().unwrap();
    r.snapshot.session == SessionStatus::Ready
        && r.snapshot
            .user
            .as_ref()
            .is_some_and(|u| u.id == ctx.account)
        && r.workers
            .get(&ctx.cid)
            .is_some_and(|(g, c)| *g == ctx.generation && !c.is_cancelled())
}
fn state(service: &Service, ctx: &Context) -> QuizState {
    service
        .runtime
        .lock()
        .unwrap()
        .snapshot
        .quizzes
        .get(&key(&ctx.account, &ctx.cid))
        .cloned()
        .unwrap_or_default()
}
fn update(service: &Service, ctx: &Context, edit: impl FnOnce(&mut QuizState)) {
    let notify;
    {
        let mut r = service.runtime.lock().unwrap();
        if r.snapshot.user.as_ref().is_none_or(|u| u.id != ctx.account)
            || r.workers
                .get(&ctx.cid)
                .is_none_or(|(g, c)| *g != ctx.generation || c.is_cancelled())
        {
            return;
        }
        let q = r
            .snapshot
            .quizzes
            .entry(key(&ctx.account, &ctx.cid))
            .or_default();
        let revision = q.revision;
        edit(q);
        q.account_id = ctx.account.clone();
        q.cid = ctx.cid.clone();
        q.revision = revision.saturating_add(1);
        let tag = format!("{}:{:?}:{}", q.task_id, q.stage, q.message);
        notify = if matches!(
            q.stage,
            Stage::AwaitingApproval | Stage::AwaitingChoice | Stage::Manual | Stage::Failed
        ) && q.notified != tag
        {
            q.notified = tag;
            Some(format!("{}：{}", q.title, q.message))
        } else {
            None
        };
        service.publish(&mut r);
    }
    if let (Some(app), Some(body)) = (&service.app, notify) {
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title("UOOC 测验需要处理")
            .body(body)
            .extra("uoocCourse", &ctx.cid)
            .show();
    }
}
pub(super) fn saved(
    states: &BTreeMap<String, QuizState>,
    root: &std::path::Path,
) -> Result<BTreeMap<String, QuizState>, String> {
    let mut copy = states.clone();
    for q in copy.values_mut() {
        for question in &mut q.questions {
            for image in &mut question.images {
                if !image.starts_with("data:image/") {
                    return Err("试题图片缓存格式无效".into());
                }
                let hash = format!("{:x}", Sha1::digest(image.as_bytes()));
                let path = root.join("quiz-media").join(format!("{hash}.json"));
                if !path.exists() {
                    super::persistence::write(&path, image)?;
                }
                *image = format!("cached:{hash}");
            }
        }
    }
    Ok(copy)
}
pub(super) fn restore_media(states: &mut BTreeMap<String, QuizState>, root: &std::path::Path) {
    for q in states.values_mut() {
        for question in &mut q.questions {
            let mut missing = false;
            question.images.retain_mut(|image| {
                let restored = image
                    .strip_prefix("cached:")
                    .filter(|hash| hash.len() == 40 && hash.bytes().all(|c| c.is_ascii_hexdigit()))
                    .and_then(|hash| {
                        let path = root.join("quiz-media").join(format!("{hash}.json"));
                        let value = super::persistence::read::<String>(&path).ok().flatten();
                        if value
                            .as_ref()
                            .is_some_and(|v| format!("{:x}", Sha1::digest(v.as_bytes())) == hash)
                        {
                            value
                        } else {
                            let _ = std::fs::remove_file(path);
                            None
                        }
                    });
                if let Some(value) =
                    restored.filter(|v| v.starts_with("data:image/") && v.len() <= 2 * 1024 * 1024)
                {
                    *image = value;
                    true
                } else {
                    missing = true;
                    false
                }
            });
            if missing {
                q.signature.clear();
                q.stage = Stage::AwaitingChoice;
                q.message = "试题图片缓存不可用，请刷新试卷后重新确认答案".into();
            }
        }
    }
}
async fn paper(
    client: &Client,
    task: &str,
    cancel: &CancellationToken,
) -> Result<protocol::Paper, Error> {
    let html = client.quiz_document(task, cancel).await?;
    let url = format!("{}/exam/{task}", client.origin())
        .parse()
        .map_err(|_| Error::Invalid)?;
    let mut paper = protocol::parse(&html, &url, task).map_err(Error::Quiz)?;

    // 防混淆策略：请求官方 /exam/getTaskPaper 获取无混淆标准题单，
    // 将 DOM 题目规范化对齐为服务端真实全局单题 ID，并纯净化题干与选项文本
    let tp_url = format!("{}/exam/getTaskPaper?tid={task}", client.origin());
    if let Ok(tp_bytes) = client.quiz_http(&tp_url, None, cancel).await {
        if let Some((tp_questions, _)) = qa::parse_task_paper(&tp_bytes) {
            qa::align_paper_with_task_paper(&mut paper.questions, &tp_questions);
        }
    }

    let mut size = 0;
    for (q, urls) in paper.questions.iter_mut().zip(&paper.image_urls) {
        for url in urls {
            let image = client.quiz_image(url, cancel).await?;
            size += image.len();
            if size > 6 * 1024 * 1024 {
                return Err(Error::Quiz("试卷图片过多，请使用官网处理".into()));
            }
            q.images.push(image);
        }
    }
    paper.signature = format!(
        "{:x}",
        Sha1::digest(
            serde_json::to_vec(&(&paper.signature, &paper.questions))
                .map_err(|_| Error::Invalid)?
        )
    );
    Ok(paper)
}
async fn locate(
    client: &Client,
    cid: &str,
    rid: &str,
    cancel: &CancellationToken,
) -> Result<Option<(Video, serde_json::Value)>, Error> {
    for row in client.quiz_list(cid, cancel).await? {
        if protocol::resource_id(&row).as_deref() != Some(rid) {
            continue;
        }
        return locate_row(client, cid, &row, cancel).await;
    }
    Ok(None)
}
async fn locate_row(
    client: &Client,
    cid: &str,
    row: &serde_json::Value,
    cancel: &CancellationToken,
) -> Result<Option<(Video, serde_json::Value)>, Error> {
    let rid = protocol::resource_id(row).ok_or(Error::Invalid)?;
    let chapter = id(&row["chapter_id"]).ok_or(Error::Invalid)?;
    let section = id(&row["section_id"]).unwrap_or_else(|| chapter.clone());
    let context = Video {
        cid: cid.into(),
        chapter,
        section,
        subsection: id(&row["point_id"]),
        resource: rid.clone(),
        title: text(&row["title"]),
        source: String::new(),
        position: 0.0,
        task_type: "quiz".into(),
        task_id: id(&row["task_id"]),
    };
    let rows = match client.unit(&context, cancel).await {
        Ok(r) => r,
        Err(Error::Gate) => return Err(Error::Gate),
        Err(_) => vec![],
    };
    if let Some(item) = rows
        .into_iter()
        .find(|v| id(&v["id"]).as_deref() == Some(rid.as_str()) && number(&v["type"]) == 80.0)
    {
        return Ok(Some((context, item)));
    }
    if id(&row["task_id"]).is_some_and(|tid| tid != "0") {
        return Ok(Some((context, row.clone())));
    }
    Ok(None)
}
async fn allowed(
    service: &Arc<Service>,
    cid: &str,
    cancel: &CancellationToken,
) -> Result<bool, Error> {
    Ok(service
        .fetch_progress(cid, cancel)
        .await?
        .test
        .weight
        .is_some_and(|w| w > 0.0))
}
pub(super) async fn run(
    service: Arc<Service>,
    client: Arc<Client>,
    cid: String,
    generation: u64,
    gated: bool,
    cancel: CancellationToken,
) -> Result<TaskStatus, Error> {
    let ctx = context(&service, &cid, generation)?;
    let default_mode = service.ai.lock().unwrap().default_mode;
    if !allowed(&service, &cid, &cancel).await? {
        update(&service, &ctx, |q| {
            q.stage = Stage::Disabled;
            q.gated = gated;
            q.message = "测验权重为零或尚未确认，答题已禁用".into();
        });
        return Ok(if gated {
            TaskStatus::Blocked
        } else {
            TaskStatus::Completed
        });
    }
    let rows = client.quiz_list(&cid, &cancel).await?;
    for row in rows {
        let Some(rid) = protocol::resource_id(&row) else {
            continue;
        };
        let item = match locate_row(&client, &cid, &row, &cancel).await {
            Ok(Some(item)) => item,
            Err(Error::Gate) => continue,
            Ok(None) => continue,
            Err(e) => return Err(e),
        };
        if number(&item.1["finished"]) != 0.0 {
            continue;
        }
        let task = id(&item.1["task_id"])
            .ok_or_else(|| Error::Quiz("测验缺少任务编号，请使用官网".into()))?;
        let old = state(&service, &ctx);
        let same = old.task_id == task;
        update(&service, &ctx, |q| {
            if !same {
                let mode = if old.cid.is_empty() {
                    default_mode
                } else {
                    old.mode
                };
                *q = QuizState {
                    mode,
                    ..Default::default()
                };
            }
            q.task_id = task.clone();
            q.resource_id = rid.clone();
            q.title = text(&item.1["title"]);
            q.gated = gated;
            q.video_complete = !gated;
        });
        if same && matches!(old.stage, Stage::Submitting | Stage::Verifying) {
            update(&service, &ctx, |q| {
                q.stage = Stage::Manual;
                q.message = "上次提交结果尚未确认，请查看官网后刷新".into();
            });
            return Ok(TaskStatus::AwaitingDecision);
        }
        let fresh = match paper(&client, &task, &cancel).await {
            Ok(p) => p,
            Err(Error::Unauthorized) => return Err(Error::Unauthorized),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(_) => {
                update(&service, &ctx, |q| {
                    q.stage = Stage::Manual;
                    q.message = "无法读取完整试卷，请在官方窗口完成验证或答题".into();
                });
                return Ok(TaskStatus::AwaitingDecision);
            }
        };
        let unchanged = same && old.signature == fresh.signature;
        update(&service, &ctx, |q| {
            q.signature = fresh.signature.clone();
            q.questions = fresh.questions.clone();
            if !unchanged {
                q.answers.clear();
                q.reviews.clear();
                q.stage = Stage::Preparing;
            }
        });
        if !unchanged {
            let qa_url = service.snapshot().settings.qa_server_url.clone();
            let qa_key = service.qa_api_key().await;
            if let Ok(qa_qs) =
                qa::query_task_answers(&qa_url, qa_key.as_deref(), &task, Some(&cid)).await
            {
                let match_res = qa::match_answers(&qa_qs, &fresh.questions);
                if !match_res.matched_answers.is_empty() || !match_res.reviews.is_empty() {
                    let all_matched = match_res.matched_answers.len() == fresh.questions.len()
                        && !fresh.questions.is_empty();
                    update(&service, &ctx, |q| {
                        q.answers.extend(match_res.matched_answers);
                        q.reviews = match_res.reviews;
                        if all_matched {
                            q.stage =
                                if q.mode == Mode::Yolo && !service.snapshot().settings.dry_run {
                                    Stage::Preparing
                                } else {
                                    Stage::AwaitingApproval
                                };
                            q.message = "已从 UOOC 题库匹配全部标准答案".into();
                        }
                    });
                }
            }
        }
        if unchanged
            && matches!(
                old.stage,
                Stage::AwaitingApproval | Stage::AwaitingChoice | Stage::Manual | Stage::Failed
            )
        {
            return Ok(TaskStatus::AwaitingDecision);
        }
        let current_state = state(&service, &ctx);
        if current_state.answers.len() == fresh.questions.len()
            && !fresh.questions.is_empty()
            && current_state.mode == Mode::Yolo
            && !service.snapshot().settings.dry_run
        {
            return submit(&service, &client, &ctx, &cancel).await;
        }
        let mode = current_state.mode;
        if mode == Mode::Supervised {
            update(&service, &ctx, |q| {
                q.stage = Stage::AwaitingChoice;
                q.message = "请选择自主答题或 AI 答题".into();
            });
            return Ok(TaskStatus::AwaitingDecision);
        }
        if !generate(&service, &ctx, &cancel, false).await {
            return Ok(TaskStatus::AwaitingDecision);
        }
        if mode != Mode::Yolo || service.snapshot().settings.dry_run {
            return Ok(TaskStatus::AwaitingDecision);
        }
        return submit(&service, &client, &ctx, &cancel).await;
    }
    update(&service, &ctx, |q| {
        q.stage = Stage::Passed;
        q.message = if gated {
            "没有可自动处理的前置测验，请检查其他前置任务"
        } else {
            "当前测验已由服务端确认完成"
        }
        .into();
        q.video_complete = !gated;
    });
    Ok(if gated {
        TaskStatus::Blocked
    } else {
        TaskStatus::Completed
    })
}
async fn generate(
    service: &Arc<Service>,
    ctx: &Context,
    cancel: &CancellationToken,
    _force_all: bool,
) -> bool {
    update(service, ctx, |q| {
        q.stage = Stage::Generating;
        q.message = "正在匹配题库与调用 AI 模型生成题目答案…".into();
    });
    let mut q = state(service, ctx);

    // 1. AI 判题前，先自动尝试匹配本地题库与远程题库！
    let qa_url = service.snapshot().settings.qa_server_url.clone();
    let qa_key = service.qa_api_key().await;

    let mut confirmed_correct_ids = HashSet::new();
    let mut wrong_options_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // 1. 先从已有评测/作答记录搜集已排除的错误选项
    for rev in &q.reviews {
        if rev.message.contains("排除") {
            let entry = wrong_options_map.entry(rev.question_id.clone()).or_default();
            for ans in &rev.answers {
                if !entry.contains(ans) {
                    entry.push(ans.clone());
                }
            }
        }
    }

    // 2. 先检查本地题库确认答案与排除选项
    let local_bank = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.load_all_bank_questions().await.unwrap_or_default()
    } else {
        Default::default()
    };
    for question in &q.questions {
        let key = qa::bank_key(&q.task_id, &question.id);
        if let Some(entry) = local_bank.get(&key) {
            // 搜集本地题库中确认错误的选项
            for a in &entry.answers {
                if !a.is_correct {
                    for v in &a.values {
                        let w_str = match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Bool(b) => if *b { "A".into() } else { "B".into() },
                            serde_json::Value::Number(n) => n.to_string(),
                            other => other.to_string(),
                        };
                        let w_entry = wrong_options_map.entry(question.id.clone()).or_default();
                        if !w_entry.contains(&w_str) {
                            w_entry.push(w_str);
                        }
                    }
                }
            }
            if !entry.correct_answers.is_empty() {
                let mapped = qa::map_answers_to_option_ids(&entry.correct_answers, &question.options);
                let final_ans = if !mapped.is_empty() { mapped } else { entry.correct_answers.clone() };
                confirmed_correct_ids.insert(question.id.clone());
                update(service, ctx, |state| {
                    state.answers.insert(question.id.clone(), final_ans.clone());
                    state.reviews.push(Review {
                        question_id: question.id.clone(),
                        answers: final_ans,
                        probabilities: Default::default(),
                        message: "本地题库命中已确认答案".into(),
                    });
                });
            }
        }
    }

    // 3. 再比对远程题库返回的结果
    if let Ok(qa_qs) = qa::query_task_answers(&qa_url, qa_key.as_deref(), &q.task_id, Some(&ctx.cid)).await {
        let matched = qa::match_answers(&qa_qs, &q.questions);
        for (qid, wrongs) in matched.wrong_options {
            let entry = wrong_options_map.entry(qid).or_default();
            for w in wrongs {
                if !entry.contains(&w) {
                    entry.push(w);
                }
            }
        }
        for (qid, ans) in matched.matched_answers {
            confirmed_correct_ids.insert(qid.clone());
            update(service, ctx, |state| {
                state.answers.insert(qid.clone(), ans.clone());
            });
        }
        update(service, ctx, |state| {
            for r in matched.reviews {
                if let Some(existing) = state.reviews.iter_mut().find(|ex| ex.question_id == r.question_id) {
                    *existing = r;
                } else {
                    state.reviews.push(r);
                }
            }
        });
    }

    q = state(service, ctx);

    // 2. 过滤：如果有确定了正确答案的，直接标定那些题的答案并过滤，交给 AI 判断的只有没有确定正确答案的！
    let needed: Vec<_> = q
        .questions
        .iter()
        .filter(|item| !confirmed_correct_ids.contains(&item.id))
        .cloned()
        .collect();

    // 3. 流式推送进度回调
    let service_clone = service.clone();
    let ctx_clone = ctx.clone();
    let callback = move |entry: &AiSessionEntry| {
        service_clone.emit_ai_session(entry);
        update(&service_clone, &ctx_clone, |state| {
            if let Some(existing) = state.ai_sessions.iter_mut().find(|s| s.batch_index == entry.batch_index) {
                *existing = entry.clone();
            } else {
                state.ai_sessions.push(entry.clone());
            }
        });
    };

    let (ai_answers, ai_reviews, ai_sessions, err) = if needed.is_empty() {
        (Answers::new(), vec![], vec![], None)
    } else {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return false,
            r = ai::generate(service, &needed, &wrong_options_map, cancel, Some(&callback)) => r,
        }
    };

    if !current(service, ctx) {
        return false;
    }

    let ok = err.is_none();
    let automatic = q.mode == Mode::Yolo && !service.snapshot().settings.dry_run && ok;

    update(service, ctx, |q| {
        for (k, v) in ai_answers {
            q.answers.insert(k, v);
        }
        for r in ai_reviews {
            if let Some(existing) = q.reviews.iter_mut().find(|ex| ex.question_id == r.question_id)
            {
                *existing = r;
            } else {
                q.reviews.push(r);
            }
        }
        if !ai_sessions.is_empty() {
            q.ai_sessions = ai_sessions;
        }
        q.stage = if automatic {
            Stage::Preparing
        } else {
            Stage::AwaitingApproval
        };
        q.message = err.unwrap_or_else(|| {
            if automatic {
                "答案已准备完毕，准备核对并提交".into()
            } else {
                format!(
                    "已完成题库匹配（命中 {} 题）与 AI 答题（生成 {} 题）",
                    confirmed_correct_ids.len(),
                    needed.len()
                )
            }
        });
    });
    ok
}
async fn submit(
    service: &Arc<Service>,
    client: &Client,
    ctx: &Context,
    cancel: &CancellationToken,
) -> Result<TaskStatus, Error> {
    if service.snapshot().settings.dry_run {
        return Err(Error::Quiz("预览模式不允许提交测验".into()));
    }
    if !allowed(service, &ctx.cid, cancel).await? {
        return Err(Error::Quiz("测验权重为零或尚未确认，禁止提交".into()));
    }
    let q = state(service, ctx);
    if q.submission_uncertain {
        return Err(Error::Quiz(
            "上次提交结果尚未确认，请先在官网检查；不会重复提交".into(),
        ));
    }
    validate_answers(&q.questions, &q.answers, true).map_err(Error::Quiz)?;
    let (_, item) = locate(client, &ctx.cid, &q.resource_id, cancel)
        .await?
        .ok_or(Error::Invalid)?;
    if number(&item["finished"]) != 0.0 {
        return Ok(TaskStatus::Queued);
    }
    let fresh = paper(client, &q.task_id, cancel).await?;
    if fresh.signature != q.signature {
        update(service, ctx, |q| {
            q.signature = fresh.signature.clone();
            q.questions = fresh.questions.clone();
            q.answers.clear();
            q.reviews.clear();
            q.stage = Stage::AwaitingChoice;
            q.message = "试卷发生变化，旧批准已失效，请重新作答".into();
        });
        return Ok(TaskStatus::AwaitingDecision);
    }
    let Some(action) = &fresh.action else {
        update(service, ctx, |q| {
            q.stage = Stage::Manual;
            q.message = "试卷需要官网验证或使用尚未核实的动态提交协议，请在官方窗口提交".into();
        });
        return Ok(TaskStatus::AwaitingDecision);
    };
    let fields = fresh.answer_fields(&q.answers).map_err(Error::Quiz)?;
    if !current(service, ctx) {
        return Err(Error::Cancelled);
    }
    update(service, ctx, |q| {
        q.stage = Stage::Submitting;
        q.submission_uncertain = true;
        q.message = "正在提交测验…".into();
    });
    let sent = client
        .quiz_http(action.as_str(), Some(&fields), cancel)
        .await;
    if sent == Err(Error::Unauthorized) {
        return Err(Error::Unauthorized);
    }
    if !current(service, ctx) {
        return Err(Error::Cancelled);
    }

    // 保存最后一次提交快照至 Toasty 数据库
    let clean_tid = qa::sanitize_task_id(&q.task_id);
    if let Ok(qa_db) = service.get_qa_db().await {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let record = qa::SubmittedPaperRecord {
            cid: ctx.cid.clone(),
            tid: clean_tid.to_string(),
            task_title: Some(q.title.clone()),
            questions: q.questions.clone(),
            answers: q.answers.clone(),
            submitted_at_ms: now_ms,
        };
        let _ = qa_db.save_submitted_paper(&record).await;
    }

    let verdict = sent
        .as_ref()
        .ok()
        .and_then(|bytes| protocol::verdict(bytes));
    if verdict == Some(false) || matches!(sent, Err(Error::Forbidden | Error::UnexpectedRedirect)) {
        update(service, ctx, |q| {
            q.stage = Stage::Failed;
            q.submission_uncertain = false;
            q.message = "测验未通过或提交被拒绝，已停止本课程；请查看官网结果".into();
        });

        // 规范要求 2：通过 AI 判题得出的答案不直接进入题库！
        // 测验未通过时，不能盲目将整卷 AI 答案上报为错误选项；必须通过官方 API (/exam/view) 验证具体单题正误后再入库
        let view_url = format!("{}/exam/view?cid={}&tid={clean_tid}", client.origin(), ctx.cid);
        if let Ok(view_bytes) = client.quiz_http(&view_url, None, cancel).await {
            let submitted = if let Ok(qa_db) = service.get_qa_db().await {
                qa_db.get_submitted_paper(Some(&ctx.cid), clean_tid).await.ok().flatten()
            } else {
                None
            };
            if let Some(details) = qa::parse_exam_view_details_with_submitted(&view_bytes, submitted.as_ref()) {
                if !details.questions.is_empty() && (!details.answers.is_empty() || !details.wrong_answers.is_empty()) {
                    let course_name = service
                        .snapshot()
                        .courses
                        .iter()
                        .find(|c| c.id == ctx.cid)
                        .map(|c| c.title.clone());
                    if let Ok(qa_db) = service.get_qa_db().await {
                        let _ = qa::record_passed_quiz_with_details(
                            qa_db,
                            &ctx.cid,
                            clean_tid,
                            &details.questions,
                            &details.answers,
                            &details.wrong_answers,
                            course_name.as_deref(),
                        )
                        .await;
                    }
                    let qa_url = service.snapshot().settings.qa_server_url.clone();
                    let qa_key = service.qa_api_key().await;
                    let payload = qa::build_batch_upload_payload_with_details(
                        clean_tid,
                        Some(&ctx.cid),
                        Some(&q.title),
                        course_name.as_deref(),
                        &details.questions,
                        &details.answers,
                        &details.wrong_answers,
                    );
                    let _ = qa::upload_batch(&qa_url, qa_key.as_deref(), &payload, Some(service)).await;
                }
            }
        }

        return Ok(TaskStatus::Failed);
    }
    update(service, ctx, |q| {
        q.stage = Stage::Verifying;
        q.message = "正在核对服务端测验结果…".into();
    });
    for _ in 0..5 {
        let (_, item) = locate(client, &ctx.cid, &q.resource_id, cancel)
            .await?
            .ok_or(Error::Invalid)?;
        if number(&item["finished"]) != 0.0 && verdict == Some(true) {
            update(service, ctx, |q| {
                q.stage = Stage::Passed;
                q.submission_uncertain = false;
                q.passed.push(q.resource_id.clone());
                q.message = "测验已通过服务端确认，继续课程".into();
            });
            service.fetch_progress(&ctx.cid, cancel).await?;

            // 完成测验时，直接执行单个题组构建与同步
            let _ = build_single_task_group(service, &ctx.cid, &q.task_id, Some(true)).await;

            return Ok(TaskStatus::Queued);
        }
        tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(std::time::Duration::from_secs(2))=>{}}
    }
    update(service, ctx, |q| {
        q.stage = Stage::Manual;
        q.message =
            "尚未确认测验通过，可能未通过、待批改或提交结果不确定；已停止当前课程，请查看官网结果"
                .into();
    });
    Ok(TaskStatus::AwaitingDecision)
}

async fn match_qa_action(service: &Arc<Service>, ctx: &Context, cancel: &CancellationToken) {
    update(service, ctx, |q| {
        q.stage = Stage::Preparing;
        q.message = "正在从题库检索答案并同步本地题目…".into();
    });
    let q = state(service, ctx);
    let qa_url = service.snapshot().settings.qa_server_url.clone();
    let qa_key = service.qa_api_key().await;

    // 自动比对后端是否存在缺失题目，并将后端缺失的题目上传
    if let Ok(qa_db) = service.get_qa_db().await {
        let _ = qa::sync_missing_to_server(
            qa_db,
            &qa_url,
            qa_key.as_deref(),
            Some(&ctx.cid),
            Some(&q.task_id),
            Some(service),
        )
        .await;
    }

    let res = tokio::select! {
        biased;
        _ = cancel.cancelled() => return,
        r = qa::query_task_answers(&qa_url, qa_key.as_deref(), &q.task_id, Some(&ctx.cid)) => r,
    };
    if !current(service, ctx) {
        return;
    }

    // 先检查本地题库是否有确认答案
    let local_bank = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.load_all_bank_questions().await.unwrap_or_default()
    } else {
        Default::default()
    };
    let mut local_hits = 0;
    for question in &q.questions {
        let key = qa::bank_key(&q.task_id, &question.id);
        if let Some(entry) = local_bank.get(&key) {
            if !entry.correct_answers.is_empty() {
                local_hits += 1;
                let mapped = qa::map_answers_to_option_ids(&entry.correct_answers, &question.options);
                let final_ans = if !mapped.is_empty() { mapped } else { entry.correct_answers.clone() };
                update(service, ctx, |state| {
                    state.answers.insert(question.id.clone(), final_ans.clone());
                    state.reviews.push(Review {
                        question_id: question.id.clone(),
                        answers: final_ans,
                        probabilities: Default::default(),
                        message: "本地题库命中已确认答案".into(),
                    });
                });
            }
        }
    }

    match res {
        Ok(qs) => {
            let matched = qa::match_answers(&qs, &q.questions);
            let count = matched.matched_answers.len() + local_hits;
            service.log_debug(
                "bank",
                format!("[题库匹配] 本地命中={local_hits}, 远程题目数={}, 最终合并命中={count}", qs.len()),
            );
            update(service, ctx, |q| {
                q.answers.extend(matched.matched_answers);
                for r in matched.reviews {
                    if let Some(existing) =
                        q.reviews.iter_mut().find(|ex| ex.question_id == r.question_id)
                    {
                        *existing = r;
                    } else {
                        q.reviews.push(r);
                    }
                }
                q.stage = Stage::AwaitingApproval;
                q.message = if count > 0 {
                    if qs.is_empty() {
                        format!("已从本地题库命中 {local_hits} 道题目答案（远程题库暂未收录该测验）")
                    } else {
                        format!("已从题库匹配 {count} 道题目的标准答案")
                    }
                } else {
                    "题库中暂未收录该测验的标准答案".into()
                };
            });
        }
        Err(e) => {
            service.log_debug("bank", format!("[题库匹配] 远程题库请求发生网络/服务异常: {e}"));
            if local_hits > 0 {
                update(service, ctx, |q| {
                    q.stage = Stage::AwaitingApproval;
                    q.message = format!("远程题库连接异常（{e}），已从本地题库命中 {local_hits} 道题目答案");
                });
            } else {
                update(service, ctx, |q| {
                    q.stage = Stage::AwaitingChoice;
                    q.message = format!("题库检索失败: {e}");
                });
            }
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Action {
    Generate,
    Submit,
    Check,
    MatchQa,
}
impl Service {
    pub(super) fn quiz_policy(&self, cid: String, mode: Mode) -> Result<(), String> {
        let mut r = self.runtime.lock().unwrap();
        let account = r.snapshot.user.as_ref().ok_or("请先登录")?.id.clone();
        if r.workers.contains_key(&cid) {
            return Err("请先暂停课程再修改答题模式".into());
        }
        if !r.snapshot.courses.iter().any(|c| c.id == cid) {
            return Err("课程不在开课列表".into());
        }
        let q = r.snapshot.quizzes.entry(key(&account, &cid)).or_default();
        q.account_id = account;
        q.cid = cid;
        q.mode = mode;
        q.revision += 1;
        self.publish(&mut r);
        Ok(())
    }
    pub(super) fn quiz_edit(
        &self,
        cid: String,
        revision: u64,
        answers: Answers,
    ) -> Result<(), String> {
        let mut r = self.runtime.lock().unwrap();
        let account = r.snapshot.user.as_ref().ok_or("请先登录")?.id.clone();
        if !r
            .snapshot
            .progress
            .get(&cid)
            .and_then(|p| p.data.as_ref())
            .and_then(|p| p.test.weight)
            .is_some_and(|w| w > 0.0)
        {
            return Err("测验权重为零或未知，编辑已禁用".into());
        }
        if r.workers.contains_key(&cid) {
            return Err("课程正在执行，请稍后编辑".into());
        }
        let q = r
            .snapshot
            .quizzes
            .get_mut(&key(&account, &cid))
            .ok_or("尚未加载试卷")?;
        if q.revision != revision {
            return Err("草稿已变化，请重新打开".into());
        }
        if q.submission_uncertain {
            return Err("上次提交结果尚未确认，请先检查官网".into());
        }
        validate_answers(&q.questions, &answers, false)?;
        q.answers = answers;
        q.revision += 1;
        q.stage = Stage::AwaitingApproval;
        q.message = "草稿已保存，尚未提交".into();
        self.publish(&mut r);
        Ok(())
    }
    pub(super) async fn quiz_action(
        self: &Arc<Self>,
        cid: String,
        revision: u64,
        action: Action,
    ) -> Result<(), String> {
        let (generation, cancel) = {
            let mut r = self.runtime.lock().unwrap();
            if r.snapshot.session != SessionStatus::Ready {
                return Err("请先登录 UOOC".into());
            }
            let account = r.snapshot.user.as_ref().ok_or("请先登录")?.id.clone();
            if r.workers.contains_key(&cid) {
                return Err("该课程已有操作正在执行".into());
            }
            let q = r
                .snapshot
                .quizzes
                .get(&key(&account, &cid))
                .ok_or("尚未加载试卷")?;
            if matches!(action, Action::Submit) && r.snapshot.settings.dry_run {
                return Err("预览模式不允许提交测验".into());
            }
            if q.revision != revision {
                return Err("草稿或试卷已变化，请重新确认".into());
            }
            if matches!(action, Action::Submit) {
                validate_answers(&q.questions, &q.answers, true)?;
                if matches!(q.stage, Stage::Submitting | Stage::Verifying) {
                    return Err("提交结果尚未确认，请先检查官网".into());
                }
            }
            let generation = if let Some(task) = r
                .snapshot
                .tasks
                .iter_mut()
                .find(|t| t.account_id == account && t.course.id == cid)
            {
                task.generation += 1;
                task.status = TaskStatus::Running;
                task.desired_running = true;
                task.generation
            } else {
                r.epoch += 1;
                r.epoch
            };
            let cancel = CancellationToken::new();
            r.workers.insert(cid.clone(), (generation, cancel.clone()));
            self.publish(&mut r);
            (generation, cancel)
        };

        let client = self.client();
        let result = async {
            self.sync_platform().map_err(Error::Quiz)?;
            let ctx = context(self, &cid, generation)?;
            if matches!(action, Action::Submit) && !allowed(self, &cid, &cancel).await? {
                return Err(Error::Quiz("测验权重为零或尚未确认，答题已禁用".into()));
            }
            match action {
                Action::Generate => {
                    let q = state(self, &ctx);
                    if q.questions.is_empty() {
                        if let Ok(p) = paper(&client, &q.task_id, &cancel).await {
                            update(self, &ctx, |q| {
                                q.signature = p.signature;
                                q.questions = p.questions;
                            });
                        }
                    }
                    generate(self, &ctx, &cancel, true).await;
                    if state(self, &ctx).stage == Stage::Preparing {
                        update(self, &ctx, |q| {
                            q.stage = Stage::AwaitingApproval;
                            q.message = "新答案已生成，请确认后提交".into();
                        });
                    }
                    Ok(TaskStatus::AwaitingDecision)
                }
                Action::MatchQa => {
                    let q = state(self, &ctx);
                    if q.questions.is_empty() {
                        if let Ok(p) = paper(&client, &q.task_id, &cancel).await {
                            update(self, &ctx, |q| {
                                q.signature = p.signature;
                                q.questions = p.questions;
                            });
                        }
                    }
                    match_qa_action(self, &ctx, &cancel).await;
                    Ok(TaskStatus::AwaitingDecision)
                }
                Action::Submit => submit(self, &client, &ctx, &cancel).await,
                Action::Check => Ok(TaskStatus::Queued),
            }
        }
        .await;

        self.finish(&cid, generation, result.clone(), &client);
        if result == Ok(TaskStatus::Queued) {
            self.reconcile();
        }

        result.map(|_| ()).map_err(|e| e.message())
    }
    pub(super) async fn quiz_portal(
        self: &Arc<Self>,
        parent: tauri::WebviewWindow,
        cid: String,
    ) -> Result<(), String> {
        super::login::local_parent(&parent)?;
        let (account, task, epoch) = {
            let r = self.runtime.lock().unwrap();
            let a = r.snapshot.user.as_ref().ok_or("请先登录")?.id.clone();
            let q = r
                .snapshot
                .quizzes
                .get(&key(&a, &cid))
                .ok_or("尚未找到测验")?;
            (a, q.task_id.clone(), r.epoch)
        };

        if task.parse::<u64>().ok().is_none_or(|t| t == 0) {
            return Err("测验任务编号无效".into());
        }
        let label = format!("uooc-quiz-{}", uuid::Uuid::new_v4());
        {
            let mut r = self.runtime.lock().unwrap();
            if r.epoch != epoch || r.snapshot.session != SessionStatus::Ready {
                return Err("会话已变化，请重试".into());
            }
            r.quiz_windows.insert(label.clone(), (epoch, cid.clone()));
        }
        #[cfg(target_os = "android")]
        let navigation = {
            let owner = Arc::downgrade(self);
            let expected = label.clone();
            let expected_account = account.clone();
            super::user_agent::AndroidNavigation::new(
                parent.app_handle().clone(),
                label.clone(),
                move || {
                    owner.upgrade().is_some_and(|s| {
                        let r = s.runtime.lock().unwrap();
                        r.epoch == epoch
                            && r.snapshot.session == SessionStatus::Ready
                            && r.snapshot
                                .user
                                .as_ref()
                                .is_some_and(|u| u.id == expected_account)
                            && r.quiz_windows.contains_key(&expected)
                    })
                },
                || {},
            )
        };
        let builder = tauri::WebviewWindowBuilder::new(
            parent.app_handle(),
            &label,
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .title("UOOC 官方测验");
        #[cfg(not(target_os = "android"))]
        let builder = builder.user_agent(super::user_agent::DESKTOP);
        #[cfg(target_os = "android")]
        let builder = builder
            .on_navigation(move |url| navigation.navigate(url))
            .activity_name("FeatureActivity")
            .created_by_activity_name(parent.activity_name().map_err(|_| "无法确认父窗口")?);
        let window = match builder.build() {
            Ok(w) => w,
            Err(_) => {
                self.runtime.lock().unwrap().quiz_windows.remove(&label);
                return Err("无法打开官方试卷".into());
            }
        };
        let copy = window.clone();
        let jar = self.client().jar.clone();
        let lease = self.native_session.clone().lock_owned().await;
        let installed = tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            super::session::install(&copy, &jar, false)
        })
        .await
        .unwrap_or_else(|_| Err("无法恢复会话".into()));
        if installed.is_err()
            || self
                .snapshot()
                .user
                .as_ref()
                .is_none_or(|u| u.id != account)
            || self.runtime.lock().unwrap().epoch != epoch
        {
            self.runtime.lock().unwrap().quiz_windows.remove(&label);
            let _ = window.destroy();
            return Err("会话恢复失败或账户已变化".into());
        }
        window
            .navigate(
                format!("https://www.uooc.net.cn/exam/{task}")
                    .parse()
                    .unwrap(),
            )
            .map_err(|_| "无法加载官方试卷".into())
    }

    /// 强制拉取已完成测验的题目与标准答案，展示在测验面板中
    pub(crate) async fn fetch_completed_quiz(
        &self,
        cid: &str,
        task_id: Option<&str>,
        resource_id: Option<&str>,
    ) -> Result<(), String> {
        let (account, client) = {
            let r = self.runtime.lock().unwrap();
            let account = r.snapshot.user.as_ref().ok_or("请先登录 UOOC")?.id.clone();
            (account, self.client())
        };

        let cancel = CancellationToken::new();

        // 确定待拉取的测验：优先使用传入的 task_id，或者通过 resource_id 查找，最后回退
        let mut target_quiz = None;
        if let Some(tid) = task_id.filter(|s| !s.trim().is_empty()) {
            target_quiz = Some((tid.to_string(), String::new(), true));
        }

        // 复用任务列表发现项查找对应的测验
        if target_quiz.is_none() {
            if let Ok((pending, completed, _)) = super::runner::discover(&client, cid, &cancel).await {
                for v in completed.into_iter().chain(pending.into_iter()) {
                    if v.task_type == "quiz" || v.title.contains("测验") {
                        if let Some(tid) = v.task_id {
                            if resource_id.is_none() || resource_id == Some(v.resource.as_str()) {
                                target_quiz = Some((tid, v.title, true));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if target_quiz.is_none() {
            if let Ok(rows) = client.quiz_list(cid, &cancel).await {
                for row in rows.iter().rev() {
                    if let Some(rid) = protocol::resource_id(row) {
                        if resource_id.is_some_and(|target_rid| target_rid == rid.as_str()) {
                            if let Ok(Some((_, item))) = locate_row(&client, cid, row, &cancel).await {
                                if let Some(tid) = id(&item["task_id"]) {
                                    target_quiz = Some((tid, text(&item["title"]), number(&item["finished"]) != 0.0));
                                    break;
                                }
                            }
                        }
                    }
                }
                if target_quiz.is_none() {
                    for row in rows.iter().rev() {
                        if let Some(_rid) = protocol::resource_id(row) {
                            if let Ok(Some((_, item))) = locate_row(&client, cid, row, &cancel).await {
                                let is_finished = number(&item["finished"]) != 0.0;
                                if let Some(tid) = id(&item["task_id"]) {
                                    target_quiz = Some((tid, text(&item["title"]), is_finished));
                                    if is_finished {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let (task_id, task_title, is_finished) = target_quiz.ok_or("该课程下未找到测验")?;
        let clean_tid = qa::sanitize_task_id(&task_id);

        let submitted = if let Ok(qa_db) = self.get_qa_db().await {
            qa_db.get_submitted_paper(Some(cid), clean_tid).await.ok().flatten()
        } else {
            None
        };

        // 优先直接获取已完成试卷内容，避免打开试卷时阻塞进行耗时的向量特征构建与网络同步
        let mut details = fetch_task_group_content(&client, cid, clean_tid, is_finished, submitted.as_ref(), &cancel).await?;

        if details.questions.is_empty() {
            let (_, fallback_details) = build_single_task_group(self, cid, clean_tid, Some(is_finished)).await?;
            details = fallback_details;
        }

        if details.questions.is_empty() {
            return Err("未能获取测验试卷题目，可能尚未开放或需要重新登录".into());
        }

        // 规范化答案值匹配到选项
        for q in &details.questions {
            if let Some(ans_list) = details.answers.get_mut(&q.id) {
                let mut mapped = Vec::new();
                for val in ans_list.iter() {
                    let val_trim = val.trim();
                    if let Some(opt) = q.options.iter().find(|o| o.id.eq_ignore_ascii_case(val_trim) || o.text.trim() == val_trim) {
                        mapped.push(opt.id.clone());
                    } else {
                        mapped.push(val.clone());
                    }
                }
                *ans_list = mapped;
            }
            if let Some(stu_list) = details.student_answers.get_mut(&q.id) {
                let mut mapped = Vec::new();
                for val in stu_list.iter() {
                    let val_trim = val.trim();
                    if let Some(opt) = q.options.iter().find(|o| o.id.eq_ignore_ascii_case(val_trim) || o.text.trim() == val_trim) {
                        mapped.push(opt.id.clone());
                    } else {
                        mapped.push(val.clone());
                    }
                }
                *stu_list = mapped;
            }
        }

        let message = if let Some(score) = details.grade_score {
            if let Some(total) = details.total_score {
                format!("测验已完成，得分：{:.2} / {:.2} 分", score, total)
            } else {
                format!("测验已完成，得分：{:.2} 分", score)
            }
        } else if is_finished {
            "已拉取已完成测验的题目与作答情况".into()
        } else {
            "已拉取测验题目，可继续答题".into()
        };

        // 更新 Snapshot 中的测验状态
        let mut r = self.runtime.lock().unwrap();
        let q = r.snapshot.quizzes.entry(key(&account, cid)).or_default();
        q.account_id = account;
        q.cid = cid.to_string();
        q.task_id = clean_tid.to_string();
        q.title = details.task_title.unwrap_or(task_title);
        q.questions = details.questions;
        q.answers = details.answers;
        q.reviews = details.reviews;
        q.student_answers = details.student_answers;
        q.stage = if is_finished { Stage::Passed } else { Stage::AwaitingApproval };
        q.message = message;
        q.revision += 1;
        self.publish(&mut r);

        Ok(())
    }
}
pub(super) fn window_closed(service: &Arc<Service>, label: &str) {
    let closed = service.runtime.lock().unwrap().quiz_windows.remove(label);
    if let Some((epoch, cid)) = closed {
        let revision = {
            let r = service.runtime.lock().unwrap();
            if r.epoch != epoch {
                return;
            }
            if r.snapshot
                .tasks
                .iter()
                .find(|t| t.course.id == cid)
                .is_none_or(|t| !t.desired_running)
            {
                return;
            }
            r.snapshot
                .user
                .as_ref()
                .and_then(|u| r.snapshot.quizzes.get(&key(&u.id, &cid)))
                .map(|q| q.revision)
        };
        if let Some(revision) = revision {
            let svc = service.clone();
            tauri::async_runtime::spawn(async move {
                let _ = svc.quiz_action(cid, revision, Action::Check).await;
            });
        }
    }
}

async fn fetch_task_group_content(
    client: &Client,
    cid: &str,
    tid: &str,
    is_finished: bool,
    submitted: Option<&qa::SubmittedPaperRecord>,
    cancel: &CancellationToken,
) -> Result<qa::ExamViewDetails, String> {
    let clean_tid = qa::sanitize_task_id(tid);
    let view_url = format!("{}/exam/view?cid={cid}&tid={clean_tid}", client.origin());
    let tp_url = format!("{}/exam/getTaskPaper?tid={clean_tid}", client.origin());

    let mut details = qa::ExamViewDetails::default();

    if is_finished {
        // 1. 已提交判题的题目：强制走 https://www.uooc.net.cn/exam/view?cid=&tid= 获取题目+作答+标准答案+评分
        if let Ok(view_bytes) = client.quiz_http(&view_url, None, cancel).await {
            if let Some(d) = qa::parse_exam_view_details_with_submitted(&view_bytes, submitted) {
                details = d;
            }
        }

        // 尝试补充 getTaskPaper 获取单题全局数字 ID 与纯净选项
        if let Ok(tp_bytes) = client.quiz_http(&tp_url, None, cancel).await {
            if let Some((tp_qs, tp_ans)) = qa::parse_task_paper(&tp_bytes) {
                if details.questions.is_empty() {
                    details.questions = tp_qs;
                    details.answers = tp_ans;
                } else {
                    for (qid, ans) in tp_ans {
                        details.answers.entry(qid).or_insert(ans);
                    }
                }
            }
        }
    } else {
        // 2. 未确定是否已提交：先尝试从 /exam/getTaskPaper 获取题单
        if let Ok(tp_bytes) = client.quiz_http(&tp_url, None, cancel).await {
            if let Some((tp_qs, tp_ans)) = qa::parse_task_paper(&tp_bytes) {
                if !tp_qs.is_empty() {
                    details.questions = tp_qs;
                    details.answers = tp_ans;
                }
            }
        }

        // 若用户还未提交过题目，则临时保存答案一次(请求保存草稿api)，此时再获取题目
        if details.questions.is_empty() {
            if let Ok(p) = paper(client, clean_tid, cancel).await {
                // 保存答案 API（POST /exam/{tid} 保存草稿）
                let save_url = format!("{}/exam/{clean_tid}", client.origin());
                let _ = client.quiz_http(&save_url, Some(&p.fields), cancel).await;

                // 临时保存后再执行获取题目
                if let Ok(tp_bytes) = client.quiz_http(&tp_url, None, cancel).await {
                    if let Some((tp_qs, tp_ans)) = qa::parse_task_paper(&tp_bytes) {
                        if !tp_qs.is_empty() {
                            details.questions = tp_qs;
                            details.answers = tp_ans;
                        }
                    }
                }
                if details.questions.is_empty() {
                    details.questions = p.questions;
                }
            }
        }

        // 关键兜底：用户可能在加入列表前已经在 UOOC 官网答过题并提交！
        // 若标准答案依然为空，尝试请求 /exam/view?cid=&tid= 获取用户提交答案与标准答案
        if details.answers.is_empty() {
            if let Ok(view_bytes) = client.quiz_http(&view_url, None, cancel).await {
                if let Some(v_details) = qa::parse_exam_view_details_with_submitted(&view_bytes, submitted) {
                    if !details.questions.is_empty() {
                        qa::align_view_details_to_questions(
                            &v_details,
                            &details.questions,
                            &mut details.answers,
                            &mut details.reviews,
                            &mut details.student_answers,
                        );
                        details.wrong_answers = v_details.wrong_answers;
                        details.grade_score = v_details.grade_score;
                        details.total_score = v_details.total_score;
                    } else if !v_details.questions.is_empty() {
                        details = v_details;
                    }
                }
            }
        }
    }

    // 兜底：如果依然为空，尝试从 HTML 解析
    if details.questions.is_empty() {
        if let Ok(p) = paper(client, clean_tid, cancel).await {
            details.questions = p.questions;
        }
    }

    // 终极保护：只要题目不为空但答案为空，始终请求一次 /exam/view 确保不漏掉官网已作答的答案
    if !details.questions.is_empty() && details.answers.is_empty() {
        if let Ok(view_bytes) = client.quiz_http(&view_url, None, cancel).await {
            if let Some(v_details) = qa::parse_exam_view_details_with_submitted(&view_bytes, submitted) {
                qa::align_view_details_to_questions(
                    &v_details,
                    &details.questions,
                    &mut details.answers,
                    &mut details.reviews,
                    &mut details.student_answers,
                );
                details.wrong_answers = v_details.wrong_answers;
                details.grade_score = v_details.grade_score;
                details.total_score = v_details.total_score;
            }
        }
    }

    // 兜底：若 API / HTML 解析未能提供题目，但存在本地持久化的提交快照，直接复用快照
    if details.questions.is_empty() {
        if let Some(sub) = submitted {
            details.questions = sub.questions.clone();
            details.answers = sub.answers.clone();
            details.student_answers = sub.answers.clone();
            details.task_title = sub.task_title.clone();
        }
    }

    Ok(details)
}

pub(crate) async fn build_single_task_group(
    service: &Service,
    cid: &str,
    tid: &str,
    known_finished: Option<bool>,
) -> Result<(usize, qa::ExamViewDetails), String> {
    let clean_tid = qa::sanitize_task_id(tid);
    let (client, qa_url, qa_key) = {
        let r = service.runtime.lock().unwrap();
        (
            service.client(),
            r.snapshot.settings.qa_server_url.clone(),
            r.qa_api_key.clone(),
        )
    };
    let cancel = CancellationToken::new();

    let (course_title, task_title) = {
        let r = service.runtime.lock().unwrap();
        let c_name = r
            .snapshot
            .courses
            .iter()
            .find(|c| c.id == cid)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| format!("课程 {cid}"));
        let t_name = r
            .snapshot
            .tasks
            .iter()
            .find(|t| t.course.id == cid)
            .and_then(|t| {
                t.videos
                    .iter()
                    .find(|v| v.task_id.as_deref().map(qa::sanitize_task_id) == Some(clean_tid))
                    .map(|v| v.title.clone())
            })
            .unwrap_or_else(|| format!("测验 {clean_tid}"));
        (c_name, t_name)
    };

    service.emit_bank_progress(&super::model::BankRebuildProgress {
        stage: "task".into(),
        cid: cid.to_string(),
        course_name: course_title.clone(),
        tid: Some(clean_tid.to_string()),
        task_name: Some(task_title.clone()),
        question_id: None,
        question_stem: None,
        current: 0,
        total: 0,
        message: format!("正在解析测验「{}」(tid: {})...", task_title, clean_tid),
    });

    // 1. 判断该题单 finished 是 true 还是 false（检测任务列表、完成记录与 getCatalogList）
    let is_finished = match known_finished {
        Some(f) => f,
        None => {
            let mut fin = false;
            // 检查已完成任务列表与已通过测验
            {
                let r = service.runtime.lock().unwrap();
                if let Some(task) = r.snapshot.tasks.iter().find(|t| t.course.id == cid) {
                    if task.videos.iter().any(|v| v.task_id.as_deref().map(qa::sanitize_task_id) == Some(clean_tid) && v.status == TaskStatus::Completed) {
                        fin = true;
                    }
                }
                if !fin {
                    if r.snapshot.quizzes.values().any(|q| q.cid == cid && qa::sanitize_task_id(&q.task_id) == clean_tid && q.stage == Stage::Passed) {
                        fin = true;
                    }
                }
            }
            if !fin {
                if let Ok(catalog) = client.catalog(cid, &cancel).await {
                    let mut queue = std::collections::VecDeque::new();
                    for node in catalog {
                        queue.push_back(node);
                    }
                    while let Some(n) = queue.pop_front() {
                        let node_tid = id(&n["task_id"]).unwrap_or_default();
                        let node_id = id(&n["id"]).unwrap_or_default();
                        if qa::sanitize_task_id(&node_tid) == clean_tid || qa::sanitize_task_id(&node_id) == clean_tid {
                            fin = number(&n["finished"]) != 0.0;
                            break;
                        }
                        if let Some(children) = n["children"].as_array() {
                            for c in children {
                                queue.push_back(c.clone());
                            }
                        }
                    }
                }
            }
            fin
        }
    };

    // 2. 按照核心流程获取题目与答案
    let submitted = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.get_submitted_paper(Some(cid), clean_tid).await.ok().flatten()
    } else {
        None
    };

    let mut details = fetch_task_group_content(&client, cid, clean_tid, is_finished, submitted.as_ref(), &cancel)
        .await
        .unwrap_or_default();

    for (q_idx, q) in details.questions.iter().enumerate() {
        let snippet = q.text.chars().take(40).collect::<String>();
        service.emit_bank_progress(&super::model::BankRebuildProgress {
            stage: "question".into(),
            cid: cid.to_string(),
            course_name: course_title.clone(),
            tid: Some(clean_tid.to_string()),
            task_name: Some(task_title.clone()),
            question_id: Some(q.id.clone()),
            question_stem: Some(snippet),
            current: q_idx + 1,
            total: details.questions.len(),
            message: format!("已解析题目 [ID: {}]", q.id),
        });
    }

    // 3. 从服务端尝试拉取已有众包答案，并检测题组在服务端的存在状态
    let mut server_lacks_task_group = false;
    match qa::query_task_answers(&qa_url, qa_key.as_deref(), clean_tid, Some(cid)).await {
        Ok(server_qs) => {
            let mut server_answers = Answers::new();
            for sq in &server_qs {
                if let Some(qid) = &sq.question_id {
                    if let Some(ca) = sq.answers.iter().find(|a| a.is_correct) {
                        if !ca.values.is_empty() {
                            server_answers.insert(qid.clone(), ca.values.clone());
                        }
                    }
                }
            }
            if !server_answers.is_empty() {
                for (qid, ans) in server_answers {
                    details.answers.entry(qid).or_insert(ans);
                }
            }
        }
        Err(e) if e.contains("Task group not found in this course") => {
            service.log_debug(
                "bank",
                format!("服务端提示「Task group not found in this course」[cid: {cid}, tid: {clean_tid}]，需要创建该新题组到服务器"),
            );
            server_lacks_task_group = true;
        }
        Err(e) => {
            service.log_debug(
                "bank",
                format!("查询服务端题组失败 [cid: {cid}, tid: {clean_tid}]: {e}"),
            );
        }
    }

    // 4. 规范要求 1：当通过官方 API 确定标准答案后，淘汰排除项，并清除 wrong_answers
    for qid in details.answers.keys() {
        details.wrong_answers.remove(qid);
    }

    // 记录到 Toasty 数据库题库 (BankQuestion)
    if !details.questions.is_empty() {
        if let Ok(qa_db) = service.get_qa_db().await {
            for q in &details.questions {
                let ans = details.answers.get(&q.id).cloned().unwrap_or_default();
                let wrongs = details.wrong_answers.get(&q.id).cloned();
                let q_type_str = match q.kind {
                    Kind::Single => {
                        if q.options.len() == 2 {
                            let opt0 = q.options[0].text.trim().to_lowercase();
                            let opt1 = q.options[1].text.trim().to_lowercase();
                            if (opt0.contains("正确") || opt0.contains("对") || opt0 == "t" || opt0 == "true" || opt0 == "a")
                                && (opt1.contains("错误") || opt1.contains("错") || opt1 == "f" || opt1 == "false" || opt1 == "b")
                            {
                                "judge"
                            } else {
                                "single"
                            }
                        } else {
                            "single"
                        }
                    }
                    Kind::Multiple => "multiple",
                    Kind::Text | Kind::Subjective => "text",
                };

                let mut bank_answers = Vec::new();
                if !ans.is_empty() {
                    let values: Vec<serde_json::Value> = if q_type_str == "judge" {
                        ans.iter().map(|s| {
                            let st = s.trim().to_lowercase();
                            let b = st == "a" || st == "true" || st == "正确" || st == "对" || st == "t" || st.starts_with("a.");
                            serde_json::Value::Bool(b)
                        }).collect()
                    } else if q_type_str == "single" || q_type_str == "multiple" {
                        ans.iter().map(|s| {
                            if let Some(opt) = q.options.iter().find(|o| qa::is_option_match(o, s)) {
                                serde_json::Value::String(opt.id.clone())
                            } else {
                                serde_json::Value::String(s.clone())
                            }
                        }).collect()
                    } else {
                        ans.iter().map(|s| serde_json::Value::String(s.clone())).collect()
                    };

                    bank_answers.push(qa::LocalBankAnswer {
                        id: None,
                        values,
                        is_correct: true,
                        source: "local".into(),
                        confidence: 1.0,
                    });
                } else if let Some(w_list) = wrongs {
                    for w in w_list {
                        let val = if let Some(opt) = q.options.iter().find(|o| qa::is_option_match(o, &w)) {
                            serde_json::Value::String(opt.id.clone())
                        } else {
                            serde_json::Value::String(w)
                        };
                        bank_answers.push(qa::LocalBankAnswer {
                            id: None,
                            values: vec![val],
                            is_correct: false,
                            source: "local".into(),
                            confidence: 1.0,
                        });
                    }
                }

                let b_q = qa::LocalBankQuestion {
                    cid: cid.to_string(),
                    course_name: Some(course_title.clone()),
                    tid: clean_tid.to_string(),
                    task_name: Some(task_title.clone()),
                    question_id: q.id.clone(),
                    question_type: q_type_str.into(),
                    stem: if q.text.trim().is_empty() { None } else { Some(qa::strip_html_tags(&q.text)) },
                    options: if q.options.is_empty() {
                        None
                    } else {
                        Some(
                            q.options
                                .iter()
                                .map(|o| qa::QaUploadOption {
                                    id: qa::strip_html_tags(&o.id),
                                    text: qa::strip_html_tags(&o.text),
                                })
                                .collect(),
                        )
                    },
                    blanks: q.blanks,
                    answers: bank_answers,
                    correct_answers: ans,
                    uploaded: false,
                    updated_at_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0),
                };
                let _ = qa_db.upsert_bank_question(&b_q, Some(cid), Some(clean_tid)).await;
            }
        }
    }

    // 5. 当检测到服务端缺少该题组（无论是提示 Task group not found in this course，还是课程接口中未包含），创建该题组到服务器
    if !server_lacks_task_group {
        let course_api_url = format!("{}/api/courses/{}", qa_url.trim_end_matches('/'), cid);
        let mut server_has_tid = false;
        if let Ok(client_http) = qa::http_client_for_url(&course_api_url) {
            let mut req = client_http.get(&course_api_url);
            if let Some(key) = &qa_key {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    req = req.header("x-api-key", trimmed);
                }
            }
            if let Ok(resp) = req.send().await {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(groups) = val["taskGroups"].as_array() {
                        for g in groups {
                            if id(&g["tid"]).as_deref().map(qa::sanitize_task_id) == Some(clean_tid) {
                                server_has_tid = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        if !server_has_tid {
            server_lacks_task_group = true;
        }
    }

    if server_lacks_task_group {
        service.log_debug(
            "bank",
            format!("服务端缺少测验题组 [cid: {cid}, tid: {clean_tid}, task: {task_title}]，正在创建新题组并上传试题至服务端..."),
        );
        let upload_payload = qa::build_batch_upload_payload_with_details(
            clean_tid,
            Some(cid),
            Some(&task_title),
            Some(&course_title),
            &details.questions,
            &details.answers,
            &details.wrong_answers,
        );
        if let Ok(res) = qa::upload_batch(&qa_url, qa_key.as_deref(), &upload_payload, Some(service)).await {
            if res.ok && !details.questions.is_empty() {
                if let Ok(qa_db) = service.get_qa_db().await {
                    let qids: Vec<String> = details.questions.iter().map(|q| q.id.clone()).collect();
                    let _ = qa_db.mark_questions_uploaded(&qids).await;
                }
            }
        }
    } else {
        // 服务端已有该题单，同步缺失的题目
        if let Ok(qa_db) = service.get_qa_db().await {
            let _ = qa::sync_missing_to_server(
                qa_db,
                &qa_url,
                qa_key.as_deref(),
                Some(cid),
                Some(clean_tid),
                Some(service),
            )
            .await;
        }
    }

    // 完成题目收录后，部分同步该课程的向量缓存
    if let Ok(qa_db) = service.get_qa_db().await {
        let _ = qa::download_qa_vectors(qa_db, &qa_url, qa_key.as_deref(), &[cid.to_string()]).await;
    }

    service.emit_bank_progress(&super::model::BankRebuildProgress {
        stage: "task_complete".into(),
        cid: cid.to_string(),
        course_name: course_title.clone(),
        tid: Some(clean_tid.to_string()),
        task_name: Some(task_title.clone()),
        question_id: None,
        question_stem: None,
        current: details.questions.len(),
        total: details.questions.len(),
        message: format!("测验「{}」已完成收录（共 {} 道题目）", task_title, details.questions.len()),
    });

    Ok((details.questions.len(), details))
}

pub(crate) async fn build_local_bank(service: &Arc<Service>) -> Result<usize, String> {
    let Ok(_guard) = service.bank_building.try_lock() else {
        if let Ok(qa_db) = service.get_qa_db().await {
            return Ok(qa_db.count_questions().await.unwrap_or(0));
        }
        return Ok(0);
    };

    service.log_debug("bank", "开始构建本地题库...");
    service.emit_bank_progress(&super::model::BankRebuildProgress {
        stage: "start".into(),
        cid: String::new(),
        course_name: String::new(),
        tid: None,
        task_name: None,
        question_id: None,
        question_stem: None,
        current: 0,
        total: 0,
        message: "正在准备构建题库...".into(),
    });
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.snapshot.logs.push("开始构建本地题库...".into());
        service.publish(&mut runtime);
    }

    // 规范要求 2：AI 判题或内存中的未交卷草稿绝对不直接进入题库！
    // 题库构建严格只扫描官方 API 确定已完成/已判分试卷，通过 /exam/getTaskPaper 和 /exam/view 提取标准答案。

    // 1. Scan enrolled courses & compare with server
    let (client, courses, qa_url, qa_key) = {
        let r = service.runtime.lock().unwrap();
        (
            service.client(),
            r.snapshot.courses.clone(),
            r.snapshot.settings.qa_server_url.clone(),
            r.qa_api_key.clone(),
        )
    };

    let cancel = CancellationToken::new();
    for (c_idx, course) in courses.iter().enumerate() {
        service.emit_bank_progress(&super::model::BankRebuildProgress {
            stage: "course".into(),
            cid: course.id.clone(),
            course_name: course.title.clone(),
            tid: None,
            task_name: None,
            question_id: None,
            question_stem: None,
            current: c_idx + 1,
            total: courses.len(),
            message: format!("正在检索课程「{}」(cid: {}) 的题单与测验...", course.title, course.id),
        });

        // 对照服务端检查当前课程下已收录的题单与题目覆盖率，拉取服务端已知 taskGroups
        let mut server_tids = HashSet::new();
        let course_api_url = format!("{}/api/courses/{}", qa_url.trim_end_matches('/'), course.id);
        if let Ok(client_http) = qa::http_client_for_url(&course_api_url) {
            let mut req = client_http.get(&course_api_url);
            if let Some(key) = &qa_key {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    req = req.header("x-api-key", trimmed);
                }
            }
            if let Ok(resp) = req.send().await {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(groups) = val["taskGroups"].as_array() {
                        for g in groups {
                            if let Some(tid) = id(&g["tid"]) {
                                server_tids.insert(tid);
                            }
                        }
                    }
                }
            }
        }

        let mut course_quizzes: Vec<(String, String, bool)> = Vec::new();
        let mut seen_tids = HashSet::new();
        let mut finished_tids = HashSet::new();

        // 1. 优先复用任务列表已运行任务中的测验项（已完成与视频列表）
        {
            let r = service.runtime.lock().unwrap();
            for q in r.snapshot.quizzes.values() {
                if q.cid == course.id && q.stage == Stage::Passed {
                    finished_tids.insert(q.task_id.clone());
                }
            }
            if let Some(task) = r.snapshot.tasks.iter().find(|t| t.course.id == course.id) {
                for v in &task.videos {
                    if v.status == TaskStatus::Completed {
                        if let Some(tid) = &v.task_id {
                            finished_tids.insert(tid.clone());
                        }
                    }
                    if v.task_type.as_deref() == Some("quiz") || v.title.contains("测验") {
                        if let Some(tid) = &v.task_id {
                            let is_fin = finished_tids.contains(tid);
                            if seen_tids.insert(tid.clone()) {
                                course_quizzes.push((tid.clone(), v.title.clone(), is_fin));
                            }
                        }
                    }
                }
            }
        }

        // 2. 复用任务列表相同的 discover 逻辑，平铺扫描完整章节树获取全部测验
        if let Ok((pending, completed, _)) = super::runner::discover(&client, &course.id, &cancel).await {
            for v in completed {
                if v.task_type == "quiz" || v.title.contains("测验") {
                    if let Some(tid) = v.task_id {
                        finished_tids.insert(tid.clone());
                        if seen_tids.insert(tid.clone()) {
                            course_quizzes.push((tid, v.title, true));
                        }
                    }
                }
            }
            for v in pending {
                if v.task_type == "quiz" || v.title.contains("测验") {
                    if let Some(tid) = v.task_id {
                        let is_fin = finished_tids.contains(&tid);
                        if seen_tids.insert(tid.clone()) {
                            course_quizzes.push((tid, v.title, is_fin));
                        }
                    }
                }
            }
        }

        // 3. 从 quiz_list (testList) 补充额外测验
        if let Ok(rows) = client.quiz_list(&course.id, &cancel).await {
            for row in rows {
                if let Ok(Some((_, item))) = locate_row(&client, &course.id, &row, &cancel).await {
                    let is_fin = number(&item["finished"]) != 0.0;
                    if let Some(tid) = id(&item["task_id"]) {
                        if is_fin {
                            finished_tids.insert(tid.clone());
                        }
                        if seen_tids.insert(tid.clone()) {
                            course_quizzes.push((tid, text(&item["title"]), is_fin));
                        }
                    }
                }
            }
        }

        // 3.1 从 exam_list 补充已批改/已完成考试
        if let Ok(rows) = client.exam_list(&course.id, &cancel).await {
            for row in rows {
                let status_code = if row.get("status_code").is_some() {
                    number(&row["status_code"]) as i32
                } else {
                    number(&row["status"]) as i32
                };
                let status_text = text(&row["status"]);
                let is_fin = status_code >= 20 || status_text == "已批改" || status_text == "已完成";
                let task_id = id(&row["task_id"]).or_else(|| id(&row["id"]));
                if let Some(tid) = task_id {
                    let clean = qa::sanitize_task_id(&tid);
                    if is_fin {
                        finished_tids.insert(clean.to_string());
                    }
                    if seen_tids.insert(clean.to_string()) {
                        let title = text(&row["title"]);
                        let title = if title.is_empty() {
                            text(&row["name"])
                        } else {
                            title
                        };
                        course_quizzes.push((clean.to_string(), title, is_fin));
                    }
                }
            }
        }

        // 3.2 从 homework_list 补充已批改/已完成作业
        if let Ok(rows) = client.homework_list(&course.id, &cancel).await {
            for row in rows {
                let status_code = if row.get("status_code").is_some() {
                    number(&row["status_code"]) as i32
                } else {
                    number(&row["status"]) as i32
                };
                let status_text = text(&row["status"]);
                let is_fin = status_code >= 20 || status_text == "已批改" || status_text == "已完成";
                let task_id = id(&row["task_id"]).or_else(|| id(&row["id"]));
                if let Some(tid) = task_id {
                    let clean = qa::sanitize_task_id(&tid);
                    if is_fin {
                        finished_tids.insert(clean.to_string());
                    }
                    if seen_tids.insert(clean.to_string()) {
                        let title = text(&row["title"]);
                        let title = if title.is_empty() {
                            text(&row["name"])
                        } else {
                            title
                        };
                        course_quizzes.push((clean.to_string(), title, is_fin));
                    }
                }
            }
        }

        // 3.3 从 Toasty 数据库历史提交记录补充测验与考试快照
        if let Ok(qa_db) = service.get_qa_db().await {
            if let Ok(submitted_papers) = qa_db.load_all_submitted_papers_for_course(&course.id).await {
                for sp in submitted_papers {
                    let clean = qa::sanitize_task_id(&sp.tid);
                    finished_tids.insert(clean.to_string());
                    if seen_tids.insert(clean.to_string()) {
                        course_quizzes.push((clean.to_string(), sp.task_title.unwrap_or_default(), true));
                    }
                }
            }
        }

        // 4. 从本地已持久化的 Toasty 题库补充该课程已知但可能暂未列出的 tid
        if let Ok(qa_db) = service.get_qa_db().await {
            if let Ok(local_bank) = qa_db.load_all_bank_questions().await {
                for q in local_bank.values() {
                    if q.cid == course.id {
                        let is_fin = !q.correct_answers.is_empty();
                        let clean = qa::sanitize_task_id(&q.tid);
                        if is_fin {
                            finished_tids.insert(clean.to_string());
                        }
                        if seen_tids.insert(clean.to_string()) {
                            course_quizzes.push((clean.to_string(), q.course_name.clone().unwrap_or_default(), is_fin));
                        }
                    }
                }
            }
        }

        // 5. 从服务端返回的该课程 taskGroups 中补充服务端已知但本地未发现的 tid
        for s_tid in &server_tids {
            let is_fin = finished_tids.contains(s_tid);
            if seen_tids.insert(s_tid.clone()) {
                course_quizzes.push((s_tid.clone(), String::new(), is_fin));
            }
        }

        // 6. 对每个测验题单执行单个题组构建
        for (task_id, _title, is_fin) in &course_quizzes {
            let fin = *is_fin || finished_tids.contains(task_id);
            let _ = build_single_task_group(service, &course.id, task_id, Some(fin)).await;
        }

        // 7. 课程级别最终全量同步
        if let Ok(qa_db) = service.get_qa_db().await {
            let _ = qa::sync_missing_to_server(
                qa_db,
                &qa_url,
                qa_key.as_deref(),
                Some(&course.id),
                None,
                Some(service),
            )
            .await;
        }
    }

    let total = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.count_questions().await.unwrap_or(0)
    } else {
        0
    };
    service.log_debug("bank", format!("本地题库构建完成，当前收录 {total} 道题目"));
    service.emit_bank_progress(&super::model::BankRebuildProgress {
        stage: "complete".into(),
        cid: String::new(),
        course_name: String::new(),
        tid: None,
        task_name: None,
        question_id: None,
        question_stem: None,
        current: total,
        total,
        message: format!("本地题库重构完成，共收录 {total} 道题目"),
    });
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.snapshot.logs.push(format!("本地题库构建完成，当前收录 {total} 道题目"));
        service.publish(&mut runtime);
    }

    Ok(total)
}
