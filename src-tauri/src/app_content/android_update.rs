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

    let urls_json = serde_json::to_string(&candidate_urls)
        .map_err(|e| format!("序列化下载地址失败: {e}"))?;

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

        let cache_dir_opt = (|| -> Result<PathBuf, jni::errors::Error> {
            let cache_file = env
                .call_method(&*context_obj, "getCacheDir", "()Ljava/io/File;", &[])?
                .l()?;
            let path_obj = env
                .call_method(&cache_file, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                .l()?;
            let path_str: String = env.get_string((&path_obj).into())?.into();
            Ok(PathBuf::from(path_str))
        })();

        let mut aria_success = false;

        if let Ok(cache_dir) = cache_dir_opt {
            let filename = "update.apk".to_string();
            let target_file = cache_dir.join(&filename);
            if target_file.exists() {
                let _ = std::fs::remove_file(&target_file);
            }

            emit_progress(UpdateProgressPayload {
                status: "downloading".to_string(),
                progress: 0,
                downloaded_bytes: 0,
                total_bytes: 0,
                error: None,
            });

            let aria_options = crate::aria2::DownloadJobOptions {
                uris: candidate_urls.clone(),
                dest_dir: cache_dir,
                filename,
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

            if let Ok(apk_path) = aria_res {
                let file_len = std::fs::metadata(&apk_path).map(|m| m.len()).unwrap_or(0);
                emit_progress(UpdateProgressPayload {
                    status: "installing".to_string(),
                    progress: 100,
                    downloaded_bytes: file_len,
                    total_bytes: file_len,
                    error: None,
                });

                if install_apk_via_bridge(&mut env, &*context_obj, &apk_path).unwrap_or(false) {
                    aria_success = true;
                }
            }
        }

        if !aria_success {
            log::warn!("aria2c Android 更新下载未完成，正在回退至原生 AppUpdateBridge...");
            fallback_start_update(&mut env, &*context_obj, &urls_json, is_gzip);
        }
    });

    Ok(())
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
            loader,
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
fn fallback_start_update(
    env: &mut jni::JNIEnv,
    context: &jni::objects::JObject,
    urls_json: &str,
    is_gzip: bool,
) {
    use jni::objects::{JClass, JValue};

    let result = (|| -> Result<bool, jni::errors::Error> {
        let loader = env
            .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        let name = env.new_string("in.rinne.your_szu_life.AppUpdateBridge")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let urls_string = env.new_string(urls_json)?;
        env.call_static_method(
            JClass::from(class),
            "startUpdate",
            "(Landroid/content/Context;Ljava/lang/String;Z)Z",
            &[
                JValue::Object(context),
                JValue::Object(&urls_string),
                JValue::Bool(is_gzip as u8),
            ],
        )?
        .z()
    })();

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    if let Err(e) = result {
        emit_error(&format!("启动备用应用内更新失败: {e}"));
    }
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
