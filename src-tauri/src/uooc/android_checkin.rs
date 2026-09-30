use jni::{
    objects::{JClass, JObject, JString, JValue},
    JNIEnv, JavaVM,
};
fn bridge<T>(
    call: impl FnOnce(&mut JNIEnv<'_>, JClass<'_>, &JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android 签到服务")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android 签到服务")?;
    let context =
        std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });
    let result = (|| {
        let loader = env
            .call_method(
                &*context,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )?
            .l()?;
        let name = env.new_string("in.rinne.your_szu_life.UoocCheckinBridge")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        call(&mut env, JClass::from(class), &context)
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result.map_err(|_| "Android 签到窗口服务不可用".into())
}
pub(super) fn open(id: &str, url: &str) -> Result<(), String> {
    let accepted = bridge(|env, class, context| {
        let id = env.new_string(id)?;
        let url = env.new_string(url)?;
        env.call_static_method(
            class,
            "open",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
            &[
                JValue::Object(context),
                JValue::Object(&id),
                JValue::Object(&url),
            ],
        )?
        .z()
    })?;
    if accepted {
        Ok(())
    } else {
        Err("无法启动 Android 后台签到".into())
    }
}
pub(super) fn ready(id: &str) -> Result<bool, String> {
    let state = bridge(|env, class, _| {
        let id = env.new_string(id)?;
        let result = JString::from(
            env.call_static_method(
                class,
                "state",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&id)],
            )?
            .l()?,
        );
        let value: String = env.get_string(&result)?.into();
        Ok(value)
    })?;
    match state.as_str() {
        "ready" => Ok(true),
        "loading" => Ok(false),
        "login" => Err("UOOC 登录已失效，请重新登录".into()),
        _ => Err("Android 后台签到页面加载失败；请手动重试".into()),
    }
}
pub(super) fn close(id: &str) -> Result<(), String> {
    bridge(|env, class, _| {
        let id = env.new_string(id)?;
        env.call_static_method(
            class,
            "close",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&id)],
        )?;
        Ok(())
    })
}
