use super::{
    model::{NetworkFeePortalNotice, NoticeCode, PageKind, PageObservation},
    urls::{is_login_url, is_portal_origin, is_target_url, same_application_origin},
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Url;

pub(super) const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
pub(super) const FORM_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const MANUAL_MESSAGE: &str = "请自行办理；应用不会自动充值、扣款或修改套餐。";
pub(super) const CAPTCHA_MESSAGE: &str = "官网需要验证码，请手动完成登录；应用不会重复提交。";
pub(super) const LOGIN_ERROR_MESSAGE: &str =
    "自动登录未完成，请在官网手动登录，或返回设置检查密码。";
pub(super) const FORM_ERROR_MESSAGE: &str =
    "登录表单与预期不符，已停止自动填写，请手动操作或返回。";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Opening,
    AwaitingForm,
    Submitting,
    RestoringTarget,
    Manual,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Action {
    Wait,
    Submit(String),
    RestoreTarget(String),
    HandOff,
    Fail(NoticeCode),
}

pub(super) struct Session {
    pub(super) generation: u64,
    pub(super) account: String,
    pub(super) return_url: Url,
    pub(super) phase: Phase,
    pub(super) phase_since: Instant,
    pub(super) seen_remote: bool,
    pub(super) attempted: bool,
    pub(super) restored_target: bool,
    pub(super) submitted_document: Option<String>,
    pub(super) restoring_document: Option<String>,
    pub(super) manual_message: &'static str,
}

impl Session {
    pub(super) fn new(generation: u64, account: String, return_url: Url, now: Instant) -> Self {
        Self {
            generation,
            account,
            return_url,
            phase: Phase::Opening,
            phase_since: now,
            seen_remote: false,
            attempted: false,
            restored_target: false,
            submitted_document: None,
            restoring_document: None,
            manual_message: MANUAL_MESSAGE,
        }
    }

    pub(super) fn is_manual(&self) -> bool {
        self.phase == Phase::Manual
    }

    pub(super) fn message(&self) -> &'static str {
        match self.phase {
            Phase::Manual => self.manual_message,
            Phase::Submitting => "正在等待官网登录结果；每次打开最多自动提交一次。",
            _ => "正在打开网费官网；遇到验证码或登录错误会停止自动操作。",
        }
    }

    pub(super) fn timeout(&self, now: Instant) -> Option<NoticeCode> {
        let elapsed = now.saturating_duration_since(self.phase_since);
        match self.phase {
            Phase::Opening | Phase::RestoringTarget if elapsed >= LOAD_TIMEOUT => {
                Some(NoticeCode::LoadTimeout)
            }
            Phase::Submitting if elapsed >= LOAD_TIMEOUT => Some(NoticeCode::LoginTimeout),
            Phase::AwaitingForm if elapsed >= FORM_TIMEOUT => Some(NoticeCode::FormTimeout),
            _ => None,
        }
    }

    pub(super) fn hand_off(&mut self, message: &'static str) -> Action {
        self.phase = Phase::Manual;
        self.manual_message = message;
        Action::HandOff
    }

    pub(super) fn observe_location(&mut self, url: &Url, now: Instant) {
        if self.phase == Phase::Opening && is_login_url(url) {
            self.phase = Phase::AwaitingForm;
            self.phase_since = now;
        }
    }

    pub(super) fn observe(&mut self, page: &PageObservation, url: &Url, now: Instant) -> Action {
        if self.phase == Phase::Manual {
            return Action::Wait;
        }
        // A probe of a provisional/previous document is not evidence about the
        // native URL. Ignore it without changing phases or renewing deadlines.
        if page.kind == PageKind::Stale {
            return self.timeout(now).map_or(Action::Wait, Action::Fail);
        }
        if !is_portal_origin(url) {
            return self.hand_off(FORM_ERROR_MESSAGE);
        }
        match page.kind {
            PageKind::Captcha => return self.hand_off(CAPTCHA_MESSAGE),
            PageKind::LoginError => return self.hand_off(LOGIN_ERROR_MESSAGE),
            PageKind::InvalidForm | PageKind::Other => return self.hand_off(FORM_ERROR_MESSAGE),
            PageKind::Manual => return self.hand_off(MANUAL_MESSAGE),
            _ => {}
        }
        if page.kind != PageKind::Loading {
            self.observe_location(url, now);
        }
        if let Some(code) = self.timeout(now) {
            return Action::Fail(code);
        }
        match self.phase {
            Phase::Opening | Phase::AwaitingForm => match page.kind {
                PageKind::LoginReady if is_login_url(url) && !self.attempted => {
                    self.attempted = true; // Reserve before eval: a lost callback must never cause a retry.
                    self.phase = Phase::Submitting;
                    self.phase_since = now;
                    self.submitted_document = Some(page.document_token.clone());
                    Action::Submit(page.document_token.clone())
                }
                PageKind::Portal => self.hand_off(MANUAL_MESSAGE),
                PageKind::LoginReady => self.hand_off(FORM_ERROR_MESSAGE),
                _ => Action::Wait,
            },
            Phase::Submitting => match page.kind {
                PageKind::LoginReady
                    if self.submitted_document.as_deref() == Some(&page.document_token) =>
                {
                    Action::Wait
                }
                PageKind::LoginReady => self.hand_off(LOGIN_ERROR_MESSAGE),
                PageKind::Portal if is_target_url(url) => self.hand_off(MANUAL_MESSAGE),
                PageKind::Portal if !self.restored_target => {
                    self.restored_target = true;
                    self.restoring_document = Some(page.document_token.clone());
                    self.phase = Phase::RestoringTarget;
                    self.phase_since = now;
                    Action::RestoreTarget(page.document_token.clone())
                }
                _ => Action::Wait,
            },
            Phase::RestoringTarget => match page.kind {
                PageKind::Portal if is_target_url(url) => self.hand_off(MANUAL_MESSAGE),
                PageKind::Portal
                    if self.restoring_document.as_deref() == Some(&page.document_token) =>
                {
                    Action::Wait
                }
                PageKind::LoginReady => self.hand_off(LOGIN_ERROR_MESSAGE),
                PageKind::Portal => self.hand_off(MANUAL_MESSAGE),
                _ => Action::Wait,
            },
            Phase::Manual => Action::Wait,
        }
    }
}

#[derive(Default)]
pub(super) struct Runtime {
    pub(super) generation: u64,
    pub(super) active: Option<Session>,
    pub(super) notice: Option<NetworkFeePortalNotice>,
    pub(super) last_account: Option<String>,
}

impl Runtime {
    /// A local reload after an initial timeout is a completed return even when
    /// the remote document never committed. Preserve its one-shot notice and
    /// release ownership so the user can retry. An explicit local route change
    /// also cancels an opening before the first remote page is observed.
    pub(super) fn finish_local_navigation(&mut self, url: &Url, generation: u64) -> bool {
        let finished = self.active.as_ref().is_some_and(|session| {
            session.generation == generation
                && same_application_origin(url, &session.return_url)
                && (session.seen_remote
                    || session.is_manual()
                    || url.path() != session.return_url.path())
        });
        if finished {
            self.active = None;
        }
        finished
    }
}

#[derive(Default)]
pub(crate) struct CampusPaymentsStore {
    pub(super) opening: Mutex<()>,
    pub(super) runtime: Mutex<Runtime>,
    pub(crate) electricity: super::electricity::ElectricityStore,
}

impl CampusPaymentsStore {
    pub(crate) fn new(credentials: crate::storage::SharedCredentialStore) -> Self {
        Self {
            opening: Mutex::new(()),
            runtime: Mutex::new(Runtime::default()),
            electricity: super::electricity::ElectricityStore::new(credentials),
        }
    }
    /// Reserve before any native browsing-data operation. Concurrent opens
    /// must not clear a WebView after another request has already navigated.
    pub(super) fn reserve(
        &self,
        account: String,
        return_url: Url,
        now: Instant,
    ) -> Result<(u64, bool), String> {
        let mut runtime = self.runtime.lock().map_err(|_| "网费服务状态不可用")?;
        if runtime.active.is_some() {
            return Err("网费官网正在打开，请勿重复操作。".into());
        }
        let changed_account = runtime
            .last_account
            .as_ref()
            .is_some_and(|previous| previous != &account);
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        runtime.notice = None;
        runtime.active = Some(Session::new(generation, account, return_url, now));
        Ok((generation, changed_account))
    }

    pub(super) fn prepare_navigation(&self, generation: u64, now: Instant) -> Result<(), String> {
        let mut runtime = self.runtime.lock().map_err(|_| "网费服务状态不可用")?;
        let session = runtime
            .active
            .as_mut()
            .filter(|session| session.generation == generation)
            .ok_or("当前网费任务已取消，请重新打开。")?;
        session.phase_since = now;
        let account = session.account.clone();
        runtime.last_account = Some(account);
        Ok(())
    }

    pub(super) fn cancel_generation(&self, generation: u64) -> Option<Url> {
        let mut runtime = self.runtime.lock().ok()?;
        if runtime.active.as_ref()?.generation != generation {
            return None;
        }
        runtime.active.take().map(|session| session.return_url)
    }

    pub(super) fn fail(&self, generation: u64, code: NoticeCode) -> Option<Url> {
        let mut runtime = self.runtime.lock().ok()?;
        if runtime.active.as_ref()?.generation != generation {
            return None;
        }
        let session = runtime.active.take()?;
        runtime.notice = Some(code.into());
        Some(session.return_url)
    }
}
