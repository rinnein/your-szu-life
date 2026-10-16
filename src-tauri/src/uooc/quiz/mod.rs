pub(crate) mod model;
pub(crate) mod qa;
mod protocol;
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
use std::{collections::BTreeMap, sync::Arc};
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
        #[cfg(target_os = "linux")]
        {
            use tauri::Emitter;
            let app = app.clone();
            let cid = ctx.cid.clone();
            std::thread::spawn(move || {
                if let Ok(handle) = notify_rust::Notification::new()
                    .summary("UOOC 测验需要处理")
                    .body(&body)
                    .action("default", "打开测验")
                    .show()
                {
                    handle.wait_for_action(|action| {
                        if action == "default" {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                            let _ = app.emit("uooc-quiz-open", &cid);
                        }
                    });
                }
            });
        }
        #[cfg(not(target_os = "linux"))]
        {
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
    };
    let rows = client.unit(&context, cancel).await?;
    if let Some(item) = rows
        .into_iter()
        .find(|v| id(&v["id"]).as_deref() == Some(rid.as_str()) && number(&v["type"]) == 80.0)
    {
        return Ok(Some((context, item)));
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
        if !generate(&service, &ctx, &cancel).await {
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
async fn generate(service: &Arc<Service>, ctx: &Context, cancel: &CancellationToken) -> bool {
    update(service, ctx, |q| {
        q.stage = Stage::Generating;
        q.message = "正在查询题库并生成答案草稿…".into();
    });
    let mut q = state(service, ctx);

    if q.answers.is_empty() {
        let qa_url = service.snapshot().settings.qa_server_url.clone();
        let qa_key = service.qa_api_key().await;
        if let Ok(qa_qs) =
            qa::query_task_answers(&qa_url, qa_key.as_deref(), &q.task_id, Some(&ctx.cid)).await
        {
            let matched = qa::match_answers(&qa_qs, &q.questions);
            if !matched.matched_answers.is_empty() || !matched.reviews.is_empty() {
                update(service, ctx, |state| {
                    state.answers.extend(matched.matched_answers);
                    for r in matched.reviews {
                        if let Some(existing) =
                            state.reviews.iter_mut().find(|ex| ex.question_id == r.question_id)
                        {
                            *existing = r;
                        } else {
                            state.reviews.push(r);
                        }
                    }
                });
                q = state(service, ctx);
            }
        }
    }

    let needed: Vec<_> = q
        .questions
        .iter()
        .filter(|item| !q.answers.get(&item.id).is_some_and(|a| !a.is_empty()))
        .cloned()
        .collect();

    let (ai_answers, ai_reviews, err) = if needed.is_empty() {
        (Answers::new(), vec![], None)
    } else {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return false,
            r = ai::generate(service, &needed, cancel) => r,
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
        q.stage = if automatic {
            Stage::Preparing
        } else {
            Stage::AwaitingApproval
        };
        q.message = err.unwrap_or_else(|| {
            if automatic {
                "答案已准备完毕，准备核对并提交".into()
            } else {
                "请检查、修改并确认整份测验答案".into()
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
        let qa_url = service.snapshot().settings.qa_server_url.clone();
        let qa_key = service.qa_api_key().await;
        let course_name = service
            .snapshot()
            .courses
            .iter()
            .find(|c| c.id == ctx.cid)
            .map(|c| c.title.clone());
        let payload = qa::build_failed_payload(
            &ctx.cid,
            course_name.as_deref(),
            &q.task_id,
            &q.title,
            &q.questions,
            &q.answers,
        );
        let _ = qa::upload_batch(&qa_url, qa_key.as_deref(), &payload).await;
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
            let qa_url = service.snapshot().settings.qa_server_url.clone();
            let qa_key = service.qa_api_key().await;
            let course_name = service
                .snapshot()
                .courses
                .iter()
                .find(|c| c.id == ctx.cid)
                .map(|c| c.title.clone());
            let payload = qa::build_passed_payload(
                &ctx.cid,
                course_name.as_deref(),
                &q.task_id,
                &q.title,
                &q.questions,
                &q.answers,
            );
            let _ = qa::upload_batch(&qa_url, qa_key.as_deref(), &payload).await;

            let bank_path = service.path.parent().unwrap().join("qa-bank.json");
            let _ = qa::record_passed_quiz(&bank_path, &ctx.cid, &q.task_id, &q.questions, &q.answers);
            let _ = qa::sync_missing_to_server(
                &bank_path,
                &qa_url,
                qa_key.as_deref(),
                Some(&ctx.cid),
                Some(&q.task_id),
            )
            .await;

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
    let bank_path = service.path.parent().unwrap().join("qa-bank.json");

    // 自动比对后端是否存在缺失题目，并将后端缺失的题目上传
    let _ = qa::sync_missing_to_server(
        &bank_path,
        &qa_url,
        qa_key.as_deref(),
        Some(&ctx.cid),
        Some(&q.task_id),
    )
    .await;

    let res = tokio::select! {
        biased;
        _ = cancel.cancelled() => return,
        r = qa::query_task_answers(&qa_url, qa_key.as_deref(), &q.task_id, Some(&ctx.cid)) => r,
    };
    if !current(service, ctx) {
        return;
    }

    // 先检查本地题库是否有确认答案
    let local_bank = qa::load_local_bank(&bank_path);
    let mut local_hits = 0;
    for question in &q.questions {
        let key = qa::bank_key(&q.task_id, &question.id);
        if let Some(entry) = local_bank.get(&key) {
            if !entry.correct_answers.is_empty() {
                local_hits += 1;
                update(service, ctx, |state| {
                    state.answers.insert(question.id.clone(), entry.correct_answers.clone());
                    state.reviews.push(Review {
                        question_id: question.id.clone(),
                        answers: entry.correct_answers.clone(),
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
                    format!("已从题库匹配 {count} 道题目的标准答案")
                } else {
                    "题库中暂未收录该测验的标准答案".into()
                };
            });
        }
        Err(e) => {
            if local_hits > 0 {
                update(service, ctx, |q| {
                    q.stage = Stage::AwaitingApproval;
                    q.message = format!("远程题库连接失败，但已从本地题库命中 {local_hits} 道题目答案");
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
    pub(super) fn quiz_action(
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
            let task = r
                .snapshot
                .tasks
                .iter_mut()
                .find(|t| t.account_id == account && t.course.id == cid)
                .ok_or("请先在任务页启动课程")?;
            task.generation += 1;
            task.status = TaskStatus::Running;
            task.desired_running = true;
            let generation = task.generation;
            let cancel = CancellationToken::new();
            r.workers.insert(cid.clone(), (generation, cancel.clone()));
            self.publish(&mut r);
            (generation, cancel)
        };
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let client = service.client();
            let result = async {
                service.sync_platform().map_err(Error::Quiz)?;
                let ctx = context(&service, &cid, generation)?;
                if !allowed(&service, &cid, &cancel).await? {
                    return Err(Error::Quiz("测验权重为零或尚未确认，答题已禁用".into()));
                }
                match action {
                    Action::Generate => {
                        let q = state(&service, &ctx);
                        let p = paper(&client, &q.task_id, &cancel).await?;
                        update(&service, &ctx, |q| {
                            q.signature = p.signature;
                            q.questions = p.questions;
                        });
                        generate(&service, &ctx, &cancel).await;
                        if state(&service, &ctx).stage == Stage::Preparing {
                            update(&service, &ctx, |q| {
                                q.stage = Stage::AwaitingApproval;
                                q.message = "新答案已生成，请确认后提交".into();
                            });
                        }
                        Ok(TaskStatus::AwaitingDecision)
                    }
                    Action::MatchQa => {
                        let q = state(&service, &ctx);
                        let p = paper(&client, &q.task_id, &cancel).await?;
                        update(&service, &ctx, |q| {
                            q.signature = p.signature;
                            q.questions = p.questions;
                        });
                        match_qa_action(&service, &ctx, &cancel).await;
                        Ok(TaskStatus::AwaitingDecision)
                    }
                    Action::Submit => submit(&service, &client, &ctx, &cancel).await,
                    Action::Check => Ok(TaskStatus::Queued),
                }
            }
            .await;
            service.finish(&cid, generation, result.clone(), &client);
            if result == Ok(TaskStatus::Queued) {
                service.reconcile();
            }
        });
        Ok(())
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
        if !allowed(self, &cid, &CancellationToken::new())
            .await
            .map_err(|e| e.message())?
        {
            return Err("测验权重为零或未知，答题已禁用".into());
        }
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
            let _ = service.quiz_action(cid, revision, Action::Check);
        }
    }
}
