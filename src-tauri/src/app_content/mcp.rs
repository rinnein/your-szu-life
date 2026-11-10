use crate::app_content::AppContentStore;
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::model::UpdateChannel;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
pub(crate) struct SetUpdateChannelParams {
    /// 目标更新频道：stable = 稳定版（只接收正式 release，暂无正式版时会回退测试版）；
    /// canary = 测试版（跟随滚动 canary 构建）。
    pub channel: UpdateChannel,
}

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
        name = "app_check_update",
        description = "检查软件版本更新状态及最新版本信息。返回当前版本、更新频道（stable 稳定版 / canary 测试版）、稳定版是否回退到测试版、是否有新版本以及最新版本信息。对应资源 your-szu-life://data/app/update"
    )]
    pub async fn app_check_update(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<AppContentStore>()
            .ok_or("公共内容存储不可用")?;
        let snapshot = store.snapshot();
        serde_json::to_string_pretty(&serde_json::json!({
            "currentVersion": snapshot.version,
            "updateChannel": snapshot.update_channel,
            "updateChannelFallback": snapshot.update_channel_fallback,
            "updateAvailable": snapshot.update_available,
            "update": snapshot.update,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "app_set_update_channel",
        description = "切换应用内更新频道并立即重新检查一次。stable 只接收正式 release（暂无正式版时回退测试版），canary 跟随滚动测试版构建。返回切换后的内容快照。"
    )]
    pub async fn app_set_update_channel(
        &self,
        Parameters(params): Parameters<SetUpdateChannelParams>,
    ) -> Result<String, String> {
        let store = self
            .app
            .try_state::<AppContentStore>()
            .ok_or("公共内容存储不可用")?
            .inner()
            .clone();
        super::channel::save(params.channel)?;
        let snapshot = tokio::task::spawn_blocking(move || {
            store.0.set_channel(params.channel);
            store.0.refresh()
        })
        .await
        .map_err(|_| "任务执行失败".to_string())?;
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("序列化失败: {e}"))
    }
}
