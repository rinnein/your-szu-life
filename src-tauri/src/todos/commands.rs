use tauri::{AppHandle, Manager};

use super::aggregate;
use super::model::TodosEvent;
use super::TodosStore;

fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内页面访问待办数据".into());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn subscribe_todos(
    window: tauri::WebviewWindow,
    app: AppHandle,
    channel: tauri::ipc::Channel<TodosEvent>,
) -> Result<(), String> {
    local(&window)?;
    let store = app.state::<TodosStore>();
    let items = aggregate::aggregate(&app);
    let _ = channel.send(TodosEvent::Initial(Box::new(items)));
    store.subscribers.subscribe(channel);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        super::refresh_sources(&app, false).await;
        super::reminder::tick(&app).await;
    });
    Ok(())
}

#[tauri::command]
pub(crate) async fn refresh_todos(
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<(), String> {
    local(&window)?;
    super::refresh_sources(&app, true).await;
    super::reminder::tick(&app).await;
    Ok(())
}
