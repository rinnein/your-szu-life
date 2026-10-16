#[path = "build_support/ocr_model.rs"]
mod ocr_model;

use std::{collections::HashMap, env, path::PathBuf};

const CONTENT_ENV_VARS: [&str; 2] = ["SZU_LIFE_BASE_URL", "SZU_LIFE_FALLBACK_BASE_URL"];

fn load_dotenv(path: &std::path::Path) -> HashMap<String, String> {
    if !path.is_file() {
        return HashMap::new();
    }
    dotenvy::from_path_iter(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .collect::<Result<HashMap<_, _>, _>>()
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn configured_value(name: &str, dotenv: &HashMap<String, String>) -> Option<String> {
    match env::var_os(name) {
        Some(value) => value.into_string().ok(),
        None => dotenv.get(name).cloned(),
    }
}

fn main() {
    if env::var("SZU_LIFE_BUNDLE_OCR_MODEL").as_deref() == Ok("1") {
        ocr_model::stage_bundled().expect("failed to stage bundled OCR model");
    }
    let dotenv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.env");
    let dotenv = load_dotenv(&dotenv_path);
    println!("cargo:rerun-if-changed={}", dotenv_path.display());
    for name in CONTENT_ENV_VARS {
        if let Some(value) = configured_value(name, &dotenv) {
            println!("cargo:rustc-env={name}={value}");
        }
        println!("cargo:rerun-if-env-changed={name}");
    }

    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../github-accelerator.json");
    println!("cargo:rerun-if-changed=resources/ocr/ddddocr/model.json");
    println!("cargo:rerun-if-changed=capabilities/authserver-credential-capture.json");
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "enrollment_command",
            "open_academic_portal",
            "open_enrollment_official",
            "get_ocr_model_status",
            "download_ocr_model",
            "update_ocr_model",
            "get_embedding_model_status",
            "download_embedding_model",
            "update_embedding_model",
            "load_oj_image",
            "save_oj_preferences",
            "configure_oj_companion",
            "set_oj_companion_status",
            "activate_oj_companion",
            "submit_oj_companion",
            "get_oj_snapshot",
            "save_oj_site",
            "remove_oj_site",
            "login_oj",
            "get_oj_captcha",
            "get_oj_groups",
            "get_oj_contests",
            "get_oj_paper",
            "get_oj_progress",
            "get_oj_ranking",
            "save_oj_draft",
            "get_oj_draft",
            "submit_oj",
            "get_oj_records",
            "get_oj_result",
            "acknowledge_oj_submission",
            "send_oj_companion",
            "refresh_oj",
            "read_oj_notice",
            "get_uooc_snapshot",
            "get_uooc_ai_config",
            "get_uooc_ai_connections",
            "save_uooc_ai_connections",
            "save_uooc_ai_preferences",
            "list_uooc_ai_models",
            "save_uooc_quiz_mode",
            "save_uooc_quiz_answers",
            "control_uooc_quiz",
            "open_uooc_quiz_portal",
            "refresh_uooc_progress",
            "start_uooc_checkin",
            "cancel_uooc_checkin",
            "start_uooc_discussion",
            "refresh_uooc_session",
            "open_uooc_login",
            "logout_uooc",
            "save_uooc_settings",
            "start_uooc_courses",
            "control_uooc_tasks",
            "get_uooc_embedding_status",
            "download_uooc_qa_vectors",
            "search_uooc_qa_questions",
            "sync_uooc_qa_bank",
            "get_uooc_qa_account_status",
            "save_uooc_qa_api_key",
            "reset_uooc_qa_api_key",
            "get_app_content_snapshot",
            "refresh_app_content",
            "get_contributors",
            "load_announcement_image",
            "get_credential_storage_status",
            "get_settings_snapshot",
            "export_settings",
            "preview_import_settings",
            "import_settings",
            "get_auth_state",
            "refresh_auth_session",
            "get_account_profile",
            "get_user_data",
            "begin_authentication",
            "capture_auth_credentials",
            "control_auth_recovery",
            "confirm_profile",
            "logout",
            "open_network_fee_portal",
            "consume_network_fee_portal_notice",
            "get_electricity_options",
            "get_electricity_settings",
            "save_electricity_settings",
            "query_electricity",
            "get_electricity_status",
            "get_cached_electricity",
            "get_electricity_constants",
            "export_electricity_settings",
            "import_electricity_settings",
            "query_electricity_recharges",
            "set_application_visibility",
            "get_network_overview",
            "refresh_campus_network",
            "request_campus_network_permissions",
            "get_debug_mode",
            "get_debug_logs",
            "set_debug_mode",
            "clear_debug_logs",
            "get_staff_network_settings",
            "save_staff_network_settings",
            "get_staff_login_status",
            "apply_staff_network_mode",
            "consume_staff_login_failure_notice",
            "get_dorm_network_settings",
            "save_dorm_network_settings",
            "get_dorm_login_status",
            "trigger_dorm_login",
            "logout_campus_network",
            "get_dorm_login_logs",
            "consume_dorm_login_failure_notice",
        ]),
    ))
    .expect("failed to build Tauri ACL metadata");
}
