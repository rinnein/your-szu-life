use super::{ImportOptions, SettingsImportPreview, SettingsSnapshot, SettingsStore};
use tauri::State;

#[tauri::command]
pub(crate) fn get_settings_snapshot(
    state: State<'_, SettingsStore>,
) -> Result<SettingsSnapshot, String> {
    state.snapshot()
}

#[tauri::command]
pub(crate) fn export_settings(
    state: State<'_, SettingsStore>,
    password: String,
) -> Result<String, String> {
    state.export(&password)
}

#[tauri::command]
pub(crate) fn preview_import_settings(
    state: State<'_, SettingsStore>,
    text: String,
    password: String,
) -> Result<SettingsImportPreview, String> {
    state.preview(&text, &password)
}

#[tauri::command]
pub(crate) fn import_settings(
    state: State<'_, SettingsStore>,
    text: String,
    password: String,
    options: ImportOptions,
) -> Result<SettingsImportPreview, String> {
    state.import(&text, &password, options.overwrite)
}
