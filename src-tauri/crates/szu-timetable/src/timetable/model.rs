//! 课表快照、提醒状态与纯函数规划。

use std::collections::BTreeMap;

use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};

use szu_core::notification::settings::NotificationSettings;
use szu_enrollment::enrollment::model::SemesterCourse;
use szu_signin::signin::model::{CheckinTodoItem, CheckinView, TimetableSession};

/// 课前提醒提前量（分钟）。
pub const CLASS_REMINDER_LEAD_MINUTES: i64 = 20;
const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 86_400_000;
/// Android 原生排程只同步未来 72 小时，应用启动/刷新时再延长。
const CLASS_SYNC_WINDOW_MS: i64 = 3 * DAY_MS;
/// 已提醒记录保留 7 天，避免课表滚动后状态无限增长。
const CLASS_NOTIFIED_RETENTION_MS: i64 = 7 * DAY_MS;

/// 课表页与首页小部件共用的快照。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableSnapshot {
    pub fetched_at_ms: i64,
    pub semester: Option<String>,
    /// 最近一次拉取考勤课表的错误；成功时为空。
    pub message: String,
    pub sessions: Vec<SessionView>,
    pub checkins: Vec<PendingCheckin>,
    /// eHall 固定学期课表（持久缓存）。
    pub semester_courses: Vec<SemesterCourse>,
    pub semester_fetched_at_ms: i64,
    /// 学期课表刷新失败时的提示；成功时为空。
    pub semester_message: String,
}

/// 快照里的课次。
///
/// `szu_signin::TimetableSession` 为了兼容已有签到缓存使用 snake_case；
/// 课表快照对前端统一输出 camelCase，避免两种命名混在同一个对象里。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub date: String,
    pub course_name: String,
    pub building_name: Option<String>,
    pub building_name_en: Option<String>,
    pub classroom: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub semester: Option<String>,
    pub checkin: SessionCheckin,
}

impl SessionView {
    pub fn new(session: TimetableSession, checkin: SessionCheckin) -> Self {
        Self {
            date: session.date,
            course_name: session.course_name,
            building_name: session.building_name,
            building_name_en: session.building_name_en,
            classroom: session.classroom,
            start_ms: session.start_ms,
            end_ms: session.end_ms,
            semester: session.semester,
            checkin,
        }
    }
}

/// 课表卡片上的签到状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionCheckin {
    #[default]
    None,
    PendingLms,
    PendingSzu,
    SignedLms,
    SignedSzu,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckinSource {
    #[default]
    Lms,
    Szu,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedCheckin {
    pub at_ms: i64,
    pub source: CheckinSource,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SemesterCache {
    pub term: String,
    pub courses: Vec<SemesterCourse>,
    pub fetched_at_ms: i64,
    pub attempted_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCheckin {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    /// 首次探测到的时间（不是每次探测都会变化的截止时间）。
    pub first_seen_ms: i64,
    pub route: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReminderState {
    /// 待签点名首次发现时间：id → 时间。
    pub checkin_first_seen: BTreeMap<String, i64>,
    /// 已提醒的待签点名：id → 提醒时间。
    pub checkin_notified: BTreeMap<String, i64>,
    /// 桌面端已发送的课前提醒：session_id → 提醒时间。
    pub class_notified: BTreeMap<String, i64>,
    /// 最近一次 Android 规格同步的签名，避免每 tick 重复 JNI。
    pub last_sync_signature: String,
    /// eHall 固定学期课表缓存。
    pub semester: SemesterCache,
    /// 学期课表最近一次刷新失败的提示；有缓存时只作为轻量提示。
    pub semester_message: String,
    /// 已观察到的签到完成记录：session_id → 来源/时间。
    pub checkin_signed: BTreeMap<String, SignedCheckin>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum TimetableEvent {
    Initial(Box<TimetableSnapshot>),
    Snapshot(Box<TimetableSnapshot>),
}

/// 传给 Android 原生通知桥的单节课规格。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClassReminderSpec {
    pub session_id: String,
    pub course_name: String,
    pub start_at_ms: i64,
    pub end_at_ms: i64,
    /// 本地时间区间，如 `08:30–10:05`。
    pub time_label: String,
    /// 楼栋 + 教室；没有地点时为空。
    pub location: String,
    pub route: String,
    pub lead_minutes: i64,
    /// 是否使用 Android 16 Live Update（promoted 实时活动）展示。
    pub live_update: bool,
}

/// 课次的稳定身份：同一课程同一天同一开始时间只提醒一次。
pub fn session_id(session: &TimetableSession) -> String {
    format!(
        "{}|{}|{}",
        session.course_name, session.date, session.start_ms
    )
}

pub fn session_route(session: &TimetableSession) -> String {
    format!("/study/timetable?tab=week&date={}", session.date)
}

pub fn session_time_label(session: &TimetableSession) -> String {
    let format = |ms: i64| {
        Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|value| value.format("%H:%M").to_string())
    };
    match (format(session.start_ms), format(session.end_ms)) {
        (Some(start), Some(end)) => format!("{start}–{end}"),
        (Some(start), None) => start,
        _ => String::new(),
    }
}

pub fn session_location(session: &TimetableSession) -> String {
    let building = session
        .building_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            session
                .building_name_en
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        });
    let classroom = session
        .classroom
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    match (building, classroom) {
        (Some(building), Some(classroom)) => format!("{building} {classroom}"),
        (Some(building), None) => building.to_string(),
        (None, Some(classroom)) => classroom.to_string(),
        (None, None) => String::new(),
    }
}

pub fn class_spec(session: &TimetableSession) -> ClassReminderSpec {
    class_spec_with_live(session, true)
}

/// 同 [`class_spec`]，可指定是否使用实时活动展示。
pub fn class_spec_with_live(
    session: &TimetableSession,
    live_update: bool,
) -> ClassReminderSpec {
    ClassReminderSpec {
        session_id: session_id(session),
        course_name: session.course_name.clone(),
        start_at_ms: session.start_ms,
        end_at_ms: session.end_ms,
        time_label: session_time_label(session),
        location: session_location(session),
        route: session_route(session),
        lead_minutes: CLASS_REMINDER_LEAD_MINUTES,
        live_update,
    }
}

/// Android 原生排程窗口：未来 72 小时内开始、且还没开始上课的课次。
///
/// 上课时刻一到就把课次从集合里剔除，`sync` 的差分逻辑随即撤掉它的闹钟、
/// 通知与实时活动——提醒只在课前显示。
pub fn class_sync_specs_with_live(
    sessions: &[TimetableSession],
    now: i64,
    live_update: bool,
) -> Vec<ClassReminderSpec> {
    sessions
        .iter()
        .filter(|session| session.start_ms > now && session.start_ms <= now + CLASS_SYNC_WINDOW_MS)
        .map(|session| class_spec_with_live(session, live_update))
        .collect()
}

/// 同 [`class_sync_specs_with_live`]，实时活动默认开启。
pub fn class_sync_specs(sessions: &[TimetableSession], now: i64) -> Vec<ClassReminderSpec> {
    class_sync_specs_with_live(sessions, now, true)
}

/// 当前应放进实时活动的那节课：`start - lead ≤ now < start` 中开始时间最晚的一节。
///
/// 实时活动是单条的（系统胶囊只有一个），连堂课或时间重叠时优先最近的课程；
/// 上课后不再保留实时活动。
pub fn active_live_spec<'a>(
    specs: &'a [ClassReminderSpec],
    now: i64,
) -> Option<&'a ClassReminderSpec> {
    specs
        .iter()
        .filter(|spec| spec.live_update)
        .filter(|spec| {
            let lead = spec.lead_minutes.max(1);
            now >= spec.start_at_ms - lead * MINUTE_MS && now < spec.start_at_ms
        })
        .max_by_key(|spec| spec.start_at_ms)
}

/// 桌面端没有常驻通知能力：只在进入 20 分钟窗口时发一次。
pub fn plan_desktop_class_reminders(
    sessions: &[TimetableSession],
    settings: &NotificationSettings,
    now: i64,
    state: &mut ReminderState,
) -> Vec<ClassReminderSpec> {
    if !settings.enabled || !settings.timetable.class_reminder {
        return Vec::new();
    }
    let lead_ms = CLASS_REMINDER_LEAD_MINUTES * MINUTE_MS;
    let mut live = std::collections::HashSet::new();
    let mut pending = Vec::new();
    for session in sessions {
        if session.start_ms <= now {
            continue;
        }
        let key = session_id(session);
        live.insert(key.clone());
        if session.start_ms - lead_ms <= now && !state.class_notified.contains_key(&key) {
            state.class_notified.insert(key, now);
            pending.push(class_spec(session));
        }
    }
    let horizon = now - CLASS_NOTIFIED_RETENTION_MS;
    state
        .class_notified
        .retain(|key, at| live.contains(key) && *at >= horizon);
    pending
}

/// 新的待签点名各提醒一次；`complete` 为真且条目消失时清理记录。
///
/// 普通待签条目受 `timetable.checkin` 控制；带失败原因（自动签到失败/缺照片
/// 跳过）的条目受 `timetable.checkin_failure` 控制，两者互不替代。
pub fn plan_checkin_reminders(
    view: &CheckinView,
    settings: &NotificationSettings,
    now: i64,
    state: &mut ReminderState,
) -> Vec<CheckinTodoItem> {
    let ids: std::collections::HashSet<&str> =
        view.items.iter().map(|item| item.id.as_str()).collect();
    if view.complete {
        state
            .checkin_first_seen
            .retain(|id, _| ids.contains(id.as_str()));
        state
            .checkin_notified
            .retain(|id, _| ids.contains(id.as_str()));
    }
    if !settings.enabled {
        return Vec::new();
    }
    let mut pending = Vec::new();
    for item in &view.items {
        state
            .checkin_first_seen
            .entry(item.id.clone())
            .or_insert(now);
        if state.checkin_notified.contains_key(&item.id) {
            continue;
        }
        let failure = item
            .subtitle
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let enabled = if failure {
            settings.timetable.checkin_failure
        } else {
            settings.timetable.checkin
        };
        if !enabled {
            // 开关关闭时不写 notified：之后重新开启仍会提醒。
            continue;
        }
        state.checkin_notified.insert(item.id.clone(), now);
        pending.push(item.clone());
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(start: i64) -> TimetableSession {
        TimetableSession {
            date: "2026-11-16".into(),
            course_name: "高等数学".into(),
            building_name: Some("致理楼".into()),
            building_name_en: None,
            classroom: Some("301".into()),
            start_ms: start,
            end_ms: start + 90 * MINUTE_MS,
            semester: None,
        }
    }

    fn checkin(id: &str) -> CheckinTodoItem {
        CheckinTodoItem {
            id: id.into(),
            title: format!("{id} 签到"),
            subtitle: None,
            due_at_ms: 0,
            route: "/study/signin?tab=lms".into(),
        }
    }

    fn failure_checkin(id: &str) -> CheckinTodoItem {
        CheckinTodoItem {
            subtitle: Some("自动签到失败：定位失败".into()),
            ..checkin(id)
        }
    }

    #[test]
    fn snapshot_sessions_serialize_camel_case() {
        let value =
            serde_json::to_value(SessionView::new(session(1_000), SessionCheckin::None)).unwrap();
        assert_eq!(value["courseName"], "高等数学");
        assert_eq!(value["startMs"], 1_000);
        assert!(value.get("course_name").is_none());
        assert!(value.get("start_ms").is_none());
    }

    #[test]
    fn checkin_notifies_once_and_clears_when_closed() {
        let settings = NotificationSettings::default();
        let mut state = ReminderState::default();
        let view = CheckinView {
            items: vec![checkin("a")],
            complete: true,
        };

        assert_eq!(
            plan_checkin_reminders(&view, &settings, 1_000, &mut state).len(),
            1
        );
        assert!(plan_checkin_reminders(&view, &settings, 2_000, &mut state).is_empty());
        assert_eq!(state.checkin_first_seen["a"], 1_000);

        let closed = CheckinView {
            items: Vec::new(),
            complete: true,
        };
        assert!(plan_checkin_reminders(&closed, &settings, 3_000, &mut state).is_empty());
        assert!(state.checkin_notified.is_empty());
        assert!(state.checkin_first_seen.is_empty());
    }

    #[test]
    fn untrusted_checkin_view_keeps_records() {
        let settings = NotificationSettings::default();
        let mut state = ReminderState::default();
        let view = CheckinView {
            items: vec![checkin("a")],
            complete: true,
        };
        plan_checkin_reminders(&view, &settings, 1_000, &mut state);

        let unavailable = CheckinView {
            items: Vec::new(),
            complete: false,
        };
        plan_checkin_reminders(&unavailable, &settings, 2_000, &mut state);
        assert!(state.checkin_notified.contains_key("a"));
    }

    #[test]
    fn failure_items_follow_the_failure_switch() {
        let mut settings = NotificationSettings::default();
        // 普通签到提醒关闭，失败提醒仍然开启：失败条目仍要提醒。
        settings.timetable.checkin = false;
        let view = CheckinView {
            items: vec![failure_checkin("a")],
            complete: true,
        };
        let mut state = ReminderState::default();
        assert_eq!(
            plan_checkin_reminders(&view, &settings, 1_000, &mut state).len(),
            1
        );

        // 关闭失败提醒：不提醒，也不写 notified，重新开启后补发。
        settings.timetable.checkin_failure = false;
        let mut state = ReminderState::default();
        assert!(plan_checkin_reminders(&view, &settings, 1_000, &mut state).is_empty());
        assert!(state.checkin_notified.is_empty());

        settings.timetable.checkin_failure = true;
        assert_eq!(
            plan_checkin_reminders(&view, &settings, 2_000, &mut state).len(),
            1
        );
    }

    #[test]
    fn failure_switch_does_not_send_generic_items() {
        let mut settings = NotificationSettings::default();
        settings.timetable.checkin = false;
        settings.timetable.checkin_failure = true;
        let view = CheckinView {
            items: vec![checkin("a")],
            complete: true,
        };
        let mut state = ReminderState::default();
        assert!(plan_checkin_reminders(&view, &settings, 1_000, &mut state).is_empty());
    }

    #[test]
    fn desktop_class_reminder_fires_only_inside_the_twenty_minute_window() {
        let settings = NotificationSettings::default();
        let mut state = ReminderState::default();
        let start = 10_000_000_000;
        let sessions = vec![session(start)];

        assert!(plan_desktop_class_reminders(
            &sessions,
            &settings,
            start - 21 * MINUTE_MS,
            &mut state
        )
        .is_empty());
        assert_eq!(
            plan_desktop_class_reminders(&sessions, &settings, start - 20 * MINUTE_MS, &mut state)
                .len(),
            1
        );
        assert!(plan_desktop_class_reminders(
            &sessions,
            &settings,
            start - 19 * MINUTE_MS,
            &mut state
        )
        .is_empty());
        assert!(plan_desktop_class_reminders(&sessions, &settings, start, &mut state).is_empty());
    }

    fn live_spec(id: &str, start: i64, end: i64, live_update: bool) -> ClassReminderSpec {
        ClassReminderSpec {
            session_id: id.into(),
            course_name: id.into(),
            start_at_ms: start,
            end_at_ms: end,
            time_label: String::new(),
            location: String::new(),
            route: String::new(),
            lead_minutes: 20,
            live_update,
        }
    }

    #[test]
    fn live_update_is_flagged_per_spec() {
        let now = 1_800_000_000_000;
        let sessions = vec![session(now + 60 * MINUTE_MS)];
        assert!(class_sync_specs_with_live(&sessions, now, true)
            .iter()
            .all(|spec| spec.live_update));
        assert!(class_sync_specs_with_live(&sessions, now, false)
            .iter()
            .all(|spec| !spec.live_update));
    }

    #[test]
    fn active_live_spec_only_covers_the_pre_class_window() {
        let base = 1_800_000_000_000;
        let specs = vec![
            live_spec("a", base, base + 60 * MINUTE_MS, true),
            live_spec("b", base + 30 * MINUTE_MS, base + 120 * MINUTE_MS, true),
        ];
        // 窗口外（开课前 21 分钟）。
        assert!(active_live_spec(&specs, base - 21 * MINUTE_MS).is_none());
        // 第一节开课前 20 分钟：进入窗口。
        assert_eq!(
            active_live_spec(&specs, base - 20 * MINUTE_MS).map(|spec| spec.session_id.as_str()),
            Some("a")
        );
        // 第一节上课时刻：立即收起，不再显示「上课中」。
        assert!(active_live_spec(&specs, base).is_none());
        // 第二节开课前 15 分钟：轮到第二节。
        assert_eq!(
            active_live_spec(&specs, base + 15 * MINUTE_MS).map(|spec| spec.session_id.as_str()),
            Some("b")
        );
        // 第二节也上课后没有实时活动。
        assert!(active_live_spec(&specs, base + 30 * MINUTE_MS).is_none());
        assert!(active_live_spec(&specs, base + 200 * MINUTE_MS).is_none());
    }

    #[test]
    fn active_live_spec_skips_specs_without_live_update() {
        let base = 1_800_000_000_000;
        let specs = vec![live_spec("a", base, base + 60 * MINUTE_MS, false)];
        assert!(active_live_spec(&specs, base - 10 * MINUTE_MS).is_none());
    }

    #[test]
    fn sync_specs_drop_a_class_at_its_start() {
        let start = 1_800_000_000_000;
        let sessions = vec![session(start)];
        assert_eq!(
            class_sync_specs_with_live(&sessions, start - 1, true).len(),
            1
        );
        assert!(class_sync_specs_with_live(&sessions, start, true).is_empty());
        assert!(class_sync_specs_with_live(&sessions, start + 1, true).is_empty());
    }
}
