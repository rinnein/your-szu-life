use crate::{
    app_content, auth, campus_network, campus_payments, oj, settings,
    storage::{PlatformCredentialStore, SharedCredentialStore},
    uooc,
};
use std::sync::Arc;
use tauri::Manager;

pub(crate) fn run() {
    let credentials: SharedCredentialStore = Arc::new(PlatformCredentialStore);
    let electricity_credentials = credentials.clone();
    let academic_credentials = credentials.clone();
    let (daily_writer, file_filter) = crate::logging::create_daily_writer();
    let file_dispatch = tauri_plugin_log::fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                crate::logging::timestamp_now(),
                record.target(),
                record.level(),
                message
            ))
        })
        .chain(tauri_plugin_log::fern::Output::writer(
            Box::new(daily_writer),
            "\n",
        ));

    let builder = tauri::Builder::default()
        .manage(auth::AuthStore::new(credentials.clone()))
        .manage(uooc::UoocStore::new(credentials.clone()))
        .manage(oj::OjStore::new(credentials.clone()))
        .manage(campus_network::CampusNetworkStore::new(credentials))
        .manage(campus_payments::CampusPaymentsStore::new(
            electricity_credentials,
        ))
        .manage(settings::SettingsStore::new(Arc::new(
            PlatformCredentialStore,
        )));
    // 必须最先注册：重复启动的进程在这里把参数交给已运行实例后退出。
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        crate::desktop::focus_main_window(app);
    }));

    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(crate::notification::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_os::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    // 控制台/系统日志只收 Info 及以上：库的 DEBUG/TRACE 会把真机
                    // logcat 刷到每秒上万行（Android 还会按每进程配额丢弃）。
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout)
                        .filter(crate::logging::quiet_targets),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Dispatch(
                        file_dispatch,
                    ))
                    .filter(file_filter),
                    // WebView 目标每转发一条日志都要过一次 JNI。若放行 `jni` 自己的
                    // 调试日志，「转发 → JNI 调用 → jni 日志 → 再转发」会互相放大，
                    // 真机实测几秒内就把主线程压到 ANR。因此这里同样只收 Info 及以上。
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview)
                        .filter(crate::logging::quiet_targets),
                ])
                // 全局仍放行 Trace，好让文件目标按用户选择记录 DEBUG/TRACE 的应用日志；
                // 噪声库单独关掉。
                .level(log::LevelFilter::Trace)
                .level_for("jni", log::LevelFilter::Off)
                .level_for("tao", log::LevelFilter::Off)
                .level_for("wry", log::LevelFilter::Off)
                .level_for("tungstenite", log::LevelFilter::Warn)
                .level_for("hyper", log::LevelFilter::Warn)
                .level_for("h2", log::LevelFilter::Warn)
                .level_for("rustls", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            crate::logging::initialize(app.handle());
            if let Err(error) = crate::storage::initialize(app.handle()) {
                log::error!("凭据存储初始化失败: {error}");
            }
            if let Err(error) = crate::ocr::initialize(app.handle()) {
                log::error!("OCR 模型服务初始化失败: {error}");
            }
            if let Err(error) = crate::embedding_model::initialize(app.handle()) {
                log::error!("Embedding 模型服务初始化失败: {error}");
            }
            auth::webview::setup_login_injection(app.handle())?;
            crate::enrollment::initialize(app.handle(), academic_credentials.clone())?;
            if let Err(error) = oj::initialize(app.handle()) {
                log::error!("OJ 初始化失败: {error}");
            }
            if let Err(error) = uooc::initialize(app.handle()) {
                log::error!("UOOC 服务初始化失败: {error}");
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
                log::warn!("应用内容后台服务未启动，将在下次打开时重试: {error}");
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
            if let Err(error) = crate::mcp::initialize(app.handle()) {
                log::error!("MCP 本地服务启动失败: {error}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::mcp::get_mcp_server_status,
            crate::mcp::set_mcp_server_enabled,
            crate::enrollment::commands::enrollment_command,
            crate::enrollment::commands::open_academic_portal,
            crate::enrollment::commands::open_enrollment_official,
            crate::enrollment::commands::subscribe_enrollment_state,
            crate::ocr::commands::get_ocr_model_status,
            crate::ocr::commands::download_ocr_model,
            crate::ocr::commands::update_ocr_model,
            crate::embedding_model::commands::get_embedding_model_status,
            crate::embedding_model::commands::download_embedding_model,
            crate::embedding_model::commands::update_embedding_model,
            oj::commands::load_oj_image,
            oj::commands::save_oj_preferences,
            oj::commands::configure_oj_companion,
            oj::commands::set_oj_companion_status,
            oj::commands::activate_oj_companion,
            oj::commands::submit_oj_companion,
            oj::commands::get_oj_snapshot,
            oj::commands::subscribe_oj_state,
            oj::commands::save_oj_site,
            oj::commands::remove_oj_site,
            oj::commands::login_oj,
            oj::commands::get_oj_captcha,
            oj::commands::auto_recognize_oj_captcha,
            oj::commands::get_oj_groups,
            oj::commands::get_oj_contests,
            oj::commands::get_oj_paper,
            oj::commands::get_oj_progress,
            oj::commands::get_oj_ranking,
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
            settings::commands::get_settings_snapshot,
            settings::commands::export_settings,
            settings::commands::preview_import_settings,
            settings::commands::import_settings,
            crate::appearance::commands::get_appearance,
            crate::appearance::commands::save_appearance,
            crate::appearance::commands::get_wallpaper_seed,
            crate::notification::commands::get_notification_settings,
            crate::notification::commands::save_notification_settings,
            crate::notification::commands::set_application_focus,
            uooc::commands::get_uooc_snapshot,
            uooc::commands::subscribe_uooc_state,
            uooc::commands::get_uooc_ai_config,
            uooc::commands::get_uooc_ai_connections,
            uooc::commands::save_uooc_ai_connections,
            uooc::commands::save_uooc_ai_preferences,
            uooc::commands::list_uooc_ai_models,
            uooc::commands::save_uooc_quiz_mode,
            uooc::commands::save_uooc_quiz_answers,
            uooc::commands::control_uooc_quiz,
            uooc::commands::open_uooc_quiz_portal,
            uooc::commands::refresh_uooc_progress,
            uooc::commands::start_uooc_checkin,
            uooc::commands::cancel_uooc_checkin,
            uooc::commands::start_uooc_discussion,
            uooc::commands::refresh_uooc_session,
            uooc::commands::open_uooc_login,
            uooc::commands::logout_uooc,
            uooc::commands::save_uooc_settings,
            uooc::commands::start_uooc_courses,
            uooc::commands::control_uooc_tasks,
            uooc::commands::get_uooc_embedding_status,
            uooc::commands::download_uooc_qa_vectors,
            uooc::commands::search_uooc_qa_questions,
            uooc::commands::sync_uooc_qa_bank,
            uooc::commands::get_uooc_qa_account_status,
            uooc::commands::save_uooc_qa_api_key,
            uooc::commands::reset_uooc_qa_api_key,
            uooc::commands::start_uooc_qa_device_auth,
            uooc::commands::poll_uooc_qa_device_auth,
            uooc::commands::upload_all_uooc_qa_bank,
            uooc::commands::list_uooc_exams,
            uooc::commands::get_uooc_exam_paper,
            uooc::commands::save_uooc_exam_draft,
            uooc::commands::submit_uooc_exam,
            uooc::commands::heartbeat_uooc_exam,
            uooc::commands::list_uooc_quizzes,
            uooc::commands::list_uooc_homeworks,
            uooc::commands::get_uooc_eval_list,
            uooc::commands::submit_uooc_eval,
            uooc::commands::generate_uooc_subjective,
            uooc::commands::open_uooc_task_portal,
            uooc::commands::match_uooc_paper_qa,
            uooc::commands::solve_uooc_paper_ai,
            uooc::commands::fetch_uooc_completed_quiz,
            uooc::commands::get_uooc_local_bank_questions,
            uooc::commands::get_uooc_local_bank_count,
            uooc::commands::clear_uooc_local_bank,
            uooc::commands::get_uooc_course_title,
            uooc::commands::build_uooc_local_bank,
            uooc::commands::uooc_qa_http_request,
            app_content::commands::get_app_content_snapshot,
            app_content::commands::refresh_app_content,
            app_content::commands::get_contributors,
            app_content::commands::load_announcement_image,
            app_content::commands::start_in_app_update,
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
            campus_payments::commands::get_cached_electricity,
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
            campus_network::commands::clear_debug_logs,
            campus_network::commands::get_log_info,
            campus_network::commands::set_log_level,
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
