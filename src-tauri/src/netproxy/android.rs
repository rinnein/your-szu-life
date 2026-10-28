//! Android 系统代理探测：通过 Kotlin `ProxySelector` 取系统配置。
//!
//! 沿用本仓其它 Android 桥的写法（`JNIEnv` + `loadClass` 调用 `@JvmStatic` 方法，
//! 类加载器取自 application context）。

#[cfg(target_os = "android")]
fn with_bridge<T>(
    callback: impl FnOnce(&mut jni::JNIEnv<'_>, jni::objects::JClass<'_>) -> Result<T, String>,
) -> Result<T, String> {
    use jni::{objects::JValue, JavaVM};

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let context = ndk_context::android_context();
        let vm =
            unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|error| error.to_string())?;
        let mut environment = vm
            .attach_current_thread()
            .map_err(|error| error.to_string())?;

        let context_object = unsafe { jni::objects::JObject::from_raw(context.context().cast()) };
        let class_loader = environment
            .call_method(
                &context_object,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        let class_name = environment
            .new_string("in.rinne.your_szu_life.SystemProxyBridge")
            .map_err(|error| error.to_string())?;
        let class = environment
            .call_method(
                &class_loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&class_name)],
            )
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        let class = jni::objects::JClass::from(class);
        let result = callback(&mut environment, class);
        std::mem::forget(context_object);
        result
    }));
    match result {
        Ok(result) => result,
        Err(_) => Err("Android bridge 调用异常".to_owned()),
    }
}

/// 系统代理，仅在 Android 上可用。
///
/// `scheme` 决定用哪个协议去问 `ProxySelector`（系统的分流规则按协议不同）。
#[cfg(target_os = "android")]
pub(super) fn system_proxy(scheme: &str) -> Result<Option<reqwest::Url>, String> {
    let probe = format!("{scheme}://example.invalid/");
    with_bridge(move |environment, class| {
        use jni::{objects::JValue, objects::JString};

        let argument = environment
            .new_string(probe)
            .map_err(|error| error.to_string())?;
        let value = environment
            .call_static_method(
                class,
                "getProxyJson",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&argument)],
            )
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        let value = JString::from(value);
        let value = environment
            .get_string(&value)
            .map_err(|error| error.to_string())?;
        super::system::parse_bridge_result(&value.to_string_lossy())
    })
}