use szu_app_content::app_content;
use szu_core::{
    auth, campus_network, campus_payments, demo, netproxy, onboarding, settings,
    storage::{PlatformCredentialStore, SharedCredentialStore},
};
use szu_download::file_manager;
use szu_jxxk::jxxk;
use szu_lms::lms;
use szu_oj::oj;
use szu_uooc::uooc;
use std::sync::Arc;
use tauri::Manager;

pub(crate) fn run() {
    let credentials: SharedCredentialStore = Arc::new(PlatformCredentialStore);
    let electricity_credentials = credentials.clone();
    let academic_credentials = credentials.clone();
    let proxy_credentials = credentials.clone();
    let sync_credentials = credentials.clone();
    let (daily_writer, file_filter) = szu_core::logging::create_daily_writer();
    let file_dispatch = tauri_plugin_log::fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                szu_core::logging::timestamp_now(),
                record.target(),
                record.level(),
                message
            ))
        })
        .chain(tauri_plugin_log::fern::Output::writer(
            Box::new(daily_writer),
            "\n",
        ));

    let builder = tauri::Builder::default();
    let builder = builder
        .manage(auth::AuthStore::new(credentials.clone()))
        .manage(uooc::UoocStore::new(credentials.clone()))
        .manage(oj::OjStore::new(credentials.clone()))
        .manage(lms::LmsStore::new(credentials.clone()))
        .manage(jxxk::JxxkStore::new(credentials.clone()))
        .manage(campus_network::CampusNetworkStore::new(credentials))
        .manage(campus_payments::CampusPaymentsStore::new(
            electricity_credentials,
        ))
        .manage(settings::SettingsStore::new(Arc::new(
            PlatformCredentialStore,
        )))
        .manage(Arc::new(szu_mesh::mesh::MeshStore::new()))
        .manage(Arc::new(szu_mesh::settings_sync::engine::SyncStore::new()))
        .manage(netproxy::ProxyStore::new(proxy_credentials));
    // 必须最先注册：重复启动的进程在这里把参数交给已运行实例后退出。
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        crate::desktop::focus_main_window(app);
    }));

    // 桌面端文件夹选择器。Android 由 SAF 桥负责，不注册该插件。
    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(szu_core::notification::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_os::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    // 控制台/系统日志只收 Warn 及以上：库的 DEBUG/TRACE 会把真机
                    // logcat 刷到每秒上万行（Android 还会按每进程配额丢弃）。
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout)
                        .filter(szu_core::logging::quiet_targets),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Dispatch(
                        file_dispatch,
                    ))
                    .filter(file_filter),
                    // WebView 目标每转发一条日志都要过一次 JNI。若放行 `jni` 自己的
                    // 调试日志，「转发 → JNI 调用 → jni 日志 → 再转发」会互相放大，
                    // 真机实测几秒内就把主线程压到 ANR。因此这里同样只收 Warn 及以上。
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview)
                        .filter(szu_core::logging::quiet_targets),
                ])
                // 全局仍放行 Trace，好让文件目标按用户选择记录 DEBUG/TRACE 的应用日志；
                // 噪声库单独关掉。
                .level(log::LevelFilter::Trace)
                .level_for("jni", log::LevelFilter::Off)
                .level_for("tao", log::LevelFilter::Off)
                .level_for("wry", log::LevelFilter::Off)
                .level_for("tungstenite", log::LevelFilter::Off)
                .level_for("tokio_tungstenite", log::LevelFilter::Off)
                .level_for("cookie_store", log::LevelFilter::Off)
                .level_for("reqwest_cookie_store", log::LevelFilter::Off)
                .level_for("hyper", log::LevelFilter::Warn)
                .level_for("h2", log::LevelFilter::Warn)
                .level_for("rustls", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_aptabase::Builder::new(szu_core::analytics::app_key()).build())
        .setup(move |app| {
            // 日志级别本身也存于 settings.sqlite，必须先打开设置库再初始化日志。
            if let Err(error) = szu_core::preferences::initialize(app.handle()) {
                eprintln!("设置数据库初始化失败，本次使用默认设置: {error}");
            }
            // 旧的内部存储日志/模型要先搬到新位置，再初始化日志与模型服务。
            szu_core::migrations::storage::migrate_legacy_storage(app.handle());
            szu_core::logging::initialize(app.handle());
            if szu_core::analytics::app_key().is_empty() {
                log::info!("未配置 Aptabase App Key，匿名统计已关闭");
            } else {
                szu_core::analytics::track(app.handle(), "app_started", None);
            }
            // 端口分配器的通知需要在任意调用点推送到前端，先把句柄存下来。
            szu_core::ports::initialize(app.handle());
            if let Err(error) = szu_core::storage::initialize(app.handle()) {
                log::error!("凭据存储初始化失败: {error}");
            }
            // 必须在任何 HTTP 客户端被构建之前：代理设置以进程级缓存决定所有出网策略。
            if let Err(error) = szu_core::netproxy::initialize(app.handle()) {
                log::error!("代理设置初始化失败，本次运行按「自动」处理: {error}");
            }
            if let Err(error) = szu_runtime::ocr::initialize(app.handle()) {
                log::error!("OCR 模型服务初始化失败: {error}");
            }
            if let Err(error) = szu_runtime::embedding_model::initialize(app.handle()) {
                log::error!("Embedding 模型服务初始化失败: {error}");
            }
            // 本地模型来自 `szu-runtime`，core 不能反向依赖它们，因此就绪状态由这里探测后传入。
            let local_models_ready = szu_runtime::ocr::status()
                .is_ok_and(|status| status.status == "ready")
                && szu_runtime::embedding_model::status()
                    .is_ok_and(|status| status.status == "ready");
            if let Err(error) = szu_core::onboarding::initialize(app.handle(), local_models_ready) {
                log::warn!("初始化设置状态迁移失败，将在下次启动重试: {error}");
            }
            auth::webview::setup_login_injection(app.handle())?;
            szu_enrollment::enrollment::initialize(app.handle(), academic_credentials.clone())?;
            if let Err(error) = oj::initialize(app.handle()) {
                log::error!("OJ 初始化失败: {error}");
            }
            if let Err(error) = uooc::initialize(app.handle()) {
                log::error!("UOOC 服务初始化失败: {error}");
            }
            if let Err(error) = lms::initialize(app.handle()) {
                log::error!("畅课服务初始化失败: {error}");
            }
            if let Err(error) = szu_todos::todos::initialize(app.handle()) {
                log::error!("待办服务初始化失败: {error}");
            }
            if let Err(error) = jxxk::initialize(app.handle()) {
                log::error!("巡课服务初始化失败: {error}");
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
                demo::initialize(&store);

                if let Ok(overview) = campus_network::service::get_network_overview(&store) {
                    netproxy::set_in_campus_network(
                        overview.region != campus_network::model::NetworkRegion::None,
                    );
                }
            }
            if let Some(store) = app.try_state::<campus_payments::CampusPaymentsStore>() {
                campus_payments::electricity::initialize(
                    app.handle().clone(),
                    store.electricity.clone(),
                );
            }
            campus_network::monitor::start_dorm_monitor(app.handle().clone());
            #[cfg(desktop)]
            {
                crate::desktop::setup(app)?;
                // 待办快照变化时刷新托盘菜单。`szu-todos` 是独立 crate，不能反向
                // 依赖 desktop，因此由 app 层注册回调。
                szu_todos::todos::set_tray_updater(crate::desktop::update_todo_menu);
            }
            if let Err(error) = crate::mcp::initialize(app.handle()) {
                log::error!("MCP 本地服务启动失败: {error}");
            }
            szu_mesh::mesh::initialize(app.handle());
            szu_mesh::settings_sync::initialize(app.handle(), sync_credentials);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::mcp::get_mcp_server_status,
            crate::mcp::set_mcp_server_enabled,
            szu_mesh::mesh::commands::get_mesh_status,
            szu_mesh::mesh::commands::list_mesh_devices,
            szu_mesh::mesh::commands::set_mesh_enabled,
            szu_mesh::mesh::commands::set_mesh_campus_access,
            szu_mesh::mesh::commands::set_mesh_device_name,
            szu_mesh::settings_sync::commands::get_sync_status,
            szu_mesh::settings_sync::commands::get_sync_items,
            szu_mesh::settings_sync::commands::trigger_sync,
            szu_mesh::settings_sync::commands::logout_uooc_with_mode,
            szu_mesh::settings_sync::commands::set_sync_item_enabled,
            szu_core::ports::commands::take_port_notices,
            szu_enrollment::enrollment::commands::enrollment_command,
            szu_enrollment::enrollment::commands::open_academic_portal,
            szu_enrollment::enrollment::commands::open_enrollment_official,
            szu_enrollment::enrollment::commands::subscribe_enrollment_state,
            szu_enrollment::enrollment::commands::watch_enrollment_online_users,
            szu_enrollment::enrollment::commands::recalculate_academic,
            szu_runtime::ocr::commands::get_ocr_model_status,
            szu_runtime::ocr::commands::download_ocr_model,
            szu_runtime::ocr::commands::update_ocr_model,
            szu_runtime::embedding_model::commands::get_embedding_model_status,
            szu_runtime::embedding_model::commands::download_embedding_model,
            szu_runtime::embedding_model::commands::update_embedding_model,
            oj::commands::load_oj_image,
            oj::commands::save_oj_preferences,
            oj::commands::configure_oj_companion,
            oj::commands::set_oj_companion_status,
            oj::commands::activate_oj_companion,
            oj::commands::submit_oj_companion,
            oj::commands::get_oj_snapshot,
            oj::commands::subscribe_oj_state,
            oj::commands::subscribe_oj_deadlines,
            oj::commands::subscribe_oj_lists,
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
            oj::commands::watch_oj_result,
            oj::commands::acknowledge_oj_submission,
            oj::commands::send_oj_companion,
            oj::commands::refresh_oj,
            oj::commands::read_oj_notice,
            szu_core::storage::commands::get_credential_storage_status,
            settings::commands::get_settings_snapshot,
            szu_core::activity::commands::set_active_module,
            szu_core::appearance::commands::get_appearance,
            szu_core::appearance::commands::save_appearance,
            szu_core::appearance::commands::get_wallpaper_seed,
            szu_core::appearance::commands::get_wallpaper_image,
            szu_core::appearance::commands::watch_wallpaper_image,
            szu_core::notification::commands::get_notification_settings,
            szu_core::notification::commands::save_notification_settings,
            szu_core::notification::commands::set_application_focus,
            szu_core::analytics::commands::get_analytics_settings,
            szu_core::analytics::commands::save_analytics_settings,
            szu_todos::todos::commands::subscribe_todos,
            szu_todos::todos::commands::refresh_todos,
            szu_core::netproxy::commands::get_proxy_settings,
            szu_core::netproxy::commands::save_proxy_settings,
            szu_core::netproxy::commands::test_proxy_settings,
            uooc::commands::get_uooc_snapshot,
            uooc::commands::subscribe_uooc_state,
            uooc::commands::subscribe_uooc_assessments,
            uooc::commands::refresh_uooc_assessments,
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
            uooc::commands::watch_uooc_qa_device_auth,
            uooc::commands::upload_all_uooc_qa_bank,
            uooc::commands::list_uooc_exams,
            uooc::commands::get_uooc_exam_paper,
            uooc::commands::save_uooc_exam_draft,
            uooc::commands::submit_uooc_exam,
            uooc::commands::heartbeat_uooc_exam,
            uooc::commands::start_uooc_exam_heartbeat,
            uooc::commands::stop_uooc_exam_heartbeat,
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
            app_content::commands::set_update_channel,
            app_content::commands::get_contributors,
            app_content::commands::load_remote_resource,
            app_content::commands::start_in_app_update,
            auth::commands::capture_auth_credentials,
            auth::commands::control_auth_recovery,
            auth::commands::get_auth_state,
            auth::commands::refresh_auth_session,
            auth::commands::get_account_profile,
            auth::commands::get_user_data,
            auth::commands::confirm_profile,
            auth::commands::begin_authentication,
            auth::commands::begin_setup_authentication,
            auth::commands::cancel_authentication,
            auth::commands::consume_auth_failure_notice,
            auth::commands::logout,
            lms::commands::get_lms_snapshot,
            lms::commands::subscribe_lms_state,
            lms::commands::refresh_lms,
            jxxk::commands::get_jxxk_snapshot,
            jxxk::commands::subscribe_jxxk_state,
            jxxk::commands::refresh_jxxk,
            jxxk::commands::jxxk_list_courses,
            jxxk::commands::jxxk_list_lessons,
            jxxk::commands::jxxk_lesson_streams,
            jxxk::commands::jxxk_play_urls,
            jxxk::commands::jxxk_download_urls,
            jxxk::commands::jxxk_live_stream,
            jxxk::commands::jxxk_download,
            jxxk::commands::jxxk_download_list,
            jxxk::commands::jxxk_download_pause,
            jxxk::commands::jxxk_download_resume,
            jxxk::commands::jxxk_download_remove,
            jxxk::commands::jxxk_fetch_media,
            jxxk::commands::jxxk_subtitle,
            jxxk::commands::subscribe_jxxk_downloads,
            onboarding::commands::get_onboarding_state,
            onboarding::commands::complete_onboarding_step,
            onboarding::commands::skip_onboarding_step,
            onboarding::commands::restart_onboarding,
            file_manager::commands::get_file_manager_state,
            file_manager::commands::pick_file_manager_directory,
            file_manager::commands::use_default_file_manager_directory,
            file_manager::commands::save_file_manager_directory,
            file_manager::commands::release_file_manager_pick,
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
            demo::commands::set_demo_mode,
            campus_network::commands::get_log_info,
            campus_network::commands::open_log_file,
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
            szu_enrollment::enrollment::portal::on_page_load(window, payload);
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
    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            szu_core::analytics::track(app_handle, "app_exited", None);
            szu_core::analytics::flush(app_handle);
        }
    });
}
