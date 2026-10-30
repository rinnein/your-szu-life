use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const CONTENT_ENV_VARS: [&str; 2] = ["SZU_LIFE_BASE_URL", "SZU_LIFE_FALLBACK_BASE_URL"];

/// Tauri CLI 只把 `bundle.externalBin` 打进桌面端安装包，Android 侧不会处理它。
/// Android 的 `nativeLibraryDir` 只包含 APK 内 `lib/<abi>/` 的条目，所以这里把侧车
/// 二进制以 `libaria2c.so` 的命名放进 `jniLibs` 由 Gradle 打包。
/// 配合 `extractNativeLibs="true"` 与 `useLegacyPackaging = true`，安装时系统会将它
/// 解压到 `nativeLibraryDir`，运行时即可直接作为可执行程序启动。
const ARIA2C_ANDROID_LIB_NAME: &str = "libaria2c.so";

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

const WIN_32_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-win-32bit-build1.zip";
const WIN_64_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-win-64bit-build1.zip";
const ANDROID_AARCH64_URL: &str = "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-aarch64-linux-android-build1.zip";

fn prepare_aria2c_binary(manifest_dir: &Path) {
    let target = env::var("TARGET").unwrap_or_else(|_| "x86_64-unknown-linux-gnu".into());
    let binaries_dir = manifest_dir.join("binaries");
    fs::create_dir_all(&binaries_dir).expect("failed to create binaries dir");

    let is_windows = target.contains("windows");
    let ext = if is_windows { ".exe" } else { "" };
    let dest_name = format!("aria2c-{target}{ext}");
    let dest_path = binaries_dir.join(&dest_name);

    if dest_path.is_file() {
        if target.contains("android") {
            stage_android_sidecar_lib(manifest_dir, &target, &dest_path);
        }
        return;
    }

    if target.contains("linux") && !target.contains("android") {
        let system_paths = ["/usr/bin/aria2c", "/usr/local/bin/aria2c"];
        let mut found = None;
        for p in system_paths {
            if Path::new(p).is_file() {
                found = Some(PathBuf::from(p));
                break;
            }
        }
        if found.is_none() {
            if let Ok(output) = std::process::Command::new("which").arg("aria2c").output() {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path_str.is_empty() && Path::new(&path_str).is_file() {
                        found = Some(PathBuf::from(path_str));
                    }
                }
            }
        }

        if let Some(src) = found {
            fs::copy(&src, &dest_path)
                .unwrap_or_else(|e| panic!("failed to copy {}: {e}", src.display()));
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&dest_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_path, perms).unwrap();
            }
            return;
        } else {
            panic!("Cannot find aria2c on system. Please install aria2 via your package manager (e.g. sudo apt install aria2).");
        }
    }

    let download_url = if target.contains("windows") {
        if target.contains("i686") {
            WIN_32_URL
        } else {
            WIN_64_URL
        }
    } else if target.contains("aarch64") && target.contains("android") {
        ANDROID_AARCH64_URL
    } else {
        println!("cargo:warning=No official aria2c binary for target {target}, creating placeholder");
        let _ = File::create(&dest_path);
        return;
    };

    println!("cargo:warning=Downloading aria2c for {target} from {download_url}...");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("failed to create reqwest client");

    match client.get(download_url).send().and_then(|r| r.error_for_status()) {
        Ok(resp) => {
            let bytes = resp.bytes().expect("failed to read response bytes");
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("failed to read zip archive");
            let target_file_name = if is_windows { "aria2c.exe" } else { "aria2c" };
            let mut extracted = false;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).expect("zip entry error");
                if file.name().ends_with(target_file_name) {
                    let mut out = File::create(&dest_path).expect("failed to create dest file");
                    std::io::copy(&mut file, &mut out).expect("failed to write binary");
                    extracted = true;
                    break;
                }
            }
            if !extracted {
                panic!("binary {target_file_name} not found in zip from {download_url}");
            }
            #[cfg(unix)]
            if !is_windows {
                let mut perms = fs::metadata(&dest_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_path, perms).unwrap();
            }
        }
        Err(e) => {
            println!("cargo:warning=Failed to download aria2c for {target}: {e}");
            let _ = File::create(&dest_path);
        }
    }

    // 下载成功和下载失败（占位文件）都会走到这里；占位文件必须直接跳过，
    // 否则后面的 soname 补写会因为「不是有效 ELF」而让整个构建失败。
    if target.contains("android") && dest_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        stage_android_sidecar_lib(manifest_dir, &target, &dest_path);
    }
}

/// Android 的 ABI 目录名（`jniLibs/<abi>/`）与 Rust target 三元的对应关系。
fn android_abi_for_target(target: &str) -> Option<&'static str> {
    if target.starts_with("aarch64") {
        Some("arm64-v8a")
    } else if target.starts_with("armv7") {
        Some("armeabi-v7a")
    } else if target.starts_with("i686") {
        Some("x86")
    } else if target.starts_with("x86_64") {
        Some("x86_64")
    } else {
        None
    }
}

/// 侧车库的最终落点：`src-tauri/gen/android/app/src/main/jniLibs/<abi>/libaria2c.so`。
///
/// `app/src/main/jniLibs` 是 Android Gradle Plugin 默认就会合并进 APK 的
/// `lib/<abi>/` 目录，因此无需改动 Tauri 自动生成的 `app/build.gradle.kts`。
fn android_sidecar_lib_path(manifest_dir: &Path, target: &str) -> Option<PathBuf> {
    Some(
        manifest_dir
            .join("gen/android/app/src/main/jniLibs")
            .join(android_abi_for_target(target)?)
            .join(ARIA2C_ANDROID_LIB_NAME),
    )
}

fn stage_android_sidecar_lib(manifest_dir: &Path, target: &str, sidecar: &Path) {
    let Some(dest) = android_sidecar_lib_path(manifest_dir, target) else {
        println!(
            "cargo:warning=暂不支持为 {target} 打包 Android 侧车二进制，APK 内将不包含 {ARIA2C_ANDROID_LIB_NAME}"
        );
        return;
    };

    let Ok(source_meta) = sidecar.metadata() else {
        println!("cargo:warning=读取 {} 失败，跳过", sidecar.display());
        return;
    };
    if source_meta.len() == 0 {
        // 下载失败时 build.rs 只留下了空占位文件，说明当前离线/无网络，
        // 此时不写入残缺的库，宁可让运行时明确报「未找到 aria2c」。
        println!(
            "cargo:warning={} 是空占位文件，APK 内将不包含 {ARIA2C_ANDROID_LIB_NAME}",
            sidecar.display()
        );
        return;
    }

    // 目标比源文件新才复用，避免改了侧车源之后仍然打包旧内容。
    if let (Ok(dest_meta), Ok(src_time)) = (dest.metadata(), source_meta.modified()) {
        if dest_meta.len() == source_meta.len() && matches!(dest_meta.modified(), Ok(dest_time) if dest_time >= src_time) {
            return;
        }
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }

    fs::copy(sidecar, &dest).unwrap_or_else(|e| {
        panic!(
            "无法复制 Android 侧车二进制 {} 到 {}: {e}",
            sidecar.display(),
            dest.display()
        )
    });

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&dest).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).unwrap();
    }

    println!(
        "cargo:warning=已为 Android 准备侧车库 {}（来源 {}）",
        dest.display(),
        sidecar.display()
    );
    println!("cargo:rerun-if-changed={}", sidecar.display());
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dotenv_path = manifest_dir.join("../.env");
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
    println!("cargo:rerun-if-changed=../huggingface-accelerator.json");
    println!("cargo:rerun-if-changed=resources/ocr/ddddocr/model.json");
    println!("cargo:rerun-if-changed=resources/embedding/bge-small-zh-v1.5/model.json");
    println!("cargo:rerun-if-changed=capabilities/authserver-credential-capture.json");

    prepare_aria2c_binary(&manifest_dir);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_mcp_server_status",
            "set_mcp_server_enabled",
            "enrollment_command",
            "open_academic_portal",
            "open_enrollment_official",
            "subscribe_enrollment_state",
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
            "subscribe_oj_state",
            "get_lms_snapshot",
            "refresh_lms",
            "save_oj_site",
            "remove_oj_site",
            "login_oj",
            "get_oj_captcha",
            "auto_recognize_oj_captcha",
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
            "subscribe_uooc_state",
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
            "start_uooc_qa_device_auth",
            "poll_uooc_qa_device_auth",
            "upload_all_uooc_qa_bank",
            "list_uooc_exams",
            "get_uooc_exam_paper",
            "save_uooc_exam_draft",
            "submit_uooc_exam",
            "heartbeat_uooc_exam",
            "list_uooc_quizzes",
            "list_uooc_homeworks",
            "get_uooc_eval_list",
            "submit_uooc_eval",
            "generate_uooc_subjective",
            "open_uooc_task_portal",
            "match_uooc_paper_qa",
            "solve_uooc_paper_ai",
            "fetch_uooc_completed_quiz",
            "get_uooc_local_bank_questions",
            "get_uooc_local_bank_count",
            "clear_uooc_local_bank",
            "get_uooc_course_title",
            "build_uooc_local_bank",
            "uooc_qa_http_request",
            "get_app_content_snapshot",
            "refresh_app_content",
            "get_contributors",
            "load_announcement_image",
            "start_in_app_update",
            "get_credential_storage_status",
            "get_settings_snapshot",
            "export_settings",
            "preview_import_settings",
            "import_settings",
            "get_appearance",
            "save_appearance",
            "get_wallpaper_seed",
            "get_wallpaper_image",
            "get_notification_settings",
            "save_notification_settings",
            "set_application_focus",
            "get_proxy_settings",
            "save_proxy_settings",
            "test_proxy_settings",
            "get_auth_state",
            "refresh_auth_session",
            "get_account_profile",
            "get_user_data",
            "begin_authentication",
            "begin_setup_authentication",
            "cancel_authentication",
            "consume_auth_failure_notice",
            "capture_auth_credentials",
            "control_auth_recovery",
            "confirm_profile",
            "logout",
            "get_onboarding_state",
            "complete_onboarding_step",
            "skip_onboarding_step",
            "restart_onboarding",
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
            "set_debug_mode",
            "clear_debug_logs",
            "get_log_info",
            "set_log_level",
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
            "consume_dorm_login_failure_notice",
        ]),
    ))
    .expect("failed to build Tauri ACL metadata");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_abi_mapping_covers_all_shipped_abis() {
        assert_eq!(android_abi_for_target("aarch64-linux-android"), Some("arm64-v8a"));
        assert_eq!(android_abi_for_target("armv7-linux-androideabi"), Some("armeabi-v7a"));
        assert_eq!(android_abi_for_target("i686-linux-android"), Some("x86"));
        assert_eq!(android_abi_for_target("x86_64-linux-android"), Some("x86_64"));
        assert_eq!(android_abi_for_target("x86_64-unknown-linux-gnu"), None);
    }

    #[test]
    fn sidecar_lib_path_is_inside_main_jnilibs() {
        let path = android_sidecar_lib_path(Path::new("/app/src-tauri"), "aarch64-linux-android")
            .expect("android target must map to an ABI");
        assert!(
            path.ends_with("gen/android/app/src/main/jniLibs/arm64-v8a/libaria2c.so"),
            "unexpected sidecar path: {}",
            path.display()
        );
    }

    #[test]
    fn stage_android_sidecar_lib_copies_binary() {
        let temp_dir = std::env::temp_dir().join(format!("test-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let sidecar = temp_dir.join("aria2c-test");
        fs::write(&sidecar, b"mock-aria2c-content").unwrap();

        let manifest_dir = temp_dir.join("src-tauri");
        fs::create_dir_all(&manifest_dir).unwrap();

        stage_android_sidecar_lib(&manifest_dir, "aarch64-linux-android", &sidecar);

        let target_so = manifest_dir
            .join("gen/android/app/src/main/jniLibs/arm64-v8a/libaria2c.so");
        assert!(target_so.is_file(), "staged libaria2c.so must exist");
        let content = fs::read(&target_so).unwrap();
        assert_eq!(content, b"mock-aria2c-content");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
