use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::ipc_channel::SubscriberPool;

pub(crate) mod aggregate;
pub(crate) mod commands;
mod db;
pub(crate) mod mcp;
pub(crate) mod model;
mod reminder;
mod schedule;

use db::TodoDatabase;
use model::{ReminderState, TodoItem, TodosEvent};

pub(crate) struct TodosStore {
    pub(crate) subscribers: SubscriberPool<TodosEvent>,
    pub(crate) database: Option<TodoDatabase>,
    pub(crate) state: Mutex<ReminderState>,
    last_signature: Mutex<Option<String>>,
}

pub(crate) fn initialize(app: &AppHandle) -> Result<(), String> {
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
    });

    reminder::start(app.clone());
    Ok(())
}

/// 聚合结果变化时才向 Channel 广播，避免每 tick 推整份列表。
pub(crate) fn publish(app: &AppHandle, items: &[TodoItem]) -> bool {
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
    // 托盘菜单跟随待办快照变化（仅桌面端有托盘）。
    #[cfg(desktop)]
    crate::desktop::update_todo_menu(app, items);
    true
}

/// 刷新各数据源。`force` 用于用户主动刷新；否则走各模块自己的缓存策略。
pub(crate) async fn refresh_sources(app: &AppHandle, force: bool) {
    if let Ok(service) = crate::oj::service(app) {
        service.scan(force).await;
    }
    if let Ok(service) = crate::lms::service(app) {
        // 没有本地缓存时不要为了待办提醒强行建立畅课会话；只有用户主动刷新才强制登录。
        let snapshot = service.snapshot();
        if force
            || snapshot.fetched_at.is_some()
            || snapshot.session == crate::lms::model::SessionState::Ready
        {
            service.ensure(force).await;
        }
    }
    if let Ok(service) = crate::uooc::service(app) {
        if service.snapshot().session == crate::uooc::model::SessionStatus::Ready {
            let _ = service.refresh_assessments().await;
        }
    }
}

#[cfg(test)]
mod tests;
