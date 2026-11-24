//! 系统权限状态与申请。
//!
//! 权限的文案与分组由前端持有，Rust 只负责把稳定的 id 映射到平台能力并返回
//! 状态。Android 实现见 [`android`]；桌面端没有任何运行时权限，统一返回
//! `unsupported`，前端也不会展示对应入口。

pub mod commands;

#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod desktop;

use serde::{Deserialize, Serialize};

/// 与 Kotlin `PermissionBridge`、前端 `src/permissions.ts` 共享的稳定 id。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionId {
    Notifications,
    PromotedNotifications,
    Location,
    NearbyWifi,
    Camera,
    InstallPackages,
    ExactAlarm,
    /// 厂商「自启动/后台运行」管理：没有官方 API，只能引导用户手动开启。
    AutoStart,
    /// 电池优化白名单：官方 API 可检测，也只能通过系统入口申请。
    BatteryOptimization,
}

impl PermissionId {
    /// 输出顺序固定：前端按这个顺序落位，不依赖原生数组的顺序。
    pub const ALL: [PermissionId; 9] = [
        PermissionId::Notifications,
        PermissionId::PromotedNotifications,
        PermissionId::Location,
        PermissionId::NearbyWifi,
        PermissionId::Camera,
        PermissionId::InstallPackages,
        PermissionId::ExactAlarm,
        PermissionId::AutoStart,
        PermissionId::BatteryOptimization,
    ];

    #[cfg(any(target_os = "android", test))]
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionId::Notifications => "notifications",
            PermissionId::PromotedNotifications => "promotedNotifications",
            PermissionId::Location => "location",
            PermissionId::NearbyWifi => "nearbyWifi",
            PermissionId::Camera => "camera",
            PermissionId::InstallPackages => "installPackages",
            PermissionId::ExactAlarm => "exactAlarm",
            PermissionId::AutoStart => "autoStart",
            PermissionId::BatteryOptimization => "batteryOptimization",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    Granted,
    Denied,
    /// 系统版本或硬件不支持该能力。
    Unsupported,
    /// 权限已授予，但相关系统开关被关闭（例如系统定位服务）。
    SystemDisabled,
    /// 只能由用户在厂商/系统设置里手动确认，应用无法读取真实状态。
    Manual,
    Unknown,
}

impl PermissionStatus {
    #[cfg(any(target_os = "android", test))]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "granted" => Some(PermissionStatus::Granted),
            "denied" => Some(PermissionStatus::Denied),
            "unsupported" => Some(PermissionStatus::Unsupported),
            "systemDisabled" => Some(PermissionStatus::SystemDisabled),
            "manual" => Some(PermissionStatus::Manual),
            "unknown" => Some(PermissionStatus::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStateView {
    pub id: PermissionId,
    pub status: PermissionStatus,
    pub can_request: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 打开与某一项权限相关的系统设置页。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionSettingsTarget {
    App,
    Notifications,
    Location,
    InstallPackages,
    ExactAlarm,
    AutoStart,
    BatteryOptimization,
}

impl PermissionSettingsTarget {
    #[cfg(target_os = "android")]
    fn as_str(self) -> &'static str {
        match self {
            PermissionSettingsTarget::App => "app",
            PermissionSettingsTarget::Notifications => "notifications",
            PermissionSettingsTarget::Location => "location",
            PermissionSettingsTarget::InstallPackages => "installPackages",
            PermissionSettingsTarget::ExactAlarm => "exactAlarm",
            PermissionSettingsTarget::AutoStart => "autoStart",
            PermissionSettingsTarget::BatteryOptimization => "batteryOptimization",
        }
    }
}

/// Kotlin 返回的单条原始状态。
#[cfg(any(target_os = "android", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativePermissionState {
    id: String,
    status: String,
    #[serde(default)]
    can_request: bool,
    #[serde(default)]
    note: Option<String>,
}

/// 把原生 JSON 映射成固定顺序、固定 id 集合的视图。
///
/// 原生缺失的项（例如旧版本桥接没有上报新权限）按 `unknown` 补齐；未知 id 丢弃，
/// 前端因此永远拿到完整目录，不会出现「某一行突然消失」。
#[cfg(any(target_os = "android", test))]
fn parse_native_states(payload: &str) -> Result<Vec<PermissionStateView>, String> {
    let native: Vec<NativePermissionState> =
        serde_json::from_str(payload).map_err(|error| format!("系统权限状态格式无效：{error}"))?;
    let by_id: std::collections::HashMap<&str, &NativePermissionState> =
        native.iter().map(|item| (item.id.as_str(), item)).collect();
    Ok(PermissionId::ALL
        .into_iter()
        .map(|id| {
            let item = by_id.get(id.as_str());
            PermissionStateView {
                id,
                status: item
                    .and_then(|value| PermissionStatus::parse(&value.status))
                    .unwrap_or(PermissionStatus::Unknown),
                can_request: item.is_some_and(|value| value.can_request),
                note: item.and_then(|value| value.note.clone()),
            }
        })
        .collect())
}

pub fn states() -> Result<Vec<PermissionStateView>, String> {
    #[cfg(target_os = "android")]
    {
        android::states()
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(desktop::states())
    }
}

pub async fn request(keys: &[PermissionId]) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        android::request(keys).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = keys;
        Err("当前平台没有可申请的系统权限".to_string())
    }
}

pub fn open_settings(target: PermissionSettingsTarget) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        android::open_settings(target)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = target;
        Err("当前平台没有可打开的系统权限设置".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_states_are_mapped_in_stable_order() {
        let payload = r#"[
            {"id":"camera","status":"denied","canRequest":true},
            {"id":"notifications","status":"granted","canRequest":false,"note":"已开启"},
            {"id":"bogus","status":"granted","canRequest":true}
        ]"#;
        let states = parse_native_states(payload).unwrap();
        assert_eq!(states.len(), PermissionId::ALL.len());
        assert_eq!(states[0].id, PermissionId::Notifications);
        assert_eq!(states[0].status, PermissionStatus::Granted);
        assert_eq!(states[0].note.as_deref(), Some("已开启"));
        // 未上报的项补 unknown，未知 id 被丢弃。
        assert_eq!(states[1].status, PermissionStatus::Unknown);
        assert_eq!(states[4].id, PermissionId::Camera);
        assert_eq!(states[4].status, PermissionStatus::Denied);
        assert!(states[4].can_request);
    }

    #[test]
    fn native_states_reject_malformed_payload() {
        assert!(parse_native_states("not json").is_err());
    }

    #[test]
    fn ids_match_the_frontend_contract() {
        assert_eq!(
            serde_json::to_string(&PermissionId::PromotedNotifications).unwrap(),
            "\"promotedNotifications\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionSettingsTarget::InstallPackages).unwrap(),
            "\"installPackages\""
        );
        let parsed: PermissionId = serde_json::from_str("\"nearbyWifi\"").unwrap();
        assert_eq!(parsed, PermissionId::NearbyWifi);
        assert!(serde_json::from_str::<PermissionId>("\"bogus\"").is_err());
        assert_eq!(
            serde_json::to_string(&PermissionId::AutoStart).unwrap(),
            "\"autoStart\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionId::BatteryOptimization).unwrap(),
            "\"batteryOptimization\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionSettingsTarget::BatteryOptimization).unwrap(),
            "\"batteryOptimization\""
        );
    }

    #[test]
    fn manual_status_is_parsed_and_kept_in_tail_order() {
        assert_eq!(
            PermissionStatus::parse("manual"),
            Some(PermissionStatus::Manual)
        );
        // 自启动与电池优化追加在末尾，既有项的索引不变。
        assert_eq!(PermissionId::ALL.len(), 9);
        assert_eq!(PermissionId::ALL[7], PermissionId::AutoStart);
        assert_eq!(PermissionId::ALL[8], PermissionId::BatteryOptimization);
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn desktop_reports_every_permission_as_unsupported() {
        let states = states().unwrap();
        assert_eq!(states.len(), PermissionId::ALL.len());
        assert!(states
            .iter()
            .all(|state| state.status == PermissionStatus::Unsupported));
    }
}
