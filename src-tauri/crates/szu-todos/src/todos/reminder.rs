use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use super::model::{reminder_key, ReminderState, TodoItem};
use super::{aggregate, publish, refresh_sources, RefreshSource, TodosStore};
use szu_core::notification::settings::NotificationSettings;
use szu_core::notification::{NotificationCategory, NotificationPayload};

const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 86_400_000;
/// 已通知记录按截止时间保留 30 天；更早的待办不会再次触发。
const NOTIFIED_RETENTION_MS: i64 = 30 * DAY_MS;

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_refresh = Instant::now() - Duration::from_secs(86_400);
        loop {
            let interval = szu_core::activity::interval(
                Some("todos"),
                Duration::from_secs(30),
                Duration::from_secs(60),
                Duration::from_secs(120),
            );
            let refresh_interval = szu_core::activity::interval(
                Some("todos"),
                Duration::from_secs(300),
                Duration::from_secs(900),
                Duration::from_secs(3600),
            );
            if last_refresh.elapsed() >= refresh_interval {
                last_refresh = Instant::now();
                refresh_sources(&app, RefreshSource::Periodic).await;
            }
            tick(&app).await;
            tokio::time::sleep(szu_core::activity::sleep_chunk(
                interval,
                Duration::from_secs(15),
            ))
            .await;
        }
    });
}

/// 单次聚合 + 广播 + 提醒判定。命令层主动刷新后也会调用一次。
pub async fn tick(app: &AppHandle) {
    let items = aggregate::aggregate(app);
    publish(app, &items);
    evaluate(app, &items).await;
    super::schedule::reconcile(app, &items).await;
}

async fn evaluate(app: &AppHandle, items: &[TodoItem]) {
    let settings = szu_core::notification::settings::load(app);
    let Some(store) = app.try_state::<TodosStore>() else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();

    let (pending, changed) = {
        let mut state = store.state.lock().unwrap();
        let before = state.notified.len();
        let pending = plan_reminders(items, &settings, now, &mut state);
        let changed = !pending.is_empty() || state.notified.len() != before;
        (pending, changed)
    };

    if changed {
        let database = store.database.clone();
        let snapshot = store.state.lock().unwrap().clone();
        drop(store);
        if let Some(database) = database {
            if let Err(error) = database.save_state(&snapshot).await {
                log::warn!("保存待办提醒状态失败: {error}");
            }
        }
    }

    for (item, offset) in pending {
        // Android 上该提醒已交给系统原生排程，避免前台运行时再发一次 Snackbar/系统通知。
        if has_native_schedule(app, &item, offset) {
            continue;
        }
        dispatch(app, &item, offset);
    }
}

#[cfg(mobile)]
fn has_native_schedule(app: &AppHandle, item: &TodoItem, offset_minutes: i64) -> bool {
    app.try_state::<TodosStore>().is_some_and(|store| {
        store
            .state
            .lock()
            .unwrap()
            .scheduled
            .contains_key(&reminder_key(item, offset_minutes))
    })
}

#[cfg(not(mobile))]
fn has_native_schedule(_app: &AppHandle, _item: &TodoItem, _offset_minutes: i64) -> bool {
    false
}

/// 纯函数：根据当前待办与设置计算本轮要发送的提醒，并就地更新去重状态。
///
/// 同一待办有多个阈值同时到达时只保留最接近截止的一档，其余一并标记为已通知，
/// 避免应用离线一段时间后补发一串过期提醒。
pub fn plan_reminders(
    items: &[TodoItem],
    settings: &NotificationSettings,
    now: i64,
    state: &mut ReminderState,
) -> Vec<(TodoItem, i64)> {
    if !settings.enabled || !settings.todo.enabled || settings.todo.lead_minutes.is_empty() {
        return Vec::new();
    }

    let mut pending = Vec::new();
    for item in items {
        if item.due_at_ms <= now || !settings.is_todo_kind_enabled(item.kind) {
            continue;
        }

        let mut reached: Vec<(i64, String)> = settings
            .todo
            .lead_minutes
            .iter()
            .filter(|offset| item.due_at_ms - *offset * MINUTE_MS <= now)
            .map(|offset| (*offset, reminder_key(item, *offset)))
            .filter(|(_, key)| !state.notified.contains_key(key))
            .collect();
        if reached.is_empty() {
            continue;
        }
        reached.sort_by_key(|(offset, _)| *offset);
        let chosen_offset = reached[0].0;
        for (_, key) in reached {
            state.notified.insert(key, now);
        }
        pending.push((item.clone(), chosen_offset));
    }

    let horizon = now - NOTIFIED_RETENTION_MS;
    state
        .notified
        .retain(|key, _| key_due_at_ms(key).is_none_or(|due_at| due_at >= horizon));
    pending
}

fn dispatch(app: &AppHandle, item: &TodoItem, offset_minutes: i64) {
    let now = chrono::Utc::now().timestamp_millis();
    let remaining = (item.due_at_ms - now).max(0);
    let body = match item.subtitle.as_deref() {
        Some(subtitle) => format!("{subtitle} · {}", humanize_remaining(remaining)),
        None => humanize_remaining(remaining),
    };
    let payload = NotificationPayload::new(NotificationCategory::Todo, &item.title, body)
        .route(item.route.clone())
        .action_label("查看")
        .extra(serde_json::json!({
            "todoId": item.id,
            "todoKind": item.kind,
            "dueAtMs": item.due_at_ms,
            "offsetMinutes": offset_minutes,
        }));
    szu_core::notification::notify(app, payload);
}

pub fn humanize_remaining(remaining_ms: i64) -> String {
    let minutes = (remaining_ms / MINUTE_MS).max(1);
    if minutes >= 1_440 {
        let days = minutes / 1_440;
        let hours = (minutes % 1_440) / 60;
        if hours > 0 {
            format!("距离截止还有 {days} 天 {hours} 小时")
        } else {
            format!("距离截止还有 {days} 天")
        }
    } else if minutes >= 60 {
        let hours = minutes / 60;
        let rest = minutes % 60;
        if rest > 0 {
            format!("距离截止还有 {hours} 小时 {rest} 分钟")
        } else {
            format!("距离截止还有 {hours} 小时")
        }
    } else {
        format!("距离截止还有 {minutes} 分钟")
    }
}

/// 供移动端排程复用：把提前量写成「N 天后截止」的文案。
#[cfg(mobile)]
pub fn describe_offset(offset_minutes: i64) -> String {
    humanize_remaining(offset_minutes * MINUTE_MS)
}

/// 去重键格式为 `kind|id|due_at_ms|offset`；取出 due_at_ms 用于按截止时间清理。
fn key_due_at_ms(key: &str) -> Option<i64> {
    let (prefix, _offset) = key.rsplit_once('|')?;
    let (_prefix, due_at) = prefix.rsplit_once('|')?;
    due_at.parse().ok()
}
