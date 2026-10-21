use super::{
    login::start_dorm_login,
    logs::load_dorm_logs,
    model::*,
    monitor::start_dorm_monitor,
    now_ms,
    settings::{dorm_settings, dorm_settings_view},
    state::set_dorm_status,
    CampusNetworkStore,
};
use crate::auth::model::is_campus_card_number;
use tauri::{Emitter, Manager};

pub(crate) fn get_network_overview(
    store: &CampusNetworkStore,
) -> Result<NetworkOverviewView, String> {
    Ok(store
        .network_overview
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone())
}

pub(crate) fn refresh_campus_network(app: tauri::AppHandle) -> Result<NetworkOverviewView, String> {
    super::monitor::refresh_now(app)
}

pub(super) fn get_dorm_network_settings(
    app: tauri::AppHandle,
    store: &CampusNetworkStore,
) -> Result<DormNetworkSettingsView, String> {
    let settings = dorm_settings(&store)?;
    let view = dorm_settings_view(&store, &settings)?;
    start_dorm_monitor(app);
    Ok(view)
}

pub(super) fn save_dorm_network_settings(
    app: tauri::AppHandle,
    input: DormNetworkSettingsInput,
    store: &CampusNetworkStore,
) -> Result<DormNetworkSettingsView, String> {
    store.operations.settings(|| {
        if input.enabled && input.credential_mode == DormCredentialMode::CurrentAccount {
            let (card, password) = crate::auth::current_network_credentials(&app)?;
            if !is_campus_card_number(&card) || password.is_empty() {
                return Err("当前账户尚未保存完整校园网凭据".into());
            }
        }
        if input.credential_mode == DormCredentialMode::Custom {
            let existing = store.credentials.load_dorm_credentials()?;
            let card = input
                .campus_card_number
                .or_else(|| existing.as_ref().map(|v| v.campus_card_number.clone()))
                .unwrap_or_default();
            let password = input
                .password
                .or_else(|| existing.as_ref().map(|v| v.password.clone()))
                .unwrap_or_default();
            if !is_campus_card_number(&card) || password.is_empty() {
                return Err("校园卡号必须是 6 位数字且密码不能为空".into());
            }
            store
                .credentials
                .save_dorm_credentials(&DormCustomCredentials {
                    campus_card_number: card,
                    password,
                })?;
        }
        let settings = DormNetworkSettings {
            enabled: input.enabled,
            credential_mode: input.credential_mode,
            pending_failure_notice: dorm_settings(&store)?.pending_failure_notice,
        };
        store.credentials.save_dorm_settings(&settings)?;
        *store.dorm_settings.lock().map_err(|_| "状态锁失败")? = Some(settings.clone());
        store
            .auto_connect_suspended
            .store(false, std::sync::atomic::Ordering::Release);
        set_dorm_status(&app, |status| {
            status.state = if settings.enabled {
                DormLoginState::Monitoring
            } else {
                DormLoginState::Disabled
            };
            status.next_probe_at_ms = if settings.enabled {
                Some(now_ms())
            } else {
                None
            };
        });
        start_dorm_monitor(app.clone());
        let view = dorm_settings_view(&store, &settings)?;
        let _ = app.emit("dorm-network-settings-changed", &view);
        Ok(view)
    })
}

pub(crate) fn get_dorm_login_status(
    store: &CampusNetworkStore,
) -> Result<DormLoginStatusView, String> {
    Ok(store.dorm_status.lock().map_err(|_| "状态锁失败")?.clone())
}

pub(crate) fn trigger_dorm_login(app: tauri::AppHandle) -> Result<(), String> {
    start_dorm_login(app, true)
}

pub(crate) fn get_dorm_login_logs(
    app: tauri::AppHandle,
    store: &CampusNetworkStore,
) -> Result<Vec<DormLoginLogView>, String> {
    let mut logs = load_dorm_logs(&app);
    logs.reverse();
    if let Ok(mut cached) = store.dorm_logs.lock() {
        *cached = logs.iter().cloned().collect();
    }
    Ok(logs)
}

pub(super) fn consume_dorm_login_failure_notice(
    app: tauri::AppHandle,
    store: &CampusNetworkStore,
) -> Result<Option<DormFailureNotice>, String> {
    store.operations.settings(|| {
        let mut settings = dorm_settings(&store)?;
        let notice = settings.pending_failure_notice.take();
        store.credentials.save_dorm_settings(&settings)?;
        *store.dorm_settings.lock().map_err(|_| "状态锁失败")? = Some(settings);
        let _ = app;
        Ok(notice)
    })
}

pub(crate) fn logout_campus_network(
    app: tauri::AppHandle,
) -> Result<NetworkLogoutResultView, String> {
    use super::{
        http::{PortalClient, UnavailablePortal},
        logout::perform_network_logout,
        network::SystemNetworkEnvironment,
        state::AppNetworkEvents,
    };
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    let client = PortalClient::new();
    let result = match client.as_ref() {
        Ok(client) => perform_network_logout(
            &store,
            client,
            &SystemNetworkEnvironment,
            &AppNetworkEvents(&app),
        ),
        Err(_) => perform_network_logout(
            &store,
            &UnavailablePortal,
            &SystemNetworkEnvironment,
            &AppNetworkEvents(&app),
        ),
    };
    let (outcome, region, message) = match &result {
        Ok(view) => (
            match view.outcome {
                NetworkLogoutOutcome::Disconnected => "disconnected",
                NetworkLogoutOutcome::AlreadyOffline => "already-offline",
                NetworkLogoutOutcome::Failed => "failed",
                NetworkLogoutOutcome::Unknown => "unknown",
            },
            Some(view.region.clone()),
            view.message.clone(),
        ),
        Err(_) => (
            "failed",
            None,
            "注销操作未完成，请查看当前设置与网络状态".into(),
        ),
    };
    super::logs::append_dorm_log(
        &app,
        DormLoginLogView {
            timestamp_ms: now_ms(),
            stage: "logout".into(),
            outcome: outcome.into(),
            region,
            message: Some(message),
            ..Default::default()
        },
    );
    result
}
