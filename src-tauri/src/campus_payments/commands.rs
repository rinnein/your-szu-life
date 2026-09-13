use super::{model::NetworkFeePortalNotice, service, CampusPaymentsStore};
use tauri::State;

#[tauri::command]
pub(crate) async fn open_network_fee_portal(window: tauri::WebviewWindow) -> Result<(), String> {
    // WebView browsing-data APIs must not block a Windows UI-thread command.
    tauri::async_runtime::spawn_blocking(move || service::open(window))
        .await
        .map_err(|_| "无法启动网费服务，请稍后重试。".to_owned())?
}

#[tauri::command]
pub(crate) fn consume_network_fee_portal_notice(
    window: tauri::WebviewWindow,
    store: State<'_, CampusPaymentsStore>,
) -> Result<Option<NetworkFeePortalNotice>, String> {
    service::local_return_url(&window)?;
    Ok(store
        .runtime
        .lock()
        .map_err(|_| "网费服务状态不可用")?
        .notice
        .take())
}
