use tauri::AppHandle;

use super::StartupSettings;

/// 读取开机自启与静默启动偏好。
#[tauri::command]
pub fn get_startup_settings() -> Result<StartupSettings, String> {
    super::load()
}

/// 保存并立即应用。系统条目写入失败时不落盘，返回错误供前端回滚。
#[tauri::command]
pub fn save_startup_settings(
    app: AppHandle,
    settings: StartupSettings,
) -> Result<StartupSettings, String> {
    super::save(&app, &settings)
}
