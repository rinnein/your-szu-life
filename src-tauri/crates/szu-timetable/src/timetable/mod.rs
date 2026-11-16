//! 课表模块：考勤课表、已选课程、签到展示与上课提醒。
//!
//! 数据来源：
//! - 考勤课表/待签点名：`szu-signin` 的 `timetable_snapshot()` / `checkin_view()`；
//! - 学期课表：`szu-enrollment` 最近一次落盘的已选课程（只读，不触发选课网络）。
//!
//! 通知：
//! - 签到提醒走统一的 `szu_core::notification::notify`；
//! - 课前提醒在 Android 上交给原生常驻通知桥（20 分钟倒计时/Live Update），
//!   其它平台在进入 20 分钟窗口时发一次系统通知。

mod android;
pub mod commands;
mod db;
mod migrations;
pub mod model;
mod scheduler;

use std::sync::Mutex;

use chrono::Utc;
use tauri::{AppHandle, Listener, Manager};

use szu_core::ipc_channel::SubscriberPool;

use db::TimetableDatabase;
use model::{PendingCheckin, ReminderState, TimetableEvent, TimetableSnapshot};

/// 拉取考勤课表失败后的静默重试间隔。
const REFRESH_BACKOFF_MS: i64 = 30 * 60_000;

pub struct TimetableStore {
    pub subscribers: SubscriberPool<TimetableEvent>,
    pub database: Option<TimetableDatabase>,
    pub state: Mutex<ReminderState>,
    message: Mutex<String>,
    refresh_backoff_until: Mutex<i64>,
    last_signature: Mutex<Option<String>>,
}

pub fn initialize(app: &AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "课表数据目录不可用")?
        .join("timetable/state.sqlite");

    let database = match tauri::async_runtime::block_on(TimetableDatabase::open(&path)) {
        Ok(database) => Some(database),
        Err(error) => {
            log::warn!("课表状态库不可用，本次运行不持久化提醒记录: {error}");
            None
        }
    };
    let state = database
        .as_ref()
        .and_then(|database| tauri::async_runtime::block_on(database.load_state()).ok())
        .unwrap_or_default();
    // 进程重启后原生闹钟可能已被系统清理或已触发；清空签名让首轮 evaluate 必定重新同步。
    let mut state = state;
    state.last_sync_signature.clear();

    app.manage(TimetableStore {
        subscribers: SubscriberPool::new(),
        database,
        state: Mutex::new(state),
        message: Mutex::new(String::new()),
        refresh_backoff_until: Mutex::new(0),
        last_signature: Mutex::new(None),
    });

    scheduler::start(app.clone());

    let handle = app.clone();
    app.listen("notification-settings-changed", move |_| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            scheduler::evaluate(&app).await;
        });
    });

    // 签到状态由 szu-signin 的课表窗口调度刷新；变化后立即重发课表快照。
    let handle = app.clone();
    app.listen("signin-checkin-status-changed", move |_| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            scheduler::evaluate(&app).await;
        });
    });

    Ok(())
}

/// 当前快照。`checkins` 的首次发现时间来自提醒状态，避免签到探测每次改写截止时间。
pub fn snapshot(app: &AppHandle) -> TimetableSnapshot {
    let signin = szu_signin::signin::service(app).ok();
    let cache = signin
        .as_ref()
        .map(|service| service.timetable_snapshot())
        .unwrap_or_default();
    let view = signin
        .as_ref()
        .map(|service| service.checkin_view())
        .unwrap_or_default();
    let (state, message) = match app.try_state::<TimetableStore>() {
        Some(store) => (
            store.state.lock().unwrap().clone(),
            store.message.lock().unwrap().clone(),
        ),
        None => (ReminderState::default(), String::new()),
    };

    let now = Utc::now().timestamp_millis();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let rollcalls = signin
        .as_ref()
        .map(|service| service.cached_rollcalls())
        .unwrap_or_default();
    let mut sessions = cache.sessions;
    sessions.sort_by_key(|session| session.start_ms);
    let sessions: Vec<model::SessionView> = sessions
        .into_iter()
        .map(|session| {
            let key = model::session_id(&session);
            let checkin =
                resolve_checkin(&session, &today, &rollcalls, signin.as_ref(), &state, &key);
            model::SessionView::new(session, checkin)
        })
        .collect();
    let checkins = view
        .items
        .into_iter()
        .map(|item| PendingCheckin {
            first_seen_ms: state
                .checkin_first_seen
                .get(&item.id)
                .copied()
                .unwrap_or(now),
            id: item.id,
            title: item.title,
            subtitle: item.subtitle,
            route: item.route,
        })
        .collect();

    TimetableSnapshot {
        fetched_at_ms: cache.fetched_at_ms,
        semester: cache.semester,
        message,
        sessions,
        checkins,
        semester_courses: state.semester.courses.clone(),
        semester_fetched_at_ms: state.semester.fetched_at_ms,
        semester_message: state.semester_message.clone(),
    }
}

fn resolve_checkin(
    session: &szu_signin::signin::model::TimetableSession,
    today: &str,
    rollcalls: &[szu_lms::lms::rollcall::RollcallItem],
    signin: Option<&std::sync::Arc<szu_signin::signin::Service>>,
    state: &ReminderState,
    key: &str,
) -> model::SessionCheckin {
    use model::{CheckinSource, SessionCheckin};
    use szu_signin::signin::matching::normalize;
    use szu_signin::signin::model::SzuCheckinState;

    let mut lms_pending = false;
    let mut lms_signed = false;
    if session.date == today {
        let title = normalize(&session.course_name);
        for item in rollcalls {
            if normalize(&item.course_title) != title {
                continue;
            }
            if item.need_answer() {
                lms_pending = true;
            } else if item.status.as_deref() == Some("on_call_fine") {
                lms_signed = true;
            }
        }
    }
    let szu = signin.map(|service| service.cached_szu_checkin(key));
    let szu_pending = matches!(szu, Some(SzuCheckinState::Pending));
    let szu_signed = matches!(szu, Some(SzuCheckinState::Signed));

    if lms_pending {
        SessionCheckin::PendingLms
    } else if szu_pending {
        SessionCheckin::PendingSzu
    } else if lms_signed {
        SessionCheckin::SignedLms
    } else if szu_signed {
        SessionCheckin::SignedSzu
    } else if let Some(signed) = state.checkin_signed.get(key) {
        match signed.source {
            CheckinSource::Lms => SessionCheckin::SignedLms,
            CheckinSource::Szu => SessionCheckin::SignedSzu,
        }
    } else {
        SessionCheckin::None
    }
}

/// 快照变化时才向 Channel 广播。
pub fn publish(app: &AppHandle) -> bool {
    let Some(store) = app.try_state::<TimetableStore>() else {
        return false;
    };
    let snapshot = snapshot(app);
    let signature = serde_json::to_string(&snapshot).unwrap_or_default();
    {
        let mut last = store.last_signature.lock().unwrap();
        if last.as_deref() == Some(signature.as_str()) {
            return false;
        }
        *last = Some(signature);
    }
    store
        .subscribers
        .broadcast(&TimetableEvent::Snapshot(Box::new(snapshot)));
    true
}

/// 拉取考勤课表（`refresh_timetable` 自带 6 小时 TTL；失败后 30 分钟内不再重试）。
pub async fn refresh(app: &AppHandle, force: bool) {
    let Some(store) = app.try_state::<TimetableStore>() else {
        return;
    };
    let now = Utc::now().timestamp_millis();
    if !force && now < *store.refresh_backoff_until.lock().unwrap() {
        return;
    }
    let Ok(service) = szu_signin::signin::service(app) else {
        return;
    };
    match service.refresh_timetable(force).await {
        Ok(_) => {
            *store.message.lock().unwrap() = String::new();
        }
        Err(error) => {
            log::warn!("拉取考勤课表失败: {error}");
            *store.message.lock().unwrap() = error;
            *store.refresh_backoff_until.lock().unwrap() = now + REFRESH_BACKOFF_MS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use szu_lms::lms::rollcall::RollcallItem;
    use szu_signin::signin::model::TimetableSession;

    fn session() -> TimetableSession {
        TimetableSession {
            date: "2026-11-16".into(),
            course_name: "高等数学".into(),
            building_name: None,
            building_name_en: None,
            classroom: None,
            start_ms: 1_000,
            end_ms: 2_000,
            semester: None,
        }
    }

    fn rollcall(status: Option<&str>) -> RollcallItem {
        RollcallItem {
            rollcall_id: 1,
            is_number: true,
            is_radar: false,
            course_id: 1,
            course_title: "高等数学".into(),
            student_rollcall_id: 1,
            kind: Some("number".into()),
            status: status.map(str::to_string),
        }
    }

    #[test]
    fn lms_pending_wins_over_signed() {
        let item = session();
        let state = ReminderState::default();
        let checkin = resolve_checkin(
            &item,
            "2026-11-16",
            &[rollcall(Some("on_call_fine")), rollcall(Some("on_call"))],
            None,
            &state,
            "key",
        );
        assert_eq!(checkin, model::SessionCheckin::PendingLms);
    }

    #[test]
    fn lms_signed_is_reported_when_no_pending() {
        let item = session();
        let state = ReminderState::default();
        let checkin = resolve_checkin(
            &item,
            "2026-11-16",
            &[rollcall(Some("on_call_fine"))],
            None,
            &state,
            "key",
        );
        assert_eq!(checkin, model::SessionCheckin::SignedLms);
    }

    #[test]
    fn persisted_signed_survives_missing_rollcalls() {
        let item = session();
        let mut state = ReminderState::default();
        state.checkin_signed.insert(
            "key".into(),
            model::SignedCheckin {
                at_ms: 1,
                source: model::CheckinSource::Szu,
            },
        );
        let checkin = resolve_checkin(&item, "2026-11-16", &[], None, &state, "key");
        assert_eq!(checkin, model::SessionCheckin::SignedSzu);
    }
}
