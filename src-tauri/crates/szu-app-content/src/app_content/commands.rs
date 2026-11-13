use super::{
    AppContentStore,
    model::{AppContentSnapshot, ContentError, ContributorsSnapshot, ErrorKind},
};
use tauri::State;

#[tauri::command]
pub fn get_app_content_snapshot(store: State<'_, AppContentStore>) -> AppContentSnapshot {
    store.0.snapshot()
}

#[tauri::command]
pub async fn refresh_app_content(
    store: State<'_, AppContentStore>,
) -> Result<AppContentSnapshot, String> {
    let service = store.0.clone();
    tauri::async_runtime::spawn_blocking(move || service.refresh())
        .await
        .map_err(|_| "公共内容检查任务未能完成".to_owned())
}

#[tauri::command]
pub async fn set_update_channel(
    store: State<'_, AppContentStore>,
    channel: super::model::UpdateChannel,
) -> Result<AppContentSnapshot, String> {
    // 先落盘再切内存：持久化失败时界面状态保持不变。
    super::channel::save(channel)?;
    let service = store.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.set_channel(channel);
        service.refresh()
    })
    .await
    .map_err(|_| "更新频道切换任务未能完成".to_owned())
}

#[tauri::command]
pub async fn get_contributors(
    store: State<'_, AppContentStore>,
) -> Result<ContributorsSnapshot, String> {
    let service = store.0.clone();
    tauri::async_runtime::spawn_blocking(move || service.get_contributors())
        .await
        .map_err(|_| "贡献者加载任务未能完成".to_owned())
}

/// 前端资源加载策略：
/// - `announcement`：公告/更新日志等可信内容源，仅允许当前公告图片，允许内网地址；
/// - `image`：任意第三方公网图片（题库、更新日志、头像等），拒绝内网/回环地址。
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteResourceKind {
    Announcement,
    Image,
}

#[tauri::command]
pub async fn load_remote_resource(
    store: State<'_, AppContentStore>,
    url: String,
    kind: RemoteResourceKind,
) -> Result<tauri::ipc::Response, ContentError> {
    let service = store.0.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || match kind {
        RemoteResourceKind::Announcement => service.load_image(&url),
        RemoteResourceKind::Image => service.load_public_image(&url),
    })
    .await
    .map_err(|_| ContentError::new(ErrorKind::Unavailable, "图片加载任务未能完成"))??;
    // Raw bytes reach the frontend as an ArrayBuffer, so it can build a Blob URL
    // without a base64 round trip.
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn start_in_app_update(
    store: State<'_, AppContentStore>,
    on_progress: tauri::ipc::Channel<super::model::UpdateProgressPayload>,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        super::android_update::start_update(&store, on_progress)
    }
    #[cfg(target_os = "windows")]
    {
        super::windows_update::start_update(&store, on_progress)
    }
    #[cfg(not(any(target_os = "android", target_os = "windows")))]
    {
        let _ = (store, on_progress);
        Err("当前平台不支持应用内直接下载更新".to_string())
    }
}
