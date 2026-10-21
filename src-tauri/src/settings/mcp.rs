use crate::mcp::service::UnifiedSzuMcpService;
use crate::settings::SettingsStore;
use rmcp::tool;
use rmcp::tool_router;
use tauri::Manager;

#[tool_router(router = settings_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "settings_get_snapshot",
        description = "获取your-szu-life当前非敏感运行配置项集合与偏好设定"
    )]
    pub async fn settings_get_snapshot(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<SettingsStore>()
            .ok_or("设置存储不可用")?;
        let snapshot = store.snapshot()?;
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "settings_get_storage_status",
        description = "查询系统底层安全凭据存储（Keyring 密钥环）的运行健康状态与硬件加密支持级别"
    )]
    pub async fn settings_get_storage_status(&self) -> Result<String, String> {
        let status = crate::storage::commands::get_credential_storage_status();
        serde_json::to_string_pretty(&status).map_err(|e| format!("序列化失败: {e}"))
    }
}
