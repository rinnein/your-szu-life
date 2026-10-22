use super::network::NetworkSnapshot;
use serde::Deserialize;

#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidNetworkSnapshotPayload {
    connected: bool,
    #[serde(default)]
    ssid: Option<String>,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    capabilities_available: Option<bool>,
    #[serde(default)]
    wifi_permission: Option<String>,
    #[serde(default)]
    location_enabled: Option<bool>,
    #[serde(default)]
    has_private_172_ipv4: bool,
    #[serde(default)]
    wifi_connected: bool,
    #[serde(default)]
    wifi_ip: Option<String>,
    #[serde(default)]
    wifi_has_private_172_ipv4: bool,
}

#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
pub(super) fn parse_network_snapshot_json(payload: &str) -> Option<NetworkSnapshot> {
    let parsed = serde_json::from_str::<AndroidNetworkSnapshotPayload>(payload).ok()?;
    let transport = parsed.transport.and_then(normalize_text);
    let raw_ssid = parsed.ssid.and_then(|value| {
        let value = value.trim().trim_matches('"');
        (!value.is_empty()
            && !value.eq_ignore_ascii_case("<unknown ssid>")
            && !value.eq_ignore_ascii_case("unknown ssid"))
        .then(|| value.to_owned())
    });
    let ssid = if raw_ssid.is_some() {
        raw_ssid
    } else if transport.as_deref() == Some("ethernet") {
        Some("有线连接".to_string())
    } else {
        None
    };
    let ip = parsed.ip.and_then(|value| {
        let value = value.trim();
        value
            .parse::<std::net::Ipv4Addr>()
            .ok()
            .map(|address| address.to_string())
    });
    let wifi_ip = parsed.wifi_ip.and_then(|value| {
        let value = value.trim();
        value
            .parse::<std::net::Ipv4Addr>()
            .ok()
            .map(|address| address.to_string())
    });
    let wifi_connected = parsed.wifi_connected || wifi_ip.is_some();
    let connected = parsed.connected || wifi_connected || ip.is_some();
    Some(NetworkSnapshot {
        connected,
        ssid,
        ip,
        transport,
        capabilities_available: parsed.capabilities_available,
        wifi_permission: parsed.wifi_permission.and_then(normalize_text),
        location_enabled: parsed.location_enabled,
        has_private_172_ipv4: parsed.has_private_172_ipv4,
        wifi_connected,
        wifi_ip,
        wifi_has_private_172_ipv4: parsed.wifi_has_private_172_ipv4,
    })
}

fn normalize_text(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

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
            .new_string("in.rinne.your_szu_life.CampusNetworkBridge")
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

#[cfg(target_os = "android")]
pub(super) fn network_snapshot_result() -> Result<NetworkSnapshot, String> {
    with_bridge(|environment, class| {
        let value = environment
            .call_static_method(class, "getNetworkSnapshotJson", "()Ljava/lang/String;", &[])
            .map_err(|error| error.to_string())?
            .l()
            .map_err(|error| error.to_string())?;
        let value = jni::objects::JString::from(value);
        let value = environment
            .get_string(&value)
            .map_err(|error| error.to_string())?;
        parse_network_snapshot_json(value.to_str().map_err(|error| error.to_string())?)
            .ok_or_else(|| "Android 网络快照格式无效".to_owned())
    })
}

#[cfg(target_os = "android")]
pub(super) fn network_snapshot() -> NetworkSnapshot {
    network_snapshot_result().unwrap_or_default()
}

#[cfg(target_os = "android")]
pub(super) fn request_permissions() -> Result<(), String> {
    with_bridge(|environment, class| {
        environment
            .call_static_method(class, "requestWifiPermissions", "()V", &[])
            .map_err(|error| error.to_string())?;
        Ok(())
    })
}

#[cfg(not(target_os = "android"))]
pub(super) fn request_permissions() -> Result<(), String> {
    Ok(())
}
