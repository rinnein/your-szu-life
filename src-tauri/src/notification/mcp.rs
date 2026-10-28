use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SetNotificationSettingsParams {
    /// 全局总开关
    pub enabled: Option<bool>,
    /// 校园网异常通知
    pub campus_network: Option<bool>,
    /// 宿舍低电量预警
    pub electricity: Option<bool>,
    /// 选课任务状态通知
    pub enrollment: Option<bool>,
    /// OJ 测验与截止通知
    pub oj: Option<bool>,
    /// UOOC 测验需要处理提醒
    pub uooc: Option<bool>,
}

#[tool_router(router = notification_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "notification_get_settings",
        description = "获取应用通知配置（总开关及校园网、电费、选课、OJ、UOOC 分类开关）。对应资源 your-szu-life://data/notification/settings"
    )]
    pub async fn notification_get_settings(&self) -> Result<String, String> {
        let settings = crate::notification::settings::load(&self.app);
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "notification_set_settings",
        description = "修改应用通知配置。修改后将持久化并在前台后台生效。对应资源 your-szu-life://data/notification/settings"
    )]
    pub async fn notification_set_settings(
        &self,
        Parameters(params): Parameters<SetNotificationSettingsParams>,
    ) -> Result<String, String> {
        let mut current = crate::notification::settings::load(&self.app);
        if let Some(enabled) = params.enabled {
            current.enabled = enabled;
        }
        if let Some(v) = params.campus_network {
            current.campus_network = v;
        }
        if let Some(v) = params.electricity {
            current.electricity = v;
        }
        if let Some(v) = params.enrollment {
            current.enrollment = v;
        }
        if let Some(v) = params.oj {
            current.oj = v;
        }
        if let Some(v) = params.uooc {
            current.uooc = v;
        }
        let saved = crate::notification::settings::save(&self.app, &current)?;
        serde_json::to_string_pretty(&saved).map_err(|e| format!("序列化失败: {e}"))
    }
}
