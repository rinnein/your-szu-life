use super::{
    checkin_view,
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
    Loading,
    Verifying,
    Succeeded,
    Skipped,
    Cancelled,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckinState {
    pub id: String,
    pub status: CheckinStatus,
    pub visible: bool,
    pub message: String,
    pub updated_at_ms: i64,
}
#[derive(Clone)]
pub(super) struct Job {
    pub id: String,
    pub cid: String,
    pub account: String,
    pub epoch: u64,
    pub visible: bool,
    pub parent: String,
    pub cancel: CancellationToken,
}
#[derive(Default)]
pub(super) struct Checkins {
    pub queue: VecDeque<Job>,
    pub active: Option<Job>,
    pub pumping: bool,
    pub attempted: HashSet<(String, String)>,
    pub closing: HashSet<String>,
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
        let _ = self.enqueue_checkins(ids, false, "main".into());
    }
    pub(super) fn enqueue_checkins(
        self: &Arc<Self>,
        ids: Vec<String>,
        visible: bool,
        parent: String,
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
                if !visible
                    && !runtime
                        .checkins
                        .attempted
                        .insert((account.clone(), cid.clone()))
                {
                    continue;
                }
                let job = Job {
                    id: format!("uooc-checkin-{}", uuid::Uuid::new_v4()),
                    cid: cid.clone(),
                    account: account.clone(),
                    epoch: runtime.epoch,
                    visible,
                    parent: parent.clone(),
                    cancel: CancellationToken::new(),
                };
                runtime.snapshot.checkins.insert(
                    cid,
                    CheckinState {
                        id: job.id.clone(),
                        status: CheckinStatus::Queued,
                        visible,
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
        if runtime.epoch != job.epoch || job.cancel.is_cancelled() {
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
                    result = tokio::time::timeout(Duration::from_secs(60), self.execute_checkin(&job)) => result.unwrap_or_else(|_| Err("签到超时，未获得服务端确认；可手动重试".into())),
                }
            };
            match result {
                Ok(true) => {
                    self.checkin_update(&job, CheckinStatus::Succeeded, "签到成功，已由服务端确认")
                }
                Ok(false) => self.checkin_update(
                    &job,
                    CheckinStatus::Skipped,
                    "本次无需签到，已更新课程进度",
                ),
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
                runtime.checkins.closing.remove(&job.id);
                self.publish(&mut runtime);
            }
            let _ = self.sync_platform();
        }
    }
    async fn execute_checkin(self: &Arc<Self>, job: &Job) -> Result<bool, String> {
        let before = self
            .fetch_progress(&job.cid, &job.cancel)
            .await
            .map_err(|e| e.message())?;
        if before.attendance != Attendance::Needed {
            return if before.attendance == Attendance::Unknown {
                Err("签到状态未知，请刷新进度后重试".into())
            } else {
                Ok(false)
            };
        }
        if !self.checkin_current(job) {
            return Err("签到已取消或账户已变化".into());
        }
        self.checkin_update(
            job,
            CheckinStatus::Loading,
            if job.visible {
                "正在打开官方课程页面…"
            } else {
                "正在后台加载官方课程页面…"
            },
        );
        let page = checkin_view::open(self.clone(), job.clone()).await?;
        self.verify_checkin(
            job,
            &before,
            &*page,
            Duration::from_secs(10),
            Duration::from_secs(2),
        )
        .await
    }
    pub(super) async fn verify_checkin(
        self: &Arc<Self>,
        job: &Job,
        before: &CourseProgress,
        page: &dyn checkin_view::Page,
        settle: Duration,
        interval: Duration,
    ) -> Result<bool, String> {
        loop {
            if !self.checkin_current(job) {
                return Err("签到已取消或账户已变化".into());
            }
            if page.ready().await? {
                break;
            }
            tokio::time::sleep(interval).await;
        }
        tokio::time::sleep(settle).await;
        page.synchronize(self.clone(), job.clone()).await?;
        self.checkin_update(job, CheckinStatus::Verifying, "等待服务端确认签到结果…");
        loop {
            if !self.checkin_current(job) {
                return Err("签到已取消或账户已变化".into());
            }
            // Detect a closed/redirected visible window while verifying, too.
            if !page.ready().await? {
                tokio::time::sleep(interval).await;
                continue;
            }
            let after = self
                .fetch_progress(&job.cid, &job.cancel)
                .await
                .map_err(|e| e.message())?;
            if after.confirms_checkin(before) {
                return Ok(true);
            }
            tokio::time::sleep(interval).await;
        }
    }
}
pub(super) fn window_closed(service: &Arc<Service>, label: &str) {
    let mut runtime = service.runtime.lock().unwrap();
    if runtime.checkins.closing.remove(label) {
        return;
    }
    let ids: Vec<_> = runtime
        .checkins
        .queue
        .iter()
        .chain(runtime.checkins.active.iter())
        .filter(|job| job.id == label || (job.visible && job.parent == label))
        .map(|job| job.cid.clone())
        .collect();
    if !ids.is_empty() {
        cancel_locked(&mut runtime, &ids, "签到窗口已关闭");
        service.publish(&mut runtime);
    }
}
