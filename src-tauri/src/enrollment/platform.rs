use super::Service;
use std::sync::Arc;

#[cfg(not(target_os = "android"))]
pub(super) fn initialize(_: &Arc<Service>) {}
#[cfg(not(target_os = "android"))]
pub(super) fn running(_: usize, _: u64) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "android")]
static SERVICE: std::sync::OnceLock<std::sync::Weak<Service>> = std::sync::OnceLock::new();
#[cfg(target_os = "android")]
pub(super) fn initialize(service: &Arc<Service>) {
    let _ = SERVICE.set(Arc::downgrade(service));
}

#[cfg(target_os = "android")]
pub(super) fn running(count: usize, generation: u64) -> Result<(), String> {
    use jni::{
        objects::{JClass, JObject, JValue},
        JavaVM,
    };
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android 后台服务")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android 后台服务")?;
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
        let name = env.new_string("in.rinne.your_szu_life.EnrollmentTaskBridge")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )
            .and_then(|v| v.l())?;
        env.call_static_method(
            JClass::from(class),
            "update",
            "(Landroid/content/Context;IJ)Z",
            &[
                JValue::Object(&context),
                JValue::Int(count.min(i32::MAX as usize) as i32),
                JValue::Long(generation as i64),
            ],
        )
        .and_then(|v| v.z())
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    match result {
        Ok(true) => Ok(()),
        _ => Err("Android 前台服务无法启动；请打开应用后继续任务".into()),
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_in_rinne_your_1szu_1life_EnrollmentTaskBridge_nativeControl(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    action: jni::sys::jint,
    generation: jni::sys::jlong,
) {
    let Some(service) = SERVICE.get().and_then(std::sync::Weak::upgrade) else {
        return;
    };
    if service.generation.load(std::sync::atomic::Ordering::SeqCst) != generation as u64 {
        return;
    }
    if action == 0 || action == 1 {
        service.pause("Android 后台任务已停止，请返回选课页面继续");
    }
}
