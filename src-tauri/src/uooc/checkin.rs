use super::{
    client::{CourseSignin, Error},
    model::SessionStatus,
    progress::{Attendance, CourseProgress},
    Runtime, Service,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CheckinStatus {
    Queued,
    Submitting,
    Verifying,
    Succeeded,
    Skipped,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CheckinSource {
    Automatic,
    Manual,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Changed,
    AlreadySatisfied,
    NotRequired,
    AlreadySigned,
    Unchanged,
}

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_secs(60);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckinState {
    pub id: String,
    pub status: CheckinStatus,
    pub source: CheckinSource,
    pub message: String,
    pub updated_at_ms: i64,
}
#[derive(Clone)]
pub(super) struct Job {
    pub id: String,
    pub cid: String,
    pub account: String,
    pub epoch: u64,
    pub source: CheckinSource,
    pub cancel: CancellationToken,
}
#[derive(Default)]
pub(super) struct Checkins {
    pub queue: VecDeque<Job>,
    pub active: Option<Job>,
    pub pumping: bool,
    pub attempted: HashSet<(String, String, chrono::NaiveDate)>,
    pub daily_scans: HashSet<(String, chrono::NaiveDate)>,
}
pub(super) fn cancel_locked(runtime: &mut Runtime, ids: &[String], message: &str) {
    for job in runtime
        .checkins
        .queue
        .iter()
        .chain(runtime.checkins.active.iter())
        .filter(|j| ids.is_empty() || ids.contains(&j.cid))
    {
        job.cancel.cancel();
        if let Some(state) = runtime
            .snapshot
            .checkins
            .get_mut(&job.cid)
            .filter(|state| state.id == job.id)
        {
            state.status = CheckinStatus::Cancelled;
            state.message = message.into();
            state.updated_at_ms = chrono::Utc::now().timestamp_millis();
        }
    }
    runtime.checkins.queue.retain(|j| !j.cancel.is_cancelled());
}
impl Service {
    pub(super) fn enqueue_automatic_checkins(self: &Arc<Self>) {
        let snapshot = self.snapshot();
        let ids = snapshot
            .courses
            .iter()
            .filter(|course| {
                snapshot.progress.get(&course.id).is_some_and(|entry| {
                    entry.error.is_none()
                        && entry
                            .data
                            .as_ref()
                            .is_some_and(|p| p.attendance == Attendance::Needed)
                })
            })
            .map(|c| c.id.clone())
            .collect();
        let _ = self.enqueue_checkins(ids, CheckinSource::Automatic);
    }
    pub(super) fn enqueue_checkins(
        self: &Arc<Self>,
        ids: Vec<String>,
        source: CheckinSource,
    ) -> Result<(), String> {
        let start;
        {
            let mut runtime = self.runtime.lock().unwrap();
            if runtime.snapshot.session != SessionStatus::Ready {
                return Err("请先登录 UOOC".into());
            }
            let account = runtime
                .snapshot
                .user
                .as_ref()
                .ok_or("请先登录 UOOC")?
                .id
                .clone();
            if ids
                .iter()
                .any(|id| !runtime.snapshot.courses.iter().any(|c| &c.id == id))
            {
                return Err("只能为开课中的课程签到".into());
            }
            let day = super::progress::today();
            runtime
                .checkins
                .attempted
                .retain(|(_, _, date)| *date == day);
            for cid in ids {
                if runtime
                    .checkins
                    .queue
                    .iter()
                    .chain(runtime.checkins.active.iter())
                    .any(|job| {
                        !job.cancel.is_cancelled() && job.cid == cid && job.account == account
                    })
                {
                    continue;
                }
                if source == CheckinSource::Automatic
                    && !runtime
                        .checkins
                        .attempted
                        .insert((account.clone(), cid.clone(), day))
                {
                    continue;
                }
                let job = Job {
                    id: format!("uooc-checkin-{}", uuid::Uuid::new_v4()),
                    cid: cid.clone(),
                    account: account.clone(),
                    epoch: runtime.epoch,
                    source,
                    cancel: CancellationToken::new(),
                };
                runtime.snapshot.checkins.insert(
                    cid,
                    CheckinState {
                        id: job.id.clone(),
                        status: CheckinStatus::Queued,
                        source: job.source,
                        message: "等待签到".into(),
                        updated_at_ms: chrono::Utc::now().timestamp_millis(),
                    },
                );
                runtime.checkins.queue.push_back(job);
            }
            start = !runtime.checkins.pumping && !runtime.checkins.queue.is_empty();
            if start {
                runtime.checkins.pumping = true;
            }
            self.publish(&mut runtime);
        }
        if start {
            let service = self.clone();
            tauri::async_runtime::spawn(async move {
                service.pump_checkins().await;
            });
        }
        Ok(())
    }
    pub(super) fn cancel_checkins(&self, ids: Vec<String>) {
        let mut runtime = self.runtime.lock().unwrap();
        cancel_locked(&mut runtime, &ids, "签到已取消");
        self.publish(&mut runtime);
    }
    fn checkin_update(&self, job: &Job, status: CheckinStatus, message: &str) {
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.epoch != job.epoch
            || job.cancel.is_cancelled()
            || runtime.snapshot.session != SessionStatus::Ready
            || runtime.snapshot.user.as_ref().map(|user| &user.id) != Some(&job.account)
        {
            return;
        }
        if let Some(state) = runtime
            .snapshot
            .checkins
            .get_mut(&job.cid)
            .filter(|s| s.id == job.id)
        {
            state.status = status;
            state.message = message.into();
            state.updated_at_ms = chrono::Utc::now().timestamp_millis();
        }
        self.publish(&mut runtime);
    }
    pub(super) fn checkin_current(&self, job: &Job) -> bool {
        let runtime = self.runtime.lock().unwrap();
        !job.cancel.is_cancelled()
            && runtime.epoch == job.epoch
            && runtime.snapshot.session == SessionStatus::Ready
            && runtime
                .snapshot
                .user
                .as_ref()
                .is_some_and(|u| u.id == job.account)
            && runtime.snapshot.courses.iter().any(|c| c.id == job.cid)
    }
    async fn pump_checkins(self: Arc<Self>) {
        loop {
            let job = {
                let mut runtime = self.runtime.lock().unwrap();
                let Some(job) = runtime.checkins.queue.pop_front() else {
                    runtime.checkins.pumping = false;
                    break;
                };
                runtime.checkins.active = Some(job.clone());
                job
            };
            let result = if !self.checkin_current(&job) {
                Err("签到已取消或账户已变化".into())
            } else if let Err(error) = self.sync_platform() {
                Err(error)
            } else {
                tokio::select! { biased;
                    _ = job.cancel.cancelled() => Err("签到已取消".into()),
                    result = self.execute_checkin(&job, POLL_INTERVAL, TIMEOUT) => result,
                }
            };
            match result {
                Ok(Outcome::Changed) => self.checkin_update(
                    &job,
                    CheckinStatus::Succeeded,
                    "签到已确认，检测到签到进度更新",
                ),
                Ok(Outcome::AlreadySatisfied) => self.checkin_update(
                    &job,
                    CheckinStatus::Skipped,
                    "本次无需签到，已更新课程进度",
                ),
                Ok(Outcome::NotRequired) => {
                    self.checkin_update(&job, CheckinStatus::Skipped, "课程入口确认无需签到")
                }
                Ok(Outcome::AlreadySigned) => self.checkin_update(
                    &job,
                    CheckinStatus::Succeeded,
                    "今日已签到，课程入口已确认",
                ),
                Ok(Outcome::Unchanged) => {
                    self.checkin_update(&job, CheckinStatus::Failed, "未检测到签到变化，可稍后重试")
                }
                Err(message) => self.checkin_update(&job, CheckinStatus::Failed, &message),
            }
            {
                let mut runtime = self.runtime.lock().unwrap();
                if runtime
                    .checkins
                    .active
                    .as_ref()
                    .is_some_and(|active| active.id == job.id)
                {
                    runtime.checkins.active = None;
                }
                // A cancelled job may still have received fresh Set-Cookie headers.
                // Never let it overwrite a newer login or another account's keyring entry.
                if runtime.epoch == job.epoch
                    && runtime.snapshot.session == SessionStatus::Ready
                    && runtime
                        .snapshot
                        .user
                        .as_ref()
                        .is_some_and(|user| user.id == job.account)
                {
                    if let Err(error) = self.save_session(&self.client()) {
                        runtime.snapshot.message = error;
                    }
                }
                self.publish(&mut runtime);
            }
            let _ = self.sync_platform();
        }
    }
    pub(super) async fn execute_checkin(
        self: &Arc<Self>,
        job: &Job,
        interval: Duration,
        timeout: Duration,
    ) -> Result<Outcome, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        self.checkin_update(
            job,
            CheckinStatus::Verifying,
            "检测中，正在读取最新签到进度…",
        );
        let before = self.checkin_progress(job, deadline).await?;
        if before.attendance != Attendance::Needed {
            return if before.attendance == Attendance::Unknown {
                Err("签到状态未知，请刷新进度后重试".into())
            } else {
                Ok(Outcome::AlreadySatisfied)
            };
        }
        if !self.checkin_current(job) {
            return Err("签到已取消或账户已变化".into());
        }
        self.checkin_update(job, CheckinStatus::Submitting, "正在提交签到…");
        // Reserve time to verify an uncertain submission instead of resending it.
        let submission_budget = (deadline.saturating_duration_since(tokio::time::Instant::now())
            / 2)
        .min(Duration::from_secs(15));
        let submitted = tokio::time::timeout(
            submission_budget,
            self.client().signin(&job.cid, &job.cancel),
        )
        .await;
        if !self.checkin_current(job) {
            return Err("签到已取消或账户已变化".into());
        }
        let uncertain = match submitted {
            Ok(Ok(CourseSignin::Signed)) => false,
            Ok(Ok(marker @ (CourseSignin::NotRequired | CourseSignin::AlreadySigned))) => {
                self.checkin_progress(job, deadline).await?;
                return Ok(if marker == CourseSignin::NotRequired {
                    Outcome::NotRequired
                } else {
                    Outcome::AlreadySigned
                });
            }
            Err(_) | Ok(Err(Error::Network | Error::Invalid)) => true,
            Ok(Err(Error::Unauthorized)) => {
                let mut runtime = self.runtime.lock().unwrap();
                if runtime.epoch == job.epoch
                    && runtime.snapshot.user.as_ref().map(|user| &user.id) == Some(&job.account)
                {
                    Self::expire_session(&mut runtime);
                    self.publish(&mut runtime);
                }
                return Err(Error::Unauthorized.message());
            }
            Ok(Err(error)) => return Err(format!("签到提交失败：{}", error.message())),
        };
        self.checkin_update(
            job,
            CheckinStatus::Verifying,
            if uncertain {
                "签到提交结果暂未确认，正在核对服务端进度…"
            } else {
                "签到已提交，正在确认服务端进度…"
            },
        );
        loop {
            if tokio::time::Instant::now() >= deadline {
                return if uncertain {
                    Err("签到提交结果未确认，未检测到签到变化；请检查网络后重试".into())
                } else {
                    Ok(Outcome::Unchanged)
                };
            }
            let after = self.checkin_progress(job, deadline).await?;
            if after.confirms_checkin(&before) {
                return Ok(Outcome::Changed);
            }
            tokio::select! { biased;
                _ = job.cancel.cancelled() => return Err("签到检测已取消".into()),
                _ = tokio::time::sleep_until((tokio::time::Instant::now() + interval).min(deadline)) => {},
            }
        }
    }
    async fn checkin_progress(
        self: &Arc<Self>,
        job: &Job,
        deadline: tokio::time::Instant,
    ) -> Result<CourseProgress, String> {
        if !self.checkin_current(job) {
            return Err("签到检测已取消或账户已变化".into());
        }
        let result = tokio::time::timeout_at(deadline, self.fetch_progress(&job.cid, &job.cancel))
            .await
            .map_err(|_| "签到进度查询超时，请检查网络后重试".to_owned())?
            .map_err(|error| error.message())?;
        if !self.checkin_current(job) {
            return Err("签到检测已取消或账户已变化".into());
        }
        Ok(result)
    }
}
