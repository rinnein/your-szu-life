use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;

#[tool_router(router = integration_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "integration_get_status",
        description = "获取外部集成（WorkBuddy 专家等）所需的本地状态：应用版本、MCP 开关/端口/端点、初始化进度、统一认证与 UOOC 登录状态。不包含姓名、学号、Cookie、密码或密钥。对应资源 your-szu-life://data/app/integration"
    )]
    pub async fn integration_get_status(&self) -> Result<String, String> {
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || {
            let status = crate::integration::build(&app);
            serde_json::to_string_pretty(&status).map_err(|error| format!("序列化失败: {error}"))
        })
        .await
        .map_err(|_| "任务执行失败".to_string())?
    }
}
