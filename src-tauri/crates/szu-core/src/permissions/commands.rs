use super::{PermissionId, PermissionSettingsTarget, PermissionStateView};

/// 读取当前系统权限状态；顺序与 id 由后端固定，前端只按 id 取用。
#[tauri::command]
pub fn get_permission_states() -> Result<Vec<PermissionStateView>, String> {
    super::states()
}

/// 申请一组权限，完成后返回最新状态，前端不需要再查一次。
#[tauri::command]
pub async fn request_permissions(
    keys: Vec<PermissionId>,
) -> Result<Vec<PermissionStateView>, String> {
    if !keys.is_empty() {
        super::request(&keys).await?;
    }
    super::states()
}

/// 打开与某一项权限相关的系统设置页。
#[tauri::command]
pub fn open_permission_settings(target: PermissionSettingsTarget) -> Result<(), String> {
    super::open_settings(target)
}
