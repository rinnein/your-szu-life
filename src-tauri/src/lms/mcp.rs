use crate::lms::service;
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;

#[tool_router(router = lms_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "lms_get_todos",
        description = "获取畅课（lms.szu.edu.cn）当前课程待办列表，包含课程名、任务标题、截止时间与任务页面链接构造所需的 course_id 与任务 id"
    )]
    pub async fn lms_get_todos(&self) -> Result<String, String> {
        let service = service(&self.app)?;
        service.ensure(false).await;
        let snapshot = service.snapshot();
        serde_json::to_string_pretty(&serde_json::json!({
            "session": snapshot.session,
            "message": snapshot.message,
            "todos": snapshot.todos,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "lms_get_notifications",
        description = "获取畅课（lms.szu.edu.cn）最近站内通知（作业截止提醒、资料发布等）与未读数"
    )]
    pub async fn lms_get_notifications(&self) -> Result<String, String> {
        let service = service(&self.app)?;
        service.ensure(false).await;
        let snapshot = service.snapshot();
        serde_json::to_string_pretty(&serde_json::json!({
            "session": snapshot.session,
            "message": snapshot.message,
            "unread_count": snapshot.unread_count,
            "notifications": snapshot.notifications,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }
}
