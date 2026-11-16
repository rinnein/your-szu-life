use std::collections::HashSet;

use chrono::{NaiveDate, NaiveDateTime, TimeZone};
use tauri::Manager;

use super::model::{TodoItem, TodoKind};
use szu_core::campus_payments::ElectricityStatusView;
use szu_lms::lms::model::Snapshot as LmsSnapshot;
use szu_oj::oj::OjDeadline;
use szu_uooc::uooc::model::{SessionStatus, Snapshot as UoocSnapshot};
use szu_uooc::uooc::Assessments;

const DAY_MS: i64 = 86_400_000;

/// 一次聚合的结果。
///
/// `trusted` 表示本次聚合中「当前视图完整」的来源：只有这些来源名下缺失的待办
/// 才能被当作「确认关闭」并清理提醒记录。网络失败、5xx、登录失效都会让对应来源
/// 不进入该集合，避免把暂时不可用的待办误判为已关闭。
#[derive(Clone, Debug, Default)]
pub struct Aggregate {
    pub items: Vec<TodoItem>,
    pub trusted: HashSet<TodoKind>,
}

/// 聚合全部来源。任一来源未初始化或暂不可用时跳过，不影响其它来源。
pub fn aggregate(app: &tauri::AppHandle) -> Aggregate {
    let now = chrono::Utc::now().timestamp_millis();
    let mut items = Vec::new();
    let mut trusted = HashSet::new();

    if let Ok(service) = szu_oj::oj::service(app) {
        let view = service.deadlines_view();
        // OJ 缓存只在扫描成功时整体替换；缓存完整时，列表里缺失即代表
        // 截止、全部通过或被服务端移除。
        if view.complete {
            trusted.insert(TodoKind::OjQuiz);
        }
        items.extend(from_oj(view.rows, now));
    }
    if let Ok(service) = szu_uooc::uooc::service(app) {
        let snapshot = service.snapshot();
        let assessments = service.assessments();
        trusted.extend(uooc_trusted_kinds(&snapshot, &assessments));
        items.extend(from_uooc(&snapshot, &assessments, now));
    }
    if let Ok(service) = szu_lms::lms::service(app) {
        let snapshot = service.snapshot();
        if lms_trusted(&snapshot) {
            trusted.insert(TodoKind::LmsHomework);
        }
        items.extend(from_lms(&snapshot, now));
    }
    if let Ok(service) = szu_signin::signin::service(app) {
        // 探测成功后 fresh 视图才可信；窗口外无待签项也视为「确认没有」。
        let view = service.checkin_view();
        if view.complete {
            trusted.insert(TodoKind::Checkin);
        }
        items.extend(from_checkin(view.items, now));
    }
    if let Some(store) = app.try_state::<szu_core::campus_payments::CampusPaymentsStore>() {
        let status = szu_core::campus_payments::electricity::status(&store.electricity);
        if electricity_trusted(&status) {
            trusted.insert(TodoKind::ElectricityOutage);
        }
        items.extend(from_electricity(&status, now));
    }

    items.sort_by_key(|item| item.due_at_ms);
    Aggregate { items, trusted }
}

/// UOOC 的课程列表与 `Ready` 在同一次成功请求里更新；考试/作业列表还要等待
/// `refresh_assessments` 全量成功。否则重启后内存中的空列表会被误当成
/// 「全部考试/作业都已完成」。
pub fn uooc_trusted_kinds(
    snapshot: &UoocSnapshot,
    assessments: &Assessments,
) -> Vec<TodoKind> {
    if snapshot.session != SessionStatus::Ready {
        return Vec::new();
    }
    let mut kinds = vec![TodoKind::UoocCourseEnd];
    if assessments.loaded {
        kinds.push(TodoKind::UoocExam);
        kinds.push(TodoKind::UoocHomework);
    }
    kinds
}

/// 畅课刷新失败只会把 session 设为 `Unavailable` 并保留旧缓存，
/// 此时不能把缺失项当作已关闭。
pub fn lms_trusted(snapshot: &LmsSnapshot) -> bool {
    snapshot.session == szu_lms::lms::model::SessionState::Ready && snapshot.fetched_at.is_some()
}

/// 只有有效、且与当前宿舍设置匹配的缓存才能确认「没有断电预测」。
pub fn electricity_trusted(status: &ElectricityStatusView) -> bool {
    !status.expired && status.remaining.is_some()
}

pub fn from_oj(deadlines: Vec<OjDeadline>, now: i64) -> Vec<TodoItem> {
    deadlines
        .into_iter()
        .filter(|contest| contest.end > now)
        .filter(|contest| {
            !(contest.has_progress && contest.total > 0 && contest.accepted >= contest.total)
        })
        .map(|contest| TodoItem {
            id: format!("oj:{}:{}", contest.site_id, contest.id),
            kind: TodoKind::OjQuiz,
            title: contest.title,
            subtitle: Some(format!(
                "{} · {}",
                contest.site_name,
                if contest.is_exam { "考试" } else { "测验" }
            )),
            start_at_ms: (contest.begin > 0 && contest.begin < contest.end)
                .then_some(contest.begin),
            due_at_ms: contest.end,
            route: format!(
                "/study/oj?site={}&contest={}",
                encode_query(&contest.site_id),
                encode_query(&contest.id)
            ),
            external_url: None,
            urgent: contest.end - now < DAY_MS,
        })
        .collect()
}

pub fn from_uooc(
    snapshot: &UoocSnapshot,
    assessments: &Assessments,
    now: i64,
) -> Vec<TodoItem> {
    if snapshot.session != SessionStatus::Ready {
        return Vec::new();
    }

    let mut items = Vec::new();

    for course in &snapshot.courses {
        let Some(due_at) = parse_uooc_datetime(&course.end_date) else {
            continue;
        };
        if due_at <= now {
            continue;
        }
        items.push(TodoItem {
            id: format!("uooc:course:{}", course.id),
            kind: TodoKind::UoocCourseEnd,
            title: format!("{} 课程结课", course.title),
            subtitle: Some("UOOC · 课程结课".into()),
            start_at_ms: None,
            due_at_ms: due_at,
            route: "/study/uooc?tab=courses".into(),
            external_url: None,
            urgent: due_at - now < 3 * DAY_MS,
        });
    }

    for exam in &assessments.exams {
        if exam_completed(exam) {
            continue;
        }
        let Some(due_at) = parse_uooc_datetime(&exam.end_time) else {
            continue;
        };
        if due_at <= now {
            continue;
        }
        items.push(TodoItem {
            id: format!("uooc:exam:{}", exam.id),
            kind: TodoKind::UoocExam,
            title: exam.title.clone(),
            subtitle: Some(format!("{} · 考试", exam.course_name)),
            start_at_ms: parse_uooc_datetime(&exam.start_time).filter(|start| *start < due_at),
            due_at_ms: due_at,
            route: "/study/uooc?tab=exams&subtab=exam".into(),
            external_url: None,
            urgent: due_at - now < DAY_MS,
        });
    }

    for homework in &assessments.homeworks {
        if homework_completed(homework) {
            continue;
        }
        let Some(due_at) = parse_uooc_datetime(&homework.end_time) else {
            continue;
        };
        if due_at <= now {
            continue;
        }
        items.push(TodoItem {
            id: format!("uooc:homework:{}", homework.id),
            kind: TodoKind::UoocHomework,
            title: homework.title.clone(),
            subtitle: Some(format!("{} · 作业", homework.course_name)),
            start_at_ms: parse_uooc_datetime(&homework.start_time).filter(|start| *start < due_at),
            due_at_ms: due_at,
            route: "/study/uooc?tab=exams&subtab=homework".into(),
            external_url: None,
            urgent: due_at - now < DAY_MS,
        });
    }

    items
}

fn exam_completed(exam: &szu_uooc::uooc::model::ExamTask) -> bool {
    exam.can_view
        || exam.status_code >= 20
        || exam.status_text == "已批改"
        || exam.score.is_some()
        || (exam.chance > 0 && exam.submit_count >= exam.chance)
}

fn homework_completed(homework: &szu_uooc::uooc::model::HomeworkTask) -> bool {
    homework.can_view
        || homework.status_code >= 20
        || homework.status_text == "已批改"
        || homework.score.is_some()
        || (homework.chance > 0 && homework.submit_count >= homework.chance)
}

pub fn from_lms(snapshot: &LmsSnapshot, now: i64) -> Vec<TodoItem> {
    if snapshot.session != szu_lms::lms::model::SessionState::Ready {
        return Vec::new();
    }

    snapshot
        .todos
        .iter()
        .filter_map(|todo| {
            let due_at = parse_rfc3339_millis(&todo.end_time)?;
            if due_at <= now {
                return None;
            }
            Some(TodoItem {
                id: format!("lms:homework:{}", todo.id),
                kind: TodoKind::LmsHomework,
                title: todo.title.clone(),
                subtitle: Some(format!("畅课 · {}", todo.course_name)),
                // 畅课待办接口只给截止时间，没有任务开始时间。
                start_at_ms: None,
                due_at_ms: due_at,
                route: "/study/lms".into(),
                external_url: Some(format!(
                    "https://lms.szu.edu.cn/course/{}/learning-activity#/{}?view=scores",
                    todo.course_id, todo.id
                )),
                urgent: due_at - now < DAY_MS,
            })
        })
        .collect()
}

/// 签到待签项（探测循环已按窗口收敛，这里只做剩余时间过滤与转换）。
pub fn from_checkin(items: Vec<szu_signin::signin::CheckinTodoItem>, now: i64) -> Vec<TodoItem> {
    items
        .into_iter()
        .filter(|item| item.due_at_ms > now)
        .map(|item| TodoItem {
            id: item.id,
            kind: TodoKind::Checkin,
            title: item.title,
            subtitle: item.subtitle.or_else(|| Some("签到".into())),
            start_at_ms: None,
            due_at_ms: item.due_at_ms,
            route: item.route,
            external_url: None,
            // 签到窗口只有几分钟，全部按紧急样式展示。
            urgent: true,
        })
        .collect()
}

/// 电费预警统一并入待办。
///
/// 低电量（`low`）即使算不出估算耗尽日也要生成一条「当天 23:59 前充值」的待办，
/// 避免原来每日后台推送被移除后漏报；算得出且尚未到达的估算日优先。
pub fn from_electricity(status: &ElectricityStatusView, now: i64) -> Vec<TodoItem> {
    if status.expired {
        return Vec::new();
    }
    let estimate = status
        .estimated_days_left
        .filter(|days| *days >= 0.0)
        .map(|days| {
            let base = status
                .observed_date
                .as_deref()
                .and_then(parse_local_date_midnight)
                .unwrap_or(now);
            base + (days * DAY_MS as f64) as i64
        });
    let due_at = if status.low {
        estimate
            .filter(|due| *due > now)
            .unwrap_or_else(|| end_of_local_day(now))
    } else {
        match estimate {
            Some(due) => due,
            None => return Vec::new(),
        }
    };

    let remaining_text = status
        .remaining
        .map(|remaining| format!("（剩余约 {remaining:.1} 度）"))
        .unwrap_or_default();
    let (title, subtitle, urgent) = if status.low {
        // `warning_reason` 区分「低于 5 度」与「预计可用不足 1 天」，直接作为标题更准确。
        (
            status
                .warning_reason
                .clone()
                .unwrap_or_else(|| "宿舍电量不足，请及时充值。".into()),
            format!("宿舍电费监控 · 电费预警{remaining_text}"),
            true,
        )
    } else {
        (
            format!("预计电量耗尽{remaining_text}"),
            "宿舍电费监控 · 预估断电日期".to_string(),
            due_at - now < DAY_MS,
        )
    };

    vec![TodoItem {
        id: "electricity:outage-forecast".into(),
        kind: TodoKind::ElectricityOutage,
        title,
        subtitle: Some(subtitle),
        start_at_ms: None,
        due_at_ms: due_at,
        route: "/life/campus-payments/recharge".into(),
        external_url: None,
        urgent,
    }]
}

/// UOOC 的时间是本地时间字符串：`2026-10-30 18:00:00`、`2026/10/14` 等。
/// 仅日期的值按当天 23:59:59 处理，与前端 `new Date(...)` 的语义保持一致。
pub fn parse_uooc_datetime(raw: &str) -> Option<i64> {
    let normalized = raw.replace('/', "-");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.timestamp_millis());
    }

    let with_time = if trimmed.contains(' ') {
        trimmed.to_string()
    } else {
        format!("{trimmed} 23:59:59")
    };
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&with_time, format) {
            return chrono::Local
                .from_local_datetime(&naive)
                .single()
                .map(|value| value.timestamp_millis());
        }
    }
    None
}

fn parse_rfc3339_millis(raw: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|value| value.timestamp_millis())
}

fn parse_local_date_midnight(raw: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()?;
    chrono::Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .single()
        .map(|value| value.timestamp_millis())
}

/// 当前本地日期结束的时刻（23:59:59.999）。
fn end_of_local_day(now: i64) -> i64 {
    let today = chrono::Local
        .timestamp_millis_opt(now)
        .single()
        .map(|value| value.date_naive());
    today
        .and_then(|date| date.succ_opt())
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .and_then(|naive| chrono::Local.from_local_datetime(&naive).single())
        .map(|value| value.timestamp_millis() - 1)
        .unwrap_or(now + DAY_MS)
}

/// 与前端 `encodeURIComponent` 等价的最小实现，用于 OJ 的路由参数。
fn encode_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
