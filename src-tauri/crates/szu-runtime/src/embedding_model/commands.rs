use super::EmbeddingModelStatus;

#[tauri::command]
pub fn get_embedding_model_status() -> Result<EmbeddingModelStatus, String> {
    super::status()
}

#[tauri::command]
pub async fn download_embedding_model(
    app: tauri::AppHandle,
    on_event: tauri::ipc::Channel<crate::local_model::ModelDownloadChannelEvent>,
) -> Result<EmbeddingModelStatus, String> {
    super::download_command(app, Some(on_event), false).await
}

#[tauri::command]
pub async fn update_embedding_model(
    app: tauri::AppHandle,
    on_event: tauri::ipc::Channel<crate::local_model::ModelDownloadChannelEvent>,
) -> Result<EmbeddingModelStatus, String> {
    super::download_command(app, Some(on_event), true).await
}
