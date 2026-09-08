use super::{model::*, now_ms, settings::dorm_settings, CampusNetworkStore};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use tauri::{Emitter, Manager};

const DORM_LOG_FILE: &str = "dorm-network-login.jsonl";

pub(super) fn dorm_log_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_log_dir()
        .map_err(|error| error.to_string())?
        .join(DORM_LOG_FILE))
}

pub(super) fn append_dorm_log(app: &tauri::AppHandle, log: DormLoginLogView) {
    let log = sanitized_log(log);
    let Ok(path) = dorm_log_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let Ok(line) = serde_json::to_string(&log) else {
        return;
    };
    let _ = writeln!(file, "{line}");
    // Keep the file bounded without retaining secrets in application state.
    if let Ok(content) = fs::read_to_string(&path) {
        let mut lines: Vec<&str> = content.lines().collect();
        if lines.len() > 1000 {
            lines = lines.split_off(lines.len() - 1000);
            let _ = fs::write(path, format!("{}\n", lines.join("\n")));
        }
    }
    if let Some(store) = app.try_state::<CampusNetworkStore>() {
        if let Ok(mut logs) = store.dorm_logs.lock() {
            logs.push_back(log.clone());
            while logs.len() > 1000 {
                logs.pop_front();
            }
        }
        let _ = app.emit("dorm-login-log-added", &log);
    }
}

pub(super) fn load_dorm_logs(app: &tauri::AppHandle) -> Vec<DormLoginLogView> {
    let Ok(path) = dorm_log_path(app) else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .rev()
        .take(1000)
        .filter_map(|line| serde_json::from_str::<DormLoginLogView>(line).ok())
        .collect()
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
