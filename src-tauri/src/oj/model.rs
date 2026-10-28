use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Site {
    pub id: String,
    pub name: String,
    pub url: String,
    pub builtin: bool,
    #[serde(default = "yes")]
    pub reminders: bool,
}
fn yes() -> bool {
    true
}
pub(super) fn defaults() -> Vec<Site> {
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
    })
    .collect()
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Secret {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub cookies: String,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Session {
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
pub(crate) struct SiteView {
    #[serde(flatten)]
    pub site: Site,
    pub enabled: bool,
    pub username: String,
    pub session_generation: Option<u64>,
    pub session: Session,
    pub message: String,
    pub last_sync: Option<i64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiteInput {
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
pub(crate) struct Captcha {
    pub token: String,
    pub image: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Group {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Contest {
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
pub(crate) struct Problem {
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
pub(crate) struct Sample {
    pub input: String,
    pub output: String,
}
#[derive(Clone, Default, PartialEq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CompileOptions {
    #[serde(default)]
    pub version: String,
    #[serde(default, alias = "enable_asan")]
    pub enable_asan: bool,
    #[serde(default, alias = "enable_lsan")]
    pub enable_lsan: bool,
}
impl CompileOptions {
    pub(super) fn for_language(
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
    pub(super) fn wire(&self) -> Value {
        serde_json::json!({"version": self.version, "enable_asan": self.enable_asan, "enable_lsan": self.enable_lsan})
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preferences {
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
    pub(super) fn options(&self) -> Result<Option<CompileOptions>, String> {
        CompileOptions::for_language(&self.language, self.options_by_language.get(&self.language))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreferencesInput {
    pub site_id: String,
    pub account: String,
    pub epoch: u64,
    pub expected_revision: u64,
    pub language: String,
    pub options: Option<CompileOptions>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompanionStatus {
    pub connected: bool,
    pub active: bool,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CompanionSubmission {
    pub url: String,
    pub source_code: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompanionResult {
    pub site_id: String,
    pub contest_id: String,
    pub problem_key: String,
    pub account: String,
    pub epoch: u64,
    pub submission: Value,
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Draft {
    pub code: String,
    pub language: String,
    pub answer: Value,
    #[serde(default)]
    pub options: Option<CompileOptions>,
    pub revision: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Paper {
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Notice {
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
pub(crate) struct Known {
    pub contests: BTreeMap<String, Contest>,
    pub events: BTreeMap<String, i64>,
    pub initialized: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupCache {
    pub groups: Vec<Group>,
    pub updated_at: i64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContestCache {
    pub contests: Vec<Contest>,
    pub updated_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Data {
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
pub(crate) struct Snapshot {
    pub sites: Vec<SiteView>,
    pub notices: Vec<Notice>,
    pub companion_port: u16,
    pub desktop: bool,
    pub companion_enabled: bool,
    pub companion_status: CompanionStatus,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DraftInput {
    pub site_id: String,
    pub contest_id: String,
    pub problem_key: String,
    pub account: String,
    pub epoch: u64,
    pub draft: Draft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmitInput {
    pub site_id: String,
    pub contest_id: String,
    pub problem_key: Option<String>,
    pub account: String,
    pub epoch: u64,
}
pub(super) fn scope(site: &str, account: &str) -> String {
    serde_json::to_string(&(site, account)).unwrap()
}
pub(super) fn draft_key(
    site: &str,
    account: &str,
    contest: &str,
    problem: &str,
    language: &str,
) -> String {
    serde_json::to_string(&(site, account, contest, problem, language)).unwrap()
}
pub(super) fn operation_key(
    site: &str,
    account: &str,
    contest: &str,
    problem: Option<&str>,
) -> String {
    serde_json::to_string(&(site, account, contest, problem)).unwrap()
}
pub(super) fn problem_key(p: &Problem) -> String {
    format!("{}:{}", p.kind, p.id)
}
