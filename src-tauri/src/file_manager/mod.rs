//! 统一文件目录：决定用户可直接获取/管理的文件落到哪里。
//!
//! 目录由初始化向导的 `storage` 步骤与「设置 → 储存」共同管理，存于
//! `settings.sqlite`（设备相关，不随设置包导出）。业务模块只通过
//! [`download_target`] 取得下载落点，不再各自决定路径，也不需要判断是否中转。
//!
//! 平台差异：
//! - 桌面端用 `tauri-plugin-dialog` 选目录，aria2 直接写入最终目录。
//! - Android 用 SAF（`ACTION_OPEN_DOCUMENT_TREE`）选目录。若设备允许按真实
//!   路径写，同样直接下载；否则先写应用标准缓存目录，再由 aria2 下载层统一
//!   复制到文档树（见 `crate::aria2::DownloadTarget::publish`）。

use crate::aria2::{DownloadTarget, PublishTarget};
use model::{
    DirectoryConfig, DirectoryPreview, DirectorySelection, FileManagerState, ManagedModule,
    ModuleDirectory, StorageAccess,
};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
#[cfg(not(target_os = "android"))]
use tauri::Manager;

pub(crate) mod android;
pub(crate) mod commands;
#[cfg(not(target_os = "android"))]
mod desktop;
pub(crate) mod mcp;
pub(crate) mod model;
pub(crate) mod paths;
#[cfg(test)]
mod tests;

/// Android 上用于下载中转的缓存子目录名。
#[cfg(target_os = "android")]
pub(crate) const STAGING_DIR: &str = ".staging";

/// 选择器返回、尚未落库的结果。
pub(crate) struct PickedDirectory {
    pub selection: DirectorySelection,
    /// 所选目录的展示路径。
    pub display: String,
    /// 目录内是否已有其它内容。
    pub has_entries: bool,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// 平台默认父目录（已包含「泥深」）。
#[cfg(target_os = "android")]
pub(crate) fn default_base_dir(_app: &AppHandle) -> Result<PathBuf, String> {
    // SAF 负责授权与写入，这里只是选择器的初始位置与展示值。
    Ok(PathBuf::from("/storage/emulated/0/Download").join(paths::BRAND_DIR))
}

/// 平台默认父目录（已包含「泥深」）。
#[cfg(not(target_os = "android"))]
pub(crate) fn default_base_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let documents = app
        .path()
        .document_dir()
        .or_else(|_| app.path().home_dir().map(|home| home.join("Documents")))
        .map_err(|_| "无法确定文档目录".to_string())?;
    Ok(documents.join(paths::BRAND_DIR))
}

/// 配置是否适用于当前平台。把 Android 的 SAF 配置带到桌面端（或反过来）时
/// 视为未配置，避免下载落到一个当前平台根本写不了的位置。
#[cfg(target_os = "android")]
fn config_matches_platform(config: &DirectoryConfig) -> bool {
    config.selection.android_tree().is_some()
}

#[cfg(not(target_os = "android"))]
fn config_matches_platform(config: &DirectoryConfig) -> bool {
    config.selection.android_tree().is_none()
}

pub(crate) fn load_config() -> Option<DirectoryConfig> {
    match crate::preferences::load::<DirectoryConfig>(crate::preferences::FILE_DIRECTORY) {
        Ok(Some(config)) if config_matches_platform(&config) => Some(config),
        Ok(Some(config)) => {
            log::warn!("文件目录配置来自其它平台（{}），本次忽略", config.selected);
            None
        }
        Ok(None) => None,
        Err(error) => {
            // 配置损坏时退回默认目录，而不是让下载与设置页一起失败。
            log::warn!("文件目录配置无法解析，已退回默认目录: {error}");
            None
        }
    }
}

fn module_dirs(base_display: &str) -> Vec<ModuleDirectory> {
    ManagedModule::ALL
        .iter()
        .map(|module| ModuleDirectory {
            id: module.id().to_string(),
            label: module.label().to_string(),
            path: paths::join_display(base_display, module.id()),
        })
        .collect()
}

#[cfg(target_os = "android")]
fn access_state(config: Option<&DirectoryConfig>) -> StorageAccess {
    let Some(uri) = config.and_then(|config| config.selection.android_tree()) else {
        return StorageAccess {
            state: "denied".to_string(),
            detail: Some("尚未选择保存目录".to_string()),
        };
    };
    match android::check_access(uri) {
        Ok(true) => StorageAccess {
            state: "granted".to_string(),
            detail: None,
        },
        Ok(false) => StorageAccess {
            state: "denied".to_string(),
            detail: Some("系统文件夹授权已失效，请重新选择保存目录".to_string()),
        },
        Err(error) => StorageAccess {
            state: "denied".to_string(),
            detail: Some(error),
        },
    }
}

#[cfg(not(target_os = "android"))]
fn access_state(_config: Option<&DirectoryConfig>) -> StorageAccess {
    StorageAccess {
        state: "notApplicable".to_string(),
        detail: Some("桌面端使用系统文件系统权限".to_string()),
    }
}

pub(crate) fn state(app: &AppHandle) -> Result<FileManagerState, String> {
    let config = load_config();
    let default_base = default_base_dir(app)?.to_string_lossy().into_owned();
    let base = config
        .as_ref()
        .map(|config| config.display_root.clone())
        .unwrap_or_else(|| default_base.clone());
    Ok(FileManagerState {
        platform: platform().to_string(),
        default_base,
        access: access_state(config.as_ref()),
        config: config.clone(),
        using_default: config.is_none(),
        modules: module_dirs(&base),
        base,
        can_use_default: !cfg!(target_os = "android"),
    })
}

fn preview_from(picked: PickedDirectory) -> DirectoryPreview {
    let android = picked.selection.android_tree().is_some();
    let base = if android {
        let relative = if picked.has_entries {
            paths::BRAND_DIR
        } else {
            ""
        };
        if relative.is_empty() {
            picked.display.clone()
        } else {
            paths::join_display(&picked.display, relative)
        }
    } else {
        let base = paths::resolve_local_base(Path::new(&picked.display), picked.has_entries);
        base.to_string_lossy().into_owned()
    };
    DirectoryPreview {
        modules: module_dirs(&base),
        notice: picked.has_entries.then(|| {
            "所选目录已有其它内容，文件将统一放在其中的「泥深」子目录。".to_string()
        }),
        selected_was_empty: !picked.has_entries,
        selection: picked.selection,
        selected: picked.display,
        base,
    }
}

/// 打开原生文件夹选择器并返回预览；用户取消时返回 `None`。
pub(crate) async fn pick_directory(app: &AppHandle) -> Result<Option<DirectoryPreview>, String> {
    #[cfg(target_os = "android")]
    let picked = android::pick(app).await?;
    #[cfg(not(target_os = "android"))]
    let picked = desktop::pick(app).await?;
    Ok(picked.map(preview_from))
}

/// 恢复平台默认目录（仅桌面）；Android 必须由用户在系统选择器中授权。
#[cfg(not(target_os = "android"))]
pub(crate) fn use_default_directory(app: &AppHandle) -> Result<DirectoryPreview, String> {
    let base = default_base_dir(app)?.to_string_lossy().into_owned();
    Ok(DirectoryPreview {
        modules: module_dirs(&base),
        notice: None,
        // 默认目录本身就包含「泥深」，不再套空目录规则。
        selected_was_empty: true,
        selection: DirectorySelection::Default,
        selected: base.clone(),
        base,
    })
}

#[cfg(target_os = "android")]
pub(crate) fn use_default_directory(_app: &AppHandle) -> Result<DirectoryPreview, String> {
    Err("Android 上需要由系统文件夹选择器授权目录".to_string())
}

#[cfg(target_os = "android")]
fn prepare_config(
    _app: &AppHandle,
    selection: &DirectorySelection,
) -> Result<DirectoryConfig, String> {
    let DirectorySelection::AndroidTree { uri } = selection else {
        return Err("请通过系统文件夹选择器选择保存目录".to_string());
    };
    let picked = android::inspect(uri)?;
    if !android::check_access(uri)? {
        return Err("系统文件夹授权已失效，请重新选择保存目录".to_string());
    }
    let relative_base = if picked.has_entries {
        paths::BRAND_DIR.to_string()
    } else {
        String::new()
    };
    android::ensure_directory(uri, &relative_base)?;
    let display_root = if relative_base.is_empty() {
        picked.display_path.clone()
    } else {
        paths::join_display(&picked.display_path, &relative_base)
    };
    Ok(DirectoryConfig {
        selection: selection.clone(),
        relative_base,
        display_root,
        selected: picked.display_path,
        via_default: false,
        chosen_at: now_ms(),
    })
}

#[cfg(not(target_os = "android"))]
fn prepare_config(
    app: &AppHandle,
    selection: &DirectorySelection,
) -> Result<DirectoryConfig, String> {
    let (base, selected, via_default) = match selection {
        DirectorySelection::Filesystem { path } => {
            let candidate = PathBuf::from(path);
            if !candidate.is_absolute() {
                return Err("请选择绝对路径目录".to_string());
            }
            if !candidate.is_dir() {
                return Err(format!("目录不存在：{}", candidate.display()));
            }
            let has_entries = paths::has_entries(&candidate)?;
            (
                paths::resolve_local_base(&candidate, has_entries),
                path.clone(),
                false,
            )
        }
        DirectorySelection::Default => {
            let base = default_base_dir(app)?;
            (base.clone(), base.to_string_lossy().into_owned(), true)
        }
        DirectorySelection::AndroidTree { .. } => {
            return Err("当前平台不支持 SAF 目录".to_string());
        }
    };
    for module in ManagedModule::ALL {
        paths::probe_writable(&base.join(module.id()))?;
    }
    Ok(DirectoryConfig {
        selection: selection.clone(),
        relative_base: String::new(),
        display_root: base.to_string_lossy().into_owned(),
        selected,
        via_default,
        chosen_at: now_ms(),
    })
}

/// 确认并落库；成功后广播 `file-manager-changed`。
pub(crate) fn save_directory(
    app: &AppHandle,
    selection: DirectorySelection,
) -> Result<FileManagerState, String> {
    let config = prepare_config(app, &selection)?;
    crate::preferences::save(crate::preferences::FILE_DIRECTORY, &config)?;
    let state = state(app)?;
    let _ = app.emit("file-manager-changed", &state);
    Ok(state)
}

/// 用户在确认框中取消时释放 SAF 持久授权，避免授权条目泄漏。
pub(crate) fn release_pick(selection: &DirectorySelection) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        if let Some(uri) = selection.android_tree() {
            return android::release(uri);
        }
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = selection;
        Ok(())
    }
}

#[cfg(target_os = "android")]
fn ensure_android_access(uri: &str) -> Result<(), String> {
    if android::check_access(uri)? {
        Ok(())
    } else {
        Err("系统文件夹授权已失效，请到「设置 → 储存」重新选择保存目录".to_string())
    }
}

#[cfg(target_os = "android")]
fn download_target_impl(
    _app: &AppHandle,
    module: ManagedModule,
    task_id: &str,
    segments: &[&str],
) -> Result<DownloadTarget, String> {
    let Some(config) = load_config() else {
        return Err("请先在初始化设置或「设置 → 储存」中选择文件保存目录".to_string());
    };
    let uri = config
        .selection
        .android_tree()
        .ok_or_else(|| "保存目录无效，请重新选择".to_string())?
        .to_string();
    let display_dir = paths::join_display_segments(
        &paths::join_display(&config.display_root, module.id()),
        segments,
    );
    let direct_dir = paths::join_local(
        &PathBuf::from(&config.display_root).join(module.id()),
        segments,
    );
    // 设备允许按真实路径写时（例如 Android 10 及更早的写权限），直接下载到
    // 用户选定目录，省掉复制与额外的缓存占用。
    if paths::probe_writable(&direct_dir).is_ok() {
        return Ok(DownloadTarget::local(direct_dir, display_dir));
    }
    ensure_android_access(&uri)?;
    let staging_root = android::staging_root()?
        .join(STAGING_DIR)
        .join(module.id())
        .join(paths::sanitize_segment(task_id));
    let work_dir = paths::join_local(&staging_root, segments);
    let mut relative = vec![module.id()];
    relative.extend(segments.iter().copied().filter(|segment| !segment.is_empty()));
    Ok(DownloadTarget {
        work_dir,
        display_dir,
        publish: Some(PublishTarget {
            tree_uri: uri,
            relative_dir: paths::join_relative(&config.relative_base, &relative),
            staging_root,
        }),
    })
}

#[cfg(not(target_os = "android"))]
fn download_target_impl(
    app: &AppHandle,
    module: ManagedModule,
    _task_id: &str,
    segments: &[&str],
) -> Result<DownloadTarget, String> {
    let base = match load_config() {
        Some(config) => PathBuf::from(config.display_root),
        None => default_base_dir(app)?,
    };
    let dir = paths::join_local(&base.join(module.id()), segments);
    let display = dir.to_string_lossy().into_owned();
    Ok(DownloadTarget::local(dir, display))
}

/// 构造某个模块的下载落点。是否中转、是否复制完全由本模块与 aria2 下载层决定，
/// 调用方只把它塞进下载任务即可。
pub(crate) fn download_target(
    app: &AppHandle,
    module: ManagedModule,
    task_id: &str,
    segments: &[&str],
) -> Result<DownloadTarget, String> {
    download_target_impl(app, module, task_id, segments)
}

/// 需要 SAF 复制时，取回断点续传所需的最小文件集合。
///
/// 只处理当前下载对象：目标目录里存在 `<文件名>.aria2` 时才把控制文件与可能
/// 只下载了一部分的 `<文件名>` 取回暂存目录；没有控制文件说明目标文件不是中断
/// 的下载，不复制任何数据。
pub(crate) fn seed_staging(publish: &PublishTarget, staging_file: &Path) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let file_name = staging_file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| "暂存文件名无效".to_string())?;
        let control = staging_file.with_file_name(format!("{file_name}.aria2"));
        // 本地已经有续传状态时不再从目标目录取回。
        if staging_file.is_file() || control.is_file() {
            return Ok(());
        }
        let destination = staging_file
            .parent()
            .ok_or_else(|| "暂存目录无效".to_string())?;
        std::fs::create_dir_all(destination)
            .map_err(|error| format!("创建暂存目录失败：{error}"))?;
        android::export_resume_state(
            &publish.tree_uri,
            &publish.relative_dir,
            &file_name,
            destination,
        )?;
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (publish, staging_file);
    }
    Ok(())
}

/// 暂存目录里需要发布的文件（排除 aria2 控制文件）。
#[cfg(target_os = "android")]
fn publishable_files(work_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(work_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (!name.ends_with(".aria2")).then_some(name)
        })
        .collect()
}

/// 把暂存目录里的文件复制到最终目录，并在复制前确认主文件完整。
/// 桌面端是 no-op（暂存目录就是最终目录）。
pub(crate) fn publish_staging(
    publish: &PublishTarget,
    work_dir: &Path,
    main_file: &str,
    expected_size: Option<u64>,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let files = publishable_files(work_dir);
        if files.is_empty() {
            return Err("暂存目录中没有可保存的文件".to_string());
        }
        if let Some(expected) = expected_size {
            let actual = std::fs::metadata(work_dir.join(main_file))
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if actual != expected {
                return Err(format!(
                    "下载文件大小不匹配（预期 {expected} 字节，实际 {actual} 字节），未保存"
                ));
            }
        }
        for file in files {
            let source = work_dir.join(&file);
            if !source.is_file() {
                continue;
            }
            android::publish_file(&publish.tree_uri, &publish.relative_dir, &source, &file)?;
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (publish, work_dir, main_file, expected_size);
    }
    Ok(())
}

/// 清理该任务的暂存目录。
pub(crate) fn clear_staging(publish: &PublishTarget) {
    if let Err(error) = std::fs::remove_dir_all(&publish.staging_root) {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!(
                "清理暂存目录失败 {}: {error}",
                publish.staging_root.display()
            );
        }
    }
}

/// 删除任务保存的文件，统一处理本地直写与 SAF 已发布两种情况。
pub(crate) fn delete_saved_files(target: &DownloadTarget, files: &[String]) {
    if let Some(publish) = &target.publish {
        #[cfg(target_os = "android")]
        {
            for file in files {
                let _ = android::delete_file(&publish.tree_uri, &publish.relative_dir, file);
            }
        }
        clear_staging(publish);
    } else {
        for file in files {
            let _ = std::fs::remove_file(target.work_dir.join(file));
            let _ = std::fs::remove_file(target.work_dir.join(format!("{file}.aria2")));
        }
    }
}
