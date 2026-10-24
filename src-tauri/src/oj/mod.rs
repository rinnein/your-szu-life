mod client;
pub(crate) mod commands;
mod companion;
pub(crate) mod db;
pub(crate) mod mcp;
pub(crate) mod model;
mod monitor;
mod persistence;
mod preferences;
mod protocol;
#[cfg(test)]
mod tests;
mod work;
use crate::{campus_network, ocr, storage::SharedCredentialStore};
use client::Client;
use model::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
use tauri::{Emitter, Manager};

pub(crate) struct OjStore {
    credentials: SharedCredentialStore,
    service: OnceLock<Arc<Service>>,
}
impl OjStore {
    pub fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials,
            service: OnceLock::new(),
        }
    }
}
struct Entry {
    client: Client,
    epoch: u64,
    username: String,
    gate: tokio::sync::Mutex<()>,
    state: Mutex<Status>,
}
#[derive(Default)]
struct Status {
    session: Session,
    message: String,
    captcha: Option<(String, String, String)>,
    last_sync: Option<i64>,
    next_sync: i64,
    failures: u32,
    offset: i64,
    progress: BTreeMap<String, (i64, Value)>,
    judged_progress: BTreeMap<String, i64>,
}
struct Runtime {
    data: Data,
    secrets: BTreeMap<String, Secret>,
    entries: BTreeMap<String, Arc<Entry>>,
    epoch: u64,
    companion_status: CompanionStatus,
    companion_requests: BTreeMap<String, i64>,
}
pub(crate) struct Service {
    app: Option<tauri::AppHandle>,
    credentials: SharedCredentialStore,
    path: PathBuf,
    runtime: Mutex<Runtime>,
    pub(crate) state_subscribers: crate::ipc_channel::SubscriberPool<OjDeltaEvent>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub(crate) enum OjDeltaEvent {
    Initial(Box<Snapshot>),
    Snapshot(Box<Snapshot>),
}
impl Service {
    fn new(
        app: Option<tauri::AppHandle>,
        credentials: SharedCredentialStore,
        path: PathBuf,
    ) -> Result<Arc<Self>, String> {
        let data = persistence::read(&path)?;
        let secrets: BTreeMap<String, Secret> = credentials
            .load_oj_accounts()?
            .map(|s| serde_json::from_str(&s).map_err(|_| "OJ 账号数据损坏"))
            .transpose()?
            .unwrap_or_default();
        let mut entries = BTreeMap::new();
        let mut epoch = chrono::Utc::now().timestamp_millis() as u64;
        for site in &data.sites {
            if let Some(s) = secrets
                .get(&site.id)
                .filter(|s| s.url == site.url && !s.username.is_empty() && !s.password.is_empty())
            {
                epoch += 1;
                entries.insert(
                    site.id.clone(),
                    Arc::new(Entry {
                        client: Client::new(&site.url, &s.cookies)?,
                        epoch,
                        username: s.username.clone(),
                        gate: tokio::sync::Mutex::new(()),
                        state: Mutex::new(Status::default()),
                    }),
                );
            }
        }
        Ok(Arc::new(Self {
            app,
            credentials,
            path,
            runtime: Mutex::new(Runtime {
                data,
                secrets,
                entries,
                epoch,
                companion_status: CompanionStatus::default(),
                companion_requests: BTreeMap::new(),
            }),
            state_subscribers: crate::ipc_channel::SubscriberPool::new(),
        }))
    }
    fn entry(&self, id: &str) -> Result<Arc<Entry>, String> {
        self.runtime
            .lock()
            .unwrap()
            .entries
            .get(id)
            .cloned()
            .ok_or("请先配置 OJ 账号密码".into())
    }
    fn current(&self, id: &str, entry: &Entry) -> bool {
        self.runtime
            .lock()
            .unwrap()
            .entries
            .get(id)
            .is_some_and(|e| e.epoch == entry.epoch)
            && !entry.client.cancel.is_cancelled()
    }
    fn publish(&self) {
        let snapshot = self.snapshot();
        self.state_subscribers.broadcast(&OjDeltaEvent::Snapshot(Box::new(snapshot.clone())));
        if let Some(app) = &self.app {
            let _ = app.emit("oj-state-changed", snapshot);
        }
    }
    pub(crate) fn snapshot(&self) -> Snapshot {
        let r = self.runtime.lock().unwrap();
        let mut sites: Vec<_> = r
            .data
            .sites
            .iter()
            .map(|site| {
                let entry = r.entries.get(&site.id);
                let s = entry.map(|e| e.state.lock().unwrap());
                SiteView {
                    site: site.clone(),
                    enabled: entry.is_some(),
                    username: entry.map(|e| e.username.clone()).unwrap_or_default(),
                    session_generation: entry.map(|e| e.epoch),
                    session: s.as_ref().map(|s| s.session).unwrap_or(Session::Disabled),
                    message: s.as_ref().map(|s| s.message.clone()).unwrap_or_default(),
                    last_sync: s.as_ref().and_then(|s| s.last_sync),
                }
            })
            .collect();
        sites.sort_by_key(|s| !s.enabled);
        Snapshot {
            sites,
            notices: r.data.notices.clone(),
            companion_port: r.data.companion_port,
            desktop: !cfg!(mobile),
            companion_enabled: r.data.companion_enabled,
            companion_status: r.companion_status.clone(),
        }
    }
    fn persist(&self, r: &Runtime) -> Result<(), String> {
        persistence::write(&self.path, &r.data)
    }
    fn log_request(
        &self,
        stage: &str,
        method: &str,
        path: &str,
        status: Option<u16>,
        elapsed_ms: u64,
        message: impl Into<String>,
    ) {
        let Some(app) = &self.app else { return };
        let Some(store) = app.try_state::<campus_network::CampusNetworkStore>() else {
            return;
        };
        campus_network::diagnostics::record(
            app,
            &store,
            campus_network::model::DebugLogEntry {
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                level: if status.is_some_and(|value| value >= 400) {
                    "warn"
                } else {
                    "info"
                }
                .into(),
                source: "oj".into(),
                stage: stage.into(),
                http_status: status,
                elapsed_ms: Some(elapsed_ms),
                error_kind: status.filter(|value| *value >= 400).map(|_| "http".into()),
                message: Some(format!("{method} {path}: {}", message.into())),
                ..Default::default()
            },
        );
    }
    async fn request(
        &self,
        id: &str,
        e: &Entry,
        stage: &str,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value, client::Error> {
        let started = Instant::now();
        let method_name = method.as_str().to_owned();
        let result = e.client.request(method, path, query, body).await;
        let (status, message) = match &result {
            Ok(_) => (Some(200), "请求成功".to_owned()),
            Err(error) => (error.status, error.message.clone()),
        };
        self.log_request(
            stage,
            &method_name,
            path,
            status,
            started.elapsed().as_millis() as u64,
            message,
        );
        let _ = id;
        result
    }
    fn save_site(&self, input: SiteInput) -> Result<String, String> {
        let url = client::normalize(&input.url)?.to_string();
        let name = input.name.trim();
        let username = input.username.trim();
        if name.is_empty()
            || name.len() > 200
            || username.len() > 200
            || input.password.as_ref().is_some_and(|p| p.len() > 4096)
        {
            return Err("名称或账号格式无效".into());
        }
        let mut r = self.runtime.lock().unwrap();
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if input.id.is_some() && !r.data.sites.iter().any(|s| s.id == id) {
            return Err("OJ 配置不存在".into());
        }
        if r.data.sites.iter().any(|s| s.id != id && s.url == url) {
            return Err("该 OJ 地址已经存在".into());
        }
        let old = r.data.sites.iter().find(|s| s.id == id);
        let builtin = old.is_some_and(|s| s.builtin);
        let changed = old.is_some_and(|s| s.url != url);
        let mut secrets = r.secrets.clone();
        let previous = secrets.get(&id);
        let password = input
            .password
            .filter(|s| !s.is_empty())
            .or_else(|| {
                previous
                    .filter(|s| s.url == url && s.username == username)
                    .map(|s| s.password.clone())
            })
            .unwrap_or_default();
        if changed && !input.clear_credentials && !username.is_empty() && password.is_empty() {
            return Err("地址已变化，请重新输入密码".into());
        }
        let secret = if !input.clear_credentials && !username.is_empty() && !password.is_empty() {
            Some(Secret {
                url: url.clone(),
                username: username.into(),
                password: password.clone(),
                cookies: String::new(),
            })
        } else {
            None
        };
        if !input.clear_credentials
            && (!username.is_empty() || !password.is_empty())
            && secret.is_none()
        {
            return Err("请同时填写账号和密码".into());
        }
        if let Some(s) = &secret {
            secrets.insert(id.clone(), s.clone());
        } else {
            secrets.remove(&id);
        }
        let client = secret.as_ref().map(|_| Client::new(&url, "")).transpose()?;
        let mut data = r.data.clone();
        if changed {
            data.preferences.remove(&id);
        }
        let site = Site {
            id: id.clone(),
            name: name.into(),
            url,
            builtin,
            reminders: input.reminders,
        };
        if let Some(old) = data.sites.iter_mut().find(|s| s.id == id) {
            *old = site
        } else {
            data.sites.push(site)
        }
        self.credentials
            .save_oj_accounts(&serde_json::to_string(&secrets).map_err(|_| "账号序列化失败")?)?;
        persistence::write(&self.path, &data)?;
        if let Some(e) = r.entries.remove(&id) {
            e.client.cancel.cancel()
        }
        r.epoch += 1;
        if let (Some(client), Some(secret)) = (client, secret) {
            let epoch = r.epoch;
            r.entries.insert(
                id.clone(),
                Arc::new(Entry {
                    client,
                    epoch,
                    username: secret.username,
                    gate: tokio::sync::Mutex::new(()),
                    state: Mutex::new(Status::default()),
                }),
            );
        }
        r.secrets = secrets;
        r.data = data;
        drop(r);
        self.cancel_notifications(&id);
        self.publish();
        Ok(id)
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        let mut r = self.runtime.lock().unwrap();
        let site = r
            .data
            .sites
            .iter()
            .find(|s| s.id == id)
            .ok_or("OJ 不存在")?;
        if site.builtin {
            return Err("内置 OJ 请使用清除账号停用".into());
        }
        let mut secrets = r.secrets.clone();
        secrets.remove(id);
        self.credentials
            .save_oj_accounts(&serde_json::to_string(&secrets).unwrap())?;
        if let Some(e) = r.entries.remove(id) {
            e.client.cancel.cancel()
        };
        r.secrets = secrets;
        r.data.sites.retain(|s| s.id != id);
        r.data.preferences.remove(id);
        r.data.notices.retain(|n| n.site_id != id);
        self.persist(&r)?;
        drop(r);
        self.cancel_notifications(id);
        self.publish();
        Ok(())
    }
    fn update_status(&self, id: &str, e: &Entry, session: Session, message: &str) {
        if !self.current(id, e) {
            return;
        }
        {
            let mut s = e.state.lock().unwrap();
            s.session = session;
            s.message = message.into();
        }
        self.publish();
    }
    fn save_cookies(&self, id: &str, e: &Entry) -> Result<(), String> {
        let cookies = e.client.cookies()?;
        let mut r = self.runtime.lock().unwrap();
        if r.entries
            .get(id)
            .is_none_or(|current| current.epoch != e.epoch)
        {
            return Err("账号已变化".into());
        }
        let mut secrets = r.secrets.clone();
        if let Some(secret) = secrets.get_mut(id) {
            secret.cookies = cookies;
        }
        self.credentials
            .save_oj_accounts(&serde_json::to_string(&secrets).unwrap())?;
        r.secrets = secrets;
        Ok(())
    }
    async fn challenge(&self, id: &str, e: &Entry) -> Result<Captcha, String> {
        let response = self
            .request(
                id,
                e,
                "login.captcha",
                reqwest::Method::POST,
                "captcha/",
                &[],
                Some(&json!({})),
            )
            .await
            .map_err(|e| e.display())?;
        let key = response["captcha_key"]
            .as_str()
            .ok_or("验证码响应不兼容")?
            .to_owned();
        let raw = response["captcha_image"].as_str().ok_or("验证码图片缺失")?;
        let image = if raw.starts_with("data:image/") {
            raw.into()
        } else {
            format!("data:image/png;base64,{raw}")
        };
        if image.len() > 2 * 1024 * 1024 {
            return Err("验证码图片过大".into());
        }
        let token = uuid::Uuid::new_v4().to_string();
        if !self.current(id, e) {
            return Err("账号已变化".into());
        }
        e.state.lock().unwrap().captcha = Some((token.clone(), key, image.clone()));
        Ok(Captcha { token, image })
    }
    async fn login(
        &self,
        id: &str,
        e: &Entry,
        manual: Option<(String, String)>,
    ) -> Result<(), String> {
        self.update_status(id, e, Session::Connecting, "正在连接 OJ");
        if manual.is_none() {
            if let Ok(profile) = self
                .request(
                    id,
                    e,
                    "login.check-session",
                    reqwest::Method::GET,
                    "user/profile/",
                    &[],
                    None,
                )
                .await
            {
                if profile["username"].as_str() == Some(&e.username) {
                    self.update_status(id, e, Session::Ready, "");
                    return Ok(());
                }
            }
        }
        let secret = self
            .runtime
            .lock()
            .unwrap()
            .secrets
            .get(id)
            .cloned()
            .ok_or("账号配置不存在")?;
        let mut body = json!({"username":secret.username,"password":secret.password});
        let supplied = if let Some((token, value)) = manual {
            if value.trim().is_empty() || value.len() > 32 {
                return Err("请填写验证码".into());
            }
            let mut state = e.state.lock().unwrap();
            let challenge = state.captcha.take().ok_or("验证码已失效，请刷新")?;
            if challenge.0 != token {
                return Err("验证码已变化，请重新填写".into());
            }
            Some((challenge.1, value))
        } else {
            let options = self
                .request(
                    id,
                    e,
                    "login.options",
                    reqwest::Method::GET,
                    "sys/options/",
                    &[],
                    None,
                )
                .await
                .map_err(|error| {
                    self.update_status(id, e, Session::NetworkError, &error.message);
                    error.display()
                })?;
            if options["enable_captcha"].as_bool().unwrap_or(true) {
                let c = self.challenge(id, e).await?;
                use base64::Engine;
                let raw = c.image.split_once(',').ok_or("验证码格式不支持")?.1;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(raw)
                    .map_err(|_| "验证码编码无效")?;
                let task = tauri::async_runtime::spawn_blocking(move || ocr::recognize(&bytes));
                let answer_result = tokio::time::timeout(std::time::Duration::from_secs(15), task)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_else(|| Err("captcha_recognition_failed".into()));
                if let Err(error) = &answer_result {
                    if error == "ocr_model_missing" || error == "ocr_model_invalid" {
                        return Err(error.clone());
                    }
                }
                let answer = answer_result
                    .ok()
                    .filter(|s| !s.is_empty() && s.len() <= 32);
                let Some(answer) = answer else {
                    self.update_status(
                        id,
                        e,
                        Session::CaptchaRequired,
                        "自动识别未完成，请手填验证码",
                    );
                    return Err("请手填验证码".into());
                };
                let key = e
                    .state
                    .lock()
                    .unwrap()
                    .captcha
                    .as_ref()
                    .map(|c| c.1.clone())
                    .ok_or("验证码已变化")?;
                Some((key, answer))
            } else {
                None
            }
        };
        if let Some((key, value)) = supplied {
            body["captcha_key"] = json!(key);
            body["captcha_value"] = json!(value)
        }
        if !self.current(id, e) {
            return Err("账号已变化".into());
        }
        match self
            .request(
                id,
                e,
                "login.submit",
                reqwest::Method::POST,
                "user/login/",
                &[],
                Some(&body),
            )
            .await
        {
            Ok(response) if response["id"].is_number() || response["id"].is_string() => {
                let profile = self
                    .request(
                        id,
                        e,
                        "login.verify",
                        reqwest::Method::GET,
                        "user/profile/",
                        &[],
                        None,
                    )
                    .await
                    .map_err(|er| {
                        self.update_status(id, e, Session::LoginFailed, "登录身份未确认，请重试");
                        er.display()
                    })?;
                if profile["username"].as_str() != Some(e.username.as_str()) {
                    self.update_status(id, e, Session::LoginFailed, "登录身份未确认");
                    return Err("登录身份未确认".into());
                }
                self.save_cookies(id, e)?;
                e.state.lock().unwrap().captcha = None;
                self.update_status(id, e, Session::Ready, "");
                Ok(())
            }
            result => {
                let message = result
                    .err()
                    .map(|e| e.display())
                    .unwrap_or("登录返回异常".into());
                let _ = self.challenge(id, e).await;
                self.update_status(
                    id,
                    e,
                    Session::CaptchaRequired,
                    "登录未完成，请检查账号密码并手填验证码",
                );
                Err(message)
            }
        }
    }
    async fn ensure(&self, id: &str, e: &Entry) -> Result<(), String> {
        let status = e.state.lock().unwrap().session;
        if status == Session::Ready {
            Ok(())
        } else if matches!(status, Session::CaptchaRequired | Session::LoginFailed) {
            Err("请先处理 OJ 登录".into())
        } else {
            self.login(id, e, None).await
        }
    }
    async fn read(
        &self,
        id: &str,
        e: &Entry,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, String> {
        match self
            .request(id, e, "read", reqwest::Method::GET, path, query, None)
            .await
        {
            Ok(v) => Ok(v),
            Err(error) if error.status == Some(401) => {
                self.update_status(id, e, Session::Disabled, "登录已过期");
                self.login(id, e, None).await?;
                self.request(id, e, "read.retry", reqwest::Method::GET, path, query, None)
                    .await
                    .map_err(|e| e.display())
            }
            Err(error) => Err(error.message),
        }
    }
    async fn list(
        &self,
        id: &str,
        e: &Entry,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Vec<Value>, String> {
        let started = Instant::now();
        let result = e.client.list(path, query).await;
        self.log_request(
            "list",
            "GET",
            path,
            result
                .as_ref()
                .err()
                .and_then(|error| error.status)
                .or_else(|| result.as_ref().ok().map(|_| 200)),
            started.elapsed().as_millis() as u64,
            result
                .as_ref()
                .err()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "请求成功".into()),
        );
        match result {
            Ok(rows) => Ok(rows),
            Err(error) if error.status == Some(401) => {
                self.update_status(id, e, Session::Disabled, "登录已过期");
                self.login(id, e, None).await?;
                e.client
                    .list(path, query)
                    .await
                    .map_err(|error| error.display())
            }
            Err(error) => Err(error.display()),
        }
    }
    async fn groups(&self, id: &str) -> Result<Vec<Group>, String> {
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.list(id, &e, "groups/", &[])
            .await?
            .iter()
            .map(protocol::group)
            .collect::<Result<_, _>>()
            .map_err(|e| e.message)
    }
    async fn contests(&self, id: &str, group: &str) -> Result<Vec<Contest>, String> {
        protocol::id(&json!(group)).map_err(|e| e.message)?;
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.list(id, &e, "contests/", &[("group", group.into())])
            .await?
            .iter()
            .map(protocol::contest)
            .collect::<Result<_, _>>()
            .map_err(|e| e.message)
    }
}
pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<OjStore>();
    let service = Service::new(
        Some(app.clone()),
        store.credentials.clone(),
        app.path()
            .app_data_dir()
            .map_err(|_| "OJ 数据目录不可用")?
            .join("oj/state.sqlite"),
    )?;
    store
        .service
        .set(service.clone())
        .map_err(|_| "OJ 已初始化")?;
    monitor::start(service);
    Ok(())
}
pub(crate) fn service(app: &tauri::AppHandle) -> Result<Arc<Service>, String> {
    app.state::<OjStore>()
        .service
        .get()
        .cloned()
        .ok_or("OJ 服务尚未初始化".into())
}
#[cfg(desktop)]
pub(crate) fn keep_running(app: &tauri::AppHandle) -> bool {
    service(app).is_ok_and(|s| {
        let r = s.runtime.lock().unwrap();
        (r.data.companion_enabled && !r.entries.is_empty())
            || r.data
                .sites
                .iter()
                .any(|site| site.reminders && r.entries.contains_key(&site.id))
    })
}
