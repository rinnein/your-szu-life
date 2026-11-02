pub mod resources;
pub mod rig_adapter;
pub mod server;
pub mod service;
pub mod tools;

#[cfg(test)]
mod tests;

use crate::ports::{self, PortService};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde::{Deserialize, Serialize};
use service::UnifiedSzuMcpService;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

/// MCP 服务的默认首选端口；实际端口优先取设置中保存的值。
pub const DEFAULT_MCP_PORT: u16 = ports::MCP_PORT_RANGE.0;

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
    /// 服务当前期望使用的端口：已保存的端口优先，否则为默认端口。
    pub preferred_port: u16,
    pub endpoint: Option<String>,
}

/// 查询当前本地公开 MCP 服务的运行状况、是否开启及端点 URL
#[tauri::command]
pub fn get_mcp_server_status() -> McpStatusView {
    let guard = SERVER_STATE.lock().unwrap();
    let preferred_port = preferred_port();
    if let Some(server) = guard.as_ref() {
        McpStatusView {
            running: true,
            enabled: true,
            port: Some(server.port),
            preferred_port,
            endpoint: Some(format!("http://127.0.0.1:{}/mcp", server.port)),
        }
    } else {
        McpStatusView {
            running: false,
            enabled: false,
            port: None,
            preferred_port,
            endpoint: None,
        }
    }
}

fn persisted_port() -> Option<u16> {
    crate::preferences::load::<u16>(crate::preferences::MCP_PORT)
        .ok()
        .flatten()
        .filter(|port| (ports::MCP_PORT_RANGE.0..=ports::MCP_PORT_RANGE.1).contains(port))
}

fn preferred_port() -> u16 {
    persisted_port().unwrap_or(DEFAULT_MCP_PORT)
}

/// 启动 MCP 本地公开服务并绑定高位端口（支持桌面与安卓端）
pub fn start_mcp_server(app: &AppHandle) -> Result<u16, String> {
    let mut guard = SERVER_STATE.lock().unwrap();
    if let Some(server) = guard.as_ref() {
        return Ok(server.port);
    }

    let app_handle = app.clone();
    let service = UnifiedSzuMcpService::new(app_handle);

    // 优先使用上次保存的端口；从未保存过时把默认端口当作首选，
    // 这样默认端口被占也能触发「端口已切换」提示。
    let saved_port = persisted_port();
    let (std_listener, allocation) =
        ports::bind_loopback(PortService::Mcp, Some(saved_port.unwrap_or(DEFAULT_MCP_PORT)))
            .map_err(|error| error.to_string())?;
    let bound_port = allocation.port;

    if saved_port != Some(bound_port) {
        if let Err(error) = crate::preferences::save(crate::preferences::MCP_PORT, &bound_port) {
            log::error!("保存 MCP 端口失败: {error}");
        }
    }
    if let Some(previous) = allocation.switched_from {
        ports::notify_mcp_port_changed(previous, bound_port);
    }

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

fn is_persisted_enabled() -> bool {
    crate::preferences::load::<bool>(crate::preferences::MCP_ENABLED)
        .ok()
        .flatten()
        .unwrap_or(false)
}

fn save_persisted_enabled(enabled: bool) {
    if let Err(error) = crate::preferences::save(crate::preferences::MCP_ENABLED, &enabled) {
        eprintln!("保存 MCP 开关失败: {error}");
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
    if is_persisted_enabled() {
        let _ = start_mcp_server(app)?;
    }
    Ok(())
}

/// 开关直接控制该端口是否开启与关闭
#[tauri::command]
pub fn set_mcp_server_enabled(app: AppHandle, enabled: bool) -> Result<McpStatusView, String> {
    if enabled {
        start_mcp_server(&app)?;
        save_persisted_enabled(true);
    } else {
        stop_mcp_server()?;
        save_persisted_enabled(false);
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
