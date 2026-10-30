use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const LOAD_TIMEOUT: Duration = Duration::from_secs(120);
const PREPARE_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const SUBMIT_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum RecoveryPhase {
    Loading,
    Ready,
    Submitted,
    Manual,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RecoveryOperation {
    Status,
    FormReady,
    Claim,
    Stop,
    PasswordSubmitted,
    DiscardPassword,
    /// 官方页面明确返回登录失败：初始化设置需要自动退回凭据页。
    LoginFailed,
    /// 自动登录已切换为手动，但仍然允许用户在官方页面完成。
    ManualRequired,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryStatus {
    pub(super) phase: RecoveryPhase,
    pub(super) remaining_ms: u64,
}

/// Native ownership survives page reloads. No credentials or URLs live here.
pub(super) struct Recovery {
    pub generation: u64,
    pub document: u64,
    pub active: bool,
    pub automatic: bool,
    pub phase: RecoveryPhase,
    login_document: Option<u64>,
    deadline: Instant,
}

impl Default for Recovery {
    fn default() -> Self {
        Self {
            generation: 0,
            document: 0,
            active: false,
            automatic: false,
            phase: RecoveryPhase::Manual,
            login_document: None,
            deadline: Instant::now(),
        }
    }
}

impl Recovery {
    pub fn begin(&mut self, automatic: bool, now: Instant) {
        self.invalidate();
        self.active = true;
        self.automatic = automatic;
        self.phase = if automatic {
            RecoveryPhase::Loading
        } else {
            RecoveryPhase::Manual
        };
        self.deadline = now + LOAD_TIMEOUT;
    }

    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.active = false;
        self.phase = RecoveryPhase::Manual;
        self.login_document = None;
    }

    pub fn prepare_login_document(&mut self) -> bool {
        if !self.active || self.login_document == Some(self.document) {
            return false;
        }
        self.login_document = Some(self.document);
        true
    }

    pub fn matches(&self, generation: &str, document: &str) -> bool {
        self.active
            && generation.parse::<u64>().ok() == Some(self.generation)
            && document.parse::<u64>().ok() == Some(self.document)
    }

    pub fn expire(&mut self, now: Instant) -> bool {
        if self.active && self.phase != RecoveryPhase::Manual && now >= self.deadline {
            self.phase = RecoveryPhase::Manual;
            return true;
        }
        false
    }

    pub fn control(
        &mut self,
        operation: RecoveryOperation,
        now: Instant,
    ) -> Result<RecoveryStatus, String> {
        self.expire(now);
        match operation {
            RecoveryOperation::Status => {}
            RecoveryOperation::PasswordSubmitted | RecoveryOperation::DiscardPassword => {}
            RecoveryOperation::LoginFailed | RecoveryOperation::ManualRequired => {
                self.phase = RecoveryPhase::Manual;
            }
            RecoveryOperation::FormReady => {
                if self.phase == RecoveryPhase::Loading {
                    self.phase = RecoveryPhase::Ready;
                    self.deadline = now + PREPARE_TIMEOUT;
                }
            }
            RecoveryOperation::Stop => self.phase = RecoveryPhase::Manual,
            RecoveryOperation::Claim => {
                if self.phase != RecoveryPhase::Ready {
                    return Err("自动登录已停止或已提交".into());
                }
                self.phase = RecoveryPhase::Submitted;
                self.deadline = now + SUBMIT_TIMEOUT;
            }
        }
        Ok(RecoveryStatus {
            phase: self.phase,
            remaining_ms: self.deadline.saturating_duration_since(now).as_millis() as u64,
        })
    }

    pub fn probe_budget(&self, now: Instant) -> Duration {
        if self.automatic && self.phase == RecoveryPhase::Submitted {
            self.deadline.saturating_duration_since(now)
        } else {
            SUBMIT_TIMEOUT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_and_finished_share_one_login_document_preparation() {
        let mut flow = Recovery::default();
        flow.begin(true, Instant::now());
        flow.document += 1;
        assert!(flow.prepare_login_document());
        assert!(!flow.prepare_login_document());
        flow.document += 1;
        assert!(flow.prepare_login_document());
        flow.invalidate();
        assert!(!flow.prepare_login_document());
    }

    #[test]
    fn rendering_preparation_and_submission_have_independent_bounded_deadlines() {
        for seconds in [20, 90, 119] {
            let start = Instant::now();
            let ready = start + Duration::from_secs(seconds);
            let mut flow = Recovery::default();
            flow.begin(true, start);
            assert!(!flow.expire(ready));
            assert!(flow.control(RecoveryOperation::Claim, ready).is_err());
            assert_eq!(
                flow.control(RecoveryOperation::FormReady, ready)
                    .unwrap()
                    .remaining_ms,
                30000
            );
            assert_eq!(
                flow.control(
                    RecoveryOperation::FormReady,
                    ready + Duration::from_secs(10)
                )
                .unwrap()
                .remaining_ms,
                20000
            );
            let submitted = ready + Duration::from_secs(29);
            assert_eq!(
                flow.control(RecoveryOperation::Claim, submitted)
                    .unwrap()
                    .remaining_ms,
                90000
            );
            assert_eq!(
                flow.probe_budget(submitted + Duration::from_secs(60))
                    .as_secs(),
                30
            );
            assert!(flow.control(RecoveryOperation::Claim, submitted).is_err());
        }
        let start = Instant::now();
        let mut flow = Recovery::default();
        flow.begin(true, start);
        assert!(flow.expire(start + LOAD_TIMEOUT));
        assert_eq!(
            flow.control(RecoveryOperation::FormReady, start + LOAD_TIMEOUT)
                .unwrap()
                .phase,
            RecoveryPhase::Manual
        );
    }

    #[test]
    fn reload_and_reinjection_cannot_grant_another_submission() {
        let now = Instant::now();
        let mut flow = Recovery::default();
        flow.begin(true, now);
        flow.control(RecoveryOperation::FormReady, now).unwrap();
        let generation = flow.generation.to_string();
        assert!(flow.matches(&generation, "0"));
        flow.control(RecoveryOperation::Claim, now).unwrap();
        flow.document += 1;
        assert!(!flow.matches(&generation, "0"));
        assert!(flow.matches(&generation, "1"));
        assert!(flow.control(RecoveryOperation::Claim, now).is_err());
        flow.begin(true, now);
        flow.control(RecoveryOperation::FormReady, now).unwrap();
        assert!(!flow.matches(&generation, "1"));
        assert!(flow.control(RecoveryOperation::Claim, now).is_ok());
    }

    #[test]
    fn manual_stop_timeout_and_exit_revoke_automation() {
        let now = Instant::now();
        let mut flow = Recovery::default();
        flow.begin(true, now);
        flow.control(RecoveryOperation::FormReady, now).unwrap();
        flow.control(RecoveryOperation::Stop, now).unwrap();
        assert!(flow.control(RecoveryOperation::Claim, now).is_err());
        flow.begin(true, now);
        flow.control(RecoveryOperation::FormReady, now).unwrap();
        assert!(flow
            .control(RecoveryOperation::Claim, now + PREPARE_TIMEOUT)
            .is_err());
        flow.begin(true, now);
        flow.control(RecoveryOperation::FormReady, now).unwrap();
        flow.control(RecoveryOperation::Claim, now).unwrap();
        assert!(!flow.expire(now + SUBMIT_TIMEOUT - Duration::from_millis(1)));
        assert!(flow.expire(now + SUBMIT_TIMEOUT));
        let generation = flow.generation.to_string();
        flow.invalidate();
        assert!(!flow.matches(&generation, "0"));
        flow.begin(false, now);
        assert!(flow.control(RecoveryOperation::Claim, now).is_err());
    }

    #[test]
    fn concurrent_callbacks_receive_only_one_submission_per_attempt() {
        use std::sync::{Arc, Mutex};
        let flow = Arc::new(Mutex::new(Recovery::default()));
        flow.lock().unwrap().begin(true, Instant::now());
        flow.lock()
            .unwrap()
            .control(RecoveryOperation::FormReady, Instant::now())
            .unwrap();
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let flow = flow.clone();
                std::thread::spawn(move || {
                    flow.lock()
                        .unwrap()
                        .control(RecoveryOperation::Claim, Instant::now())
                        .is_ok()
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .filter_map(|worker| worker.join().ok())
                .filter(|claimed| *claimed)
                .count(),
            1
        );
    }
}
