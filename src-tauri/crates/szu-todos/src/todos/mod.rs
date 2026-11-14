use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};

use szu_core::ipc_channel::SubscriberPool;
use szu_lms::lms::model::{SessionState, Snapshot as LmsSnapshot};

pub mod aggregate;
pub mod commands;
pub mod migrations;
mod db;
pub mod model;
mod reminder;
mod schedule;

use db::TodoDatabase;
use model::{ReminderState, TodoItem, TodosEvent};

/// 首页订阅发现畅课未登录时自动登录的冷却时间。
///
/// 畅课的冷缓存路径没有失败冷却（`lms::Service::ensure`），而订阅在每个 WebView
/// 挂载/重挂时都会触发，因此这里必须自己限流，避免认证失败后反复消耗 CAS 票据。
const LMS_AUTO_LOGIN_COOLDOWN: Duration = Duration::from_secs(10 * 60);

/// 触发刷新的来源。`force` 只属于用户主动刷新，避免「后台轮询却强制登录」的组合。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshSource {
    /// 页面订阅后的首次刷新（首页进入、窗口重挂）。
    Subscription,
    /// 后台定时轮询。
    Periodic,
    /// 用户主动刷新。
    Manual,
}

impl RefreshSource {
    fn force(self) -> bool {
        matches!(self, Self::Manual)
    }
}

/// 托盘菜单刷新回调。
///
/// 桌面端由 app crate 在启动时注册 `desktop::update_todo_menu`；Android 没有托盘，
/// 永远拿不到回调。用回调而不是直接依赖 desktop，`szu-todos` 才能独立成 crate。
pub type TrayUpdater = fn(&AppHandle, &[TodoItem]);

static TRAY_UPDATER: OnceLock<TrayUpdater> = OnceLock::new();

/// 注册托盘刷新回调（仅桌面端调用一次）。
pub fn set_tray_updater(updater: TrayUpdater) {
    let _ = TRAY_UPDATER.set(updater);
}

pub struct TodosStore {
    pub subscribers: SubscriberPool<TodosEvent>,
    pub database: Option<TodoDatabase>,
    pub state: Mutex<ReminderState>,
    last_signature: Mutex<Option<String>>,
    /// 最近一次为首页待办自动登录畅课的时间（仅内存，用于跨订阅限流）。
    last_lms_auto_login: Mutex<Option<Instant>>,
}

pub fn initialize(app: &AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "待办数据目录不可用")?
        .join("todos/state.sqlite");

    let database = match tauri::async_runtime::block_on(TodoDatabase::open(&path)) {
        Ok(database) => Some(database),
        Err(error) => {
            log::warn!("待办提醒状态库不可用，本次运行不持久化提醒记录: {error}");
            None
        }
    };
    let state = database
        .as_ref()
        .and_then(|database| tauri::async_runtime::block_on(database.load_state()).ok())
        .unwrap_or_default();

    app.manage(TodosStore {
        subscribers: SubscriberPool::new(),
        database,
        state: Mutex::new(state),
        last_signature: Mutex::new(None),
        last_lms_auto_login: Mutex::new(None),
    });

    reminder::start(app.clone());
    Ok(())
}

/// 聚合结果变化时才向 Channel 广播，避免每 tick 推整份列表。
pub fn publish(app: &AppHandle, items: &[TodoItem]) -> bool {
    let Some(store) = app.try_state::<TodosStore>() else {
        return false;
    };
    let signature = serde_json::to_string(items).unwrap_or_default();
    {
        let mut last = store.last_signature.lock().unwrap();
        if last.as_deref() == Some(signature.as_str()) {
            return false;
        }
        *last = Some(signature);
    }
    store
        .subscribers
        .broadcast(&TodosEvent::Snapshot(Box::new(items.to_vec())));
    // 托盘菜单跟随待办快照变化（仅桌面端会注册回调）。
    #[cfg(desktop)]
    if let Some(update_tray) = TRAY_UPDATER.get() {
        update_tray(app, items);
    }
    true
}

/// 刷新各数据源。用户主动刷新会强制各模块重新登录；其余来源走模块自己的缓存策略。
pub async fn refresh_sources(app: &AppHandle, source: RefreshSource) {
    let force = source.force();
    if let Ok(service) = szu_oj::oj::service(app) {
        service.scan(force).await;
    }
    if let Ok(service) = szu_lms::lms::service(app) {
        // 定时轮询不会为了待办提醒强行建立畅课会话；首页订阅时若畅课明确未登录
        // （needsAuth 且没有任何缓存），用 authserver SSO 自动登录一次，让待办
        // 列表能直接显示畅课项。失败由冷却限流，状态照旧交给页面展示。
        let snapshot = service.snapshot();
        let auto_login = lms_needs_auto_login(&snapshot, source) && allow_lms_auto_login(app);
        if force
            || snapshot.fetched_at.is_some()
            || snapshot.session == SessionState::Ready
            || auto_login
        {
            service.ensure(force).await;
        }
    }
    if let Ok(service) = szu_uooc::uooc::service(app) {
        if service.snapshot().session == szu_uooc::uooc::model::SessionStatus::Ready {
            let _ = service.refresh_assessments().await;
        }
    }
    // 签到探测的节拍由课表窗口驱动（szu-signin 调度器 + probe_if_due 的
    // 窗口门控）：自动刷新在窗外完全不动网络；用户主动刷新（force）始终拉取。
    if let Ok(service) = szu_signin::signin::service(app) {
        service.probe_if_due(source.force()).await;
    }
}

/// 首页订阅是否应该为畅课自动登录：只有「完全没有缓存 + 明确未登录 + 页面订阅」。
/// 有缓存时交给 `ensure` 的后台重验证（自带冷却）；unavailable 不自动重试。
fn lms_needs_auto_login(snapshot: &LmsSnapshot, source: RefreshSource) -> bool {
    source == RefreshSource::Subscription
        && snapshot.fetched_at.is_none()
        && snapshot.session == SessionState::NeedsAuth
}

/// 自动登录的跨订阅限流与统一认证前置。没有 SSO 会话时 `ensure` 本来就不会
/// 真的发出 CAS 请求，这里提前挡掉可以避免反复读取凭据。
fn allow_lms_auto_login(app: &AppHandle) -> bool {
    if szu_core::auth::reusable_sso_jar(app).is_err() {
        return false;
    }
    let Some(store) = app.try_state::<TodosStore>() else {
        return false;
    };
    let mut last = store.last_lms_auto_login.lock().unwrap();
    if last.is_some_and(|at| at.elapsed() < LMS_AUTO_LOGIN_COOLDOWN) {
        return false;
    }
    *last = Some(Instant::now());
    true
}

#[cfg(test)]
mod tests;
