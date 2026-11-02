use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;

#[tool_router(router = file_manager_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "file_manager_get_state",
        description = "获取应用统一文件保存目录：平台默认目录、用户已选目录、各业务模块（如督导巡课录像 jxxk）的最终落盘路径，以及 Android 系统文件夹授权状态。对应资源 your-szu-life://data/file-manager/state"
    )]
    pub fn file_manager_get_state(&self) -> Result<String, String> {
        let state = crate::file_manager::state(&self.app)?;
        serde_json::to_string_pretty(&state).map_err(|error| format!("序列化失败: {error}"))
    }
}
