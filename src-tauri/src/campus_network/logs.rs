use super::{model::*, now_ms, settings::dorm_settings, CampusNetworkStore};
use tauri::Manager;

pub(super) fn append_dorm_log(_app: &tauri::AppHandle, log: DormLoginLogView) {
    let sanitized = sanitized_log(log);
    log::info!(
        target: "campus_network::dorm",
        "阶段: {}, 结果: {}, 可达: {}, 消息: {:?}",
        sanitized.stage,
        sanitized.outcome,
        sanitized.portal_reachable,
        sanitized.message
    );
}

pub(super) fn dorm_notify_failure(app: &tauri::AppHandle, message: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("校园网登录失败")
        .body(message)
        .show();
}

pub(super) fn save_dorm_failure_notice(app: &tauri::AppHandle, message: &str) {
    let Some(store) = app.try_state::<CampusNetworkStore>() else {
        return;
    };
    let Ok(mut settings) = dorm_settings(&store) else {
        return;
    };
    settings.pending_failure_notice = Some(DormFailureNotice {
        message: message.to_owned(),
        timestamp_ms: now_ms(),
    });
    let _ = store.credentials.save_dorm_settings(&settings);
    {
        if let Ok(mut cached) = store.dorm_settings.lock() {
            *cached = Some(settings);
        };
    }
}

/// Central safety boundary: gateway response bodies never enter disk logs or log events.
pub(super) fn sanitized_log(mut log: DormLoginLogView) -> DormLoginLogView {
    log.response = None;
    log
}
