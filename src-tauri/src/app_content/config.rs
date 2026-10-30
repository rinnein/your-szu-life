use super::model::*;
use reqwest::Url;
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Sources {
    pub primary: Url,
    pub fallback: Option<Url>,
}

impl Sources {
    pub fn key(&self) -> String {
        format!(
            "{}\n{}",
            self.primary,
            self.fallback.as_ref().map(Url::as_str).unwrap_or_default()
        )
    }
}

pub(super) const DEFAULT_CNB_REPOSITORY: &str = "rinnein/szu/your-szu-life";

pub(super) struct ContentConfig {
    pub version: String,
    pub sources: Option<Sources>,
    pub view: ConfigurationView,
    pub repository: Option<Repository>,
    pub repository_message: Option<String>,
    pub cnb_repository: String,
    pub github_accelerators: Vec<Url>,
}

fn directory_url(value: &str) -> Result<Url, ContentError> {
    let mut url = web_url(value.trim())?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ContentError::invalid(
            "内容源应为目录地址，不能包含查询参数或片段",
        ));
    }
    if !url.path().ends_with('/') {
        // Appending to the serialized URL preserves already percent-encoded path prefixes.
        url = web_url(&format!("{url}/"))?;
    }
    Ok(url)
}

impl ContentConfig {
    pub fn uses_github_releases(&self) -> bool {
        self.sources.is_none()
            && self.view.status == ConfigurationStatus::Unconfigured
            && self.repository.is_some()
    }

    pub fn compiled(version: String) -> Self {
        let package: Value = serde_json::from_str(include_str!("../../../package.json"))
            .expect("package.json must be valid JSON");
        Self::new(
            version,
            option_env!("SZU_LIFE_BASE_URL"),
            option_env!("SZU_LIFE_FALLBACK_BASE_URL"),
            package.get("repository"),
        )
    }

    pub fn new(
        version: String,
        primary: Option<&str>,
        fallback: Option<&str>,
        repository: Option<&Value>,
    ) -> Self {
        let primary = primary.map(str::trim).filter(|s| !s.is_empty());
        let fallback = fallback.map(str::trim).filter(|s| !s.is_empty());
        let (sources, view) = match primary {
            None => (
                None,
                ConfigurationView {
                    status: ConfigurationStatus::Unconfigured,
                    message: Some(
                        "本构建未配置独立内容源；公告与更新信息将从 GitHub Release 获取。".into(),
                    ),
                },
            ),
            Some(primary) => match directory_url(primary).and_then(|primary| {
                Ok(Sources {
                    primary,
                    fallback: fallback.map(directory_url).transpose()?,
                })
            }) {
                Ok(sources) => (
                    Some(sources),
                    ConfigurationView {
                        status: ConfigurationStatus::Ready,
                        message: None,
                    },
                ),
                Err(error) => (
                    None,
                    ConfigurationView {
                        status: ConfigurationStatus::Invalid,
                        message: Some(format!("构建时内容源配置无效：{}", error.message)),
                    },
                ),
            },
        };
        let (repository, repository_message) = match repository {
            None | Some(Value::Null) => (
                None,
                Some("package.json 未配置 repository，暂时无法展示贡献者。".into()),
            ),
            Some(value) => match parse_repository(value) {
                Some(repo) => (Some(repo), None),
                None => (
                    None,
                    Some("repository 不是支持的公开 GitHub 仓库地址。".into()),
                ),
            },
        };
        Self {
            version,
            sources,
            view,
            repository,
            repository_message,
            cnb_repository: option_env!("SZU_LIFE_CNB_REPO")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(DEFAULT_CNB_REPOSITORY)
                .to_string(),
            github_accelerators: serde_json::from_str::<Vec<String>>(include_str!(
                "../../../github-accelerator.json"
            ))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| directory_url(&value).ok())
            .collect(),
        }
    }
}

pub(super) fn parse_repository(value: &Value) -> Option<Repository> {
    let text = value
        .as_str()
        .or_else(|| value.get("url")?.as_str())?
        .trim();
    let text = text.strip_prefix("git+").unwrap_or(text);
    let path = if let Some(path) = text.strip_prefix("github:") {
        path.to_owned()
    } else if let Some(path) = text.strip_prefix("git@github.com:") {
        path.to_owned()
    } else if !text.contains(':') && !text.starts_with('/') {
        text.to_owned()
    } else {
        let url = Url::parse(text).ok()?;
        if url.host_str()? != "github.com"
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.password().is_some()
            || !matches!(url.scheme(), "http" | "https" | "git" | "ssh")
            || !(url.username().is_empty() || url.scheme() == "ssh" && url.username() == "git")
        {
            return None;
        }
        url.path().trim_start_matches('/').to_owned()
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty()
        || owner.len() > 39
        || repo.is_empty()
        || repo.len() > 100
        || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        || matches!(repo, "." | "..")
    {
        return None;
    }
    let full_name = format!("{owner}/{repo}");
    Some(Repository {
        url: format!("https://github.com/{full_name}"),
        full_name,
    })
}
