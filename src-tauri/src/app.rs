use crate::{
    auth, campus_network,
    storage::{PlatformCredentialStore, SharedCredentialStore},
};
use std::sync::Arc;

pub(crate) fn run() {
    let credentials: SharedCredentialStore = Arc::new(PlatformCredentialStore);
    let builder = tauri::Builder::default()
        .manage(auth::AuthStore::new(credentials.clone()))
        .manage(campus_network::CampusNetworkStore::new(credentials))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            campus_network::monitor::start_dorm_monitor(app.handle().clone());
            #[cfg(desktop)]
            crate::desktop::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth::commands::capture_auth_credentials,
            auth::commands::get_auth_state,
            auth::commands::get_account_profile,
            auth::commands::get_user_data,
            auth::commands::confirm_profile,
            auth::commands::begin_authentication,
            auth::commands::logout,
            campus_network::commands::get_network_overview,
            campus_network::commands::get_staff_network_settings,
            campus_network::commands::save_staff_network_settings,
            campus_network::commands::get_staff_login_status,
            campus_network::commands::apply_staff_network_mode,
            campus_network::commands::consume_staff_login_failure_notice,
            campus_network::commands::get_dorm_network_settings,
            campus_network::commands::save_dorm_network_settings,
            campus_network::commands::get_dorm_login_status,
            campus_network::commands::trigger_dorm_login,
            campus_network::commands::logout_campus_network,
            campus_network::commands::get_dorm_login_logs,
            campus_network::commands::consume_dorm_login_failure_notice,
        ])
        .on_page_load(auth::webview::on_page_load);
    #[cfg(desktop)]
    let builder = builder.on_window_event(crate::desktop::on_window_event);
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
