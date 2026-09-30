use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const PREPARE_TIMEOUT: Duration = Duration::from_secs(15);
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum RecoveryPhase {
    Ready,
    Submitted,
    Manual,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RecoveryOperation {
    Status,
    Claim,
    Stop,
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
            RecoveryPhase::Ready
        } else {
            RecoveryPhase::Manual
        };
        self.deadline = now + PREPARE_TIMEOUT;
    }

    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.active = false;
        self.phase = RecoveryPhase::Manual;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_and_reinjection_cannot_grant_another_submission() {
        let now = Instant::now();
        let mut flow = Recovery::default();
        flow.begin(true, now);
        let generation = flow.generation.to_string();
        assert!(flow.matches(&generation, "0"));
        flow.control(RecoveryOperation::Claim, now).unwrap();
        flow.document += 1;
        assert!(!flow.matches(&generation, "0"));
        assert!(flow.matches(&generation, "1"));
        assert!(flow.control(RecoveryOperation::Claim, now).is_err());
        flow.begin(true, now);
        assert!(!flow.matches(&generation, "1"));
        assert!(flow.control(RecoveryOperation::Claim, now).is_ok());
    }

    #[test]
    fn manual_stop_timeout_and_exit_revoke_automation() {
        let now = Instant::now();
        let mut flow = Recovery::default();
        flow.begin(true, now);
        flow.control(RecoveryOperation::Stop, now).unwrap();
        assert!(flow.control(RecoveryOperation::Claim, now).is_err());
        flow.begin(true, now);
        assert!(flow
            .control(RecoveryOperation::Claim, now + PREPARE_TIMEOUT)
            .is_err());
        flow.begin(true, now);
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
