use super::EmbeddingModelStatus;

#[tauri::command]
pub(crate) fn get_embedding_model_status() -> Result<EmbeddingModelStatus, String> {
    super::status()
}

#[tauri::command]
pub(crate) async fn download_embedding_model(
    app: tauri::AppHandle,
) -> Result<EmbeddingModelStatus, String> {
    super::download_command(app, false).await
}

#[tauri::command]
pub(crate) async fn update_embedding_model(
    app: tauri::AppHandle,
) -> Result<EmbeddingModelStatus, String> {
    super::download_command(app, true).await
}
