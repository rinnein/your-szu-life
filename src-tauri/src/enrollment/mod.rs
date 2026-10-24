mod academic;
mod cache;
mod captcha;
mod captcha_flow;
mod client;
pub(crate) mod commands;
pub(crate) use commands::Action;
mod contract;
#[cfg(target_os = "linux")]
mod linux_cookies;
mod maintenance;
pub(crate) mod mcp;
mod model;
mod official;
mod platform;
pub(crate) mod portal;
mod protocol;
mod school_cookies;
mod session;
#[cfg(test)]
mod tests;
use model::*;
use protocol::{Error, Failure, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Listener, Manager};

pub struct Store(pub Arc<Service>);
pub struct Service {
    app: tauri::AppHandle,
    credentials: crate::storage::SharedCredentialStore,
    operation: Mutex<()>,
    state: Mutex<Snapshot>,
    cache: Mutex<cache::Cache>,
    secrets: Mutex<session::Secrets>,
    challenge: Mutex<Option<(client::Client, String)>>,
    captcha_flows: Mutex<HashMap<String, captcha_flow::Flow>>,
    next_tick: AtomicI64,
    resume_after_auth: AtomicBool,
    generation: AtomicU64,
    login_attempt: Mutex<Option<String>>,
    shared_sessions: Mutex<HashMap<Mode, session::SharedSchool>>,
    session_serial: AtomicU64,
    official_windows: Mutex<HashMap<Mode, String>>,
    official_opener: tokio::sync::Mutex<()>,
    pub(crate) state_subscribers: crate::ipc_channel::SubscriberPool<EnrollmentDeltaEvent>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub(crate) enum EnrollmentDeltaEvent {
    Initial(Box<Snapshot>),
    Snapshot(Box<Snapshot>),
}
pub(crate) fn initialize(
    app: &tauri::AppHandle,
    credentials: crate::storage::SharedCredentialStore,
) -> std::result::Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定选课数据目录")?;
    std::fs::create_dir_all(&dir).map_err(|_| "无法创建选课目录")?;
    let service = Arc::new(Service {
        app: app.clone(),
        credentials,
        operation: Mutex::new(()),
        state: Mutex::new(Snapshot::default()),
        cache: Mutex::new(cache::Cache::open(&dir.join("academic.sqlite"))?),
        secrets: Mutex::new(session::Secrets::default()),
        challenge: Mutex::new(None),
        captcha_flows: Mutex::new(HashMap::new()),
        next_tick: AtomicI64::new(0),
        resume_after_auth: AtomicBool::new(false),
        generation: AtomicU64::new(1),
        login_attempt: Mutex::new(None),
        shared_sessions: Mutex::new(HashMap::new()),
        session_serial: AtomicU64::new(chrono::Utc::now().timestamp_millis() as u64),
        official_windows: Mutex::new(HashMap::new()),
        official_opener: tokio::sync::Mutex::new(()),
        state_subscribers: crate::ipc_channel::SubscriberPool::new(),
    });
    platform::initialize(&service);
    app.manage(Store(service.clone()));
    let auth_owner = Arc::downgrade(&service);
    app.listen("auth-state-changed", move |_| {
        let Some(service) = auth_owner.upgrade() else {
            return;
        };
        std::thread::spawn(move || {
            let current = service
                .credentials
                .load_profile()
                .ok()
                .flatten()
                .map(|p| p.student_id);
            if current.as_deref() != Some(service.snapshot().context.student.as_str()) {
                for label in service
                    .official_windows
                    .lock()
                    .unwrap()
                    .drain()
                    .map(|(_, l)| l)
                {
                    if let Some(w) = service.app.get_webview_window(&label) {
                        let _ = w.destroy();
                    }
                }
            }
        });
    });
    let weak = Arc::downgrade(&service);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let Some(s) = weak.upgrade() else {
            break;
        };
        if let Err(e) = s.tick() {
            s.recover_tick(e);
        }
    });
    let weak = Arc::downgrade(&service);
    std::thread::spawn(move || {
        let mut cadence = maintenance::Cadence::new(std::time::Instant::now());
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Some(s) = weak.upgrade() else {
                break;
            };
            if cadence.due(std::time::Instant::now()) {
                s.keepalive();
            }
        }
    });
    Ok(())
}
impl Service {
    fn keepalive(&self) {
        let before = self.snapshot();
        if !before.logged_in || before.authentication != Authentication::Authenticated {
            return;
        }
        if self
            .credentials
            .load_profile()
            .ok()
            .flatten()
            .is_none_or(|p| p.student_id != before.context.student)
        {
            return;
        }
        let Ok(shared) = self.shared_client(before.mode) else {
            return;
        };
        if shared.logged_out || shared.client.token.is_empty() {
            return;
        }
        let selected = maintenance::selected_probe(&before, uuid::Uuid::new_v4().as_bytes()[0]);
        let result = shared.client.keepalive(selected);
        let current_account = self
            .credentials
            .load_profile()
            .ok()
            .flatten()
            .map(|p| p.student_id);
        let sessions = self.shared_sessions.lock().unwrap();
        if sessions
            .get(&before.mode)
            .is_none_or(|v| v.generation != shared.generation || v.logged_out)
        {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if state.context.student != before.context.student || state.mode != before.mode {
            return;
        }
        if let Err(error) = result {
            if maintenance::applies(
                &before,
                &state,
                shared.generation,
                shared.generation,
                false,
                current_account.as_deref(),
                error.kind.clone(),
            ) {
                state.admission_failure(&error);
                state.running = false;
            } else {
                return;
            }
        } else {
            drop(state);
            drop(sessions);
            let _ = self.save_shared_cookies();
            return;
        }
        drop(state);
        drop(sessions);
        self.pause("选课会话已失效，请返回登录页面");
    }

    fn recover_tick(&self, e: Error) {
        self.message(&e.message);
        self.next_tick.store(
            chrono::Utc::now().timestamp_millis() + 10000,
            Ordering::SeqCst,
        );
        if !self.snapshot().running {
            return;
        }
        if e.kind == Failure::Business {
            if let Ok(_operation) = self.operation.try_lock() {
                if let Ok(client) = self.client(false) {
                    let student = self.snapshot().context.student;
                    let _ = self.refresh_context(&client, &student);
                }
            }
        }
        if e.kind == Failure::AdmissionRestricted {
            self.pause(&e.message);
            return;
        }
        if self
            .shared_client(self.snapshot().mode)
            .ok()
            .is_some_and(|s| s.logged_out)
        {
            self.pause("已从学校官网退出登录，请手动登录后继续");
            return;
        }
        if e.kind == Failure::Needs2fa {
            self.pause("需2FA验证，完成认证后继续任务");
            self.resume_after_auth.store(true, Ordering::SeqCst);
        } else if e.kind == Failure::Session {
            let Ok(_operation) = self.operation.try_lock() else {
                return;
            };
            match self.login() {
                Ok(()) => {}
                Err(error) if matches!(error.kind, Failure::Initializing | Failure::Network) => {
                    self.message(&error.message)
                }
                Err(error) => {
                    self.pause(&error.message);
                    self.resume_after_auth.store(true, Ordering::SeqCst);
                }
            }
        }
    }
    fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    fn publish(&self) {
        let snapshot = self.snapshot();
        self.state_subscribers.broadcast(&EnrollmentDeltaEvent::Snapshot(Box::new(snapshot.clone())));
        let _ = self.app.emit("enrollment-update", snapshot);
    }
    fn message(&self, message: &str) {
        let mut state = self.state.lock().unwrap();
        if state.message == message {
            return;
        }
        state.message = message.into();
        drop(state);
        self.publish();
        if message.contains("需2FA") || message.contains("已暂停") || message.contains("任务已完成")
        {
            use tauri_plugin_notification::NotificationExt;
            let _ = self
                .app
                .notification()
                .builder()
                .title("选课任务")
                .body(message)
                .show();
        }
    }
    fn persist(&self) -> Result<()> {
        self.save_shared_cookies()?;
        let s = self.snapshot();
        let scope = serde_json::to_string(&s.context)
            .map_err(|_| Error::new(Failure::Protocol, "选课范围无效"))?;
        self.cache
            .lock()
            .unwrap()
            .workspace(&s.context.student, &scope, &s)
            .map_err(Error::from)?;
        Ok(())
    }
    fn identity(&self) -> Result<crate::auth::model::StoredAccountProfile> {
        let profile = self
            .credentials
            .load_profile()
            .map_err(Error::from)?
            .ok_or_else(|| Error::new(Failure::Session, "请先登录应用账户"))?;
        if self.state.lock().unwrap().context.student != profile.student_id {
            self.generation.fetch_add(1, Ordering::SeqCst);
            let mut state: Snapshot = self
                .cache
                .lock()
                .unwrap()
                .get(&profile.student_id, "workspace", "latest")
                .map_err(Error::from)?
                .unwrap_or_default();
            state.context.student = profile.student_id.clone();
            state.running = false;
            state.logged_in = false;
            state.authentication = Authentication::Unknown;
            state.admission = Admission::Unchecked;
            state.admission_message.clear();
            for label in self
                .official_windows
                .lock()
                .unwrap()
                .drain()
                .map(|(_, v)| v)
            {
                if let Some(w) = self.app.get_webview_window(&label) {
                    let _ = w.destroy();
                }
            }
            self.shared_sessions.lock().unwrap().clear();
            self.session_serial.fetch_add(1, Ordering::SeqCst);
            let mut secrets: session::Secrets = self
                .credentials
                .load_academic_sessions()
                .map_err(Error::from)?
                .map(|s| {
                    serde_json::from_str(&s)
                        .map_err(|_| Error::new(Failure::Protocol, "保存的认证数据损坏"))
                })
                .transpose()?
                .unwrap_or_default();
            if secrets.student != profile.student_id {
                secrets = session::Secrets {
                    student: profile.student_id.clone(),
                    ..Default::default()
                };
            }
            *self.secrets.lock().unwrap() = secrets;
            *self.state.lock().unwrap() = state;
            *self.challenge.lock().unwrap() = None;
            let _ = platform::running(0, self.generation.load(Ordering::SeqCst));
        }
        Ok(profile)
    }
    fn save_client(&self, c: &client::Client) -> Result<()> {
        if self
            .credentials
            .load_profile()
            .map_err(Error::from)?
            .is_none_or(|p| p.student_id != c.student)
        {
            return Err(Error::new(Failure::Permanent, "账户已切换，未导入旧会话"));
        }
        let gateway_seed = self.secrets.lock().unwrap().vpn.clone();
        let mut sessions = self.shared_sessions.lock().unwrap();
        if sessions
            .get(&c.mode)
            .is_some_and(|current| c.generation != 0 && current.generation != c.generation)
        {
            return Err(Error::new(
                Failure::Session,
                "会话已更新，已取消过时的登录结果",
            ));
        }
        let generation = self.session_serial.fetch_add(1, Ordering::SeqCst) + 1;
        let mut promoted = c.clone();
        promoted.generation = generation;
        sessions.insert(
            c.mode,
            session::SharedSchool {
                client: promoted,
                generation,
                gateway_seed,
                logged_out: false,
            },
        );
        drop(sessions);
        {
            let mut state = self.state.lock().unwrap();
            if state.mode == c.mode {
                state.authenticated();
                state.admission = Admission::Unchecked;
                state.admission_message.clear();
            }
        }
        self.save_shared_cookies()?;
        self.publish();
        Ok(())
    }
    fn save_shared_cookies(&self) -> Result<()> {
        let clients: Vec<_> = self
            .shared_sessions
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect();
        let mut secrets = self.secrets.lock().unwrap();
        let mut changed = false;
        for shared in clients {
            let raw = session::serialize(&shared.client.jar)?;
            if shared.client.mode == Mode::Webvpn && !shared.logged_out {
                let gateway = session::gateway_only(&shared.client.jar)?;
                if secrets.vpn != gateway {
                    secrets.vpn = gateway;
                    changed = true;
                }
            }

            let session::Secrets {
                campus,
                campus_token,
                webvpn,
                webvpn_token,
                ..
            } = &mut *secrets;
            let (cookies, token) = match shared.client.mode {
                Mode::Campus => (campus, campus_token),
                Mode::Webvpn => (webvpn, webvpn_token),
            };
            if *cookies != raw || *token != shared.client.token {
                *cookies = raw;
                *token = shared.client.token;
                changed = true;
            }
        }
        if changed {
            self.credentials
                .save_academic_sessions(
                    &serde_json::to_string(&*secrets)
                        .map_err(|_| Error::new(Failure::Protocol, "认证编码失败"))?,
                )
                .map_err(Error::from)?;
        }
        Ok(())
    }
    fn shared_client(&self, mode: Mode) -> Result<session::SharedSchool> {
        let gateway_seed = self.secrets.lock().unwrap().vpn.clone();
        if let Some(shared) = self.shared_sessions.lock().unwrap().get_mut(&mode) {
            if mode == Mode::Webvpn && shared.gateway_seed != gateway_seed {
                session::merge(&shared.client.jar, session::jar(&gateway_seed)?.as_ref());
                shared.gateway_seed = gateway_seed;
            }
            return Ok(shared.clone());
        }
        let mut client = client::Client::new(&self.secrets.lock().unwrap(), mode, false)?;
        let generation = self.session_serial.fetch_add(1, Ordering::SeqCst) + 1;
        client.generation = generation;
        let shared = session::SharedSchool {
            client,
            generation,
            gateway_seed,
            logged_out: false,
        };
        Ok(self
            .shared_sessions
            .lock()
            .unwrap()
            .entry(mode)
            .or_insert(shared)
            .clone())
    }
    fn client(&self, fresh: bool) -> Result<client::Client> {
        let mode = self.snapshot().mode;
        if fresh {
            let generation = self.shared_client(mode)?.generation;
            let mut client = client::Client::new(&self.secrets.lock().unwrap(), mode, true)?;
            client.generation = generation;
            Ok(client)
        } else {
            Ok(self.shared_client(mode)?.client)
        }
    }
    fn record_admission_error(&self, error: &Error) -> Result<()> {
        self.state.lock().unwrap().admission_failure(error);
        if error.kind == Failure::AdmissionRestricted && self.snapshot().running {
            self.pause("当前选课准入受限，已暂停任务");
        }
        self.persist()?;
        self.publish();
        Ok(())
    }
    fn require_admission(&self) -> Result<()> {
        let s = self.snapshot();
        if s.authentication != Authentication::Authenticated {
            return Err(Error::new(Failure::Session, "请先登录选课系统"));
        }
        if s.admission != Admission::Ready {
            let kind = match s.admission {
                Admission::Restricted => Failure::AdmissionRestricted,
                Admission::Initializing => Failure::Initializing,
                _ => Failure::Network,
            };
            return Err(Error::new(
                kind,
                if s.admission_message.is_empty() {
                    "尚未确认当前批次可选课".into()
                } else {
                    s.admission_message
                },
            ));
        }
        Ok(())
    }
    fn check_admission(&self, c: &client::Client, student: &str) -> Result<()> {
        if !self.snapshot().can_select() {
            self.refresh_context(c, student)?;
        }
        self.require_admission()
    }
    fn cache_snapshot(&self) -> Result<Snapshot> {
        let profile = self
            .credentials
            .load_profile()
            .map_err(Error::from)?
            .ok_or_else(|| Error::new(Failure::Session, "请先在应用中选择账户"))?;
        let live = self.snapshot();
        if live.context.student == profile.student_id {
            return Ok(live);
        }
        let mut cached: Snapshot = self
            .cache
            .lock()
            .unwrap()
            .get(&profile.student_id, "workspace", "latest")
            .map_err(Error::from)?
            .unwrap_or_default();
        cached.context.student = profile.student_id;
        cached.running = false;
        cached.logged_in = false;
        cached.authentication = Authentication::Unknown;
        cached.admission = Admission::Unchecked;
        Ok(cached)
    }
    fn login(&self) -> Result<()> {
        let profile = self.identity()?;
        if self.snapshot().mode == Mode::Webvpn
            && !session::gateway_valid(&self.secrets.lock().unwrap().vpn)?
        {
            return Err(Error::new(Failure::Needs2fa, "需2FA验证"));
        }
        let existing = self.client(false)?;
        if !existing.token.is_empty() {
            match existing.context(&profile.student_id) {
                Ok(_) => return self.refresh_context(&existing, &profile.student_id),
                Err(e) if e.kind == Failure::Session => {}
                Err(e) if e.kind == Failure::AdmissionRestricted => {
                    self.record_admission_error(&e)?;
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        }
        self.message("正在识别验证码并登录");
        let mut structural = 0;
        for _ in 0..5 {
            let mut c = self.client(true)?;
            let (token, bytes) = match c.captcha() {
                Ok(v) => v,
                Err(e) => {
                    if e.kind == Failure::Protocol {
                        structural += 1;
                        if structural < 3 {
                            continue;
                        }
                    }
                    return Err(e);
                }
            };
            let points = match captcha::recognize(&bytes) {
                Ok(p) => {
                    structural = 0;
                    p
                }
                Err(error) => {
                    if error == captcha::RecognitionError::ModelMissing {
                        return Err(Error::new(Failure::Protocol, "ocr_model_missing"));
                    }
                    if error == captcha::RecognitionError::Structure {
                        structural += 1;
                    } else {
                        structural = 0;
                    }
                    if structural >= 3 {
                        return Err(Error::new(
                            Failure::Protocol,
                            "验证码连续三次结构异常，请使用手动点选",
                        ));
                    }
                    *self.challenge.lock().unwrap() = Some((c, token));
                    continue;
                }
            };
            match c.login(&profile.student_id, &profile.password, &token, &points) {
                Ok(()) => {
                    self.save_client(&c)?;
                    return self.refresh_context(&c, &profile.student_id);
                }
                Err(e)
                    if e.kind == Failure::Business
                        && ["验证码", "校验码", "verifyCode", "vcode"]
                            .iter()
                            .any(|part| e.message.contains(part)) =>
                {
                    continue
                }
                Err(e) => return Err(e),
            }
        }
        Err(Error::new(
            Failure::Business,
            "自动识别未成功，请使用手动点选验证码",
        ))
    }
    fn refresh_context(&self, c: &client::Client, student: &str) -> Result<()> {
        // Login is already saved even if the school is still initializing.
        let mut context = match c.context(student) {
            Ok(v) => v,
            Err(e) => {
                self.record_admission_error(&e)?;
                return if matches!(e.kind, Failure::Session | Failure::Needs2fa) {
                    Err(e)
                } else {
                    Ok(())
                };
            }
        };
        {
            let mut state = self.state.lock().unwrap();
            state.authenticated();
            state.admission = Admission::Ready;
            state.admission_message.clear();
        }
        let old = self.snapshot().context;
        if old.student == context.student && !old.batch.is_empty() && old.term == context.term {
            if let Ok(options) = c.options() {
                if options["batches"].as_array().is_some_and(|rows| {
                    rows.iter().any(|b| protocol::text(&b["code"]) == old.batch)
                }) {
                    context.batch = old.batch;
                    context.campus = old.campus;
                }
            }
        }
        let mut s = self.state.lock().unwrap();
        if !s.context.batch.is_empty() && s.context.batch != context.batch {
            s.running = false;
            for t in &mut s.tasks {
                t.enabled = false;
            }
            s.courses.clear();
            s.selected.clear();
            s.complete = false;
        }
        s.context = context;
        s.message.clear();
        drop(s);
        let _ = contract::discover(c);
        self.persist()?;
        self.publish();
        Ok(())
    }
    fn reconcile(&self, c: &client::Client) -> Result<()> {
        let s = self.snapshot();
        let selected = c.selected(&s.context)?;
        // A shared official page may overwrite the school's uncorrelated process result.
        // Only selected membership or this request's own response is authoritative then.
        let process = if !self.official_windows.lock().unwrap().contains_key(&s.mode)
            && s.tasks
                .iter()
                .any(|t| t.pending.as_ref().is_some_and(|p| p.accepted))
        {
            c.process().err()
        } else {
            None
        };
        let mut s = self.state.lock().unwrap();
        for task in &mut s.tasks {
            reconcile_membership(task, &selected);
        }
        if let Some(e) = process {
            if matches!(
                e.kind,
                Failure::Full | Failure::Business | Failure::Permanent
            ) {
                for t in &mut s.tasks {
                    if t.pending.as_ref().is_some_and(|p| p.accepted) {
                        t.rejected(
                            e.kind == Failure::Full,
                            e.kind == Failure::Permanent,
                            e.message.clone(),
                        );
                    }
                }
            }
        }
        let groups: Vec<_> = s
            .tasks
            .iter()
            .filter(|t| t.phase == Phase::Completed)
            .filter_map(|t| t.group.clone())
            .collect();
        for t in &mut s.tasks {
            if t.group.as_ref().is_some_and(|g| groups.contains(g))
                && t.phase != Phase::Completed
                && t.pending.is_none()
            {
                t.enabled = false;
                t.phase = Phase::Stopped;
                t.message = "备选组已有课程选中".into();
            }
        }
        s.selected = selected;
        drop(s);
        self.persist()?;
        self.publish();
        Ok(())
    }
    fn tick(&self) -> Result<()> {
        if !self.snapshot().running {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp_millis();
        if now < self.next_tick.load(Ordering::SeqCst) {
            return Ok(());
        }

        let Ok(_operation) = self.operation.try_lock() else {
            return Ok(());
        };
        self.next_tick.store(now + 250, Ordering::SeqCst);
        let profile = match self.identity() {
            Ok(p) => p,
            Err(e) => {
                self.pause(&e.message);
                return Err(e);
            }
        };
        let s = self.snapshot();
        if !s.running {
            return Ok(());
        }
        if !s.tasks.iter().any(Task::active) {
            self.pause("本轮任务已结束，请查看各课程结果");
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp_millis();
        let mut candidates: Vec<_> = s
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.active() && t.next_at <= now && t.started_at.is_none_or(|at| at <= now)
            })
            .collect();
        let eligible = candidates.clone();
        candidates.retain(|(index, t)| {
            t.group.as_ref().is_none_or(|group| {
                !eligible.iter().any(|(other, c)| {
                    other != index
                        && c.group.as_ref() == Some(group)
                        && (c.attempts, std::cmp::Reverse(c.priority), *other)
                            < (t.attempts, std::cmp::Reverse(t.priority), *index)
                })
            })
        });
        candidates.sort_by_key(|(_, t)| {
            (
                !matches!(
                    t.phase,
                    Phase::Restoring | Phase::Dropping | Phase::Selecting
                ),
                t.pending.is_none(),
                std::cmp::Reverse(t.priority),
                t.next_at,
            )
        });
        let Some((index, _)) = candidates.first() else {
            return Ok(());
        };
        let index = *index;
        let c = self.client(false)?;
        self.check_admission(&c, &profile.student_id)?;
        if c.token.is_empty() {
            self.pause("请先登录选课系统");
            return Ok(());
        }
        if s.context.batch.is_empty() {
            self.refresh_context(&c, &profile.student_id)?;
            return Ok(());
        }
        if s.tasks.iter().any(|t| t.pending.is_some()) {
            self.next_tick.store(now + 1000, Ordering::SeqCst);
            let outcome = self.reconcile(&c);
            // A successful read that still disagrees with a write is not proof of
            // rejection: the school may still be processing it. Never resubmit.
            let mut state = self.state.lock().unwrap();
            for t in &mut state.tasks {
                if t.pending.is_some() {
                    t.next_at = now + 1000;
                    t.message = "等待学校确认选退课结果；未重复提交".into();
                }
            }
            drop(state);
            return outcome;
        }
        if self.official_windows.lock().unwrap().contains_key(&s.mode) {
            self.reconcile(&c)?;
            if !self.snapshot().tasks[index].active() {
                return Ok(());
            }
        }
        let mut task = self.snapshot().tasks[index].clone();
        if task.phase == Phase::Monitoring {
            match c.capacity(&s.context, &task.course) {
                Ok(n) if n >= task.threshold as i64 => {
                    let selected = c.selected(&s.context)?;
                    if selected.iter().any(|c| c.id == task.course.id) {
                        task.phase = Phase::Completed;
                        task.enabled = false;
                        self.state.lock().unwrap().tasks[index] = task;
                        self.persist()?;
                        return Ok(());
                    }
                    if !task
                        .old_course
                        .as_ref()
                        .is_some_and(|old| selected.iter().any(|c| c.id == old.id))
                    {
                        task.enabled = false;
                        task.message = "旧课当前未选中，已暂停换课".into();
                        self.state.lock().unwrap().tasks[index] = task;
                        self.persist()?;
                        return Ok(());
                    }
                    task.phase = Phase::Dropping;
                }
                Ok(_) => {
                    self.state.lock().unwrap().tasks[index].next_at =
                        now + task.gear.interval() as i64;
                    return Ok(());
                }
                Err(e) => {
                    self.state.lock().unwrap().tasks[index].next_at = now + 10000;
                    return Err(e);
                }
            }
        }
        let dropping = task.phase == Phase::Dropping;
        let course = if matches!(task.phase, Phase::Dropping | Phase::Restoring) {
            task.old_course
                .clone()
                .ok_or_else(|| Error::new(Failure::Protocol, "换课缺少旧课信息"))?
        } else {
            task.course.clone()
        };
        if task.phase == Phase::Pending {
            task.phase = Phase::Selecting;
        }
        task.attempts = task.attempts.saturating_add(1);
        task.pending = Some(PendingWrite {
            class_id: course.id.clone(),
            dropping,
            phase: task.phase,
            sent_at: now,
            accepted: false,
        });
        task.next_at = now + task.gear.interval() as i64;
        self.state.lock().unwrap().tasks[index] = task;
        if let Err(e) = self.persist() {
            let mut s = self.state.lock().unwrap();
            s.tasks[index].pending = None;
            s.tasks[index].enabled = false;
            drop(s);
            self.pause("缓存写入失败，未发送选退课请求");
            return Err(e);
        } // durable intent before network write
        self.next_tick.store(
            now + s.tasks[index].gear.interval() as i64,
            Ordering::SeqCst,
        );
        if self
            .credentials
            .load_profile()
            .map_err(Error::from)?
            .as_ref()
            .is_none_or(|p| p.student_id != profile.student_id)
        {
            self.state.lock().unwrap().tasks[index].pending = None;
            self.pause("账户已切换，停止选退课");
            return Ok(());
        }
        let outcome = c.submit(&s.context, &course, dropping);
        let outcome = match outcome {
            Err(e) if e.kind == Failure::Forbidden => match c.context(&profile.student_id) {
                Err(probe) if probe.kind == Failure::Session => Err(probe),
                _ => Err(Error::new(
                    Failure::Throttled,
                    "访问被拒绝但未确认会话失效，等待后重试",
                )),
            },
            other => other,
        };
        match outcome {
            Ok(_) => {
                if let Some(p) = self.state.lock().unwrap().tasks[index].pending.as_mut() {
                    p.accepted = true;
                }
                self.persist()?;
                self.reconcile(&c)?;
                if matches!(
                    self.snapshot().tasks[index].phase,
                    Phase::Selecting | Phase::Restoring
                ) && self.snapshot().tasks[index].pending.is_none()
                {
                    self.next_tick.store(now + 250, Ordering::SeqCst);
                }
            }
            Err(e) => {
                if matches!(
                    e.kind,
                    Failure::Business | Failure::Full | Failure::Permanent
                ) {
                    if let Ok(selected) = c.selected(&s.context) {
                        let achieved = selected.iter().any(|v| v.id == course.id) != dropping;
                        if achieved {
                            let mut state = self.state.lock().unwrap();
                            state.tasks[index].confirmed(!dropping);
                            for task in &mut state.tasks {
                                reconcile_membership(task, &selected);
                            }
                            state.selected = selected;
                            drop(state);
                            self.persist()?;
                            self.publish();
                            return Ok(());
                        }
                    }
                }
                let mut s = self.state.lock().unwrap();
                let task = &mut s.tasks[index];
                match e.kind {
                    Failure::Full | Failure::Permanent | Failure::Business => task.rejected(
                        e.kind == Failure::Full,
                        e.kind == Failure::Permanent,
                        e.message.clone(),
                    ),
                    Failure::Session | Failure::Needs2fa => {
                        task.pending = None;
                        task.failed();
                        task.message = e.message.clone();
                        task.next_at = now + 10000;
                    }
                    Failure::Initializing | Failure::Throttled => {
                        task.pending = None;
                        task.message = e.message.clone();
                        task.next_at = now + 10000;
                    }
                    _ => {
                        task.message = "请求结果不确定，先核验已选列表".into();
                        task.next_at = now + 1000;
                    }
                }
                drop(s);
                self.persist()?;
                self.publish();
                if self.snapshot().tasks[index].phase == Phase::Restoring
                    && self.snapshot().tasks[index].pending.is_none()
                {
                    self.next_tick.store(now + 250, Ordering::SeqCst);
                }
                return Err(e);
            }
        }
        self.persist()?;
        self.publish();
        if !self.snapshot().tasks.iter().any(Task::active) {
            self.pause("本轮任务已结束，请查看各课程结果");
        }
        Ok(())
    }
    fn pause(&self, message: &str) {
        self.resume_after_auth.store(false, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        let mut s = self.state.lock().unwrap();
        s.running = false;
        s.message = message.into();
        drop(s);
        let _ = self.persist();
        let _ = platform::running(0, self.generation.load(Ordering::SeqCst));
        self.publish();
        use tauri_plugin_notification::NotificationExt;
        let _ = self
            .app
            .notification()
            .builder()
            .title("选课任务")
            .body(message)
            .show();
    }
    fn resume_authenticated(&self) -> Result<()> {
        if !self.snapshot().can_select() {
            return Ok(());
        }
        if self.resume_after_auth.swap(false, Ordering::SeqCst) {
            self.state.lock().unwrap().running = true;
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            platform::running(
                self.snapshot().tasks.iter().filter(|t| t.active()).count(),
                generation,
            )
            .map_err(Error::from)?;
        }
        Ok(())
    }
    pub(crate) fn execute(&self, action: commands::Action) -> Result<Value> {
        use commands::Action;
        match &action {
            Action::CaptchaCancel {
                flow,
                revision,
                discard,
            } => return self.cancel_captcha(flow, *revision, *discard),
            Action::CaptchaPrepare { flow, revision } => {
                return self.prepare_captcha(flow, *revision)
            }
            Action::CaptchaPoint {
                flow,
                revision,
                challenge_id,
                points,
            } => return self.captcha_point(flow, *revision, challenge_id, points),
            Action::CaptchaSubmit {
                flow,
                revision,
                challenge_id,
                points,
            } => return self.submit_captcha(flow, *revision, challenge_id, points),
            _ => {}
        }
        if matches!(&action, Action::CacheSnapshot) {
            return Ok(json!(self.cache_snapshot()?));
        }
        if let Action::Cache { query } = &action {
            let snapshot = self.cache_snapshot()?;
            return self
                .cache
                .lock()
                .unwrap()
                .catalog(&snapshot.context.student, &snapshot.context, query)
                .map_err(Error::from);
        }
        if let Action::BatchInfo { mode } | Action::OnlineUsers { mode } = &action {
            let profile = self
                .credentials
                .load_profile()
                .map_err(Error::from)?
                .ok_or_else(|| Error::new(Failure::Session, "请先登录应用账户"))?;
            let secrets = self.secrets.lock().unwrap();
            if secrets.student != profile.student_id {
                return Err(Error::new(Failure::Session, "账户信息正在更新"));
            }
            let c = client::Client::new(&secrets, *mode, false)?;
            drop(secrets);
            if *mode == Mode::Webvpn && session::gateway_only(&c.jar)?.is_empty() {
                return Err(Error::new(Failure::Needs2fa, "需2FA验证"));
            }
            let online = matches!(&action, Action::OnlineUsers { .. });
            return c.quiet_post(
                if online {
                    "publicinfo/onlineUsers.do"
                } else {
                    "elective/batch.do"
                },
                &[],
                if online { 3 } else { 5 },
            );
        }
        let _operation = self.operation.lock().unwrap();
        let profile = self.identity()?;
        match action {
            Action::CaptchaCancel { .. }
            | Action::CaptchaPrepare { .. }
            | Action::CaptchaPoint { .. }
            | Action::CaptchaSubmit { .. } => unreachable!(),
            Action::Snapshot => {}
            Action::CacheSnapshot
            | Action::Cache { .. }
            | Action::BatchInfo { .. }
            | Action::OnlineUsers { .. } => unreachable!(),
            Action::Session => {
                let c = self.client(false)?;
                if c.token.is_empty() {
                    let mut s = self.state.lock().unwrap();
                    s.logged_in = false;
                    s.authentication = Authentication::Unknown;
                    s.admission = Admission::Unchecked;
                    drop(s);
                    self.publish();
                    return Ok(json!({"status":"required","snapshot":self.snapshot()}));
                }
                let _ = self.refresh_context(&c, &profile.student_id);
                let s = self.snapshot();
                let status = if s.authentication == Authentication::Expired {
                    "required"
                } else {
                    match s.admission {
                        Admission::Ready => "ready",
                        Admission::Restricted => "restricted",
                        Admission::Initializing => "initializing",
                        _ => "unavailable",
                    }
                };
                return Ok(json!({"status":status,"message":s.admission_message,"snapshot":s}));
            }
            Action::Options => return self.client(false)?.options(),
            Action::Context { batch, campus } => {
                let old = self.snapshot();
                if old.tasks.iter().any(|t| t.pending.is_some()) {
                    return Err(Error::new(Failure::Business, "请先核验未决选退课"));
                }
                let options = self.client(false)?.options()?;
                let selected = options["batches"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|b| protocol::text(&b["code"]) == batch))
                    .ok_or_else(|| Error::new(Failure::Protocol, "批次不在学校返回的可用列表中"))?;
                if !options["campuses"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|c| protocol::text(&c["code"]) == campus))
                {
                    return Err(Error::new(Failure::Protocol, "校区无效"));
                }
                self.pause("批次或校区已切换，任务暂停");
                let context = Context {
                    student: profile.student_id.clone(),
                    term: protocol::text(&selected["schoolTerm"]),
                    batch,
                    campus,
                };
                let scope = serde_json::to_string(&context).unwrap();
                let mut next: Snapshot = self
                    .cache
                    .lock()
                    .unwrap()
                    .get(&profile.student_id, "workspace", &scope)
                    .map_err(Error::from)?
                    .unwrap_or_default();
                next.context = context;
                next.mode = old.mode;
                next.logged_in = old.logged_in;
                next.authentication = old.authentication;
                next.admission = Admission::Unchecked;
                next.running = false;
                next.academic = old.academic;
                *self.state.lock().unwrap() = next;
            }
            Action::Labs { course } => {
                return Ok(json!(self
                    .client(false)?
                    .labs(&self.snapshot().context, &course)?))
            }
            Action::Login => {
                self.login()?;
                self.resume_authenticated()?;
            }
            Action::Captcha => {
                use base64::Engine;
                let c = self.client(true)?;
                let (token, bytes) = c.captcha()?;
                *self.challenge.lock().unwrap() = Some((c, token));
                return Ok(
                    json!({"image":format!("data:image/jpeg;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}),
                );
            }
            Action::Solve { points } => {
                let (mut c, token) = self
                    .challenge
                    .lock()
                    .unwrap()
                    .take()
                    .ok_or_else(|| Error::new(Failure::Protocol, "请先刷新验证码"))?;
                c.login(&profile.student_id, &profile.password, &token, &points)?;
                self.save_client(&c)?;
                self.refresh_context(&c, &profile.student_id)?;
                self.resume_authenticated()?;
            }
            Action::Mode { mode } => {
                if self.snapshot().tasks.iter().any(|t| t.pending.is_some()) {
                    self.reconcile(&self.client(false)?)?;
                    if self.snapshot().tasks.iter().any(|t| t.pending.is_some()) {
                        return Err(Error::new(
                            Failure::Business,
                            "存在未确认选退课，暂不能切换模式",
                        ));
                    }
                }
                self.pause("访问模式已切换，请验证选课登录");
                self.state.lock().unwrap().mode = mode;
                {
                    let mut s = self.state.lock().unwrap();
                    s.logged_in = false;
                    s.authentication = Authentication::Unknown;
                    s.admission = Admission::Unchecked;
                    s.admission_message.clear();
                }
                *self.challenge.lock().unwrap() = None;
            }
            Action::Probe => {
                let vpn = self.secrets.lock().unwrap().vpn.clone();
                let probes = std::thread::scope(|scope| {
                    let a = scope.spawn(|| probe(Mode::Campus, ""));
                    let b = scope.spawn(|| probe(Mode::Webvpn, &vpn));
                    vec![a.join().unwrap(), b.join().unwrap()]
                });
                return Ok(json!(probes));
            }
            Action::Refresh { query } => {
                let c = self.client(false)?;
                self.check_admission(&c, &profile.student_id)?;
                if self.snapshot().context.batch.is_empty() {
                    self.refresh_context(&c, &profile.student_id)?;
                }
                let context = self.snapshot().context;
                let scope = serde_json::to_string(&(context.clone(), query.clone())).unwrap();
                if let Some((courses, complete, updated)) = self
                    .cache
                    .lock()
                    .unwrap()
                    .get::<(Vec<Course>, bool, i64)>(&profile.student_id, "catalog", &scope)
                    .map_err(Error::from)?
                {
                    let mut s = self.state.lock().unwrap();
                    s.courses = courses;
                    s.complete = complete;
                    s.updated_at = Some(updated);
                    s.query = query.clone();
                    drop(s);
                    self.publish();
                }
                let (courses, complete) = c.catalog(&context, &query)?;
                let now = chrono::Utc::now().timestamp_millis();
                self.cache
                    .lock()
                    .unwrap()
                    .put(
                        &profile.student_id,
                        "catalog",
                        &scope,
                        &(&courses, complete, now),
                    )
                    .map_err(Error::from)?;
                let mut s = self.state.lock().unwrap();
                s.courses = courses;
                s.complete = complete;
                s.updated_at = Some(now);
                s.query = query;
                s.message.clear();
            }
            Action::Selected => {
                let c = self.client(false)?;
                self.check_admission(&c, &profile.student_id)?;
                self.reconcile(&c)?;
            }
            Action::Pin { id, pinned } => {
                let mut s = self.state.lock().unwrap();
                s.pins.retain(|v| v != &id);
                if pinned {
                    s.pins.push(id);
                }
            }
            Action::Add { input } => {
                self.require_admission()?;
                if input.one_shot && input.old_course.is_some() {
                    return Err(Error::new(Failure::Protocol, "立即选课不能包含自动换课"));
                }
                if input.one_shot {
                    self.reconcile(&self.client(false)?)?;
                }

                if input.course.id.is_empty() || input.threshold == 0 || input.threshold > 10000 {
                    return Err(Error::new(Failure::Protocol, "任务参数无效"));
                }
                if input.old_course.is_some() && input.group.as_ref().is_some_and(|g| !g.is_empty())
                {
                    return Err(Error::new(
                        Failure::Protocol,
                        "自动换课任务不能加入普通备选组",
                    ));
                }
                if input.old_course.is_some() && !input.risk_confirmed {
                    return Err(Error::new(Failure::Permanent, "请确认自动换课退旧风险"));
                }
                if input
                    .old_course
                    .as_ref()
                    .is_some_and(|c| c.id == input.course.id)
                {
                    return Err(Error::new(Failure::Protocol, "新旧教学班不能相同"));
                }
                let mut s = self.state.lock().unwrap();
                if s.context.batch.is_empty() {
                    return Err(Error::new(Failure::Session, "请先登录并获取选课批次"));
                }
                if s.selected.iter().any(|c| c.id == input.course.id) {
                    s.message = "该教学班已经选上".into();
                    drop(s);
                    self.persist()?;
                    self.publish();
                    return Ok(json!(self.snapshot()));
                }
                if s.tasks
                    .iter()
                    .any(|t| t.active() && t.course.id == input.course.id)
                {
                    return Err(Error::new(Failure::Business, "该课程已有待抢任务"));
                }
                if input.one_shot && !s.running {
                    for t in &mut s.tasks {
                        t.enabled = false;
                    }
                }
                s.tasks.push(Task {
                    id: uuid::Uuid::new_v4().to_string(),
                    phase: if input.old_course.is_some() {
                        Phase::Monitoring
                    } else {
                        Phase::Pending
                    },
                    course: input.course,
                    old_course: input.old_course,
                    threshold: input.threshold,
                    group: input.group.filter(|g| !g.is_empty()),
                    priority: input.priority,
                    gear: input.gear,
                    failures: 0,
                    attempts: 0,
                    message: String::new(),
                    next_at: 0,
                    started_at: input.start_at,
                    enabled: true,
                    pending: None,
                    one_shot: input.one_shot,
                    goal_observed: false,
                });
                if input.one_shot {
                    s.running = true;
                    let count = s.tasks.iter().filter(|t| t.active()).count();
                    drop(s);
                    let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Err(e) = platform::running(count, generation) {
                        self.pause(&e);
                        return Err(Error::from(e));
                    }
                }
            }
            Action::Control {
                id,
                action,
                gear,
                priority,
            } => {
                if action == "remove" {
                    let id =
                        id.ok_or_else(|| Error::new(Failure::Protocol, "请选择要移除的任务"))?;
                    let mut s = self.state.lock().unwrap();
                    if s.tasks.iter().any(|t| {
                        t.id == id
                            && (t.pending.is_some()
                                || (t.old_course.is_some()
                                    && matches!(
                                        t.phase,
                                        Phase::Dropping | Phase::Selecting | Phase::Restoring
                                    )))
                    }) {
                        return Err(Error::new(
                            Failure::Business,
                            "请先核验或停止正在换课的任务",
                        ));
                    }
                    s.tasks.retain(|t| t.id != id);
                } else if action == "pause" {
                    self.pause("任务已暂停");
                } else if action == "start" {
                    let c = self.client(false)?;
                    self.refresh_context(&c, &profile.student_id)?;
                    self.require_admission()?;
                    self.reconcile(&c)?;
                    let mut s = self.state.lock().unwrap();
                    if s.tasks.iter().any(|t| t.pending.is_some()) {
                        return Err(Error::new(
                            Failure::Business,
                            "请先核实未决操作，不会自动重新提交",
                        ));
                    }
                    for t in &mut s.tasks {
                        if id.as_ref().is_none_or(|id| id == &t.id)
                            && !matches!(t.phase, Phase::Completed | Phase::Stopped)
                        {
                            t.enabled = true;
                        }
                    }
                    s.running = true;
                    let count = s.tasks.iter().filter(|t| t.active()).count();
                    drop(s);
                    let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Err(e) = platform::running(count, generation) {
                        self.pause(&e);
                        return Err(Error::from(e));
                    }
                } else {
                    let mut s = self.state.lock().unwrap();
                    for t in &mut s.tasks {
                        if id.as_ref().is_none_or(|id| id == &t.id) {
                            if action == "stop" {
                                t.enabled = false;
                                if t.pending.is_none() {
                                    t.phase = Phase::Stopped;
                                }
                            }
                            if let Some(gear) = gear {
                                t.switch_gear(gear);
                            }
                            if let Some(priority) = priority {
                                t.priority = priority;
                            }
                        }
                    }
                }
            }
            Action::Drop { course, confirmed } => {
                self.require_admission()?;
                if !confirmed {
                    return Err(Error::new(Failure::Permanent, "请确认退课"));
                }
                if self.snapshot().tasks.iter().any(|t| t.pending.is_some()) {
                    return Err(Error::new(Failure::Business, "请先核验未决选退课"));
                }
                let context = self.snapshot().context;
                let c = self.client(false)?;
                if !c
                    .selected(&context)?
                    .iter()
                    .any(|selected| selected.id == course.id)
                {
                    self.state.lock().unwrap().message = "该教学班已退课，无需重复提交".into();
                    self.reconcile(&c)?;
                    return Ok(json!(self.snapshot()));
                }
                self.pause("正在退课");
                let task = Task {
                    id: uuid::Uuid::new_v4().to_string(),
                    course: course.clone(),
                    old_course: None,
                    threshold: 2,
                    group: None,
                    priority: 0,
                    gear: Gear::Medium,
                    failures: 0,
                    attempts: 0,
                    phase: Phase::Selecting,
                    message: "手动退课待核验".into(),
                    next_at: 0,
                    started_at: None,
                    enabled: true,
                    pending: Some(PendingWrite {
                        class_id: course.id.clone(),
                        dropping: true,
                        phase: Phase::Selecting,
                        sent_at: chrono::Utc::now().timestamp_millis(),
                        accepted: false,
                    }),
                    one_shot: true,
                    goal_observed: false,
                };
                {
                    let mut s = self.state.lock().unwrap();
                    for t in &mut s.tasks {
                        t.enabled = false;
                    }
                    s.tasks.push(task);
                    s.running = true;
                }
                self.persist()?;
                let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                if let Err(e) = platform::running(1, generation) {
                    if let Some(t) = self.state.lock().unwrap().tasks.last_mut() {
                        t.pending = None;
                        t.enabled = false;
                    }
                    self.pause(&e);
                    return Err(Error::from(e));
                }
                match c.submit(&context, &course, true) {
                    Ok(_) => {
                        if let Some(p) = self
                            .state
                            .lock()
                            .unwrap()
                            .tasks
                            .last_mut()
                            .and_then(|t| t.pending.as_mut())
                        {
                            p.accepted = true;
                        }
                    }
                    Err(e) => {
                        if matches!(
                            e.kind,
                            Failure::Business
                                | Failure::Full
                                | Failure::Permanent
                                | Failure::Session
                                | Failure::Forbidden
                                | Failure::Needs2fa
                        ) {
                            let mut s = self.state.lock().unwrap();
                            let t = s.tasks.last_mut().unwrap();
                            t.pending = None;
                            t.enabled = false;
                            t.phase = Phase::Stopped;
                            t.message = e.message.clone();
                        }
                        self.persist()?;
                        return Err(e);
                    }
                }
                self.persist()?;
                self.reconcile(&c)?;
            }
            Action::Academic { recalculate } => {
                let raw = self.secrets.lock().unwrap().ehall.clone();
                if raw.is_empty() {
                    return Err(Error::new(Failure::Session, "请先打开学业认证窗口"));
                }
                let context = self.secrets.lock().unwrap().academic_context.clone();
                let c = academic::Academic::new(&raw)?.with_context(context);
                if recalculate {
                    if self.snapshot().academic_recalculation.is_some() {
                        return Ok(json!({"calculating":true}));
                    }
                    self.state.lock().unwrap().academic_recalculation =
                        Some(chrono::Utc::now().timestamp_millis());
                    self.persist()?;
                    if let Err(e) = c.recalculate(&profile.student_id) {
                        if matches!(
                            e.kind,
                            Failure::Business | Failure::Permanent | Failure::Session
                        ) {
                            self.state.lock().unwrap().academic_recalculation = None;
                            self.persist()?;
                        }
                        return Err(e);
                    }
                    return Ok(json!({"calculating":true}));
                }
                let value = c.load(&profile.student_id)?;
                self.state.lock().unwrap().academic = Some(value);
            }
            Action::AcademicProgress => {
                let value = academic::Academic::new(&self.secrets.lock().unwrap().ehall)?
                    .progress(&profile.student_id)?;
                if let Some(row) = value["datas"]["byscjd"]["rows"]
                    .as_array()
                    .and_then(|r| r.first())
                {
                    let total = protocol::text(&row["ZS"]).parse::<u64>().unwrap_or(0);
                    let done = protocol::text(&row["YWCS"]).parse::<u64>().unwrap_or(0);
                    if total > 0 && done >= total {
                        self.state.lock().unwrap().academic_recalculation = None;
                        self.persist()?;
                        self.publish();
                    }
                }
                return Ok(value);
            }
        }
        if self.snapshot().running && !self.snapshot().tasks.iter().any(Task::active) {
            self.pause("任务已停止");
        }
        self.persist()?;
        self.publish();
        Ok(json!(self.snapshot()))
    }
}
fn probe(mode: Mode, vpn: &str) -> Probe {
    let start = std::time::Instant::now();
    let result = if mode == Mode::Webvpn {
        session::gateway_valid(vpn)
    } else {
        session::client(session::jar("").unwrap()).and_then(|c| {
            c.get(format!(
                "{}/xsxkapp/sys/xsxkapp/*default/index.do",
                mode.origin()
            ))
            .timeout(Duration::from_secs(5))
            .send()
            .map(|r| r.status().is_success())
            .map_err(|_| Error::new(Failure::Network, "连接超时"))
        })
    };
    match result {
        Ok(true) => Probe {
            mode,
            latency: Some(start.elapsed().as_millis() as u64),
            ..Default::default()
        },
        Ok(false) => Probe {
            mode,
            needs_2fa: mode == Mode::Webvpn,
            message: if mode == Mode::Webvpn {
                "需2FA验证"
            } else {
                "暂不可用"
            }
            .into(),
            ..Default::default()
        },
        Err(e) => Probe {
            mode,
            message: e.message,
            ..Default::default()
        },
    }
}
