use super::{
    http::{HttpError, HttpResponse, PortalHttp},
    model::{DebugLogEntry, DebugModeView},
    network::NetworkSnapshot,
    now_ms, CampusNetworkStore,
};
use reqwest::Url;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::Emitter;

pub(crate) fn initialize(_app: &tauri::AppHandle, store: &CampusNetworkStore) {
    if let Ok(Some(enabled)) =
        crate::preferences::load::<bool>(crate::preferences::DEBUG_ENABLED)
    {
        store.debug_enabled.store(enabled, Ordering::Release);
    }
}

pub(super) fn mode_view(store: &CampusNetworkStore) -> DebugModeView {
    DebugModeView {
        enabled: store.debug_enabled.load(Ordering::Acquire),
        log_count: 0,
    }
}

pub(super) fn set_mode(
    app: &tauri::AppHandle,
    store: &CampusNetworkStore,
    enabled: bool,
) -> Result<DebugModeView, String> {
    crate::preferences::save(crate::preferences::DEBUG_ENABLED, &enabled)?;
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

pub(super) fn clear_logs(app: &tauri::AppHandle, store: &CampusNetworkStore) -> Result<(), String> {
    let view = mode_view(store);
    let _ = app.emit("debug-mode-changed", &view);
    Ok(())
}

pub(crate) fn record(_app: &tauri::AppHandle, store: &CampusNetworkStore, entry: DebugLogEntry) {
    if !store.debug_enabled.load(Ordering::Acquire) {
        return;
    }
    let entry = sanitize_entry(entry);
    let summary = format_log_entry(&entry);
    match entry.level.to_lowercase().as_str() {
        "error" => log::error!(target: "campus_network::diagnostics", "{summary}"),
        "warn" | "warning" => log::warn!(target: "campus_network::diagnostics", "{summary}"),
        "debug" => log::debug!(target: "campus_network::diagnostics", "{summary}"),
        _ => log::info!(target: "campus_network::diagnostics", "{summary}"),
    }
}

fn format_log_entry(entry: &DebugLogEntry) -> String {
    let mut parts = Vec::new();
    parts.push(format!("[{}/{}]", entry.source, entry.stage));
    if let Some(msg) = &entry.message {
        parts.push(msg.clone());
    }
    if let Some(ssid) = &entry.ssid {
        parts.push(format!("SSID={ssid}"));
    }
    if let Some(ip) = &entry.ip {
        parts.push(format!("IP={ip}"));
    }
    if let Some(portal) = &entry.portal {
        parts.push(format!("门户={portal}"));
    }
    if let Some(status) = entry.http_status {
        parts.push(format!("HTTP={status}"));
    }
    if let Some(elapsed) = entry.elapsed_ms {
        parts.push(format!("{elapsed}ms"));
    }
    if let Some(err) = &entry.error_kind {
        parts.push(format!("错误={err}"));
    }
    parts.join(" ")
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
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
        .collect();
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
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
        let long_message = format!("bad\0message\nmultiline-{}", "x".repeat(1000));
        let entry = sanitize_entry(DebugLogEntry {
            ssid: Some("SZU_WLAN".into()),
            ip: Some("172.30.12.34".into()),
            message: Some(long_message),
            ..Default::default()
        });
        assert_eq!(entry.ssid.as_deref(), Some("S***N"));
        assert_eq!(entry.ip.as_deref(), Some("172.30.12.*"));
        assert_eq!(
            entry.message.as_deref(),
            Some(format!("badmessage\nmultiline-{}", "x".repeat(1000)).as_str())
        );
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
