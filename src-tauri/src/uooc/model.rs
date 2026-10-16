use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn text(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Number(v) => v.to_string(),
        _ => String::new(),
    }
}
pub(super) fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}
pub(super) fn id(value: &Value) -> Option<String> {
    let id = text(value).parse::<u64>().ok()?;
    (id > 0).then(|| id.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Course {
    pub id: String,
    pub title: String,
    pub semester: String,
    pub organization: String,
    pub weeks: String,
    #[serde(default)]
    pub end_date: String,
}
pub(super) fn courses_page(value: &Value) -> Result<(Vec<Course>, u64), String> {
    let rows = value["data"].as_array().ok_or("课程列表格式无效")?;
    let pages = text(&value["page"]["pages"])
        .parse::<u64>()
        .map_err(|_| "课程分页格式无效")?;
    if pages > 1000 {
        return Err("课程分页超过支持范围".into());
    }
    let courses = rows
        .iter()
        .filter(|row| text(&row["status"]) == "31")
        .filter_map(|row| {
            Some(Course {
                id: id(&row["id"])?,
                title: text(&row["parent_name"]),
                semester: text(&row["name"]),
                organization: text(&row["org_name"]),
                weeks: text(&row["weeks"]),
                end_date: text(&row["end_time"]),
            })
        })
        .collect();
    Ok((courses, pages))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    pub concurrency: usize,
    pub report_interval_sec: u64,
    pub speed_multiplier: f64,
    pub dry_run: bool,
    #[serde(default = "default_discussion_enabled")]
    pub discussion_enabled: bool,
    #[serde(default = "default_discussion_content")]
    pub discussion_content: String,
    #[serde(default = "default_discussion_interval_min")]
    pub discussion_interval_min: u64,
    #[serde(default = "default_qa_server_url")]
    pub qa_server_url: String,
}
fn default_discussion_enabled() -> bool {
    true
}
fn default_discussion_content() -> String {
    "要好好学习".into()
}
fn default_discussion_interval_min() -> u64 {
    30
}
fn default_qa_server_url() -> String {
    "http://localhost:4321".into()
}
pub(crate) fn normalize_qa_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Ok("http://localhost:4321".into());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "题库服务地址无效")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("题库服务地址需使用 HTTP 或 HTTPS，不能包含凭据或路径错误".into());
    }
    Ok(url.as_str().trim_end_matches('/').into())
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            concurrency: 3,
            report_interval_sec: 20,
            speed_multiplier: 2.0,
            dry_run: false,
            discussion_enabled: true,
            discussion_content: default_discussion_content(),
            discussion_interval_min: default_discussion_interval_min(),
            qa_server_url: default_qa_server_url(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.concurrency == 0
            || self.concurrency > tokio::sync::Semaphore::MAX_PERMITS
            || self.report_interval_sec == 0
            || self.report_interval_sec > 86400
            || !self.speed_multiplier.is_finite()
            || self.speed_multiplier <= 0.0
            || !(self.report_interval_sec as f64 * self.speed_multiplier).is_finite()
            || (self.discussion_enabled && self.discussion_content.trim().is_empty())
            || !(1..=1440).contains(&self.discussion_interval_min)
        {
            return Err(
                "并发数和间隔必须为正整数，间隔不超过一天，倍率必须为有效正数，讨论文案不能为空"
                    .into(),
            );
        }
        if !self.qa_server_url.trim().is_empty() {
            normalize_qa_url(&self.qa_server_url)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DiscussionStatus {
    #[default]
    Idle,
    WaitingCooldown,
    Eligible,
    Sending,
    Completed,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscussionCourseState {
    pub status: DiscussionStatus,
    pub message: String,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub last_tid: Option<String>,
    #[serde(default)]
    pub last_sent_at_ms: Option<i64>,
    #[serde(default)]
    pub processed_tids: Vec<String>,
    #[serde(default)]
    #[serde(alias = "lastPercent")]
    pub last_completion_ratio: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscussionStoreState {
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub last_sent_at_ms: Option<i64>,
    #[serde(default)]
    pub courses: BTreeMap<String, DiscussionCourseState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct User {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SessionStatus {
    #[default]
    Checking,
    Ready,
    LoginRequired,
    NetworkError,
    LoggingIn,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TaskStatus {
    Queued,
    Running,
    Paused,
    Stopped,
    AwaitingLogin,
    SystemPaused,
    Completed,
    DryRun,
    Blocked,
    Failed,
    AwaitingDecision,
}
impl TaskStatus {
    pub fn active(&self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoState {
    pub resource_id: String,
    pub title: String,
    pub status: TaskStatus,
    pub position: f64,
    pub duration: Option<f64>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CourseTask {
    pub course: Course,
    pub account_id: String,
    pub desired_running: bool,
    pub status: TaskStatus,
    pub message: String,
    pub videos: Vec<VideoState>,
    #[serde(default)]
    pub generation: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SavedState {
    pub settings: Settings,
    pub tasks: Vec<CourseTask>,
    #[serde(default)]
    pub quizzes: std::collections::BTreeMap<String, super::quiz::model::QuizState>,
    #[serde(default)]
    pub discussion: DiscussionStoreState,
    #[serde(default)]
    pub qa_api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub revision: u64,
    pub session: SessionStatus,
    pub user: Option<User>,
    pub courses: Vec<Course>,
    pub settings: Settings,
    pub tasks: Vec<CourseTask>,
    pub message: String,
    pub logs: Vec<String>,
    #[serde(default)]
    pub progress: std::collections::HashMap<String, super::progress::ProgressEntry>,
    #[serde(default)]
    pub checkins: std::collections::HashMap<String, super::checkin::CheckinState>,
    #[serde(default)]
    pub quizzes: std::collections::BTreeMap<String, super::quiz::model::QuizState>,
    #[serde(default)]
    pub discussions: BTreeMap<String, DiscussionCourseState>,
    #[serde(default)]
    pub discussion_cooldown_until_ms: Option<i64>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            session: SessionStatus::Checking,
            user: None,
            courses: vec![],
            settings: Settings::default(),
            tasks: vec![],
            message: String::new(),
            logs: vec![],
            progress: Default::default(),
            checkins: Default::default(),
            quizzes: Default::default(),
            discussions: Default::default(),
            discussion_cooldown_until_ms: None,
        }
    }
}

impl Snapshot {
    pub(super) fn update_courses(&mut self, user: User, courses: Vec<Course>) {
        for task in self
            .tasks
            .iter_mut()
            .filter(|task| task.account_id == user.id)
        {
            if let Some(course) = courses.iter().find(|course| course.id == task.course.id) {
                task.course.clone_from(course);
            }
        }
        self.user = Some(user);
        self.courses = courses;
    }
}

#[derive(Clone)]
pub(super) struct Video {
    pub cid: String,
    pub chapter: String,
    pub section: String,
    pub subsection: Option<String>,
    pub resource: String,
    pub title: String,
    pub source: String,
    pub position: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TaskAction {
    Pause,
    Resume,
    Stop,
}

pub(super) fn multiplier(request: usize, target: f64) -> f64 {
    match request {
        1 => 1.0_f64.min(target),
        2 => 1.5_f64.min(target),
        3 => 2.0_f64.min(target),
        _ => target,
    }
}
