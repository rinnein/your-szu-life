use std::collections::HashSet;

use chrono::{Local, TimeZone};

use super::aggregate::{
    electricity_trusted, from_electricity, from_lms, from_oj, from_uooc, lms_trusted,
    parse_uooc_datetime, uooc_trusted_kinds,
};
use super::model::{item_key, ReminderState, TodoItem, TodoKind};
use super::reminder::{establish_discovery_baseline, plan_reminders, prune_closed};
use szu_core::campus_payments::ElectricityStatusView;
use szu_lms::lms::model::{SessionState, Snapshot as LmsSnapshot, Todo as LmsTodo};
use szu_core::notification::settings::NotificationSettings;
use szu_oj::oj::OjDeadline;
use szu_uooc::uooc::model::{Course, ExamTask, SessionStatus, Snapshot as UoocSnapshot};
use szu_uooc::uooc::Assessments;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

fn now() -> i64 {
    1_700_000_000_000
}

fn item(id: &str, kind: TodoKind, due_at_ms: i64) -> TodoItem {
    TodoItem {
        id: id.into(),
        kind,
        title: id.into(),
        subtitle: None,
        due_at_ms,
        route: "/".into(),
        external_url: None,
        urgent: false,
    }
}

/// 已经见过这些待办的状态：用于只验证提前量逻辑的用例。
fn seen_state(items: &[TodoItem]) -> ReminderState {
    let mut state = ReminderState::default();
    for item in items {
        state.discovered.insert(item_key(item), item.due_at_ms);
    }
    state
}

fn oj_deadline(id: &str, end: i64, accepted: i64, total: i64) -> OjDeadline {
    OjDeadline {
        site_id: "site".into(),
        site_name: "SZU OJ".into(),
        group_id: "group".into(),
        id: id.into(),
        title: format!("contest {id}"),
        begin: end - DAY,
        end,
        now: now(),
        is_exam: false,
        accepted,
        total,
        has_progress: total > 0,
    }
}

#[test]
fn oj_filters_finished_and_completed_contests() {
    let items = from_oj(
        vec![
            oj_deadline("upcoming", now() + HOUR, 0, 0),
            oj_deadline("past", now() - 1, 0, 0),
            oj_deadline("ac", now() + DAY, 3, 3),
        ],
        now(),
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "oj:site:upcoming");
    assert_eq!(items[0].kind, TodoKind::OjQuiz);
    assert_eq!(items[0].route, "/study/oj?site=site&contest=upcoming");
    assert!(items[0].urgent);
}

fn course(id: &str, end_date: &str) -> Course {
    Course {
        id: id.into(),
        title: format!("course {id}"),
        semester: "2026 秋".into(),
        organization: "SZU".into(),
        weeks: "1/16".into(),
        end_date: end_date.into(),
        status: "31".into(),
    }
}

fn exam(id: &str, end_time: &str, status_code: i32, can_view: bool) -> ExamTask {
    ExamTask {
        id: id.into(),
        title: format!("exam {id}"),
        cid: "c1".into(),
        course_name: "人工智能".into(),
        start_time: "2026-10-01 00:00:00".into(),
        end_time: end_time.into(),
        limit_time: 0,
        status: 0,
        status_text: String::new(),
        status_code,
        can_do: true,
        can_view,
        can_eval: false,
        chance: 0,
        submit_count: 0,
        score: None,
        paper_id: None,
        task_id: id.into(),
        chapter_text: None,
        section_text: None,
    }
}

#[test]
fn uooc_aggregates_course_end_and_unfinished_exams() {
    let mut snapshot = UoocSnapshot::default();
    snapshot.session = SessionStatus::Ready;
    snapshot.courses = vec![course("c1", "2099/12/31")];
    let assessments = Assessments {
        exams: vec![
            exam("open", "2099-12-31 18:00:00", 0, false),
            exam("done", "2099-12-31 18:00:00", 20, false),
        ],
        homeworks: Vec::new(),
        loaded: true,
    };

    let items = from_uooc(&snapshot, &assessments, now());
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| item.id == "uooc:course:c1"));
    assert!(items.iter().any(|item| item.id == "uooc:exam:open"));
    assert!(!items.iter().any(|item| item.id == "uooc:exam:done"));
}

#[test]
fn uooc_skips_everything_until_session_is_ready() {
    let mut snapshot = UoocSnapshot::default();
    snapshot.session = SessionStatus::Checking;
    snapshot.courses = vec![course("c1", "2099/12/31")];
    let items = from_uooc(&snapshot, &Assessments::default(), now());
    assert!(items.is_empty());
}

#[test]
fn uooc_datetime_parses_local_strings() {
    let expected =
        Local.with_ymd_and_hms(2026, 10, 30, 18, 0, 0).unwrap().timestamp_millis();
    assert_eq!(
        parse_uooc_datetime("2026-10-30 18:00:00"),
        Some(expected)
    );

    let end_of_day =
        Local.with_ymd_and_hms(2026, 10, 14, 23, 59, 59).unwrap().timestamp_millis();
    assert_eq!(parse_uooc_datetime("2026/10/14"), Some(end_of_day));
    assert_eq!(parse_uooc_datetime(""), None);
    assert_eq!(parse_uooc_datetime("not a date"), None);
}

fn lms_todo(id: i64, end_time: &str) -> LmsTodo {
    LmsTodo {
        course_code: "code".into(),
        course_id: 42,
        course_name: "数据库系统".into(),
        course_type: 0,
        end_time: end_time.into(),
        id,
        is_locked: false,
        is_student: true,
        prerequisites: Vec::new(),
        title: format!("todo {id}"),
        kind: "homework".into(),
    }
}

#[test]
fn lms_keeps_future_todos_and_builds_external_url() {
    let future = (now() + DAY) / 1000;
    let past = (now() - DAY) / 1000;
    let snapshot = LmsSnapshot {
        session: SessionState::Ready,
        message: String::new(),
        profile: None,
        todos: vec![
            lms_todo(1, &chrono::DateTime::from_timestamp(future, 0).unwrap().to_rfc3339()),
            lms_todo(2, &chrono::DateTime::from_timestamp(past, 0).unwrap().to_rfc3339()),
        ],
        notifications: Vec::new(),
        unread_count: 0,
        fetched_at: None,
    };

    let items = from_lms(&snapshot, now());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "lms:homework:1");
    assert_eq!(
        items[0].external_url.as_deref(),
        Some("https://lms.szu.edu.cn/course/42/learning-activity#/1?view=scores")
    );
}

#[test]
fn electricity_forecast_uses_observed_date_and_estimated_days() {
    let status = ElectricityStatusView {
        remaining: Some(3.25),
        observed_date: Some("2026-11-01".into()),
        expires_at_ms: None,
        expired: false,
        low: false,
        estimated_days_left: Some(1.5),
        warning_reason: None,
    };
    let items = from_electricity(&status, now());
    assert_eq!(items.len(), 1);
    let expected = Local.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap().timestamp_millis()
        + (1.5 * DAY as f64) as i64;
    assert_eq!(items[0].due_at_ms, expected);
    assert_eq!(items[0].kind, TodoKind::ElectricityOutage);
}

#[test]
fn reminders_fire_once_per_lead_time() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![7 * 24 * 60, 3 * 24 * 60];
    let due = now() + 8 * DAY;
    let items = vec![item("t", TodoKind::OjQuiz, due)];
    let mut state = seen_state(&items);

    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());

    let at_seven_days = due - 7 * DAY;
    let pending = plan_reminders(&items, &settings, at_seven_days, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, Some(7 * 24 * 60));
    assert!(plan_reminders(&items, &settings, at_seven_days, &mut state).is_empty());

    let at_three_days = due - 3 * DAY;
    let pending = plan_reminders(&items, &settings, at_three_days, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, Some(3 * 24 * 60));
    assert_eq!(state.notified.len(), 2);
}

#[test]
fn reminders_collapse_missed_lead_times_to_the_most_urgent() {
    let settings = NotificationSettings::default();
    let due = now() + DAY - MINUTE;
    let items = vec![item("t", TodoKind::UoocHomework, due)];
    let mut state = seen_state(&items);

    // 默认勾选的 7天/3天/1天三档都已到达，只发最接近截止的 1 天档。
    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, Some(24 * 60));
    assert_eq!(state.notified.len(), 3);
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());
}

#[test]
fn reminders_respect_kind_module_switch_and_past_due() {
    let mut settings = NotificationSettings::default();
    let due = now() + 5 * MINUTE;
    let items = vec![item("t", TodoKind::LmsHomework, due)];

    let mut disabled_kind = seen_state(&items);
    settings.todo.lms_homework = false;
    assert!(plan_reminders(&items, &settings, now(), &mut disabled_kind).is_empty());
    assert!(disabled_kind.notified.is_empty());

    settings.todo.lms_homework = true;
    settings.todo.enabled = false;
    let mut disabled_module = seen_state(&items);
    assert!(plan_reminders(&items, &settings, now(), &mut disabled_module).is_empty());

    settings.todo.enabled = true;
    let mut enabled = seen_state(&items);
    assert_eq!(plan_reminders(&items, &settings, now(), &mut enabled).len(), 1);

    let mut past = seen_state(&items);
    assert!(plan_reminders(&items, &settings, due, &mut past).is_empty());
}

#[test]
fn reminders_rearm_when_deadline_changes() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![24 * 60];
    let old_due = now() + DAY;
    let items = vec![item("t", TodoKind::OjQuiz, old_due)];
    let mut state = seen_state(&items);
    assert_eq!(plan_reminders(&items, &settings, now(), &mut state).len(), 1);

    let moved = vec![item("t", TodoKind::OjQuiz, old_due + HOUR)];
    assert_eq!(
        plan_reminders(&moved, &settings, now() + HOUR, &mut state).len(),
        1
    );
}

#[test]
fn discovery_reminder_fires_once_when_item_first_seen() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![24 * 60];
    let due = now() + 2 * DAY;
    let items = vec![item("t", TodoKind::OjQuiz, due)];
    let mut state = ReminderState::default();

    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, None);
    assert!(state.discovered.contains_key(&item_key(&items[0])));

    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());
}

#[test]
fn discovery_reminder_swallows_lead_times_already_reached() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![24 * 60, 60];
    let due = now() + 5 * MINUTE;
    let items = vec![item("t", TodoKind::UoocExam, due)];
    let mut state = ReminderState::default();

    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, None);
    // 两档都已到达，但随初次探测一起吞掉，不会立刻再补一条。
    assert_eq!(state.notified.len(), 2);
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());
}

#[test]
fn discovery_reminder_works_without_lead_minutes() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let due = now() + DAY;
    let items = vec![item("t", TodoKind::LmsHomework, due)];
    let mut state = ReminderState::default();

    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, None);
}

#[test]
fn fresh_install_notifies_for_the_first_batch() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let due = now() + DAY;
    let items = vec![item("t", TodoKind::OjQuiz, due)];
    let mut state = ReminderState::default();
    let trusted = HashSet::from([TodoKind::OjQuiz]);

    establish_discovery_baseline(&items, &trusted, &mut state);
    assert!(state.discovered.is_empty());
    assert_eq!(plan_reminders(&items, &settings, now(), &mut state).len(), 1);
}

#[test]
fn legacy_state_establishes_silent_discovery_baseline_per_kind() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let existing = item("old", TodoKind::OjQuiz, now() + DAY);
    let items = vec![existing.clone()];
    let mut state = ReminderState {
        discovery_baseline_version: 0,
        ..ReminderState::default()
    };
    let trusted = HashSet::from([TodoKind::OjQuiz]);

    establish_discovery_baseline(&items, &trusted, &mut state);
    assert!(state.discovered.contains_key(&item_key(&existing)));
    assert!(state.baselined_kinds.contains(&TodoKind::OjQuiz));
    // 基线只记录，不产生通知。
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());

    // 同一来源之后新增的待办照常提醒。
    let fresh = item("new", TodoKind::OjQuiz, now() + DAY);
    let pending = plan_reminders(&[fresh], &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, None);
}

#[test]
fn closed_item_clears_records_and_rearms_on_reappearance() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![24 * 60];
    let due = now() + DAY;
    let items = vec![item("t", TodoKind::OjQuiz, due)];
    let mut state = ReminderState::default();
    assert_eq!(plan_reminders(&items, &settings, now(), &mut state).len(), 1);
    assert_eq!(state.discovered.len(), 1);
    assert_eq!(state.notified.len(), 1);

    prune_closed(&[], &HashSet::from([TodoKind::OjQuiz]), now(), &mut state);
    assert!(state.discovered.is_empty());
    assert!(state.notified.is_empty());

    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, None);
}

#[test]
fn untrusted_source_keeps_records() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let items = vec![item("t", TodoKind::UoocExam, now() + DAY)];
    let mut state = ReminderState::default();
    plan_reminders(&items, &settings, now(), &mut state);

    prune_closed(&[], &HashSet::from([TodoKind::OjQuiz]), now(), &mut state);
    assert!(state.discovered.contains_key(&item_key(&items[0])));
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());
}

#[test]
fn closed_item_pruning_is_scoped_to_the_trusted_kind() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let oj = item("oj", TodoKind::OjQuiz, now() + DAY);
    let uooc = item("uooc", TodoKind::UoocExam, now() + DAY);
    let items = vec![oj.clone(), uooc.clone()];
    let mut state = ReminderState::default();
    plan_reminders(&items, &settings, now(), &mut state);

    prune_closed(&[], &HashSet::from([TodoKind::OjQuiz]), now(), &mut state);
    assert!(!state.discovered.contains_key(&item_key(&oj)));
    assert!(state.discovered.contains_key(&item_key(&uooc)));
}

#[test]
fn discovered_records_fall_back_to_the_due_horizon() {
    let mut state = ReminderState::default();
    let old = item("old", TodoKind::UoocExam, now() - 31 * DAY);
    let fresh = item("fresh", TodoKind::UoocExam, now() + DAY);
    state.discovered.insert(item_key(&old), old.due_at_ms);
    state.discovered.insert(item_key(&fresh), fresh.due_at_ms);

    prune_closed(&[], &HashSet::new(), now(), &mut state);
    assert!(!state.discovered.contains_key(&item_key(&old)));
    assert!(state.discovered.contains_key(&item_key(&fresh)));
}

#[test]
fn live_items_are_never_pruned_by_the_due_horizon() {
    let item = item("t", TodoKind::UoocExam, now() - 31 * DAY);
    let items = vec![item.clone()];
    let mut state = ReminderState::default();
    state.discovered.insert(item_key(&item), item.due_at_ms);

    // 截止时间可能被延长；只要仍在当前列表里，就不能按旧 due 丢掉探测记录。
    prune_closed(&items, &HashSet::new(), now(), &mut state);
    assert!(state.discovered.contains_key(&item_key(&item)));
}

#[test]
fn source_trust_requires_a_complete_current_view() {
    let mut snapshot = UoocSnapshot::default();
    snapshot.session = SessionStatus::Ready;
    assert_eq!(
        uooc_trusted_kinds(&snapshot, &Assessments::default()),
        vec![TodoKind::UoocCourseEnd]
    );
    let loaded = Assessments {
        exams: Vec::new(),
        homeworks: Vec::new(),
        loaded: true,
    };
    assert_eq!(
        uooc_trusted_kinds(&snapshot, &loaded),
        vec![
            TodoKind::UoocCourseEnd,
            TodoKind::UoocExam,
            TodoKind::UoocHomework
        ]
    );
    snapshot.session = SessionStatus::NetworkError;
    assert!(uooc_trusted_kinds(&snapshot, &loaded).is_empty());

    assert!(!lms_trusted(&lms_snapshot_with(SessionState::Ready, None)));
    assert!(lms_trusted(&lms_snapshot_with(SessionState::Ready, Some(now()))));
    assert!(!lms_trusted(&lms_snapshot_with(
        SessionState::Unavailable,
        Some(now())
    )));

    let valid = ElectricityStatusView {
        remaining: Some(3.25),
        observed_date: None,
        expires_at_ms: None,
        expired: false,
        low: false,
        estimated_days_left: None,
        warning_reason: None,
    };
    assert!(electricity_trusted(&valid));
    let expired = ElectricityStatusView {
        expired: true,
        ..valid.clone()
    };
    assert!(!electricity_trusted(&expired));
    let missing = ElectricityStatusView {
        remaining: None,
        ..valid
    };
    assert!(!electricity_trusted(&missing));
}

#[test]
fn legacy_reminder_state_json_defaults_to_pre_baseline_version() {
    let legacy: ReminderState =
        serde_json::from_str(r#"{"notified":{"oj-quiz|t|1|60":1},"scheduled":{}}"#).unwrap();
    // 旧版本 JSON 缺少新字段：字段级 default 给 0，与全新安装的 Default(1) 不同。
    assert_eq!(legacy.discovery_baseline_version, 0);
    assert!(legacy.discovered.is_empty());
    assert!(legacy.baselined_kinds.is_empty());
    assert_eq!(ReminderState::default().discovery_baseline_version, 1);
}

#[tokio::test]
async fn reminder_state_roundtrips_through_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let database = crate::todos::db::TodoDatabase::open(&dir.path().join("todos.sqlite"))
        .await
        .unwrap();

    assert!(database.load_state().await.unwrap().notified.is_empty());

    let mut state = super::model::ReminderState::default();
    state.notified.insert("oj-quiz|t|1|60".into(), 42);
    state.scheduled.insert("oj-quiz|t|1|60".into(), 7);
    state.discovered.insert("oj-quiz|t".into(), 1_700_000_000_000);
    state.baselined_kinds.insert(TodoKind::OjQuiz);
    state.discovery_baseline_version = 0;
    database.save_state(&state).await.unwrap();

    let loaded = database.load_state().await.unwrap();
    assert_eq!(loaded.notified["oj-quiz|t|1|60"], 42);
    assert_eq!(loaded.scheduled["oj-quiz|t|1|60"], 7);
    assert_eq!(loaded.discovered["oj-quiz|t"], 1_700_000_000_000);
    assert!(loaded.baselined_kinds.contains(&TodoKind::OjQuiz));
    assert_eq!(loaded.discovery_baseline_version, 0);
}

fn lms_snapshot_with(session: SessionState, fetched_at: Option<i64>) -> LmsSnapshot {
    LmsSnapshot {
        session,
        message: String::new(),
        profile: None,
        todos: Vec::new(),
        notifications: Vec::new(),
        unread_count: 0,
        fetched_at,
    }
}

/// 首页待办只在「畅课确实未登录且没有任何缓存」时自动登录一次；
/// 定时轮询与用户手动刷新都不走这条捷径（后者本来就会 force 重登）。
#[test]
fn home_subscription_auto_logs_in_only_a_cold_needs_auth_snapshot() {
    use super::{lms_needs_auto_login, RefreshSource};

    let cases = [
        (SessionState::NeedsAuth, false, RefreshSource::Subscription, true),
        (SessionState::NeedsAuth, false, RefreshSource::Periodic, false),
        (SessionState::NeedsAuth, false, RefreshSource::Manual, false),
        (SessionState::NeedsAuth, true, RefreshSource::Subscription, false),
        (SessionState::Unavailable, false, RefreshSource::Subscription, false),
        (SessionState::Ready, false, RefreshSource::Subscription, false),
    ];
    for (session, has_cache, source, expected) in cases {
        let snapshot = lms_snapshot_with(session, has_cache.then_some(now()));
        assert_eq!(
            lms_needs_auto_login(&snapshot, source),
            expected,
            "session={session:?} has_cache={has_cache} source={source:?}"
        );
    }
}
