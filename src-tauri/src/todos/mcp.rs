use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::TodoKind;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListTodosParams {
    /// 可选：只返回某一类待办（oj-quiz、uooc-exam、uooc-homework、uooc-course-end、
    /// lms-homework、electricity-outage）。省略时返回全部。
    pub kind: Option<TodoKind>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TodoSettingsUpdate {
    /// 待办提醒总开关
    pub enabled: Option<bool>,
    /// OJ 测验截止提醒
    pub oj_quiz: Option<bool>,
    /// UOOC 考试截止提醒
    pub uooc_exam: Option<bool>,
    /// UOOC 作业截止提醒
    pub uooc_homework: Option<bool>,
    /// UOOC 课程结课提醒
    pub uooc_course_end: Option<bool>,
    /// 畅课作业截止提醒
    pub lms_homework: Option<bool>,
    /// 电费预估断电日期提醒
    pub electricity_outage: Option<bool>,
    /// 提前量（分钟），传空数组表示不提醒
    pub lead_minutes: Option<Vec<i64>>,
}

impl TodoSettingsUpdate {
    pub(crate) fn apply(
        self,
        target: &mut crate::notification::settings::TodoNotificationSettings,
    ) {
        if let Some(value) = self.enabled {
            target.enabled = value;
        }
        if let Some(value) = self.oj_quiz {
            target.oj_quiz = value;
        }
        if let Some(value) = self.uooc_exam {
            target.uooc_exam = value;
        }
        if let Some(value) = self.uooc_homework {
            target.uooc_homework = value;
        }
        if let Some(value) = self.uooc_course_end {
            target.uooc_course_end = value;
        }
        if let Some(value) = self.lms_homework {
            target.lms_homework = value;
        }
        if let Some(value) = self.electricity_outage {
            target.electricity_outage = value;
        }
        if let Some(value) = self.lead_minutes {
            target.lead_minutes = value;
        }
    }
}

#[tool_router(router = todos_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "todos_list",
        description = "获取首页日历聚合后的全部待办事项（OJ 测验、UOOC 考试/作业/结课、畅课作业、电费预估断电）。对应资源 your-szu-life://data/todos"
    )]
    pub async fn todos_list(
        &self,
        Parameters(params): Parameters<ListTodosParams>,
    ) -> Result<String, String> {
        let mut items = super::aggregate::aggregate(&self.app);
        if let Some(kind) = params.kind {
            items.retain(|item| item.kind == kind);
        }
        serde_json::to_string_pretty(&items).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "todos_get_settings",
        description = "获取待办提醒配置（六个注册项开关与提前量列表）。对应资源 your-szu-life://data/todos/notification-settings"
    )]
    pub async fn todos_get_settings(&self) -> Result<String, String> {
        let settings = crate::notification::settings::load(&self.app);
        serde_json::to_string_pretty(&settings.todo).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "todos_set_settings",
        description = "修改待办提醒配置。修改后将持久化，并在下一次提醒判定时生效。对应资源 your-szu-life://data/todos/notification-settings"
    )]
    pub async fn todos_set_settings(
        &self,
        Parameters(params): Parameters<TodoSettingsUpdate>,
    ) -> Result<String, String> {
        let mut settings = crate::notification::settings::load(&self.app);
        params.apply(&mut settings.todo);
        let saved = crate::notification::settings::save(&self.app, &settings)?;
        serde_json::to_string_pretty(&saved.todo).map_err(|e| format!("序列化失败: {e}"))
    }
}
