use super::*;
fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内页面访问畅课数据".into());
    }
    Ok(())
}
/// 获取畅课快照（会话状态、待办与通知）。有本地缓存时立即返回，
/// 过期数据在后台刷新并通过 `lms-state-changed` 事件回填。
#[tauri::command]
pub(crate) async fn get_lms_snapshot(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    local(&window)?;
    let service = service(&app)?;
    service.ensure(false).await;
    Ok(service.snapshot())
}
/// 订阅畅课状态（Initial + 后续变更），供首页月历等长期消费者使用。
#[tauri::command]
pub(crate) fn subscribe_lms_state(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    channel: tauri::ipc::Channel<LmsDeltaEvent>,
) -> Result<(), String> {
    local(&window)?;
    let service = service(&app)?;
    let _ = channel.send(LmsDeltaEvent::Initial(Box::new(service.snapshot())));
    service.state_subscribers.subscribe(channel);
    Ok(())
}
/// 强制刷新畅课数据（重新建立会话并拉取）。
#[tauri::command]
pub(crate) async fn refresh_lms(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    local(&window)?;
    let service = service(&app)?;
    service.ensure(true).await;
    Ok(service.snapshot())
}
