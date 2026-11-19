use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub use szu_core::notification::TodoKind;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub id: String,
    pub kind: TodoKind,
    pub title: String,
    pub subtitle: Option<String>,
    /// 任务开始时间；没有明确开始时间的来源为空，不参与「任务开始」提醒。
    pub start_at_ms: Option<i64>,
    pub due_at_ms: i64,
    pub route: String,
    pub external_url: Option<String>,
    pub urgent: bool,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum TodosEvent {
    Initial(Box<Vec<TodoItem>>),
    Snapshot(Box<Vec<TodoItem>>),
}

/// 当前版本的状态已经带「初次探测」与「任务开始」基线语义；新安装不需要迁移旧记录。
pub(crate) const DISCOVERY_BASELINE_VERSION: u8 = 2;

/// 跨重启保存的提醒去重状态。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderState {
    /// 提前量去重键（`kind|id|due|offset`）→ 通知时间（Unix 毫秒）
    #[serde(default)]
    pub notified: BTreeMap<String, i64>,
    /// 移动端原生排程：去重键 → 通知 id
    #[serde(default)]
    pub scheduled: BTreeMap<String, i32>,
    /// 「初次探测」去重：`kind|id` → 最近一次看到该待办时的截止时间
    #[serde(default)]
    pub discovered: BTreeMap<String, i64>,
    /// 「任务开始」去重：`kind|id` → 已提醒过的开始时间
    #[serde(default)]
    pub started: BTreeMap<String, i64>,
    /// 每日只提醒一次的待办（电费预警）：`kind|id` → 最近一次提醒的本地日期
    #[serde(default)]
    pub notified_day: BTreeMap<String, String>,
    /// 0 表示旧版本状态（升级后需要为各来源静默建立探测基线）；
    /// 1 表示已建立「初次探测」基线，但还没有「任务开始」基线；
    /// 2 表示两者都已建立。
    #[serde(default)]
    pub discovery_baseline_version: u8,
    /// 升级基线已经覆盖过的待办类型，避免同一来源反复静默补基线。
    #[serde(default)]
    pub baselined_kinds: BTreeSet<TodoKind>,
}

impl Default for ReminderState {
    fn default() -> Self {
        Self {
            notified: BTreeMap::new(),
            scheduled: BTreeMap::new(),
            discovered: BTreeMap::new(),
            started: BTreeMap::new(),
            notified_day: BTreeMap::new(),
            // 注意：字段级 `serde(default)` 对旧 JSON 给出 0，与此处的 2 不同，
            // 正好用来区分「升级迁移」与「全新安装」。
            discovery_baseline_version: DISCOVERY_BASELINE_VERSION,
            baselined_kinds: BTreeSet::new(),
        }
    }
}

/// 「初次探测」与清理逻辑使用的稳定身份；不包含截止时间，改期不会重新触发。
pub(crate) fn item_key(item: &TodoItem) -> String {
    format!("{}|{}", item.kind.key(), item.id)
}

pub fn reminder_key(item: &TodoItem, offset_minutes: i64) -> String {
    format!(
        "{}|{}|{}|{}",
        item.kind.key(),
        item.id,
        item.due_at_ms,
        offset_minutes
    )
}
