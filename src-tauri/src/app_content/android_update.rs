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
pub(crate) fn start_update(
    store: &crate::app_content::AppContentStore,
) -> Result<(), String> {
    use jni::{
        objects::{JClass, JObject, JValue},
        JavaVM,
    };

    let snapshot = store.snapshot();
    let update = snapshot
        .update
        .data
        .ok_or_else(|| "当前未检测到可用更新".to_string())?;

    // Collect download candidate URLs (accelerators + direct)
    let direct_url = update.latest_ver_url;
    let is_gzip = direct_url.ends_with(".gz");

    // Remove any accelerator prefix if direct_url was already rewritten
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

    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android 服务")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android 服务")?;
    let context =
        std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });

    let result = (|| {
        let loader = env
            .call_method(
                &*context,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
            .and_then(|v| v.l())?;
        let name = env.new_string("in.rinne.your_szu_life.AppUpdateBridge")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )
            .and_then(|v| v.l())?;
        let urls_string = env.new_string(&urls_json)?;
        env.call_static_method(
            JClass::from(class),
            "startUpdate",
            "(Landroid/content/Context;Ljava/lang/String;Z)Z",
            &[
                JValue::Object(&context),
                JValue::Object(&urls_string),
                JValue::Bool(is_gzip as u8),
            ],
        )
        .and_then(|v| v.z())
    })();

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err("更新任务已在进行中，或下载地址无效".to_string()),
        Err(e) => Err(format!("启动应用内更新失败: {e}")),
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
