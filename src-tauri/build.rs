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
        ]),
    ))
    .expect("failed to build Tauri ACL metadata");
}
