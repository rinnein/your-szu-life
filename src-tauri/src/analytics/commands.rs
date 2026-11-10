use super::{load, save, AnalyticsSettings};
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn get_analytics_settings() -> AnalyticsSettings {
    load()
}

#[tauri::command]
pub(crate) fn save_analytics_settings(
    app: AppHandle,
    settings: AnalyticsSettings,
) -> Result<AnalyticsSettings, String> {
    save(&app, &settings)
}
