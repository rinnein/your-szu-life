use super::OcrModelStatus;

#[tauri::command]
pub(crate) fn get_ocr_model_status() -> Result<OcrModelStatus, String> {
    super::status()
}

#[tauri::command]
pub(crate) async fn download_ocr_model(
    app: tauri::AppHandle,
    on_event: tauri::ipc::Channel<crate::local_model::ModelDownloadChannelEvent>,
) -> Result<OcrModelStatus, String> {
    super::download_command(app, Some(on_event), false).await
}

#[tauri::command]
pub(crate) async fn update_ocr_model(
    app: tauri::AppHandle,
    on_event: tauri::ipc::Channel<crate::local_model::ModelDownloadChannelEvent>,
) -> Result<OcrModelStatus, String> {
    super::download_command(app, Some(on_event), true).await
}
