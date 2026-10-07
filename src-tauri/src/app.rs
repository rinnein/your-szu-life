use crate::{
    app_content, auth, campus_network, campus_payments, oj,
    storage::{PlatformCredentialStore, SharedCredentialStore},
    uooc,
};
use std::sync::Arc;
use tauri::Manager;

pub(crate) fn run() {
    let credentials: SharedCredentialStore = Arc::new(PlatformCredentialStore);
    let electricity_credentials = credentials.clone();
    let academic_credentials = credentials.clone();
    let builder = tauri::Builder::default()
        .manage(auth::AuthStore::new(credentials.clone()))
        .manage(uooc::UoocStore::new(credentials.clone()))
        .manage(oj::OjStore::new(credentials.clone()))
        .manage(campus_network::CampusNetworkStore::new(credentials))
        .manage(campus_payments::CampusPaymentsStore::new(
            electricity_credentials,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(crate::notification::init())
        .setup(move |app| {
            crate::storage::initialize(app.handle())?;
            crate::enrollment::initialize(app.handle(), academic_credentials.clone())?;
            if let Err(error) = oj::initialize(app.handle()) {
                eprintln!("OJ 初始化失败: {error}");
            }
            if let Err(error) = uooc::initialize(app.handle()) {
                eprintln!("UOOC 服务初始化失败: {error}");
            }
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
            crate::enrollment::commands::enrollment_command,
            crate::enrollment::commands::open_academic_portal,
            crate::enrollment::commands::open_enrollment_official,
            oj::commands::load_oj_image,
            oj::commands::save_oj_preferences,
            oj::commands::configure_oj_companion,
            oj::commands::set_oj_companion_status,
            oj::commands::activate_oj_companion,
            oj::commands::submit_oj_companion,
            oj::commands::get_oj_snapshot,
            oj::commands::save_oj_site,
            oj::commands::remove_oj_site,
            oj::commands::login_oj,
            oj::commands::get_oj_captcha,
            oj::commands::get_oj_groups,
            oj::commands::get_oj_contests,
            oj::commands::get_oj_paper,
            oj::commands::save_oj_draft,
            oj::commands::get_oj_draft,
            oj::commands::submit_oj,
            oj::commands::get_oj_records,
            oj::commands::get_oj_result,
            oj::commands::acknowledge_oj_submission,
            oj::commands::send_oj_companion,
            oj::commands::refresh_oj,
            oj::commands::read_oj_notice,
            crate::storage::commands::get_credential_storage_status,
            uooc::commands::get_uooc_snapshot,
            uooc::commands::get_uooc_ai_config,
            uooc::commands::save_uooc_ai_config,
            uooc::commands::list_uooc_ai_models,
            uooc::commands::save_uooc_quiz_mode,
            uooc::commands::save_uooc_quiz_answers,
            uooc::commands::control_uooc_quiz,
            uooc::commands::open_uooc_quiz_portal,
            uooc::commands::refresh_uooc_progress,
            uooc::commands::start_uooc_checkin,
            uooc::commands::cancel_uooc_checkin,
            uooc::commands::refresh_uooc_session,
            uooc::commands::open_uooc_login,
            uooc::commands::logout_uooc,
            uooc::commands::save_uooc_settings,
            uooc::commands::start_uooc_courses,
            uooc::commands::control_uooc_tasks,
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
            campus_payments::commands::get_electricity_constants,
            campus_payments::commands::export_electricity_settings,
            campus_payments::commands::import_electricity_settings,
            campus_payments::commands::query_electricity_recharges,
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
            crate::enrollment::portal::on_page_load(window, payload);
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
            std::thread::spawn(move || {
                campus_payments::window_closed(&app, &label);
                uooc::window_closed(&app, &label);
            });
        }
        #[cfg(desktop)]
        crate::desktop::on_window_event(window, event);
    });
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
