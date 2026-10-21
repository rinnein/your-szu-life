use super::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OpenPageParams {
    /// 目标页面 URI，格式为 your-szu-life://page/<route>，例如 your-szu-life://page/electricity、your-szu-life://page/uooc
    pub uri: String,
}

#[tool_router(router = ui_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "open_page",
        description = "打开或激活your-szu-life桌面窗口，并导航到指定的 your-szu-life://page/... 页面（例如 workbench, auth, network, electricity, enrollment, uooc, oj, settings, debug）"
    )]
    pub async fn open_page(
        &self,
        Parameters(params): Parameters<OpenPageParams>,
    ) -> Result<String, String> {
        let target = params.uri.trim();
        let path = if let Some(stripped) = target.strip_prefix("your-szu-life://page") {
            stripped.trim_start_matches('/')
        } else if let Some(stripped) = target.strip_prefix("your-szu-life://") {
            stripped.trim_start_matches('/')
        } else {
            target.trim_start_matches('/')
        };

        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            let route_event = serde_json::json!({
                "route": format!("/{}", path),
                "uri": target,
            });
            let _ = window.emit("navigate-page", route_event);
            Ok(format!("已唤起窗口并导航至页面: /{}", path))
        } else {
            Err("主窗口不存在或未就绪".to_string())
        }
    }
}
