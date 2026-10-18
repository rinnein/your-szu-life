use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Mode {
    Yolo,
    Approval,
    #[default]
    Supervised,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Stage {
    #[default]
    Idle,
    Preparing,
    Generating,
    AwaitingChoice,
    AwaitingApproval,
    Submitting,
    Verifying,
    Passed,
    Failed,
    Manual,
    Disabled,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Kind {
    Single,
    Multiple,
    Text,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OptionItem {
    pub id: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Question {
    pub id: String,
    pub text: String,
    pub kind: Kind,
    pub options: Vec<OptionItem>,
    pub blanks: usize,
    #[serde(default)]
    pub images: Vec<String>,
}
pub(crate) type Answers = BTreeMap<String, Vec<String>>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Review {
    pub question_id: String,
    pub answers: Vec<String>,
    pub probabilities: BTreeMap<String, f64>,
    pub message: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSessionEntry {
    pub batch_index: usize,
    pub question_ids: Vec<String>,
    pub question_titles: Vec<String>,
    pub model: String,
    pub prompt: String,
    pub response: String,
    pub answers: BTreeMap<String, Vec<String>>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuizState {
    pub account_id: String,
    pub cid: String,
    pub task_id: String,
    pub resource_id: String,
    pub title: String,
    pub mode: Mode,
    pub stage: Stage,
    pub revision: u64,
    pub questions: Vec<Question>,
    pub answers: Answers,
    pub reviews: Vec<Review>,
    pub message: String,
    pub gated: bool,
    pub video_complete: bool,
    pub signature: String,
    #[serde(default)]
    pub submission_uncertain: bool,
    pub passed: Vec<String>,
    pub notified: String,
    #[serde(default)]
    pub ai_sessions: Vec<AiSessionEntry>,
}
pub(crate) fn validate_answers(
    questions: &[Question],
    answers: &Answers,
    complete: bool,
) -> Result<(), String> {
    if answers
        .keys()
        .any(|id| !questions.iter().any(|q| &q.id == id))
    {
        return Err("答案中存在未知题目".into());
    }
    for q in questions {
        let empty = vec![];
        let values = answers.get(&q.id).unwrap_or(&empty);
        if !complete && values.is_empty() {
            continue;
        }
        if values.is_empty()
            || values
                .iter()
                .any(|v| v.trim().is_empty() || v.len() > 32000)
        {
            return Err("请填写全部题目的答案".into());
        }
        match q.kind {
            Kind::Text if values.len() != q.blanks => {
                return Err("填写数量与题目空格数量不一致".into())
            }
            Kind::Text => {}
            _ => {
                let unique: std::collections::HashSet<_> = values.iter().collect();
                if unique.len() != values.len()
                    || (q.kind == Kind::Single && values.len() != 1)
                    || values
                        .iter()
                        .any(|id| !q.options.iter().any(|o| &o.id == id))
                {
                    return Err("选择题答案不完整或包含无效选项".into());
                }
            }
        }
    }
    Ok(())
}
