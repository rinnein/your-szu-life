use crate::enrollment::{Action, Query, Store};
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct SearchCoursesParams {
    /// 检索关键词（如课程名称、教师姓名、课程代码）
    pub keyword: String,
    /// 分类（默认 TJKC：推荐课程，或 FANKC：方案课程等）
    pub category: Option<String>,
    /// 分页页码，从 0 开始
    pub page: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct AcademicRecordParams {
    /// 是否强制从教务系统重新拉取并重算绩点
    pub recalculate: Option<bool>,
}

#[tool_router(router = enrollment_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "enrollment_get_snapshot",
        description = "获取深大选课系统整体状态快照（当前开放选课批次、个人学分限制、抢课监控任务列表）"
    )]
    pub async fn enrollment_get_snapshot(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Store>()
            .ok_or("选课服务存储不可用")?;
        let service = store.0.clone();
        let value = tokio::task::spawn_blocking(move || {
            service.execute(Action::Snapshot).map_err(|e| e.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "enrollment_get_selected_courses",
        description = "获取学生当前学期已成功选上的所有课程明细与授课教师、上课地点及学分"
    )]
    pub async fn enrollment_get_selected_courses(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Store>()
            .ok_or("选课服务存储不可用")?;
        let service = store.0.clone();
        let value = tokio::task::spawn_blocking(move || {
            service.execute(Action::Selected).map_err(|e| e.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "enrollment_search_courses",
        description = "检索全校可选开课信息（包括课程名称、课程代码、授课教师、校区、容量与余量）"
    )]
    pub async fn enrollment_search_courses(
        &self,
        Parameters(params): Parameters<SearchCoursesParams>,
    ) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Store>()
            .ok_or("选课服务存储不可用")?;
        let service = store.0.clone();
        let query = Query {
            category: params.category.unwrap_or_else(|| "TJKC".to_string()),
            keyword: params.keyword,
            page: params.page.unwrap_or(0),
        };
        let value = tokio::task::spawn_blocking(move || {
            service.execute(Action::Cache { query }).map_err(|e| e.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "enrollment_get_academic_record",
        description = "提取本科教务系统成绩档案、历年所修课程成绩及当前平均学分绩点（GPA）"
    )]
    pub async fn enrollment_get_academic_record(
        &self,
        Parameters(params): Parameters<AcademicRecordParams>,
    ) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Store>()
            .ok_or("教务服务存储不可用")?;
        let service = store.0.clone();
        let recalculate = params.recalculate.unwrap_or(false);
        let value = tokio::task::spawn_blocking(move || {
            service.execute(Action::Academic { recalculate }).map_err(|e| e.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "enrollment_get_academic_progress",
        description = "分析本科毕业学分修读完成度（各类别必修/选修学分要求、已修学分与尚欠学分）"
    )]
    pub async fn enrollment_get_academic_progress(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Store>()
            .ok_or("教务服务存储不可用")?;
        let service = store.0.clone();
        let value = tokio::task::spawn_blocking(move || {
            service.execute(Action::AcademicProgress).map_err(|e| e.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败: {e}"))
    }
}
