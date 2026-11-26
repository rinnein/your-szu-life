//! 学生评教：SSO 自动登录、待评清单、一键提交与待办摘要。
//!
//! 一次 `overview` 会拉取当前学期的评价目标清单，并逐个补齐目标明细
//! （典型 1 + N 个请求）。结果缓存在内存里 30 分钟，供页面与待办共享；
//! 统一认证失效时保留旧缓存但标记为不可信，避免把暂时不可用的待办误删。

mod jxpj;
pub mod model;
#[cfg(test)]
mod tests;

pub mod commands;

use self::jxpj::{Detail, JxpjClient, TargetRow};
pub use self::model::*;
use super::protocol::{Error, Failure, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Listener, Manager};

/// 缓存有效期：页面、待办与 MCP 共用同一次拉取结果。
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
/// 开放题回答的默认值。
const DEFAULT_COMMENT: &str = "无";
/// 页面最低停留时长最多等待的秒数，避免命令被异常配置长时间占住。
const MAX_STAY_SECONDS: i64 = 60;

struct CachedOverview {
    overview: EvaluationOverview,
    at: Instant,
}

/// 评教缓存与串行化门闩（全部字段仅内存，不落库）。
#[derive(Default)]
pub struct EvaluationStore {
    cache: Mutex<Option<CachedOverview>>,
    /// 最近一次拉取是否失败；失败时待办保留但不可清理提醒记录。
    failed: AtomicBool,
    /// 同一时刻只允许一次评教网络操作（页面刷新、待办刷新、提交）。
    operation: Mutex<()>,
}

impl EvaluationStore {
    fn invalidate(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            *cache = None;
        }
        self.failed.store(false, Ordering::Release);
    }

    fn store(&self, overview: &EvaluationOverview) {
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some(CachedOverview {
                overview: overview.clone(),
                at: Instant::now(),
            });
        }
        self.failed.store(false, Ordering::Release);
    }
}

/// 由 `enrollment::initialize` 调用（只注册一次）。
pub fn initialize(app: &AppHandle) {
    if app.try_state::<EvaluationStore>().is_some() {
        return;
    }
    app.manage(EvaluationStore::default());
    // 账号切换/退出登录时丢弃上一个账号的缓存与待办摘要，
    // 并在认证状态变化后立即尝试拉一次，让评教待办尽快出现。
    let handle = app.clone();
    app.listen("auth-state-changed", move |_| {
        if let Some(store) = handle.try_state::<EvaluationStore>() {
            store.invalidate();
        }
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            refresh(&handle, false).await;
        });
    });
}

fn store_of(app: &AppHandle) -> Result<tauri::State<'_, EvaluationStore>> {
    app.try_state::<EvaluationStore>()
        .ok_or_else(|| Error::new(Failure::Protocol, "评教服务尚未初始化"))
}

fn is_fresh(app: &AppHandle) -> bool {
    let Some(store) = app.try_state::<EvaluationStore>() else {
        return false;
    };
    let Ok(cache) = store.cache.lock() else {
        return false;
    };
    cache
        .as_ref()
        .is_some_and(|cached| cached.at.elapsed() < CACHE_TTL)
}

fn cached_overview(app: &AppHandle) -> Option<EvaluationOverview> {
    let store = app.try_state::<EvaluationStore>()?;
    let cache = store.cache.lock().ok()?;
    cache.as_ref().map(|cached| cached.overview.clone())
}

/// 拉取（或复用缓存）待评清单。
pub fn overview(app: &AppHandle, force: bool) -> Result<EvaluationOverview> {
    if !force {
        if let Some(overview) = cached_overview(app) {
            if is_fresh(app) {
                return Ok(overview);
            }
        }
    }
    let store = store_of(app)?;
    let _guard = store
        .operation
        .lock()
        .map_err(|_| Error::new(Failure::Protocol, "评教服务状态异常"))?;
    // 等锁期间可能已有线程刷新完成。
    if !force {
        if let Some(overview) = cached_overview(app) {
            if is_fresh(app) {
                return Ok(overview);
            }
        }
    }
    match fetch(app) {
        Ok(overview) => {
            store.store(&overview);
            Ok(overview)
        }
        Err(error) => {
            store.failed.store(true, Ordering::Release);
            Err(error)
        }
    }
}

fn fetch(app: &AppHandle) -> Result<EvaluationOverview> {
    let sso = szu_core::auth::reusable_sso_jar(app)
        .map_err(|message| Error::new(Failure::Session, message))?;
    let client = JxpjClient::new(jxpj::JXPJ_BASE, jxpj::AUTH_BASE, sso)?;
    fetch_with(&client)
}

fn fetch_with(client: &JxpjClient) -> Result<EvaluationOverview> {
    let account = client.bootstrap()?;
    let rows = client.list_targets(account.term_id)?;
    let mut targets = Vec::with_capacity(rows.len());
    let mut pending_targets = 0usize;
    let mut pending_records = 0usize;
    let mut expired_records = 0usize;
    for row in rows {
        let target = if row.pending > 0 {
            match client.target_detail(&row.id)? {
                Some(detail) => build_target(&row, detail),
                None => finished_target(&row),
            }
        } else {
            finished_target(&row)
        };
        if matches!(
            target.status,
            TargetStatus::Pending | TargetStatus::Unsupported
        ) {
            pending_targets += 1;
            pending_records += target.actionable_records().count();
        }
        expired_records += expired_count(&target);
        targets.push(target);
    }
    Ok(EvaluationOverview {
        fetched_at: chrono::Utc::now().timestamp_millis(),
        term: (!account.term_name.is_empty()).then(|| EvaluationTerm {
            id: account.term_id,
            name: account.term_name,
        }),
        comment: load_comment(),
        pending_targets,
        pending_records,
        expired_records,
        targets,
    })
}

fn finished_target(row: &TargetRow) -> EvaluationTarget {
    EvaluationTarget {
        id: row.id.clone(),
        target_id: 0,
        target_name: row.target_name.clone(),
        term_name: row.term_name.clone(),
        expired_date: None,
        status: TargetStatus::Done,
        message: None,
        score_type: 0,
        comment_required: 0,
        stay_seconds: 0,
        records: Vec::new(),
        questions: Vec::new(),
    }
}

fn build_target(row: &TargetRow, detail: Detail) -> EvaluationTarget {
    let records: Vec<EvaluationRecord> = detail
        .records
        .into_iter()
        .map(|record| EvaluationRecord {
            sc_id: record.sc_id,
            course_name: record.course_name,
            course_no: record.course_no,
            employee_name: record.employee_name,
            class_name: record.class_name,
            current_times: record.current_times,
            limited_times: record.limited_times,
            expired_date: record.expired_date,
            participate: record.participate,
        })
        .collect();
    let questions: Vec<EvaluationQuestion> = detail
        .questions
        .into_iter()
        .map(|question| EvaluationQuestion {
            sub_id: question.sub_id,
            sub_name: question.sub_name,
            kind: question.kind,
            choices: question
                .choices
                .into_iter()
                .map(|choice| ChoiceOption {
                    label: choice.label,
                    value_low: choice.value_low,
                    value_high: choice.value_high,
                    value_normal: choice.value_normal,
                })
                .collect(),
        })
        .collect();
    let target = EvaluationTarget {
        id: row.id.clone(),
        target_id: detail.target_id,
        target_name: if detail.target_name.is_empty() {
            row.target_name.clone()
        } else {
            detail.target_name
        },
        term_name: row.term_name.clone(),
        expired_date: records.iter().find_map(|record| record.expired_date.clone()),
        status: TargetStatus::Done,
        message: None,
        score_type: detail.score_type,
        comment_required: detail.comment_required,
        stay_seconds: detail.stay_seconds,
        records,
        questions,
    };
    let actionable = target.actionable_records().count();
    let expired = expired_count(&target);
    let unsupported = actionable > 0 && target.questions.iter().any(|question| !supported(question));
    let (status, message) = if actionable == 0 {
        if expired > 0 {
            (TargetStatus::Expired, Some("评教已过截止日期".to_string()))
        } else {
            (TargetStatus::Done, None)
        }
    } else if unsupported {
        (
            TargetStatus::Unsupported,
            Some("包含暂不支持自动作答的题型，已跳过".to_string()),
        )
    } else {
        (TargetStatus::Pending, None)
    };
    EvaluationTarget {
        status,
        message,
        ..target
    }
}

fn supported(question: &EvaluationQuestion) -> bool {
    question.kind == "options"
        && !question.choices.is_empty()
        && choose_choice(&question.choices).is_some()
}

/// 3/4 档规则：按选项区间中点降序取第 2 个（4 档即「良好/比较清楚/比较符合/比较满意」）；
/// 只有 1–2 个选项时取最高档（2 档量表不能把「否」当默认）。
fn choose_choice(choices: &[ChoiceOption]) -> Option<usize> {
    if choices.is_empty() {
        return None;
    }
    let mut order: Vec<usize> = (0..choices.len()).collect();
    order.sort_by(|left, right| {
        midpoint(&choices[*right])
            .total_cmp(&midpoint(&choices[*left]))
            .then_with(|| normal(&choices[*right]).total_cmp(&normal(&choices[*left])))
            .then_with(|| left.cmp(right))
    });
    Some(if order.len() >= 3 { order[1] } else { order[0] })
}

fn parse_number(raw: &str) -> Option<f64> {
    raw.trim().parse::<f64>().ok()
}

fn normal(choice: &ChoiceOption) -> f64 {
    parse_number(&choice.value_normal).unwrap_or(f64::NEG_INFINITY)
}

fn midpoint(choice: &ChoiceOption) -> f64 {
    match (
        parse_number(&choice.value_low),
        parse_number(&choice.value_high),
    ) {
        (Some(low), Some(high)) => (low + high) / 2.0,
        _ => normal(choice),
    }
}

/// 滑条模式的实际分值：优先取 `valueNormal`，缺失时退回区间中点。
fn choice_score(choice: &ChoiceOption) -> f64 {
    parse_number(&choice.value_normal).unwrap_or_else(|| midpoint(choice))
}

fn build_plan(target: &EvaluationTarget, comment: &str) -> Result<TargetPlan> {
    let records: Vec<&EvaluationRecord> = target.actionable_records().collect();
    if records.is_empty() {
        return Err(Error::new(Failure::Business, "没有可提交的评价记录"));
    }
    if target.questions.is_empty() {
        return Err(Error::new(
            Failure::Business,
            "评价指标没有题目，已跳过",
        ));
    }
    let mut choices = Vec::with_capacity(records.len() * target.questions.len());
    for record in &records {
        for question in &target.questions {
            let index = choose_choice(&question.choices).ok_or_else(|| {
                Error::new(
                    Failure::Business,
                    format!("题目「{}」没有可选项", question.sub_name),
                )
            })?;
            let choice = &question.choices[index];
            choices.push(PlannedChoice {
                sc_id: record.sc_id,
                sub_id: question.sub_id,
                choice_label: format!(
                    "{}_{}_{}_{}",
                    record.sc_id, question.sub_id, choice.label, index
                ),
                score: (target.score_type == 4).then(|| choice_score(choice)),
            });
        }
    }
    let comments = records
        .iter()
        .map(|record| PlannedComment {
            sc_id: record.sc_id,
            comment: comment.to_string(),
        })
        .collect();
    Ok(TargetPlan {
        target_id: target.target_id,
        target_name: target.target_name.clone(),
        choices,
        comments,
    })
}

/// 待办聚合读取的摘要。
pub fn summary(app: &AppHandle) -> PendingSummary {
    let Some(store) = app.try_state::<EvaluationStore>() else {
        return PendingSummary::default();
    };
    let Some(overview) = cached_overview(app) else {
        return PendingSummary::default();
    };
    PendingSummary {
        pending_targets: overview.pending_targets,
        pending_records: overview.pending_records,
        trusted: !store.failed.load(Ordering::Acquire),
    }
}

/// 待办引擎按自己的节拍调用；30 分钟内不重复访问 jxpj。
pub async fn refresh(app: &AppHandle, force: bool) {
    if !force && is_fresh(app) {
        return;
    }
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let _ = overview(&handle, force);
    })
    .await;
}

/// 执行一次评教：`dry_run` 只构造提交内容，不发写请求。
pub fn run(
    app: &AppHandle,
    comment: Option<&str>,
    dry_run: bool,
    emit: &mut dyn FnMut(EvaluationRunEvent),
) -> Result<RunOutcome> {
    let store = store_of(app)?;
    let _guard = store
        .operation
        .lock()
        .map_err(|_| Error::new(Failure::Protocol, "评教服务状态异常"))?;
    let sso = szu_core::auth::reusable_sso_jar(app)
        .map_err(|message| Error::new(Failure::Session, message))?;
    let client = JxpjClient::new(jxpj::JXPJ_BASE, jxpj::AUTH_BASE, sso)?;
    let overview = match fetch_with(&client) {
        Ok(overview) => overview,
        Err(error) => {
            store.failed.store(true, Ordering::Release);
            return Err(error);
        }
    };
    store.store(&overview);
    let outcome = run_with(&client, &overview, comment, dry_run, emit)?;
    // 真实提交后立刻回读一次，待办与 MCP 资源不必等下一个 30 分钟周期。
    if !dry_run && outcome.summary.submitted_targets > 0 {
        if let Ok(fresh) = fetch_with(&client) {
            store.store(&fresh);
        }
    }
    Ok(outcome)
}

fn run_with(
    client: &JxpjClient,
    overview: &EvaluationOverview,
    comment: Option<&str>,
    dry_run: bool,
    emit: &mut dyn FnMut(EvaluationRunEvent),
) -> Result<RunOutcome> {
    let comment = resolve_comment(comment);
    let targets: Vec<&EvaluationTarget> = overview
        .targets
        .iter()
        .filter(|target| {
            matches!(
                target.status,
                TargetStatus::Pending | TargetStatus::Unsupported
            )
        })
        .collect();
    let total = targets.len();
    let mut summary = EvaluationRunSummary::default();
    let mut plans = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        emit(EvaluationRunEvent::Target(TargetProgress {
            index: index + 1,
            total,
            target_name: target.target_name.clone(),
            state: TargetState::Started,
            message: None,
        }));
        if target.stay_seconds > 0 {
            wait(target.stay_seconds.min(MAX_STAY_SECONDS), emit);
        }
        let plan = match build_plan(target, &comment) {
            Ok(plan) => plan,
            Err(error) => {
                summary.failures.push(RunFailure {
                    target_name: target.target_name.clone(),
                    course_name: None,
                    message: error.message.clone(),
                });
                emit(EvaluationRunEvent::Target(TargetProgress {
                    index: index + 1,
                    total,
                    target_name: target.target_name.clone(),
                    state: TargetState::Failed,
                    message: Some(error.message),
                }));
                continue;
            }
        };
        if dry_run {
            plans.push(plan);
            emit(EvaluationRunEvent::Target(TargetProgress {
                index: index + 1,
                total,
                target_name: target.target_name.clone(),
                state: TargetState::Ok,
                message: Some("仅预览，未提交".to_string()),
            }));
            continue;
        }
        match client.save_score(plan.target_id, &plan.choices, &plan.comments) {
            Ok(()) => {
                summary.submitted_targets += 1;
                summary.submitted_records += plan.comments.len();
                emit(EvaluationRunEvent::Target(TargetProgress {
                    index: index + 1,
                    total,
                    target_name: target.target_name.clone(),
                    state: TargetState::Ok,
                    message: None,
                }));
            }
            Err(error) => {
                summary.failures.push(RunFailure {
                    target_name: target.target_name.clone(),
                    course_name: None,
                    message: error.message.clone(),
                });
                emit(EvaluationRunEvent::Target(TargetProgress {
                    index: index + 1,
                    total,
                    target_name: target.target_name.clone(),
                    state: TargetState::Failed,
                    message: Some(error.message),
                }));
            }
        }
    }
    emit(EvaluationRunEvent::Done {
        summary: summary.clone(),
        dry_run,
    });
    Ok(RunOutcome {
        dry_run,
        summary,
        plans: if dry_run { plans } else { Vec::new() },
    })
}

fn wait(seconds: i64, emit: &mut dyn FnMut(EvaluationRunEvent)) {
    let mut remaining = seconds;
    while remaining > 0 {
        emit(EvaluationRunEvent::Waiting { seconds: remaining });
        std::thread::sleep(Duration::from_secs(1));
        remaining -= 1;
    }
}

fn resolve_comment(override_value: Option<&str>) -> String {
    // 页面总是显式传入输入框内容（可能为空）：空值按用户约定回退「无」。
    // 只有 MCP 这类「未提供参数」的调用才读取已保存的配置。
    match override_value {
        Some(text) => {
            let text = text.trim();
            if text.is_empty() {
                DEFAULT_COMMENT.to_string()
            } else {
                text.to_string()
            }
        }
        None => load_comment(),
    }
}

/// 读取开放题回答；未配置或为空时回退「无」。
pub fn load_comment() -> String {
    szu_core::preferences::load::<String>(szu_core::preferences::EVALUATION_COMMENT)
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_COMMENT.to_string())
}

pub fn save_comment(
    app: &AppHandle,
    comment: &str,
) -> std::result::Result<EvaluationConfig, String> {
    let value = comment.trim().to_string();
    szu_core::preferences::save(szu_core::preferences::EVALUATION_COMMENT, &value)
        .map_err(|error| format!("保存评教配置失败: {error}"))?;
    // 缓存里的 comment 也同步，避免 MCP 资源在 30 分钟内读到旧值。
    if let Some(store) = app.try_state::<EvaluationStore>() {
        if let Ok(mut cache) = store.cache.lock() {
            if let Some(cached) = cache.as_mut() {
                cached.overview.comment = value.clone();
            }
        }
    }
    Ok(EvaluationConfig { comment: value })
}
