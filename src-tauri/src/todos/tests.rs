use chrono::{Local, TimeZone};

use super::aggregate::{from_electricity, from_lms, from_oj, from_uooc, parse_uooc_datetime};
use super::model::{TodoItem, TodoKind};
use super::reminder::plan_reminders;
use crate::campus_payments::ElectricityStatusView;
use crate::lms::model::{SessionState, Snapshot as LmsSnapshot, Todo as LmsTodo};
use crate::notification::settings::NotificationSettings;
use crate::oj::OjDeadline;
use crate::uooc::model::{Course, ExamTask, SessionStatus, Snapshot as UoocSnapshot};
use crate::uooc::Assessments;

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
    let mut state = Default::default();

    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());

    let at_seven_days = due - 7 * DAY;
    let pending = plan_reminders(&items, &settings, at_seven_days, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, 7 * 24 * 60);
    assert!(plan_reminders(&items, &settings, at_seven_days, &mut state).is_empty());

    let at_three_days = due - 3 * DAY;
    let pending = plan_reminders(&items, &settings, at_three_days, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, 3 * 24 * 60);
    assert_eq!(state.notified.len(), 2);
}

#[test]
fn reminders_collapse_missed_lead_times_to_the_most_urgent() {
    let settings = NotificationSettings::default();
    let due = now() + DAY - MINUTE;
    let items = vec![item("t", TodoKind::UoocHomework, due)];
    let mut state = Default::default();

    // 默认勾选的 7天/3天/1天三档都已到达，只发最接近截止的 1 天档。
    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, 24 * 60);
    assert_eq!(state.notified.len(), 3);
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());
}

#[test]
fn reminders_respect_kind_module_switch_and_past_due() {
    let mut settings = NotificationSettings::default();
    let due = now() + 5 * MINUTE;
    let items = vec![item("t", TodoKind::LmsHomework, due)];

    let mut disabled_kind = Default::default();
    settings.todo.lms_homework = false;
    assert!(plan_reminders(&items, &settings, now(), &mut disabled_kind).is_empty());
    assert!(disabled_kind.notified.is_empty());

    settings.todo.lms_homework = true;
    settings.todo.enabled = false;
    let mut disabled_module = Default::default();
    assert!(plan_reminders(&items, &settings, now(), &mut disabled_module).is_empty());

    settings.todo.enabled = true;
    let mut enabled = Default::default();
    assert_eq!(plan_reminders(&items, &settings, now(), &mut enabled).len(), 1);

    let mut past = Default::default();
    assert!(plan_reminders(&items, &settings, due, &mut past).is_empty());
}

#[test]
fn reminders_rearm_when_deadline_changes() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![24 * 60];
    let old_due = now() + DAY;
    let mut state = Default::default();
    let items = vec![item("t", TodoKind::OjQuiz, old_due)];
    assert_eq!(plan_reminders(&items, &settings, now(), &mut state).len(), 1);

    let moved = vec![item("t", TodoKind::OjQuiz, old_due + HOUR)];
    assert_eq!(
        plan_reminders(&moved, &settings, now() + HOUR, &mut state).len(),
        1
    );
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
    database.save_state(&state).await.unwrap();

    let loaded = database.load_state().await.unwrap();
    assert_eq!(loaded.notified["oj-quiz|t|1|60"], 42);
    assert_eq!(loaded.scheduled["oj-quiz|t|1|60"], 7);
}
