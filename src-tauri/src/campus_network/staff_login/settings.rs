use super::{protocol::credential_stamp, CredentialsProvider};
use crate::{
    auth::model::is_campus_card_number,
    campus_network::{model::*, state::NetworkEvents, CampusNetworkStore},
};
use std::sync::atomic::Ordering;

pub(crate) fn load(store: &CampusNetworkStore) -> Result<StaffNetworkSettings, String> {
    let mut cached = store
        .staff
        .settings
        .lock()
        .map_err(|_| "教工区设置锁失败")?;
    if cached.is_none() {
        *cached = Some(
            store
                .credentials
                .load_staff_settings()
                .map_err(|_| "无法读取教工区设置")?
                .unwrap_or_default(),
        );
    }
    let mut settings = cached.as_ref().unwrap().clone();
    if settings.mode == StaffLoginMode::MacAuth {
        settings.enabled = false;
    }
    Ok(settings)
}
pub(super) fn credentials(
    store: &CampusNetworkStore,
    settings: &StaffNetworkSettings,
    current: &CredentialsProvider<'_>,
) -> Result<(String, String), String> {
    let (card, password) = match settings.credential_mode {
        DormCredentialMode::CurrentAccount => {
            current().map_err(|_| "当前应用账户未保存可用的校园网凭据")?
        }
        DormCredentialMode::Custom => {
            let value = store
                .credentials
                .load_staff_credentials()
                .map_err(|_| "无法读取教工区凭据")?
                .ok_or("尚未配置教工区凭据")?;
            (value.campus_card_number, value.password)
        }
    };
    if !is_campus_card_number(&card) || password.is_empty() || password.len() > 1024 {
        return Err("校园卡号须为 6 位数字，密码不能为空且不得超过 1024 字节".into());
    }
    Ok((card, password))
}
pub(super) fn applied(settings: &StaffNetworkSettings, stamp: &str) -> bool {
    settings.applied_revision == Some(settings.revision)
        && settings.applied_mode == Some(settings.mode)
        && settings.applied_credentials.as_deref() == Some(stamp)
}
pub(crate) fn cached_view(
    store: &CampusNetworkStore,
    settings: &StaffNetworkSettings,
) -> Result<StaffNetworkSettingsView, String> {
    let credentials = store
        .credentials
        .load_staff_credentials()
        .map_err(|_| "无法读取教工区凭据状态")?;
    let custom_campus_card_number = credentials
        .as_ref()
        .filter(|value| is_campus_card_number(&value.campus_card_number))
        .map(|value| value.campus_card_number.clone());
    let custom_configured = custom_campus_card_number.is_some()
        && credentials
            .as_ref()
            .is_some_and(|value| !value.password.is_empty());
    store
        .staff
        .configured
        .store(custom_configured, Ordering::Relaxed);
    Ok(StaffNetworkSettingsView {
        revision: settings.revision,
        mode: settings.mode,
        enabled: settings.enabled && settings.mode == StaffLoginMode::AppAuto,
        credential_mode: settings.credential_mode.clone(),
        custom_configured,
        custom_campus_card_number,
        needs_apply: store.staff.needs_apply.load(Ordering::Acquire),
        applied_mode: settings.applied_mode,
    })
}
pub(super) fn view(
    store: &CampusNetworkStore,
    current: &CredentialsProvider<'_>,
) -> Result<StaffNetworkSettingsView, String> {
    let settings = load(store)?;
    let needs_apply = credentials(store, &settings, current)
        .map(|(card, password)| !applied(&settings, &credential_stamp(&card, &password)))
        .unwrap_or(true);
    store
        .staff
        .needs_apply
        .store(needs_apply, Ordering::Release);
    cached_view(store, &settings)
}
pub(super) fn persist(
    store: &CampusNetworkStore,
    settings: &StaffNetworkSettings,
) -> Result<(), String> {
    let mut cache = store
        .staff
        .settings
        .lock()
        .map_err(|_| "教工区设置锁失败")?;
    store
        .credentials
        .save_staff_settings(settings)
        .map_err(|_| "保存教工区设置失败")?;
    *cache = Some(settings.clone());
    Ok(())
}
pub(super) fn save(
    store: &CampusNetworkStore,
    input: StaffNetworkSettingsInput,
    current: &CredentialsProvider<'_>,
    events: &dyn NetworkEvents,
) -> Result<StaffNetworkSettingsView, String> {
    // Stopping automatic login is allowed even while a manual apply/login is in flight.
    if !input.enabled && input.campus_card_number.is_none() && input.password.is_none() {
        let old = load(store)?;
        if old.mode == input.mode && old.credential_mode == input.credential_mode {
            return store.operations.settings(|| {
                let mut settings = load(store)?;
                if settings.mode != input.mode || settings.credential_mode != input.credential_mode
                {
                    return Err("教工区配置已变化，请重新保存".into());
                }
                store.staff.suspended.store(true, Ordering::Release);
                settings.enabled = false;
                *store
                    .staff
                    .settings
                    .lock()
                    .map_err(|_| "教工区设置锁失败")? = Some(settings.clone());
                persist(store, &settings)?;
                let view = cached_view(store, &settings)?;
                events.staff_settings_changed(&view);
                store.change_staff_status(
                    |status| {
                        status.state = DormLoginState::Disabled;
                        status.next_probe_at_ms = None;
                        status.message = Some("应用自动登录已关闭".into());
                    },
                    events,
                );
                Ok(view)
            });
        }
    }
    store.operations.configure(|| {
        let old = load(store)?;
        let mut next = old.clone();
        let mut changed = old.mode != input.mode || old.credential_mode != input.credential_mode;
        if input.credential_mode == DormCredentialMode::Custom {
            let old_credentials = store
                .credentials
                .load_staff_credentials()
                .map_err(|_| "无法读取教工区凭据")?;
            let merged = DormCustomCredentials {
                campus_card_number: input
                    .campus_card_number
                    .or_else(|| {
                        old_credentials
                            .as_ref()
                            .map(|v| v.campus_card_number.clone())
                    })
                    .unwrap_or_default(),
                password: input
                    .password
                    .or_else(|| old_credentials.as_ref().map(|v| v.password.clone()))
                    .unwrap_or_default(),
            };
            if !is_campus_card_number(&merged.campus_card_number)
                || merged.password.is_empty()
                || merged.password.len() > 1024
            {
                return Err("校园卡号须为 6 位数字，密码不能为空且不得超过 1024 字节".into());
            }
            changed |= old_credentials.as_ref() != Some(&merged);
            if changed {
                store.staff.suspended.store(true, Ordering::Release);
                store.staff.needs_apply.store(true, Ordering::Release);
            }
            store
                .credentials
                .save_staff_credentials(&merged)
                .map_err(|_| "保存教工区凭据失败")?;
            store.staff.configured.store(true, Ordering::Relaxed);
        }
        next.mode = input.mode;
        next.enabled = input.enabled && input.mode == StaffLoginMode::AppAuto;
        next.credential_mode = input.credential_mode;
        if next.enabled {
            credentials(store, &next, current)?;
        }
        if changed {
            next.revision = next.revision.checked_add(1).ok_or("设置版本已溢出")?;
        }
        next.pending_failure_notice = None;
        if changed || !next.enabled {
            store.staff.suspended.store(true, Ordering::Release);
        }
        persist(store, &next)?;
        store.staff.suspended.store(false, Ordering::Release);
        *store
            .staff
            .confirmation
            .lock()
            .map_err(|_| "教工区确认锁失败")? = None;
        let result = view(store, current)?;
        store.change_staff_status(
            |status| {
                status.state = if result.needs_apply || !result.enabled {
                    DormLoginState::Disabled
                } else {
                    DormLoginState::Monitoring
                };
                status.consecutive_failures = 0;
                status.next_probe_at_ms = result.enabled.then_some(0);
                status.message = Some(
                    if result.needs_apply {
                        "配置已保存，等待手动应用"
                    } else {
                        "教工区设置已保存"
                    }
                    .into(),
                );
            },
            events,
        );
        events.staff_settings_changed(&result);
        Ok(result)
    })
}

/// No credential reads. Even a keyring failure leaves automatic login inhibited in this process.
pub(crate) fn disable(
    store: &CampusNetworkStore,
    events: &dyn NetworkEvents,
) -> Result<(), String> {
    store.staff.suspended.store(true, Ordering::Release);
    let result = (|| {
        let mut settings = load(store)?;
        settings.enabled = false;
        *store
            .staff
            .settings
            .lock()
            .map_err(|_| "教工区设置锁失败")? = Some(settings.clone());
        persist(store, &settings)?;
        events.staff_settings_changed(&cached_view(store, &settings)?);
        Ok(())
    })();
    store.change_staff_status(
        |status| {
            status.state = DormLoginState::Disabled;
            status.next_probe_at_ms = None;
            status.message = Some("应用自动登录已关闭".into());
        },
        events,
    );
    if let Ok(mut pending) = store.staff.confirmation.lock() {
        *pending = None;
    }
    result.map_err(|_: String| {
        "无法保存教工区自动登录关闭设置，已暂停自动登录，未发送注销请求".into()
    })
}
