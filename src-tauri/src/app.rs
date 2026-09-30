use crate::{
    app_content, auth, campus_network, campus_payments,
    storage::{PlatformCredentialStore, SharedCredentialStore},
};
use std::sync::Arc;
use tauri::Manager;

pub(crate) fn run() {
    let credentials: SharedCredentialStore = Arc::new(PlatformCredentialStore);
    let electricity_credentials = credentials.clone();
    let builder = tauri::Builder::default()
        .manage(auth::AuthStore::new(credentials.clone()))
        .manage(campus_network::CampusNetworkStore::new(credentials))
        .manage(campus_payments::CampusPaymentsStore::new(
            electricity_credentials,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[cfg(mobile)]
            {
                use tauri_plugin_notification::{Action, ActionType, NotificationExt};
                app.notification()
                    .register_action_types(vec![ActionType::builder("electricity-low")
                        .actions(vec![Action::builder("open-recharge", "打开充电费")
                            .foreground(true)
                            .build()])
                        .build()])?;
            }
            if let Err(error) = app_content::initialize(app.handle()) {
                eprintln!("应用内容后台服务未启动，将在下次打开时重试: {error}");
            }
            if let Some(store) = app.try_state::<campus_network::CampusNetworkStore>() {
                campus_network::diagnostics::initialize(app.handle(), &store);
            }
            if let Some(store) = app.try_state::<campus_payments::CampusPaymentsStore>() {
                campus_payments::electricity::initialize(
                    app.handle().clone(),
                    store.electricity.clone(),
                );
            }
            campus_network::monitor::start_dorm_monitor(app.handle().clone());
            #[cfg(desktop)]
            crate::desktop::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_content::commands::get_app_content_snapshot,
            app_content::commands::refresh_app_content,
            app_content::commands::get_contributors,
            app_content::commands::load_announcement_image,
            auth::commands::capture_auth_credentials,
            auth::commands::control_auth_recovery,
            auth::commands::get_auth_state,
            auth::commands::refresh_auth_session,
            auth::commands::get_account_profile,
            auth::commands::get_user_data,
            auth::commands::confirm_profile,
            auth::commands::begin_authentication,
            auth::commands::logout,
            campus_payments::commands::open_network_fee_portal,
            campus_payments::commands::consume_network_fee_portal_notice,
            campus_payments::commands::get_electricity_options,
            campus_payments::commands::get_electricity_settings,
            campus_payments::commands::save_electricity_settings,
            campus_payments::commands::query_electricity,
            campus_payments::commands::get_electricity_status,
            campus_payments::commands::set_application_visibility,
            campus_network::commands::get_network_overview,
            campus_network::commands::refresh_campus_network,
            campus_network::commands::request_campus_network_permissions,
            campus_network::commands::get_debug_mode,
            campus_network::commands::set_debug_mode,
            campus_network::commands::get_debug_logs,
            campus_network::commands::clear_debug_logs,
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
        .on_page_load(|window, payload| {
            if !campus_payments::on_page_load(window, payload) && window.label() == "main" {
                auth::webview::on_page_load(window, payload);
            }
        });
    let builder = builder.on_window_event(|window, event| {
        if window.label() == "main"
            && matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            )
        {
            auth::webview::close_authentication_window(window.app_handle());
        }
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let app = window.app_handle().clone();
            let label = window.label().to_owned();
            std::thread::spawn(move || campus_payments::window_closed(&app, &label));
        }
        #[cfg(desktop)]
        crate::desktop::on_window_event(window, event);
    });
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
