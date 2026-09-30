use super::{
    login,
    model::{Settings, Snapshot, TaskAction},
    service,
};

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
    service.enqueue_checkins(course_ids, true, window.label().to_owned())?;
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
