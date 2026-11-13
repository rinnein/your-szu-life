use std::collections::HashSet;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use super::model::{item_key, reminder_key, ReminderState, TodoItem};
use super::{aggregate, publish, refresh_sources, RefreshSource, TodosStore};
use szu_core::notification::settings::NotificationSettings;
use szu_core::notification::{NotificationCategory, NotificationPayload, TodoKind};

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
    let aggregate::Aggregate { items, trusted } = aggregate::aggregate(app);
    publish(app, &items);
    evaluate(app, &items, &trusted).await;
    super::schedule::reconcile(app, &items).await;
}

async fn evaluate(app: &AppHandle, items: &[TodoItem], trusted: &HashSet<TodoKind>) {
    let settings = szu_core::notification::settings::load(app);
    let Some(store) = app.try_state::<TodosStore>() else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();

    let (pending, changed) = {
        let mut state = store.state.lock().unwrap();
        let before_notified = state.notified.clone();
        let before_discovered = state.discovered.clone();
        let before_baseline = (
            state.discovery_baseline_version,
            state.baselined_kinds.clone(),
        );
        establish_discovery_baseline(items, trusted, &mut state);
        let pending = plan_reminders(items, &settings, now, &mut state);
        prune_closed(items, trusted, now, &mut state);
        let changed = !pending.is_empty()
            || state.notified != before_notified
            || state.discovered != before_discovered
            || state.discovery_baseline_version != before_baseline.0
            || state.baselined_kinds != before_baseline.1;
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
        if let Some(offset) = offset {
            // Android 上该提醒已交给系统原生排程，避免前台运行时再发一次 Snackbar/系统通知。
            if has_native_schedule(app, &item, offset) {
                continue;
            }
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

/// 升级迁移：旧版本状态（`discovery_baseline_version == 0`）第一次看到某个来源的
/// 完整视图时，把当前已有待办静默写入 `discovered`，避免升级瞬间补发一批通知。
/// 新安装的 `Default` 已是 1，因此第一批待办会正常触发「初次探测到」。
pub fn establish_discovery_baseline(
    items: &[TodoItem],
    trusted: &HashSet<TodoKind>,
    state: &mut ReminderState,
) {
    if state.discovery_baseline_version >= 1 {
        return;
    }

    let mut kinds: std::collections::BTreeSet<TodoKind> = trusted.iter().copied().collect();
    kinds.extend(items.iter().map(|item| item.kind));
    for kind in kinds {
        if !state.baselined_kinds.insert(kind) {
            continue;
        }
        for item in items.iter().filter(|item| item.kind == kind) {
            state
                .discovered
                .entry(item_key(item))
                .or_insert(item.due_at_ms);
        }
    }
}

/// 纯函数：根据当前待办与设置计算本轮要发送的提醒，并就地更新去重状态。
///
/// 返回值的第二个字段为 `None` 时表示「初次探测到」，`Some(minutes)` 表示提前量。
/// 同一待办有多个阈值同时到达时只保留最接近截止的一档，其余一并标记为已通知，
/// 避免应用离线一段时间后补发一串过期提醒。
pub fn plan_reminders(
    items: &[TodoItem],
    settings: &NotificationSettings,
    now: i64,
    state: &mut ReminderState,
) -> Vec<(TodoItem, Option<i64>)> {
    if !settings.enabled || !settings.todo.enabled {
        return Vec::new();
    }

    let mut pending = Vec::new();
    for item in items {
        if item.due_at_ms <= now || !settings.is_todo_kind_enabled(item.kind) {
            continue;
        }

        let key = item_key(item);
        if !state.discovered.contains_key(&key) {
            state.discovered.insert(key, item.due_at_ms);
            // 初次探测立即提醒；把本轮已经到达的提前量一并标记，
            // 避免同一待办在下一次 tick 再补一条截止提醒。
            for offset in &settings.todo.lead_minutes {
                if item.due_at_ms - *offset * MINUTE_MS <= now {
                    state.notified.insert(reminder_key(item, *offset), now);
                }
            }
            pending.push((item.clone(), None));
            continue;
        }
        // 截止时间可能被延长；记录最新值供关闭后的兜底清理使用。
        state.discovered.insert(key, item.due_at_ms);

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
        pending.push((item.clone(), Some(chosen_offset)));
    }

    let horizon = now - NOTIFIED_RETENTION_MS;
    state
        .notified
        .retain(|key, _| key_due_at_ms(key).is_none_or(|due_at| due_at >= horizon));
    pending
}

/// 可信来源中已经不在当前列表里的待办视为「确认关闭」，连同类名下所有提醒记录一起删除。
/// 下次同一 `kind|id` 再次出现时 `discovered` 为空，会重新按「初次探测到」处理；
/// 已发过的提前量也随之作废重发。
///
/// 网络失败 / 5xx / 登录失效会让来源不进入 `trusted`，因此不会误清理。
pub fn prune_closed(
    items: &[TodoItem],
    trusted: &HashSet<TodoKind>,
    now: i64,
    state: &mut ReminderState,
) {
    let horizon = now - NOTIFIED_RETENTION_MS;
    let trusted_keys: HashSet<&str> = trusted.iter().map(|kind| kind.key()).collect();
    let live: HashSet<String> = items.iter().map(item_key).collect();

    state.notified.retain(|key, _| {
        if let Some(item_key) = notified_item_key(key) {
            let trusted_kind = item_key
                .split_once('|')
                .is_some_and(|(kind, _)| trusted_keys.contains(kind));
            if trusted_kind && !live.contains(item_key) {
                return false;
            }
        }
        key_due_at_ms(key).is_none_or(|due_at| due_at >= horizon)
    });

    state.discovered.retain(|key, due_at| {
        if live.contains(key) {
            // 仍然存在的待办永远保留；截止时间可能已经延长，不能按旧 due 兜底清掉。
            return true;
        }
        let trusted_kind = key
            .split_once('|')
            .is_some_and(|(kind, _)| trusted_keys.contains(kind));
        if trusted_kind {
            return false;
        }
        *due_at >= horizon
    });
}

fn dispatch(app: &AppHandle, item: &TodoItem, offset_minutes: Option<i64>) {
    let now = chrono::Utc::now().timestamp_millis();
    let remaining = (item.due_at_ms - now).max(0);
    let remaining_text = humanize_remaining(remaining);
    let body = match (item.subtitle.as_deref(), offset_minutes) {
        (Some(subtitle), Some(_)) => format!("{subtitle} · {remaining_text}"),
        (Some(subtitle), None) => format!("{subtitle} · 初次探测到 · {remaining_text}"),
        (None, Some(_)) => remaining_text,
        (None, None) => format!("初次探测到 · {remaining_text}"),
    };
    let mut extra = serde_json::json!({
        "todoId": item.id,
        "todoKind": item.kind,
        "dueAtMs": item.due_at_ms,
    });
    if let Some(offset) = offset_minutes {
        extra["offsetMinutes"] = serde_json::json!(offset);
    }
    let payload = NotificationPayload::new(NotificationCategory::Todo, &item.title, body)
        .route(item.route.clone())
        .action_label("查看")
        .extra(extra);
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

/// 提前量键格式为 `kind|id|due_at_ms|offset`；去掉最后两段得到 `kind|id`。
fn notified_item_key(key: &str) -> Option<&str> {
    let (rest, _offset) = key.rsplit_once('|')?;
    let (item_key, _due_at) = rest.rsplit_once('|')?;
    Some(item_key)
}

/// 去重键格式为 `kind|id|due_at_ms|offset`；取出 due_at_ms 用于按截止时间清理。
fn key_due_at_ms(key: &str) -> Option<i64> {
    let (prefix, _offset) = key.rsplit_once('|')?;
    let (_prefix, due_at) = prefix.rsplit_once('|')?;
    due_at.parse().ok()
}
