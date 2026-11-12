use std::sync::Arc;

use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;
use tauri::Manager;

#[tool_router(router = sync_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "sync_get_status",
        description = "获取设置同步状态：开关、最近一次同步的对端数量、应用/推送条数、题库行数与逐项状态。同步冲突由 Loro CRDT 自动合并，不存在冻结项。对应资源 your-szu-life://data/sync/status"
    )]
    pub async fn sync_get_status(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Arc<szu_mesh::settings_sync::engine::SyncStore>>()
            .ok_or_else(|| "设置同步模块未初始化".to_string())?;
        let view = szu_mesh::settings_sync::status(&self.app, &store).await;
        serde_json::to_string_pretty(&view).map_err(|error| format!("序列化失败: {error}"))
    }

    #[tool(
        name = "sync_list_items",
        description = "列出可同步项及其状态（已同步 / 等待对端 / 已停用 / 已全局退出）。同一 key 的并发修改由 Loro 自动合并。对应资源 your-szu-life://data/sync/items"
    )]
    pub async fn sync_list_items(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Arc<szu_mesh::settings_sync::engine::SyncStore>>()
            .ok_or_else(|| "设置同步模块未初始化".to_string())?;
        serde_json::to_string_pretty(&store.item_views(&self.app).await)
            .map_err(|error| format!("序列化失败: {error}"))
    }
}
