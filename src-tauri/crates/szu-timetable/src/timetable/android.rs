//! Android 原生常驻通知桥。
//!
//! 课表提醒需要「20 分钟内持续倒计时 + ProgressStyle / setProgress」，
//! `tauri-plugin-notification` 只提供一次性通知，因此 Android 上把排程规格
//! 交给 Kotlin 侧的 `TimetableNotificationBridge`，由 AlarmManager +
//! BroadcastReceiver 负责发布、更新与清除。

#[cfg(target_os = "android")]
use super::model::ClassReminderSpec;

#[cfg(target_os = "android")]
fn call_bridge(method: &str, signature: &str, payload: Option<&str>) -> Result<(), String> {
    use jni::{
        objects::{JClass, JObject, JValue},
        JavaVM,
    };

    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android 通知服务")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android 通知服务")?;
    let context =
        std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });

    let result = (|| {
        let loader = env.call_method(
            &*context,
            "getClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )?;
        let loader = loader.l()?;
        let name = env.new_string("in.rinne.your_szu_life.TimetableNotificationBridge")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )?
            .l()?;
        let payload = match payload {
            Some(payload) => Some(env.new_string(payload)?),
            None => None,
        };
        let mut args = vec![JValue::Object(&context)];
        if let Some(text) = &payload {
            args.push(JValue::Object(text));
        }
        env.call_static_method(JClass::from(class), method, signature, &args)?
            .z()
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err("Android 课表通知发布失败；请检查通知权限".into()),
        Err(error) => Err(format!("Android 课表通知桥调用失败: {error}")),
    }
}

#[cfg(target_os = "android")]
pub fn sync(specs: &[ClassReminderSpec]) -> Result<(), String> {
    let json =
        serde_json::to_string(specs).map_err(|error| format!("课表通知规格序列化失败: {error}"))?;
    call_bridge(
        "sync",
        "(Landroid/content/Context;Ljava/lang/String;)Z",
        Some(&json),
    )
}

#[cfg(target_os = "android")]
pub fn cancel_all() -> Result<(), String> {
    call_bridge("cancelAll", "(Landroid/content/Context;)Z", None)
}
