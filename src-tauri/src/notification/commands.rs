use super::settings::{self, NotificationSettings};
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn get_notification_settings(app: AppHandle) -> NotificationSettings {
    settings::load(&app)
}

#[tauri::command]
pub(crate) fn save_notification_settings(
    app: AppHandle,
    settings: NotificationSettings,
) -> Result<NotificationSettings, String> {
    settings::save(&app, &settings)
}

#[tauri::command]
pub(crate) fn set_application_focus(focused: bool) {
    super::set_foreground_focused(focused);
}
