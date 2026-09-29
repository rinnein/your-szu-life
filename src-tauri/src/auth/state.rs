use super::model::*;
use crate::storage::SharedCredentialStore;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

const SESSION_REFRESH_COOLDOWN_MS: u64 = 10_000;

pub struct AuthStore {
    // Serializes native cookie writes with reauthentication and logout.
    pub(super) session_operations: Mutex<()>,
    pub(super) state: Mutex<AuthState>,
    pub(super) attempt: Mutex<AuthAttempt>,
    pub(super) profile: Mutex<Option<StoredAccountProfile>>,
    pub(super) pending: Mutex<Option<PendingAuthCredentials>>,
    pub(super) captured: Mutex<Option<CapturedCredentials>>,
    pub(super) login_prefill: Mutex<Option<LoginFormPrefill>>,
    pub(super) resume_url: Mutex<Option<tauri::Url>>,
    pub(super) auth_probe_running: AtomicBool,
    pub(super) auth_probe_generation: AtomicU64,
    pub(super) auth_completion_claimed: AtomicBool,
    pub(super) session_refresh_running: AtomicBool,
    pub(super) session_refresh_generation: AtomicU64,
    pub(super) session_refresh_last_started_ms: AtomicU64,
    pub(super) credentials: SharedCredentialStore,
}

impl AuthStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            session_operations: Mutex::new(()),
            state: Mutex::new(AuthState::Checking),
            attempt: Mutex::new(AuthAttempt::None),
            profile: Mutex::new(None),
            pending: Mutex::new(None),
            captured: Mutex::new(None),
            login_prefill: Mutex::new(None),
            resume_url: Mutex::new(None),
            auth_probe_running: AtomicBool::new(false),
            auth_probe_generation: AtomicU64::new(0),
            auth_completion_claimed: AtomicBool::new(false),
            session_refresh_running: AtomicBool::new(false),
            session_refresh_generation: AtomicU64::new(0),
            session_refresh_last_started_ms: AtomicU64::new(0),
            credentials,
        }
    }

    pub(super) fn reset_auth_flow_guards(&self) {
        self.auth_probe_generation.fetch_add(1, Ordering::AcqRel);
        self.auth_probe_running.store(false, Ordering::Release);
        self.auth_completion_claimed.store(false, Ordering::Release);
        self.session_refresh_last_started_ms
            .store(0, Ordering::Release);
        self.cancel_session_refresh();
    }

    pub(super) fn try_start_session_refresh(&self) -> Option<u64> {
        if self
            .session_refresh_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return None;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        let last = self.session_refresh_last_started_ms.load(Ordering::Acquire);
        if now.saturating_sub(last) < SESSION_REFRESH_COOLDOWN_MS && last != 0 {
            self.session_refresh_running.store(false, Ordering::Release);
            return None;
        }
        self.session_refresh_last_started_ms
            .store(now, Ordering::Release);
        Some(self.session_refresh_generation.load(Ordering::Acquire))
    }

    pub(super) fn cancel_session_refresh(&self) {
        self.session_refresh_generation
            .fetch_add(1, Ordering::AcqRel);
        self.session_refresh_running.store(false, Ordering::Release);
    }

    pub(super) fn session_refresh_is_current(&self, generation: u64) -> bool {
        self.session_refresh_running.load(Ordering::Acquire)
            && self.session_refresh_generation.load(Ordering::Acquire) == generation
    }

    pub(super) fn finish_session_refresh(&self, generation: u64) {
        if self.session_refresh_generation.load(Ordering::Acquire) == generation {
            self.session_refresh_running.store(false, Ordering::Release);
        }
    }

    pub(super) fn try_start_auth_probe(&self) -> Option<u64> {
        if self
            .auth_probe_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return None;
        }
        Some(self.auth_probe_generation.load(Ordering::Acquire))
    }

    pub(super) fn cancel_auth_probe(&self) {
        self.auth_probe_generation.fetch_add(1, Ordering::AcqRel);
        self.auth_probe_running.store(false, Ordering::Release);
    }

    pub(super) fn auth_probe_is_current(&self, generation: u64) -> bool {
        self.auth_probe_running.load(Ordering::Acquire)
            && self.auth_probe_generation.load(Ordering::Acquire) == generation
    }

    pub(super) fn finish_auth_probe(&self, generation: u64) {
        if self.auth_probe_generation.load(Ordering::Acquire) == generation {
            self.auth_probe_running.store(false, Ordering::Release);
        }
    }

    pub(super) fn claim_auth_completion(&self) -> bool {
        self.auth_completion_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(super) fn auth_completion_is_claimed(&self) -> bool {
        self.auth_completion_claimed.load(Ordering::Acquire)
    }

    pub(super) fn finish_attempt(&self, state: AuthState) {
        if let Ok(mut current) = self.state.lock() {
            *current = state;
        }
        if let Ok(mut attempt) = self.attempt.lock() {
            *attempt = AuthAttempt::None;
        }
        if let Ok(mut captured) = self.captured.lock() {
            *captured = None;
        }
        if let Ok(mut prefill) = self.login_prefill.lock() {
            *prefill = None;
        }
        self.auth_probe_running.store(false, Ordering::Release);
        self.auth_completion_claimed.store(false, Ordering::Release);
    }

    /// A narrow, password-free account snapshot for authenticated service flows.
    pub(super) fn authenticated_campus_card_number(&self) -> Result<String, String> {
        let state = self.state.lock().map_err(|_| "认证状态不可用")?;
        let AuthState::Authenticated { student_id, .. } = &*state else {
            return Err("请先完成应用统一认证，再打开网费服务。".into());
        };
        let profile = self.profile.lock().map_err(|_| "账户资料不可用")?;
        let profile = profile
            .as_ref()
            .filter(|profile| &profile.student_id == student_id)
            .ok_or("当前账户资料不可用，请重新认证。")?;
        if profile.campus_card_number.len() != 6
            || !profile
                .campus_card_number
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            return Err("校园卡号必须为 6 位数字，请先在设置中补全。".into());
        }
        Ok(profile.campus_card_number.clone())
    }

    /// Take only the two required values; no auth lock escapes this boundary.
    pub(super) fn network_credentials(&self) -> Result<(String, String), String> {
        {
            let profile = self.profile.lock().map_err(|_| "状态锁失败")?;
            if let Some(profile) = profile.as_ref() {
                return Ok((profile.campus_card_number.clone(), profile.password.clone()));
            }
        }
        let profile = self
            .credentials
            .load_profile()?
            .ok_or("当前账户尚未保存校园卡号和密码")?;
        Ok((profile.campus_card_number, profile.password))
    }
}
