use super::{
    login,
    model::{Settings, Snapshot, TaskAction},
    service,
};
#[tauri::command]
pub(crate) fn subscribe_uooc_state(
    app: tauri::AppHandle,
    channel: tauri::ipc::Channel<super::UoocDeltaEvent>,
) -> Result<(), String> {
    let service = service(&app)?;
    let snapshot = {
        let runtime = service.runtime.lock().unwrap();
        super::Service::visible_snapshot(&runtime)
    };
    let _ = channel.send(super::UoocDeltaEvent::Initial(Box::new(snapshot)));
    service.state_subscribers.subscribe(channel);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_uooc_ai_config(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<super::ai::ConfigView, String> {
    login::local_app_parent(&window)?;
    Ok(service(&app)?.ai.lock().unwrap().view())
}
#[tauri::command]
pub(crate) fn get_uooc_ai_connections(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<super::ai::ConnectionsView, String> {
    login::local_app_parent(&window)?;
    Ok(service(&app)?.ai.lock().unwrap().connections())
}
#[tauri::command]
pub(crate) fn save_uooc_ai_connections(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    connections: super::ai::ConnectionsInput,
) -> Result<super::ai::ConnectionsView, String> {
    login::local_app_parent(&window)?;
    let service = service(&app)?;
    let mut old = service.ai.lock().unwrap();
    let next = old.update_connections(connections)?;
    service
        .credentials
        .save_uooc_ai(&serde_json::to_string(&next).map_err(|_| "无法序列化 AI 配置")?)
        .map_err(|_| "AI 配置无法写入系统密钥存储")?;
    *old = next;
    Ok(old.connections())
}
#[tauri::command]
pub(crate) fn save_uooc_ai_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    preferences: super::ai::PreferencesInput,
) -> Result<super::ai::ConfigView, String> {
    login::local_app_parent(&window)?;
    let service = service(&app)?;
    let mut old = service.ai.lock().unwrap();
    let next = old.update_preferences(preferences)?;
    service
        .credentials
        .save_uooc_ai(&serde_json::to_string(&next).map_err(|_| "无法序列化 AI 配置")?)
        .map_err(|_| "AI 配置无法写入系统密钥存储")?;
    *old = next;
    Ok(old.view())
}
#[tauri::command]
pub(crate) async fn list_uooc_ai_models(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    provider: super::ai::ProviderKind,
) -> Result<Vec<String>, String> {
    login::local_app_parent(&window)?;
    let service = service(&app)?;
    let config = service.ai.lock().unwrap().clone();
    super::ai::models(
        &config,
        provider,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
}
#[tauri::command]
pub(crate) fn save_uooc_quiz_mode(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    cid: String,
    mode: super::quiz::model::Mode,
) -> Result<Snapshot, String> {
    login::local_parent(&window)?;
    let service = service(&app)?;
    service.quiz_policy(cid, mode)?;
    Ok(service.snapshot())
}
#[tauri::command]
pub(crate) fn save_uooc_quiz_answers(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    cid: String,
    revision: u64,
    answers: super::quiz::model::Answers,
) -> Result<Snapshot, String> {
    login::local_parent(&window)?;
    let service = service(&app)?;
    service.quiz_edit(cid, revision, answers)?;
    Ok(service.snapshot())
}
#[tauri::command]
pub(crate) async fn control_uooc_quiz(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    cid: String,
    revision: u64,
    action: super::quiz::Action,
) -> Result<Snapshot, String> {
    login::local_parent(&window)?;
    let service = service(&app)?;
    service.quiz_action(cid, revision, action).await?;
    Ok(service.snapshot())
}
#[tauri::command]
pub(crate) async fn open_uooc_quiz_portal(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    cid: String,
) -> Result<(), String> {
    service(&app)?.quiz_portal(window, cid).await
}

#[tauri::command]
pub(crate) fn get_uooc_snapshot(app: tauri::AppHandle) -> Result<Snapshot, String> {
    Ok(service(&app)?.snapshot())
}

#[tauri::command]
pub(crate) async fn refresh_uooc_session(app: tauri::AppHandle) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service.refresh().await?;
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) async fn open_uooc_login(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    login::open(service(&app)?, window).await
}

#[tauri::command]
pub(crate) async fn logout_uooc(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    login::logout(service(&app)?, window).await
}

#[tauri::command]
pub(crate) fn save_uooc_settings(
    app: tauri::AppHandle,
    settings: Settings,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service.set_settings(settings)?;
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) fn start_uooc_courses(
    app: tauri::AppHandle,
    course_ids: Vec<String>,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service.start(course_ids)?;
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) fn control_uooc_tasks(
    app: tauri::AppHandle,
    course_ids: Vec<String>,
    action: TaskAction,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service.control(course_ids, action)?;
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) async fn refresh_uooc_progress(
    app: tauri::AppHandle,
    course_ids: Vec<String>,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    let ids = service
        .snapshot()
        .courses
        .into_iter()
        .filter(|c| course_ids.is_empty() || course_ids.contains(&c.id))
        .map(|c| c.id)
        .collect();
    service.refresh_progress(ids).await;
    Ok(service.snapshot())
}
#[tauri::command]
pub(crate) fn start_uooc_checkin(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    course_ids: Vec<String>,
) -> Result<Snapshot, String> {
    login::local_parent(&window)?;
    let service = service(&app)?;
    service.enqueue_checkins(course_ids, super::checkin::CheckinSource::Manual)?;
    Ok(service.snapshot())
}
#[tauri::command]
pub(crate) fn cancel_uooc_checkin(
    app: tauri::AppHandle,
    course_ids: Vec<String>,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service.cancel_checkins(course_ids);
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) async fn start_uooc_discussion(
    app: tauri::AppHandle,
    course_id: String,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service.start_discussion_once(&course_id).await?;
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) async fn get_uooc_embedding_status(
    app: tauri::AppHandle,
) -> Result<super::quiz::qa::EmbeddingModelStatus, String> {
    let service = service(&app)?;
    let qa_db = service.get_qa_db().await?;
    Ok(super::quiz::qa::get_vector_status(qa_db).await)
}

#[tauri::command]
pub(crate) async fn download_uooc_qa_vectors(
    app: tauri::AppHandle,
    server_url: Option<String>,
) -> Result<super::quiz::qa::EmbeddingModelStatus, String> {
    let service = service(&app)?;
    let qa_db = service.get_qa_db().await?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    let api_key = service.qa_api_key_for_server(Some(&url)).await;
    let cids: Vec<String> = service
        .snapshot()
        .courses
        .into_iter()
        .map(|c| c.id)
        .collect();
    super::quiz::qa::download_qa_vectors(qa_db, &url, api_key.as_deref(), &cids).await
}

#[tauri::command]
pub(crate) async fn search_uooc_qa_questions(
    app: tauri::AppHandle,
    query: String,
    server_url: Option<String>,
) -> Result<Vec<super::quiz::qa::QaSearchResultItem>, String> {
    let service = service(&app)?;
    let qa_db = service.get_qa_db().await?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    let api_key = service.qa_api_key_for_server(Some(&url)).await;
    super::quiz::qa::search_questions_with_local_embedding(
        qa_db,
        &url,
        api_key.as_deref(),
        &query,
        20,
    )
    .await
}

#[tauri::command]
pub(crate) async fn list_uooc_exams(
    app: tauri::AppHandle,
    cid: Option<String>,
) -> Result<Vec<super::model::ExamTask>, String> {
    let service = service(&app)?;
    service.list_exams(cid.as_deref()).await
}

#[tauri::command]
pub(crate) async fn get_uooc_exam_paper(
    app: tauri::AppHandle,
    cid: String,
    task_id: String,
) -> Result<super::model::ExamPaper, String> {
    let service = service(&app)?;
    service.get_exam_paper(&cid, &task_id).await
}

#[tauri::command]
pub(crate) async fn save_uooc_exam_draft(
    app: tauri::AppHandle,
    cid: String,
    task_id: String,
    answers: super::quiz::model::Answers,
) -> Result<(), String> {
    let service = service(&app)?;
    service.save_exam_draft(&cid, &task_id, answers).await
}

#[tauri::command]
pub(crate) async fn submit_uooc_exam(
    app: tauri::AppHandle,
    cid: String,
    task_id: String,
    answers: super::quiz::model::Answers,
    questions: Option<Vec<super::quiz::model::Question>>,
) -> Result<String, String> {
    let service = service(&app)?;
    service.submit_exam(&cid, &task_id, answers, questions).await
}

#[tauri::command]
pub(crate) async fn heartbeat_uooc_exam(
    app: tauri::AppHandle,
    cid: String,
    task_id: String,
) -> Result<bool, String> {
    let service = service(&app)?;
    service.heartbeat_exam(&cid, &task_id).await
}

#[tauri::command]
pub(crate) async fn sync_uooc_qa_bank(
    app: tauri::AppHandle,
    server_url: Option<String>,
) -> Result<usize, String> {
    let service = service(&app)?;
    let qa_db = service.get_qa_db().await?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    let api_key = service.qa_api_key_for_server(Some(&url)).await;
    super::quiz::qa::sync_missing_to_server(qa_db, &url, api_key.as_deref(), None, None, Some(&service)).await
}

#[tauri::command]
pub(crate) async fn get_uooc_qa_account_status(
    app: tauri::AppHandle,
    server_url: Option<String>,
) -> Result<super::quiz::qa::QaAccountStatus, String> {
    let service = service(&app)?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    let api_key = service.qa_api_key_for_server(Some(&url)).await;
    super::quiz::qa::get_qa_account_status(&url, api_key.as_deref()).await
}

#[tauri::command]
pub(crate) fn save_uooc_qa_api_key(app: tauri::AppHandle, api_key: String) -> Result<(), String> {
    let service = service(&app)?;
    service.save_qa_api_key(api_key)
}

#[tauri::command]
pub(crate) fn reset_uooc_qa_api_key(app: tauri::AppHandle) -> Result<(), String> {
    let service = service(&app)?;
    service.reset_qa_api_key()
}

#[tauri::command]
pub(crate) async fn start_uooc_qa_device_auth(
    app: tauri::AppHandle,
    server_url: Option<String>,
) -> Result<super::quiz::qa::QaDeviceCodeResponse, String> {
    let service = service(&app)?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    super::quiz::qa::request_device_code(&url).await
}

#[tauri::command]
pub(crate) async fn poll_uooc_qa_device_auth(
    app: tauri::AppHandle,
    device_code: String,
    server_url: Option<String>,
) -> Result<super::quiz::qa::QaDeviceTokenPollResult, String> {
    let service = service(&app)?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    let res = super::quiz::qa::poll_device_token(&url, &device_code).await?;
    if res.status == "success" {
        if let Some(key) = &res.api_key {
            let _ = service.save_qa_api_key(key.clone());
        }
    }
    Ok(res)
}

#[tauri::command]
pub(crate) async fn upload_all_uooc_qa_bank(
    app: tauri::AppHandle,
    server_url: Option<String>,
) -> Result<usize, String> {
    let service = service(&app)?;
    let qa_db = service.get_qa_db().await?;
    let url = server_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
    let api_key = service.qa_api_key_for_server(Some(&url)).await;
    super::quiz::qa::upload_all_local_bank(qa_db, &url, api_key.as_deref(), Some(&service)).await
}

#[tauri::command]
pub(crate) async fn fetch_uooc_completed_quiz(
    app: tauri::AppHandle,
    cid: String,
    task_id: Option<String>,
    resource_id: Option<String>,
) -> Result<Snapshot, String> {
    let service = service(&app)?;
    service
        .fetch_completed_quiz(&cid, task_id.as_deref(), resource_id.as_deref())
        .await?;
    Ok(service.snapshot())
}

#[tauri::command]
pub(crate) async fn get_uooc_local_bank_questions(
    app: tauri::AppHandle,
) -> Result<Vec<super::quiz::qa::LocalBankQuestion>, String> {
    let service = service(&app)?;
    let bank = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.load_all_bank_questions().await.unwrap_or_default()
    } else {
        Default::default()
    };
    let courses = {
        let r = service.runtime.lock().unwrap();
        r.snapshot.courses.clone()
    };
    let mut list: Vec<_> = bank.into_values().collect();
    for q in &mut list {
        if q.course_name.is_none() {
            if let Some(c) = courses.iter().find(|c| c.id == q.cid) {
                q.course_name = Some(c.title.clone());
            }
        }
    }
    list.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    Ok(list)
}

#[tauri::command]
pub(crate) async fn list_uooc_quizzes(
    app: tauri::AppHandle,
    cid: Option<String>,
) -> Result<Vec<super::model::HomeworkTask>, String> {
    let service = service(&app)?;
    service.list_quizzes(cid.as_deref()).await
}

#[tauri::command]
pub(crate) async fn list_uooc_homeworks(
    app: tauri::AppHandle,
    cid: Option<String>,
) -> Result<Vec<super::model::HomeworkTask>, String> {
    let service = service(&app)?;
    service.list_homeworks(cid.as_deref()).await
}

#[tauri::command]
pub(crate) async fn get_uooc_eval_list(
    app: tauri::AppHandle,
    cid: Option<String>,
) -> Result<Vec<super::model::PeerEvalTask>, String> {
    let service = service(&app)?;
    service.list_evals(cid.as_deref()).await
}

#[tauri::command]
pub(crate) async fn submit_uooc_eval(
    app: tauri::AppHandle,
    cid: String,
    tid: String,
    eval_id: String,
    score: f64,
    comments: String,
) -> Result<String, String> {
    let service = service(&app)?;
    service.submit_eval(&cid, &tid, &eval_id, score, &comments).await
}

#[tauri::command]
pub(crate) async fn generate_uooc_subjective(
    app: tauri::AppHandle,
    stem: Option<String>,
    question: Option<super::quiz::model::Question>,
    context: Option<String>,
    reference_examples: Option<Vec<super::ai::ReferenceExample>>,
) -> Result<String, String> {
    let service = service(&app)?;
    let actual_stem = if let Some(s) = stem.filter(|s| !s.trim().is_empty()) {
        s
    } else if let Some(q) = question {
        q.text
    } else {
        return Err("缺少题目题干 (stem 或 question)".to_string());
    };
    service
        .generate_subjective(&actual_stem, context.as_deref(), reference_examples.unwrap_or_default())
        .await
}

#[tauri::command]
pub(crate) async fn open_uooc_task_portal(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    task_id: String,
) -> Result<(), String> {
    let service = service(&app)?;
    service.task_portal(window, task_id).await
}

#[tauri::command]
pub(crate) async fn match_uooc_paper_qa(
    app: tauri::AppHandle,
    questions: Vec<super::quiz::model::Question>,
) -> Result<(super::quiz::model::Answers, Vec<super::quiz::model::Review>), String> {
    let service = service(&app)?;
    service.match_paper_qa(questions).await
}

#[tauri::command]
pub(crate) async fn solve_uooc_paper_ai(
    app: tauri::AppHandle,
    questions: Vec<super::quiz::model::Question>,
    on_event: tauri::ipc::Channel<super::quiz::model::AiSessionEntry>,
) -> Result<(super::quiz::model::Answers, Vec<super::quiz::model::Review>), String> {
    let service = service(&app)?;
    service.solve_paper_ai_with_channel(questions, Some(on_event)).await
}

#[tauri::command]
pub(crate) async fn get_uooc_course_title(
    app: tauri::AppHandle,
    cid: String,
) -> Result<String, String> {
    let service = service(&app)?;
    // 1. From snapshot courses
    {
        let r = service.runtime.lock().unwrap();
        if let Some(c) = r.snapshot.courses.iter().find(|c| c.id == cid) {
            return Ok(c.title.clone());
        }
    }
    // 2. Query via UOOC client
    let client = service.client();
    let cancel = tokio_util::sync::CancellationToken::new();
    if let Ok(courses) = client.courses(&cancel).await {
        if let Some(c) = courses.iter().find(|c| c.id == cid) {
            return Ok(c.title.clone());
        }
    }
    Ok(format!("课程 {cid}"))
}

#[tauri::command]
pub(crate) async fn build_uooc_local_bank(
    app: tauri::AppHandle,
    on_progress: tauri::ipc::Channel<super::model::BankRebuildProgress>,
) -> Result<usize, String> {
    let service = service(&app)?;
    service.build_local_bank_with_channel(Some(on_progress)).await
}

#[tauri::command]
pub(crate) async fn clear_uooc_local_bank(app: tauri::AppHandle) -> Result<usize, String> {
    let service = service(&app)?;
    let count = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.clear_all_questions().await.unwrap_or(0)
    } else {
        0
    };
    if let Some(parent) = service.path.parent() {
        crate::migrations::uooc::cleanup_legacy_uooc_files(parent);
    }
    service.log_debug("bank", format!("已清空本地题库（共 {count} 道题目）"));
    Ok(count)
}

#[tauri::command]
pub(crate) async fn uooc_qa_http_request(
    request: super::quiz::qa::QaHttpRequest,
) -> Result<super::quiz::qa::QaHttpResponse, String> {
    super::quiz::qa::execute_qa_http_request(request).await
}



