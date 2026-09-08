use super::model::*;
use crate::storage::SharedCredentialStore;
use std::sync::Mutex;

pub struct AuthStore {
    pub(super) state: Mutex<AuthState>,
    pub(super) attempt: Mutex<AuthAttempt>,
    pub(super) profile: Mutex<Option<StoredAccountProfile>>,
    pub(super) pending: Mutex<Option<PendingAuthCredentials>>,
    pub(super) captured: Mutex<Option<CapturedCredentials>>,
    pub(super) login_prefill: Mutex<Option<LoginFormPrefill>>,
    pub(super) resume_url: Mutex<Option<tauri::Url>>,
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
            credentials,
        }
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
