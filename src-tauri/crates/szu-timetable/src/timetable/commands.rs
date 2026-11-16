use tauri::{AppHandle, Manager};

use super::model::TimetableEvent;
use super::TimetableStore;

fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内页面访问课表数据".into());
    }
    Ok(())
}

#[tauri::command]
pub fn subscribe_timetable(
    window: tauri::WebviewWindow,
    app: AppHandle,
    channel: tauri::ipc::Channel<TimetableEvent>,
) -> Result<(), String> {
    local(&window)?;
    let store = app.state::<TimetableStore>();
    let _ = channel.send(TimetableEvent::Initial(Box::new(super::snapshot(&app))));
    store.subscribers.subscribe(channel);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        super::refresh(&app, false).await;
        super::scheduler::evaluate(&app).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn refresh_timetable(
    window: tauri::WebviewWindow,
    app: AppHandle,
    force_semester: Option<bool>,
) -> Result<(), String> {
    local(&window)?;
    super::refresh(&app, true).await;
    super::scheduler::refresh_semester(&app, force_semester.unwrap_or(false)).await;
    super::scheduler::evaluate(&app).await;
    Ok(())
}
