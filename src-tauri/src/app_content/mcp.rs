use crate::app_content::AppContentStore;
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;
use tauri::Manager;

#[tool_router(router = app_content_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "app_get_announcements",
        description = "获取软件公告、电费/OJ提醒。每条公告可携带 id、mode（silent 静默展示 / popup 弹窗一次 / alert 每次启动弹出）与 conditions.ver 版本限定（如 \">=0.11.0, <0.12.0\"）；conditions 或 ver 缺省表示全局公告。返回结果已按当前应用版本过滤并完成降级规范化。"
    )]
    pub async fn app_get_announcements(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<AppContentStore>()
            .ok_or("公共内容存储不可用")?;
        let snapshot = store.snapshot();
        serde_json::to_string_pretty(&snapshot.announcements)
            .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "app_get_contributors",
        description = "获取your-szu-life开源项目的作者与社区贡献者列表"
    )]
    pub async fn app_get_contributors(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<AppContentStore>()
            .ok_or("公共内容存储不可用")?
            .inner()
            .clone();
        let contributors = tokio::task::spawn_blocking(move || store.get_contributors())
            .await
            .map_err(|_| "任务执行失败".to_string())?;
        serde_json::to_string_pretty(&contributors).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "app_check_update",
        description = "检查软件版本更新状态及最新版本信息"
    )]
    pub async fn app_check_update(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<AppContentStore>()
            .ok_or("公共内容存储不可用")?;
        let snapshot = store.snapshot();
        serde_json::to_string_pretty(&serde_json::json!({
            "currentVersion": snapshot.version,
            "updateAvailable": snapshot.update_available,
            "update": snapshot.update,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }
}
