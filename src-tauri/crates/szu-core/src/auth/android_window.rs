//! Calls the app-owned bridge from worker threads, never Android's UI thread.
use jni::{
    objects::{JClass, JObject, JValue},
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

pub(super) fn show_authentication(return_url: &str) -> Result<(), String> {
    with_bridge(|env, class| {
        let input = env.new_string(return_url).map_err(|_| "认证返回地址无效")?;
        let shown = env
            .call_static_method(
                class,
                "showAuthentication",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&input)],
            )
            .and_then(|value| value.z())
            .map_err(|_| "无法显示 Android 认证窗口")?;
        if shown {
            Ok(())
        } else {
            Err("无法显示 Android 认证窗口".into())
        }
    })
}

pub(super) fn finish_authentication(restore: bool) -> Result<(), String> {
    with_bridge(|env, class| {
        env.call_static_method(
            class,
            "finishAuthentication",
            "(Z)Z",
            &[JValue::Bool(restore.into())],
        )
        .map_err(|_| "无法恢复 Android 功能窗口")?;
        Ok(())
    })
}
