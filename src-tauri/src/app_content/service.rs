use super::{
    cache::Cache,
    config::{ContentConfig, Sources},
    http::{PublicHttp, RequestKind},
    images::normalize_image,
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

type ImageFlight = Arc<SingleFlight<Result<Vec<u8>, ContentError>>>;

/// 一次更新检查的结果。`fallback` 表示稳定版频道因暂无正式版而回退了测试版。
#[derive(Clone)]
struct UpdateOutcome {
    data: UpdateInfo,
    source: Source,
    fallback: bool,
}

/// GitHub Release 的最小视图。`draft` / `prerelease` 决定它属于哪个频道。
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub(super) struct ContentService {
    pub config: ContentConfig,
    http: Arc<dyn PublicHttp>,
    clock: Arc<dyn Clock>,
    cache: Cache,
    state: Mutex<AppContentSnapshot>,
    schedule: Mutex<AutomaticSchedule>,
    /// `(当前频道, 切换代数)`。代数用于丢弃切换前在途刷新返回的旧频道数据。
    channel: Mutex<(UpdateChannel, u64)>,
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
        channel: UpdateChannel,
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
            update_channel: channel,
            update_channel_fallback: false,
            announcements: FeedSnapshot::default(),
            update: FeedSnapshot::default(),
            update_available: false,
        };
        if let Some(sources) = &config.sources {
            snapshot.announcements =
                load_announcements_feed(&cache, &sources.key(), now, &config.version);
            snapshot.update = load_feed(&cache, &sources.key(), "update.json", now);
        }
        if config.uses_github_releases() {
            if let Some(repository) = &config.repository {
                snapshot.announcements = load_announcements_feed(
                    &cache,
                    &github_announcement_scope(repository),
                    now,
                    &config.version,
                );
                snapshot.update = load_feed(
                    &cache,
                    &github_update_scope(repository, channel),
                    "latest-release.json",
                    now,
                );
            }
        }
        snapshot.update_available = snapshot
            .update
            .data
            .as_ref()
            .is_some_and(|update| is_newer(&config.version, &update.latest_ver));
        Self {
            config,
            http,
            clock,
            cache,
            state: Mutex::new(snapshot),
            schedule: Mutex::new(AutomaticSchedule::new(now)),
            channel: Mutex::new((channel, 0)),
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

    /// 切换更新频道：立即展示该频道自己的缓存，随后由调用方触发一次检查。
    ///
    /// 频道状态与代数在同一个锁里更新，刷新侧据此丢弃切换前出发的请求结果。
    pub fn set_channel(&self, channel: UpdateChannel) -> AppContentSnapshot {
        let changed = {
            let mut state = self.channel.lock().unwrap();
            if state.0 == channel {
                false
            } else {
                state.0 = channel;
                state.1 = state.1.wrapping_add(1);
                true
            }
        };
        if !changed {
            return self.snapshot();
        }
        let now = self.clock.now_ms();
        let update = if let Some(sources) = &self.config.sources {
            load_feed(&self.cache, &sources.key(), "update.json", now)
        } else if self.config.uses_github_releases() {
            match &self.config.repository {
                Some(repository) => load_feed(
                    &self.cache,
                    &github_update_scope(repository, channel),
                    "latest-release.json",
                    now,
                ),
                None => FeedSnapshot::default(),
            }
        } else {
            FeedSnapshot::default()
        };
        self.mutate(|snapshot| {
            snapshot.update_channel = channel;
            // 回退标记只代表上一轮检查的结论，换频道后先当作「未确认」。
            snapshot.update_channel_fallback = false;
            snapshot.update = update;
            snapshot.update_available = snapshot
                .update
                .data
                .as_ref()
                .is_some_and(|update| is_newer(&snapshot.version, &update.latest_ver));
        });
        self.snapshot()
    }

    pub fn refresh(&self) -> AppContentSnapshot {
        if self.config.sources.is_none() && !self.config.uses_github_releases() {
            return self.snapshot();
        }
        // 切频道会让在途刷新的结果作废；此时最多再跑一轮，避免旧频道的数据
        // 盖住新频道的快照。
        for _ in 0..2 {
            let epoch = self.channel_state().1;
            let snapshot = self.refresh_flight.run(|| self.refresh_once(epoch));
            if self.channel_state().1 == epoch {
                return snapshot;
            }
        }
        self.snapshot()
    }

    fn channel_state(&self) -> (UpdateChannel, u64) {
        *self.channel.lock().unwrap()
    }

    fn refresh_once(&self, epoch: u64) -> AppContentSnapshot {
        let channel = self.channel_state().0;
        let attempted_at = self.clock.now_ms();
        self.mutate(|snapshot| {
            snapshot.refreshing = true;
            snapshot.announcements.last_attempt_at = Some(attempted_at);
            snapshot.update.last_attempt_at = Some(attempted_at);
        });
        let announcement_scope = self.config.sources.as_ref().map(Sources::key).or_else(|| {
            self.config
                .repository
                .as_ref()
                .map(github_announcement_scope)
        });
        let update_scope = self.config.sources.as_ref().map(Sources::key).or_else(|| {
            self.config
                .repository
                .as_ref()
                .map(|repository| github_update_scope(repository, channel))
        });
        // Each document has its own fallback chain. No successful request is repeated.
        let (announcements, update) = if let Some(sources) = &self.config.sources {
            std::thread::scope(|scope| {
                let announcements =
                    scope.spawn(|| self.fetch::<Announcements>(sources, "announcements.json"));
                let update =
                    self.fetch::<UpdateInfo>(sources, "update.json")
                        .map(|(data, source)| UpdateOutcome {
                            data,
                            source,
                            fallback: false,
                        });
                (
                    Some(announcements.join().expect("content request worker")),
                    update,
                )
            })
        } else {
            let announcements = self.fetch_github_announcement().or_else(|github_err| {
                log::info!("GitHub 公告获取失败 ({github_err:?})，尝试从 CNB 镜像获取");
                self.fetch_cnb_announcements()
            });
            (Some(announcements), self.fetch_update(channel))
        };
        let completed_at = self.clock.now_ms();

        // 频道在请求期间被切换：丢弃这一轮结果，让调用方按新频道重跑。
        if self.channel_state().1 != epoch {
            self.mutate(|snapshot| snapshot.refreshing = false);
            return self.snapshot();
        }

        if let (Some(scope), Some(Ok((data, source)))) = (&announcement_scope, &announcements) {
            self.cache
                .save(scope, "announcements.json", data, *source, completed_at);
        }
        if let (Some(scope), Ok(outcome)) = (&update_scope, &update) {
            self.cache.save(
                scope,
                if self.config.sources.is_some() {
                    "update.json"
                } else {
                    "latest-release.json"
                },
                &outcome.data,
                outcome.source,
                completed_at,
            );
        }
        let fallback = match &update {
            Ok(outcome) => outcome.fallback,
            Err(_) => false,
        };
        self.mutate(|snapshot| {
            if let Some(announcements) = announcements.as_ref() {
                // 快照侧按当前版本过滤，缓存侧已在上方保存原始数据。
                let filtered = announcements.clone().map(|(data, source)| {
                    (applicable_announcements(data, &snapshot.version), source)
                });
                apply_result(&mut snapshot.announcements, filtered, completed_at);
            }
            apply_update(&mut snapshot.update, &update, completed_at);
            snapshot.refreshing = false;
            snapshot.update_channel = channel;
            snapshot.update_channel_fallback = fallback;
            snapshot.update_available = snapshot
                .update
                .data
                .as_ref()
                .is_some_and(|update| is_newer(&snapshot.version, &update.latest_ver));
        });
        if let (Some(Ok(_)), Some(scope)) = (&announcements, &announcement_scope) {
            let allowed = self.allowed_images();
            self.image_flights
                .lock()
                .unwrap()
                .retain(|url, _| allowed.contains(url));
            self.cache.prune_images(scope, &allowed);
        }
        self.snapshot()
    }

    fn fetch_github_announcement(&self) -> Result<(Announcements, Source), ContentError> {
        let Some(repository) = self.config.repository.as_ref() else {
            return Err(ContentError::new(
                ErrorKind::Unavailable,
                "未配置 GitHub 仓库",
            ));
        };
        let release = self.github_release(repository, "releases/tags/announcement")?;
        let body = release.body.unwrap_or_default();
        let data: Announcements = serde_json::from_str(&body)
            .map_err(|_| ContentError::invalid("announcement Release 的 body 不是有效公告 JSON"))?;
        data.validate()?;
        Ok((data, Source::Github))
    }

    fn fetch_update(&self, channel: UpdateChannel) -> Result<UpdateOutcome, ContentError> {
        self.fetch_github_update(channel).or_else(|github_err| {
            log::info!("GitHub 更新检查失败 ({github_err:?})，尝试从 CNB 镜像获取");
            self.fetch_cnb_update(channel)
        })
    }

    fn github_release(
        &self,
        repository: &Repository,
        endpoint: &str,
    ) -> Result<GithubRelease, ContentError> {
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
    }

    fn github_releases(
        &self,
        repository: &Repository,
        endpoint: &str,
    ) -> Result<Vec<GithubRelease>, ContentError> {
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
    }

    fn fetch_github_update(&self, channel: UpdateChannel) -> Result<UpdateOutcome, ContentError> {
        let Some(repository) = self.config.repository.as_ref() else {
            return Err(ContentError::new(
                ErrorKind::Unavailable,
                "未配置 GitHub 仓库",
            ));
        };

        // canary 频道优先直接取滚动 tag；稳定版在列表里筛掉草稿与预发布。
        let mut releases = Vec::new();
        if !channel.is_stable() {
            match self.github_release(repository, "releases/tags/canary") {
                Ok(release) => releases.push(release),
                Err(error) if error.kind == ErrorKind::RateLimited => return Err(error),
                Err(_) => {}
            }
        }
        if releases.is_empty() {
            releases = self.github_releases(repository, "releases?per_page=10")?;
        }
        if let Some(selected) = self.select_github_update(repository, &releases, channel)? {
            return Ok(selected);
        }

        if !channel.is_stable() {
            return Err(ContentError::new(
                ErrorKind::Unavailable,
                "GitHub Release 中还没有测试版",
            ));
        }

        // 列表可能因排序或分页漏掉正式版，用 latest 端点再确认一次。
        if let Ok(latest) = self.github_release(repository, "releases/latest") {
            if let Some(selected) =
                self.select_github_update(repository, std::slice::from_ref(&latest), channel)?
            {
                return Ok(selected);
            }
        }
        // 正式版空窗：回退到 canary，并标记「当前展示的是测试版内容」。
        if let Some(mut selected) = self.github_canary_update(repository)? {
            selected.fallback = true;
            return Ok(selected);
        }
        Err(ContentError::new(
            ErrorKind::Unavailable,
            "GitHub Release 中还没有正式版本",
        ))
    }

    fn github_canary_update(
        &self,
        repository: &Repository,
    ) -> Result<Option<UpdateOutcome>, ContentError> {
        let mut releases = Vec::new();
        match self.github_release(repository, "releases/tags/canary") {
            Ok(release) => releases.push(release),
            Err(error) if error.kind == ErrorKind::RateLimited => return Err(error),
            Err(_) => releases = self.github_releases(repository, "releases?per_page=10")?,
        }
        self.select_github_update(repository, &releases, UpdateChannel::Canary)
    }

    fn select_github_update(
        &self,
        repository: &Repository,
        releases: &[GithubRelease],
        channel: UpdateChannel,
    ) -> Result<Option<UpdateOutcome>, ContentError> {
        struct Candidate<'a> {
            release: &'a GithubRelease,
            asset: Option<&'a GithubAsset>,
            version: String,
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        for release in releases {
            if !release_matches_channel(release, channel) {
                continue;
            }
            let asset_names: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            if let Some(asset_idx) = find_platform_asset_index(&asset_names) {
                let asset = &release.assets[asset_idx];
                if let Some(ver) =
                    extract_release_version(&release.tag_name, release.name.as_deref(), &asset.name)
                {
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
            return Ok(None);
        }

        let newer_candidate = candidates
            .iter()
            .find(|c| is_newer(&self.config.version, &c.version));
        let selected = newer_candidate.unwrap_or(&candidates[0]);

        let release_url = Some(selected.release.html_url.clone().unwrap_or_else(|| {
            format!(
                "https://github.com/{}/releases/tag/{}",
                repository.full_name, selected.release.tag_name
            )
        }));

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
        Ok(Some(UpdateOutcome {
            data: update,
            source: Source::Github,
            fallback: false,
        }))
    }

    fn fetch_cnb_announcements(&self) -> Result<(Announcements, Source), ContentError> {
        let url = Url::parse(&format!(
            "https://cnb.cool/{}/-/releases/tags/announcement",
            self.config.cnb_repository
        ))
        .map_err(|_| ContentError::invalid("CNB 公告地址无效"))?;
        let response = self
            .http
            .get(&url, RequestKind::CnbWeb, JSON_LIMIT)?
            .successful(RequestKind::CnbWeb)?;
        let data: CnbTagReleaseResponse = serde_json::from_slice(&response.body)
            .map_err(|_| ContentError::invalid("CNB 公告响应不是有效 JSON"))?;
        let release = data
            .release
            .ok_or_else(|| ContentError::invalid("CNB 未找到 announcement Release"))?;
        let body = release.body.unwrap_or_default();
        let announcements: Announcements = serde_json::from_str(&body).map_err(|_| {
            ContentError::invalid("CNB announcement Release 的 body 不是有效公告 JSON")
        })?;
        announcements.validate()?;
        Ok((announcements, Source::Cnb))
    }

    fn fetch_cnb_update(&self, channel: UpdateChannel) -> Result<UpdateOutcome, ContentError> {
        let url = Url::parse(&format!(
            "https://cnb.cool/{}/-/releases",
            self.config.cnb_repository
        ))
        .map_err(|_| ContentError::invalid("CNB 仓库地址无效"))?;
        let response = self
            .http
            .get(&url, RequestKind::CnbWeb, JSON_LIMIT)?
            .successful(RequestKind::CnbWeb)?;
        let data: CnbReleasesResponse = serde_json::from_slice(&response.body)
            .map_err(|_| ContentError::invalid("CNB Release 列表响应无效"))?;

        if let Some(selected) = self.select_cnb_update(&data.releases, channel)? {
            return Ok(selected);
        }
        if !channel.is_stable() {
            return Err(ContentError::new(
                ErrorKind::Unavailable,
                "CNB Release 中还没有测试版",
            ));
        }
        // 正式版空窗：回退到 canary，并标记「当前展示的是测试版内容」。
        if let Some(mut selected) = self.select_cnb_update(&data.releases, UpdateChannel::Canary)? {
            selected.fallback = true;
            return Ok(selected);
        }
        Err(ContentError::new(
            ErrorKind::Unavailable,
            "CNB Release 中还没有正式版本",
        ))
    }

    fn select_cnb_update(
        &self,
        releases: &[CnbRelease],
        channel: UpdateChannel,
    ) -> Result<Option<UpdateOutcome>, ContentError> {
        struct Candidate<'a> {
            release: &'a CnbRelease,
            asset: Option<&'a CnbAsset>,
            version: String,
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        for release in releases {
            if !cnb_release_matches_channel(release, channel) {
                continue;
            }
            let asset_names: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            if let Some(asset_idx) = find_platform_asset_index(&asset_names) {
                let asset = &release.assets[asset_idx];
                if let Some(ver) = extract_release_version(
                    release.tag_name(),
                    release.title.as_deref(),
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
                        extract_release_version(release.tag_name(), release.title.as_deref(), "")
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
            return Ok(None);
        }

        let newer_candidate = candidates
            .iter()
            .find(|c| is_newer(&self.config.version, &c.version));
        let selected = newer_candidate.unwrap_or(&candidates[0]);

        let release_url = format!(
            "https://cnb.cool/{}/-/releases/tag/{}",
            self.config.cnb_repository,
            selected.release.tag_name()
        );

        let direct_url = if let Some(asset) = selected.asset {
            if !asset.browser_download_url.is_empty() {
                asset.browser_download_url.clone()
            } else {
                format!(
                    "https://cnb.cool/{}/-/releases/download/{}/{}",
                    self.config.cnb_repository,
                    selected.release.tag_name(),
                    asset.name
                )
            }
        } else {
            release_url.clone()
        };

        let update = UpdateInfo {
            latest_ver: selected.version.clone(),
            latest_ver_url: direct_url,
            changelog: selected.release.body.clone().unwrap_or_default(),
            release_url: Some(release_url),
        };
        update.validate()?;
        Ok(Some(UpdateOutcome {
            data: update,
            source: Source::Cnb,
            fallback: false,
        }))
    }

    #[cfg(any(target_os = "android", target_os = "windows", test))]
    pub fn check_cnb_sync(&self, tag: &str, asset_name: &str) -> Option<String> {
        let url = Url::parse(&format!(
            "https://cnb.cool/{}/-/releases/tags/{}",
            self.config.cnb_repository, tag
        ))
        .ok()?;
        let response = self
            .http
            .get(&url, RequestKind::CnbWeb, JSON_LIMIT)
            .ok()?
            .successful(RequestKind::CnbWeb)
            .ok()?;
        let data: CnbTagReleaseResponse = serde_json::from_slice(&response.body).ok()?;
        let release = data.release?;
        let asset = release.assets.iter().find(|a| a.name == asset_name)?;
        if !asset.browser_download_url.is_empty() {
            Some(asset.browser_download_url.clone())
        } else {
            Some(format!(
                "https://cnb.cool/{}/-/releases/download/{}/{}",
                self.config.cnb_repository, tag, asset_name
            ))
        }
    }

    #[cfg(any(target_os = "android", target_os = "windows", test))]
    pub fn build_download_candidates(&self, direct_url: &str) -> Vec<(u32, String)> {
        let mut candidates = Vec::new();
        let github_repo = self
            .config
            .repository
            .as_ref()
            .map(|r| r.full_name.as_str())
            .unwrap_or("rinnein/your-szu-life");

        if let Some((tag, filename)) = extract_release_tag_and_filename(direct_url) {
            let is_from_cnb = direct_url.contains("cnb.cool");
            let cnb_url = if is_from_cnb {
                Some(direct_url.to_string())
            } else {
                self.check_cnb_sync(tag, filename)
            };

            let github_direct =
                format!("https://github.com/{github_repo}/releases/download/{tag}/{filename}");

            if let Some(cnb) = cnb_url {
                candidates.push((1, cnb));
                for accelerator in &self.config.github_accelerators {
                    let prefix = accelerator.as_str();
                    candidates.push((2, format!("{prefix}{github_direct}")));
                }
                candidates.push((3, github_direct));
            } else {
                for accelerator in &self.config.github_accelerators {
                    let prefix = accelerator.as_str();
                    candidates.push((1, format!("{prefix}{github_direct}")));
                }
                candidates.push((2, github_direct));
            }
        } else {
            let urls = collect_candidate_urls(direct_url, &self.config.github_accelerators);
            for (idx, url) in urls.into_iter().enumerate() {
                candidates.push((idx as u32 + 1, url));
            }
        }

        candidates
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

    pub fn load_image(&self, value: &str) -> Result<Vec<u8>, ContentError> {
        let url = web_url(value)?;
        self.require_image(url.as_str())?;
        let scope = if let Some(sources) = &self.config.sources {
            Some(sources.key())
        } else if self.config.uses_github_releases() {
            self.config
                .repository
                .as_ref()
                .map(github_announcement_scope)
        } else {
            None
        };
        let scope = scope.ok_or_else(|| ContentError::invalid("未配置内容源"))?;
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
                if let Ok((bytes, _)) = normalize_image(&bytes, None) {
                    self.require_image(url.as_str())?;
                    return Ok(bytes);
                }
            }
            let response = self
                .http
                .get(&url, RequestKind::Image, IMAGE_DOWNLOAD_LIMIT)?
                .successful(RequestKind::Image)?;
            // No HTML, SVG, missing Content-Type, or mismatched magic bytes can enter the WebView.
            let content_type = response
                .content_type
                .as_deref()
                .ok_or_else(|| ContentError::invalid("头图缺少图片类型"))?;
            let (bytes, _) = normalize_image(&response.body, Some(content_type))?;
            self.require_image(url.as_str())?;
            // The cache keeps the normalized copy, so cached reads skip decoding and resizing.
            self.cache.save_image(&scope, url.as_str(), &bytes);
            Ok(bytes)
        })
    }
}

#[cfg(any(target_os = "android", target_os = "windows", test))]
pub(crate) fn extract_release_tag_and_filename(url: &str) -> Option<(&str, &str)> {
    if let Some(idx) = url.find("releases/download/") {
        let rest = &url[idx + "releases/download/".len()..];
        let (tag, filename) = rest.split_once('/')?;
        let filename = filename.split(['?', '#']).next().unwrap_or(filename);
        if !tag.is_empty() && !filename.is_empty() {
            return Some((tag, filename));
        }
    }
    None
}

#[cfg(any(target_os = "android", target_os = "windows", test))]
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
        name.ends_with(".apk.gz")
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
        // Android 应用内更新仅下载 .apk.gz
        &[".apk.gz"]
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

/// 快照中的公告按当前版本过滤；磁盘缓存中的原始数据保持不变。
fn load_announcements_feed(
    cache: &Cache,
    scope: &str,
    now: u64,
    current_version: &str,
) -> FeedSnapshot<Announcements> {
    let mut feed = load_feed(cache, scope, "announcements.json", now);
    feed.data = feed
        .data
        .map(|data| applicable_announcements(data, current_version));
    feed
}

/// 公告的缓存 scope 与更新时间无关：公告不区分频道。
fn github_announcement_scope(repository: &Repository) -> String {
    format!("github:{}", repository.full_name)
}

/// 更新缓存按频道隔离：切换频道不会读到另一条通道的版本。
fn github_update_scope(repository: &Repository, channel: UpdateChannel) -> String {
    format!("github:{}:{}", repository.full_name, channel.slug())
}

/// 稳定版只认非草稿、非预发布且不是滚动 canary 的 release；测试版只认 canary。
fn release_matches_channel(release: &GithubRelease, channel: UpdateChannel) -> bool {
    if channel.is_stable() {
        !release.draft && !release.prerelease && release.tag_name != "canary"
    } else {
        release.tag_name == "canary"
    }
}

fn cnb_release_matches_channel(release: &CnbRelease, channel: UpdateChannel) -> bool {
    if channel.is_stable() {
        !release.prerelease && release.tag_name() != "canary"
    } else {
        release.tag_name() == "canary"
    }
}

fn apply_update(
    snapshot: &mut FeedSnapshot<UpdateInfo>,
    result: &Result<UpdateOutcome, ContentError>,
    completed_at: u64,
) {
    match result {
        Ok(outcome) => {
            snapshot.data = Some(outcome.data.clone());
            snapshot.last_success_at = Some(completed_at);
            snapshot.source = Some(outcome.source);
            snapshot.from_cache = false;
            snapshot.error = None;
        }
        Err(error) => {
            snapshot.error = Some(error.clone());
            snapshot.from_cache = snapshot.data.is_some();
        }
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
