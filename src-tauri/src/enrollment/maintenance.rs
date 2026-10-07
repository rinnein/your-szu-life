//! Best-effort maintenance; this worker never takes the enrollment operation lock.
use super::{model::Snapshot, protocol::Failure};
use std::time::{Duration, Instant};
pub struct Cadence {
    next: Instant,
}
impl Cadence {
    pub fn new(now: Instant) -> Self {
        Self {
            next: now + Duration::from_secs(60),
        }
    }
    pub fn due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + Duration::from_secs(60);
        true
    }
}
pub fn selected_probe(s: &Snapshot, random: u8) -> bool {
    s.can_select() && random % 2 == 0
}
pub fn applies(
    before: &Snapshot,
    now: &Snapshot,
    generation: u64,
    current_generation: u64,
    logged_out: bool,
    account: Option<&str>,
    failure: Failure,
) -> bool {
    failure == Failure::Session
        && !logged_out
        && generation == current_generation
        && before.context.student == now.context.student
        && before.mode == now.mode
        && account == Some(before.context.student.as_str())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::enrollment::model::{Admission, Mode};
    #[test]
    fn skips_missed_ticks_instead_of_replaying_them() {
        let now = Instant::now();
        let mut c = Cadence::new(now);
        assert!(!c.due(now + Duration::from_secs(59)));
        assert!(c.due(now + Duration::from_secs(60)));
        assert!(!c.due(now + Duration::from_secs(60)));
        assert!(c.due(now + Duration::from_secs(600)));
        assert!(!c.due(now + Duration::from_secs(601)));
    }
    #[test]
    fn random_choice_only_uses_selected_list_when_admitted() {
        let mut s = Snapshot::default();
        s.authenticated();
        s.admission = Admission::Ready;
        assert!(selected_probe(&s, 0));
        assert!(!selected_probe(&s, 1));
        s.admission = Admission::Restricted;
        assert!(!selected_probe(&s, 0));
    }
    #[test]
    fn ignores_stale_account_mode_generation_and_temporary_failures() {
        let mut s = Snapshot::default();
        s.context.student = "synthetic".into();
        assert!(applies(
            &s,
            &s,
            1,
            1,
            false,
            Some("synthetic"),
            Failure::Session
        ));
        for failure in [
            Failure::Network,
            Failure::Initializing,
            Failure::AdmissionRestricted,
            Failure::Business,
        ] {
            assert!(!applies(&s, &s, 1, 1, false, Some("synthetic"), failure));
        }
        assert!(!applies(
            &s,
            &s,
            1,
            2,
            false,
            Some("synthetic"),
            Failure::Session
        ));
        assert!(!applies(
            &s,
            &s,
            1,
            1,
            true,
            Some("synthetic"),
            Failure::Session
        ));
        assert!(!applies(
            &s,
            &s,
            1,
            1,
            false,
            Some("other"),
            Failure::Session
        ));
        let mut next = s.clone();
        next.mode = Mode::Webvpn;
        assert!(!applies(
            &s,
            &next,
            1,
            1,
            false,
            Some("synthetic"),
            Failure::Session
        ));
    }
}
