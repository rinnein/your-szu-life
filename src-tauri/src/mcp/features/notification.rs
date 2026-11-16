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
    /// 选课任务状态通知
    pub enrollment: Option<bool>,
    /// UOOC 刷课需要人工处理提醒
    pub uooc: Option<bool>,
    /// 待办类通知（七个注册项开关、初次探测开关与提前量）
    pub todo: Option<crate::mcp::features::todos::TodoSettingsUpdate>,
}

#[tool_router(router = notification_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "notification_get_settings",
        description = "获取应用通知配置（总开关、校园网、选课、UOOC 分类开关与待办提醒配置）。对应资源 your-szu-life://data/notification/settings"
    )]
    pub async fn notification_get_settings(&self) -> Result<String, String> {
        let settings = szu_core::notification::settings::load(&self.app);
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
        let mut current = szu_core::notification::settings::load(&self.app);
        if let Some(enabled) = params.enabled {
            current.enabled = enabled;
        }
        if let Some(v) = params.campus_network {
            current.campus_network = v;
        }
        if let Some(v) = params.enrollment {
            current.enrollment = v;
        }
        if let Some(v) = params.uooc {
            current.uooc = v;
        }
        if let Some(todo) = params.todo {
            todo.apply(&mut current.todo);
        }
        let saved = szu_core::notification::settings::save(&self.app, &current)?;
        serde_json::to_string_pretty(&saved).map_err(|e| format!("序列化失败: {e}"))
    }
}
