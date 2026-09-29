//! Calls the app-owned bridge from worker threads, never Android's UI thread.
use jni::{
    objects::{JClass, JObject, JString, JValue},
    JNIEnv, JavaVM,
};

fn with_bridge<T>(
    call: impl FnOnce(&mut JNIEnv<'_>, JClass<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android Cookie 服务")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android Cookie 服务")?;
    // The Android context is a borrowed global reference, not ours to delete.
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
            .and_then(|v| v.l())
            .map_err(|_| "无法加载 Android Cookie 服务")?;
        let name = env
            .new_string("in.rinne.your_szu_life.AuthCookieBridge")
            .map_err(|_| "无法加载 Android Cookie 服务")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )
            .and_then(|v| v.l())
            .map_err(|_| "无法加载 Android Cookie 服务")?;
        call(&mut env, JClass::from(class))
    })();
    // Java exceptions must not leak into the next JNI call or include cookie headers.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}

pub(super) fn supports_cookie_info() -> Result<bool, String> {
    with_bridge(|env, class| {
        env.call_static_method(class, "supportsCookieInfo", "()Z", &[])
            .and_then(|v| v.z())
            .map_err(|_| "无法检查 Android WebView 版本".into())
    })
}

pub(super) fn read_cookies(urls: &[&str]) -> Result<Vec<String>, String> {
    with_bridge(|env, class| {
        let input = env
            .new_string(serde_json::to_string(urls).map_err(|_| "认证地址无效")?)
            .map_err(|_| "读取 Android 认证会话失败")?;
        let result = env
            .call_static_method(
                class,
                "readCookies",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&input)],
            )
            .and_then(|v| v.l())
            .map_err(|_| "读取 Android 认证会话失败")?;
        let result = JString::from(result);
        let raw: String = env
            .get_string(&result)
            .map_err(|_| "读取 Android 认证会话失败")?
            .into();
        serde_json::from_str(&raw).map_err(|_| "Android 认证会话格式无效".into())
    })
}

pub(super) fn write_cookies(cookies: &[serde_json::Value]) -> Result<(), String> {
    with_bridge(|env, class| {
        let input = env
            .new_string(serde_json::to_string(cookies).map_err(|_| "认证会话格式无效")?)
            .map_err(|_| "写入 Android 认证会话失败")?;
        let accepted = env
            .call_static_method(
                class,
                "writeCookies",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&input)],
            )
            .and_then(|v| v.z())
            .map_err(|_| "写入 Android 认证会话失败")?;
        if accepted {
            Ok(())
        } else {
            Err("写入 Android 认证会话失败或超时".into())
        }
    })
}
