use chrono::{NaiveDate, NaiveDateTime, TimeZone};
use tauri::Manager;

use super::model::{TodoItem, TodoKind};
use szu_core::campus_payments::ElectricityStatusView;
use szu_lms::lms::model::Snapshot as LmsSnapshot;
use szu_oj::oj::OjDeadline;
use szu_uooc::uooc::model::{SessionStatus, Snapshot as UoocSnapshot};
use szu_uooc::uooc::Assessments;

const DAY_MS: i64 = 86_400_000;

/// 聚合全部来源。任一来源未初始化或暂不可用时跳过，不影响其它来源。
pub fn aggregate(app: &tauri::AppHandle) -> Vec<TodoItem> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut items = Vec::new();

    if let Ok(service) = szu_oj::oj::service(app) {
        items.extend(from_oj(service.deadlines(), now));
    }
    if let Ok(service) = szu_uooc::uooc::service(app) {
        items.extend(from_uooc(&service.snapshot(), &service.assessments(), now));
    }
    if let Ok(service) = szu_lms::lms::service(app) {
        items.extend(from_lms(&service.snapshot(), now));
    }
    if let Some(store) = app.try_state::<szu_core::campus_payments::CampusPaymentsStore>() {
        items.extend(from_electricity(
            &szu_core::campus_payments::electricity::status(&store.electricity),
            now,
        ));
    }

    items.sort_by_key(|item| item.due_at_ms);
    items
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

pub fn from_electricity(status: &ElectricityStatusView, now: i64) -> Vec<TodoItem> {
    if status.expired {
        return Vec::new();
    }
    let Some(days) = status.estimated_days_left else {
        return Vec::new();
    };
    if days < 0.0 {
        return Vec::new();
    }

    let base = status
        .observed_date
        .as_deref()
        .and_then(parse_local_date_midnight)
        .unwrap_or(now);
    let due_at = base + (days * DAY_MS as f64) as i64;
    let remaining_text = status
        .remaining
        .map(|remaining| format!("（剩余约 {remaining:.1} 度）"))
        .unwrap_or_default();

    vec![TodoItem {
        id: "electricity:outage-forecast".into(),
        kind: TodoKind::ElectricityOutage,
        title: format!("预计电量耗尽{remaining_text}"),
        subtitle: Some("宿舍电费监控 · 预估断电日期".into()),
        due_at_ms: due_at,
        route: "/life/campus-payments/recharge".into(),
        external_url: None,
        urgent: days < 1.0,
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
