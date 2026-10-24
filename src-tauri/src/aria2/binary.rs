use std::path::PathBuf;
use tauri::Manager;

/// Android 侧车库文件名（由 `build.rs` 写入 `jniLibs/<abi>/`）。
#[cfg(target_os = "android")]
const ANDROID_SIDECAR_LIB: &str = "libaria2c.so";
/// 嵌入在侧车库中的载荷被导出为 `_binary_payload_bin_{start,end,size}`。
#[cfg(target_os = "android")]
const ANDROID_PAYLOAD_START: &str = "_binary_payload_bin_start";
#[cfg(target_os = "android")]
const ANDROID_PAYLOAD_END: &str = "_binary_payload_bin_end";
#[cfg(target_os = "android")]
const ANDROID_PAYLOAD_SIZE: &str = "_binary_payload_bin_size";
/// 还原出来的可执行文件放在应用私有目录，Android 10+ 才允许执行。
#[cfg(target_os = "android")]
const ANDROID_RUNTIME_DIR: &str = "aria2";
#[cfg(target_os = "android")]
const ANDROID_RUNTIME_BINARY: &str = "aria2c";

pub fn find_aria2c_binary(app: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    // 1. If explicit environment variable is set
    if let Ok(env_path) = std::env::var("ARIA2C_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Ok(p);
        }
    }

    let bin_name = if cfg!(windows) { "aria2c.exe" } else { "aria2c" };

    // 2. Android: 侧车二进制不随 externalBin 打包，而是嵌入 `libaria2c.so`，
    //    运行时从 nativeLibraryDir 加载并还原到应用私有目录。
    #[cfg(target_os = "android")]
    {
        if let Some(path) = extract_android_sidecar_binary(app)? {
            return Ok(path);
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
    #[cfg(not(target_os = "android"))]
    {
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(&dev_bin_name);
        if dev_path.is_file() {
            return Ok(dev_path);
        }
    }
    let _ = dev_bin_name;

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

/// 从 APK 内的 `libaria2c.so` 中还原出可执行的 aria2c。
///
/// Tauri CLI 只对桌面端处理 `bundle.externalBin`，Android 侧的 sidecar 由 `build.rs`
/// 用 `ld.lld --soname=libaria2c.so` 把 `aria2c` 原样嵌入一个共享库并放进
/// `jniLibs/<abi>/`。这里 `dlopen` 该库、读取导出的 `_binary_payload_bin_*` 符号，
/// 把内容写到应用私有目录后 `execve` 才是合法路径（`lib/` 目录只读且不可执行）。
#[cfg(target_os = "android")]
fn extract_android_sidecar_binary(
    app: Option<&tauri::AppHandle>,
) -> Result<Option<PathBuf>, String> {
    let Some(lib_path) = installed_native_lib_path(ANDROID_SIDECAR_LIB) else {
        return Ok(None);
    };
    if !lib_path.is_file() {
        return Ok(None);
    }

    let output_dir = runtime_dir(app).ok_or_else(|| "无法确定 aria2c 的运行目录".to_string())?;
    let output_path = output_dir.join(ANDROID_RUNTIME_BINARY);

    let (start, size) = embedded_payload_range(&lib_path)?;
    if size == 0 {
        return Err(format!(
            "{} 中嵌入的 aria2c 载荷为空，请重新构建 APK",
            lib_path.display()
        ));
    }

    // 已经是同一份载荷就直接复用，避免每次启动都重写 6 MB。
    if let Ok(existing) = std::fs::metadata(&output_path) {
        if existing.len() == size as u64 {
            return Ok(Some(output_path));
        }
    }

    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("创建 {} 失败: {e}", output_dir.display()))?;
    let payload = unsafe { std::slice::from_raw_parts(start as *const u8, size) };
    std::fs::write(&output_path, payload)
        .map_err(|e| format!("写入 {} 失败: {e}", output_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("设置 {} 可执行权限失败: {e}", output_path.display()))?;
    }

    Ok(Some(output_path))
}

/// 应用私有、可执行的目录（Android 只允许执行此处文件）。
#[cfg(target_os = "android")]
fn runtime_dir(app: Option<&tauri::AppHandle>) -> Option<PathBuf> {
    if let Some(app) = app {
        if let Ok(dir) = app.path().app_data_dir() {
            return Some(dir.join(ANDROID_RUNTIME_DIR));
        }
    }
    // Tauri 尚未初始化时（例如单元测试）退回到 Java 侧的 files 目录。
    android_files_dir().map(|dir| dir.join(ANDROID_RUNTIME_DIR))
}

#[cfg(target_os = "android")]
fn android_files_dir() -> Option<PathBuf> {
    use jni::{objects::JObject, JavaVM};
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let context = std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });
    let files_dir = env
        .call_method(&*context, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;
    let path_obj = env
        .call_method(&files_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let path: String = env.get_string((&path_obj).into()).ok()?.into();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// `dlopen` 侧车库并解析嵌入载荷的地址区间。
#[cfg(target_os = "android")]
fn embedded_payload_range(lib_path: &std::path::Path) -> Result<(usize, usize), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    extern "C" {
        fn dlopen(filename: *const std::os::raw::c_char, flag: std::os::raw::c_int)
            -> *mut std::os::raw::c_void;
        fn dlsym(handle: *mut std::os::raw::c_void, symbol: *const std::os::raw::c_char)
            -> *mut std::os::raw::c_void;
        fn dlerror() -> *mut std::os::raw::c_char;
    }
    // RTLD_NOW = 2，RTLD_LOCAL = 0
    const RTLD_NOW: std::os::raw::c_int = 2;

    let path = CString::new(lib_path.as_os_str().as_bytes())
        .map_err(|_| format!("非法路径: {}", lib_path.display()))?;
    let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW) };
    if handle.is_null() {
        let message = unsafe {
            let error = dlerror();
            if error.is_null() {
                "未知错误".to_string()
            } else {
                std::ffi::CStr::from_ptr(error).to_string_lossy().into_owned()
            }
        };
        return Err(format!("加载 {} 失败: {message}", lib_path.display()));
    }
    let lookup = |name: &str| -> Option<*mut std::os::raw::c_void> {
        let symbol = CString::new(name).ok()?;
        let pointer = unsafe { dlsym(handle, symbol.as_ptr()) };
        (!pointer.is_null()).then_some(pointer)
    };

    let start = lookup(ANDROID_PAYLOAD_START)
        .ok_or_else(|| format!("{} 缺少 {ANDROID_PAYLOAD_START} 符号", lib_path.display()))?;
    let end = lookup(ANDROID_PAYLOAD_END)
        .ok_or_else(|| format!("{} 缺少 {ANDROID_PAYLOAD_END} 符号", lib_path.display()))?;
    let size_symbol = lookup(ANDROID_PAYLOAD_SIZE)
        .ok_or_else(|| format!("{} 缺少 {ANDROID_PAYLOAD_SIZE} 符号", lib_path.display()))?;

    // `ld.lld` 把 `-b binary` 的大小符号生成为 ABSOLUTE 符号：它的「地址」本身就是
    // 载荷长度，解引用会读到非法地址（段错误）。因此直接把指针值当长度使用。
    let size = size_symbol as usize;
    let begin = start as usize;
    let finish = end as usize;
    // 用两种方式交叉校验，防止符号被裁剪或重排。
    if finish.saturating_sub(begin) != size {
        return Err(format!(
            "{} 中的 payload 区间不一致（{begin:#x}..{finish:#x} vs {size}）",
            lib_path.display()
        ));
    }

    Ok((begin, size))
}

#[cfg(target_os = "android")]
fn installed_native_lib_path(lib_name: &str) -> Option<PathBuf> {
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
    Some(PathBuf::from(dir_str).join(lib_name))
}
