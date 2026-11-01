mod client;
pub(crate) mod commands;
pub(crate) mod db;
pub(crate) mod download;
pub(crate) mod mcp;
pub(crate) mod model;
pub(crate) mod stream;
#[cfg(test)]
mod tests;
use crate::storage::SharedCredentialStore;
use client::Client;
use model::*;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock, Weak},
};
use tauri::{Emitter, Manager};

/// 重验证间隔：学期/课程 24 小时（选课阶段与跨学期时会变，一天足够新鲜）。
const REVALIDATE_SEMESTERS_MS: i64 = 24 * 60 * 60 * 1000;
const REVALIDATE_COURSES_MS: i64 = 24 * 60 * 60 * 1000;
/// 课次 30 分钟（对应 45min 课程时长，直播/回放状态要及时）。
const REVALIDATE_LESSONS_MS: i64 = 30 * 60 * 1000;
/// streams 每次打开播放页都后台重拉（直播状态易变，但缓存先行）。
const REVALIDATE_STREAMS_MS: i64 = 0;
/// 引导失败后的冷却时长：平台对账号有最大登录数/登录频次限制
/// （实测超限报「feign接口失败：当前超过最大登录数」），每次引导都会消耗
/// 一张 CAS 票据并触发一次平台登录，冷却期内直接复用上次结果。
const BOOTSTRAP_COOLDOWN_MS: u128 = 60_000;

pub(crate) struct JxxkStore {
    credentials: SharedCredentialStore,
    service: OnceLock<Arc<Service>>,
}
impl JxxkStore {
    pub fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials,
            service: OnceLock::new(),
        }
    }
}

/// 可缓存的列表数据。
#[derive(Clone)]
enum CachedList {
    Courses(CoursePage),
    Lessons(LessonPage),
    Streams(LessonStreams),
}

struct Runtime {
    session: SessionState,
    message: String,
    semesters: Option<(i64, Vec<Semester>)>,
    lists: HashMap<String, (i64, CachedList)>,
}

pub(crate) struct Service {
    app: Option<tauri::AppHandle>,
    credentials: SharedCredentialStore,
    runtime: Mutex<Runtime>,
    /// 串行化会话建立，避免并发命令重复引导。
    gate: tokio::sync::Mutex<()>,
    /// 复用的 HTTP 客户端（含 cookie jar 与 token）。
    client: Mutex<Option<Arc<Client>>>,
    /// 最近一次引导失败的时间与结果（冷却期内复用，见 BOOTSTRAP_COOLDOWN_MS）。
    last_bootstrap_failure: Mutex<Option<(std::time::Instant, BootstrapFailure)>>,
    /// 录像下载任务注册表。
    pub(crate) downloads: Arc<download::DownloadRegistry>,
    /// 本地 SQLite 缓存（jxxk/state.sqlite）。
    db: db::JxxkDatabase,
    /// 后台重验证去重锁。
    revalidating: Mutex<HashSet<String>>,
    /// 自引用，用于派生后台任务。
    self_weak: Mutex<Weak<Service>>,
}

/// 条目键 → 重验证间隔。
fn revalidate_interval(key: &str) -> i64 {
    if key == "semesters" {
        return REVALIDATE_SEMESTERS_MS;
    }
    if key.starts_with("courses:") {
        return REVALIDATE_COURSES_MS;
    }
    if key.starts_with("lessons:") {
        return REVALIDATE_LESSONS_MS;
    }
    REVALIDATE_STREAMS_MS
}

/// 解析 SQLite 缓存条目进 Runtime（损坏条目跳过）。
fn seed_cache_entry(runtime: &mut Runtime, id: &str, payload_json: &str, updated_at_ms: i64) {
    if id == "semesters" {
        if let Ok(semesters) = serde_json::from_str::<Vec<Semester>>(payload_json) {
            runtime.semesters = Some((updated_at_ms, semesters));
        }
        return;
    }
    // Runtime 键必须与读写路径一致（带前缀的完整 id），否则启动后缓存永不命中。
    let cached = if id.starts_with("courses:") {
        serde_json::from_str::<CoursePage>(payload_json)
            .ok()
            .map(|page| (id.to_string(), CachedList::Courses(page)))
    } else if id.starts_with("lessons:") {
        serde_json::from_str::<LessonPage>(payload_json)
            .ok()
            .map(|page| (id.to_string(), CachedList::Lessons(page)))
    } else if id.starts_with("streams:") {
        serde_json::from_str::<LessonStreams>(payload_json)
            .ok()
            .map(|info| (id.to_string(), CachedList::Streams(info)))
    } else {
        None
    };
    if let Some((key, value)) = cached {
        runtime.lists.insert(key, (updated_at_ms, value));
    }
}

impl Service {
    fn new(
        app: Option<tauri::AppHandle>,
        credentials: SharedCredentialStore,
        db_path: std::path::PathBuf,
    ) -> Result<Arc<Self>, String> {
        let (db, entries) = crate::migrations::db::run_sync(async {
            let database = db::JxxkDatabase::open(&db_path).await?;
            let entries = database.load_all().await?;
            Ok::<_, String>((database, entries))
        })
        .map_err(|e| format!("无法读取巡课本地缓存: {e}"))?;

        let mut runtime = Runtime {
            session: SessionState::NeedsAuth,
            message: String::new(),
            semesters: None,
            lists: HashMap::new(),
        };
        let mut has_cache = false;
        let mut task_records = Vec::new();
        for entry in &entries {
            // 任务记录（task:<uuid>）单独恢复，不进数据缓存。
            if let Some(task_id) = entry.id.strip_prefix("task:") {
                has_cache = true;
                if let Ok(record) =
                    serde_json::from_str::<download::PersistedTask>(&entry.payload_json)
                {
                    task_records.push((task_id.to_string(), record));
                }
                continue;
            }
            has_cache = true;
            seed_cache_entry(
                &mut runtime,
                &entry.id,
                &entry.payload_json,
                entry.updated_at_ms,
            );
        }
        if has_cache {
            // 有本地缓存：乐观渲染缓存数据，会话稍后在后台建立。
            runtime.session = SessionState::Ready;
        }
        let service = Arc::new(Self {
            app: app.clone(),
            credentials,
            runtime: Mutex::new(runtime),
            gate: tokio::sync::Mutex::new(()),
            client: Mutex::new(None),
            last_bootstrap_failure: Mutex::new(None),
            downloads: Arc::new(download::DownloadRegistry::default()),
            db,
            revalidating: Mutex::new(HashSet::new()),
            self_weak: Mutex::new(Weak::new()),
        });

        // 恢复上次会话的下载任务记录：已完成照原样；中断的标记为待续传。
        for (id, record) in task_records {
            let needs_respawn = !matches!(
                record.status,
                download::TaskStatus::Completed | download::TaskStatus::Failed
            );
            let status = if needs_respawn {
                download::TaskStatus::Paused
            } else {
                record.status
            };
            let core = download::task_core_from_record(
                app.clone(),
                record,
                status,
                needs_respawn,
                Arc::downgrade(&service.downloads),
            );
            let _ = id;
            service.downloads.register(core);
        }
        Ok(service)
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let r = self.runtime.lock().unwrap();
        Snapshot {
            session: r.session,
            message: r.message.clone(),
            semesters: r
                .semesters
                .as_ref()
                .map(|(_, semesters)| semesters.clone())
                .unwrap_or_default(),
            fetched_at: r.semesters.as_ref().map(|(at, _)| *at),
        }
    }

    fn update(&self, session: SessionState, message: &str) {
        let mut r = self.runtime.lock().unwrap();
        r.session = session;
        r.message = message.to_owned();
    }

    /// 取或构建 HTTP 客户端。vault 中保存的巡课 cookie 会被恢复；
    /// 统一会话（authserver SSO）不存在时返回 None 并把状态标记为 NeedsAuth。
    pub(crate) async fn client(&self) -> Option<Arc<Client>> {
        if let Some(client) = self.client.lock().unwrap().clone() {
            return Some(client);
        }
        let cookies = self
            .credentials
            .load_jxxk_session()
            .map_err(|error| log::warn!("读取巡课会话失败: {error}"))
            .ok()
            .flatten();
        let sso_cookies = match &self.app {
            Some(app) => crate::auth::reusable_sso_cookies(app)
                .map_err(|message| {
                    log::warn!("[jxxk] 统一会话不可用: {message}");
                    self.update(SessionState::NeedsAuth, &message);
                })
                .map(|cookies| {
                    log::info!("[jxxk] 注入统一认证 cookie {} 条", cookies.len());
                    cookies
                })
                .ok()?,
            None => Vec::new(),
        };
        let client = Client::new(
            client::JXXK_ORIGIN,
            client::AUTH_LOGIN,
            cookies.as_deref(),
            &sso_cookies,
        )
        .map_err(|message| self.update(SessionState::Unavailable, &message))
        .ok()?;
        let client = Arc::new(client);
        *self.client.lock().unwrap() = Some(client.clone());
        Some(client)
    }

    /// 丢弃当前客户端（会话彻底失效时调用，下次重新构建）。
    fn drop_client(&self) {
        *self.client.lock().unwrap() = None;
    }

    /// 确保会话可用：无 token 时先刷新 token、再走 CAS 引导。
    /// `force` 为用户显式刷新时绕过失败冷却与到期检测。
    async fn establish(&self, client: &Client, force: bool) -> Result<(), BootstrapFailure> {
        if client.has_token() {
            if !force && !client.token_needs_refresh() {
                return Ok(());
            }
            log::info!("[jxxk] establish: token 需续期或强制刷新，尝试 refreshToken 续期");
            if client.refresh_token().await.is_ok() {
                // 轮换后的 JWT 必须随会话落盘，否则重启后 vault 里还是旧 token。
                if let Err(error) = self.persist(client) {
                    log::warn!("保存巡课会话失败: {error}");
                }
                return Ok(());
            }
            log::warn!("[jxxk] establish: refreshToken 失败，转投 CAS 重建会话");
        } else {
            log::info!("[jxxk] establish: 无 token，直接走 CAS 引导");
        }
        if !force {
            let recent = self
                .last_bootstrap_failure
                .lock()
                .unwrap()
                .as_ref()
                .map(|(at, _)| at.elapsed().as_millis() < BOOTSTRAP_COOLDOWN_MS)
                .unwrap_or(false);
            if recent {
                let (_, failure) = self.last_bootstrap_failure.lock().unwrap().clone().unwrap();
                log::info!("[jxxk] establish: 距上次引导失败不足冷却期，直接复用结果");
                return Err(failure);
            }
        }
        match client.bootstrap().await {
            Ok(client::Bootstrap::Established) => {
                *self.last_bootstrap_failure.lock().unwrap() = None;
                if let Err(error) = self.persist(client) {
                    log::warn!("保存巡课会话失败: {error}");
                }
                Ok(())
            }
            Ok(client::Bootstrap::NeedsAuth) => {
                let failure =
                    BootstrapFailure::NeedsAuth("请先完成应用统一认证，再使用监察巡课".into());
                *self.last_bootstrap_failure.lock().unwrap() =
                    Some((std::time::Instant::now(), failure.clone()));
                Err(failure)
            }
            Err(message) => {
                let failure = BootstrapFailure::Error(message);
                *self.last_bootstrap_failure.lock().unwrap() =
                    Some((std::time::Instant::now(), failure.clone()));
                Err(failure)
            }
        }
    }

    /// 确保数据可用：有本地缓存时立即返回（仅派发过期条目的后台重验证），
    /// 缓存完全不存在时阻塞建立会话并首拉学期列表。`force` 走阻塞式全量刷新。
    pub(crate) async fn ensure(&self, force: bool) {
        let has_cache = {
            let r = self.runtime.lock().unwrap();
            r.semesters.is_some() || !r.lists.is_empty()
        };
        if has_cache {
            if force {
                let _ = self.revalidate_all().await;
            } else {
                self.spawn_stale_revalidations();
            }
            return;
        }
        let _guard = self.gate.lock().await;
        let Some(client) = self.client().await else {
            return;
        };
        match self.establish(&client, force).await {
            Ok(()) => {}
            Err(BootstrapFailure::NeedsAuth(message)) => {
                self.drop_client();
                self.update(SessionState::NeedsAuth, &message);
                return;
            }
            Err(BootstrapFailure::Error(message)) => {
                self.update(SessionState::Unavailable, &message);
                return;
            }
        }
        if let Err(error) = self.fetch_semesters(&client).await {
            self.update(SessionState::Unavailable, &error);
        }
    }

    async fn fetch_semesters(&self, client: &Client) -> Result<(), String> {
        let semesters = client.get_semesters().await?;
        let now = chrono::Utc::now().timestamp_millis();
        self.persist_semesters(now, &semesters).await;
        let mut r = self.runtime.lock().unwrap();
        r.semesters = Some((now, semesters));
        if r.session != SessionState::Ready {
            r.session = SessionState::Ready;
            r.message = String::new();
        }
        Ok(())
    }

    /// 把学期列表写进 Runtime 与 SQLite，并广播缓存变化。
    async fn persist_semesters(&self, now: i64, semesters: &[Semester]) {
        let payload = serde_json::to_string(semesters).unwrap_or_else(|_| "[]".to_string());
        let db = self.db.clone();
        let payload_clone = payload.clone();
        // 落盘失败只影响下次启动的缓存新鲜度，不打断当前请求。
        tauri::async_runtime::spawn(async move {
            if let Err(error) = db
                .save(&db::CacheEntry {
                    id: "semesters".into(),
                    payload_json: payload_clone,
                    updated_at_ms: now,
                })
                .await
            {
                log::warn!("[jxxk] 缓存学期列表失败: {error}");
            }
        });
    }

    /// 把网络拉取的列表写进 Runtime 与 SQLite。
    async fn store_list(&self, key: &str, value: CachedList) {
        let now = chrono::Utc::now().timestamp_millis();
        let payload = match &value {
            CachedList::Courses(page) => serde_json::to_string(page),
            CachedList::Lessons(page) => serde_json::to_string(page),
            CachedList::Streams(info) => serde_json::to_string(info),
        }
        .unwrap_or_else(|_| "null".to_string());
        {
            let mut r = self.runtime.lock().unwrap();
            r.lists.insert(key.to_owned(), (now, value));
            if r.session != SessionState::Ready {
                r.session = SessionState::Ready;
                r.message = String::new();
            }
        }
        let db = self.db.clone();
        let key = key.to_owned();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = db
                .save(&db::CacheEntry {
                    id: key,
                    payload_json: payload,
                    updated_at_ms: now,
                })
                .await
            {
                log::warn!("[jxxk] 缓存列表失败: {error}");
            }
        });
    }

    /// 缓存命中立即返回（无论多旧）；过期条目派发后台重验证。
    async fn cached_list<T>(
        &self,
        key: &str,
        fetch: impl Future<Output = Result<CachedList, String>>,
    ) -> Result<T, String>
    where
        T: TryFrom<CachedList, Error = String> + Clone,
    {
        let hit = {
            let r = self.runtime.lock().unwrap();
            r.lists
                .get(key)
                .map(|(at, value)| (at.to_owned(), value.clone()))
        };
        if let Some((updated_at, value)) = hit {
            self.maybe_revalidate(key, updated_at);
            return T::try_from(value);
        }
        let value = fetch.await?;
        let extracted = T::try_from(value.clone())?;
        self.store_list(key, value).await;
        Ok(extracted)
    }

    /// 过期条目派发后台重验证（去重锁 + Weak 自引用，不阻塞调用方）。
    fn maybe_revalidate(&self, key: &str, updated_at: i64) {
        if self.app.is_none() {
            return;
        }
        if chrono::Utc::now().timestamp_millis() - updated_at <= revalidate_interval(key) {
            return;
        }
        if !self.revalidating.lock().unwrap().insert(key.to_owned()) {
            return;
        }
        let Some(this) = self.self_weak.lock().unwrap().upgrade() else {
            self.revalidating.lock().unwrap().remove(key);
            return;
        };
        let key = key.to_owned();
        tauri::async_runtime::spawn(async move {
            let result = this.revalidate_entry(&key).await;
            this.revalidating.lock().unwrap().remove(&key);
            match result {
                Ok(()) => {
                    log::info!("[jxxk] 缓存重验证完成: {key}");
                    if let Some(app) = &this.app {
                        let _ = app.emit("jxxk-cache-changed", key.clone());
                    }
                }
                Err(error) => {
                    log::warn!("[jxxk] 缓存重验证失败 {key}: {error}");
                }
            }
        });
    }

    /// 重验证单个缓存条目（走网络并写回 Runtime + SQLite）。
    async fn revalidate_entry(&self, key: &str) -> Result<(), String> {
        if key == "semesters" {
            let client = self.client().await.ok_or_else(|| self.session_message())?;
            self.establish(&client, false).await.map_err(|f| match f {
                BootstrapFailure::NeedsAuth(m) | BootstrapFailure::Error(m) => m,
            })?;
            return self.fetch_semesters(&client).await;
        }
        if let Some(sem) = key.strip_prefix("courses:") {
            let semester = sem.to_string();
            let page = self.network_courses(&semester, "", 1, 200).await?;
            self.store_list(key, CachedList::Courses(page)).await;
            return Ok(());
        }
        if let Some(cid) = key.strip_prefix("lessons:") {
            let course_id = cid.to_string();
            let page = self.network_lessons(&course_id, 1, 200).await?;
            self.store_list(key, CachedList::Lessons(page)).await;
            return Ok(());
        }
        if let Some(lid) = key.strip_prefix("streams:") {
            let live_id = lid.to_string();
            let info = self.network_streams(&live_id).await?;
            self.store_list(key, CachedList::Streams(info)).await;
            return Ok(());
        }
        Err("缓存键无效".into())
    }

    /// 对全部缓存条目做阻塞式重验证（刷新按钮路径）。
    async fn revalidate_all(&self) {
        let keys: Vec<String> = {
            let r = self.runtime.lock().unwrap();
            let mut keys = vec!["semesters".to_string()];
            keys.extend(r.lists.keys().cloned());
            keys
        };
        for key in keys {
            if let Err(error) = self.revalidate_entry(&key).await {
                log::warn!("[jxxk] 强制刷新条目失败 {key}: {error}");
            }
        }
        if let Some(app) = &self.app {
            let _ = app.emit("jxxk-cache-changed", "all");
        }
    }

    /// 对全部过期条目派发后台重验证（不阻塞）。
    fn spawn_stale_revalidations(&self) {
        let entries: Vec<(String, i64)> = {
            let r = self.runtime.lock().unwrap();
            let mut entries = Vec::new();
            if let Some((at, _)) = &r.semesters {
                entries.push(("semesters".to_string(), *at));
            }
            for (key, (at, _)) in &r.lists {
                entries.push((key.clone(), *at));
            }
            entries
        };
        for (key, updated_at) in entries {
            self.maybe_revalidate(&key, updated_at);
        }
    }

    /// 带会话建立的列表拉取：先确保会话可用，失败时把状态落盘并返回错误。
    async fn with_session_retry<T, F>(&self, fetch: F) -> Result<T, String>
    where
        F: for<'a> FnOnce(
            &'a Client,
        ) -> Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>,
    {
        let Some(client) = self.client().await else {
            return Err(self.session_message());
        };
        match self.establish(&client, false).await {
            Ok(()) => {}
            Err(BootstrapFailure::NeedsAuth(message)) => {
                self.drop_client();
                self.update(SessionState::NeedsAuth, &message);
                return Err(message);
            }
            Err(BootstrapFailure::Error(message)) => {
                self.update(SessionState::Unavailable, &message);
                return Err(message);
            }
        }
        fetch(&client).await
    }

    fn session_message(&self) -> String {
        let r = self.runtime.lock().unwrap();
        if r.message.is_empty() {
            "巡课会话不可用".into()
        } else {
            r.message.clone()
        }
    }

    /// 课程列表网络拉取（前台缓存未命中与后台重验证共用）。
    async fn network_courses(
        &self,
        semester: &str,
        search: &str,
        page_index: u32,
        page_size: u32,
    ) -> Result<CoursePage, String> {
        let semester = semester.to_string();
        let search = search.to_string();
        self.with_session_retry(move |client| {
            Box::pin(async move {
                let query = [
                    ("yearSemesterId", semester.clone()),
                    ("categorySectionId", String::new()),
                    ("courseTypeId", String::new()),
                    ("eduLevelId", String::new()),
                    ("path", String::new()),
                    ("searchKey", search.clone()),
                    ("order", "7".to_string()),
                    ("courseStatus", String::new()),
                    ("pageIndex", page_index.to_string()),
                    ("pageSize", page_size.to_string()),
                    ("ifOrdinaryEquipment", "false".to_string()),
                ];
                let value = client
                    .get_json("university-live/protect/course/page", &query)
                    .await
                    .map_err(|e| e.message)?;
                parse_course_page(&value)
            })
        })
        .await
    }

    /// 课次列表网络拉取。
    async fn network_lessons(
        &self,
        course_id: &str,
        page_index: u32,
        page_size: u32,
    ) -> Result<LessonPage, String> {
        let course_id = course_id.to_string();
        self.with_session_retry(move |client| {
            Box::pin(async move {
                let query = [
                    ("courseId", course_id.clone()),
                    ("searchKey", String::new()),
                    ("liveState", String::new()),
                    ("aiLabel", String::new()),
                    ("liveStartTimeGe", String::new()),
                    ("liveStartTimeLe", String::new()),
                    ("videoCreateTimeGe", String::new()),
                    ("videoCreateTimeLe", String::new()),
                    ("pageCount", "0".to_string()),
                    ("pageIndex", page_index.to_string()),
                    ("pageSize", page_size.to_string()),
                    ("total", "0".to_string()),
                    ("status", "1".to_string()),
                    ("verifyStatus", "1".to_string()),
                    ("videoPublicView", "true".to_string()),
                ];
                let value = client
                    .get_json("university-live/protect/courseResource/page", &query)
                    .await
                    .map_err(|e| e.message)?;
                parse_lesson_page(&value)
            })
        })
        .await
    }

    /// 多角度视频信息网络拉取。
    async fn network_streams(&self, live_id: &str) -> Result<LessonStreams, String> {
        let live_id = live_id.to_string();
        self.with_session_retry(move |client| {
            Box::pin(async move {
                let value = client
                    .get_json(
                        "university-live/protect/video/page",
                        &[
                            ("liveId", live_id.clone()),
                            ("publicView", "true".to_string()),
                        ],
                    )
                    .await
                    .map_err(|e| e.message)?;
                parse_lesson_streams(&value)
            })
        })
        .await
    }

    /// 拉取课程列表（乐观缓存：命中即回，过期后台重验证）。
    pub(crate) async fn list_courses(
        &self,
        year_semester_id: Option<&str>,
        search_key: Option<&str>,
        page_index: u32,
        page_size: u32,
    ) -> Result<CoursePage, String> {
        // 仅默认列表（首页、无搜索）走缓存；搜索/翻页直接走网络。
        let semester = year_semester_id.unwrap_or("").to_string();
        let search = search_key.unwrap_or("").to_string();
        // 仅默认列表（首页、无搜索）走缓存；搜索/翻页直接走网络。
        let cached_key = (search.is_empty() && page_index == 1 && page_size == 200)
            .then(|| "courses:".to_string() + &semester);
        match cached_key {
            Some(key) => {
                self.cached_list(&key, async {
                    self.network_courses(&semester, &search, page_index, page_size)
                        .await
                        .map(CachedList::Courses)
                })
                .await
            }
            None => {
                self.network_courses(&semester, &search, page_index, page_size)
                    .await
            }
        }
    }

    /// 拉取课次列表（乐观缓存：命中即回，过期后台重验证）。
    pub(crate) async fn list_lessons(
        &self,
        course_id: &str,
        page_index: u32,
        page_size: u32,
    ) -> Result<LessonPage, String> {
        // 仅首页全量（pageSize 200）走缓存；其它分页直接走网络。
        let cached_key =
            (page_index == 1 && page_size == 200).then(|| format!("lessons:{course_id}"));
        match cached_key {
            Some(key) => {
                self.cached_list(&key, async {
                    self.network_lessons(course_id, page_index, page_size)
                        .await
                        .map(CachedList::Lessons)
                })
                .await
            }
            None => self.network_lessons(course_id, page_index, page_size).await,
        }
    }

    /// 拉取一节课的多角度视频信息（乐观缓存）。
    pub(crate) async fn lesson_streams(&self, live_id: &str) -> Result<LessonStreams, String> {
        let key = format!("streams:{live_id}");
        self.cached_list(&key, async {
            self.network_streams(live_id).await.map(CachedList::Streams)
        })
        .await
    }

    /// 获取一节课各画面的 HLS 播放地址（带签名时效，不缓存）。
    pub(crate) async fn play_urls(&self, live_id: &str) -> Result<Vec<PlayUrlScreen>, String> {
        let streams = self.lesson_streams(live_id).await?;
        self.with_session_retry(|client| {
            let media_video_id = streams.media_video_id.clone();
            Box::pin(async move {
                let value = client
                    .get_json(
                        "university-live/protect/video/getVideoPlayUrl",
                        &[("mediaVideoId", media_video_id)],
                    )
                    .await
                    .map_err(|e| e.message)?;
                parse_play_urls(&value)
            })
        })
        .await
    }

    /// 获取各画面的明文 mp4 下载直链。
    pub(crate) async fn download_urls(
        &self,
        media_video_id: &str,
    ) -> Result<Vec<DownloadUrl>, String> {
        let media_video_id = media_video_id.to_string();
        self.with_session_retry(move |client| {
            Box::pin(async move {
                let value = client
                    .get_json(
                        "university-live/protect/video/getFileDownloadUrl",
                        &[("mediaVideoId", media_video_id)],
                    )
                    .await
                    .map_err(|e| e.message)?;
                parse_download_urls(&value)
            })
        })
        .await
    }

    /// 获取直播流地址（结构未完全实测，best-effort 提取）。
    pub(crate) async fn live_stream(&self, live_id: &str) -> Result<Vec<LiveStreamSource>, String> {
        let live_id = live_id.to_string();
        self.with_session_retry(move |client| {
            Box::pin(async move {
                let value = client
                    .get_json(
                        "university-live/protect/live/getLiveStream",
                        &[("id", live_id)],
                    )
                    .await
                    .map_err(|e| e.message)?;
                Ok(client::parse_live_streams(&value))
            })
        })
        .await
    }

    fn persist(&self, client: &Client) -> Result<(), String> {
        let cookies = client.cookies()?;
        self.credentials.save_jxxk_session(&cookies)
    }
}

#[derive(Debug, Clone)]
enum BootstrapFailure {
    NeedsAuth(String),
    Error(String),
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<JxxkStore>();
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|_| "应用数据目录不可用".to_string())?
        .join("jxxk/state.sqlite");
    let service = Service::new(Some(app.clone()), store.credentials.clone(), db_path)?;
    *service.self_weak.lock().unwrap() = Arc::downgrade(&service);
    store
        .service
        .set(service)
        .map_err(|_| "巡课服务已初始化".to_string())
}

pub(crate) fn service(app: &tauri::AppHandle) -> Result<Arc<Service>, String> {
    app.state::<JxxkStore>()
        .service
        .get()
        .cloned()
        .ok_or_else(|| "巡课服务尚未初始化".into())
}

/// 供流代理使用：取客户端，必要时构建（不引导 CAS，签名 URL 本身可访问）。
pub(crate) async fn client_for_stream(app: &tauri::AppHandle) -> Result<Arc<Client>, String> {
    let service = service(app)?;
    service
        .client()
        .await
        .ok_or_else(|| "巡课会话不可用".to_string())
}

impl TryFrom<CachedList> for CoursePage {
    type Error = String;
    fn try_from(value: CachedList) -> Result<Self, String> {
        match value {
            CachedList::Courses(value) => Ok(value),
            _ => Err("缓存类型不匹配".into()),
        }
    }
}

impl TryFrom<CachedList> for LessonPage {
    type Error = String;
    fn try_from(value: CachedList) -> Result<Self, String> {
        match value {
            CachedList::Lessons(value) => Ok(value),
            _ => Err("缓存类型不匹配".into()),
        }
    }
}

impl TryFrom<CachedList> for LessonStreams {
    type Error = String;
    fn try_from(value: CachedList) -> Result<Self, String> {
        match value {
            CachedList::Streams(value) => Ok(value),
            _ => Err("缓存类型不匹配".into()),
        }
    }
}

// ---- 响应解析（此处做字段投影，测试直接喂 JSON）----

fn parse_course_page(value: &serde_json::Value) -> Result<CoursePage, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        #[serde(default)]
        page_index: i64,
        #[serde(default)]
        page_size: i64,
        #[serde(default)]
        record_count: i64,
        #[serde(default)]
        content: Vec<Course>,
    }
    let page: Raw =
        serde_json::from_value(value.clone()).map_err(|_| "课程列表响应格式不兼容".to_string())?;
    Ok(CoursePage {
        page_index: page.page_index,
        page_size: page.page_size,
        record_count: page.record_count,
        content: page.content,
    })
}

fn parse_lesson_page(value: &serde_json::Value) -> Result<LessonPage, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawItem {
        #[serde(default)]
        id: String,
        #[serde(default)]
        target_name: String,
        #[serde(default, rename = "liveDTO")]
        live_dto: serde_json::Value,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        #[serde(default)]
        page_index: i64,
        #[serde(default)]
        page_size: i64,
        #[serde(default)]
        record_count: i64,
        #[serde(default)]
        content: Vec<RawItem>,
    }
    let page: Raw =
        serde_json::from_value(value.clone()).map_err(|_| "课次列表响应格式不兼容".to_string())?;
    let content = page
        .content
        .into_iter()
        .map(|item| {
            let live = &item.live_dto;
            let classroom = live
                .get("liveDTORecordEquipments")
                .and_then(|v| v.as_array())
                .and_then(|items| items.first())
                .and_then(|item| {
                    item.get("recordEquipmentThirdPartyNo")
                        .or_else(|| item.get("recordEquipmentName"))
                        .and_then(|v| v.as_str())
                })
                .map(str::to_owned);
            Lesson {
                id: item.id,
                target_name: item.target_name,
                live_id: live.get("id").and_then(|v| v.as_str()).map(str::to_owned),
                live_state: live.get("liveState").and_then(|v| v.as_i64()),
                live_state_text: live
                    .get("liveStateText")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                start_time: live
                    .get("startTime")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                end_time: live
                    .get("endTime")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                classroom,
                media_video_id: client::extract_media_video_id(live),
            }
        })
        .collect();
    Ok(LessonPage {
        page_index: page.page_index,
        page_size: page.page_size,
        record_count: page.record_count,
        content,
    })
}

fn parse_lesson_streams(value: &serde_json::Value) -> Result<LessonStreams, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        #[serde(default)]
        content: Vec<serde_json::Value>,
    }
    let page: Raw =
        serde_json::from_value(value.clone()).map_err(|_| "视频列表响应格式不兼容".to_string())?;
    let video = page
        .content
        .first()
        .ok_or_else(|| "该课次还没有可用的课堂录像".to_string())?;
    let streams: Vec<VideoStream> = video
        .get("streams")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let string_field = |name: &str| video.get(name).and_then(|v| v.as_str()).map(str::to_owned);
    Ok(LessonStreams {
        video_id: string_field("id").unwrap_or_default(),
        media_video_id: string_field("associateId").unwrap_or_default(),
        title: string_field("title").unwrap_or_default(),
        duration: video.get("duration").and_then(|v| v.as_i64()),
        download: video
            .get("download")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        record_equipment_name: string_field("recordEquipmentName"),
        streams,
    })
}

fn parse_play_urls(value: &serde_json::Value) -> Result<Vec<PlayUrlScreen>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawScreen {
        #[serde(default)]
        screen_tag: Option<String>,
        #[serde(default)]
        stream_name: Option<String>,
        #[serde(default)]
        stream_no: Option<String>,
        #[serde(default)]
        stream_label: Option<String>,
        #[serde(default)]
        stream_permission_type: Option<String>,
        #[serde(default)]
        streams: Vec<RawStream>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawStream {
        #[serde(default)]
        play_url: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        #[serde(default)]
        content: Vec<RawScreen>,
    }
    let page: Raw =
        serde_json::from_value(value.clone()).map_err(|_| "播放地址响应格式不兼容".to_string())?;
    let mut screens = Vec::new();
    for screen in page.content {
        let Some(play_url) = screen.streams.into_iter().find_map(|s| s.play_url) else {
            continue;
        };
        screens.push(PlayUrlScreen {
            screen_tag: screen.screen_tag,
            stream_name: screen.stream_name,
            stream_no: screen.stream_no,
            stream_label: screen.stream_label,
            stream_permission_type: screen.stream_permission_type,
            play_url,
        });
    }
    if screens.is_empty() {
        return Err("未获取到可用的播放地址".into());
    }
    Ok(screens)
}

fn parse_download_urls(value: &serde_json::Value) -> Result<Vec<DownloadUrl>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        #[serde(default)]
        stream_name: Option<String>,
        #[serde(default)]
        stream_no: Option<String>,
        #[serde(default)]
        stream_label: Option<String>,
        #[serde(default)]
        file_name: Option<String>,
        #[serde(default)]
        file_url: Option<String>,
    }
    // 响应可能是分页对象（content 数组）或单对象（videoStream 接口）。
    let raws: Vec<Raw> = if let Some(content) = value.get("content").and_then(|v| v.as_array()) {
        content
            .iter()
            .filter_map(|item| serde_json::from_value(item.clone()).ok())
            .collect()
    } else {
        match serde_json::from_value::<Raw>(value.clone()) {
            Ok(item) => vec![item],
            Err(_) => vec![],
        }
    };
    let urls: Vec<DownloadUrl> = raws
        .into_iter()
        .filter_map(|raw| {
            raw.file_url
                .filter(|url| !url.is_empty())
                .map(|file_url| DownloadUrl {
                    stream_name: raw.stream_name,
                    stream_no: raw.stream_no,
                    stream_label: raw.stream_label,
                    file_name: raw.file_name,
                    file_url,
                })
        })
        .collect();
    if urls.is_empty() {
        return Err("未获取到可用的下载地址".into());
    }
    Ok(urls)
}
