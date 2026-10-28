use super::{
    model::{ProxyProbeView, ProxySettingsInput, ProxySettingsView},
    ProxyStore,
};
use tauri::State;

#[tauri::command]
pub(crate) fn get_proxy_settings(
    store: State<'_, ProxyStore>,
) -> Result<ProxySettingsView, String> {
    super::model::view(store.credentials.as_ref())
}

#[tauri::command]
pub(crate) fn save_proxy_settings(
    app: tauri::AppHandle,
    settings: ProxySettingsInput,
    store: State<'_, ProxyStore>,
) -> Result<ProxySettingsView, String> {
    super::save(store.credentials.as_ref(), &app, settings)
}

/// 用**待保存**的设置试连一次，好让用户在落盘前就知道代理是否可用。
#[tauri::command]
pub(crate) async fn test_proxy_settings(
    settings: ProxySettingsInput,
    url: Option<String>,
    store: State<'_, ProxyStore>,
) -> Result<ProxyProbeView, String> {
    let credentials = store.credentials.clone();
    tauri::async_runtime::spawn_blocking(move || {
        super::test(credentials.as_ref(), settings, url.as_deref())
    })
    .await
    .map_err(|_| "代理连通性测试任务异常结束".to_owned())?
}