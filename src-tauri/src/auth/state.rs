use super::model::*;
use crate::storage::SharedCredentialStore;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};

pub struct AuthStore {
    pub(super) state: Mutex<AuthState>,
    pub(super) attempt: Mutex<AuthAttempt>,
    pub(super) profile: Mutex<Option<StoredAccountProfile>>,
    pub(super) pending: Mutex<Option<PendingAuthCredentials>>,
    pub(super) captured: Mutex<Option<CapturedCredentials>>,
    pub(super) login_prefill: Mutex<Option<LoginFormPrefill>>,
    pub(super) resume_url: Mutex<Option<tauri::Url>>,
    pub(super) session_restore_redirect_handled: AtomicBool,
    pub(super) auth_probe_running: AtomicBool,
    pub(super) auth_probe_generation: AtomicU64,
    pub(super) auth_completion_claimed: AtomicBool,
    pub(super) credentials: SharedCredentialStore,
}

impl AuthStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            state: Mutex::new(AuthState::Checking),
            attempt: Mutex::new(AuthAttempt::None),
            profile: Mutex::new(None),
            pending: Mutex::new(None),
            captured: Mutex::new(None),
            login_prefill: Mutex::new(None),
            resume_url: Mutex::new(None),
            session_restore_redirect_handled: AtomicBool::new(false),
            auth_probe_running: AtomicBool::new(false),
            auth_probe_generation: AtomicU64::new(0),
            auth_completion_claimed: AtomicBool::new(false),
            credentials,
        }
    }

    pub(super) fn reset_auth_flow_guards(&self) {
        self.session_restore_redirect_handled
            .store(false, Ordering::Release);
        self.auth_probe_generation.fetch_add(1, Ordering::AcqRel);
        self.auth_probe_running.store(false, Ordering::Release);
        self.auth_completion_claimed.store(false, Ordering::Release);
    }

    pub(super) fn claim_session_restore_redirect(&self) -> bool {
        self.session_restore_redirect_handled
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
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
