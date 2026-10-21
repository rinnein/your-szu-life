pub mod resources;
pub mod rig_adapter;
pub mod server;
pub mod service;
pub mod tools;

#[cfg(test)]
mod tests;

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde::{Deserialize, Serialize};
use service::UnifiedSzuMcpService;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_MCP_PORT: u16 = 53901;
pub const MAX_PORT_SCAN: u16 = 54000;

struct RunningServer {
    port: u16,
    cancel: CancellationToken,
}

static SERVER_STATE: Mutex<Option<RunningServer>> = Mutex::new(None);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatusView {
    pub running: bool,
    pub enabled: bool,
    pub port: Option<u16>,
    pub endpoint: Option<String>,
}

/// 查询当前本地公开 MCP 服务的运行状况、是否开启及端点 URL
#[tauri::command]
pub fn get_mcp_server_status() -> McpStatusView {
    let guard = SERVER_STATE.lock().unwrap();
    if let Some(server) = guard.as_ref() {
        McpStatusView {
            running: true,
            enabled: true,
            port: Some(server.port),
            endpoint: Some(format!("http://127.0.0.1:{}/mcp", server.port)),
        }
    } else {
        McpStatusView {
            running: false,
            enabled: false,
            port: None,
            endpoint: None,
        }
    }
}

/// 启动 MCP 本地公开服务并绑定高位端口（支持桌面与安卓端）
pub fn start_mcp_server(app: &AppHandle) -> Result<u16, String> {
    let mut guard = SERVER_STATE.lock().unwrap();
    if let Some(server) = guard.as_ref() {
        return Ok(server.port);
    }

    let app_handle = app.clone();
    let service = UnifiedSzuMcpService::new(app_handle);

    // 扫描高位端口并尝试绑定
    let mut chosen_listener = None;
    let mut bound_port = 0;

    for port in DEFAULT_MCP_PORT..=MAX_PORT_SCAN {
        let addr = format!("127.0.0.1:{port}");
        match std::net::TcpListener::bind(&addr) {
            Ok(std_listener) => {
                std_listener
                    .set_nonblocking(true)
                    .map_err(|e| e.to_string())?;
                chosen_listener = Some(std_listener);
                bound_port = port;
                break;
            }
            Err(_) => continue,
        }
    }

    let std_listener = chosen_listener
        .ok_or_else(|| format!("无法在端口范围 {DEFAULT_MCP_PORT}..={MAX_PORT_SCAN} 找到可用端口"))?;

    let cancel = CancellationToken::new();

    let session_manager = Arc::new(LocalSessionManager::default());
    let config = StreamableHttpServerConfig::default()
        .with_stateful_mode(true)
        .with_allowed_hosts(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            format!("127.0.0.1:{bound_port}"),
            format!("localhost:{bound_port}"),
        ])
        .with_cancellation_token(cancel.clone());

    let http_service: StreamableHttpService<UnifiedSzuMcpService, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(service.clone()),
            session_manager,
            config,
        );

    let router = axum::Router::new().nest_service("/mcp", http_service);

    let cancel_worker = cancel.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(tcp_listener) = tokio::net::TcpListener::from_std(std_listener) else {
            eprintln!("无法在 Tokio 运行时中将 TcpListener 转换为异步监听器");
            return;
        };
        eprintln!("MCP server listening on http://127.0.0.1:{bound_port}/mcp");
        let _ = axum::serve(tcp_listener, router)
            .with_graceful_shutdown(async move { cancel_worker.cancelled_owned().await })
            .await;
        eprintln!("MCP server on port {bound_port} closed");
    });

    *guard = Some(RunningServer {
        port: bound_port,
        cancel,
    });

    Ok(bound_port)
}

use std::path::PathBuf;
use tauri::Manager;

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("mcp-config.json"))
}

fn is_persisted_enabled(app: &AppHandle) -> bool {
    let Some(path) = config_path(app) else {
        return false;
    };
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            return json.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        }
    }
    false
}

fn save_persisted_enabled(app: &AppHandle, enabled: bool) {
    if let Some(path) = config_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::json!({ "enabled": enabled });
        let _ = std::fs::write(path, json.to_string());
    }
}

/// 停止 MCP 本地公开服务并彻底释放关闭端口
pub fn stop_mcp_server() -> Result<(), String> {
    let mut guard = SERVER_STATE.lock().unwrap();
    if let Some(server) = guard.take() {
        server.cancel.cancel();
    }
    Ok(())
}

/// 初始化 MCP 服务。默认保持关闭，仅当用户先前主动开启过时才自动恢复
pub fn initialize(app: &AppHandle) -> Result<(), String> {
    if is_persisted_enabled(app) {
        let _ = start_mcp_server(app)?;
    }
    Ok(())
}

/// 开关直接控制该端口是否开启与关闭
#[tauri::command]
pub fn set_mcp_server_enabled(app: AppHandle, enabled: bool) -> Result<McpStatusView, String> {
    save_persisted_enabled(&app, enabled);
    if enabled {
        start_mcp_server(&app)?;
    } else {
        stop_mcp_server()?;
    }
    Ok(get_mcp_server_status())
}

/// 以 Stdio 命令行模式运行 MCP 服务（供 CLI 或外部编辑器子进程接入）
pub async fn run_stdio(app: AppHandle) -> Result<(), String> {
    let service = UnifiedSzuMcpService::new(app);
    let transport = rmcp::transport::io::stdio();
    let server = rmcp::serve_server(service, transport)
        .await
        .map_err(|e| format!("MCP stdio 启动失败: {e}"))?;
    server
        .waiting()
        .await
        .map_err(|e| format!("MCP stdio 服务异常终止: {e}"))?;
    Ok(())
}
