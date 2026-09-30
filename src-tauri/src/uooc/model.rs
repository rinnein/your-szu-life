use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            concurrency: 3,
            report_interval_sec: 20,
            speed_multiplier: 2.0,
            dry_run: false,
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
        {
            return Err("并发数和间隔必须为正整数，间隔不超过一天，倍率必须为有效正数".into());
        }
        Ok(())
    }
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
