//! 通知相关的共享数据类型。
//!
//! [`TodoKind`] 同时用于待办聚合（`szu-todos`）与按类别细分的通知设置，
//! 因此放在 core，避免两者互相依赖。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 待办注册项，与首页日历的 `ScheduleCategory` 逐字对应。
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
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
    pub fn key(self) -> &'static str {
        match self {
            Self::OjQuiz => "oj-quiz",
            Self::UoocExam => "uooc-exam",
            Self::UoocHomework => "uooc-homework",
            Self::UoocCourseEnd => "uooc-course-end",
            Self::LmsHomework => "lms-homework",
            Self::ElectricityOutage => "electricity-outage",
        }
    }

    /// 电费预警是持续状态而非一次性任务：估算每天都会变，如果沿用带 `due_at` 的
    /// 去重键，四档提前量会随估算抖动反复重新上膛。这类待办改为「同一天最多提醒
    /// 一次」，桌面端由提醒引擎节流，Android 也只把最接近截止的一档交给原生排程。
    pub fn notify_at_most_daily(self) -> bool {
        matches!(self, Self::ElectricityOutage)
    }
}
