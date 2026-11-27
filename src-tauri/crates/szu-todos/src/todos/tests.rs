use std::collections::HashSet;

use chrono::{Local, TimeZone, Timelike};

use super::aggregate::{
    electricity_trusted, from_electricity, from_evaluation, from_lms, from_oj, from_uooc,
    lms_trusted, parse_uooc_datetime, uooc_trusted_kinds,
};
use super::model::{item_key, ReminderState, TodoItem, TodoKind};
use super::reminder::{establish_discovery_baseline, plan_reminders, prune_closed, ReminderEvent};
use szu_core::campus_payments::ElectricityStatusView;
use szu_lms::lms::model::{SessionState, Snapshot as LmsSnapshot, Todo as LmsTodo};
use szu_core::notification::settings::NotificationSettings;
use szu_enrollment::enrollment::evaluation::PendingSummary;
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
        start_at_ms: None,
        due_at_ms,
        route: "/".into(),
        external_url: None,
        urgent: false,
    }
}

fn item_starting(id: &str, kind: TodoKind, start_at_ms: i64, due_at_ms: i64) -> TodoItem {
    TodoItem {
        start_at_ms: Some(start_at_ms),
        ..item(id, kind, due_at_ms)
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
    assert_eq!(items[0].start_at_ms, Some(now() + HOUR - DAY));
    assert!(items[0].urgent);
}

#[test]
fn oj_skips_contests_marked_as_not_graded() {
    // 标题含「不计分」的测验不计入成绩，不该出现在首页待办里（也不该触发提醒）。
    let mut ungraded = oj_deadline("ungraded", now() + HOUR, 0, 0);
    ungraded.title = "第一章练习（不计分）".into();
    let mut ungraded_exam = oj_deadline("ungraded-exam", now() + 2 * HOUR, 0, 0);
    ungraded_exam.title = "不计分 模拟测试".into();
    ungraded_exam.is_exam = true;
    let items = from_oj(
        vec![
            ungraded,
            ungraded_exam,
            oj_deadline("graded", now() + 3 * HOUR, 0, 0),
        ],
        now(),
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "oj:site:graded");
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
    assert!(!items.iter().any(|item| item.id == "uooc:exam:done"));
    let open = items.iter().find(|item| item.id == "uooc:exam:open").unwrap();
    assert_eq!(
        open.start_at_ms,
        parse_uooc_datetime("2026-10-01 00:00:00")
    );
    // 课程结课没有明确开始时间。
    let course_end = items.iter().find(|item| item.id == "uooc:course:c1").unwrap();
    assert_eq!(course_end.start_at_ms, None);
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
fn electricity_forecast_starts_from_now_and_lands_on_the_day_end() {
    let status = electricity_status(false, Some(1.5));
    let items = from_electricity(&status, now());
    assert_eq!(items.len(), 1);
    // 估算以「此刻还剩多少」为起点，再落到那一天的日终，避免读数抖动改写去重键。
    assert_eq!(
        items[0].due_at_ms,
        end_of_local_day(now() + (1.5 * DAY as f64) as i64)
    );
    assert_eq!(items[0].kind, TodoKind::ElectricityOutage);
    assert_eq!(items[0].start_at_ms, None);
    assert!(items[0].title.contains("预计电量耗尽"));
}

fn electricity_status(low: bool, estimated_days_left: Option<f64>) -> ElectricityStatusView {
    ElectricityStatusView {
        remaining: Some(3.25),
        observed_date: Some("2026-11-01".into()),
        expires_at_ms: None,
        expired: false,
        low,
        estimated_days_left,
        warning_reason: low.then(|| "宿舍剩余电量低于 5 度，请及时充值。".into()),
    }
}

fn end_of_local_day(ms: i64) -> i64 {
    let today = Local.timestamp_millis_opt(ms).single().unwrap().date_naive();
    Local
        .from_local_datetime(&today.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap()
        .timestamp_millis()
        - 1
}

#[test]
fn electricity_low_without_estimate_falls_back_to_end_of_today() {
    let status = electricity_status(true, None);
    let items = from_electricity(&status, now());
    assert_eq!(items.len(), 1);
    assert!(items[0].title.contains("低于 5 度"));
    assert!(items[0].subtitle.as_deref().unwrap().contains("电费预警"));
    assert!(items[0].subtitle.as_deref().unwrap().contains("剩余约"));
    assert!(items[0].urgent);
    assert_eq!(items[0].due_at_ms, end_of_local_day(now()));
}

#[test]
fn electricity_low_with_estimate_keeps_the_forecast_day() {
    let items = from_electricity(&electricity_status(true, Some(2.0)), now());
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].due_at_ms,
        end_of_local_day(now() + 2 * DAY)
    );
}

#[test]
fn electricity_overdraft_is_reported_as_such() {
    let mut status = electricity_status(true, Some(0.0));
    status.remaining = Some(-1.82);
    // 读数过期也必须照常提醒：门户延迟发布期间正是最需要预警的时候。
    status.expired = true;
    let items = from_electricity(&status, now());
    assert_eq!(items.len(), 1);
    let subtitle = items[0].subtitle.as_deref().unwrap();
    assert!(subtitle.contains("已透支 1.8 度"));
    assert!(subtitle.contains("读数已过期"));
    assert_eq!(items[0].due_at_ms, end_of_local_day(now()));
}

#[test]
fn electricity_without_low_or_estimate_produces_nothing() {
    assert!(from_electricity(&electricity_status(false, None), now()).is_empty());
}

#[test]
fn electricity_reminders_fire_at_most_once_per_day() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![2 * 24 * 60, 24 * 60];
    let due = now() + 2 * DAY;
    let items = vec![item("outage", TodoKind::ElectricityOutage, due)];
    let mut state = seen_state(&items);

    // 进入两天的预警档：第一次提醒。
    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Lead(2 * 24 * 60));
    assert!(state.notified_day.contains_key(&item_key(&items[0])));

    // 同一天估算微调不再补发第二条。
    let moved = vec![item("outage", TodoKind::ElectricityOutage, due + 5 * MINUTE)];
    assert!(plan_reminders(&moved, &settings, now() + MINUTE, &mut state).is_empty());

    // 第二天新到达的 1 天档照常提醒一次。
    let pending = plan_reminders(&items, &settings, now() + DAY, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Lead(24 * 60));
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
    assert_eq!(pending[0].1, ReminderEvent::Lead(7 * 24 * 60));
    assert!(plan_reminders(&items, &settings, at_seven_days, &mut state).is_empty());

    let at_three_days = due - 3 * DAY;
    let pending = plan_reminders(&items, &settings, at_three_days, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Lead(3 * 24 * 60));
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
    assert_eq!(pending[0].1, ReminderEvent::Lead(24 * 60));
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
    assert_eq!(pending[0].1, ReminderEvent::Discovered);
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
    assert_eq!(pending[0].1, ReminderEvent::Discovered);
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
    assert_eq!(pending[0].1, ReminderEvent::Discovered);
}

#[test]
fn discovery_can_be_disabled_without_losing_lead_reminders() {
    let mut settings = NotificationSettings::default();
    settings.todo.discovery = false;
    settings.todo.lead_minutes = vec![60];
    let due = now() + 30 * MINUTE;
    let items = vec![item("t", TodoKind::OjQuiz, due)];
    let mut state = ReminderState::default();

    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Lead(60));
    assert!(state.discovered.contains_key(&item_key(&items[0])));
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());
}

#[test]
fn started_reminder_fires_once_when_crossing_start() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let start = now() + HOUR;
    let items = vec![item_starting("t", TodoKind::OjQuiz, start, now() + DAY)];
    let mut state = seen_state(&items);

    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());

    let pending = plan_reminders(&items, &settings, start, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Started);
    assert!(state.started.contains_key(&item_key(&items[0])));
    assert!(plan_reminders(&items, &settings, start + HOUR, &mut state).is_empty());
}

#[test]
fn started_reminder_rearms_when_start_moves_later() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let start = now() + HOUR;
    let items = vec![item_starting("t", TodoKind::OjQuiz, start, now() + DAY)];
    let mut state = seen_state(&items);
    let pending = plan_reminders(&items, &settings, start, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Started);

    let moved = vec![item_starting("t", TodoKind::OjQuiz, start + HOUR, now() + DAY)];
    let pending = plan_reminders(&moved, &settings, start + HOUR, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Started);
    assert!(plan_reminders(&moved, &settings, start + 2 * HOUR, &mut state).is_empty());
}

#[test]
fn discovery_suppresses_started_when_start_already_passed() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let items = vec![item_starting(
        "t",
        TodoKind::OjQuiz,
        now() - HOUR,
        now() + DAY,
    )];
    let mut state = ReminderState::default();

    let pending = plan_reminders(&items, &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Discovered);
    assert!(state.started.contains_key(&item_key(&items[0])));
}

#[test]
fn started_yields_to_a_lead_reminder_due_in_the_same_tick() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes = vec![60];
    let start = now() + HOUR;
    let items = vec![item_starting("t", TodoKind::UoocExam, start, now() + 2 * HOUR)];
    let mut state = seen_state(&items);

    let pending = plan_reminders(&items, &settings, start + 30 * MINUTE, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Lead(60));
    assert!(state.started.contains_key(&item_key(&items[0])));
}

#[test]
fn baseline_v2_silences_started_for_in_progress_items() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let running = item_starting("running", TodoKind::OjQuiz, now() - HOUR, now() + DAY);
    let future = item_starting("future", TodoKind::OjQuiz, now() + HOUR, now() + DAY);
    let items = vec![running.clone(), future.clone()];
    let mut state = ReminderState {
        discovery_baseline_version: 1,
        ..ReminderState::default()
    };
    state.discovered.insert(item_key(&running), running.due_at_ms);
    state.discovered.insert(item_key(&future), future.due_at_ms);

    establish_discovery_baseline(&items, &HashSet::new(), now(), &mut state);
    assert_eq!(state.discovery_baseline_version, 2);
    assert!(state.started.contains_key(&item_key(&running)));
    assert!(!state.started.contains_key(&item_key(&future)));
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());

    // 未来才开始的任务仍然照常提醒。
    let pending = plan_reminders(&items, &settings, now() + HOUR, &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Started);
}

#[test]
fn closed_item_clears_started_records() {
    let item = item_starting("t", TodoKind::OjQuiz, now() - HOUR, now() + DAY);
    let mut state = ReminderState::default();
    state.discovered.insert(item_key(&item), item.due_at_ms);
    state.started.insert(item_key(&item), item.start_at_ms.unwrap());

    prune_closed(&[], &HashSet::from([TodoKind::OjQuiz]), now(), &mut state);
    assert!(state.started.is_empty());
}

#[test]
fn fresh_install_notifies_for_the_first_batch() {
    let mut settings = NotificationSettings::default();
    settings.todo.lead_minutes.clear();
    let due = now() + DAY;
    let items = vec![item("t", TodoKind::OjQuiz, due)];
    let mut state = ReminderState::default();
    let trusted = HashSet::from([TodoKind::OjQuiz]);

    establish_discovery_baseline(&items, &trusted, now(), &mut state);
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

    establish_discovery_baseline(&items, &trusted, now(), &mut state);
    assert!(state.discovered.contains_key(&item_key(&existing)));
    assert!(state.baselined_kinds.contains(&TodoKind::OjQuiz));
    // 基线只记录，不产生通知。
    assert!(plan_reminders(&items, &settings, now(), &mut state).is_empty());

    // 同一来源之后新增的待办照常提醒。
    let fresh = item("new", TodoKind::OjQuiz, now() + DAY);
    let pending = plan_reminders(&[fresh], &settings, now(), &mut state);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, ReminderEvent::Discovered);
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
    assert_eq!(pending[0].1, ReminderEvent::Discovered);
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
    // 旧版本 JSON 缺少新字段：字段级 default 给 0，与全新安装的 Default(2) 不同。
    assert_eq!(legacy.discovery_baseline_version, 0);
    assert!(legacy.discovered.is_empty());
    assert!(legacy.started.is_empty());
    assert!(legacy.baselined_kinds.is_empty());
    assert_eq!(ReminderState::default().discovery_baseline_version, 2);
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
    state.started.insert("oj-quiz|t".into(), 1_699_000_000_000);
    state.baselined_kinds.insert(TodoKind::OjQuiz);
    state.discovery_baseline_version = 0;
    database.save_state(&state).await.unwrap();

    let loaded = database.load_state().await.unwrap();
    assert_eq!(loaded.notified["oj-quiz|t|1|60"], 42);
    assert_eq!(loaded.scheduled["oj-quiz|t|1|60"], 7);
    assert_eq!(loaded.discovered["oj-quiz|t"], 1_700_000_000_000);
    assert_eq!(loaded.started["oj-quiz|t"], 1_699_000_000_000);
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

/// 评教待办：0 条不出项；有未评时出一条当天截止、固定 id 的提醒。
#[test]
fn evaluation_todo_is_a_single_due_today_item() {
    assert!(from_evaluation(PendingSummary::default(), now()).is_empty());

    let summary = PendingSummary {
        pending_targets: 2,
        pending_records: 5,
        trusted: true,
    };
    let items = from_evaluation(summary, now());
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.id, "evaluation:pending");
    assert_eq!(item.kind, TodoKind::EvaluationPending);
    assert_eq!(item.title, "评教未完成");
    assert_eq!(
        item.subtitle.as_deref(),
        Some("点击一键完成 · 5 位教师待评")
    );
    assert_eq!(item.route, "/study/evaluation");
    assert!(item.start_at_ms.is_none());
    assert!(item.urgent);
    // 当天 23:59:59.999（本地时区）。
    let due = Local.timestamp_millis_opt(item.due_at_ms).single().unwrap();
    let now_local = Local.timestamp_millis_opt(now()).single().unwrap();
    assert_eq!(due.date_naive(), now_local.date_naive());
    assert_eq!((due.hour(), due.minute()), (23, 59));
    let expected = Local
        .from_local_datetime(&now_local.date_naive().succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap()
        .timestamp_millis()
        - 1;
    assert_eq!(item.due_at_ms, expected);
}
