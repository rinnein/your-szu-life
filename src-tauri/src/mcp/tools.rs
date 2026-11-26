use super::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OpenPageParams {
    /// 目标页面 URI，格式为 your-szu-life://page/<route>，值为前端路由路径，例如
    /// your-szu-life://page/life/campus-payments/electricity、your-szu-life://page/study/uooc
    pub uri: String,
}

#[tool_router(router = ui_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "open_page",
        description = "打开或激活your-szu-life桌面窗口，并导航到指定的 your-szu-life://page/... 页面。可用页面：空路径(首页)、study、life、settings、study/timetable、study/oj、study/uooc、study/lms、study/signin、study/academic、study/grades、study/evaluation、study/patrol（含 patrol/course、patrol/downloads、patrol/watch）、study/enrollment（含 enrollment/login、enrollment/selected、enrollment/tasks、enrollment/timetable、enrollment/settings）、life/campus-network、life/campus-payments（含 card、network、electricity、recharge）。设置页的外观、通知、后台、权限、网络、储存、本地模型、AI、MCP、同步、快捷键、关于与调试是 tab，使用 settings?tab=profile、appearance、notifications、background、permissions、network、storage、models、ai、mcp、sync、shortcuts、about 与 debug。"
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
            #[cfg(desktop)]
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
