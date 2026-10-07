use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Campus,
    Webvpn,
}
impl Mode {
    pub fn origin(self) -> &'static str {
        match self {
            Self::Campus => "http://bkxk.szu.edu.cn",
            Self::Webvpn => "https://bkxk.webvpn.szu.edu.cn",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Gear {
    Low,
    #[default]
    Medium,
    High,
    Ultra,
}
impl Gear {
    pub fn interval(self) -> u64 {
        match self {
            Self::Low => 60000,
            Self::Medium => 10000,
            Self::High => 1000,
            Self::Ultra => 250,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub id: String,
    pub code: String,
    pub name: String,
    pub teacher: String,
    pub category: String,
    pub credits: Option<f64>,
    pub schedule: String,
    pub capacity: Option<i64>,
    pub remaining: Option<i64>,
    pub campus: String,
    pub main_select: Option<bool>,
    pub has_test: bool,
    #[serde(default)]
    pub test_ids: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub student: String,
    pub term: String,
    pub batch: String,
    pub campus: String,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Pending,
    Monitoring,
    Dropping,
    Selecting,
    Restoring,
    Uncertain,
    Completed,
    Stopped,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub course: Course,
    pub old_course: Option<Course>,
    pub threshold: u32,
    pub group: Option<String>,
    pub priority: i32,
    pub gear: Gear,
    pub failures: u32,
    #[serde(default)]
    pub attempts: u64,
    pub phase: Phase,
    pub message: String,
    pub next_at: i64,
    pub started_at: Option<i64>,
    pub enabled: bool,
    pub pending: Option<PendingWrite>,
    #[serde(default)]
    pub one_shot: bool,
    #[serde(default)]
    pub goal_observed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWrite {
    pub class_id: String,
    pub dropping: bool,
    pub phase: Phase,
    pub sent_at: i64,
    #[serde(default)]
    pub accepted: bool,
}
impl Task {
    pub fn failed(&mut self) {
        self.failures += 1;
        let lower = match self.gear {
            Gear::Ultra if self.failures >= 5 => Some(Gear::High),
            Gear::High if self.failures >= 30 => Some(Gear::Medium),
            _ => None,
        };
        if let Some(gear) = lower {
            self.gear = gear;
            self.failures = 0;
        }
    }
    pub fn switch_gear(&mut self, gear: Gear) {
        self.gear = gear;
        self.failures = 0;
    }
    pub fn active(&self) -> bool {
        self.enabled && !matches!(self.phase, Phase::Completed | Phase::Stopped)
    }
    pub fn confirmed(&mut self, selected: bool) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        if selected == pending.dropping {
            return;
        }
        self.pending = None;
        self.message.clear();
        match pending.phase {
            Phase::Dropping => {
                self.phase = Phase::Selecting;
                self.switch_gear(Gear::Ultra);
            }
            Phase::Restoring => {
                self.phase = Phase::Monitoring;
                self.switch_gear(Gear::Medium);
            }
            _ => {
                self.phase = Phase::Completed;
                self.enabled = false;
            }
        }
        self.next_at = 0;
    }
    pub fn rejected(&mut self, full: bool, permanent: bool, message: String) {
        let phase = self.pending.take().map(|p| p.phase).unwrap_or(self.phase);
        self.phase = phase;
        self.message = message;
        self.failed();
        if full && phase == Phase::Selecting && self.old_course.is_some() {
            self.phase = Phase::Restoring;
            self.switch_gear(Gear::Ultra);
            self.next_at = 0;
        } else if permanent || phase == Phase::Dropping || self.one_shot {
            self.enabled = false;
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub mode: Mode,
    pub latency: Option<u64>,
    pub needs_2fa: bool,
    pub message: String,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Authentication {
    #[default]
    Unknown,
    Authenticated,
    Expired,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Admission {
    #[default]
    Unchecked,
    Ready,
    Restricted,
    Initializing,
    Unavailable,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub mode: Mode,
    pub context: Context,
    pub logged_in: bool,
    #[serde(default)]
    pub authentication: Authentication,
    #[serde(default)]
    pub admission: Admission,
    #[serde(default)]
    pub admission_message: String,
    pub message: String,
    pub courses: Vec<Course>,
    pub selected: Vec<Course>,
    pub tasks: Vec<Task>,
    pub pins: Vec<String>,
    pub updated_at: Option<i64>,
    pub complete: bool,
    pub running: bool,
    pub query: Query,
    pub academic: Option<serde_json::Value>,
    #[serde(default)]
    pub academic_recalculation: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub category: String,
    pub keyword: String,
    pub page: u32,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            category: "TJKC".into(),
            keyword: String::new(),
            page: 0,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub course: Course,
    pub old_course: Option<Course>,
    pub threshold: u32,
    pub group: Option<String>,
    pub priority: i32,
    pub gear: Gear,
    pub start_at: Option<i64>,
    pub risk_confirmed: bool,
    #[serde(default)]
    pub one_shot: bool,
}

impl Snapshot {
    pub fn authenticated(&mut self) {
        self.authentication = Authentication::Authenticated;
        self.logged_in = true;
    }
    pub fn admission_failure(&mut self, error: &super::protocol::Error) {
        use super::protocol::Failure;
        self.admission_message = error.message.clone();
        self.admission = match error.kind {
            Failure::AdmissionRestricted => {
                self.authenticated();
                Admission::Restricted
            }
            Failure::Session | Failure::Needs2fa => {
                self.authentication = Authentication::Expired;
                self.logged_in = false;
                Admission::Unchecked
            }
            Failure::Initializing => Admission::Initializing,
            _ => Admission::Unavailable,
        };
    }
    pub fn can_select(&self) -> bool {
        self.authentication == Authentication::Authenticated && self.admission == Admission::Ready
    }
}

pub fn reconcile_membership(task: &mut Task, selected: &[Course]) {
    let standalone_drop =
        task.old_course.is_none() && task.pending.as_ref().is_some_and(|p| p.dropping);
    if let Some(p) = task.pending.as_ref() {
        let present = selected.iter().any(|c| c.id == p.class_id);
        task.confirmed(present);
    }
    if standalone_drop {
        return;
    }
    let target = selected.iter().any(|c| c.id == task.course.id);
    if target && (task.active() || task.goal_observed) {
        if task.pending.is_some() {
            task.goal_observed = true;
            task.message = "目标课程已选中，正在核验未决操作".into();
        } else {
            task.phase = Phase::Completed;
            task.enabled = false;
            task.goal_observed = false;
            task.message = "已确认目标课程选课成功".into();
        }
    } else if task.goal_observed && task.pending.is_none() {
        task.enabled = false;
        task.phase = Phase::Stopped;
        task.message = "官网已改变课程选择，任务已停止".into();
    } else if task.active()
        && task.phase == Phase::Monitoring
        && task
            .old_course
            .as_ref()
            .is_some_and(|old| !selected.iter().any(|c| c.id == old.id))
    {
        task.enabled = false;
        task.message = "旧课已发生变更，已暂停此换课任务；请核对官网结果".into();
    }
}
