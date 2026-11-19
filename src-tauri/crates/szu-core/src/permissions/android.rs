//! Android 权限桥：`PermissionBridge.kt` 的 Rust 侧封装。
//!
//! 状态查询与打开系统设置是同步调用；权限申请要等用户在系统弹窗里选择，走
//! `nativePermissionResult` 回调 + oneshot，与目录选择器同一套范式。

use super::{PermissionId, PermissionSettingsTarget, PermissionStateView};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

const BRIDGE_CLASS: &str = "in.rinne.your_szu_life.PermissionBridge";

/// 一次只允许一个申请；结果由 JNI 回调唤醒。
static REQUEST_TX: Mutex<Option<oneshot::Sender<(String, String)>>> = Mutex::new(None);

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

/// 调用返回 `String` 的静态方法；参数只能是字符串。
fn call_string(method: &str, args: &[&str]) -> Result<String, String> {
    let signature = match args.len() {
        0 => "()Ljava/lang/String;",
        1 => "(Ljava/lang/String;)Ljava/lang/String;",
        _ => return Err("不支持的桥接调用参数数量".to_string()),
    };
    with_bridge(|environment, class| {
        let strings: Vec<jni::objects::JString> = args
            .iter()
            .map(|arg| environment.new_string(arg))
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?;
        let values: Vec<jni::objects::JValue> = strings
            .iter()
            .map(|value| jni::objects::JValue::Object(value))
            .collect();
        let value = environment
            .call_static_method(class, method, signature, &values)
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        let value = jni::objects::JString::from(value);
        let text: String = environment
            .get_string(&value)
            .map_err(|error| error.to_string())?
            .into();
        Ok(text)
    })
}

/// 调用只有一个字符串参数的 `void` 静态方法。
fn call_void(method: &str, arg: &str) -> Result<(), String> {
    with_bridge(|environment, class| {
        let value = environment
            .new_string(arg)
            .map_err(|error| error.to_string())?;
        environment
            .call_static_method(
                class,
                method,
                "(Ljava/lang/String;)V",
                &[jni::objects::JValue::Object(&value)],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    })
}

pub(super) fn states() -> Result<Vec<PermissionStateView>, String> {
    let payload = call_string("statusJson", &[])?;
    super::parse_native_states(&payload)
}

pub(super) async fn request(keys: &[PermissionId]) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    {
        let mut guard = REQUEST_TX.lock().unwrap();
        if guard.is_some() {
            return Err("已经有一个权限申请正在进行".to_string());
        }
        *guard = Some(tx);
    }
    let keys = keys
        .iter()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    if let Err(error) = call_void("requestPermissions", &keys) {
        let _ = REQUEST_TX.lock().unwrap().take();
        return Err(error);
    }
    // 超时或回调被丢弃时也要把占位清掉，否则之后的申请会一直报「正在进行」。
    let outcome = tokio::time::timeout(Duration::from_secs(120), rx).await;
    let (status, payload) = match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            let _ = REQUEST_TX.lock().unwrap().take();
            return Err("权限申请已取消".to_string());
        }
        Err(_) => {
            let _ = REQUEST_TX.lock().unwrap().take();
            return Err("等待权限申请结果超时".to_string());
        }
    };
    match status.as_str() {
        "ok" => Ok(()),
        _ => Err(if payload.is_empty() {
            "权限申请失败".to_string()
        } else {
            payload
        }),
    }
}

pub(super) fn open_settings(target: PermissionSettingsTarget) -> Result<(), String> {
    let error = call_string("openSettings", &[target.as_str()])?;
    if error.is_empty() {
        Ok(())
    } else {
        Err(error)
    }
}

/// Kotlin 侧申请结束后回调；只投递结果，不做业务判断。
#[no_mangle]
pub extern "system" fn Java_in_rinne_your_1szu_1life_PermissionBridge_nativePermissionResult(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    status: jni::objects::JString,
    payload: jni::objects::JString,
) {
    let status: String = env
        .get_string(&status)
        .map(|text| text.into())
        .unwrap_or_default();
    let payload: String = env
        .get_string(&payload)
        .map(|text| text.into())
        .unwrap_or_default();
    if let Some(sender) = REQUEST_TX.lock().unwrap().take() {
        let _ = sender.send((status, payload));
    }
}
