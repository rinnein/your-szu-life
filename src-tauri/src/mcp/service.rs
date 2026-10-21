use tauri::AppHandle;

/// 深圳大学生活助手统一 MCP 服务结构体
/// 持有 Tauri AppHandle，供各功能模块的 Tool 与 Resource 直接读取应用内实时状态与会话。
#[derive(Clone)]
pub struct UnifiedSzuMcpService {
    pub app: AppHandle,
}

impl UnifiedSzuMcpService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}
