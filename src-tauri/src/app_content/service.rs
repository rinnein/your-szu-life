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
            next_automatic_check_at: config.sources.as_ref().map(|_| now),
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
        if self.config.sources.is_none() {
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
        let Some(sources) = &self.config.sources else {
            return self.snapshot();
        };
        self.refresh_flight.run(|| {
            let attempted_at = self.clock.now_ms();
            self.mutate(|snapshot| {
                snapshot.refreshing = true;
                snapshot.announcements.last_attempt_at = Some(attempted_at);
                snapshot.update.last_attempt_at = Some(attempted_at);
            });
            // Each document has its own fallback chain. No successful request is repeated.
            let (announcements, update) = std::thread::scope(|scope| {
                let announcements =
                    scope.spawn(|| self.fetch::<Announcements>(sources, "announcements.json"));
                let update = self.fetch::<UpdateInfo>(sources, "update.json");
                (
                    announcements.join().expect("content request worker"),
                    update,
                )
            });
            let completed_at = self.clock.now_ms();
            if let Ok((data, source)) = &announcements {
                self.cache.save(
                    &sources.key(),
                    "announcements.json",
                    data,
                    *source,
                    completed_at,
                );
            }
            if let Ok((data, source)) = &update {
                self.cache
                    .save(&sources.key(), "update.json", data, *source, completed_at);
            }
            self.mutate(|snapshot| {
                apply_result(&mut snapshot.announcements, announcements, completed_at);
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
            self.cache.prune_images(&sources.key(), &allowed);
            self.snapshot()
        })
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
            .ok_or_else(|| ContentError::invalid("未配置内容源"))?
            .key();
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
