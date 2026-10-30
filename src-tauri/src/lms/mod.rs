mod client;
pub(crate) mod commands;
pub(crate) mod db;
pub(crate) mod mcp;
pub(crate) mod model;
mod session;
#[cfg(test)]
mod tests;
use crate::storage::SharedCredentialStore;
use client::Client;
use model::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use tauri::Manager;

/// 待办/通知列表的本地缓存有效期。页面再次打开时先用缓存渲染，
/// 过期后才走网络；网络失败则继续使用旧缓存。
const LIST_CACHE_TTL_MS: i64 = 10 * 60 * 1000;

pub(crate) struct LmsStore {
    credentials: SharedCredentialStore,
    service: OnceLock<Arc<Service>>,
}
impl LmsStore {
    pub fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials,
            service: OnceLock::new(),
        }
    }
}

struct Runtime {
    session: SessionState,
    message: String,
    cache: Cache,
}

pub(crate) struct Service {
    app: Option<tauri::AppHandle>,
    credentials: SharedCredentialStore,
    path: PathBuf,
    runtime: Mutex<Runtime>,
    /// 串行化网络刷新，避免并发命令重复拉取。
    gate: tokio::sync::Mutex<()>,
}

impl Service {
    fn new(
        app: Option<tauri::AppHandle>,
        credentials: SharedCredentialStore,
        path: PathBuf,
    ) -> Result<Arc<Self>, String> {
        let cache = crate::migrations::db::run_sync(async {
            let database = db::LmsDatabase::open(&path).await?;
            database.load_cache().await
        })
        .map_err(|e| format!("无法读取畅课本地数据: {e}"))?;
        let session = if cache.fetched_at > 0 {
            SessionState::Ready
        } else {
            SessionState::NeedsAuth
        };
        Ok(Arc::new(Self {
            app,
            credentials,
            path,
            runtime: Mutex::new(Runtime {
                session,
                message: String::new(),
                cache,
            }),
            gate: tokio::sync::Mutex::new(()),
        }))
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let r = self.runtime.lock().unwrap();
        Snapshot {
            session: r.session,
            message: r.message.clone(),
            profile: r.cache.profile.clone(),
            todos: r.cache.todos.clone(),
            notifications: r.cache.notifications.clone(),
            unread_count: r.cache.unread_count,
            fetched_at: (r.cache.fetched_at > 0).then_some(r.cache.fetched_at),
        }
    }

    fn update(&self, session: SessionState, message: &str, cache: Option<Cache>) {
        let mut r = self.runtime.lock().unwrap();
        r.session = session;
        r.message = message.to_owned();
        if let Some(cache) = cache {
            r.cache = cache;
        }
    }

    /// 确保缓存数据可用：未过期直接返回；否则（重新）建立会话并拉取。
    /// 失败时只更新会话状态与错误信息，不清空已有缓存。
    pub(crate) async fn ensure(&self, force: bool) {
        if !force && !self.cache_expired() {
            return;
        }
        let _guard = self.gate.lock().await;
        // 双检：等锁期间其它调用可能已完成刷新。
        if !force && !self.cache_expired() {
            return;
        }
        let Some(client) = self.build_client() else {
            return;
        };
        self.refresh_with_client(&client).await;
    }

    fn cache_expired(&self) -> bool {
        let r = self.runtime.lock().unwrap();
        r.cache.fetched_at <= 0
            || chrono::Utc::now().timestamp_millis() - r.cache.fetched_at > LIST_CACHE_TTL_MS
            || r.session != SessionState::Ready
    }

    /// 用 vault 中的畅课会话与统一认证 SSO cookie 构建客户端。
    /// 统一会话不存在时把状态标记为 NeedsAuth 并返回 None。
    fn build_client(&self) -> Option<Client> {
        let cookies = self
            .credentials
            .load_lms_session()
            .map_err(|error| log::warn!("读取畅课会话失败: {error}"))
            .ok()
            .flatten();
        let sso_cookies = match &self.app {
            Some(app) => crate::auth::reusable_sso_cookies(app)
                .map_err(|message| {
                    self.update(SessionState::NeedsAuth, &message, None);
                })
                .ok()?,
            None => Vec::new(),
        };
        Some(
            Client::new(
                client::LMS_ORIGIN,
                client::AUTH_LOGIN,
                cookies.as_deref(),
                &sso_cookies,
            )
            .map_err(|message| self.update(SessionState::Unavailable, &message, None))
            .ok()?,
        )
    }

    /// 建立会话并拉取待办与通知，落盘缓存。测试可直接注入 mock 客户端。
    async fn refresh_with_client(&self, client: &Client) {
        let profile = match self.establish(client).await {
            Ok(profile) => profile,
            Err(BootstrapFailure::NeedsAuth(message)) => {
                self.update(SessionState::NeedsAuth, &message, None);
                return;
            }
            Err(BootstrapFailure::Error(message)) => {
                self.update(SessionState::Unavailable, &message, None);
                return;
            }
        };
        let todos = match self.fetch_todos(client).await {
            Ok(todos) => todos,
            Err(message) => {
                self.update(SessionState::Unavailable, &message, None);
                return;
            }
        };
        let notifications = match self.fetch_notifications(client, profile.uid).await {
            Ok(notifications) => notifications,
            Err(message) => {
                self.update(SessionState::Unavailable, &message, None);
                return;
            }
        };
        let cache = Cache {
            profile: Some(profile),
            todos,
            notifications: notifications.notifications,
            unread_count: notifications.unread_count,
            fetched_at: chrono::Utc::now().timestamp_millis(),
        };
        if let Err(error) = self.persist(&cache, client) {
            self.update(SessionState::Unavailable, &error, None);
            return;
        }
        self.update(SessionState::Ready, "", Some(cache));
    }

    /// 引导会话并解析用户资料。
    async fn establish(&self, client: &Client) -> Result<Profile, BootstrapFailure> {
        match client.bootstrap().await {
            Ok(client::Bootstrap::Established(html)) => session::parse_global_data(&html)
                .ok_or_else(|| BootstrapFailure::Error("畅课页面格式不兼容".into())),
            Ok(client::Bootstrap::NeedsAuth) => Err(BootstrapFailure::NeedsAuth(
                "请先完成应用统一认证，再查看畅课数据".into(),
            )),
            Err(message) => Err(BootstrapFailure::Error(message)),
        }
    }

    /// 拉取待办列表；会话失效时重新引导一次后重试。
    async fn fetch_todos(&self, client: &Client) -> Result<Vec<Todo>, String> {
        let mut retried = false;
        loop {
            match client.get_json("api/todos", &[]).await {
                Ok(value) => {
                    return serde_json::from_value::<TodosResponse>(value)
                        .map(|response| response.todo_list)
                        .map_err(|_| "畅课待办接口响应格式不兼容".to_string());
                }
                Err(error) if error.session_expired && !retried => {
                    retried = true;
                    if let Err(BootstrapFailure::NeedsAuth(message)) =
                        self.establish(client).await
                    {
                        return Err(message);
                    }
                }
                Err(error) => return Err(error.message),
            }
        }
    }

    async fn fetch_notifications(
        &self,
        client: &Client,
        uid: i64,
    ) -> Result<NotificationsResponse, String> {
        let path = format!("ntf/users/{uid}/notifications");
        let value = client
            .get_json(
                &path,
                &[
                    ("limit", "5".to_string()),
                    ("additionalFields", "unread_count".to_string()),
                    ("removed", "only_mobile".to_string()),
                ],
            )
            .await
            .map_err(|error| error.message)?;
        serde_json::from_value::<NotificationsResponse>(value)
            .map_err(|_| "畅课通知接口响应格式不兼容".to_string())
    }

    fn persist(&self, cache: &Cache, client: &Client) -> Result<(), String> {
        let cookies = client.cookies()?;
        self.credentials.save_lms_session(&cookies)?;
        let path = self.path.clone();
        let cache = cache.clone();
        crate::migrations::db::run_sync(async move {
            let database = db::LmsDatabase::open(&path).await?;
            database.save_cache(&cache).await
        })
    }
}

enum BootstrapFailure {
    NeedsAuth(String),
    Error(String),
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<LmsStore>();
    let service = Service::new(
        Some(app.clone()),
        store.credentials.clone(),
        app.path()
            .app_data_dir()
            .map_err(|_| "应用数据目录不可用".to_string())?
            .join("lms/state.sqlite"),
    )?;
    store
        .service
        .set(service)
        .map_err(|_| "畅课已初始化".to_string())
}

pub(crate) fn service(app: &tauri::AppHandle) -> Result<Arc<Service>, String> {
    app.state::<LmsStore>()
        .service
        .get()
        .cloned()
        .ok_or_else(|| "畅课服务尚未初始化".into())
}
