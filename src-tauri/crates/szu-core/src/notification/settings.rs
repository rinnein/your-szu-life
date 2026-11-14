use serde::{Deserialize, Serialize};
use tauri::{Emitter, Runtime};

use super::model::TodoKind;

/// 待办提醒的默认提前量（分钟）：7天、3天、1天、2小时。
/// 其余预设（2天、1小时、30分、15分、5分）仍可在设置页勾选启用。
pub const DEFAULT_TODO_LEAD_MINUTES: [i64; 4] = [10_080, 4_320, 1_440, 120];

/// 一年的分钟数，作为提前量的硬上限，避免异常配置撑爆排程。
const MAX_LEAD_MINUTES: i64 = 525_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCategory {
    CampusNetwork,
    Electricity,
    Enrollment,
    Oj,
    Uooc,
    Todo,
}

/// 待办类通知的统一配置。
///
/// 六个注册项与首页日历的六类待办一一对应；`lead_minutes` 是模块级统一阈值，
/// 同一待办在每档各提醒一次。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TodoNotificationSettings {
    /// 待办提醒总开关
    pub enabled: bool,
    /// OJ 测验截止
    pub oj_quiz: bool,
    /// UOOC 考试截止
    pub uooc_exam: bool,
    /// UOOC 作业截止
    pub uooc_homework: bool,
    /// UOOC 课程结课
    pub uooc_course_end: bool,
    /// 畅课作业截止
    pub lms_homework: bool,
    /// 电费预估断电日期
    pub electricity_outage: bool,
    /// 签到/打卡（点名开始与进行中提醒）
    pub checkin: bool,
    /// 已启用的提前量（分钟）
    pub lead_minutes: Vec<i64>,
}

impl Default for TodoNotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            oj_quiz: true,
            uooc_exam: true,
            uooc_homework: true,
            uooc_course_end: true,
            lms_homework: true,
            electricity_outage: true,
            checkin: true,
            lead_minutes: DEFAULT_TODO_LEAD_MINUTES.to_vec(),
        }
    }
}

impl TodoNotificationSettings {
    /// 保存前清洗提前量：丢弃非正数、去重、按从大到小排序并限制上限。
    pub fn sanitized(mut self) -> Self {
        self.lead_minutes = normalized_lead_minutes(self.lead_minutes);
        self
    }

    pub fn is_kind_enabled(&self, kind: TodoKind) -> bool {
        if !self.enabled {
            return false;
        }
        match kind {
            TodoKind::OjQuiz => self.oj_quiz,
            TodoKind::UoocExam => self.uooc_exam,
            TodoKind::UoocHomework => self.uooc_homework,
            TodoKind::UoocCourseEnd => self.uooc_course_end,
            TodoKind::LmsHomework => self.lms_homework,
            TodoKind::ElectricityOutage => self.electricity_outage,
            TodoKind::Checkin => self.checkin,
        }
    }
}

fn normalized_lead_minutes(values: Vec<i64>) -> Vec<i64> {
    let mut values: Vec<i64> = values
        .into_iter()
        .filter(|value| *value > 0)
        .map(|value| value.min(MAX_LEAD_MINUTES))
        .collect();
    values.sort_unstable_by(|a, b| b.cmp(a));
    values.dedup();
    values
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    /// 全局总开关：若为 false，所有通知均不发送
    pub enabled: bool,
    /// 校园网异常通知
    pub campus_network: bool,
    /// 宿舍电费低电量预警
    pub electricity: bool,
    /// 选课任务状态通知
    pub enrollment: bool,
    /// OJ 新测验发布与登录状态提醒（截止提醒由待办模块负责）
    pub oj: bool,
    /// UOOC 测验需要处理提醒（截止提醒由待办模块负责）
    pub uooc: bool,
    /// 待办类通知
    #[serde(default)]
    pub todo: TodoNotificationSettings,
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
            todo: TodoNotificationSettings::default(),
        }
    }
}

impl NotificationSettings {
    pub fn is_category_enabled(&self, category: NotificationCategory) -> bool {
        if !self.enabled {
            return false;
        }
        match category {
            NotificationCategory::CampusNetwork => self.campus_network,
            NotificationCategory::Electricity => self.electricity,
            NotificationCategory::Enrollment => self.enrollment,
            NotificationCategory::Oj => self.oj,
            NotificationCategory::Uooc => self.uooc,
            NotificationCategory::Todo => self.todo.enabled,
        }
    }

    pub fn is_todo_kind_enabled(&self, kind: TodoKind) -> bool {
        self.enabled && self.todo.is_kind_enabled(kind)
    }
}

pub fn load<R: Runtime>(_app: &tauri::AppHandle<R>) -> NotificationSettings {
    crate::preferences::load::<NotificationSettings>(crate::preferences::NOTIFICATIONS)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn save<R: Runtime>(
    app: &tauri::AppHandle<R>,
    value: &NotificationSettings,
) -> Result<NotificationSettings, String> {
    let mut value = value.clone();
    value.todo = value.todo.sanitized();
    crate::preferences::save(crate::preferences::NOTIFICATIONS, &value)?;
    let _ = app.emit("notification-settings-changed", &value);
    Ok(value)
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
        assert!(settings.is_category_enabled(NotificationCategory::Todo));
    }

    #[test]
    fn default_todo_settings_enable_every_kind_and_default_lead_times() {
        let todo = TodoNotificationSettings::default();
        assert!(todo.enabled);
        for kind in [
            TodoKind::OjQuiz,
            TodoKind::UoocExam,
            TodoKind::UoocHomework,
            TodoKind::UoocCourseEnd,
            TodoKind::LmsHomework,
            TodoKind::ElectricityOutage,
        ] {
            assert!(todo.is_kind_enabled(kind), "{kind:?} should be enabled");
        }
        assert_eq!(
            todo.lead_minutes,
            vec![10_080, 4_320, 1_440, 120],
            "默认只勾选 7天、3天、1天、2小时"
        );
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
        assert!(!settings.is_category_enabled(NotificationCategory::Todo));
        assert!(!settings.is_todo_kind_enabled(TodoKind::OjQuiz));
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
    fn todo_kind_toggle_respects_module_switch() {
        let mut settings = NotificationSettings::default();
        settings.todo.uooc_exam = false;
        assert!(settings.is_todo_kind_enabled(TodoKind::OjQuiz));
        assert!(!settings.is_todo_kind_enabled(TodoKind::UoocExam));

        settings.todo.enabled = false;
        assert!(!settings.is_todo_kind_enabled(TodoKind::OjQuiz));
    }

    #[test]
    fn lead_minutes_are_sorted_deduplicated_and_clamped() {
        let todo = TodoNotificationSettings {
            lead_minutes: vec![60, 0, -5, 60, 10_080, MAX_LEAD_MINUTES + 1],
            ..TodoNotificationSettings::default()
        }
        .sanitized();
        assert_eq!(todo.lead_minutes, vec![MAX_LEAD_MINUTES, 10_080, 60]);
    }

    #[test]
    fn old_json_without_todo_field_uses_defaults() {
        let legacy = r#"{
            "enabled": true,
            "campusNetwork": true,
            "electricity": true,
            "enrollment": true,
            "oj": false,
            "uooc": true
        }"#;
        let settings: NotificationSettings = serde_json::from_str(legacy).unwrap();
        assert!(!settings.oj);
        assert!(settings.todo.enabled);
        assert_eq!(
            settings.todo.lead_minutes,
            DEFAULT_TODO_LEAD_MINUTES.to_vec()
        );
    }

    #[test]
    fn partial_todo_json_fills_missing_fields() {
        let partial = r#"{
            "enabled": true,
            "campusNetwork": true,
            "electricity": true,
            "enrollment": true,
            "oj": true,
            "uooc": true,
            "todo": { "enabled": false, "leadMinutes": [60] }
        }"#;
        let settings: NotificationSettings = serde_json::from_str(partial).unwrap();
        assert!(!settings.todo.enabled);
        assert!(settings.todo.oj_quiz);
        assert_eq!(settings.todo.lead_minutes, vec![60]);
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
            todo: TodoNotificationSettings {
                enabled: false,
                oj_quiz: true,
                uooc_exam: false,
                uooc_homework: true,
                uooc_course_end: false,
                lms_homework: true,
                electricity_outage: false,
                checkin: true,
                lead_minutes: vec![60, 5],
            },
        };
        let serialized = serde_json::to_string(&settings).unwrap();
        let deserialized: NotificationSettings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(settings, deserialized);
    }
}
