//! Android SAF 桥：目录选择、授权校验与文件发布。
//!
//! Android 11 起 SAF 授权只对 `ContentResolver` 生效，无法按真实路径写入，
//! 因此下载先落在应用私有的外部暂存目录，完成后由 Kotlin 侧通过
//! `DocumentsContract` 复制到用户选择的文档树里。

#[cfg(any(target_os = "android", test))]
use serde::Deserialize;

#[cfg(any(target_os = "android", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AndroidPickedDirectory {
    #[serde(default)]
    pub display_path: String,
    #[serde(default)]
    pub tree_uri: String,
    #[serde(default)]
    pub has_entries: bool,
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn parse_picked_json(raw: &str) -> Result<AndroidPickedDirectory, String> {
    let parsed: AndroidPickedDirectory = serde_json::from_str(raw)
        .map_err(|error| format!("Android 目录选择结果无法解析：{error}"))?;
    if parsed.display_path.trim().is_empty() || parsed.tree_uri.trim().is_empty() {
        return Err("Android 目录选择结果缺少路径或授权 URI".to_string());
    }
    Ok(parsed)
}

#[cfg(target_os = "android")]
pub(super) use android_impl::{
    check_access, delete_file, ensure_directory, export_resume_state, inspect, pick, publish_file,
    release, staging_root,
};

#[cfg(target_os = "android")]
mod android_impl {
    use super::super::{DirectorySelection, PickedDirectory};
    use super::parse_picked_json;
    use jni::objects::{JClass, JString, JValue};
    use std::sync::Mutex;
    use std::time::Duration;
    use tauri::AppHandle;
    use tokio::sync::oneshot;

    const BRIDGE_CLASS: &str = "in.rinne.your_szu_life.StorageDirectoryBridge";

    type PickOutcome = (String, String);

    /// 一次只允许一个选择器；取消/成功都由 JNI 回调唤醒。
    static PICK_TX: Mutex<Option<oneshot::Sender<PickOutcome>>> = Mutex::new(None);

    fn with_bridge<T>(
        callback: impl FnOnce(&mut jni::JNIEnv<'_>, JClass<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let context = ndk_context::android_context();
            let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }
                .map_err(|error| error.to_string())?;
            let mut environment = vm
                .attach_current_thread()
                .map_err(|error| error.to_string())?;
            let context_object =
                unsafe { jni::objects::JObject::from_raw(context.context().cast()) };
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
            let class = JClass::from(class);
            let result = callback(&mut environment, class);
            std::mem::forget(context_object);
            result
        }));
        match result {
            Ok(result) => result,
            Err(_) => Err("Android bridge 调用异常".to_string()),
        }
    }

    fn string_signature(count: usize) -> Option<&'static str> {
        match count {
            0 => Some("()Ljava/lang/String;"),
            1 => Some("(Ljava/lang/String;)Ljava/lang/String;"),
            2 => Some("(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
            3 => Some("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
            4 => Some(
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            ),
            _ => None,
        }
    }

    // `JValue` 借用 `JObject`，而 `JValue` 的生命周期必须覆盖调用本身，
    // 因此参数表只能在每个调用处就地构造，不能从辅助函数里连同引用一起返回。

    fn call_string(method: &str, args: &[&str]) -> Result<String, String> {
        let signature =
            string_signature(args.len()).ok_or_else(|| "不支持的桥接调用参数数量".to_string())?;
        with_bridge(|environment, class| {
            let strings: Vec<JString> = args
                .iter()
                .map(|arg| environment.new_string(arg))
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?;
            let values: Vec<JValue> = strings
                .iter()
                .map(|value| JValue::Object(value))
                .collect();
            let result = environment
                .call_static_method(class, method, signature, &values)
                .map_err(|error| error.to_string())?;
            let value = JString::from(result.l().map_err(|error| error.to_string())?);
            let value = environment
                .get_string(&value)
                .map_err(|error| error.to_string())?;
            Ok(value
                .to_str()
                .map_err(|error| error.to_string())?
                .to_string())
        })
    }

    fn call_boolean(method: &str, args: &[&str]) -> Result<bool, String> {
        let signature = match args.len() {
            0 => "()Z",
            1 => "(Ljava/lang/String;)Z",
            _ => return Err("不支持的桥接调用参数数量".to_string()),
        };
        with_bridge(|environment, class| {
            let strings: Vec<JString> = args
                .iter()
                .map(|arg| environment.new_string(arg))
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?;
            let values: Vec<JValue> = strings
                .iter()
                .map(|value| JValue::Object(value))
                .collect();
            environment
                .call_static_method(class, method, signature, &values)
                .map_err(|error| error.to_string())?
                .z()
                .map_err(|error| error.to_string())
        })
    }

    fn call_void(method: &str, args: &[&str]) -> Result<(), String> {
        let signature = match args.len() {
            0 => "()V",
            1 => "(Ljava/lang/String;)V",
            _ => return Err("不支持的桥接调用参数数量".to_string()),
        };
        with_bridge(|environment, class| {
            let strings: Vec<JString> = args
                .iter()
                .map(|arg| environment.new_string(arg))
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?;
            let values: Vec<JValue> = strings
                .iter()
                .map(|value| JValue::Object(value))
                .collect();
            environment
                .call_static_method(class, method, signature, &values)
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    pub(crate) async fn pick(app: &AppHandle) -> Result<Option<PickedDirectory>, String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = PICK_TX.lock().unwrap();
            if guard.is_some() {
                return Err("已经有一个文件夹选择器正在打开".to_string());
            }
            *guard = Some(tx);
        }
        let initial = super::super::default_base_dir(app)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Err(error) = call_void("pickDirectory", &[&initial]) {
            let _ = PICK_TX.lock().unwrap().take();
            return Err(error);
        }
        let (status, payload) = tokio::time::timeout(Duration::from_secs(600), rx)
            .await
            .map_err(|_| "等待文件夹选择超时".to_string())?
            .map_err(|_| "文件夹选择器已关闭".to_string())?;
        match status.as_str() {
            "ok" => {
                let parsed = parse_picked_json(&payload)?;
                Ok(Some(PickedDirectory {
                    selection: DirectorySelection::AndroidTree {
                        uri: parsed.tree_uri,
                    },
                    display: parsed.display_path,
                    has_entries: parsed.has_entries,
                }))
            }
            "cancelled" => Ok(None),
            _ => Err(if payload.is_empty() {
                "文件夹选择器失败".to_string()
            } else {
                payload
            }),
        }
    }

    pub(crate) fn inspect(uri: &str) -> Result<super::AndroidPickedDirectory, String> {
        let raw = call_string("inspectTree", &[uri])?;
        parse_picked_json(&raw)
    }

    pub(crate) fn check_access(uri: &str) -> Result<bool, String> {
        call_boolean("checkAccess", &[uri])
    }

    /// Android 标准缓存目录（优先外部缓存，容量足够放录像）。
    pub(crate) fn staging_root() -> Result<std::path::PathBuf, String> {
        let root = call_string("stagingRoot", &[])?;
        if root.trim().is_empty() {
            return Err("无法确定 Android 缓存目录".to_string());
        }
        Ok(std::path::PathBuf::from(root))
    }

    /// 只取回断点续传需要的最小文件集合（控制文件存在时才取目标文件）。
    pub(crate) fn export_resume_state(
        tree_uri: &str,
        relative_dir: &str,
        file_name: &str,
        destination_dir: &std::path::Path,
    ) -> Result<(), String> {
        let destination = destination_dir.to_string_lossy();
        let error = call_string(
            "exportResumeState",
            &[tree_uri, relative_dir, file_name, destination.as_ref()],
        )?;
        if error.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub(crate) fn ensure_directory(uri: &str, relative_base: &str) -> Result<(), String> {
        let error = call_string("ensureDirectory", &[uri, relative_base])?;
        if error.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub(crate) fn publish_file(
        tree_uri: &str,
        relative_dir: &str,
        source: &std::path::Path,
        file_name: &str,
    ) -> Result<(), String> {
        let source = source.to_string_lossy();
        let error = call_string(
            "publishFile",
            &[tree_uri, relative_dir, source.as_ref(), file_name],
        )?;
        if error.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub(crate) fn delete_file(
        tree_uri: &str,
        relative_dir: &str,
        file_name: &str,
    ) -> Result<(), String> {
        let error = call_string("deleteFile", &[tree_uri, relative_dir, file_name])?;
        if error.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub(crate) fn release(uri: &str) -> Result<(), String> {
        call_void("releaseTree", &[uri])
    }

    /// Kotlin 侧选完目录后回调；只投递结果，不做业务判断。
    #[no_mangle]
    pub extern "system" fn Java_in_rinne_your_1szu_1life_StorageDirectoryBridge_nativeDirectoryPicked(
        mut env: jni::JNIEnv,
        _class: JClass,
        status: JString,
        payload: JString,
    ) {
        let status: String = env
            .get_string(&status)
            .map(|text| text.into())
            .unwrap_or_default();
        let payload: String = env
            .get_string(&payload)
            .map(|text| text.into())
            .unwrap_or_default();
        if let Some(sender) = PICK_TX.lock().unwrap().take() {
            let _ = sender.send((status, payload));
        }
    }
}
