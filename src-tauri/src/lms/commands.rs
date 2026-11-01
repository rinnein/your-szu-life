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
