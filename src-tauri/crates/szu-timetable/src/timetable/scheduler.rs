use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::{AppHandle, Manager};

use szu_core::notification::settings;
use szu_core::notification::{notify, NotificationCategory, NotificationPayload};
use szu_signin::signin::model::CheckinTodoItem;

#[cfg(target_os = "android")]
use super::android;
#[cfg(target_os = "android")]
use super::model::class_sync_specs;
#[cfg(not(target_os = "android"))]
use super::model::plan_desktop_class_reminders;
use super::model::{plan_checkin_reminders, ClassReminderSpec, ReminderState};
use super::model::{CheckinSource, SemesterCache, SignedCheckin};
use super::TimetableStore;

/// eHall 固定课表最多每天刷新一次；失败继续使用缓存。
const SEMESTER_REFRESH_INTERVAL_MS: i64 = 86_400_000;
const SIGNED_RETENTION_MS: i64 = 2 * 86_400_000;

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_refresh = Instant::now() - Duration::from_secs(86_400);
        loop {
            let tick = szu_core::activity::interval(
                Some("timetable"),
                Duration::from_secs(30),
                Duration::from_secs(60),
                Duration::from_secs(120),
            );
            let refresh_interval = szu_core::activity::interval(
                Some("timetable"),
                Duration::from_secs(300),
                Duration::from_secs(900),
                Duration::from_secs(3600),
            );
            if last_refresh.elapsed() >= refresh_interval {
                last_refresh = Instant::now();
                super::refresh(&app, false).await;
            }
            evaluate(&app).await;
            tokio::time::sleep(szu_core::activity::sleep_chunk(
                tick,
                Duration::from_secs(15),
            ))
            .await;
        }
    });
}

/// 单轮：签到提醒 → 课前提醒 → Android 原生排程同步 → 广播。
pub async fn evaluate(app: &AppHandle) {
    let settings = settings::load(app);
    let now = Utc::now().timestamp_millis();
    let Some(store) = app.try_state::<TimetableStore>() else {
        return;
    };
    let (sessions, view) = match szu_signin::signin::service(app) {
        Ok(service) => (
            service.timetable_snapshot().sessions,
            service.checkin_view(),
        ),
        Err(_) => (Vec::new(), Default::default()),
    };

    let before = store.state.lock().unwrap().clone();
    let (checkins, desktop_classes) = {
        let mut state = store.state.lock().unwrap();
        if let Some(service) = szu_signin::signin::service(app).ok() {
            observe_signed(&mut state, &service, &sessions, now);
        }
        let checkins = plan_checkin_reminders(&view, &settings, now, &mut state);

        #[cfg(not(target_os = "android"))]
        let desktop_classes = plan_desktop_class_reminders(&sessions, &settings, now, &mut state);
        #[cfg(target_os = "android")]
        let desktop_classes: Vec<ClassReminderSpec> = Vec::new();

        (checkins, desktop_classes)
    };
    save_if_changed(&store, &before).await;

    for item in checkins {
        dispatch_checkin(app, &item);
    }
    for spec in desktop_classes {
        dispatch_class(app, &spec);
    }

    refresh_semester(app, false).await;

    #[cfg(target_os = "android")]
    sync_android(app, &sessions, &settings, now).await;

    super::publish(app);
}

fn observe_signed(
    state: &mut ReminderState,
    service: &std::sync::Arc<szu_signin::signin::Service>,
    sessions: &[szu_signin::signin::model::TimetableSession],
    now: i64,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let rollcalls = service.cached_rollcalls();
    for session in sessions {
        if session.date != today || now < session.start_ms - 10 * 60_000 {
            continue;
        }
        let key = super::model::session_id(session);
        let title = szu_signin::signin::matching::normalize(&session.course_name);
        if rollcalls.iter().any(|item| {
            szu_signin::signin::matching::normalize(&item.course_title) == title
                && !item.need_answer()
                && item.status.as_deref() == Some("on_call_fine")
        }) {
            state
                .checkin_signed
                .entry(key.clone())
                .or_insert(SignedCheckin {
                    at_ms: now,
                    source: CheckinSource::Lms,
                });
        }
        if service.cached_szu_checkin(&key) == szu_signin::signin::model::SzuCheckinState::Signed {
            state.checkin_signed.entry(key).or_insert(SignedCheckin {
                at_ms: now,
                source: CheckinSource::Szu,
            });
        }
    }
    state
        .checkin_signed
        .retain(|_, signed| now - signed.at_ms <= SIGNED_RETENTION_MS);
}

/// 刷新 eHall 学期固定课表：自动最多 24 小时一次；失败保留缓存。
pub async fn refresh_semester(app: &AppHandle, force: bool) {
    let now = Utc::now().timestamp_millis();
    let Some(store) = app.try_state::<TimetableStore>() else {
        return;
    };
    let term = szu_signin::signin::service(app)
        .ok()
        .and_then(|service| service.timetable_snapshot().semester)
        .or_else(|| {
            app.try_state::<szu_enrollment::enrollment::Store>()
                .map(|store| store.0.context_term())
        })
        .unwrap_or_default();
    let (should_refresh, previous_term) = {
        let state = store.state.lock().unwrap();
        (
            force || now - state.semester.attempted_at_ms >= SEMESTER_REFRESH_INTERVAL_MS,
            state.semester.term.clone(),
        )
    };
    if !should_refresh {
        return;
    }

    let before = store.state.lock().unwrap().clone();
    {
        let mut state = store.state.lock().unwrap();
        state.semester.attempted_at_ms = now;
        if !term.trim().is_empty() && previous_term != term {
            // 已知学期变化后旧缓存不可用；失败时宁可显示认证提示，也不展示别的学期。
            state.semester = SemesterCache {
                term: term.clone(),
                courses: Vec::new(),
                fetched_at_ms: 0,
                attempted_at_ms: now,
            };
            state.semester_message.clear();
        }
    }

    let fetch = app
        .try_state::<szu_enrollment::enrollment::Store>()
        .map(|store| {
            let service = store.0.clone();
            let preferred = (!term.trim().is_empty()).then_some(term.clone());
            tauri::async_runtime::spawn_blocking(move || {
                service.fetch_semester_schedule(preferred.as_deref())
            })
        });
    let result = match fetch {
        Some(handle) => handle.await,
        None => {
            save_if_changed(&store, &before).await;
            return;
        }
    };

    {
        let mut state = store.state.lock().unwrap();
        match result {
            Ok(Ok(schedule)) => {
                state.semester = SemesterCache {
                    term: schedule.term,
                    courses: schedule.courses,
                    fetched_at_ms: now,
                    attempted_at_ms: now,
                };
                state.semester_message.clear();
            }
            Ok(Err(error)) => {
                state.semester.attempted_at_ms = now;
                state.semester_message = if state.semester.courses.is_empty() {
                    error
                } else {
                    format!("{error}（正在显示上次缓存）")
                };
            }
            Err(error) => {
                state.semester.attempted_at_ms = now;
                state.semester_message = format!("教务课表刷新失败: {error}");
            }
        }
    }
    save_if_changed(&store, &before).await;
    super::publish(app);
}

async fn save_if_changed(store: &TimetableStore, before: &ReminderState) {
    let snapshot = {
        let state = store.state.lock().unwrap();
        if *state == *before {
            return;
        }
        state.clone()
    };
    if let Some(database) = store.database.clone() {
        if let Err(error) = database.save_state(&snapshot).await {
            log::warn!("保存课表提醒状态失败: {error}");
        }
    }
}

#[cfg(target_os = "android")]
async fn sync_android(
    app: &AppHandle,
    sessions: &[szu_signin::signin::model::TimetableSession],
    settings: &szu_core::notification::settings::NotificationSettings,
    now: i64,
) {
    let Some(store) = app.try_state::<TimetableStore>() else {
        return;
    };
    let before = store.state.lock().unwrap().clone();
    let (specs, enabled, changed) = {
        let mut state = store.state.lock().unwrap();
        let enabled = settings.enabled && settings.timetable.class_reminder;
        let specs = if enabled {
            class_sync_specs(sessions, now)
        } else {
            Vec::new()
        };
        let signature = if enabled {
            serde_json::to_string(&specs).unwrap_or_default()
        } else {
            String::new()
        };
        let changed = state.last_sync_signature != signature;
        if changed {
            state.last_sync_signature = signature;
        }
        (specs, enabled, changed)
    };
    if !changed {
        return;
    }
    save_if_changed(&store, &before).await;
    let result = if enabled {
        android::sync(&specs)
    } else {
        android::cancel_all()
    };
    if let Err(error) = result {
        log::warn!("同步 Android 课表通知失败: {error}");
    }
}

fn checkin_payload(item: &CheckinTodoItem) -> NotificationPayload {
    let details = item
        .subtitle
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("探测到新的点名，请尽快前往签到页处理。");
    NotificationPayload::new(NotificationCategory::Timetable, &item.title, details)
        .route(item.route.clone())
        .action_label("去签到")
}

fn dispatch_checkin(app: &AppHandle, item: &CheckinTodoItem) {
    notify(app, checkin_payload(item));
}

fn dispatch_class(app: &AppHandle, spec: &ClassReminderSpec) {
    let mut parts = Vec::new();
    if !spec.time_label.is_empty() {
        parts.push(spec.time_label.clone());
    }
    if !spec.location.is_empty() {
        parts.push(spec.location.clone());
    }
    parts.push(format!("{} 分钟后开始", spec.lead_minutes));
    let payload = NotificationPayload::new(
        NotificationCategory::Timetable,
        &spec.course_name,
        parts.join(" · "),
    )
    .route(spec.route.clone())
    .action_label("查看课表");
    notify(app, payload);
}

#[cfg(test)]
mod tests {
    use super::checkin_payload;
    use super::super::model::{class_spec, class_sync_specs};
    use szu_signin::signin::model::TimetableSession;

    fn session(start: i64, end: i64) -> TimetableSession {
        TimetableSession {
            date: "2026-11-16".into(),
            course_name: "高等数学".into(),
            building_name: Some("致理楼".into()),
            building_name_en: None,
            classroom: Some("301".into()),
            start_ms: start,
            end_ms: end,
            semester: None,
        }
    }

    #[test]
    fn class_spec_carries_time_location_and_twenty_minutes() {
        let start = 1_800_000_000_000;
        let spec = class_spec(&session(start, start + 90 * 60_000));
        assert_eq!(spec.course_name, "高等数学");
        assert_eq!(spec.location, "致理楼 301");
        assert_eq!(spec.lead_minutes, 20);
        assert!(spec.route.contains("tab=week"));
        assert!(!spec.time_label.is_empty());
    }

    #[test]
    fn sync_specs_only_include_the_next_three_days() {
        let now = 1_800_000_000_000;
        let sessions = vec![
            session(now + 60 * 60_000, now + 90 * 60_000),
            session(now + 4 * 86_400_000, now + 4 * 86_400_000 + 90 * 60_000),
            session(now - 60_000, now + 60_000),
        ];
        let specs = class_sync_specs(&sessions, now);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].start_at_ms, now + 60 * 60_000);
    }

    #[test]
    fn checkin_payload_uses_auto_failure_reason_when_present() {
        use szu_signin::signin::model::CheckinTodoItem;

        let item = CheckinTodoItem {
            id: "checkin:szu:高等数学|2026-11-17|1000".into(),
            title: "高等数学 签到".into(),
            subtitle: Some("没有为该课程保存签到照片，自动签到已跳过。".into()),
            due_at_ms: 0,
            route: "/study/signin?tab=szu".into(),
        };
        let payload = checkin_payload(&item);
        assert_eq!(
            payload.body,
            "没有为该课程保存签到照片，自动签到已跳过。"
        );
        assert_eq!(
            payload.route.as_deref(),
            Some("/study/signin?tab=szu")
        );

        let plain = CheckinTodoItem {
            subtitle: None,
            ..item
        };
        assert_eq!(
            checkin_payload(&plain).body,
            "探测到新的点名，请尽快前往签到页处理。"
        );
    }
}
