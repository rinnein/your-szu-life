//! 应用持久文件在磁盘上的统一落点。
//!
//! Android 上日志、本地模型这类需要长期保留、又应当由系统随应用一起清理的
//! 文件放在应用专属外部目录（`/sdcard/Android/data/<applicationId>/files`）：
//! 不需要任何存储权限，卸载或清除数据时由系统一并删除。桌面沿用 Tauri 的
//! 应用目录约定。

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// 应用专属的持久文件根目录。
///
/// Android 优先使用应用专属外部目录；外部存储不可用时退回内部
/// `<dataDir>/files`（同样随卸载删除）。桌面使用 `app_data_dir()`。
pub fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        if let Some(root) = android_external_files_root(app) {
            return Ok(root);
        }
        return app
            .path()
            .app_data_dir()
            .map(|dir| dir.join("files"))
            .map_err(|_| "无法确定应用外部存储目录".to_string());
    }
    #[cfg(not(target_os = "android"))]
    {
        app.path()
            .app_data_dir()
            .map_err(|error| format!("无法确定应用数据目录：{error}"))
    }
}

/// 日志目录。
///
/// Android 跟随持久文件根目录；桌面保持系统日志目录约定（`app_log_dir()`）。
pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        return data_root(app).map(|root| root.join("logs"));
    }
    #[cfg(not(target_os = "android"))]
    {
        app.path()
            .app_log_dir()
            .map_err(|error| format!("无法确定日志目录：{error}"))
    }
}

/// 本地模型根目录；各模型再按自己的名字建子目录。
pub fn local_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    data_root(app).map(|root| root.join("local_model"))
}

/// Tauri 在 Android 上把 `document_dir` / `download_dir` 映射为
/// `getExternalFilesDir(DIRECTORY_DOCUMENTS / DIRECTORY_DOWNLOADS)`，两者都是
/// `<external-files>/<子目录>`，父目录即应用专属外部根目录。
#[cfg(target_os = "android")]
fn android_external_files_root(app: &AppHandle) -> Option<PathBuf> {
    [app.path().document_dir(), app.path().download_dir()]
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|dir| external_root(&dir))
}

#[cfg(any(target_os = "android", test))]
fn external_root(dir: &std::path::Path) -> Option<PathBuf> {
    dir.parent().map(std::path::Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_root_is_parent_of_tauri_subdirectory() {
        assert_eq!(
            external_root(std::path::Path::new(
                "/storage/emulated/0/Android/data/in.rinne.your_szu_life/files/Documents"
            )),
            Some(PathBuf::from(
                "/storage/emulated/0/Android/data/in.rinne.your_szu_life/files"
            ))
        );
        assert_eq!(external_root(std::path::Path::new("/")), None);
    }
}
