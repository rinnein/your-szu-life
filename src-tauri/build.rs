fn main() {
    println!("cargo:rerun-if-changed=capabilities/authserver-credential-capture.json");
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_auth_state",
            "get_account_profile",
            "get_user_data",
            "begin_authentication",
            "capture_auth_credentials",
            "confirm_profile",
            "logout",
            "get_network_overview",
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
