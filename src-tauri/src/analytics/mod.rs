//! 匿名使用统计。
//!
//! App Key 由编译期环境变量 `APTABASE_APP_KEY` 注入；为空时 Aptabase 客户端
//! 内部会关闭，不会产生任何网络请求。用户开关存于 `settings.sqlite`，默认开启。
//!
//! 页面浏览由前端经插件的 `track_event` 命令上报；启动/退出这类进程级事件
//! 在这里统一上报，保证它们同样受用户开关控制。

pub(crate) mod commands;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_aptabase::EventTracker;

use crate::preferences;

pub(crate) fn app_key() -> &'static str {
    option_env!("APTABASE_APP_KEY").map(str::trim).unwrap_or("")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct AnalyticsSettings {
    pub(crate) enabled: bool,
}

impl Default for AnalyticsSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub(crate) fn load() -> AnalyticsSettings {
    preferences::load::<AnalyticsSettings>(preferences::ANALYTICS_ENABLED)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save(
    app: &AppHandle,
    value: &AnalyticsSettings,
) -> Result<AnalyticsSettings, String> {
    preferences::save(preferences::ANALYTICS_ENABLED, value)?;
    let _ = app.emit("analytics-settings-changed", value);
    Ok(value.clone())
}

pub(crate) fn track(app: &AppHandle, name: &str, props: Option<serde_json::Value>) {
    if !load().enabled {
        return;
    }
    if let Err(error) = app.track_event(name, props) {
        log::warn!("发送统计事件 {name} 失败: {error}");
    }
}

pub(crate) fn flush(app: &AppHandle) {
    app.flush_events_blocking();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytics_defaults_to_enabled() {
        assert!(AnalyticsSettings::default().enabled);
    }
}
