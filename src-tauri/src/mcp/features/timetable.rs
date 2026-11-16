use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;

#[tool_router(router = timetable_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "timetable_get_snapshot",
        description = "获取课表快照：考勤课表的未来课次（日期/起止时间/楼栋教室）、待签点名、以及 eHall 固定学期课表（持久缓存）。对应资源 your-szu-life://data/timetable"
    )]
    pub async fn timetable_get_snapshot(&self) -> Result<String, String> {
        let snapshot = szu_timetable::timetable::snapshot(&self.app);
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("序列化失败: {e}"))
    }
}
