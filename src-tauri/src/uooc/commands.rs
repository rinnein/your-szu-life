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
