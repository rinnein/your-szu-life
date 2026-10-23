use super::{
    cache::Cache,
    config::{ContentConfig, Sources},
    http::{PublicHttp, RequestKind},
    images::image_data_url,
    model::*,
    runtime::{AutomaticSchedule, Clock, ImageSlots, SingleFlight},
};
use reqwest::Url;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

type ImageFlight = Arc<SingleFlight<Result<String, ContentError>>>;

pub(super) struct ContentService {
    pub config: ContentConfig,
    http: Arc<dyn PublicHttp>,
    clock: Arc<dyn Clock>,
    cache: Cache,
    state: Mutex<AppContentSnapshot>,
    schedule: Mutex<AutomaticSchedule>,
    refresh_flight: SingleFlight<AppContentSnapshot>,
    contributors_flight: SingleFlight<ContributorsSnapshot>,
    contributors: Mutex<Option<ContributorsSnapshot>>,
    image_flights: Mutex<HashMap<String, ImageFlight>>,
    image_slots: ImageSlots,
    emit: Box<dyn Fn(AppContentSnapshot) + Send + Sync>,
}

impl ContentService {
    pub fn new(
        config: ContentConfig,
        http: Arc<dyn PublicHttp>,
        clock: Arc<dyn Clock>,
        cache_dir: Option<PathBuf>,
        emit: impl Fn(AppContentSnapshot) + Send + Sync + 'static,
    ) -> Self {
        let now = clock.now_ms();
        let cache = Cache::new(cache_dir);
        let mut snapshot = AppContentSnapshot {
            revision: 0,
            version: config.version.clone(),
            configuration: config.view.clone(),
            repository: config.repository.clone(),
            repository_message: config.repository_message.clone(),
            refreshing: false,
            next_automatic_check_at: (config.sources.is_some() || config.uses_github_releases())
                .then_some(now),
            announcements: FeedSnapshot::default(),
            update: FeedSnapshot::default(),
            update_available: false,
        };
        if let Some(sources) = &config.sources {
            snapshot.announcements = load_feed(&cache, &sources.key(), "announcements.json", now);
            snapshot.update = load_feed(&cache, &sources.key(), "update.json", now);
            snapshot.update_available = snapshot
                .update
                .data
                .as_ref()
                .is_some_and(|update| is_newer(&config.version, &update.latest_ver));
        }
        if config.uses_github_releases() {
            if let Some(repository) = &config.repository {
                let key = format!("github:{}", repository.full_name);
                snapshot.update = load_feed(&cache, &key, "latest-release.json", now);
                snapshot.update_available = snapshot
                    .update
                    .data
                    .as_ref()
                    .is_some_and(|update| is_newer(&config.version, &update.latest_ver));
            }
        }
        Self {
            config,
            http,
            clock,
            cache,
            state: Mutex::new(snapshot),
            schedule: Mutex::new(AutomaticSchedule::new(now)),
            refresh_flight: SingleFlight::default(),
            contributors_flight: SingleFlight::default(),
            contributors: Mutex::new(None),
            image_flights: Mutex::new(HashMap::new()),
            image_slots: ImageSlots::default(),
            emit: Box::new(emit),
        }
    }

    /// Read-only: navigation, remounts and authentication return may call this freely.
    pub fn snapshot(&self) -> AppContentSnapshot {
        self.state.lock().unwrap().clone()
    }

    pub fn automatic_tick(&self) -> bool {
        if self.config.sources.is_none() && !self.config.uses_github_releases() {
            return false;
        }
        let next = {
            let mut schedule = self.schedule.lock().unwrap();
            if !schedule.take_due(self.clock.now_ms()) {
                return false;
            }
            schedule.next_at
        };
        self.mutate(|snapshot| snapshot.next_automatic_check_at = Some(next));
        self.refresh();
        true
    }

    fn mutate(&self, change: impl FnOnce(&mut AppContentSnapshot)) {
        let snapshot = {
            let mut snapshot = self.state.lock().unwrap();
            change(&mut snapshot);
            snapshot.revision += 1;
            snapshot.clone()
        };
        (self.emit)(snapshot);
    }

    pub fn refresh(&self) -> AppContentSnapshot {
        let sources = self.config.sources.as_ref();
        if sources.is_none() && !self.config.uses_github_releases() {
            return self.snapshot();
        }
        self.refresh_flight.run(|| {
            let attempted_at = self.clock.now_ms();
            self.mutate(|snapshot| {
                snapshot.refreshing = true;
                snapshot.announcements.last_attempt_at = Some(attempted_at);
                snapshot.update.last_attempt_at = Some(attempted_at);
            });
            // Each document has its own fallback chain. No successful request is repeated.
            let (announcements, update) = if let Some(sources) = sources {
                std::thread::scope(|scope| {
                    let announcements =
                        scope.spawn(|| self.fetch::<Announcements>(sources, "announcements.json"));
                    let update = self.fetch::<UpdateInfo>(sources, "update.json");
                    (
                        Some(announcements.join().expect("content request worker")),
                        update,
                    )
                })
            } else {
                let (announcements, update) = self.fetch_github_releases();
                (Some(announcements), update)
            };
            let completed_at = self.clock.now_ms();
            let cache_scope = sources.map(Sources::key).unwrap_or_else(|| {
                format!(
                    "github:{}",
                    self.config.repository.as_ref().unwrap().full_name
                )
            });
            if let Some(Ok((data, source))) = &announcements {
                self.cache.save(
                    &cache_scope,
                    "announcements.json",
                    data,
                    *source,
                    completed_at,
                );
            }
            if let Ok((data, source)) = &update {
                self.cache.save(
                    &cache_scope,
                    if sources.is_some() {
                        "update.json"
                    } else {
                        "latest-release.json"
                    },
                    data,
                    *source,
                    completed_at,
                );
            }
            self.mutate(|snapshot| {
                if let Some(announcements) = announcements {
                    apply_result(&mut snapshot.announcements, announcements, completed_at);
                }
                apply_result(&mut snapshot.update, update, completed_at);
                snapshot.refreshing = false;
                snapshot.update_available = snapshot
                    .update
                    .data
                    .as_ref()
                    .is_some_and(|update| is_newer(&snapshot.version, &update.latest_ver));
            });
            let allowed = self.allowed_images();
            self.image_flights
                .lock()
                .unwrap()
                .retain(|url, _| allowed.contains(url));
            self.cache.prune_images(&cache_scope, &allowed);
            self.snapshot()
        })
    }

    fn fetch_github_releases(
        &self,
    ) -> (
        Result<(Announcements, Source), ContentError>,
        Result<(UpdateInfo, Source), ContentError>,
    ) {
        #[derive(Deserialize)]
        struct Release {
            tag_name: String,
            name: Option<String>,
            body: Option<String>,
            html_url: Option<String>,
            assets: Vec<Asset>,
        }
        #[derive(Deserialize)]
        struct Asset {
            name: String,
            browser_download_url: String,
        }
        let Some(repository) = self.config.repository.as_ref() else {
            let error = ContentError::new(ErrorKind::Unavailable, "未配置 GitHub 仓库");
            return (Err(error.clone()), Err(error));
        };
        let get_release = |endpoint: &str| -> Result<Release, ContentError> {
            let url = Url::parse(&format!(
                "https://api.github.com/repos/{}/{endpoint}",
                repository.full_name
            ))
            .map_err(|_| ContentError::invalid("GitHub 仓库地址无效"))?;
            let response = self
                .http
                .get(&url, RequestKind::Github, JSON_LIMIT)?
                .successful(RequestKind::Github)?;
            serde_json::from_slice(&response.body)
                .map_err(|_| ContentError::invalid("GitHub Release 响应无效"))
        };
        let get_releases = |endpoint: &str| -> Result<Vec<Release>, ContentError> {
            let url = Url::parse(&format!(
                "https://api.github.com/repos/{}/{endpoint}",
                repository.full_name
            ))
            .map_err(|_| ContentError::invalid("GitHub 仓库地址无效"))?;
            let response = self
                .http
                .get(&url, RequestKind::Github, JSON_LIMIT)?
                .successful(RequestKind::Github)?;
            serde_json::from_slice(&response.body)
                .map_err(|_| ContentError::invalid("GitHub Release 响应无效"))
        };
        let announcement = get_release("releases/tags/announcement").and_then(|release| {
            let body = release.body.unwrap_or_default();
            let data: Announcements = serde_json::from_str(&body).map_err(|_| {
                ContentError::invalid("announcement Release 的 body 不是有效公告 JSON")
            })?;
            data.validate()?;
            Ok((data, Source::Github))
        });
        let update = (|| -> Result<(UpdateInfo, Source), ContentError> {
            let releases = get_releases("releases?per_page=10")
                .or_else(|_| get_release("releases/latest").map(|r| vec![r]))
                .or_else(|_| get_release("releases/tags/canary").map(|r| vec![r]))?;

            struct Candidate<'a> {
                release: &'a Release,
                asset: Option<&'a Asset>,
                version: String,
            }

            let mut candidates: Vec<Candidate> = Vec::new();
            for release in &releases {
                let asset_names: Vec<&str> =
                    release.assets.iter().map(|a| a.name.as_str()).collect();
                if let Some(asset_idx) = find_platform_asset_index(&asset_names) {
                    let asset = &release.assets[asset_idx];
                    if let Some(ver) = extract_release_version(
                        &release.tag_name,
                        release.name.as_deref(),
                        &asset.name,
                    ) {
                        candidates.push(Candidate {
                            release,
                            asset: Some(asset),
                            version: ver,
                        });
                    }
                } else {
                    #[cfg(target_os = "linux")]
                    {
                        if let Some(ver) =
                            extract_release_version(&release.tag_name, release.name.as_deref(), "")
                        {
                            candidates.push(Candidate {
                                release,
                                asset: None,
                                version: ver,
                            });
                        }
                    }
                }
            }

            if candidates.is_empty() {
                return Err(ContentError::new(
                    ErrorKind::Unavailable,
                    "最新 GitHub Release 未包含当前平台的安装包",
                ));
            }

            let newer_candidate = candidates
                .iter()
                .find(|c| is_newer(&self.config.version, &c.version));
            let selected = newer_candidate.unwrap_or(&candidates[0]);

            let release_url = selected.release.html_url.clone().or_else(|| {
                self.config.repository.as_ref().map(|r| {
                    format!(
                        "https://github.com/{}/releases/tag/{}",
                        r.full_name, selected.release.tag_name
                    )
                })
            });

            let (direct, accelerated) = if let Some(asset) = selected.asset {
                let direct = web_url(&asset.browser_download_url)?;
                let accelerated = self
                    .config
                    .github_accelerators
                    .iter()
                    .find_map(|prefix| web_url(&format!("{prefix}{direct}")).ok());
                (direct, accelerated)
            } else {
                let direct = web_url(release_url.as_deref().unwrap_or("https://github.com"))?;
                (direct, None)
            };

            let update = UpdateInfo {
                latest_ver: selected.version.clone(),
                latest_ver_url: accelerated.unwrap_or(direct).to_string(),
                changelog: selected.release.body.clone().unwrap_or_default(),
                release_url,
            };
            update.validate()?;
            Ok((update, Source::Github))
        })();
        (announcement, update)
    }

    fn fetch<T: Document>(
        &self,
        sources: &Sources,
        name: &str,
    ) -> Result<(T, Source), ContentError> {
        let request = |base: &Url| {
            let url = base
                .join(name)
                .map_err(|_| ContentError::invalid("内容源地址无效"))?;
            let response = self
                .http
                .get(&url, RequestKind::Json, JSON_LIMIT)?
                .successful(RequestKind::Json)?;
            parse_document(&response.body)
        };
        match request(&sources.primary) {
            Ok(data) => Ok((data, Source::Primary)),
            Err(primary) => match &sources.fallback {
                None => Err(primary),
                Some(fallback) => request(fallback)
                    .map(|data| (data, Source::Fallback))
                    .map_err(|fallback| {
                        ContentError::new(
                            fallback.kind,
                            format!(
                                "主地址：{}；备用地址：{}",
                                primary.message, fallback.message
                            ),
                        )
                    }),
            },
        }
    }

    pub fn get_contributors(&self) -> ContributorsSnapshot {
        self.contributors_flight.run(|| {
            let Some(repository) = &self.config.repository else {
                return ContributorsSnapshot {
                    status: ContributorStatus::Unconfigured,
                    repository: None,
                    contributors: Vec::new(),
                    truncated: false,
                    from_cache: false,
                    last_success_at: None,
                    error: None,
                    message: self.config.repository_message.clone(),
                };
            };
            let now = self.clock.now_ms();
            let scope = format!("github:{}", repository.full_name.to_ascii_lowercase());
            let mut previous = self.contributors.lock().unwrap().clone().or_else(|| {
                self.cache
                    .load::<ContributorList>(&scope, "contributors.json", now)
                    .map(|cached| ContributorsSnapshot {
                        status: ContributorStatus::Ready,
                        repository: Some(repository.clone()),
                        contributors: cached.data.contributors,
                        truncated: cached.data.truncated,
                        from_cache: true,
                        last_success_at: Some(cached.saved_at),
                        error: None,
                        message: None,
                    })
            });
            if let Some(cached) = &mut previous {
                let fresh = cached.error.is_none()
                    && cached
                        .last_success_at
                        .is_some_and(|time| now >= time && now - time < DAY_MS);
                let limited = cached
                    .error
                    .as_ref()
                    .and_then(|error| error.retry_at)
                    .is_some_and(|time| time > now);
                if fresh || limited {
                    cached.from_cache = cached.last_success_at.is_some();
                    *self.contributors.lock().unwrap() = Some(cached.clone());
                    return cached.clone();
                }
            }
            let result = match self.fetch_contributors(repository) {
                Ok(data) => {
                    let completed_at = self.clock.now_ms();
                    self.cache.save(
                        &scope,
                        "contributors.json",
                        &data,
                        Source::Primary,
                        completed_at,
                    );
                    ContributorsSnapshot {
                        status: ContributorStatus::Ready,
                        repository: Some(repository.clone()),
                        contributors: data.contributors,
                        truncated: data.truncated,
                        from_cache: false,
                        last_success_at: Some(completed_at),
                        error: None,
                        message: None,
                    }
                }
                Err(mut error) => {
                    let status = if error.kind == ErrorKind::RateLimited {
                        error.retry_at = Some(
                            error
                                .retry_at
                                .filter(|time| *time > now)
                                .unwrap_or(now + 60 * 60 * 1000),
                        );
                        ContributorStatus::RateLimited
                    } else {
                        ContributorStatus::Error
                    };
                    ContributorsSnapshot {
                        status,
                        repository: Some(repository.clone()),
                        contributors: previous
                            .as_ref()
                            .map(|value| value.contributors.clone())
                            .unwrap_or_default(),
                        truncated: previous.as_ref().is_some_and(|value| value.truncated),
                        from_cache: previous
                            .as_ref()
                            .is_some_and(|value| value.last_success_at.is_some()),
                        last_success_at: previous.and_then(|value| value.last_success_at),
                        error: Some(error),
                        message: None,
                    }
                }
            };
            *self.contributors.lock().unwrap() = Some(result.clone());
            result
        })
    }

    fn fetch_contributors(&self, repository: &Repository) -> Result<ContributorList, ContentError> {
        #[derive(Deserialize)]
        struct GithubContributor {
            login: String,
            avatar_url: String,
        }
        let mut result = ContributorList {
            contributors: Vec::new(),
            truncated: false,
        };
        let mut seen = HashSet::new();
        for page in 1..=10 {
            // Construct every page locally; never follow arbitrary URLs from the Link header.
            let url = Url::parse(&format!(
                "https://api.github.com/repos/{}/contributors?per_page=100&page={page}",
                repository.full_name
            ))
            .map_err(|_| ContentError::invalid("GitHub 仓库地址无效"))?;
            let response = self
                .http
                .get(&url, RequestKind::Github, JSON_LIMIT)?
                .successful(RequestKind::Github)?;
            let entries: Vec<GithubContributor> = if response.status == 204 {
                Vec::new()
            } else {
                serde_json::from_slice(&response.body)
                    .map_err(|_| ContentError::invalid("GitHub 贡献者响应无效"))?
            };
            if entries.len() > 100 {
                return Err(ContentError::invalid("GitHub 单页贡献者数量超限"));
            }
            for entry in entries {
                if seen.insert(entry.login.clone()) {
                    result.contributors.push(Contributor {
                        url: format!("https://github.com/{}", entry.login),
                        login: entry.login,
                        avatar_url: entry.avatar_url,
                    });
                }
            }
            result.validate()?;
            if !response.has_next_page {
                break;
            }
            if page == 10 {
                result.truncated = true;
            }
        }
        Ok(result)
    }

    fn allowed_images(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .announcements
            .data
            .as_ref()
            .map(|data| {
                data.announcements
                    .iter()
                    .filter_map(|unit| unit.pic.as_deref())
                    .filter_map(|pic| web_url(pic).ok().map(|url| url.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn require_image(&self, url: &str) -> Result<(), ContentError> {
        if self.allowed_images().iter().any(|allowed| allowed == url) {
            Ok(())
        } else {
            Err(ContentError::invalid("只能加载当前公告中的头图"))
        }
    }

    pub fn load_image(&self, value: &str) -> Result<String, ContentError> {
        let url = web_url(value)?;
        self.require_image(url.as_str())?;
        let scope = self
            .config
            .sources
            .as_ref()
            .map(Sources::key)
            .or_else(|| {
                self.config.uses_github_releases().then(|| {
                    format!(
                        "github:{}",
                        self.config.repository.as_ref().unwrap().full_name
                    )
                })
            })
            .ok_or_else(|| ContentError::invalid("未配置内容源"))?;
        let flight = self
            .image_flights
            .lock()
            .unwrap()
            .entry(url.to_string())
            .or_default()
            .clone();
        flight.run(|| {
            let _slot = self.image_slots.acquire();
            self.require_image(url.as_str())?;
            if let Some(bytes) = self.cache.load_image(&scope, url.as_str()) {
                if let Ok(data) = image_data_url(&bytes, None) {
                    self.require_image(url.as_str())?;
                    return Ok(data);
                }
            }
            let response = self
                .http
                .get(&url, RequestKind::Image, IMAGE_LIMIT)?
                .successful(RequestKind::Image)?;
            // No HTML, SVG, missing Content-Type, or mismatched magic bytes can enter the WebView.
            let content_type = response
                .content_type
                .as_deref()
                .ok_or_else(|| ContentError::invalid("头图缺少图片类型"))?;
            let data = image_data_url(&response.body, Some(content_type))?;
            self.require_image(url.as_str())?;
            self.cache.save_image(&scope, url.as_str(), &response.body);
            Ok(data)
        })
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn collect_candidate_urls(
    direct_url: &str,
    accelerators: &[reqwest::Url],
) -> Vec<String> {
    let clean_direct = if let Some(idx) = direct_url.find("https://github.com/") {
        &direct_url[idx..]
    } else if let Some(idx) = direct_url.find("http://github.com/") {
        &direct_url[idx..]
    } else {
        direct_url
    };

    let mut candidate_urls = Vec::new();
    for accelerator in accelerators {
        let prefix = accelerator.as_str();
        candidate_urls.push(format!("{prefix}{clean_direct}"));
    }
    candidate_urls.push(clean_direct.to_string());
    candidate_urls
}

/// 判断发布资产文件名是否匹配当前平台的可安装包。
pub(super) fn is_platform_asset(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    #[cfg(target_os = "android")]
    {
        name.ends_with(".apk.gz") || name.ends_with(".apk")
    }
    #[cfg(target_os = "windows")]
    {
        name.ends_with(".msi")
    }
    #[cfg(target_os = "macos")]
    {
        name.ends_with(".dmg") || name.ends_with(".pkg")
    }
    #[cfg(target_os = "linux")]
    {
        name.ends_with(".deb") || name.ends_with(".appimage") || name.ends_with(".rpm")
    }
    #[cfg(not(any(
        target_os = "android",
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    {
        let _ = name;
        false
    }
}

/// 找出当前平台安装包的资产下标。
///
/// 用「偏好扩展名」的顺序表达平台差异，而不是给每个平台重复写一遍匹配逻辑，
/// 这样 [`is_platform_asset`] 在所有平台上都是真正参与运行时判断的代码。
pub(super) fn find_platform_asset_index(names: &[&str]) -> Option<usize> {
    let preferred: &[&str] = if cfg!(target_os = "android") {
        // Android 优先选择压缩后的 .apk.gz，再回退到 .apk。
        &[".apk.gz", ".apk"]
    } else {
        &[]
    };

    for extension in preferred {
        if let Some(index) = names
            .iter()
            .position(|name| name.to_ascii_lowercase().ends_with(extension))
        {
            return Some(index);
        }
    }

    names.iter().position(|name| is_platform_asset(name))
}

pub(super) fn extract_version_from_asset_name(name: &str) -> Option<String> {
    if let Some(idx) = name.find("-release.") {
        let prefix = &name[..idx];
        if let Some(last_underscore) = prefix.rfind('_') {
            let candidate = &prefix[last_underscore + 1..];
            if let Ok(ver) = version(candidate) {
                return Some(ver.to_string());
            }
        }
    }
    None
}

pub(super) fn extract_release_version(
    tag_name: &str,
    release_name: Option<&str>,
    asset_name: &str,
) -> Option<String> {
    // 1. Try tag_name if it is valid SemVer (e.g. v0.9.0)
    if let Ok(ver) = version(tag_name) {
        return Some(ver.to_string());
    }
    // 2. Try extracting from asset filename (e.g. your-szu-life_android_arm64_0.9.0-release.apk.gz)
    if let Some(ver) = extract_version_from_asset_name(asset_name) {
        return Some(ver);
    }
    // 3. Try extracting from release_name (e.g. canary v0.9.0)
    if let Some(name) = release_name {
        for word in name.split_whitespace() {
            let candidate = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
            if let Ok(ver) = version(candidate) {
                return Some(ver.to_string());
            }
        }
    }
    None
}

fn load_feed<T: Document>(cache: &Cache, scope: &str, name: &str, now: u64) -> FeedSnapshot<T> {
    match cache.load(scope, name, now) {
        Some(cached) => FeedSnapshot {
            data: Some(cached.data),
            last_attempt_at: None,
            last_success_at: Some(cached.saved_at),
            source: Some(cached.source),
            from_cache: true,
            error: None,
        },
        None => FeedSnapshot::default(),
    }
}

fn apply_result<T>(
    snapshot: &mut FeedSnapshot<T>,
    result: Result<(T, Source), ContentError>,
    completed_at: u64,
) {
    match result {
        Ok((data, source)) => {
            snapshot.data = Some(data);
            snapshot.last_success_at = Some(completed_at);
            snapshot.source = Some(source);
            snapshot.from_cache = false;
            snapshot.error = None;
        }
        Err(error) => {
            snapshot.error = Some(error);
            snapshot.from_cache = snapshot.data.is_some();
        }
    }
}
