use reqwest::Url;
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::cmp::Ordering;

pub const DAY_MS: u64 = 24 * 60 * 60 * 1000;
pub const JSON_LIMIT: usize = 1024 * 1024;
/// Final image size handed to the WebView (and stored in the cache).
pub const IMAGE_LIMIT: usize = 4 * 1024 * 1024;
/// 第三方图片（题库、更新日志等）的最终大小上限；不重新编码，保留原始清晰度。
pub const PUBLIC_IMAGE_LIMIT: usize = 8 * 1024 * 1024;
/// Raw image responses may be much larger: they are decoded, downscaled and re-encoded
/// before they reach the WebView, so the download cap is independent of `IMAGE_LIMIT`.
pub const IMAGE_DOWNLOAD_LIMIT: usize = 16 * 1024 * 1024;
pub const MAX_ANNOUNCEMENTS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContentError {
    pub kind: ErrorKind,
    pub message: String,
    pub retry_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Network,
    Http,
    InvalidData,
    TooLarge,
    RateLimited,
    Unavailable,
}

impl ContentError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_at: None,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidData, message)
    }
}

/// Public content deliberately accepts HTTP, private addresses and intranet names.
/// This validator is NOT used by authentication or campus-network clients.
pub fn web_url(value: &str) -> Result<Url, ContentError> {
    if value.len() > 4096 || value.chars().any(char::is_control) || value.trim() != value {
        return Err(ContentError::invalid("地址格式无效"));
    }
    let url = Url::parse(value).map_err(|_| ContentError::invalid("地址格式无效"))?;
    let authority = value
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || authority.is_some_and(|part| part.contains('@'))
    {
        return Err(ContentError::invalid(
            "仅支持不含用户名和密码的 HTTP / HTTPS 地址",
        ));
    }
    Ok(url)
}

pub trait Document: Clone + Serialize + DeserializeOwned {
    fn validate(&self) -> Result<(), ContentError>;
}

pub fn parse_document<T: Document>(bytes: &[u8]) -> Result<T, ContentError> {
    if bytes.len() > JSON_LIMIT {
        return Err(ContentError::new(
            ErrorKind::TooLarge,
            "JSON 响应超过 1 MiB 限制",
        ));
    }
    let data: T = serde_json::from_slice(bytes)
        .map_err(|_| ContentError::invalid("JSON 格式或字段类型无效"))?;
    data.validate()?;
    Ok(data)
}

// A missing field is allowed here; an explicitly null field is not a string.
fn present_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnouncementMode {
    #[default]
    Silent,
    Popup,
    Alert,
}

impl AnnouncementMode {
    fn is_silent(mode: &Self) -> bool {
        *mode == Self::Silent
    }
}

/// 未知或 null 的 mode 只降级该条公告为 silent，不整体拒绝文档。
///
/// 不能简单地对 `AnnouncementMode::deserialize(...).unwrap_or(Silent)` 兜底：
/// 自描述格式里反序列化失败时值可能只被消费了一半（例如 null 在
/// `deserialize_enum` 的 peek 阶段就报错），后续 JSON 流会错位。这里用
/// `deserialize_any` 的 visitor 恰好消费一次值且永不报错。
fn lenient_mode<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AnnouncementMode, D::Error> {
    struct LenientModeVisitor;

    impl<'de> serde::de::Visitor<'de> for LenientModeVisitor {
        type Value = AnnouncementMode;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("公告模式（silent / popup / alert）")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<AnnouncementMode, E> {
            Ok(match value {
                "popup" => AnnouncementMode::Popup,
                "alert" => AnnouncementMode::Alert,
                _ => AnnouncementMode::Silent,
            })
        }

        fn visit_unit<E>(self) -> Result<AnnouncementMode, E> {
            Ok(AnnouncementMode::Silent)
        }

        fn visit_none<E>(self) -> Result<AnnouncementMode, E> {
            Ok(AnnouncementMode::Silent)
        }
    }

    deserializer.deserialize_any(LenientModeVisitor)
}

/// popup 公告的弹出去重依赖稳定 id；缺失、空白或携带控制字符的 id 无法承担该职责。
fn is_usable_announcement_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Announcement {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient_mode",
        skip_serializing_if = "AnnouncementMode::is_silent"
    )]
    pub mode: AnnouncementMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<AnnouncementConditions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pic: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub desc: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub goto: Option<String>,
}

/// 版本限定条件。`conditions` / `conditions.ver` 缺省 = 全局公告，对所有版本生效。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnouncementConditions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VersionOp {
    Exact,
    AtLeast,
    AtMost,
    Greater,
    Less,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VersionComparator {
    op: VersionOp,
    target: Version,
}

impl VersionComparator {
    /// 每段条件为「可选比较符 + SemVer」，比较符仅支持 >= <= > <，省略表示精确匹配。
    fn parse(part: &str) -> Option<Self> {
        let part = part.trim();
        let (op, rest) = if let Some(rest) = part.strip_prefix(">=") {
            (VersionOp::AtLeast, rest)
        } else if let Some(rest) = part.strip_prefix("<=") {
            (VersionOp::AtMost, rest)
        } else if let Some(rest) = part.strip_prefix('>') {
            (VersionOp::Greater, rest)
        } else if let Some(rest) = part.strip_prefix('<') {
            (VersionOp::Less, rest)
        } else {
            (VersionOp::Exact, part)
        };
        Some(Self {
            op,
            target: version(rest.trim()).ok()?,
        })
    }

    fn matches(&self, current: &Version) -> bool {
        let ordering = self.target.cmp_precedence(current);
        match self.op {
            VersionOp::Exact => ordering == Ordering::Equal,
            VersionOp::AtLeast => ordering != Ordering::Greater,
            VersionOp::AtMost => ordering != Ordering::Less,
            VersionOp::Greater => ordering == Ordering::Less,
            VersionOp::Less => ordering == Ordering::Greater,
        }
    }
}

/// 逗号分隔的条件组，全部满足（AND）才算命中。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionCondition {
    comparators: Vec<VersionComparator>,
}

impl VersionCondition {
    pub fn parse(value: &str) -> Option<Self> {
        let mut comparators = Vec::new();
        for part in value.split(',') {
            comparators.push(VersionComparator::parse(part)?);
        }
        (!comparators.is_empty()).then_some(Self { comparators })
    }

    pub fn matches(&self, current: &Version) -> bool {
        self.comparators.iter().all(|c| c.matches(current))
    }
}

/// 暴露给前端前做规范化与版本过滤；磁盘缓存始终保持发布端原始数据。
///
/// 新字段的语义问题只降级或跳过单条，绝不整体拒绝文档：
/// - 无 `conditions` / `ver`：全局公告，直接保留；
/// - `ver` 解析失败：丢弃该条（定向公告错误地展示给全部版本比不展示更危险）；
/// - popup 缺失或携带不可用 id：降级为 silent，照常展示；
/// - 当前版本号解析失败：全部保留（宁可多显示）。
pub fn applicable_announcements(data: Announcements, current: &str) -> Announcements {
    let Ok(current) = version(current) else {
        return data;
    };
    let announcements = data
        .announcements
        .into_iter()
        .filter_map(|mut unit| {
            if let Some(ver) = unit.conditions.as_ref().and_then(|c| c.ver.as_deref()) {
                if !VersionCondition::parse(ver).is_some_and(|c| c.matches(&current)) {
                    return None;
                }
            }
            if unit.mode == AnnouncementMode::Popup
                && !unit.id.as_deref().is_some_and(is_usable_announcement_id)
            {
                unit.mode = AnnouncementMode::Silent;
            }
            Some(unit)
        })
        .collect();
    Announcements { announcements }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Announcements {
    pub announcements: Vec<Announcement>,
}

impl Document for Announcements {
    fn validate(&self) -> Result<(), ContentError> {
        if self.announcements.len() > MAX_ANNOUNCEMENTS {
            return Err(ContentError::invalid("公告数量超过 100 条限制"));
        }
        for unit in &self.announcements {
            if unit.title.trim().is_empty()
                || unit.title.len() > 1024
                || unit.desc.is_some() == unit.goto.is_some()
                || unit
                    .desc
                    .as_ref()
                    .is_some_and(|desc| desc.len() > 64 * 1024)
            {
                return Err(ContentError::invalid(
                    "公告需要标题，以及且仅有 desc / goto 中的一项",
                ));
            }
            if let Some(pic) = unit.pic.as_deref().filter(|pic| !pic.trim().is_empty()) {
                web_url(pic)?;
            }
            if let Some(goto) = &unit.goto {
                web_url(goto)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub latest_ver: String,
    pub latest_ver_url: String,
    pub changelog: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
}

pub fn version(value: &str) -> Result<Version, ContentError> {
    Version::parse(value.strip_prefix('v').unwrap_or(value))
        .map_err(|_| ContentError::invalid("版本号不是有效的 SemVer"))
}

impl Document for UpdateInfo {
    fn validate(&self) -> Result<(), ContentError> {
        version(&self.latest_ver)?;
        web_url(&self.latest_ver_url)?;
        if let Some(release_url) = &self.release_url {
            web_url(release_url)?;
        }
        if self.changelog.len() > 256 * 1024 {
            return Err(ContentError::invalid("更新日志超过 256 KiB 限制"));
        }
        Ok(())
    }
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    match (version(current), version(latest)) {
        (Ok(current), Ok(latest)) => latest.cmp_precedence(&current).is_gt(),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Primary,
    Fallback,
    Github,
    Cnb,
}

/// 应用内更新频道。
///
/// - `Stable`：只接收正式 release（GitHub 的 latest / CNB 的非预发布版本）；
///   还没有正式版时回退到 canary 内容，并由快照上的 `update_channel_fallback`
///   告知前端「当前显示的是测试版」。
/// - `Canary`：跟随滚动 `canary` release。
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Canary,
}

impl UpdateChannel {
    /// 写进缓存 scope 与 URL 的稳定标识。
    pub fn slug(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Canary => "canary",
        }
    }

    pub fn is_stable(self) -> bool {
        self == Self::Stable
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CnbAsset {
    pub name: String,
    #[serde(default)]
    pub browser_download_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CnbRelease {
    #[serde(default)]
    pub tag_ref: String,
    pub title: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<CnbAsset>,
}

impl CnbRelease {
    pub fn tag_name(&self) -> &str {
        self.tag_ref
            .strip_prefix("refs/tags/")
            .unwrap_or(&self.tag_ref)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CnbTagReleaseResponse {
    pub release: Option<CnbRelease>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CnbReleasesResponse {
    #[serde(default)]
    pub releases: Vec<CnbRelease>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSnapshot<T> {
    pub data: Option<T>,
    pub last_attempt_at: Option<u64>,
    pub last_success_at: Option<u64>,
    pub source: Option<Source>,
    pub from_cache: bool,
    pub error: Option<ContentError>,
}

impl<T> Default for FeedSnapshot<T> {
    fn default() -> Self {
        Self {
            data: None,
            last_attempt_at: None,
            last_success_at: None,
            source: None,
            from_cache: false,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationView {
    pub status: ConfigurationStatus,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConfigurationStatus {
    Ready,
    Unconfigured,
    Invalid,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub full_name: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContentSnapshot {
    pub revision: u64,
    pub version: String,
    pub configuration: ConfigurationView,
    pub repository: Option<Repository>,
    pub repository_message: Option<String>,
    pub refreshing: bool,
    pub next_automatic_check_at: Option<u64>,
    /// 当前选中的更新频道。
    pub update_channel: UpdateChannel,
    /// 稳定版频道因暂无正式 release 而回退显示测试版内容。
    pub update_channel_fallback: bool,
    pub announcements: FeedSnapshot<Announcements>,
    pub update: FeedSnapshot<UpdateInfo>,
    pub update_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Contributor {
    pub login: String,
    pub avatar_url: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub status: String,
    pub progress: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: Option<u64>,
    pub port: Option<u16>,
    pub num_pieces: Option<u32>,
    pub piece_length: Option<u64>,
    pub bitfield: Option<String>,
    pub connections: Option<u32>,
    pub is_resumed: Option<bool>,
    pub resumed_bytes: Option<u64>,
    pub total_active_tasks: Option<u32>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributorList {
    pub contributors: Vec<Contributor>,
    pub truncated: bool,
}

impl Document for ContributorList {
    fn validate(&self) -> Result<(), ContentError> {
        if self.contributors.len() > 1000 {
            return Err(ContentError::invalid("贡献者数量超限"));
        }
        for contributor in &self.contributors {
            if contributor.login.is_empty()
                || contributor.login.len() > 100
                || !contributor
                    .login
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_[]".contains(c))
            {
                return Err(ContentError::invalid("贡献者用户名无效"));
            }
            web_url(&contributor.avatar_url)?;
            let profile = web_url(&contributor.url)?;
            if profile.scheme() != "https" || profile.host_str() != Some("github.com") {
                return Err(ContentError::invalid("贡献者主页地址无效"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributorsSnapshot {
    pub status: ContributorStatus,
    pub repository: Option<Repository>,
    pub contributors: Vec<Contributor>,
    pub truncated: bool,
    pub from_cache: bool,
    pub last_success_at: Option<u64>,
    pub error: Option<ContentError>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContributorStatus {
    Unconfigured,
    Ready,
    Error,
    RateLimited,
}
