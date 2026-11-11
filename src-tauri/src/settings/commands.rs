use super::{SettingsSnapshot, SettingsStore};
use tauri::State;

#[tauri::command]
pub(crate) fn get_settings_snapshot(
    state: State<'_, SettingsStore>,
) -> Result<SettingsSnapshot, String> {
    state.snapshot()
}
