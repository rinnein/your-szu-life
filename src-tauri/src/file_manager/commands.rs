use super::model::{DirectoryPreview, DirectorySelection, FileManagerState};
use super::{pick_directory, release_pick, save_directory, state, use_default_directory};
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn get_file_manager_state(app: AppHandle) -> Result<FileManagerState, String> {
    state(&app)
}

/// 打开系统原生文件夹选择器。用户取消时返回 `None`。
#[tauri::command]
pub(crate) async fn pick_file_manager_directory(
    app: AppHandle,
) -> Result<Option<DirectoryPreview>, String> {
    pick_directory(&app).await
}

/// 恢复平台默认目录（仅桌面提供；Android 必须由用户在系统选择器中授权）。
#[tauri::command]
pub(crate) fn use_default_file_manager_directory(
    app: AppHandle,
) -> Result<DirectoryPreview, String> {
    use_default_directory(&app)
}

#[tauri::command]
pub(crate) fn save_file_manager_directory(
    app: AppHandle,
    selection: DirectorySelection,
) -> Result<FileManagerState, String> {
    save_directory(&app, selection)
}

/// 用户在确认框中取消时释放刚取得的 SAF 持久授权。
#[tauri::command]
pub(crate) fn release_file_manager_pick(selection: DirectorySelection) -> Result<(), String> {
    release_pick(&selection)
}
