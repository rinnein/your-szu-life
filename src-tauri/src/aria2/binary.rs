use std::path::PathBuf;
use tauri::Manager;

pub fn find_aria2c_binary(app: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    // 1. If explicit environment variable is set
    if let Ok(env_path) = std::env::var("ARIA2C_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Ok(p);
        }
    }

    let bin_name = if cfg!(windows) { "aria2c.exe" } else { "aria2c" };

    // 2. Android: check nativeLibraryDir if running on Android
    #[cfg(target_os = "android")]
    {
        if let Some(path) = find_android_native_lib("libaria2c.so") {
            if path.is_file() {
                return Ok(path);
            }
        }
    }

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
    if let Some(app) = app {
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

#[cfg(target_os = "android")]
fn find_android_native_lib(lib_name: &str) -> Option<PathBuf> {
    use jni::{objects::JObject, JavaVM};
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let context = std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });
    let app_info = env
        .call_method(
            &*context,
            "getApplicationInfo",
            "()Landroid/content/pm/ApplicationInfo;",
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let native_lib_dir = env
        .get_field(app_info, "nativeLibraryDir", "Ljava/lang/String;")
        .ok()?
        .l()
        .ok()?;
    let dir_str: String = env.get_string((&native_lib_dir).into()).ok()?.into();
    let path = PathBuf::from(dir_str).join(lib_name);
    Some(path)
}
