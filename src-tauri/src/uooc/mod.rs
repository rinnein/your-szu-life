#[cfg(target_os = "android")]
mod android_checkin;
mod checkin;
mod checkin_view;
mod client;
pub(crate) mod commands;
mod duration;
mod login;
mod media;
pub(crate) mod model;
mod persistence;
mod platform;
mod progress;
mod runner;
mod session;

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
    #[cfg(test)]
    checkin_factory: Mutex<Option<Arc<dyn checkin_view::Factory>>>,
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
        Ok(Arc::new(Self {
            app,
            credentials,
            path,
            durations: DurationCache::new(root.join("video-durations.json")),
            progress_requests: progress::ProgressRequests::default(),
            native_session: Arc::new(tokio::sync::Mutex::new(())),
            #[cfg(test)]
            checkin_factory: Mutex::new(None),
            permits: RwLock::new(Arc::new(Semaphore::new(saved.settings.concurrency))),
            client: RwLock::new(Arc::new(Client::new(jar)?)),
            refresh_lock: tokio::sync::Mutex::new(()),
            runtime: Mutex::new(Runtime {
                snapshot: Snapshot {
                    settings: saved.settings,
                    tasks: saved.tasks,
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
            }),
        }))
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        self.runtime.lock().unwrap().snapshot.clone()
    }
    fn publish(&self, runtime: &mut Runtime) {
        runtime.snapshot.revision = runtime.snapshot.revision.saturating_add(1);
        if runtime.snapshot.logs.len() > 200 {
            runtime
                .snapshot
                .logs
                .drain(..runtime.snapshot.logs.len() - 200);
        }
        let saved = SavedState {
            settings: runtime.snapshot.settings.clone(),
            tasks: runtime.snapshot.tasks.clone(),
        };
        if let Err(error) = persistence::write(&self.path, &saved) {
            runtime.snapshot.message = error;
        }
        if let Some(app) = &self.app {
            let _ = app.emit("uooc-state-changed", &runtime.snapshot);
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
        let Ok(_refresh) = self.refresh_lock.try_lock() else {
            return Ok(());
        };
        let (epoch, cancel) = {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.login.is_some() {
                return Ok(());
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
    fn set_settings(&self, settings: Settings) -> Result<(), String> {
        settings.validate()?;
        let mut runtime = self.runtime.lock().unwrap();
        if !runtime.workers.is_empty() {
            return Err("请先暂停全部任务再修改设置".into());
        }
        *self.permits.write().unwrap() = Arc::new(Semaphore::new(settings.concurrency));
        runtime.snapshot.settings = settings;
        self.publish(&mut runtime);
        Ok(())
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
                })
                .map(|t| t.course.id.clone())
                .collect();
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
                };
                task.message.clear();
                for video in task.videos.iter_mut().filter(|v| v.status.active()) {
                    video.status = task.status.clone();
                }
            }
            self.publish(&mut runtime);
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
                service.finish(&cid, generation, result, &client);
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
                task.desired_running = false;
                task.status = result.clone().unwrap_or(TaskStatus::Failed);
                task.message = match &result {
                    Err(error) => error.message(),
                    Ok(TaskStatus::Blocked) => "需要先在官网完成前置任务或测验，再继续".into(),
                    Ok(TaskStatus::Failed) => "部分视频失败或无法解析，请查看视频详情".into(),
                    _ => String::new(),
                };
            }
            runtime.snapshot.logs.push(format!(
                "课程 {cid}：{}",
                result
                    .as_ref()
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|e| e.message())
            ));
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
fn service(app: &tauri::AppHandle) -> Result<Arc<Service>, String> {
    app.state::<UoocStore>()
        .service
        .get()
        .cloned()
        .ok_or("UOOC 服务尚未初始化".into())
}
pub(crate) fn keep_running(app: &tauri::AppHandle) -> bool {
    service(app).is_ok_and(|s| {
        let r = s.runtime.lock().unwrap();
        !r.workers.is_empty() || !r.checkins.queue.is_empty() || r.checkins.active.is_some()
    })
}
pub(crate) fn window_closed(app: &tauri::AppHandle, label: &str) {
    if let Ok(service) = service(app) {
        login::window_closed(&service, label);
        checkin::window_closed(&service, label);
    }
}

#[cfg(test)]
mod checkin_tests;
#[cfg(test)]
mod progress_tests;
#[cfg(test)]
mod tests;
