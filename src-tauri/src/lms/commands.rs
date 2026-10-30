use super::*;
fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内页面访问畅课数据".into());
    }
    Ok(())
}
/// 获取畅课快照（会话状态、待办与通知）。缓存过期时会自动拉取最新数据。
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
