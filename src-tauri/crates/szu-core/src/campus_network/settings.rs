use super::{model::*, CampusNetworkStore};
use crate::auth::model::is_campus_card_number;

pub(super) fn dorm_settings(store: &CampusNetworkStore) -> Result<DormNetworkSettings, String> {
    // Serialize lazy loading with disabling: an older keyring read must never restore enabled=true.
    let mut cached = store.dorm_settings.lock().map_err(|_| "状态锁失败")?;
    if cached.is_none() {
        *cached = Some(store.credentials.load_dorm_settings()?.unwrap_or_default());
    }
    let mut settings = cached.as_ref().unwrap().clone();
    if store
        .auto_connect_suspended
        .load(std::sync::atomic::Ordering::Acquire)
    {
        settings.enabled = false;
    }
    Ok(settings)
}

/// Caller owns the logout operation. Failure keeps the runtime disabled and forbids a remote logout.
pub(super) fn disable_auto_connection(
    store: &CampusNetworkStore,
) -> Result<DormNetworkSettings, String> {
    store
        .auto_connect_suspended
        .store(true, std::sync::atomic::Ordering::Release);
    let mut cached = store.dorm_settings.lock().map_err(|_| "状态锁失败")?;
    let loaded = match cached.clone() {
        Some(settings) => Ok(settings),
        None => store
            .credentials
            .load_dorm_settings()
            .map(Option::unwrap_or_default),
    };
    let mut settings = match loaded {
        Ok(settings) => settings,
        Err(_) => {
            return Err("无法读取自动连接设置，已在本次运行中暂停自动连接，未发送注销请求".into());
        }
    };
    settings.enabled = false;
    *cached = Some(settings.clone());
    store
        .credentials
        .save_dorm_settings(&settings)
        .map_err(|_| "无法保存关闭设置，已在本次运行中暂停自动连接，未发送注销请求".to_owned())?;
    Ok(settings)
}

pub(super) fn dorm_settings_view(
    store: &CampusNetworkStore,
    settings: &DormNetworkSettings,
) -> Result<DormNetworkSettingsView, String> {
    let custom_credentials = store.credentials.load_dorm_credentials()?;
    let custom_campus_card_number = custom_credentials
        .as_ref()
        .filter(|value| is_campus_card_number(&value.campus_card_number))
        .map(|value| value.campus_card_number.clone());
    let custom_configured = custom_campus_card_number.is_some()
        && custom_credentials
            .as_ref()
            .is_some_and(|value| !value.password.is_empty());
    store
        .custom_configured
        .store(custom_configured, std::sync::atomic::Ordering::Relaxed);
    Ok(DormNetworkSettingsView {
        enabled: settings.enabled,
        credential_mode: settings.credential_mode.clone(),
        custom_configured,
        custom_campus_card_number,
    })
}

pub(super) fn dormant_credentials(
    app: &tauri::AppHandle,
    store: &CampusNetworkStore,
    settings: &DormNetworkSettings,
) -> Result<(String, String), String> {
    resolve_credentials(store, settings, || {
        crate::auth::current_network_credentials(app)
    })
}

pub(super) fn resolve_credentials(
    store: &CampusNetworkStore,
    settings: &DormNetworkSettings,
    current_account: impl FnOnce() -> Result<(String, String), String>,
) -> Result<(String, String), String> {
    let (card, password) = match settings.credential_mode {
        DormCredentialMode::CurrentAccount => current_account()?,
        DormCredentialMode::Custom => {
            let value = store
                .credentials
                .load_dorm_credentials()?
                .ok_or("尚未配置宿舍区校园网凭据")?;
            (value.campus_card_number, value.password)
        }
    };
    if !is_campus_card_number(&card) || password.is_empty() {
        return Err("校园卡号必须是 6 位数字且密码不能为空".into());
    }
    Ok((card, password))
}
