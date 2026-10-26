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
            .new_string("in.rinne.your_szu_life.WallpaperColorBridge")
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

/// 壁纸种子色，仅在 Android 上可用。
#[cfg(target_os = "android")]
pub(super) fn wallpaper_seed_result() -> Result<super::WallpaperSeed, String> {
    with_bridge(|environment, class| {
        let value = environment
            .call_static_method(class, "getSeedColorJson", "()Ljava/lang/String;", &[])
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        let value = jni::objects::JString::from(value);
        let value = environment
            .get_string(&value)
            .map_err(|error| error.to_string())?;
        super::parse_wallpaper_seed(&value.to_string_lossy())
    })
}
