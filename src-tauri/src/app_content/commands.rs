use super::{
    model::{AppContentSnapshot, ContentError, ContributorsSnapshot, ErrorKind},
    AppContentStore,
};
use tauri::State;

#[tauri::command]
pub(crate) fn get_app_content_snapshot(store: State<'_, AppContentStore>) -> AppContentSnapshot {
    store.0.snapshot()
}

#[tauri::command]
pub(crate) async fn refresh_app_content(
    store: State<'_, AppContentStore>,
) -> Result<AppContentSnapshot, String> {
    let service = store.0.clone();
    tauri::async_runtime::spawn_blocking(move || service.refresh())
        .await
        .map_err(|_| "公共内容检查任务未能完成".to_owned())
}

#[tauri::command]
pub(crate) async fn get_contributors(
    store: State<'_, AppContentStore>,
) -> Result<ContributorsSnapshot, String> {
    let service = store.0.clone();
    tauri::async_runtime::spawn_blocking(move || service.get_contributors())
        .await
        .map_err(|_| "贡献者加载任务未能完成".to_owned())
}

#[tauri::command]
pub(crate) async fn load_announcement_image(
    store: State<'_, AppContentStore>,
    url: String,
) -> Result<String, ContentError> {
    let service = store.0.clone();
    tauri::async_runtime::spawn_blocking(move || service.load_image(&url))
        .await
        .map_err(|_| ContentError::new(ErrorKind::Unavailable, "头图加载任务未能完成"))?
}

#[tauri::command]
pub(crate) fn start_in_app_update(
    store: State<'_, AppContentStore>,
) -> Result<(), String> {
    super::android_update::start_update(&store)
}
