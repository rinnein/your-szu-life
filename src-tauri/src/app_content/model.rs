use reqwest::Url;
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

pub(super) const DAY_MS: u64 = 24 * 60 * 60 * 1000;
pub(super) const JSON_LIMIT: usize = 1024 * 1024;
pub(super) const IMAGE_LIMIT: usize = 4 * 1024 * 1024;
pub(super) const MAX_ANNOUNCEMENTS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentError {
    pub kind: ErrorKind,
    pub message: String,
    pub retry_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ErrorKind {
    Network,
    Http,
    InvalidData,
    TooLarge,
    RateLimited,
    Unavailable,
}

impl ContentError {
    pub(super) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_at: None,
        }
    }

    pub(super) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidData, message)
    }
}

/// Public content deliberately accepts HTTP, private addresses and intranet names.
/// This validator is NOT used by authentication or campus-network clients.
pub(super) fn web_url(value: &str) -> Result<Url, ContentError> {
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

pub(super) trait Document: Clone + Serialize + DeserializeOwned {
    fn validate(&self) -> Result<(), ContentError>;
}

pub(super) fn parse_document<T: Document>(bytes: &[u8]) -> Result<T, ContentError> {
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Announcement {
    pub title: String,
    pub pic: String,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Announcements {
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
                    "公告需要标题、头图，以及且仅有 desc / goto 中的一项",
                ));
            }
            web_url(&unit.pic)?;
            if let Some(goto) = &unit.goto {
                web_url(goto)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct UpdateInfo {
    pub latest_ver: String,
    pub latest_ver_url: String,
    pub changelog: String,
}

pub(super) fn version(value: &str) -> Result<Version, ContentError> {
    Version::parse(value.strip_prefix('v').unwrap_or(value))
        .map_err(|_| ContentError::invalid("版本号不是有效的 SemVer"))
}

impl Document for UpdateInfo {
    fn validate(&self) -> Result<(), ContentError> {
        version(&self.latest_ver)?;
        web_url(&self.latest_ver_url)?;
        if self.changelog.len() > 256 * 1024 {
            return Err(ContentError::invalid("更新日志超过 256 KiB 限制"));
        }
        Ok(())
    }
}

pub(super) fn is_newer(current: &str, latest: &str) -> bool {
    match (version(current), version(latest)) {
        (Ok(current), Ok(latest)) => latest.cmp_precedence(&current).is_gt(),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Source {
    Primary,
    Fallback,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedSnapshot<T> {
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
pub(crate) struct ConfigurationView {
    pub status: ConfigurationStatus,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ConfigurationStatus {
    Ready,
    Unconfigured,
    Invalid,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Repository {
    pub full_name: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppContentSnapshot {
    pub revision: u64,
    pub version: String,
    pub configuration: ConfigurationView,
    pub repository: Option<Repository>,
    pub repository_message: Option<String>,
    pub refreshing: bool,
    pub next_automatic_check_at: Option<u64>,
    pub announcements: FeedSnapshot<Announcements>,
    pub update: FeedSnapshot<UpdateInfo>,
    pub update_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Contributor {
    pub login: String,
    pub avatar_url: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContributorList {
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
pub(crate) struct ContributorsSnapshot {
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
pub(crate) enum ContributorStatus {
    Unconfigured,
    Ready,
    Error,
    RateLimited,
}
