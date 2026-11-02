use super::PortNotice;

/// 取走启动阶段积压的端口通知，并告知后端前端监听器已就绪。
///
/// 前端必须先注册 `app://port-notice` 监听器再调用本命令，保证后端在就绪后
/// 发出的通知不会落在监听器注册之前。
#[tauri::command]
pub(crate) fn take_port_notices() -> Vec<PortNotice> {
    super::take_pending_notices()
}
