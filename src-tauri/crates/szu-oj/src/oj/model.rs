use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub id: String,
    pub name: String,
    pub url: String,
    pub builtin: bool,
    #[serde(default = "yes")]
    pub reminders: bool,
    /// 上次登录确认的平台角色：0 管理员 / 1 教师 / 2 助教 / 3 学生。
    /// 只用于站点卡片标记与助教入口判断，后端每次 manage 调用仍会重新校验。
    #[serde(default)]
    pub permission: Option<u8>,
}
fn yes() -> bool {
    true
}
pub fn defaults() -> Vec<Site> {
    [
        (53, "程设C(高研院)"),
        (55, "程序设计基础实验(C)"),
        (56, "数据结构(AI学院)(备用OJ，仅供老师登录)"),
        (61, "程序设计基础(AI OJ)"),
        (62, "数据结构(AI OJ)"),
        (64, "Python程序设计"),
        (67, "数据结构"),
        (68, "程序设计综合实训(C++)"),
        (71, "程序设计基础实验(C)(备用OJ，已关闭学生登录)"),
    ]
    .into_iter()
    .map(|(ip, name)| Site {
        id: format!("szu-{ip}"),
        name: name.into(),
        url: format!("http://172.31.221.{ip}/"),
        builtin: true,
        reminders: true,
        permission: None,
    })
    .collect()
}
pub use szu_core::storage::model::OjSecret as Secret;
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Session {
    #[default]
    Disabled,
    Connecting,
    Ready,
    CaptchaRequired,
    LoginFailed,
    NetworkError,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteView {
    #[serde(flatten)]
    pub site: Site,
    pub enabled: bool,
    pub username: String,
    pub session_generation: Option<u64>,
    pub session: Session,
    pub message: String,
    pub last_sync: Option<i64>,
    /// 本地是否已有该站点的班级/测验缓存（会话尚未建立时也可展示）。
    pub has_cache: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteInput {
    pub id: Option<String>,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: Option<String>,
    pub clear_credentials: bool,
    pub reminders: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Captcha {
    pub token: String,
    pub image: String,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contest {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub description: String,
    pub begin: i64,
    pub end: i64,
    pub now: i64,
    pub is_exam: bool,
}
impl Contest {
    pub fn phase(&self, now: i64) -> u8 {
        if now < self.begin {
            0
        } else if now < self.end {
            1
        } else {
            2
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub id: String,
    pub display_label: String,
    pub kind: u8,
    pub title: String,
    pub description: String,
    pub input: String,
    pub output: String,
    pub hint: String,
    pub time_limit: f64,
    pub memory_limit: u64,
    pub samples: Vec<Sample>,
    pub choices: Vec<String>,
    pub multiple: bool,
    pub blanks: usize,
    pub languages: Vec<String>,
    pub templates: BTreeMap<String, String>,
    pub score: Option<f64>,
    pub io_mode: String,
    pub input_file: Option<String>,
    pub output_file: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Sample {
    pub input: String,
    pub output: String,
}
#[derive(Clone, Default, PartialEq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileOptions {
    #[serde(default)]
    pub version: String,
    #[serde(default, alias = "enable_asan")]
    pub enable_asan: bool,
    #[serde(default, alias = "enable_lsan")]
    pub enable_lsan: bool,
}
impl CompileOptions {
    pub fn for_language(
        language: &str,
        saved: Option<&Self>,
    ) -> Result<Option<Self>, String> {
        let (versions, default): (&[&str], &str) = match language {
            "c" => (&["c99", "c11", "c17"], "c17"),
            "cpp" => (
                &["c++98", "c++03", "c++11", "c++14", "c++17", "c++20"],
                "c++20",
            ),
            _ => return Ok(None),
        };
        let mut value = saved.cloned().unwrap_or_default();
        if value.version.is_empty() {
            value.version = default.into();
        }
        if !versions.contains(&value.version.as_str()) {
            return Err("当前语言不支持该编译标准".into());
        }
        if value.enable_lsan && !value.enable_asan {
            return Err("开启 LSan 前必须先开启 ASan".into());
        }
        Ok(Some(value))
    }
    pub fn wire(&self) -> Value {
        serde_json::json!({"version": self.version, "enable_asan": self.enable_asan, "enable_lsan": self.enable_lsan})
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub language: String,
    #[serde(default)]
    pub options_by_language: BTreeMap<String, CompileOptions>,
    #[serde(default)]
    pub revision: u64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: "cpp".into(),
            options_by_language: BTreeMap::new(),
            revision: 0,
        }
    }
}
impl Preferences {
    pub fn options(&self) -> Result<Option<CompileOptions>, String> {
        CompileOptions::for_language(&self.language, self.options_by_language.get(&self.language))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesInput {
    pub site_id: String,
    pub account: String,
    pub epoch: u64,
    pub expected_revision: u64,
    pub language: String,
    pub options: Option<CompileOptions>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionStatus {
    pub connected: bool,
    pub active: bool,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanionSubmission {
    pub url: String,
    pub source_code: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionResult {
    pub site_id: String,
    pub contest_id: String,
    pub problem_key: String,
    pub account: String,
    pub epoch: u64,
    pub submission: Value,
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub code: String,
    pub language: String,
    pub answer: Value,
    #[serde(default)]
    pub options: Option<CompileOptions>,
    pub revision: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paper {
    pub contest: Contest,
    pub problems: Vec<Problem>,
    pub drafts: BTreeMap<String, Draft>,
    pub account: String,
    pub epoch: u64,
    pub writable: bool,
    pub submitted: Option<Value>,
    pub source_url: String,
    pub preferences: Preferences,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewUser {
    pub id: String,
    pub username: String,
    pub name: String,
    #[serde(default)]
    pub nickname: String,
}

/// 助教工作台的管理测验列表项。字段取自 `GET /api/manage/contests/`。
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedContest {
    pub id: String,
    pub group_id: String,
    pub group_name: String,
    pub title: String,
    pub description: String,
    pub begin: i64,
    pub end: i64,
    pub is_exam: bool,
    /// `waiting` / `running` / `done` / 其它错误状态；空串表示后端未提供。
    pub scored_status: String,
    pub score_start_time: Option<i64>,
    pub total_score: Option<f64>,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ManagedContests {
    pub rows: Vec<ManagedContest>,
    /// 服务端当前时间（毫秒），用于在不依赖设备时钟的情况下判断阶段。
    pub now: i64,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ManagedContestDetail {
    #[serde(flatten)]
    pub contest: ManagedContest,
    pub problems: Vec<Problem>,
    /// 服务端当前时间（毫秒）。
    pub now: i64,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    pub id: String,
    pub user: ReviewUser,
    pub total_score: f64,
    pub judged: bool,
    pub checked: bool,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReviewList {
    pub count: u64,
    pub rows: Vec<ReviewSummary>,
}

/// 批阅页单题视图。编程/补全题带代码与判题结果；选择/填空带学生答案；
/// 问答题带文本答案与教师评语。
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProblem {
    pub id: String,
    pub display_label: String,
    pub kind: u8,
    pub title: String,
    pub description: String,
    pub hint: String,
    pub max_score: f64,
    pub score: f64,
    pub submission_id: Option<String>,
    #[serde(default)]
    pub answer: Value,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub blanks: usize,
    pub code: Option<String>,
    pub language: Option<String>,
    pub status: Option<i32>,
    pub comment: String,
    /// 参考答案：编程为代码字符串，选择/填空为选项下标数组，问答为文本。
    pub reference_answer: Option<Value>,
    /// 编程/补全提交的原始判题结果，用于展示测试点与隐藏输出。
    pub judge: Option<Value>,
    pub load_error: Option<String>,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDetail {
    pub contest: ManagedContest,
    pub submission_id: String,
    pub user: ReviewUser,
    pub total_score: f64,
    pub judged: bool,
    pub checked: bool,
    pub problems: Vec<ReviewProblem>,
    /// 参考答案等非关键数据读取失败的提示，不阻塞批阅。
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSubmissionRow {
    pub id: String,
    pub user: ReviewUser,
    pub language: String,
    pub status: Option<i32>,
    pub create_time: Value,
    pub cpu_time: Option<f64>,
    pub real_time: Option<f64>,
    pub memory: Option<f64>,
    pub can_view_detail: bool,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSubmissions {
    pub count: u64,
    pub rows: Vec<ProblemSubmissionRow>,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaSubmission {
    pub submission: Value,
    /// `coding_problem` / `code_fill_problem` 返回的 `test_output`。
    pub expected: Vec<String>,
    /// 同一接口返回的 `sample_output`。
    pub samples: Vec<String>,
    pub warnings: Vec<String>,
}

/// 批阅保存的分数结构。wire 后的字段名与平台前端一致。
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReviewScores {
    #[serde(default)]
    pub coding: BTreeMap<String, BTreeMap<String, f64>>,
    #[serde(default)]
    pub code_fill: BTreeMap<String, BTreeMap<String, f64>>,
    #[serde(default)]
    pub qa: BTreeMap<String, BTreeMap<String, f64>>,
    #[serde(default)]
    pub choice: BTreeMap<String, f64>,
    #[serde(default)]
    pub fill_in: BTreeMap<String, f64>,
}
impl ReviewScores {
    pub fn wire(&self) -> Value {
        json!({
            "coding_score": self.coding,
            "code_fill_score": self.code_fill,
            "qa_score": self.qa,
            "choice_score": self.choice,
            "fill_in_score": self.fill_in,
        })
    }
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SaveReviewInput {
    pub site_id: String,
    pub contest_id: String,
    pub submission_id: String,
    pub scores: ReviewScores,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentInput {
    pub site_id: String,
    pub submission_id: String,
    pub kind: u8,
    pub comment: String,
}

/// 写操作结果。`uncertain` 表示请求可能已在服务端生效，前端必须刷新确认而不是重发。
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub saved: bool,
    pub uncertain: bool,
    pub message: String,
}

/// 批阅保存结果：成功后带回刷新后的答卷，刷新失败时仍保留 `saved=true`。
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SaveReviewResult {
    pub saved: bool,
    pub uncertain: bool,
    pub message: String,
    pub detail: Option<ReviewDetail>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub id: String,
    pub site_id: String,
    pub account: String,
    pub contest_id: String,
    pub title: String,
    pub message: String,
    pub timestamp: i64,
    #[serde(default)]
    pub read: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Known {
    pub contests: BTreeMap<String, Contest>,
    pub events: BTreeMap<String, i64>,
    pub initialized: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCache {
    pub groups: Vec<Group>,
    pub updated_at: i64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestCache {
    pub contests: Vec<Contest>,
    pub updated_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub version: u8,
    pub sites: Vec<Site>,
    pub drafts: BTreeMap<String, Draft>,
    pub known: BTreeMap<String, Known>,
    pub notices: Vec<Notice>,
    #[serde(default)]
    pub uncertain: BTreeMap<String, Value>,
    #[serde(default)]
    pub scheduled: BTreeMap<String, i32>,
    pub companion_port: u16,
    #[serde(default)]
    pub preferences: BTreeMap<String, Preferences>,
    #[serde(default = "yes")]
    pub companion_enabled: bool,
    /// 按站点缓存的班级列表，避免每次打开页面都重新请求。
    #[serde(default)]
    pub groups: BTreeMap<String, GroupCache>,
    /// 按 `site:group` 缓存的测验列表。
    #[serde(default)]
    pub contests: BTreeMap<String, ContestCache>,
}
impl Default for Data {
    fn default() -> Self {
        Self {
            version: 1,
            sites: defaults(),
            drafts: BTreeMap::new(),
            known: BTreeMap::new(),
            notices: vec![],
            uncertain: BTreeMap::new(),
            scheduled: BTreeMap::new(),
            companion_port: 27121,
            preferences: BTreeMap::new(),
            companion_enabled: true,
            groups: BTreeMap::new(),
            contests: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub sites: Vec<SiteView>,
    pub notices: Vec<Notice>,
    pub companion_port: u16,
    pub desktop: bool,
    pub companion_enabled: bool,
    pub companion_status: CompanionStatus,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftInput {
    pub site_id: String,
    pub contest_id: String,
    pub problem_key: String,
    pub account: String,
    pub epoch: u64,
    pub draft: Draft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitInput {
    pub site_id: String,
    pub contest_id: String,
    pub problem_key: Option<String>,
    pub account: String,
    pub epoch: u64,
}
pub fn scope(site: &str, account: &str) -> String {
    serde_json::to_string(&(site, account)).unwrap()
}
pub fn draft_key(
    site: &str,
    account: &str,
    contest: &str,
    problem: &str,
    language: &str,
) -> String {
    serde_json::to_string(&(site, account, contest, problem, language)).unwrap()
}
pub fn operation_key(
    site: &str,
    account: &str,
    contest: &str,
    problem: Option<&str>,
) -> String {
    serde_json::to_string(&(site, account, contest, problem)).unwrap()
}
pub fn problem_key(p: &Problem) -> String {
    format!("{}:{}", p.kind, p.id)
}
