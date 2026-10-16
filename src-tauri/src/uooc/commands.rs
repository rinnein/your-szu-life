use super::{
    login,
    model::{Settings, Snapshot, TaskAction},
    service,
};
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
pub(crate) fn control_uooc_quiz(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    cid: String,
    revision: u64,
    action: super::quiz::Action,
) -> Result<Snapshot, String> {
    login::local_parent(&window)?;
    let service = service(&app)?;
    service.quiz_action(cid, revision, action)?;
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
pub(crate) fn get_uooc_embedding_status(
    app: tauri::AppHandle,
) -> Result<super::quiz::qa::EmbeddingModelStatus, String> {
    let service = service(&app)?;
    let cache_path = service.path.parent().unwrap().join("qa-vector-cache.json");
    Ok(super::quiz::qa::get_vector_status(&cache_path))
}

#[tauri::command]
pub(crate) async fn download_uooc_qa_vectors(
    app: tauri::AppHandle,
) -> Result<super::quiz::qa::EmbeddingModelStatus, String> {
    let service = service(&app)?;
    let cache_path = service.path.parent().unwrap().join("qa-vector-cache.json");
    let server_url = service.snapshot().settings.qa_server_url.clone();
    let api_key = service.qa_api_key().await;
    let cids: Vec<String> = service
        .snapshot()
        .courses
        .into_iter()
        .map(|c| c.id)
        .collect();
    super::quiz::qa::download_qa_vectors(&cache_path, &server_url, api_key.as_deref(), &cids).await
}

#[tauri::command]
pub(crate) async fn search_uooc_qa_questions(
    app: tauri::AppHandle,
    query: String,
) -> Result<Vec<super::quiz::qa::QaSearchResultItem>, String> {
    let service = service(&app)?;
    let cache_path = service.path.parent().unwrap().join("qa-vector-cache.json");
    let server_url = service.snapshot().settings.qa_server_url.clone();
    let api_key = service.qa_api_key().await;
    super::quiz::qa::search_questions_with_local_embedding(
        &cache_path,
        &server_url,
        api_key.as_deref(),
        &query,
        15,
    )
    .await
}

#[tauri::command]
pub(crate) async fn sync_uooc_qa_bank(app: tauri::AppHandle) -> Result<usize, String> {
    let service = service(&app)?;
    let bank_path = service.path.parent().unwrap().join("qa-bank.json");
    let server_url = service.snapshot().settings.qa_server_url.clone();
    let api_key = service.qa_api_key().await;
    super::quiz::qa::sync_missing_to_server(&bank_path, &server_url, api_key.as_deref(), None, None).await
}

#[tauri::command]
pub(crate) async fn get_uooc_qa_account_status(
    app: tauri::AppHandle,
) -> Result<super::quiz::qa::QaAccountStatus, String> {
    let service = service(&app)?;
    let server_url = service.snapshot().settings.qa_server_url.clone();
    let api_key = service.qa_api_key().await;
    super::quiz::qa::get_qa_account_status(&server_url, api_key.as_deref()).await
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


