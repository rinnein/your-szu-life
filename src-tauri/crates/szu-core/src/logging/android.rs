//! Android 日志打开桥。
//!
//! 用 FileProvider 把日志文件包装成带读取授权的 `content://` URI，再交给系统
//! 应用打开。`tauri-plugin-opener` 的移动端实现既会把路径当裸字符串转发（与
//! Kotlin 侧 `OpenArgs` 的对象契约不符），也不会为私有目录生成读取授权。

use jni::objects::{JClass, JObject, JString, JValue};
use std::path::Path;

const BRIDGE_CLASS: &str = "in.rinne.your_szu_life.LogFileBridge";

pub(super) fn open_log_file(path: &Path) -> Result<(), String> {
    let context = ndk_context::android_context();
    let vm =
        unsafe { jni::JavaVM::from_raw(context.vm().cast()) }.map_err(|error| error.to_string())?;
    let mut environment = vm
        .attach_current_thread()
        .map_err(|error| error.to_string())?;
    // `ndk_context` 借出的是 Java 侧的全局引用，不归本函数释放。
    let context_object =
        std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });
    let path_string = environment
        .new_string(path.to_string_lossy().into_owned())
        .map_err(|error| error.to_string())?;

    let class_loader = environment
        .call_method(
            &*context_object,
            "getClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .map_err(|error| error.to_string())?
        .l()
        .map_err(|error| error.to_string())?;
    let class_name = environment
        .new_string(BRIDGE_CLASS)
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

    let result = environment
        .call_static_method(
            JClass::from(class),
            "openLogFile",
            "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&*context_object),
                JValue::Object(&path_string),
            ],
        )
        .map_err(|error| error.to_string())?
        .l()
        .map_err(|error| error.to_string())?;

    let message: String = environment
        .get_string(&JString::from(result))
        .map_err(|error| error.to_string())?
        .into();
    if message.is_empty() {
        Ok(())
    } else {
        Err(message)
    }
}
