#[cfg(mobile)]
mod mobile {
    use std::collections::BTreeMap;

    use sha2::{Digest, Sha256};
    use tauri::Manager;
    use tauri_plugin_notification::{NotificationExt, Schedule};

    use super::super::model::{reminder_key, TodoItem};
    use super::super::reminder::describe_offset;
    use super::super::TodosStore;

    /// 只预排未来 14 天内的提醒，避免一次写入过多系统通知。
    const SCHEDULE_WINDOW_MS: i64 = 14 * 86_400_000;

    struct Planned<'a> {
        key: String,
        id: i32,
        item: &'a TodoItem,
        offset: i64,
    }

    pub(in crate::todos) async fn reconcile(app: &tauri::AppHandle, items: &[TodoItem]) {
        let settings = crate::notification::settings::load(app);
        let Some(store) = app.try_state::<TodosStore>() else {
            return;
        };

        let now = chrono::Utc::now().timestamp_millis();
        let window_end = now + SCHEDULE_WINDOW_MS;
        let mut planned = Vec::new();

        if settings.enabled && settings.todo.enabled {
            for item in items {
                if item.due_at_ms <= now || !settings.is_todo_kind_enabled(item.kind) {
                    continue;
                }
                for offset in &settings.todo.lead_minutes {
                    let fire_at = item.due_at_ms - offset * 60_000;
                    if fire_at <= now || fire_at > window_end {
                        continue;
                    }
                    let key = reminder_key(item, *offset);
                    let hash = Sha256::digest(key.as_bytes());
                    let id =
                        i32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]) & 0x3fff_ffff;
                    planned.push(Planned {
                        key,
                        id,
                        item,
                        offset: *offset,
                    });
                }
            }
        }

        let existing = store.state.lock().unwrap().scheduled.clone();

        let stale: Vec<i32> = {
            let planned_keys: std::collections::BTreeSet<&str> =
                planned.iter().map(|entry| entry.key.as_str()).collect();
            existing
                .iter()
                .filter(|(key, _)| !planned_keys.contains(key.as_str()))
                .map(|(_, id)| *id)
                .collect()
        };
        if !stale.is_empty() {
            let _ = app.notification().cancel(stale.clone());
            let _ = app.notification().remove_active(stale);
        }

        let mut next = BTreeMap::new();
        for entry in planned {
            if let Some(id) = existing.get(&entry.key) {
                next.insert(entry.key, *id);
                continue;
            }
            let fire_at = entry.item.due_at_ms - entry.offset * 60_000;
            let Ok(date) =
                time::OffsetDateTime::from_unix_timestamp_nanos(fire_at as i128 * 1_000_000)
            else {
                continue;
            };
            let subtitle = entry.item.subtitle.as_deref().unwrap_or("待办");
            let body = format!("{subtitle} · {}", describe_offset(entry.offset));
            if app
                .notification()
                .builder()
                .id(entry.id)
                .title(&entry.item.title)
                .body(body)
                .extra("route", entry.item.route.clone())
                .extra("todoId", entry.item.id.clone())
                .extra("todoKind", entry.item.kind)
                .extra("dueAtMs", entry.item.due_at_ms)
                .schedule(Schedule::At {
                    date,
                    repeating: false,
                    allow_while_idle: true,
                })
                .show()
                .is_ok()
            {
                next.insert(entry.key, entry.id);
            }
        }

        let database = store.database.clone();
        let (changed, snapshot) = {
            let mut state = store.state.lock().unwrap();
            let changed = state.scheduled != next;
            state.scheduled = next;
            (changed, state.clone())
        };
        drop(store);

        if changed {
            if let Some(database) = database {
                if let Err(error) = database.save_state(&snapshot).await {
                    log::warn!("保存待办排程状态失败: {error}");
                }
            }
        }
    }
}

#[cfg(mobile)]
pub(super) use mobile::reconcile;

#[cfg(not(mobile))]
pub(super) async fn reconcile(_app: &tauri::AppHandle, _items: &[super::model::TodoItem]) {}
