use super::{
    http::{HttpError, HttpResponse, PortalHttp},
    model::{DebugLogEntry, DebugModeView},
    network::NetworkSnapshot,
    now_ms, CampusNetworkStore,
};
use reqwest::Url;
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::Write,
    sync::atomic::Ordering,
    time::Instant,
};
use tauri::{Emitter, Manager};

const MAX_DEBUG_LOGS: usize = 1000;
const DEBUG_LOG_FILE: &str = "campus-network-debug.jsonl";
const DEBUG_MODE_FILE: &str = "campus-network-debug.json";

pub(crate) fn initialize(app: &tauri::AppHandle, store: &CampusNetworkStore) {
    if let Ok(path) = debug_mode_path(app) {
        if let Ok(value) = fs::read_to_string(path) {
            if let Ok(enabled) = serde_json::from_str::<bool>(&value) {
                store.debug_enabled.store(enabled, Ordering::Release);
            }
        }
    }
    let _ = load_logs(app, store);
}

fn debug_mode_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?
        .join(DEBUG_MODE_FILE))
}

fn debug_log_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_log_dir()
        .map_err(|error| error.to_string())?
        .join(DEBUG_LOG_FILE))
}

pub(super) fn mode_view(store: &CampusNetworkStore) -> DebugModeView {
    let log_count = store
        .debug_logs
        .lock()
        .map(|logs| logs.len())
        .unwrap_or_default();
    DebugModeView {
        enabled: store.debug_enabled.load(Ordering::Acquire),
        log_count,
    }
}

pub(super) fn set_mode(
    app: &tauri::AppHandle,
    store: &CampusNetworkStore,
    enabled: bool,
) -> Result<DebugModeView, String> {
    let path = debug_mode_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let raw = serde_json::to_string(&enabled).map_err(|error| error.to_string())?;
    fs::write(path, raw).map_err(|error| error.to_string())?;
    store.debug_enabled.store(enabled, Ordering::Release);
    let view = mode_view(store);
    let _ = app.emit("debug-mode-changed", &view);
    if enabled {
        record(
            app,
            store,
            DebugLogEntry {
                timestamp_ms: now_ms(),
                level: "info".into(),
                source: "debug".into(),
                stage: "enabled".into(),
                message: Some("调试模式已启用".into()),
                ..Default::default()
            },
        );
    }
    Ok(mode_view(store))
}

pub(super) fn load_logs(
    app: &tauri::AppHandle,
    store: &CampusNetworkStore,
) -> Result<Vec<DebugLogEntry>, String> {
    let path = debug_log_path(app)?;
    let content = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<_> = content.lines().collect();
    let start = lines.len().saturating_sub(MAX_DEBUG_LOGS);
    let logs: VecDeque<_> = lines[start..]
        .iter()
        .filter_map(|line| serde_json::from_str::<DebugLogEntry>(line).ok())
        .collect();
    let result = logs.iter().cloned().collect::<Vec<_>>();
    if let Ok(mut cached) = store.debug_logs.lock() {
        *cached = logs;
    }
    Ok(result)
}

pub(super) fn clear_logs(app: &tauri::AppHandle, store: &CampusNetworkStore) -> Result<(), String> {
    let path = debug_log_path(app)?;
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    if let Ok(mut cached) = store.debug_logs.lock() {
        cached.clear();
    }
    let view = mode_view(store);
    let _ = app.emit("debug-mode-changed", &view);
    Ok(())
}

pub(crate) fn record(app: &tauri::AppHandle, store: &CampusNetworkStore, entry: DebugLogEntry) {
    if !store.debug_enabled.load(Ordering::Acquire) {
        return;
    }
    let entry = sanitize_entry(entry);
    let Ok(path) = debug_log_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    if writeln!(file, "{line}").is_err() {
        return;
    }
    if let Ok(mut cached) = store.debug_logs.lock() {
        cached.push_back(entry.clone());
        while cached.len() > MAX_DEBUG_LOGS {
            cached.pop_front();
        }
    }
    trim_file(&path);
    let _ = app.emit("debug-log-added", &entry);
}

fn trim_file(path: &std::path::Path) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > MAX_DEBUG_LOGS {
        let trimmed = &lines[lines.len() - MAX_DEBUG_LOGS..];
        let _ = fs::write(path, format!("{}\n", trimmed.join("\n")));
    }
}

fn mask_ssid(value: Option<String>) -> Option<String> {
    let value = value?;
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 2 {
        return Some("**".into());
    }
    Some(format!("{}***{}", chars[0], chars[chars.len() - 1]))
}

fn mask_ip(value: Option<String>) -> Option<String> {
    let value = value?;
    let mut parts = value.split('.');
    let (Some(a), Some(b), Some(c), Some(_)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Some("<private>".into());
    };
    Some(format!("{a}.{b}.{c}.*"))
}

fn sanitize_text(value: Option<String>) -> Option<String> {
    let value = value?;
    let value: String = value
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let value = value.trim();
    (!value.is_empty()).then(|| value.chars().take(240).collect())
}

fn sanitize_entry(mut entry: DebugLogEntry) -> DebugLogEntry {
    entry.ssid = mask_ssid(entry.ssid);
    entry.ip = mask_ip(entry.ip);
    entry.message = sanitize_text(entry.message);
    entry.source = entry
        .source
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    entry.stage = entry
        .stage
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '/')
        .take(96)
        .collect();
    entry.portal = entry.portal.map(|value| {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(64)
            .collect()
    });
    entry.error_kind = entry.error_kind.map(|value| {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(64)
            .collect()
    });
    entry
}

pub(super) fn record_snapshot(
    app: &tauri::AppHandle,
    store: &CampusNetworkStore,
    snapshot: &NetworkSnapshot,
    error: Option<&str>,
) {
    record(
        app,
        store,
        DebugLogEntry {
            timestamp_ms: now_ms(),
            level: if error.is_some() { "warn" } else { "info" }.into(),
            source: "network".into(),
            stage: "snapshot".into(),
            connected: Some(snapshot.effective_connected()),
            transport: snapshot.transport.clone(),
            capabilities_available: snapshot.capabilities_available,
            wifi_permission: snapshot.wifi_permission.clone(),
            location_enabled: snapshot.location_enabled,
            wifi_connected: Some(snapshot.wifi_connected),
            wifi_ip: snapshot.wifi_ip.clone(),
            wifi_has_private_172_ipv4: Some(snapshot.wifi_has_private_172_ipv4),
            ssid: snapshot.ssid.clone(),
            ip: snapshot.ip.clone(),
            error_kind: error.map(classify_bridge_error),
            message: error.map(str::to_owned),
            ..Default::default()
        },
    );
}

pub(super) struct DiagnosticHttp<'a> {
    pub(super) inner: &'a dyn PortalHttp,
    pub(super) app: &'a tauri::AppHandle,
    pub(super) store: &'a CampusNetworkStore,
}

impl PortalHttp for DiagnosticHttp<'_> {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError> {
        let started = Instant::now();
        let result = self.inner.get(url);
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let portal = portal_name(url);
        let stage = portal_stage(url);
        let (status, error_kind, message, level) = match &result {
            Ok(response) if response.status >= 400 => (
                Some(response.status),
                Some("http-status".into()),
                Some("门户返回 HTTP 状态异常".into()),
                "warn",
            ),
            Ok(response) => (Some(response.status), None, None, "info"),
            Err(error) => (
                None,
                Some(http_error_kind(*error).into()),
                Some(error.to_string()),
                "warn",
            ),
        };
        record(
            self.app,
            self.store,
            DebugLogEntry {
                timestamp_ms: now_ms(),
                level: level.into(),
                source: "portal".into(),
                stage,
                portal,
                http_status: status,
                elapsed_ms: Some(elapsed_ms),
                error_kind,
                message,
                ..Default::default()
            },
        );
        result
    }
}

fn portal_name(url: &Url) -> Option<String> {
    match url.host_str() {
        Some("net.szu.edu.cn") => Some("staff".into()),
        Some("172.30.255.42") => Some("dormitory".into()),
        _ => None,
    }
}

fn portal_stage(url: &Url) -> String {
    match url.path() {
        "/" => "config".into(),
        path if path.contains("rad_user_info") => "session".into(),
        path if path.contains("chkstatus") => "status".into(),
        path if path.contains("logout") => "logout".into(),
        _ => "request".into(),
    }
}

fn classify_bridge_error(error: &str) -> String {
    let text = error.to_ascii_lowercase();
    if text.contains("context") {
        "android-context".into()
    } else if text.contains("jni") || text.contains("java") {
        "android-bridge".into()
    } else {
        "snapshot".into()
    }
}

fn http_error_kind(error: HttpError) -> &'static str {
    match error {
        HttpError::Timeout => "timeout",
        HttpError::InvalidResponse => "invalid-response",
        HttpError::UnsafeUrl => "unsafe-url",
        HttpError::Canceled => "canceled",
        HttpError::Dns => "dns",
        HttpError::Tls => "tls",
        HttpError::Connection => "connection",
        HttpError::Unavailable => "unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_private_network_identifiers_and_control_text() {
        let entry = sanitize_entry(DebugLogEntry {
            ssid: Some("SZU_WLAN".into()),
            ip: Some("172.30.12.34".into()),
            message: Some("bad\nmessage".into()),
            ..Default::default()
        });
        assert_eq!(entry.ssid.as_deref(), Some("S***N"));
        assert_eq!(entry.ip.as_deref(), Some("172.30.12.*"));
        assert_eq!(entry.message.as_deref(), Some("badmessage"));
    }

    #[test]
    fn maps_fixed_portals_without_query_parameters() {
        let url =
            Url::parse("https://net.szu.edu.cn/cgi-bin/rad_user_info?ip=172.30.1.2&_=1").unwrap();
        assert_eq!(portal_name(&url).as_deref(), Some("staff"));
        assert_eq!(portal_stage(&url), "session");
    }

    #[test]
    fn classifies_http_transport_failures_without_exposing_raw_details() {
        assert_eq!(http_error_kind(HttpError::Dns), "dns");
        assert_eq!(http_error_kind(HttpError::Tls), "tls");
        assert_eq!(http_error_kind(HttpError::Timeout), "timeout");
        assert_eq!(http_error_kind(HttpError::Connection), "connection");
    }
}
