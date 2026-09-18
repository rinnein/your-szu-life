use super::{electricity, model::NetworkFeePortalNotice, service, CampusPaymentsStore};
use tauri::{AppHandle, State};

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

#[tauri::command]
pub(crate) async fn get_electricity_options() -> Result<Vec<super::ElectricityCampus>, String> {
    tauri::async_runtime::spawn_blocking(electricity::options)
        .await
        .map_err(|_| "无法加载电费宿舍选项，请稍后重试。".to_owned())?
}

#[tauri::command]
pub(crate) async fn get_electricity_settings(
    store: State<'_, CampusPaymentsStore>,
) -> Result<Option<super::ElectricityLocationSettings>, String> {
    let electricity_store = store.electricity.clone();
    tauri::async_runtime::spawn_blocking(move || electricity::load_settings(&electricity_store))
        .await
        .map_err(|_| "无法读取电费宿舍配置，请稍后重试。".to_owned())?
}

#[tauri::command]
pub(crate) async fn save_electricity_settings(
    settings: super::ElectricityLocationSettings,
    store: State<'_, CampusPaymentsStore>,
) -> Result<(), String> {
    let electricity_store = store.electricity.clone();
    tauri::async_runtime::spawn_blocking(move || {
        electricity::save_settings(&electricity_store, settings)
    })
    .await
    .map_err(|_| "无法保存电费宿舍配置，请稍后重试。".to_owned())?
}

#[tauri::command]
pub(crate) async fn query_electricity(
    input: super::ElectricityQueryInput,
    app: AppHandle,
    store: State<'_, CampusPaymentsStore>,
) -> Result<super::ElectricityQueryView, String> {
    let electricity_store = store.electricity.clone();
    tauri::async_runtime::spawn_blocking(move || {
        electricity::query_and_record(input, &electricity_store, &app)
    })
    .await
    .map_err(|_| "电费查询任务未完成，请稍后重试。".to_owned())?
}

#[tauri::command]
pub(crate) async fn get_electricity_status(
    store: State<'_, CampusPaymentsStore>,
) -> Result<super::ElectricityStatusView, String> {
    let electricity_store = store.electricity.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(electricity::status(&electricity_store)))
        .await
        .map_err(|_| "无法读取当前电量状态，请稍后重试。".to_owned())?
}

#[tauri::command]
pub(crate) fn set_application_visibility(
    visible: bool,
    app: AppHandle,
    store: State<'_, CampusPaymentsStore>,
) {
    electricity::set_background(app, store.electricity.clone(), !visible);
}
