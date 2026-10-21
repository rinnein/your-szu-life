use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;
use tauri::Manager;

#[tool_router(router = auth_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "auth_get_status",
        description = "查询当前深圳大学统一身份认证会话状态（如已认证、未认证、过期等）"
    )]
    pub async fn auth_get_status(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<crate::auth::AuthStore>()
            .ok_or("认证存储不可用")?;
        let state = crate::auth::service::get_auth_state(self.app.clone(), &store)?;
        serde_json::to_string_pretty(&state).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "auth_get_profile",
        description = "获取当前登录学生/教工的个人基础资料（姓名、学号、所属学院、校园卡号、身份类型）"
    )]
    pub async fn auth_get_profile(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<crate::auth::AuthStore>()
            .ok_or("认证存储不可用")?;
        let profile = crate::auth::service::get_account_profile(&store)?;
        let user_data = crate::auth::service::get_user_data(&store)?;
        serde_json::to_string_pretty(&serde_json::json!({
            "profile": profile,
            "userData": user_data,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "auth_refresh_session",
        description = "主动刷新深大统一身份认证会话与校园各子系统的认证 Cookie"
    )]
    pub async fn auth_refresh_session(&self) -> Result<String, String> {
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || crate::auth::service::refresh_auth_session(app))
            .await
            .map_err(|_| "任务执行失败".to_string())??;
        Ok("认证会话刷新成功".to_string())
    }
}
