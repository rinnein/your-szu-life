//! 通知相关的共享数据类型。
//!
//! [`TodoKind`] 同时用于待办聚合（`szu-todos`）与按类别细分的通知设置，
//! 因此放在 core，避免两者互相依赖。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
}
