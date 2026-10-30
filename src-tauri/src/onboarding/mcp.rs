use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;

#[tool_router(router = onboarding_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "onboarding_get_status",
        description = "获取your-szu-life初始化设置的步骤顺序、完成/跳过状态与下一步。对应资源 your-szu-life://data/onboarding/status"
    )]
    pub async fn onboarding_get_status(&self) -> Result<String, String> {
        let state = crate::onboarding::state_view()?;
        serde_json::to_string_pretty(&state).map_err(|error| format!("序列化失败: {error}"))
    }
}
