//! 学生评教（jxpj.szu.edu.cn）对外数据模型。
//!
//! 字段全部以 camelCase 输出给前端与 MCP；`choices` 保留服务端原始顺序，
//! 选择档位时的“原始下标”就来自这个顺序。

use serde::Serialize;

/// 当前评教学期（来自 `sso/login`）。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationTerm {
    pub id: i64,
    pub name: String,
}

/// 单个选项。`value_low`/`value_high` 是分值区间，`value_normal` 是滑条模式的落点分。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    pub label: String,
    pub value_low: String,
    pub value_high: String,
    pub value_normal: String,
}

/// 一道评价题。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationQuestion {
    pub sub_id: i64,
    pub sub_name: String,
    /// 服务端题型；目前只支持 `options`。
    pub kind: String,
    pub choices: Vec<ChoiceOption>,
}

/// 一条「学生-课程-教师」评教记录。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRecord {
    pub sc_id: i64,
    pub course_name: String,
    pub course_no: String,
    pub employee_name: String,
    pub class_name: String,
    pub current_times: i64,
    pub limited_times: i64,
    pub expired_date: Option<String>,
    /// 是否参评；服务端为 `false` 时表示课程未开课，不参与本次评教。
    pub participate: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetStatus {
    /// 有待评记录，可一键提交。
    Pending,
    /// 已全部评完（或全部不参评）。
    Done,
    /// 只有已过期记录。
    Expired,
    /// 存在暂不支持的题型，跳过不提交。
    Unsupported,
}

/// 一个评价目标（批次/指标）及其记录与题目。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationTarget {
    pub id: String,
    pub target_id: i64,
    pub target_name: String,
    pub term_name: String,
    pub expired_date: Option<String>,
    pub status: TargetStatus,
    pub message: Option<String>,
    pub score_type: i64,
    pub comment_required: i64,
    pub stay_seconds: i64,
    pub records: Vec<EvaluationRecord>,
    pub questions: Vec<EvaluationQuestion>,
}

impl EvaluationTarget {
    /// 真正会被提交的记录：参评、未过期、且未达到可评次数上限。
    pub fn actionable_records(&self) -> impl Iterator<Item = &EvaluationRecord> {
        self.records.iter().filter(|record| {
            record.participate
                && !is_expired(record.expired_date.as_deref())
                && (record.limited_times <= 0 || record.current_times < record.limited_times)
        })
    }
}

/// 已过期记录数（用于界面上提示“这些记录已过期”）。
pub fn expired_count(target: &EvaluationTarget) -> usize {
    target
        .records
        .iter()
        .filter(|record| record.participate && is_expired(record.expired_date.as_deref()))
        .count()
}

/// 概览：一次 `overview` 拉取的全部结果。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationOverview {
    pub fetched_at: i64,
    pub term: Option<EvaluationTerm>,
    pub comment: String,
    pub pending_targets: usize,
    pub pending_records: usize,
    pub expired_records: usize,
    pub targets: Vec<EvaluationTarget>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationConfig {
    pub comment: String,
}

/// 一条待提交的选择。
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedChoice {
    pub sc_id: i64,
    pub sub_id: i64,
    pub choice_label: String,
    /// 仅滑条模式（`scoreType == 4`）写入；单选模式省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

/// 一条待提交的评语。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedComment {
    pub sc_id: i64,
    pub comment: String,
}

/// 一个目标的完整提交内容（dry-run 时原样返回给调用方）。
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TargetPlan {
    pub target_id: i64,
    pub target_name: String,
    pub choices: Vec<PlannedChoice>,
    pub comments: Vec<PlannedComment>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunFailure {
    pub target_name: String,
    pub course_name: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRunSummary {
    pub submitted_targets: usize,
    pub submitted_records: usize,
    pub failures: Vec<RunFailure>,
}

/// 一次运行的结果（命令与 MCP 共用）。
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcome {
    pub dry_run: bool,
    pub summary: EvaluationRunSummary,
    pub plans: Vec<TargetPlan>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetState {
    Started,
    Ok,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TargetProgress {
    pub index: usize,
    pub total: usize,
    pub target_name: String,
    pub state: TargetState,
    pub message: Option<String>,
}

/// 运行过程中的 Channel 事件。
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum EvaluationRunEvent {
    Target(TargetProgress),
    Waiting { seconds: i64 },
    Done {
        summary: EvaluationRunSummary,
        dry_run: bool,
    },
}

/// 待办聚合只需要的最小摘要。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingSummary {
    pub pending_targets: usize,
    pub pending_records: usize,
    /// 最近一次拉取是否成功。`false` 时不得把缺失的待办当作“已完成”清理。
    pub trusted: bool,
}

/// `expiredDate` 形如 `2026-11-27`，按本地日期比较；缺失视为不过期。
pub fn is_expired(date: Option<&str>) -> bool {
    let Some(date) = date.map(str::trim).filter(|date| !date.is_empty()) else {
        return false;
    };
    let today = chrono::Local::now().date_naive().to_string();
    date < today.as_str()
}
