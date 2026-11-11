use super::{client::Error, model::SessionStatus, Service};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metric {
    pub percent: Option<f64>,
    pub count: Option<u64>,
    pub total: Option<u64>,
    pub weight: Option<f64>,
    pub score: Option<f64>,
    pub average_score: Option<f64>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Attendance {
    NotRequired,
    Complete,
    Today,
    Needed,
    #[default]
    Unknown,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CourseProgress {
    pub total_percent: Option<f64>,
    pub video: Metric,
    pub test: Metric,
    pub homework: Metric,
    pub signin: Metric,
    #[serde(default)]
    pub discussion: Metric,
    #[serde(default)]
    pub exam: Metric,
    pub weighted_percent: Option<f64>,
    #[serde(default)]
    pub weighted_partial: bool,
    pub weight_total: Option<f64>,
    pub score: Option<f64>,
    pub weighted_score: Option<f64>,
    pub video_minutes: Option<f64>,
    pub discussion_posts: Option<u64>,
    pub discussion_replies: Option<u64>,
    pub signin_weight: Option<f64>,
    pub signin_time: Option<String>,
    pub attendance: Attendance,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressEntry {
    pub data: Option<CourseProgress>,
    pub updated_at_ms: Option<i64>,
    pub refreshing: bool,
    pub error: Option<String>,
}
fn nonnegative(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
}
fn count(value: &Value) -> Option<u64> {
    nonnegative(value)
        .filter(|v| v.fract() == 0.0 && *v <= 9_007_199_254_740_991.0)
        .map(|v| v as u64)
}
fn percent(value: &Value, name: &str) -> Option<f64> {
    nonnegative(&value[format!("{name}_progress_value")])
        .filter(|v| *v <= 100.0)
        .or_else(|| {
            value[format!("{name}_progress")]
                .as_str()?
                .trim()
                .trim_end_matches('%')
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
        })
}
fn metric(value: &Value, name: &str, weight_key: &str) -> Metric {
    let completed = count(&value[format!("{name}_cnt")]);
    let total = count(&value[format!("{name}_total")]);
    let completion = percent(value, name).or_else(|| {
        completed
            .zip(total)
            .filter(|(_, total)| *total > 0)
            .map(|(count, total)| (count as f64 / total as f64 * 100.0).min(100.0))
    });
    Metric {
        percent: completion,
        count: completed,
        total,
        weight: nonnegative(&value[weight_key]).filter(|weight| *weight <= 100.0),
        score: nonnegative(&value[format!("{name}_score")]),
        average_score: nonnegative(&value[format!("{name}_avg_score")]),
    }
}

/// UOOC weights are percentages, not a fixed rubric shared by every course.
/// Unknown completion rates remain unknown; scores never substitute for completion.
fn weighted_progress(metrics: [&Metric; 6]) -> (Option<f64>, Option<f64>, bool) {
    let Some(weights) = metrics.iter().map(|m| m.weight).collect::<Option<Vec<_>>>() else {
        return (None, None, true);
    };
    let total: f64 = weights.iter().sum();
    if (total - 100.0).abs() > 0.000001 {
        return (None, Some(total), false);
    }
    let mut confirmed = 0.0;
    let mut known = false;
    let mut partial = false;
    for (metric, weight) in metrics.iter().zip(weights) {
        if weight == 0.0 {
            continue;
        }
        match metric.percent {
            Some(percent) => {
                confirmed += weight * percent / 100.0;
                known = true;
            }
            None => partial = true,
        }
    }
    (known.then_some(confirmed.min(100.0)), Some(total), partial)
}
pub(super) fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
        .date_naive()
}
fn signin_date(value: Option<&Value>) -> Result<Option<NaiveDate>, ()> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => parse_signin_date(s).map(Some),
        _ => Err(()),
    }
}
fn parse_signin_date(value: &str) -> Result<NaiveDate, ()> {
    NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S")
        .map(|d| d.date())
        .or_else(|_| {
            DateTime::parse_from_rfc3339(value.trim()).map(|d| {
                d.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
                    .date_naive()
            })
        })
        .map_err(|_| ())
}
impl CourseProgress {
    pub fn parse(value: &Value, date: NaiveDate) -> Self {
        let signin = metric(value, "signin", "signin");
        let video = metric(value, "video", "video");
        let homework = metric(value, "homework", "homework");
        let test = metric(value, "test", "test");
        let discussion = metric(value, "discuss", "discuz");
        let exam = metric(value, "exam", "finalExam");
        let (weighted_percent, weight_total, weighted_partial) =
            weighted_progress([&signin, &video, &homework, &test, &discussion, &exam]);
        let weight = signin.weight;
        let last_date = signin_date(value.get("signin_time"));
        let attendance = match (weight, signin.count, signin.total, last_date) {
            (Some(0.0), _, _, _) | (_, _, Some(0), _) => Attendance::NotRequired,
            (Some(_), Some(count), Some(total), _) if count >= total => Attendance::Complete,
            (Some(_), Some(_), Some(_), Ok(Some(last))) if last == date => Attendance::Today,
            (Some(_), Some(_), Some(_), Ok(None)) => Attendance::Needed,
            (Some(_), Some(_), Some(_), Ok(Some(last))) if last < date => Attendance::Needed,
            _ => Attendance::Unknown,
        };
        Self {
            total_percent: percent(value, "total"),
            video,
            test,
            homework,
            signin,
            discussion,
            exam,
            weighted_percent,
            weighted_partial,
            weight_total,
            score: nonnegative(&value["score"]),
            weighted_score: nonnegative(&value["weighted_score"]),
            video_minutes: nonnegative(&value["video_mins"]),
            discussion_posts: count(&value["discuss_publish_cnt"]),
            discussion_replies: count(&value["discuss_reply_cnt"]),
            signin_weight: weight,
            signin_time: value["signin_time"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            attendance,
        }
    }
    pub fn video_complete(&self) -> bool {
        self.video.percent == Some(100.0)
    }
    /// UOOC reports `discuss_score` as the earned weighted contribution and
    /// `discuz` as the maximum weighted contribution.
    pub fn discussion_completion_ratio(&self) -> Option<f64> {
        let score = self.discussion.score?;
        let weight = self.discussion.weight.filter(|weight| *weight > 0.0)?;
        let ratio = score / weight;
        ratio.is_finite().then_some(ratio.max(0.0))
    }
    pub fn confirms_checkin(&self, before: &Self) -> bool {
        if self
            .signin
            .percent
            .zip(before.signin.percent)
            .is_some_and(|(after, before)| after < before)
        {
            return false;
        }
        self.signin
            .percent
            .zip(before.signin.percent)
            .is_some_and(|(after, before)| after > before)
            || self
                .signin
                .count
                .zip(before.signin.count)
                .is_some_and(|(after, before)| after > before)
            || (self.signin_time != before.signin_time
                && self
                    .signin_time
                    .as_deref()
                    .and_then(|s| parse_signin_date(s).ok())
                    == Some(today()))
    }
}

#[derive(Default)]
struct Flight {
    last: Option<(Instant, Result<CourseProgress, Error>)>,
}
pub(super) struct ProgressRequests {
    flights: Mutex<HashMap<(u64, String, String), Arc<tokio::sync::Mutex<Flight>>>>,
    permits: Arc<Semaphore>,
}
// The caller may time out or close a page while HTTP is pending. Async future
// cancellation must not leave a permanent loading indicator in the snapshot.
struct RefreshGuard {
    service: Arc<Service>,
    epoch: u64,
    account: String,
    cid: String,
    armed: bool,
}
impl Drop for RefreshGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(mut runtime) = self.service.runtime.lock() else {
            return;
        };
        if runtime.epoch != self.epoch
            || runtime.snapshot.user.as_ref().map(|u| &u.id) != Some(&self.account)
        {
            return;
        }
        if let Some(entry) = runtime.snapshot.progress.get_mut(&self.cid) {
            entry.refreshing = false;
            entry.error = Some("进度查询已中止，可重新刷新".into());
        }
        self.service.publish(&mut runtime);
    }
}
impl Default for ProgressRequests {
    fn default() -> Self {
        Self {
            flights: Mutex::new(HashMap::new()),
            permits: Arc::new(Semaphore::new(3)),
        }
    }
}
impl Service {
    pub(super) async fn fetch_progress(
        self: &Arc<Self>,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<CourseProgress, Error> {
        self.fetch_progress_with(cid, cancel, false).await
    }
    /// 后台轮询不切换 `refreshing`，避免每 30 秒把整份快照推给前端。
    pub(super) async fn fetch_progress_background(
        self: &Arc<Self>,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<CourseProgress, Error> {
        self.fetch_progress_with(cid, cancel, true).await
    }
    async fn fetch_progress_with(
        self: &Arc<Self>,
        cid: &str,
        cancel: &CancellationToken,
        background: bool,
    ) -> Result<CourseProgress, Error> {
        let requested = Instant::now();
        let (epoch, account, session_cancel) = {
            let runtime = self.runtime.lock().unwrap();
            if runtime.snapshot.session != SessionStatus::Ready {
                return Err(Error::Unauthorized);
            }
            (
                runtime.epoch,
                runtime
                    .snapshot
                    .user
                    .as_ref()
                    .ok_or(Error::Unauthorized)?
                    .id
                    .clone(),
                runtime.session_cancel.clone(),
            )
        };
        let key = (epoch, account.clone(), cid.to_owned());
        let flight = self
            .progress_requests
            .flights
            .lock()
            .unwrap()
            .entry(key)
            .or_default()
            .clone();
        let mut flight = tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), _ = session_cancel.cancelled() => return Err(Error::Cancelled), lock = flight.lock() => lock };
        if let Some((at, result)) = &flight.last {
            if *at >= requested {
                return result.clone();
            }
        }
        let _permit = tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), _ = session_cancel.cancelled() => return Err(Error::Cancelled), permit = self.progress_requests.permits.clone().acquire_owned() => permit.map_err(|_| Error::Cancelled)? };
        {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.epoch != epoch
                || runtime.snapshot.user.as_ref().map(|u| &u.id) != Some(&account)
            {
                return Err(Error::Cancelled);
            }
            if !background {
                runtime
                    .snapshot
                    .progress
                    .entry(cid.into())
                    .or_default()
                    .refreshing = true;
                self.publish(&mut runtime);
            }
        }
        let mut refresh_guard = RefreshGuard {
            service: self.clone(),
            epoch,
            account: account.clone(),
            cid: cid.into(),
            armed: true,
        };
        let client = self.client();
        let result = tokio::select! { biased;
            _ = cancel.cancelled() => Err(Error::Cancelled), _ = session_cancel.cancelled() => Err(Error::Cancelled),
            result = client.progress(cid, cancel) => result,
        };
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.epoch != epoch || runtime.snapshot.user.as_ref().map(|u| &u.id) != Some(&account)
        {
            return Err(Error::Cancelled);
        }
        let entry = runtime.snapshot.progress.entry(cid.into()).or_default();
        entry.refreshing = false;
        match &result {
            Ok(data) => {
                // 数据没变时保留旧的 updated_at_ms，避免每 30 秒都产生一次“新”快照。
                if entry.data.as_ref() != Some(data) {
                    entry.data = Some(data.clone());
                    entry.updated_at_ms = Some(Utc::now().timestamp_millis());
                }
                entry.error = None;
            }
            Err(error) if *error != Error::Cancelled => entry.error = Some(error.message()),
            _ => {}
        }
        if result == Err(Error::Unauthorized) {
            Self::expire_session(&mut runtime);
        }
        self.publish(&mut runtime);
        refresh_guard.armed = false;
        drop(runtime);
        if result == Err(Error::Unauthorized) {
            let _ = self.sync_platform();
        }
        flight.last = if result == Err(Error::Cancelled) {
            None
        } else {
            Some((Instant::now(), result.clone()))
        };
        result
    }
    pub(crate) async fn refresh_progress(self: &Arc<Self>, ids: Vec<String>, background: bool) {
        let mut pending = tokio::task::JoinSet::new();
        for cid in ids {
            let service = self.clone();
            pending.spawn(async move {
                let cancel = CancellationToken::new();
                let _ = if background {
                    service.fetch_progress_background(&cid, &cancel).await
                } else {
                    service.fetch_progress(&cid, &cancel).await
                };
            });
        }
        while pending.join_next().await.is_some() {}
    }
    pub(super) async fn session_ready(self: &Arc<Self>) {
        let ids = {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.snapshot.session != SessionStatus::Ready {
                return;
            }
            let Some(account) = runtime.snapshot.user.as_ref().map(|user| user.id.clone()) else {
                return;
            };
            let day = today();
            runtime
                .checkins
                .daily_scans
                .retain(|(_, date)| *date == day);
            runtime.checkins.daily_scans.insert((account, day));
            runtime
                .snapshot
                .courses
                .iter()
                .map(|course| course.id.clone())
                .collect()
        };
        self.refresh_progress(ids, false).await;
        self.enqueue_automatic_checkins();
        self.run_discussions().await;
        // 只有首页/考试面板正在订阅时才补拉考试与作业，避免无人查看时的额外请求。
        if !self.assessment_subscribers.is_empty() {
            let _ = self.refresh_assessments().await;
        }
    }
}

pub(super) fn start_updates(service: &Arc<Service>) {
    use crate::activity::ActivityTier;
    use std::time::{Duration, Instant};

    let weak = Arc::downgrade(service);
    tauri::async_runtime::spawn(async move {
        let mut last_progress = Instant::now();
        let mut last_assessments_refresh = Instant::now();
        let mut first_progress = true;
        loop {
            let Some(service) = weak.upgrade() else {
                break;
            };
            let active_work = {
                let runtime = service.runtime.lock().unwrap();
                !runtime.workers.is_empty()
                    || !runtime.checkins.queue.is_empty()
                    || runtime
                        .checkins
                        .active
                        .as_ref()
                        .is_some_and(|job| !job.cancel.is_cancelled())
            };
            // 任务/签到在跑时强制 Active；否则按页面是否打开、应用是否在前台分档。
            let tier = if active_work {
                ActivityTier::Active
            } else if crate::activity::tier(Some("uooc")) == ActivityTier::Active {
                ActivityTier::Active
            } else {
                crate::activity::tier(None)
            };
            let progress_interval = match tier {
                ActivityTier::Active => Duration::from_secs(30),
                ActivityTier::Foreground => Duration::from_secs(120),
                ActivityTier::Background => Duration::from_secs(900),
            };
            let assessment_interval = match tier {
                ActivityTier::Active => Duration::from_secs(300),
                ActivityTier::Foreground => Duration::from_secs(900),
                ActivityTier::Background => Duration::from_secs(3600),
            };

            if first_progress || last_progress.elapsed() >= progress_interval {
                first_progress = false;
                // This timer belongs to the native service, not the UOOC page or video queue.
                // On wake/resume it catches up to today's Beijing date without replaying missed days.
                let _ = service.refresh_session(Some(today())).await;
                let ids = {
                    let runtime = service.runtime.lock().unwrap();
                    if active_work || runtime.snapshot.settings.discussion_enabled {
                        runtime
                            .snapshot
                            .courses
                            .iter()
                            .map(|course| course.id.clone())
                            .collect()
                    } else {
                        runtime.workers.keys().cloned().collect()
                    }
                };
                service.refresh_progress(ids, true).await;
                service.run_discussions().await;
                last_progress = Instant::now();
            }

            // 只在有前端订阅时刷新考试/作业，避免无人查看时产生无意义的远端请求。
            if !service.assessment_subscribers.is_empty()
                && last_assessments_refresh.elapsed() >= assessment_interval
            {
                last_assessments_refresh = Instant::now();
                let _ = service.refresh_assessments().await;
            }

            // 分块睡眠让打开 UOOC 页/回到前台时最多一个块后就能提速。
            tokio::time::sleep(crate::activity::sleep_chunk(
                progress_interval,
                Duration::from_secs(15),
            ))
            .await;
        }
    });
}
