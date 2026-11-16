use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TimetableSettingsUpdate {
    /// 课前提醒（默认提前 20 分钟；Android 上显示常驻倒计时）
    pub class_reminder: Option<bool>,
    /// 签到提醒
    pub checkin: Option<bool>,
}

impl TimetableSettingsUpdate {
    pub(crate) fn apply(
        self,
        target: &mut szu_core::notification::settings::TimetableNotificationSettings,
    ) {
        if let Some(value) = self.class_reminder {
            target.class_reminder = value;
        }
        if let Some(value) = self.checkin {
            target.checkin = value;
        }
    }
}

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
    /// 待办类通知（六个注册项开关、初次探测开关与提前量）
    pub todo: Option<crate::mcp::features::todos::TodoSettingsUpdate>,
    /// 课表通知（课前提醒、签到提醒）
    pub timetable: Option<TimetableSettingsUpdate>,
}

#[tool_router(router = notification_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "notification_get_settings",
        description = "获取应用通知配置（总开关、校园网、选课、UOOC 分类开关、待办提醒与课表提醒配置）。对应资源 your-szu-life://data/notification/settings"
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
        if let Some(timetable) = params.timetable {
            timetable.apply(&mut current.timetable);
        }
        let saved = szu_core::notification::settings::save(&self.app, &current)?;
        serde_json::to_string_pretty(&saved).map_err(|e| format!("序列化失败: {e}"))
    }
}
