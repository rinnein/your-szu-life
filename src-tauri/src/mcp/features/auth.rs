use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;
use tauri::Manager;

#[tool_router(router = auth_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "auth_get_status",
        description = "查询统一认证会话状态。Android 优先使用共享 AndroidX Cookie API 完成普通 Cookie 的读取、恢复与清理；缺少 API 或原生 API 无法表达分区及来源属性时，使用应用自己的 CDP 通道，不受数据库 schema 版本限制。失败保留已有保存数据，不自动提交密码；不返回 Cookie 或密码。"
    )]
    pub async fn auth_get_status(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<szu_core::auth::AuthStore>()
            .ok_or("认证存储不可用")?;
        let state = szu_core::auth::service::get_auth_state(self.app.clone(), &store)?;
        serde_json::to_string_pretty(&state).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "auth_get_profile",
        description = "获取当前登录学生/教工的个人基础资料（姓名、学号、所属学院、校园卡号、身份类型）"
    )]
    pub async fn auth_get_profile(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<szu_core::auth::AuthStore>()
            .ok_or("认证存储不可用")?;
        let profile = szu_core::auth::service::get_account_profile(&store)?;
        let user_data = szu_core::auth::service::get_user_data(&store)?;
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
        tokio::task::spawn_blocking(move || szu_core::auth::service::refresh_auth_session(app))
            .await
            .map_err(|_| "任务执行失败".to_string())??;
        Ok("认证会话刷新成功".to_string())
    }
}
