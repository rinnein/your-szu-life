use crate::campus_network::{service, staff_login, CampusNetworkStore};
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct ApplyStaffLoginParams {
    /// 二次确认令牌（若需要跳过确认或提交已确认令牌）
    pub confirmation_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct GetDormLogsParams {
    /// 返回日志最大条数，默认 20 条
    pub limit: Option<usize>,
}

#[tool_router(router = network_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "network_get_overview",
        description = "获取当前校园网络连接状态总览，包含当前 IP、默认网关及所在区域（宿舍区/教学办公区/校外）"
    )]
    pub async fn network_get_overview(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<CampusNetworkStore>()
            .ok_or("校园网状态存储不可用")?;
        let overview = service::get_network_overview(&store)?;
        serde_json::to_string_pretty(&overview).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "network_trigger_dorm_login",
        description = "触发宿舍区 Dr.COM 校园网一键认证登录"
    )]
    pub async fn network_trigger_dorm_login(&self) -> Result<String, String> {
        service::trigger_dorm_login(self.app.clone())?;
        Ok("已触发宿舍网络登录任务".to_string())
    }

    #[tool(
        name = "network_logout",
        description = "立即主动断开并注销当前的校园网络在线连接"
    )]
    pub async fn network_logout(&self) -> Result<String, String> {
        let app = self.app.clone();
        let result = tokio::task::spawn_blocking(move || service::logout_campus_network(app))
            .await
            .map_err(|_| "任务执行失败".to_string())??;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "network_apply_staff_login",
        description = "应用并登录教学办公区 Portal 网络模式"
    )]
    pub async fn network_apply_staff_login(
        &self,
        Parameters(params): Parameters<ApplyStaffLoginParams>,
    ) -> Result<String, String> {
        let app = self.app.clone();
        let token = params.confirmation_token;
        let result = tokio::task::spawn_blocking(move || staff_login::apply(app, token))
            .await
            .map_err(|_| "任务执行失败".to_string())??;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "network_get_dorm_logs",
        description = "查询最近的宿舍网络认证与连接日志记录"
    )]
    pub async fn network_get_dorm_logs(
        &self,
        Parameters(params): Parameters<GetDormLogsParams>,
    ) -> Result<String, String> {
        let store = self
            .app
            .try_state::<CampusNetworkStore>()
            .ok_or("校园网状态存储不可用")?;
        let mut logs = service::get_dorm_login_logs(self.app.clone(), &store)?;
        if let Some(limit) = params.limit {
            if logs.len() > limit {
                logs.truncate(limit);
            }
        }
        serde_json::to_string_pretty(&logs).map_err(|e| format!("序列化失败: {e}"))
    }
}
