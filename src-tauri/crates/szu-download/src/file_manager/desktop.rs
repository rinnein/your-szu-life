//! 桌面端目录选择：调用系统原生文件夹选择器。

use super::{paths, DirectorySelection, PickedDirectory};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

pub async fn pick(app: &AppHandle) -> Result<Option<PickedDirectory>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut builder = app.dialog().file();
    if let Ok(default) = super::default_base_dir(app) {
        builder = builder.set_directory(default);
    }
    // 打开的是系统原生选择器（Windows IFileDialog / Linux xdg-desktop-portal）。
    builder.pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let folder = rx
        .await
        .map_err(|_| "文件夹选择器已关闭".to_string())?;
    let Some(folder) = folder else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|error| format!("所选目录无法解析：{error}"))?;
    if !path.is_dir() {
        return Err(format!("所选路径不是目录：{}", path.display()));
    }
    let has_entries = paths::has_entries(&path)?;
    let display = path.to_string_lossy().into_owned();
    Ok(Some(PickedDirectory {
        selection: DirectorySelection::Filesystem {
            path: display.clone(),
        },
        display,
        has_entries,
    }))
}
