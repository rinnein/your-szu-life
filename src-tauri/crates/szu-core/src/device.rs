//! 本机可读名称：桌面取 hostname，Android 优先取有意义的 hostname，其次取设备型号。
//!
//! 返回 `None` 表示系统没有提供可用名称；上层会回退到稳定的随机英文词。

const PLACEHOLDER_NAMES: [&str; 8] = [
    "localhost",
    "localhost.localdomain",
    "ip6-localhost",
    "ip6-loopback",
    "unknown",
    "none",
    "null",
    "android",
];

/// 读取当前平台可用的本机名称。
pub fn system_name() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        meaningful(&tauri_plugin_os::hostname()).or_else(android_model)
    }
    #[cfg(not(target_os = "android"))]
    {
        meaningful(&tauri_plugin_os::hostname())
    }
}

/// 过滤空值、占位名与回环/未指定 IP。
fn meaningful(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() {
        return None;
    }
    if PLACEHOLDER_NAMES
        .iter()
        .any(|placeholder| name.eq_ignore_ascii_case(placeholder))
    {
        return None;
    }
    let bare = name
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(name);
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        if ip.is_loopback() || ip.is_unspecified() {
            return None;
        }
    }
    Some(name.to_owned())
}

/// 组合 Android 的厂商与型号；型号已包含厂商前缀时只保留型号。
#[cfg(any(target_os = "android", test))]
fn combine_model(manufacturer: Option<String>, model: Option<String>) -> Option<String> {
    let model = model.and_then(|value| meaningful(&value));
    let manufacturer = manufacturer.and_then(|value| meaningful(&value));
    let combined = match (manufacturer, model) {
        (Some(manufacturer), Some(model))
            if !model
                .to_ascii_lowercase()
                .starts_with(&manufacturer.to_ascii_lowercase()) =>
        {
            format!("{manufacturer} {model}")
        }
        (_, Some(model)) => model,
        (Some(manufacturer), None) => manufacturer,
        (None, None) => return None,
    };
    meaningful(&combined)
}

#[cfg(target_os = "android")]
fn android_model() -> Option<String> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let context = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }.ok()?;
        let mut environment = vm.attach_current_thread().ok()?;
        let model = static_string(&mut environment, "MODEL");
        let manufacturer = static_string(&mut environment, "MANUFACTURER");
        combine_model(manufacturer, model)
    }));
    result.ok().flatten()
}

#[cfg(target_os = "android")]
fn static_string(environment: &mut jni::JNIEnv<'_>, name: &str) -> Option<String> {
    use jni::objects::JString;

    let value = environment
        .get_static_field("android/os/Build", name, "Ljava/lang/String;")
        .ok()?;
    let object = value.l().ok()?;
    if object.is_null() {
        return None;
    }
    let string = JString::from(object);
    environment
        .get_string(&string)
        .ok()
        .map(|value| value.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meaningful_rejects_placeholders_and_loopback_addresses() {
        for value in [
            "",
            "   ",
            "localhost",
            "LOCALHOST",
            "localhost.localdomain",
            "ip6-localhost",
            "unknown",
            "none",
            "null",
            "android",
            "127.0.0.1",
            "127.1.2.3",
            "::1",
            "[::1]",
            "0.0.0.0",
            "::",
        ] {
            assert!(meaningful(value).is_none(), "{value:?} should be rejected");
        }
    }

    #[test]
    fn meaningful_keeps_real_names() {
        assert_eq!(meaningful("  my-laptop  ").as_deref(), Some("my-laptop"));
        assert_eq!(meaningful("Pixel 7").as_deref(), Some("Pixel 7"));
    }

    #[test]
    fn model_keeps_manufacturer_prefix() {
        assert_eq!(
            combine_model(Some("samsung".into()), Some("Samsung SM-G991B".into())).as_deref(),
            Some("Samsung SM-G991B")
        );
    }

    #[test]
    fn model_combines_manufacturer_when_needed() {
        assert_eq!(
            combine_model(Some("Google".into()), Some("Pixel 7".into())).as_deref(),
            Some("Google Pixel 7")
        );
    }

    #[test]
    fn model_falls_back_to_whichever_side_is_present() {
        assert_eq!(
            combine_model(Some("Google".into()), Some("unknown".into())).as_deref(),
            Some("Google")
        );
        assert_eq!(
            combine_model(None, Some("Pixel 7".into())).as_deref(),
            Some("Pixel 7")
        );
        assert!(combine_model(None, None).is_none());
    }
}
