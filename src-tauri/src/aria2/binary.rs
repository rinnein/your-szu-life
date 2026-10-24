use std::path::PathBuf;
#[cfg(not(target_os = "android"))]
use tauri::Manager;

/// Android 侧车库文件名（由 `build.rs` 写入 `jniLibs/<abi>/`）。
#[cfg(target_os = "android")]
const ANDROID_SIDECAR_LIB: &str = "libaria2c.so";

pub fn find_aria2c_binary(_app: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    // 1. If explicit environment variable is set
    if let Ok(env_path) = std::env::var("ARIA2C_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Ok(p);
        }
    }

    // 2. Android: 侧车二进制内置在 APK 的 native library 中，
    //    安装时由系统解压到 nativeLibraryDir，直接作为可执行程序执行。
    #[cfg(target_os = "android")]
    {
        return find_android_native_lib(ANDROID_SIDECAR_LIB);
    }

    #[cfg(not(target_os = "android"))]
    {
        let bin_name = if cfg!(windows) { "aria2c.exe" } else { "aria2c" };

        // 3. Current executable directory (production desktop bundle)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(bin_name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }

        // 4. Tauri app resource directory
        if let Some(app) = _app {
            if let Ok(resource_dir) = app.path().resource_dir() {
                let candidate = resource_dir.join(bin_name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }

        // 5. Development mode: check src-tauri/binaries/aria2c-<TARGET>
        let dev_bin_name = format!(
            "aria2c-{}{}",
            env!("TAURI_ENV_TARGET_TRIPLE"),
            if cfg!(windows) { ".exe" } else { "" }
        );
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(&dev_bin_name);
        if dev_path.is_file() {
            return Ok(dev_path);
        }

        // 6. System PATH (e.g. /usr/bin/aria2c)
        let system_paths = if cfg!(windows) {
            vec![PathBuf::from(r"C:\Program Files\aria2\aria2c.exe")]
        } else {
            vec![
                PathBuf::from("/usr/bin/aria2c"),
                PathBuf::from("/usr/local/bin/aria2c"),
            ]
        };
        for p in system_paths {
            if p.is_file() {
                return Ok(p);
            }
        }

        let which_cmd = if cfg!(windows) { "where" } else { "which" };
        if let Ok(output) = std::process::Command::new(which_cmd).arg(bin_name).output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !path_str.is_empty() && PathBuf::from(&path_str).is_file() {
                    return Ok(PathBuf::from(path_str));
                }
            }
        }

        Err("未找到 aria2c 可执行文件（已检查安装目录、资源目录、开发目录和系统 PATH）".to_string())
    }
}

#[cfg(target_os = "android")]
fn find_android_native_lib(lib_name: &str) -> Result<PathBuf, String> {
    let candidate = match installed_native_lib_path(lib_name) {
        Ok(path) => path,
        Err(e) => {
            log::warn!("获取 Android nativeLibraryDir 失败: {e}");
            PathBuf::from(format!("/data/data/in.rinne.your_szu_life/lib/{lib_name}"))
        }
    };

    log::debug!("检查 Android 原生库路径: {}", candidate.display());
    if candidate.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = candidate.metadata() {
                let mut perms = meta.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(perms.mode() | 0o755);
                    let _ = std::fs::set_permissions(&candidate, perms);
                }
            }
        }
        return Ok(candidate);
    }

    let alt_candidate = PathBuf::from(format!("/data/data/in.rinne.your_szu_life/lib/{lib_name}"));
    if alt_candidate != candidate && alt_candidate.is_file() {
        log::debug!("在备用路径找到原生库: {}", alt_candidate.display());
        return Ok(alt_candidate);
    }

    Err(format!(
        "未在 Android 原生库目录找到 {}（已检查 {}），请确保 APK 构建已包含该原生库且安装时已解压",
        lib_name,
        candidate.display()
    ))
}

#[cfg(target_os = "android")]
fn installed_native_lib_path(lib_name: &str) -> Result<PathBuf, String> {
    use jni::{objects::JObject, JavaVM};
    let context = ndk_context::android_context();
    if context.vm().is_null() {
        return Err("ndk_context::android_context().vm() 为 null".to_string());
    }
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|e| format!("JavaVM::from_raw 失败: {e}"))?;
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("attach_current_thread 失败: {e}"))?;
    if context.context().is_null() {
        return Err("ndk_context::android_context().context() 为 null".to_string());
    }
    let context_obj = std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });
    let app_info = env
        .call_method(
            &*context_obj,
            "getApplicationInfo",
            "()Landroid/content/pm/ApplicationInfo;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| format!("调用 getApplicationInfo 失败: {e}"))?;

    let native_lib_dir = env
        .get_field(app_info, "nativeLibraryDir", "Ljava/lang/String;")
        .and_then(|v| v.l())
        .map_err(|e| format!("获取 nativeLibraryDir 字段失败: {e}"))?;

    let dir_str: String = env
        .get_string((&native_lib_dir).into())
        .map_err(|e| format!("读取 nativeLibraryDir 字符串失败: {e}"))?
        .into();

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    Ok(PathBuf::from(dir_str).join(lib_name))
}
