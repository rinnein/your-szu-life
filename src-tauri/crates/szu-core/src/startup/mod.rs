//! 开机自启与静默启动。
//!
//! 偏好保存在 `settings.sqlite`（设备本地，不参与设备同步）：`autostart` 缺省开启，
//! `silent` 缺省关闭。真正的系统自启项由 `tauri-plugin-autostart` 维护；Linux 下
//! 插件优先读取 `$APPIMAGE`，因此每次启动都重写一次条目可以顺带修正镜像被移动后
//! 的失效路径。
//!
//! 「静默启动」只影响由自启拉起的那一次：自启条目固定带 [`AUTOSTART_ARG`]，
//! 应用据此判断启动来源，再结合 `silent` 偏好决定是否显示主窗口。手动启动不带该
//! 参数，因此永远显示窗口。

pub mod commands;

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use tauri::AppHandle;

use crate::preferences;

pub const AUTOSTART: &str = "startup.autostart";
pub const SILENT: &str = "startup.silent";
/// 写进系统自启条目的启动参数，只表示「由开机自启拉起」。
pub const AUTOSTART_ARG: &str = "--autostart";

const DEFAULT_AUTOSTART: bool = true;
const DEFAULT_SILENT: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupSettings {
    pub autostart: bool,
    pub silent: bool,
    /// 当前构建是否真的会管理系统自启项：移动端与开发构建为 `false`。
    pub supported: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            autostart: DEFAULT_AUTOSTART,
            silent: DEFAULT_SILENT,
            supported: platform_supported(),
        }
    }
}

/// 是否具备管理自启项的能力。开发构建刻意不写系统自启项：否则 `cargo run`
/// 会把 debug 二进制注册为登录启动项。
pub fn platform_supported() -> bool {
    cfg!(desktop) && !cfg!(dev)
}

/// 读取偏好。缺失时返回默认值但不写盘；命令与启动流程都不因此产生副作用。
pub fn load() -> Result<StartupSettings, String> {
    let autostart = preferences::load::<bool>(AUTOSTART)?.unwrap_or(DEFAULT_AUTOSTART);
    let silent = preferences::load::<bool>(SILENT)?.unwrap_or(DEFAULT_SILENT);
    Ok(StartupSettings {
        autostart,
        silent,
        supported: platform_supported(),
    })
}

/// 保存偏好并同步系统自启项。
///
/// **先**改系统条目，成功后才落盘：写入失败时界面回滚，偏好仍代表上一次确认
/// 落盘的状态，不会出现「开关显示开、系统里却没有条目」。
pub fn save(app: &AppHandle, incoming: &StartupSettings) -> Result<StartupSettings, String> {
    if !platform_supported() {
        return Err("当前平台不支持开机自启".to_string());
    }
    apply_autostart(app, incoming.autostart)?;
    preferences::save(AUTOSTART, &incoming.autostart)?;
    preferences::save(SILENT, &incoming.silent)?;
    Ok(StartupSettings {
        autostart: incoming.autostart,
        silent: incoming.silent,
        supported: true,
    })
}

/// 启动时把系统条目同步到偏好：开启则重写（刷新 AppImage/可执行文件路径），
/// 关闭则移除。失败只记录日志，不影响应用启动。
pub fn sync_at_launch(app: &AppHandle) {
    if !platform_supported() {
        return;
    }
    match load_persisting_defaults() {
        Ok(settings) => {
            if let Err(error) = apply_autostart(app, settings.autostart) {
                log::warn!("同步开机自启失败: {error}");
            }
        }
        Err(error) => log::warn!("读取开机自启设置失败: {error}"),
    }
}

/// 首次运行时把默认值落盘，让「默认开启」成为一次性的显式状态。
fn load_persisting_defaults() -> Result<StartupSettings, String> {
    let stored_autostart = preferences::load::<bool>(AUTOSTART)?;
    if stored_autostart.is_none() {
        preferences::save(AUTOSTART, &DEFAULT_AUTOSTART)?;
    }
    let stored_silent = preferences::load::<bool>(SILENT)?;
    if stored_silent.is_none() {
        preferences::save(SILENT, &DEFAULT_SILENT)?;
    }
    Ok(StartupSettings {
        autostart: stored_autostart.unwrap_or(DEFAULT_AUTOSTART),
        silent: stored_silent.unwrap_or(DEFAULT_SILENT),
        supported: platform_supported(),
    })
}

#[cfg(desktop)]
fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| {
        format!(
            "无法{}开机自启：{error}",
            if enabled { "开启" } else { "关闭" }
        )
    })
}

#[cfg(not(desktop))]
fn apply_autostart(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
    Err("当前平台不支持开机自启".to_string())
}

/// 启动参数里是否带 [`AUTOSTART_ARG`]。
pub fn launched_by_autostart(args: &[String]) -> bool {
    args.iter().any(|arg| arg == AUTOSTART_ARG)
}

/// 本次启动是否应保持隐藏：由自启拉起且用户开启了静默启动。
pub fn should_start_hidden(args: &[String], settings: &StartupSettings) -> bool {
    launched_by_autostart(args) && settings.silent
}

/// 读取当前进程参数并判断启动来源。
pub fn current_launch_is_autostart() -> bool {
    std::env::args_os()
        .skip(1)
        .any(|arg| arg.as_os_str() == OsStr::new(AUTOSTART_ARG))
}

/// 桌面启动流程使用：带自启参数且偏好为静默时返回 `true`。
pub fn should_start_hidden_now() -> bool {
    if !current_launch_is_autostart() {
        return false;
    }
    match load() {
        Ok(settings) => settings.silent,
        Err(error) => {
            log::warn!("读取静默启动设置失败，按显示窗口处理: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(autostart: bool, silent: bool) -> StartupSettings {
        StartupSettings {
            autostart,
            silent,
            supported: true,
        }
    }

    #[test]
    fn autostart_arg_matches_exactly() {
        let args = vec![AUTOSTART_ARG.to_string()];
        assert!(launched_by_autostart(&args));
        assert!(!launched_by_autostart(&["--autostarted".to_string()]));
        assert!(!launched_by_autostart(&["autostart".to_string()]));
        assert!(!launched_by_autostart(&[]));
    }

    #[test]
    fn hidden_only_when_autostarted_and_silent() {
        let args = vec![AUTOSTART_ARG.to_string()];
        assert!(should_start_hidden(&args, &settings(true, true)));
        assert!(!should_start_hidden(&args, &settings(true, false)));
        assert!(!should_start_hidden(&[], &settings(true, true)));
    }

    #[test]
    fn defaults_are_autostart_on_and_silent_off() {
        let defaults = StartupSettings::default();
        assert!(defaults.autostart);
        assert!(!defaults.silent);
        assert_eq!(defaults.supported, platform_supported());
    }

    #[test]
    fn settings_serialize_with_camel_case_fields() {
        let json = serde_json::to_string(&settings(true, false)).unwrap();
        assert_eq!(
            json,
            r#"{"autostart":true,"silent":false,"supported":true}"#
        );
    }

    #[test]
    fn dev_and_mobile_builds_report_unsupported() {
        let expected = cfg!(desktop) && !cfg!(dev);
        assert_eq!(platform_supported(), expected);
    }
}
