use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 待办注册项，与首页日历的 `ScheduleCategory` 逐字对应。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TodoKind {
    OjQuiz,
    UoocExam,
    UoocHomework,
    UoocCourseEnd,
    LmsHomework,
    ElectricityOutage,
}

impl TodoKind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::OjQuiz => "oj-quiz",
            Self::UoocExam => "uooc-exam",
            Self::UoocHomework => "uooc-homework",
            Self::UoocCourseEnd => "uooc-course-end",
            Self::LmsHomework => "lms-homework",
            Self::ElectricityOutage => "electricity-outage",
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TodoItem {
    pub id: String,
    pub kind: TodoKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub due_at_ms: i64,
    pub route: String,
    pub external_url: Option<String>,
    pub urgent: bool,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub(crate) enum TodosEvent {
    Initial(Box<Vec<TodoItem>>),
    Snapshot(Box<Vec<TodoItem>>),
}

/// 跨重启保存的提醒去重状态。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReminderState {
    /// 去重键 → 通知时间（Unix 毫秒）
    #[serde(default)]
    pub notified: BTreeMap<String, i64>,
    /// 移动端原生排程：去重键 → 通知 id
    #[serde(default)]
    pub scheduled: BTreeMap<String, i32>,
}

pub(crate) fn reminder_key(item: &TodoItem, offset_minutes: i64) -> String {
    format!(
        "{}|{}|{}|{}",
        item.kind.key(),
        item.id,
        item.due_at_ms,
        offset_minutes
    )
}
