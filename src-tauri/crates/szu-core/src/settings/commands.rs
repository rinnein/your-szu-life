use super::{SettingsSnapshot, SettingsStore};
use tauri::State;

#[tauri::command]
pub fn get_settings_snapshot(
    state: State<'_, SettingsStore>,
) -> Result<SettingsSnapshot, String> {
    state.snapshot()
}
