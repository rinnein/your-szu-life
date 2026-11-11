use super::set_enabled;
use crate::campus_network::model::DebugModeView;
use tauri::State;

#[tauri::command]
pub(crate) fn set_demo_mode(
    app: tauri::AppHandle,
    store: State<'_, crate::campus_network::CampusNetworkStore>,
    enabled: bool,
) -> Result<DebugModeView, String> {
    set_enabled(&app, &store, enabled)?;
    Ok(crate::campus_network::diagnostics::mode_view(&store))
}
