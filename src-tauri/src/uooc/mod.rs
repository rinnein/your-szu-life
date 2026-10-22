mod ai;
mod checkin;
mod client;
pub(crate) mod commands;
mod discussion;
mod duration;
mod login;
pub(crate) mod mcp;
mod media;
pub(crate) mod model;
mod persistence;
mod platform;
mod progress;
pub(crate) mod quiz;
mod runner;
mod session;
mod user_agent;

use crate::storage::SharedCredentialStore;
use client::{Client, Error};
use duration::DurationCache;
use model::*;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, RwLock},
};
use tauri::{Emitter, Manager};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub(crate) struct UoocStore {
    credentials: SharedCredentialStore,
    service: OnceLock<Arc<Service>>,
}
impl UoocStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials,
            service: OnceLock::new(),
        }
    }
}
struct Runtime {
    snapshot: Snapshot,
    workers: HashMap<String, (u64, CancellationToken)>,
    epoch: u64,
    session_cancel: CancellationToken,
    login: Option<login::Attempt>,
    background_generation: u64,
    background_active: bool,
    checkins: checkin::Checkins,
    quiz_windows: HashMap<String, (u64, String)>,
    discussion: DiscussionStoreState,
    qa_api_key: Option<String>,
    initial_qa_bootstrapped_accounts: HashSet<String>,
}
pub(super) struct Service {
    app: Option<tauri::AppHandle>,
    credentials: SharedCredentialStore,
    path: PathBuf,
    runtime: Mutex<Runtime>,
    client: RwLock<Arc<Client>>,
    refresh_lock: tokio::sync::Mutex<()>,
    permits: RwLock<Arc<Semaphore>>,
    durations: DurationCache,
    progress_requests: progress::ProgressRequests,
    native_session: Arc<tokio::sync::Mutex<()>>,
    ai: Mutex<ai::Config>,
    ai_permits: Semaphore,
    discussion_lock: tokio::sync::Mutex<()>,
    pub(super) bank_building: Arc<tokio::sync::Mutex<()>>,
}
impl Service {
    fn new(
        app: Option<tauri::AppHandle>,
        credentials: SharedCredentialStore,
        root: PathBuf,
    ) -> Result<Arc<Self>, String> {
        let path = root.join("state.json");
        let saved = persistence::read::<SavedState>(&path);
        let mut message = saved.as_ref().err().cloned().unwrap_or_default();
        let mut saved = saved.ok().flatten().unwrap_or_default();
        let saved_settings = saved.settings.clone();
        quiz::restore_media(&mut saved.quizzes, &root);
        if saved.settings.validate().is_err() {
            saved.settings = Settings::default();
        }
        for task in &mut saved.tasks {
            task.generation = task.generation.saturating_add(1);
            if task.desired_running {
                task.status = TaskStatus::AwaitingLogin;
                task.message = "等待启动登录检查后恢复".into();
            }
        }
        let raw = match credentials.load_uooc_session() {
            Ok(raw) => raw,
            Err(_) => {
                message = "无法读取 UOOC 安全存储，请重新登录".into();
                None
            }
        };
        let jar = match session::load(raw.as_deref()) {
            Ok(jar) => jar,
            Err(error) => {
                message = error;
                session::load(None)?
            }
        };
        let ai = credentials
            .load_uooc_ai()
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();
        Ok(Arc::new(Self {
            app,
            credentials,
            path,
            durations: DurationCache::new(root.join("video-durations.json")),
            progress_requests: progress::ProgressRequests::default(),
            native_session: Arc::new(tokio::sync::Mutex::new(())),
            ai: Mutex::new(ai),
            ai_permits: Semaphore::new(2),
            discussion_lock: tokio::sync::Mutex::new(()),
            bank_building: Arc::new(tokio::sync::Mutex::new(())),
            permits: RwLock::new(Arc::new(Semaphore::new(saved.settings.concurrency))),
            client: RwLock::new(Arc::new(Client::new(jar)?)),
            refresh_lock: tokio::sync::Mutex::new(()),
            runtime: Mutex::new(Runtime {
                snapshot: Snapshot {
                    settings: saved_settings.clone(),
                    tasks: saved.tasks,
                    quizzes: saved.quizzes,
                    discussions: saved.discussion.courses.clone(),
                    discussion_cooldown_until_ms: saved
                        .discussion
                        .last_sent_at_ms
                        .and_then(|at| discussion::cooldown_until(Some(at), &saved_settings)),
                    message,
                    ..Snapshot::default()
                },
                workers: HashMap::new(),
                epoch: 0,
                session_cancel: CancellationToken::new(),
                login: None,
                background_generation: 0,
                background_active: false,
                checkins: checkin::Checkins::default(),
                quiz_windows: HashMap::new(),
                discussion: saved.discussion,
                qa_api_key: saved.qa_api_key,
                initial_qa_bootstrapped_accounts: saved.initial_qa_bootstrapped_accounts,
            }),
        }))
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        Self::visible_snapshot(&self.runtime.lock().unwrap())
    }
    fn visible_snapshot(runtime: &Runtime) -> Snapshot {
        let mut snapshot = runtime.snapshot.clone();
        let account = snapshot.user.as_ref().map(|u| u.id.as_str());
        snapshot
            .quizzes
            .retain(|_, q| Some(q.account_id.as_str()) == account);
        snapshot.discussions = runtime.discussion.courses.clone();
        snapshot.discussion_cooldown_until_ms = runtime
            .discussion
            .last_sent_at_ms
            .and_then(|at| discussion::cooldown_until(Some(at), &runtime.snapshot.settings));
        snapshot
    }
    fn publish(&self, runtime: &mut Runtime) {
        runtime.snapshot.revision = runtime.snapshot.revision.saturating_add(1);
        if runtime.snapshot.logs.len() > 200 {
            runtime
                .snapshot
                .logs
                .drain(..runtime.snapshot.logs.len() - 200);
        }
        let saved =
            quiz::saved(&runtime.snapshot.quizzes, self.path.parent().unwrap()).map(|quizzes| {
                SavedState {
                    settings: runtime.snapshot.settings.clone(),
                    tasks: runtime.snapshot.tasks.clone(),
                    quizzes,
                    discussion: runtime.discussion.clone(),
                    qa_api_key: runtime.qa_api_key.clone(),
                    initial_qa_bootstrapped_accounts: runtime.initial_qa_bootstrapped_accounts.clone(),
                }
            });
        if let Err(error) = saved.and_then(|saved| persistence::write(&self.path, &saved)) {
            runtime.snapshot.message = error;
        }
        if let Some(app) = &self.app {
            let stale: Vec<_> = runtime
                .quiz_windows
                .iter()
                .filter(|(_, (epoch, _))| {
                    *epoch != runtime.epoch || runtime.snapshot.session != SessionStatus::Ready
                })
                .map(|(label, _)| label.clone())
                .collect();
            for label in stale {
                runtime.quiz_windows.remove(&label);
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(w) = app.get_webview_window(&label) {
                        let _ = w.destroy();
                    }
                });
            }
            let _ = app.emit("uooc-state-changed", Self::visible_snapshot(runtime));
        }
    }
    pub(crate) fn emit_bank_progress(&self, progress: &model::BankRebuildProgress) {
        if let Some(app) = &self.app {
            let _ = app.emit("uooc-bank-rebuild-progress", progress);
        }
    }
    pub(crate) fn emit_ai_session(&self, entry: &quiz::model::AiSessionEntry) {
        if let Some(app) = &self.app {
            let _ = app.emit("uooc-ai-session-progress", entry);
        }
    }
    fn change(&self, f: impl FnOnce(&mut Snapshot)) {
        let mut runtime = self.runtime.lock().unwrap();
        f(&mut runtime.snapshot);
        self.publish(&mut runtime);
    }
    fn client(&self) -> Arc<Client> {
        self.client.read().unwrap().clone()
    }
    fn save_session(&self, client: &Client) -> Result<(), String> {
        let raw = session::serialize(&client.jar)?;
        self.credentials
            .save_uooc_session(&raw)
            .map_err(|_| "UOOC Cookie 无法写入系统密钥存储".into())
    }
    fn freeze(runtime: &mut Runtime, status: TaskStatus, message: &str) {
        for (_, cancel) in runtime.workers.values() {
            cancel.cancel();
        }
        runtime.workers.clear();
        for task in runtime
            .snapshot
            .tasks
            .iter_mut()
            .filter(|t| t.desired_running)
        {
            task.generation += 1;
            task.status = status.clone();
            task.message = message.into();
            for video in task.videos.iter_mut().filter(|v| v.status.active()) {
                video.status = status.clone();
            }
        }
    }
    fn expire_session(runtime: &mut Runtime) {
        runtime.epoch += 1;
        runtime.session_cancel.cancel();
        runtime.session_cancel = CancellationToken::new();
        Self::freeze(
            runtime,
            TaskStatus::AwaitingLogin,
            "UOOC 登录已失效，请重新登录",
        );
        checkin::cancel_locked(runtime, &[], "UOOC 登录已失效");
        runtime.snapshot.session = SessionStatus::LoginRequired;
        for entry in runtime.snapshot.progress.values_mut() {
            entry.refreshing = false;
            entry.error = Some("UOOC 登录已失效，请重新登录".into());
        }
    }
    async fn refresh(self: &Arc<Self>) -> Result<(), String> {
        self.refresh_session(None).await
    }
    // Daily scans share the normal session/list refresh lock, including startup and login.
    async fn refresh_session(
        self: &Arc<Self>,
        automatic_day: Option<chrono::NaiveDate>,
    ) -> Result<(), String> {
        let Ok(_refresh) = self.refresh_lock.try_lock() else {
            return Ok(());
        };
        let (epoch, cancel) = {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.login.is_some() {
                return Ok(());
            }
            if let Some(day) = automatic_day {
                if !matches!(
                    runtime.snapshot.session,
                    SessionStatus::Ready | SessionStatus::NetworkError
                ) {
                    return Ok(());
                }
                let Some(account) = runtime.snapshot.user.as_ref().map(|user| user.id.clone())
                else {
                    return Ok(());
                };
                runtime
                    .checkins
                    .daily_scans
                    .retain(|(_, date)| *date == day);
                if !runtime.checkins.daily_scans.insert((account, day)) {
                    return Ok(());
                }
            }
            if runtime.snapshot.session != SessionStatus::Ready {
                runtime.snapshot.session = SessionStatus::Checking;
            }
            runtime.snapshot.message = "正在刷新 UOOC 会话和课程…".into();
            self.publish(&mut runtime);
            (runtime.epoch, runtime.session_cancel.clone())
        };
        let client = self.client();
        // The list request always happens, even without cookies or an open /uooc page.
        let result = async {
            let courses = client.courses(&cancel).await?;
            let user = client.user(&cancel).await?;
            Ok::<_, Error>((user, courses))
        }
        .await;
        {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.epoch != epoch || cancel.is_cancelled() {
                return Ok(());
            }
            match result {
                Ok((user, courses)) => {
                    if runtime.discussion.account_id != user.id {
                        runtime.discussion = DiscussionStoreState {
                            account_id: user.id.clone(),
                            ..DiscussionStoreState::default()
                        };
                    }
                    if runtime
                        .snapshot
                        .user
                        .as_ref()
                        .is_some_and(|old| old.id != user.id)
                    {
                        Self::freeze(&mut runtime, TaskStatus::Paused, "UOOC 账户已变化");
                        checkin::cancel_locked(&mut runtime, &[], "UOOC 账户已变化");
                        runtime.epoch += 1;
                        runtime.session_cancel.cancel();
                        runtime.session_cancel = CancellationToken::new();
                        runtime.snapshot.progress.clear();
                        runtime.snapshot.checkins.clear();
                        runtime.discussion = DiscussionStoreState::default();
                    }
                    runtime.snapshot.update_courses(user, courses);
                    runtime.snapshot.session = SessionStatus::Ready;
                    runtime.snapshot.message = match self.save_session(&client) {
                        Ok(()) => String::new(),
                        Err(error) => error,
                    };
                }
                Err(error) => {
                    runtime.snapshot.session = if error == Error::Unauthorized {
                        SessionStatus::LoginRequired
                    } else {
                        SessionStatus::NetworkError
                    };
                    runtime.snapshot.message = error.message();
                    Self::freeze(&mut runtime, TaskStatus::AwaitingLogin, &error.message());
                    checkin::cancel_locked(&mut runtime, &[], &error.message());
                    if error == Error::Unauthorized {
                        Self::expire_session(&mut runtime);
                    }
                }
            }
            self.publish(&mut runtime);
        }
        self.reconcile();
        if self.snapshot().session == SessionStatus::Ready {
            self.session_ready().await;
        }
        Ok(())
    }
    fn set_settings(&self, mut settings: Settings) -> Result<(), String> {
        settings.validate()?;
        settings.qa_server_url = normalize_qa_url(&settings.qa_server_url)?;
        let mut runtime = self.runtime.lock().unwrap();
        if !runtime.workers.is_empty() {
            return Err("请先暂停全部任务再修改设置".into());
        }
        *self.permits.write().unwrap() = Arc::new(Semaphore::new(settings.concurrency));
        runtime.snapshot.settings = settings;
        self.publish(&mut runtime);
        Ok(())
    }
    pub(crate) async fn qa_api_key(&self) -> Option<String> {
        self.qa_api_key_for_server(None).await
    }
    pub(crate) async fn qa_api_key_for_server(&self, server: Option<&str>) -> Option<String> {
        let (server_url, key) = {
            let r = self.runtime.lock().unwrap();
            let target = server
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| r.snapshot.settings.qa_server_url.clone());
            (target, r.qa_api_key.clone())
        };
        if let Some(key) = &key {
            if !key.trim().is_empty() {
                return Some(key.clone());
            }
        }
        let new_key = quiz::qa::ensure_api_key(&server_url, None).await?;
        let mut r = self.runtime.lock().unwrap();
        r.qa_api_key = Some(new_key.clone());
        self.publish(&mut r);
        Some(new_key)
    }
    pub(crate) fn save_qa_api_key(&self, key: String) -> Result<(), String> {
        let trimmed = key.trim().to_string();
        let mut r = self.runtime.lock().unwrap();
        r.qa_api_key = if trimmed.is_empty() { None } else { Some(trimmed) };
        self.publish(&mut r);
        Ok(())
    }
    pub(crate) fn reset_qa_api_key(&self) -> Result<(), String> {
        let mut r = self.runtime.lock().unwrap();
        r.qa_api_key = None;
        self.publish(&mut r);
        Ok(())
    }
    pub(crate) fn log_debug(&self, stage: &str, message: impl Into<String>) {
        let msg = message.into();
        let Some(app) = &self.app else { return };
        let Some(store) = app.try_state::<crate::campus_network::CampusNetworkStore>() else {
            return;
        };
        crate::campus_network::diagnostics::record(
            app,
            &store,
            crate::campus_network::model::DebugLogEntry {
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                level: "info".into(),
                source: "uooc".into(),
                stage: stage.into(),
                message: Some(msg),
                ..Default::default()
            },
        );
    }
    pub(crate) async fn build_local_bank(self: &Arc<Self>) -> Result<usize, String> {
        quiz::build_local_bank(self).await
    }
    pub(crate) async fn full_qa_sync(self: &Arc<Self>) {
        let (_account_id, cids, qa_server_url) = {
            let r = self.runtime.lock().unwrap();
            let Some(user_id) = r.snapshot.user.as_ref().map(|u| u.id.clone()) else {
                return;
            };
            let cids: Vec<String> = r.snapshot.courses.iter().map(|c| c.id.clone()).collect();
            let server_url = r.snapshot.settings.qa_server_url.clone();
            (user_id, cids, server_url)
        };

        if cids.is_empty() {
            return;
        }

        self.log_debug(
            "qa",
            format!("完成登录，开始全量同步已添加的 {} 门课程服务端题库向量缓存…", cids.len()),
        );

        let cache_path = self.path.parent().unwrap().join("qa-vector-cache.json");
        let api_key = self.qa_api_key_for_server(Some(&qa_server_url)).await;

        match quiz::qa::download_qa_vectors(&cache_path, &qa_server_url, api_key.as_deref(), &cids).await {
            Ok(status) => {
                self.log_debug(
                    "qa",
                    format!("服务端题库向量缓存拉取成功，已缓存 {} 条题目向量", status.cached_count),
                );
            }
            Err(err) => {
                self.log_debug("qa", format!("拉取服务端题库向量缓存失败: {err}"));
            }
        }

        self.log_debug("qa", "开始全量题库构建…");
        match self.build_local_bank().await {
            Ok(count) => {
                self.log_debug("qa", format!("全量题库构建完成，共收录 {count} 道题目"));
            }
            Err(err) => {
                self.log_debug("qa", format!("全量题库构建失败: {err}"));
            }
        }
    }
    fn start(self: &Arc<Self>, ids: Vec<String>) -> Result<(), String> {
        if ids.is_empty() {
            return Err("请先选择课程".into());
        }
        let mut added = HashSet::new();
        {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.snapshot.session != SessionStatus::Ready {
                return Err("请先登录并刷新课程列表".into());
            }
            let account_id = runtime.snapshot.user.as_ref().ok_or("请先登录")?.id.clone();
            let courses = ids
                .iter()
                .map(|id| {
                    runtime
                        .snapshot
                        .courses
                        .iter()
                        .find(|c| &c.id == id)
                        .cloned()
                        .ok_or("只能启动当前开课中的课程".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            for course in courses {
                if course.status == "33" {
                    return Err(format!("课程「{}」处于复习模式，无法启动视频任务", course.title));
                }
                if let Some(task) = runtime
                    .snapshot
                    .tasks
                    .iter_mut()
                    .find(|t| t.course.id == course.id)
                {
                    if task.account_id == account_id {
                        continue;
                    }
                    task.account_id = account_id.clone();
                    added.insert(course.id.clone());
                    task.course = course;
                    task.desired_running = true;
                    task.status = TaskStatus::Queued;
                    task.message.clear();
                    task.videos.clear();
                } else {
                    added.insert(course.id.clone());
                    runtime.snapshot.tasks.push(CourseTask {
                        course,
                        account_id: account_id.clone(),
                        desired_running: true,
                        status: TaskStatus::Queued,
                        message: String::new(),
                        videos: vec![],
                        generation: 0,
                    });
                }
            }
            self.publish(&mut runtime);
        }
        if !added.is_empty() {
            self.reconcile_selected(Some(&added));
        }
        Ok(())
    }
    fn control(self: &Arc<Self>, ids: Vec<String>, action: TaskAction) -> Result<(), String> {
        self.control_generation(ids, action, None)
    }
    fn control_generation(
        self: &Arc<Self>,
        ids: Vec<String>,
        action: TaskAction,
        background_generation: Option<u64>,
    ) -> Result<(), String> {
        let mut resuming = HashSet::new();
        {
            let mut runtime = self.runtime.lock().unwrap();
            if background_generation
                .is_some_and(|generation| generation != runtime.background_generation)
            {
                return Ok(());
            }
            if matches!(action, TaskAction::Resume)
                && runtime.snapshot.session != SessionStatus::Ready
            {
                return Err("请先登录并刷新课程列表".into());
            }
            let targets: Vec<_> = runtime
                .snapshot
                .tasks
                .iter()
                .filter(|t| {
                    runtime
                        .snapshot
                        .user
                        .as_ref()
                        .is_none_or(|user| user.id == t.account_id)
                })
                .filter(|t| ids.is_empty() || ids.contains(&t.course.id))
                .filter(|t| match action {
                    TaskAction::Pause => t.desired_running || t.status.active(),
                    TaskAction::Resume => {
                        !t.status.active()
                            && (!ids.is_empty()
                                || !matches!(t.status, TaskStatus::Completed | TaskStatus::DryRun))
                    }
                    TaskAction::Stop => !matches!(
                        t.status,
                        TaskStatus::Completed | TaskStatus::DryRun | TaskStatus::Stopped
                    ),
                    TaskAction::Delete => true,
                })
                .map(|t| t.course.id.clone())
                .collect();
            if matches!(action, TaskAction::Delete) {
                for id in &targets {
                    if let Some((_, token)) = runtime.workers.remove(id) {
                        token.cancel();
                    }
                    runtime.snapshot.quizzes.remove(id);
                    runtime.snapshot.discussions.remove(id);
                    runtime.discussion.courses.remove(id);
                    runtime.snapshot.checkins.remove(id);
                }
                runtime.snapshot.tasks.retain(|t| !targets.contains(&t.course.id));
                self.publish(&mut runtime);
            } else {
                if background_generation.is_some() && matches!(action, TaskAction::Stop) {
                    checkin::cancel_locked(&mut runtime, &[], "已停止后台任务");
                }
                for id in targets {
                    if matches!(action, TaskAction::Resume) {
                        resuming.insert(id.clone());
                    }
                    if let Some((_, token)) = runtime.workers.remove(&id) {
                        token.cancel();
                    }
                    let task = runtime
                        .snapshot
                        .tasks
                        .iter_mut()
                        .find(|t| t.course.id == id)
                        .unwrap();
                    task.generation += 1;
                    task.desired_running = matches!(action, TaskAction::Resume);
                    task.status = match action {
                        TaskAction::Pause => TaskStatus::Paused,
                        TaskAction::Resume => TaskStatus::Queued,
                        TaskAction::Stop => TaskStatus::Stopped,
                        TaskAction::Delete => unreachable!(),
                    };
                    task.message.clear();
                    for video in task.videos.iter_mut().filter(|v| v.status.active()) {
                        video.status = task.status.clone();
                    }
                }
                self.publish(&mut runtime);
            }
        }
        if !resuming.is_empty() {
            self.reconcile_selected(Some(&resuming));
        } else {
            let _ = self.sync_platform();
        }
        Ok(())
    }
    fn reconcile(self: &Arc<Self>) {
        self.reconcile_selected(None);
    }
    fn reconcile_selected(self: &Arc<Self>, selected: Option<&HashSet<String>>) {
        let mut starts = vec![];
        {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.snapshot.session == SessionStatus::Ready {
                let user = runtime
                    .snapshot
                    .user
                    .as_ref()
                    .map(|u| u.id.clone())
                    .unwrap_or_default();
                let courses = runtime.snapshot.courses.clone();
                let existing: Vec<_> = runtime.workers.keys().cloned().collect();
                let mut cancel_ids = vec![];
                for task in &mut runtime.snapshot.tasks {
                    if !task.desired_running {
                        continue;
                    }
                    if task.status == TaskStatus::AwaitingDecision {
                        continue;
                    }
                    if selected.is_some_and(|ids| !ids.contains(&task.course.id)) {
                        continue;
                    }
                    if task.account_id != user || !courses.iter().any(|c| c.id == task.course.id) {
                        task.generation += 1;
                        task.desired_running = false;
                        task.status = TaskStatus::Stopped;
                        task.message = "账户已变化或课程不再处于开课中".into();
                        cancel_ids.push(task.course.id.clone());
                        continue;
                    }
                    if task.course.status == "33" {
                        task.generation += 1;
                        task.desired_running = false;
                        task.status = TaskStatus::Stopped;
                        task.message = "课程已进入复习模式，无法看视频与做测验/作业".into();
                        cancel_ids.push(task.course.id.clone());
                        continue;
                    }
                    if existing.contains(&task.course.id) {
                        continue;
                    }
                    task.generation += 1;
                    task.status = TaskStatus::Queued;
                    task.message.clear();
                    // Preserve completed rows across resume; discovery reconciles fresh server state.
                    task.videos
                        .retain(|video| video.status == TaskStatus::Completed);
                    starts.push((
                        task.course.id.clone(),
                        task.generation,
                        CancellationToken::new(),
                    ));
                }
                for id in cancel_ids {
                    if let Some((_, token)) = runtime.workers.remove(&id) {
                        token.cancel();
                    }
                }
                for (id, generation, token) in &starts {
                    runtime
                        .workers
                        .insert(id.clone(), (*generation, token.clone()));
                }
            }
            self.publish(&mut runtime);
        }
        if let Err(error) = self.sync_platform() {
            self.system_pause(&error);
            return;
        }
        for (cid, generation, cancel) in starts {
            let service = self.clone();
            let client = self.client();
            let settings = self.snapshot().settings;
            let permits = self.permits.read().unwrap().clone();
            tauri::async_runtime::spawn(async move {
                let result = runner::run(
                    service.clone(),
                    client.clone(),
                    cid.clone(),
                    generation,
                    settings,
                    permits,
                    cancel.clone(),
                )
                .await;
                if !cancel.is_cancelled() {
                    let _ = service.fetch_progress(&cid, &cancel).await;
                }
                let resume = result == Ok(TaskStatus::Queued);
                service.finish(&cid, generation, result, &client);
                if resume {
                    service.reconcile();
                }
            });
        }
    }
    fn finish(
        self: &Arc<Self>,
        cid: &str,
        generation: u64,
        result: Result<TaskStatus, Error>,
        client: &Client,
    ) {
        {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.workers.get(cid).map(|(g, _)| *g) != Some(generation) {
                return;
            }
            runtime.workers.remove(cid);
            if result == Err(Error::Unauthorized) {
                Self::expire_session(&mut runtime);
            } else if let Some(task) = runtime
                .snapshot
                .tasks
                .iter_mut()
                .find(|t| t.course.id == cid && t.generation == generation)
            {
                task.desired_running = matches!(
                    result,
                    Ok(TaskStatus::Queued | TaskStatus::AwaitingDecision)
                );
                task.status = result.clone().unwrap_or(TaskStatus::Failed);
                task.message = match &result {
                    Err(error) => error.message(),
                    Ok(TaskStatus::Blocked) => "需要先在官网完成前置任务或测验，再继续".into(),
                    Ok(TaskStatus::Failed) => "部分视频失败或无法解析，请查看视频详情".into(),
                    _ => String::new(),
                };
            }
            let log_msg = format!(
                "课程 {cid}：{}",
                result
                    .as_ref()
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|e| e.message())
            );
            self.log_debug("task", &log_msg);
            runtime.snapshot.logs.push(log_msg);
            if let Err(error) = self.save_session(client) {
                runtime.snapshot.message = error;
            }
            self.publish(&mut runtime);
        }
        let _ = self.sync_platform();
    }
    fn task_change(&self, cid: &str, generation: u64, f: impl FnOnce(&mut CourseTask)) {
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.workers.get(cid).map(|(g, _)| *g) != Some(generation) {
            return;
        }
        if let Some(task) = runtime
            .snapshot
            .tasks
            .iter_mut()
            .find(|t| t.course.id == cid && t.generation == generation)
        {
            f(task);
        }
        self.publish(&mut runtime);
    }
    fn system_pause(&self, message: &str) {
        self.system_pause_generation(message, None);
    }
    fn system_pause_generation(&self, message: &str, background_generation: Option<u64>) {
        {
            let mut runtime = self.runtime.lock().unwrap();
            if background_generation
                .is_some_and(|generation| generation != runtime.background_generation)
            {
                return;
            }
            Self::freeze(&mut runtime, TaskStatus::SystemPaused, message);
            checkin::cancel_locked(&mut runtime, &[], message);
            runtime.snapshot.message = message.into();
            self.publish(&mut runtime);
        }
        let _ = self.sync_platform();
    }
    fn sync_platform(&self) -> Result<(), String> {
        // Serialize native updates with state changes; an old completion cannot stop a new run.
        let mut runtime = self.runtime.lock().unwrap();
        let count = runtime.workers.len()
            + runtime.checkins.queue.len()
            + usize::from(
                runtime
                    .checkins
                    .active
                    .as_ref()
                    .is_some_and(|job| !job.cancel.is_cancelled()),
            );
        if count > 0 && !runtime.background_active {
            runtime.background_generation += 1;
        }
        runtime.background_active = count > 0;
        platform::running(count, runtime.background_generation)
    }

    pub(crate) async fn list_exams(&self, cid: Option<&str>) -> Result<Vec<ExamTask>, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let (courses, is_ready) = {
            let r = self.runtime.lock().unwrap();
            (r.snapshot.courses.clone(), r.snapshot.session == SessionStatus::Ready)
        };
        if !is_ready {
            return Err("请先登录 UOOC".into());
        }

        let target_courses: Vec<(String, String)> = if let Some(target_cid) = cid {
            courses
                .into_iter()
                .filter(|c| c.id == target_cid)
                .map(|c| (c.id, c.title))
                .collect()
        } else {
            courses
                .into_iter()
                .map(|c| (c.id, c.title))
                .collect()
        };

        let mut all_tasks = Vec::new();
        for (course_id, course_title) in target_courses {
            if let Ok(rows) = client.exam_list(&course_id, &cancel).await {
                for row in rows {
                    let task_id = id(&row["task_id"])
                        .or_else(|| id(&row["id"]))
                        .unwrap_or_default();
                    let exam_id = id(&row["id"]).unwrap_or_else(|| task_id.clone());
                    let title = text(&row["title"]);
                    let title = if title.is_empty() {
                        text(&row["name"])
                    } else {
                        title
                    };
                    let start_time = text(&row["start_time"]);
                    let end_time = text(&row["end_time"]);
                    let limit_time = number(&row["limit_time"]) as u64;
                    let limit_time = if limit_time == 0 {
                        number(&row["exam_time"]) as u64
                    } else {
                        limit_time
                    };
                    let status_text = text(&row["status"]);
                    let mut status_code = number(&row["status_code"]) as i32;
                    if status_code == 0 && (status_text == "已批改" || status_text == "已完成") {
                        status_code = 20;
                    }
                    let can_do = row["state"]["do"].as_bool().unwrap_or(false);
                    let can_view = row["state"]["view"].as_bool().unwrap_or(false);
                    let can_eval = row["state"]["eval"].as_bool().unwrap_or(false);
                    let chance = number(&row["chance"]) as u32;
                    let submit_count = number(&row["submit_count"]) as u32;
                    let score = row.get("score").and_then(|v| {
                        v.as_f64().or_else(|| {
                            v.as_str().and_then(|s| s.trim().parse::<f64>().ok())
                        })
                    });
                    let paper_id = id(&row["paper_id"]);
                    let chapter_text = text(&row["chapter_text"]);
                    let chapter_text = if chapter_text.is_empty() { None } else { Some(chapter_text) };
                    let section_text = text(&row["section_text"]);
                    let section_text = if section_text.is_empty() { None } else { Some(section_text) };

                    all_tasks.push(ExamTask {
                        id: exam_id,
                        title,
                        cid: course_id.clone(),
                        course_name: course_title.clone(),
                        start_time,
                        end_time,
                        limit_time,
                        status: status_code,
                        status_text,
                        status_code,
                        can_do,
                        can_view,
                        can_eval,
                        chance,
                        submit_count,
                        score,
                        paper_id,
                        task_id,
                        chapter_text,
                        section_text,
                    });
                }
            }
        }
        Ok(all_tasks)
    }

    pub(crate) async fn get_exam_paper(&self, cid: &str, task_id: &str) -> Result<ExamPaper, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        {
            let r = self.runtime.lock().unwrap();
            if r.snapshot.session != SessionStatus::Ready {
                return Err("请先登录 UOOC".into());
            }
        }

        let info = client.exam_task_info(task_id, cid, &cancel).await.ok();
        let title = info.as_ref()
            .and_then(|v| v["title"].as_str().or_else(|| v["name"].as_str()))
            .unwrap_or("课程考核")
            .to_string();
        let limit_time = info.as_ref()
            .map(|v| number(&v["limit_time"]) as u64)
            .filter(|&t| t > 0)
            .or_else(|| info.as_ref().map(|v| number(&v["exam_time"]) as u64))
            .unwrap_or(0);
        let is_submitted = info.as_ref()
            .map(|v| number(&v["status"]) as i64)
            .map(|s| s >= 2)
            .unwrap_or(false);

        let mut questions = Vec::new();
        let mut answers = quiz::model::Answers::new();

        // 1. 若为已完成测验/已提交任务，优先通过 /exam/view 获取已完成题目、作答及评分
        if is_submitted {
            let view_url = format!("{}/exam/view?cid={cid}&tid={task_id}", client.origin());
            if let Ok(view_bytes) = client.quiz_http(&view_url, None, &cancel).await {
                if let Some(d) = quiz::qa::parse_exam_view_details(&view_bytes) {
                    if !d.questions.is_empty() {
                        questions = d.questions;
                        answers = if !d.student_answers.is_empty() {
                            d.student_answers
                        } else {
                            d.answers
                        };
                    }
                }
            }
        }

        // 2. 若未提交或 /exam/view 未能获取到题目，通过 /exam/getTaskPaper 获取题单
        if questions.is_empty() {
            let tp_url = format!("{}/exam/getTaskPaper?tid={task_id}", client.origin());
            if let Ok(bytes) = client.quiz_http(&tp_url, None, &cancel).await {
                if let Some((q_list, a_list)) = quiz::qa::parse_task_paper(&bytes) {
                    questions = q_list;
                    answers = a_list;
                }
            }
        }

        // 3. 若仍为空且尚未尝试过 /exam/view，则兜底尝试 /exam/view
        if questions.is_empty() && !is_submitted {
            let view_url = format!("{}/exam/view?cid={cid}&tid={task_id}", client.origin());
            if let Ok(view_bytes) = client.quiz_http(&view_url, None, &cancel).await {
                if let Some(d) = quiz::qa::parse_exam_view_details(&view_bytes) {
                    if !d.questions.is_empty() {
                        questions = d.questions;
                        answers = if !d.student_answers.is_empty() {
                            d.student_answers
                        } else {
                            d.answers
                        };
                    }
                }
            }
        }

        // 4. HTML 文档页面解析兜底
        if questions.is_empty() {
            if let Ok(html) = client.quiz_document(task_id, &cancel).await {
                if let Ok(url) = format!("{}/exam/{task_id}", client.origin()).parse() {
                    if let Ok(p) = quiz::protocol::parse(&html, &url, task_id) {
                        questions = p.questions;
                    }
                }
            }
        }

        if questions.is_empty() {
            return Err("无法获取试卷题目，可能考试/测验尚未开始或需要官方页面处理".into());
        }

        let mut reviews = Vec::new();
        let cache_path = self.path.parent().unwrap().join("qa-vector-cache.json");
        let bank_path = self.path.parent().unwrap().join("qa-bank.json");
        let (server_url, api_key) = {
            let r = self.runtime.lock().unwrap();
            (r.snapshot.settings.qa_server_url.clone(), r.qa_api_key.clone())
        };

        let local_bank = if bank_path.exists() {
            quiz::qa::load_local_bank(&bank_path)
        } else {
            Default::default()
        };

        for q in &questions {
            let mut matched_this_q = false;

            // 1. 优先尝试单题 ID 精确匹配 (本地题库 bank 中 key 或者 question_id)
            if let Some(bank_q) = local_bank.get(&q.id).or_else(|| {
                local_bank.values().find(|item| item.question_id == q.id)
            }) {
                let clean_answers: Vec<String> = if !bank_q.correct_answers.is_empty() {
                    bank_q.correct_answers.clone()
                } else {
                    bank_q
                        .answers
                        .iter()
                        .filter(|a| a.is_correct)
                        .flat_map(|a| a.values.iter().filter_map(|v| v.as_str().map(String::from)))
                        .collect()
                };
                if !clean_answers.is_empty() {
                    let mapped_answers: Vec<String> = clean_answers
                        .iter()
                        .map(|ans| {
                            if let Some(opt) = q.options.iter().find(|o| quiz::qa::is_option_match(o, ans)) {
                                opt.id.clone()
                            } else {
                                ans.clone()
                            }
                        })
                        .collect();
                    if !mapped_answers.is_empty() {
                        answers.entry(q.id.clone()).or_insert_with(|| mapped_answers.clone());
                        reviews.push(quiz::model::Review {
                            question_id: q.id.clone(),
                            answers: mapped_answers,
                            probabilities: Default::default(),
                            message: "题库单题ID精确命中 (来自随堂测验)".into(),
                        });
                        matched_this_q = true;
                    }
                }
            }

            // 2. 单题向量特征匹配 (针对题干 + 选项语义)
            if !matched_this_q {
                let opts: Vec<quiz::qa::QaUploadOption> = q
                    .options
                    .iter()
                    .map(|o| quiz::qa::QaUploadOption {
                        id: o.id.clone(),
                        text: o.text.clone(),
                    })
                    .collect();
                let query = quiz::qa::build_embedding_text(Some(&q.text), Some(&opts));
                if let Ok(matched) = quiz::qa::search_questions_with_local_embedding(
                    &cache_path,
                    &bank_path,
                    &server_url,
                    api_key.as_deref(),
                    &query,
                    3,
                )
                .await
                {
                    if let Some(top) = matched.first() {
                        let mapped_answers: Vec<String> = top
                            .answers
                            .iter()
                            .map(|ans| {
                                if let Some(opt) = q.options.iter().find(|o| quiz::qa::is_option_match(o, ans)) {
                                    opt.id.clone()
                                } else {
                                    ans.clone()
                                }
                            })
                            .collect();

                        if top.similarity > 0.85 || top.question_id == q.id {
                            if !mapped_answers.is_empty() {
                                answers.entry(q.id.clone()).or_insert_with(|| mapped_answers.clone());
                            }
                            reviews.push(quiz::model::Review {
                                question_id: q.id.clone(),
                                answers: mapped_answers,
                                probabilities: Default::default(),
                                message: format!("题库单题向量命中 (相似度 {:.0}%)", top.similarity * 100.0),
                            });
                        } else if top.similarity > 0.70 {
                            reviews.push(quiz::model::Review {
                                question_id: q.id.clone(),
                                answers: mapped_answers,
                                probabilities: Default::default(),
                                message: format!("测验相似考点参考 (相似度 {:.0}%)", top.similarity * 100.0),
                            });
                        }
                    }
                }
            }
        }

        Ok(ExamPaper {
            task_id: task_id.to_string(),
            title,
            cid: cid.to_string(),
            limit_time,
            questions,
            answers,
            reviews,
            is_submitted,
        })
    }

    pub(crate) async fn save_exam_draft(
        &self,
        _cid: &str,
        task_id: &str,
        answers: quiz::model::Answers,
    ) -> Result<(), String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let mut fields = Vec::new();
        for (qid, vals) in answers {
            for v in vals {
                fields.push((format!("answers[{qid}]"), v));
            }
        }
        client
            .exam_save(task_id, &fields, &cancel)
            .await
            .map_err(|e| format!("草稿保存失败: {e:?}"))?;
        Ok(())
    }

    pub(crate) async fn submit_exam(
        &self,
        _cid: &str,
        task_id: &str,
        answers: quiz::model::Answers,
    ) -> Result<String, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let mut fields = Vec::new();
        for (qid, vals) in answers {
            for v in vals {
                fields.push((format!("answers[{qid}]"), v));
            }
        }
        let res = client
            .exam_commit(task_id, &fields, &cancel)
            .await
            .map_err(|e| format!("提交试卷失败: {e:?}"))?;
        let msg = text(&res["msg"]);
        Ok(if msg.is_empty() { "试卷已成功提交".into() } else { msg })
    }

    pub(crate) async fn heartbeat_exam(
        &self,
        cid: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        client
            .exam_heartbeat(cid, task_id, &cancel)
            .await
            .map_err(|e| format!("防作弊心跳异常: {e:?}"))
    }

    pub(crate) async fn list_quizzes(&self, cid: Option<&str>) -> Result<Vec<HomeworkTask>, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let (courses, is_ready) = {
            let r = self.runtime.lock().unwrap();
            (r.snapshot.courses.clone(), r.snapshot.session == SessionStatus::Ready)
        };
        if !is_ready {
            return Err("请先登录 UOOC".into());
        }

        let target_courses: Vec<(String, String)> = if let Some(target_cid) = cid {
            courses
                .into_iter()
                .filter(|c| c.id == target_cid)
                .map(|c| (c.id, c.title))
                .collect()
        } else {
            courses
                .into_iter()
                .map(|c| (c.id, c.title))
                .collect()
        };

        let mut all_tasks = Vec::new();
        for (course_id, course_title) in target_courses {
            if let Ok(rows) = client.quiz_list(&course_id, &cancel).await {
                for row in rows {
                    let task_id = id(&row["task_id"])
                        .or_else(|| id(&row["id"]))
                        .or_else(|| id(&row["res_id"]))
                        .unwrap_or_default();
                    let q_id = id(&row["id"]).unwrap_or_else(|| task_id.clone());
                    let title = text(&row["title"]);
                    let title = if title.is_empty() {
                        text(&row["name"])
                    } else {
                        title
                    };
                    let start_time = text(&row["start_time"]);
                    let end_time = text(&row["end_time"]);
                    let status_text = text(&row["status"]);
                    let mut status_code = number(&row["status_code"]) as i32;
                    if status_code == 0 && (status_text == "已批改" || status_text == "已完成") {
                        status_code = 20;
                    }
                    let can_do = row["state"]["do"].as_bool().unwrap_or(false);
                    let can_view = row["state"]["view"].as_bool().unwrap_or(false);
                    let can_eval = row["state"]["eval"].as_bool().unwrap_or(false);
                    let chance = number(&row["chance"]) as u32;
                    let submit_count = number(&row["submit_count"]) as u32;
                    let score = row.get("score").and_then(|v| {
                        v.as_f64().or_else(|| {
                            v.as_str().and_then(|s| s.trim().parse::<f64>().ok())
                        })
                    });
                    let paper_id = id(&row["paper_id"]);
                    let chapter_text = text(&row["chapter_text"]);
                    let chapter_text = if chapter_text.is_empty() { None } else { Some(chapter_text) };
                    let section_text = text(&row["section_text"]);
                    let section_text = if section_text.is_empty() { None } else { Some(section_text) };
                    let res_id = id(&row["res_id"]).or_else(|| id(&row["resource_id"]));
                    let commit_time = text(&row["commit_time"]);
                    let commit_time = if commit_time.is_empty() { None } else { Some(commit_time) };
                    let do_time = text(&row["do_time"]);
                    let do_time = if do_time.is_empty() { None } else { Some(do_time) };

                    all_tasks.push(HomeworkTask {
                        id: q_id,
                        title,
                        cid: course_id.clone(),
                        course_name: course_title.clone(),
                        start_time,
                        end_time,
                        status: status_code,
                        status_text,
                        status_code,
                        can_do,
                        can_view,
                        can_eval,
                        chance,
                        submit_count,
                        score,
                        paper_id,
                        task_id,
                        has_eval: false,
                        eval_status: None,
                        chapter_text,
                        section_text,
                        res_id,
                        commit_time,
                        do_time,
                    });
                }
            }
        }
        Ok(all_tasks)
    }

    pub(crate) async fn list_homeworks(&self, cid: Option<&str>) -> Result<Vec<HomeworkTask>, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let (courses, is_ready) = {
            let r = self.runtime.lock().unwrap();
            (r.snapshot.courses.clone(), r.snapshot.session == SessionStatus::Ready)
        };
        if !is_ready {
            return Err("请先登录 UOOC".into());
        }

        let target_courses: Vec<(String, String)> = if let Some(target_cid) = cid {
            courses
                .into_iter()
                .filter(|c| c.id == target_cid)
                .map(|c| (c.id, c.title))
                .collect()
        } else {
            courses
                .into_iter()
                .map(|c| (c.id, c.title))
                .collect()
        };

        let mut all_tasks = Vec::new();
        for (course_id, course_title) in target_courses {
            let has_eval = client.exist_eval(&course_id, &cancel).await.unwrap_or(false);
            if let Ok(rows) = client.homework_list(&course_id, &cancel).await {
                for row in rows {
                    let task_id = id(&row["task_id"])
                        .or_else(|| id(&row["id"]))
                        .unwrap_or_default();
                    let hw_id = id(&row["id"]).unwrap_or_else(|| task_id.clone());
                    let title = text(&row["title"]);
                    let title = if title.is_empty() {
                        text(&row["name"])
                    } else {
                        title
                    };
                    let start_time = text(&row["start_time"]);
                    let end_time = text(&row["end_time"]);
                    let status_text = text(&row["status"]);
                    let mut status_code = number(&row["status_code"]) as i32;
                    if status_code == 0 && (status_text == "已批改" || status_text == "已完成") {
                        status_code = 20;
                    }
                    let can_do = row["state"]["do"].as_bool().unwrap_or(false);
                    let can_view = row["state"]["view"].as_bool().unwrap_or(false);
                    let can_eval = row["state"]["eval"].as_bool().unwrap_or(false);
                    let chance = number(&row["chance"]) as u32;
                    let submit_count = number(&row["submit_count"]) as u32;
                    let score = row.get("score").and_then(|v| {
                        v.as_f64().or_else(|| {
                            v.as_str().and_then(|s| s.trim().parse::<f64>().ok())
                        })
                    });
                    let paper_id = id(&row["paper_id"]);
                    let eval_status = text(&row["eval_status"]);
                    let eval_status = if eval_status.is_empty() { None } else { Some(eval_status) };
                    let chapter_text = text(&row["chapter_text"]);
                    let chapter_text = if chapter_text.is_empty() { None } else { Some(chapter_text) };
                    let section_text = text(&row["section_text"]);
                    let section_text = if section_text.is_empty() { None } else { Some(section_text) };
                    let res_id = id(&row["res_id"]).or_else(|| id(&row["resource_id"]));
                    let commit_time = text(&row["commit_time"]);
                    let commit_time = if commit_time.is_empty() { None } else { Some(commit_time) };
                    let do_time = text(&row["do_time"]);
                    let do_time = if do_time.is_empty() { None } else { Some(do_time) };

                    all_tasks.push(HomeworkTask {
                        id: hw_id,
                        title,
                        cid: course_id.clone(),
                        course_name: course_title.clone(),
                        start_time,
                        end_time,
                        status: status_code,
                        status_text,
                        status_code,
                        can_do,
                        can_view,
                        can_eval,
                        chance,
                        submit_count,
                        score,
                        paper_id,
                        task_id,
                        has_eval: has_eval || can_eval,
                        eval_status,
                        chapter_text,
                        section_text,
                        res_id,
                        commit_time,
                        do_time,
                    });
                }
            }
        }
        Ok(all_tasks)
    }

    pub(crate) async fn match_paper_qa(
        &self,
        questions: Vec<quiz::model::Question>,
    ) -> Result<(quiz::model::Answers, Vec<quiz::model::Review>), String> {
        let mut answers = quiz::model::Answers::new();
        let mut reviews = Vec::new();
        let cache_path = self.path.parent().unwrap().join("qa-vector-cache.json");
        let bank_path = self.path.parent().unwrap().join("qa-bank.json");
        let (server_url, api_key) = {
            let r = self.runtime.lock().unwrap();
            (r.snapshot.settings.qa_server_url.clone(), r.qa_api_key.clone())
        };

        let local_bank = if bank_path.exists() {
            quiz::qa::load_local_bank(&bank_path)
        } else {
            Default::default()
        };

        for q in &questions {
            let mut matched_this_q = false;

            // 1. 优先尝试单题 ID 精确匹配
            if let Some(bank_q) = local_bank.get(&q.id).or_else(|| {
                local_bank.values().find(|item| item.question_id == q.id)
            }) {
                let clean_answers: Vec<String> = if !bank_q.correct_answers.is_empty() {
                    bank_q.correct_answers.clone()
                } else {
                    bank_q
                        .answers
                        .iter()
                        .filter(|a| a.is_correct)
                        .flat_map(|a| a.values.iter().filter_map(|v| v.as_str().map(String::from)))
                        .collect()
                };
                if !clean_answers.is_empty() {
                    let mapped_answers: Vec<String> = clean_answers
                        .iter()
                        .map(|ans| {
                            if let Some(opt) = q.options.iter().find(|o| quiz::qa::is_option_match(o, ans)) {
                                opt.id.clone()
                            } else {
                                ans.clone()
                            }
                        })
                        .collect();
                    if !mapped_answers.is_empty() {
                        answers.insert(q.id.clone(), mapped_answers.clone());
                        reviews.push(quiz::model::Review {
                            question_id: q.id.clone(),
                            answers: mapped_answers,
                            probabilities: Default::default(),
                            message: "题库单题ID精确命中 (来自随堂测验)".into(),
                        });
                        matched_this_q = true;
                    }
                }
            }

            // 2. 单题向量特征匹配
            if !matched_this_q {
                let opts: Vec<quiz::qa::QaUploadOption> = q
                    .options
                    .iter()
                    .map(|o| quiz::qa::QaUploadOption {
                        id: o.id.clone(),
                        text: o.text.clone(),
                    })
                    .collect();
                let query = quiz::qa::build_embedding_text(Some(&q.text), Some(&opts));
                if let Ok(matched) = quiz::qa::search_questions_with_local_embedding(
                    &cache_path,
                    &bank_path,
                    &server_url,
                    api_key.as_deref(),
                    &query,
                    3,
                )
                .await
                {
                    if let Some(top) = matched.first() {
                        let mapped_answers: Vec<String> = top
                            .answers
                            .iter()
                            .map(|ans| {
                                if let Some(opt) = q.options.iter().find(|o| quiz::qa::is_option_match(o, ans)) {
                                    opt.id.clone()
                                } else {
                                    ans.clone()
                                }
                            })
                            .collect();

                        if top.similarity > 0.85 || top.question_id == q.id {
                            if !mapped_answers.is_empty() {
                                answers.insert(q.id.clone(), mapped_answers.clone());
                            }
                            reviews.push(quiz::model::Review {
                                question_id: q.id.clone(),
                                answers: mapped_answers,
                                probabilities: Default::default(),
                                message: format!("题库单题向量命中 (相似度 {:.0}%)", top.similarity * 100.0),
                            });
                        } else if top.similarity > 0.70 {
                            reviews.push(quiz::model::Review {
                                question_id: q.id.clone(),
                                answers: mapped_answers,
                                probabilities: Default::default(),
                                message: format!("测验相似考点参考 (相似度 {:.0}%)", top.similarity * 100.0),
                            });
                        }
                    }
                }
            }
        }

        Ok((answers, reviews))
    }

    pub(crate) async fn solve_paper_ai(
        &self,
        questions: Vec<quiz::model::Question>,
    ) -> Result<(quiz::model::Answers, Vec<quiz::model::Review>), String> {
        let cancel = CancellationToken::new();
        let wrong_map = std::collections::BTreeMap::new();
        let (answers, reviews, _, err) = ai::generate(self, &questions, &wrong_map, &cancel, None).await;
        if let Some(e) = err {
            if answers.is_empty() && reviews.is_empty() {
                return Err(format!("AI 答题失败: {e}"));
            }
        }
        Ok((answers, reviews))
    }

    pub(crate) async fn list_evals(&self, cid: Option<&str>) -> Result<Vec<PeerEvalTask>, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let (courses, is_ready) = {
            let r = self.runtime.lock().unwrap();
            (r.snapshot.courses.clone(), r.snapshot.session == SessionStatus::Ready)
        };
        if !is_ready {
            return Err("请先登录 UOOC".into());
        }

        let target_courses: Vec<(String, String)> = if let Some(target_cid) = cid {
            courses
                .into_iter()
                .filter(|c| c.id == target_cid)
                .map(|c| (c.id, c.title))
                .collect()
        } else {
            courses
                .into_iter()
                .map(|c| (c.id, c.title))
                .collect()
        };

        let mut results = Vec::new();
        for (course_id, course_title) in target_courses {
            if let Ok(eval_rows) = client.eval_list(&course_id, &cancel).await {
                for item in eval_rows {
                    let tid = id(&item["tid"]).or_else(|| id(&item["task_id"])).unwrap_or_default();
                    let title = text(&item["title"]);
                    let title = if title.is_empty() { text(&item["name"]) } else { title };
                    let start_time = text(&item["start_time"]);
                    let end_time = text(&item["end_time"]);

                    let mut submissions = Vec::new();
                    if let Some(list) = item["sub_list"].as_array().or_else(|| item["eval_list"].as_array()) {
                        for sub in list {
                            let eval_id = id(&sub["eval_id"]).or_else(|| id(&sub["id"])).unwrap_or_default();
                            let student_task_id = id(&sub["student_task_id"]).or_else(|| id(&sub["tid"]));
                            let content = text(&sub["content"]);
                            let score = sub.get("score").and_then(|v| v.as_f64());
                            let comments = text(&sub["comments"]);
                            let comments = if comments.is_empty() { None } else { Some(comments) };
                            let is_evaluated = number(&sub["status"]) > 0.0 || score.is_some();
                            submissions.push(PeerSubmission {
                                eval_id,
                                student_task_id,
                                answers: Default::default(),
                                content,
                                score,
                                comments,
                                is_evaluated,
                            });
                        }
                    }

                    let completed_count = submissions.iter().filter(|s| s.is_evaluated).count() as u32;
                    let total_count = submissions.len() as u32;

                    results.push(PeerEvalTask {
                        cid: course_id.clone(),
                        tid,
                        title,
                        course_name: course_title.clone(),
                        start_time,
                        end_time,
                        total_count,
                        completed_count,
                        submissions,
                    });
                }
            }
        }
        Ok(results)
    }

    pub(crate) async fn submit_eval(
        &self,
        cid: &str,
        tid: &str,
        eval_id: &str,
        score: f64,
        comments: &str,
    ) -> Result<String, String> {
        let cancel = CancellationToken::new();
        let client = self.client.read().unwrap().clone();
        let res = client
            .submit_eval_score(cid, tid, eval_id, score, comments, &cancel)
            .await
            .map_err(|e| format!("提交互评失败: {e:?}"))?;
        let msg = text(&res["msg"]);
        Ok(if msg.is_empty() { "互评提交成功".into() } else { msg })
    }

    pub(crate) async fn generate_subjective(
        &self,
        stem: &str,
        context: Option<&str>,
        reference_examples: Vec<ai::ReferenceExample>,
    ) -> Result<String, String> {
        let cancel = CancellationToken::new();
        ai::generate_subjective_answer(self, stem, context, &reference_examples, &cancel).await
    }

    pub(crate) async fn task_portal(
        self: &Arc<Self>,
        parent: tauri::WebviewWindow,
        task_id: String,
    ) -> Result<(), String> {
        login::local_parent(&parent)?;
        let (account, epoch) = {
            let r = self.runtime.lock().unwrap();
            let a = r.snapshot.user.as_ref().ok_or("请先登录")?.id.clone();
            (a, r.epoch)
        };
        if task_id.trim().is_empty() {
            return Err("任务编号无效".into());
        }
        let label = format!("uooc-task-{}", uuid::Uuid::new_v4());
        let builder = tauri::WebviewWindowBuilder::new(
            parent.app_handle(),
            &label,
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .title("UOOC 官方试卷/任务页面");
        #[cfg(not(target_os = "android"))]
        let builder = builder.user_agent(user_agent::DESKTOP);
        let window = builder.build().map_err(|_| "无法打开官方页面窗口")?;
        let copy = window.clone();
        let jar = self.client().jar.clone();
        let lease = self.native_session.clone().lock_owned().await;
        let installed = tauri::async_runtime::spawn_blocking(move || {
            let _lease = lease;
            session::install(&copy, &jar, false)
        })
        .await
        .unwrap_or_else(|_| Err("无法恢复会话".into()));
        if installed.is_err()
            || self.snapshot().user.as_ref().is_none_or(|u| u.id != account)
            || self.runtime.lock().unwrap().epoch != epoch
        {
            let _ = window.destroy();
            return Err("会话恢复失败或账户已变化".into());
        }
        window
            .navigate(
                format!("https://www.uooc.net.cn/exam/{task_id}")
                    .parse()
                    .unwrap(),
            )
            .map_err(|_| "无法加载官方试卷".into())
    }
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<UoocStore>();
    if store.service.get().is_some() {
        return Ok(());
    }
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法找到 UOOC 数据目录")?
        .join("uooc");
    let service = Service::new(Some(app.clone()), store.credentials.clone(), root)?;
    store
        .service
        .set(service.clone())
        .map_err(|_| "UOOC 服务已经初始化")?;
    platform::initialize(&service);
    progress::start_updates(&service);
    tauri::async_runtime::spawn(async move {
        let _ = service.refresh().await;
    });
    Ok(())
}
pub(crate) fn service(app: &tauri::AppHandle) -> Result<Arc<Service>, String> {
    app.state::<UoocStore>()
        .service
        .get()
        .cloned()
        .ok_or("UOOC 服务尚未初始化".into())
}
#[cfg(desktop)]
pub(crate) fn keep_running(app: &tauri::AppHandle) -> bool {
    service(app).is_ok_and(|s| {
        let r = s.runtime.lock().unwrap();
        !r.workers.is_empty()
            || !r.checkins.queue.is_empty()
            || r.checkins.active.is_some()
            || r.snapshot
                .tasks
                .iter()
                .any(|t| t.desired_running && t.status == TaskStatus::AwaitingDecision)
    })
}
pub(crate) fn window_closed(app: &tauri::AppHandle, label: &str) {
    if let Ok(service) = service(app) {
        login::window_closed(&service, label);
        quiz::window_closed(&service, label);
    }
}

#[cfg(test)]
mod checkin_tests;
#[cfg(test)]
mod progress_tests;
#[cfg(test)]
mod tests;
