use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[cfg(target_os = "android")]
use tauri::Emitter;

pub(crate) static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub(crate) fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

#[cfg(target_os = "android")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub status: String,
    pub progress: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

#[cfg(not(target_os = "android"))]
pub(crate) fn start_update(
    _store: &crate::app_content::AppContentStore,
) -> Result<(), String> {
    Err("应用内更新仅支持 Android 端".to_string())
}

#[cfg(target_os = "android")]
fn emit_progress(payload: UpdateProgressPayload) {
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("in-app-update-progress", payload);
    }
}

#[cfg(target_os = "android")]
fn emit_error(msg: &str) {
    emit_progress(UpdateProgressPayload {
        status: "error".to_string(),
        progress: 0,
        downloaded_bytes: 0,
        total_bytes: 0,
        error: Some(msg.to_string()),
    });
}

#[cfg(target_os = "android")]
pub(crate) fn start_update(
    store: &crate::app_content::AppContentStore,
) -> Result<(), String> {
    use jni::JavaVM;
    use std::path::PathBuf;

    let snapshot = store.snapshot();
    let update = snapshot
        .update
        .data
        .ok_or_else(|| "当前未检测到可用更新".to_string())?;

    let direct_url = update.latest_ver_url;
    let is_gzip = direct_url.ends_with(".gz");

    let clean_direct = if let Some(idx) = direct_url.find("https://github.com/") {
        &direct_url[idx..]
    } else if let Some(idx) = direct_url.find("http://github.com/") {
        &direct_url[idx..]
    } else {
        &direct_url
    };

    let mut candidate_urls = Vec::new();
    for accelerator in store.accelerators() {
        let prefix = accelerator.as_str();
        candidate_urls.push(format!("{prefix}{clean_direct}"));
    }
    candidate_urls.push(clean_direct.to_string());

    std::thread::spawn(move || {
        let context = ndk_context::android_context();
        let Ok(vm) = (unsafe { JavaVM::from_raw(context.vm().cast()) }) else {
            emit_error("无法连接 Android 服务");
            return;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            emit_error("无法连接 Android 服务");
            return;
        };
        let context_obj =
            std::mem::ManuallyDrop::new(unsafe { jni::objects::JObject::from_raw(context.context().cast()) });

        let cache_dir = match (|| -> Result<PathBuf, jni::errors::Error> {
            let cache_file = env
                .call_method(&*context_obj, "getCacheDir", "()Ljava/io/File;", &[])?
                .l()?;
            let path_obj = env
                .call_method(&cache_file, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                .l()?;
            let path_str: String = env.get_string((&path_obj).into())?.into();
            Ok(PathBuf::from(path_str))
        })() {
            Ok(dir) => dir,
            Err(e) => {
                emit_error(&format!("获取缓存目录失败: {e}"));
                return;
            }
        };

        clean_temporary_apks(&cache_dir);
        let download_filename = if is_gzip {
            "update.apk.gz".to_string()
        } else {
            "update.apk".to_string()
        };

        emit_progress(UpdateProgressPayload {
            status: "downloading".to_string(),
            progress: 0,
            downloaded_bytes: 0,
            total_bytes: 0,
            error: None,
        });

        let aria_options = crate::aria2::DownloadJobOptions {
            uris: candidate_urls,
            dest_dir: cache_dir.clone(),
            filename: download_filename,
            expected_sha256: None,
            expected_size: None,
        };

        let aria_res = tauri::async_runtime::block_on(crate::aria2::download_with_aria2(
            APP_HANDLE.get(),
            aria_options,
            |prog| {
                let progress = if prog.total_bytes > 0 {
                    ((prog.downloaded_bytes as f64 / prog.total_bytes as f64) * 100.0)
                        .clamp(0.0, 100.0) as u32
                } else {
                    0
                };
                emit_progress(UpdateProgressPayload {
                    status: "downloading".to_string(),
                    progress,
                    downloaded_bytes: prog.downloaded_bytes,
                    total_bytes: prog.total_bytes,
                    error: None,
                });
            },
        ));

        let downloaded_path = match aria_res {
            Ok(path) => path,
            Err(e) => {
                emit_error(&format!("aria2c 下载更新失败: {e}"));
                return;
            }
        };

        let final_apk_path = if is_gzip {
            let target_apk = cache_dir.join("update.apk");
            if let Err(e) = decompress_gzip(&downloaded_path, &target_apk) {
                let _ = std::fs::remove_file(&downloaded_path);
                emit_error(&format!("解压更新包失败: {e}"));
                return;
            }
            let _ = std::fs::remove_file(&downloaded_path);
            target_apk
        } else {
            downloaded_path
        };

        let file_len = std::fs::metadata(&final_apk_path).map(|m| m.len()).unwrap_or(0);
        emit_progress(UpdateProgressPayload {
            status: "installing".to_string(),
            progress: 100,
            downloaded_bytes: file_len,
            total_bytes: file_len,
            error: None,
        });

        match install_apk_via_bridge(&mut env, &*context_obj, &final_apk_path) {
            Ok(true) => {}
            Ok(false) => {
                emit_error("启动安装程序失败，请检查系统设置与安装未知应用权限");
            }
            Err(e) => {
                emit_error(&format!("调用系统安装器失败: {e}"));
            }
        }
    });

    Ok(())
}

#[cfg(target_os = "android")]
fn decompress_gzip(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::io::{Read, Write};

    if dst.is_file() {
        let _ = std::fs::remove_file(dst);
    }

    let gz_file = std::fs::File::open(src)
        .map_err(|e| format!("打开压缩包失败: {e}"))?;
    let mut decoder = GzDecoder::new(gz_file);
    let mut out_file = std::fs::File::create(dst)
        .map_err(|e| format!("创建目标 APK 失败: {e}"))?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = decoder
            .read(&mut buffer)
            .map_err(|e| format!("解压更新数据失败: {e}"))?;
        if n == 0 {
            break;
        }
        out_file
            .write_all(&buffer[..n])
            .map_err(|e| format!("写入 APK 失败: {e}"))?;
    }
    out_file
        .flush()
        .map_err(|e| format!("刷新 APK 文件失败: {e}"))?;
    Ok(())
}

#[cfg(target_os = "android")]
fn clean_temporary_apks(cache_dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".apk") || lower.ends_with(".apk.gz") || lower.starts_with("update.") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
fn install_apk_via_bridge(
    env: &mut jni::JNIEnv,
    context: &jni::objects::JObject,
    apk_path: &std::path::Path,
) -> Result<bool, jni::errors::Error> {
    use jni::objects::{JClass, JValue};

    let loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let name = env.new_string("in.rinne.your_szu_life.AppUpdateBridge")?;
    let bridge_class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?
        .l()?;

    let file_class_name = env.new_string("java.io.File")?;
    let file_class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&file_class_name)],
        )?
        .l()?;
    let path_str = env.new_string(apk_path.to_string_lossy())?;
    let file_obj = env.new_object(
        JClass::from(file_class),
        "(Ljava/lang/String;)V",
        &[JValue::Object(&path_str)],
    )?;

    let result = env
        .call_static_method(
            JClass::from(bridge_class),
            "installApk",
            "(Landroid/content/Context;Ljava/io/File;)Z",
            &[JValue::Object(context), JValue::Object(&file_obj)],
        )?
        .z()?;

    Ok(result)
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_in_rinne_your_1szu_1life_AppUpdateBridge_nativeUpdateProgress(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    status: jni::objects::JString,
    progress: jni::sys::jint,
    downloaded_bytes: jni::sys::jlong,
    total_bytes: jni::sys::jlong,
    error: jni::objects::JString,
) {
    let status_str: String = env
        .get_string(&status)
        .map(|s| s.into())
        .unwrap_or_else(|_| "unknown".to_string());

    let error_str: Option<String> = if !error.is_null() {
        env.get_string(&error).ok().map(|s| s.into())
    } else {
        None
    };

    let payload = UpdateProgressPayload {
        status: status_str,
        progress: progress.max(0) as u32,
        downloaded_bytes: downloaded_bytes.max(0) as u64,
        total_bytes: total_bytes.max(0) as u64,
        error: error_str,
    };

    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("in-app-update-progress", payload);
    }
}
