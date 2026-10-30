use serde::{Deserialize, Serialize};
use tauri::{Emitter, Runtime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NotificationCategory {
    CampusNetwork,
    Electricity,
    Enrollment,
    Oj,
    Uooc,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationSettings {
    /// 全局总开关：若为 false，所有通知均不发送
    pub enabled: bool,
    /// 校园网异常通知
    pub campus_network: bool,
    /// 宿舍电费低电量预警
    pub electricity: bool,
    /// 选课任务状态通知
    pub enrollment: bool,
    /// OJ 竞赛/测验提醒
    pub oj: bool,
    /// UOOC 测验需要处理提醒
    pub uooc: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            campus_network: true,
            electricity: true,
            enrollment: true,
            oj: true,
            uooc: true,
        }
    }
}

impl NotificationSettings {
    pub(crate) fn is_category_enabled(&self, category: NotificationCategory) -> bool {
        if !self.enabled {
            return false;
        }
        match category {
            NotificationCategory::CampusNetwork => self.campus_network,
            NotificationCategory::Electricity => self.electricity,
            NotificationCategory::Enrollment => self.enrollment,
            NotificationCategory::Oj => self.oj,
            NotificationCategory::Uooc => self.uooc,
        }
    }
}

pub(crate) fn load<R: Runtime>(_app: &tauri::AppHandle<R>) -> NotificationSettings {
    crate::preferences::load::<NotificationSettings>(crate::preferences::NOTIFICATIONS)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save<R: Runtime>(
    app: &tauri::AppHandle<R>,
    value: &NotificationSettings,
) -> Result<NotificationSettings, String> {
    crate::preferences::save(crate::preferences::NOTIFICATIONS, value)?;
    let _ = app.emit("notification-settings-changed", value);
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_enables_all_categories() {
        let settings = NotificationSettings::default();
        assert!(settings.enabled);
        assert!(settings.is_category_enabled(NotificationCategory::CampusNetwork));
        assert!(settings.is_category_enabled(NotificationCategory::Electricity));
        assert!(settings.is_category_enabled(NotificationCategory::Enrollment));
        assert!(settings.is_category_enabled(NotificationCategory::Oj));
        assert!(settings.is_category_enabled(NotificationCategory::Uooc));
    }

    #[test]
    fn disabled_master_switch_overrides_categories() {
        let mut settings = NotificationSettings::default();
        settings.enabled = false;
        assert!(!settings.is_category_enabled(NotificationCategory::CampusNetwork));
        assert!(!settings.is_category_enabled(NotificationCategory::Electricity));
        assert!(!settings.is_category_enabled(NotificationCategory::Enrollment));
        assert!(!settings.is_category_enabled(NotificationCategory::Oj));
        assert!(!settings.is_category_enabled(NotificationCategory::Uooc));
    }

    #[test]
    fn individual_category_toggle_works() {
        let mut settings = NotificationSettings::default();
        settings.electricity = false;
        assert!(settings.is_category_enabled(NotificationCategory::CampusNetwork));
        assert!(!settings.is_category_enabled(NotificationCategory::Electricity));
        assert!(settings.is_category_enabled(NotificationCategory::Oj));
    }

    #[test]
    fn json_roundtrip() {
        let settings = NotificationSettings {
            enabled: true,
            campus_network: false,
            electricity: true,
            enrollment: false,
            oj: true,
            uooc: false,
        };
        let serialized = serde_json::to_string(&settings).unwrap();
        let deserialized: NotificationSettings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(settings, deserialized);
    }
}
