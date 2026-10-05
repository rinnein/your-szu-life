use super::{
    model::*, probe::*, refresh::*, session::*, webview::resume_application_window, AuthStore,
    AUTH_HOST,
};
use std::{thread, time::Instant};
use tauri::{Emitter, Manager};

pub(super) fn emit_state(app: &tauri::AppHandle, state: AuthState) {
    let _ = app.emit("auth-state-changed", state);
}

pub(super) fn set_terminal_state(app: &tauri::AppHandle, store: &AuthStore, state: AuthState) {
    store.finish_attempt(state.clone());
    emit_state(app, state);
    resume_application_window(app, store);
}

pub(super) fn session_failure(app: &tauri::AppHandle, store: &AuthStore) {
    let _ = store.credentials.delete_session();
    let _ = clear_webview_session(app);
    set_terminal_state(app, store, AuthState::SessionCaptureFailed);
}

pub(super) fn credential_capture_failure(app: &tauri::AppHandle, store: &AuthStore) {
    // Even a failed first-time setup may already have obtained official
    // cookies. The keyring must remain the only durable session copy.
    let _ = store.credentials.delete_session();
    let _ = clear_webview_session(app);
    set_terminal_state(app, store, AuthState::CredentialCaptureFailed);
}

pub(super) fn complete_authentication(app: tauri::AppHandle, payload: String, generation: u64) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Ok(_operation) = store.session_operations.lock() else {
        return;
    };
    if !store.auth_probe_is_current(generation)
        || !matches!(
            store.state.lock().ok().as_deref(),
            Some(AuthState::Authenticating)
        )
        || !auth_probe_reached_verified_endpoint(&payload)
    {
        return;
    }
    // A profile page can finish loading before its user-config request has
    // settled.  Ignore that intermediate result and let the bounded probe
    // loop try again instead of turning a transient response into a terminal
    // login error.
    if !auth_probe_succeeded(&payload) {
        return;
    }
    let Some(identity) = probe_identity(&payload) else {
        return;
    };
    if !store.claim_auth_completion() {
        return;
    }
    let student_id = identity.student_id;
    let authenticated_name = identity.name;

    let profile = store
        .profile
        .lock()
        .ok()
        .and_then(|profile| profile.clone());
    let pending = store
        .pending
        .lock()
        .ok()
        .and_then(|pending| pending.clone());
    let attempt = store
        .attempt
        .lock()
        .map(|attempt| attempt.clone())
        .unwrap_or(AuthAttempt::None);
    let captured = store
        .submitted_password
        .lock()
        .ok()
        .and_then(|mut captured| captured.take());

    enum Completion {
        Existing(StoredAccountProfile),
        Pending(PendingAuthCredentials),
        NewProfile(StoredAccountProfile),
        NewPending(PendingAuthCredentials),
    }

    let completion = match (profile, pending, attempt) {
        (Some(mut profile), _, AuthAttempt::ReauthenticateExisting { .. }) => {
            if profile.student_id != student_id {
                let _ = store.credentials.delete_session();
                let _ = clear_webview_session(&app);
                set_terminal_state(&app, &store, AuthState::AccountMismatch);
                return;
            }
            if authenticated_name.is_some() {
                profile.name = authenticated_name.clone();
            }
            if let Some(password) = verified_password_update(
                &profile.student_id,
                Some(&profile.campus_card_number),
                &student_id,
                captured.as_ref(),
            ) {
                profile.password = password.to_owned();
            }
            Completion::Existing(profile)
        }
        (_, Some(mut pending), AuthAttempt::InitialSetup) => {
            if pending.student_id != student_id {
                let _ = store.credentials.delete_session();
                let _ = clear_webview_session(&app);
                set_terminal_state(&app, &store, AuthState::AccountMismatch);
                return;
            }
            if authenticated_name.is_some() {
                pending.name = authenticated_name.clone();
            }
            if let Some(password) =
                verified_password_update(&pending.student_id, None, &student_id, captured.as_ref())
            {
                pending.password = password.to_owned();
            }
            Completion::Pending(pending)
        }
        (None, None, AuthAttempt::InitialSetup) => {
            let Some(capture) = captured else {
                set_terminal_state(&app, &store, AuthState::CredentialCaptureFailed);
                return;
            };
            if capture.password.is_empty() {
                set_terminal_state(&app, &store, AuthState::CredentialCaptureFailed);
                return;
            }
            if is_campus_card_number(&capture.identifier) {
                Completion::NewProfile(StoredAccountProfile {
                    student_id: student_id.clone(),
                    campus_card_number: capture.identifier,
                    name: authenticated_name.clone(),
                    password: capture.password,
                })
            } else if is_student_id(&capture.identifier) && capture.identifier == student_id {
                Completion::NewPending(PendingAuthCredentials {
                    student_id: capture.identifier,
                    password: capture.password,
                    name: authenticated_name.clone(),
                })
            } else {
                credential_capture_failure(&app, &store);
                return;
            }
        }
        _ => {
            credential_capture_failure(&app, &store);
            return;
        }
    };

    let Some(window) = app.get_webview_window("main") else {
        session_failure(&app, &store);
        return;
    };
    let Ok(session) = capture_auth_session(&window, student_id.clone()) else {
        session_failure(&app, &store);
        return;
    };
    if !store.auth_probe_is_current(generation) {
        return;
    }
    if store.credentials.save_session(&session).is_err() {
        session_failure(&app, &store);
        return;
    }
    if !store.auth_probe_is_current(generation) {
        return;
    }

    let terminal = match completion {
        Completion::Existing(profile) => {
            if store.credentials.save_profile(&profile).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            if let Ok(mut stored) = store.profile.lock() {
                *stored = Some(profile.clone());
            }
            AuthState::Authenticated {
                name: profile.name,
                student_id: profile.student_id,
            }
        }
        Completion::Pending(pending) => {
            if store.credentials.save_pending(&pending).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            if let Ok(mut stored) = store.pending.lock() {
                *stored = Some(pending.clone());
            }
            AuthState::AwaitingProfile {
                name: pending.name,
                student_id: Some(pending.student_id),
            }
        }
        Completion::NewProfile(profile) => {
            if store.credentials.save_profile(&profile).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            let _ = store.credentials.delete_pending();
            if let Ok(mut stored) = store.profile.lock() {
                *stored = Some(profile.clone());
            }
            if let Ok(mut stored) = store.pending.lock() {
                *stored = None;
            }
            AuthState::Authenticated {
                name: profile.name,
                student_id: profile.student_id,
            }
        }
        Completion::NewPending(pending) => {
            if store.credentials.save_pending(&pending).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            if let Ok(mut stored) = store.pending.lock() {
                *stored = Some(pending.clone());
            }
            AuthState::AwaitingProfile {
                name: pending.name,
                student_id: Some(pending.student_id),
            }
        }
    };
    set_terminal_state(&app, &store, terminal);
}

pub(super) fn record_credential_capture_failure(category: &str) {
    // Never log the captured identifier or password. Categories are useful for
    // diagnosing remote-page drift and capability failures without secrets.
    eprintln!("authentication credential capture rejected: {category}");
}

pub(super) fn capture_auth_credentials(
    window: tauri::WebviewWindow,
    capture: CapturedCredentials,
    generation: &str,
    document_id: &str,
    store: &AuthStore,
) -> Result<(), String> {
    let _operation = store
        .session_operations
        .lock()
        .map_err(|_| "会话操作锁失败")?;
    let _flow = validate_recovery_page(&window, generation, document_id, store)?;
    let state = store.state.lock().map_err(|_| "状态锁失败")?.clone();
    if !matches!(state, AuthState::Authenticating) {
        return Err("认证流程已结束".into());
    }
    if !(is_student_id(&capture.identifier) || is_campus_card_number(&capture.identifier)) {
        record_credential_capture_failure("invalid-identifier");
        return Err("请输入 10 位学号或 6 位校园卡号".into());
    }
    if capture.password.is_empty() {
        record_credential_capture_failure("empty-password");
        return Err("密码不能为空".into());
    }
    *store.captured.lock().map_err(|_| "状态锁失败")? = Some(capture);
    Ok(())
}

fn validate_recovery_page<'a>(
    window: &tauri::WebviewWindow,
    generation: &str,
    document_id: &str,
    store: &'a AuthStore,
) -> Result<std::sync::MutexGuard<'a, super::recovery::Recovery>, String> {
    // Query the native URL before taking the document lock: page-load events
    // run on the UI thread and must remain able to invalidate this document.
    let trusted = window.label() == "main"
        && window
            .url()
            .ok()
            .is_some_and(|url| is_auth_login_url(url.as_str()));
    let flow = store.recovery.lock().map_err(|_| "状态锁失败")?;
    if !trusted || !flow.matches(generation, document_id) {
        return Err("认证页面或流程已失效".into());
    }
    Ok(flow)
}

pub(super) fn control_auth_recovery(
    window: tauri::WebviewWindow,
    generation: &str,
    document_id: &str,
    operation: super::recovery::RecoveryOperation,
    store: &AuthStore,
) -> Result<super::recovery::RecoveryStatus, String> {
    let _operation = store
        .session_operations
        .lock()
        .map_err(|_| "会话操作锁失败")?;
    let mut flow = validate_recovery_page(&window, generation, document_id, store)?;
    match operation {
        super::recovery::RecoveryOperation::PasswordSubmitted => {
            let capture = store
                .captured
                .lock()
                .map_err(|_| "状态锁失败")?
                .clone()
                .ok_or("缺少本次登录凭据")?;
            *store.submitted_password.lock().map_err(|_| "状态锁失败")? = Some(capture);
        }
        super::recovery::RecoveryOperation::DiscardPassword => {
            *store.captured.lock().map_err(|_| "状态锁失败")? = None;
            *store.submitted_password.lock().map_err(|_| "状态锁失败")? = None;
        }
        _ => {}
    }
    flow.control(operation, Instant::now())
}

pub(super) fn get_auth_state(
    app: tauri::AppHandle,
    store: &AuthStore,
) -> Result<AuthState, String> {
    let _operation = store
        .session_operations
        .lock()
        .map_err(|_| "会话操作锁失败")?;
    let current = store.state.lock().map_err(|_| "状态锁失败")?.clone();
    if !matches!(current, AuthState::Checking) {
        return Ok(current);
    }

    let profile = store.credentials.load_profile()?;
    let pending = store.credentials.load_pending()?;
    // Missing sessions can be refreshed in the background. Unreadable storage
    // must not be interpreted as expiry or overwritten with a new session.
    let session = store.credentials.load_session()?;
    let legacy_session = session
        .as_ref()
        .is_some_and(|session| !session.can_restore());
    *store.profile.lock().map_err(|_| "状态锁失败")? = profile.clone();
    *store.pending.lock().map_err(|_| "状态锁失败")? = pending.clone();

    if let Some(profile) = profile {
        if legacy_session {
            let _ = store.credentials.delete_session();
            *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::SessionExpired;
            recover_expired_session(&app, store);
            return Ok(store.state.lock().map_err(|_| "状态锁失败")?.clone());
        }
        let session = match session
            .as_ref()
            .map(|session| {
                session.restorable_snapshot(&profile.student_id, time::OffsetDateTime::now_utc())
            })
            .transpose()
        {
            Ok(session) => session.flatten(),
            Err(error) => {
                let state = if error == SessionRestoreError::AccountMismatch {
                    AuthState::AccountMismatch
                } else {
                    AuthState::SessionCaptureFailed
                };
                store.finish_attempt(state.clone());
                emit_state(&app, state.clone());
                return Ok(state);
            }
        };
        // Restore the native jar before any HTTP validation or page uses it.
        // Offline/transient validation failures must leave this copy intact.
        if let Some(session) = &session {
            if install_auth_session_cookies(&app, session).is_err() {
                store.finish_attempt(AuthState::SessionCaptureFailed);
                emit_state(&app, AuthState::SessionCaptureFailed);
                return Ok(AuthState::SessionCaptureFailed);
            }
        }
        let next = AuthState::Authenticated {
            name: profile.name,
            student_id: profile.student_id.clone(),
        };
        *store.state.lock().map_err(|_| "状态锁失败")? = next.clone();
        emit_state(&app, next.clone());
        start_session_refresh(app, profile.student_id, session);
        return Ok(next);
    }
    let next = if let Some(pending) = pending {
        AuthState::AwaitingProfile {
            name: pending.name,
            student_id: Some(pending.student_id),
        }
    } else {
        let _ = store.credentials.delete_session();
        AuthState::Unauthenticated
    };
    *store.state.lock().map_err(|_| "状态锁失败")? = next.clone();
    emit_state(&app, next.clone());
    Ok(next)
}

pub(super) fn refresh_auth_session(app: tauri::AppHandle) -> Result<(), String> {
    let store = app.try_state::<AuthStore>().ok_or("认证状态不可用")?;
    let _operation = store
        .session_operations
        .lock()
        .map_err(|_| "会话操作锁失败")?;
    // Refresh is only a background continuation of an already authenticated
    // local account. Reauthentication and terminal states must not start a
    // second worker or emit duplicate expiry events.
    if !matches!(
        store.state.lock().map_err(|_| "状态锁失败")?.clone(),
        AuthState::Authenticated { .. }
    ) {
        return Ok(());
    }
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .map_or_else(
            || store.credentials.load_profile(),
            |profile| Ok(Some(profile)),
        )?;
    let Some(profile) = profile else {
        return Ok(());
    };
    let session = store.credentials.load_session()?;
    start_session_refresh(app.clone(), profile.student_id, session);
    Ok(())
}

fn start_session_refresh(
    app: tauri::AppHandle,
    student_id: String,
    session: Option<AuthSessionSecret>,
) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Some(generation) = store.try_start_session_refresh() else {
        return;
    };
    let worker_app = app.clone();
    thread::spawn(move || {
        let outcome = refresh_session(session, &student_id);
        let Some(store) = worker_app.try_state::<AuthStore>() else {
            return;
        };
        let Ok(_operation) = store.session_operations.lock() else {
            return;
        };
        if !store.session_refresh_is_current(generation) {
            return;
        }
        match outcome {
            RefreshOutcome::Refreshed(session) => {
                if store.credentials.save_session(&session).is_ok() {
                    if install_auth_session_cookies(&worker_app, &session).is_err() {
                        store.finish_attempt(AuthState::SessionCaptureFailed);
                        emit_state(&worker_app, AuthState::SessionCaptureFailed);
                    }
                }
            }
            RefreshOutcome::Invalid => {
                let _ = store.credentials.delete_session();
                store.finish_attempt(AuthState::SessionExpired);
                recover_expired_session(&worker_app, &store);
            }
            RefreshOutcome::Mismatch => {
                let _ = store.credentials.delete_session();
                store.finish_attempt(AuthState::AccountMismatch);
                emit_state(&worker_app, AuthState::AccountMismatch);
            }
            RefreshOutcome::Unavailable => {}
        }
        store.finish_session_refresh(generation);
    });
}

pub(super) fn get_account_profile(store: &AuthStore) -> Result<Option<AccountProfileView>, String> {
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .map_or_else(
            || store.credentials.load_profile(),
            |profile| Ok(Some(profile)),
        )?;
    Ok(profile.as_ref().map(profile_view))
}

pub(super) fn get_user_data(store: &AuthStore) -> Result<Option<UserDataView>, String> {
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .map_or_else(
            || store.credentials.load_profile(),
            |profile| Ok(Some(profile)),
        )?;
    if profile.is_some() {
        return Ok(user_data_view(profile.as_ref(), None));
    }
    let pending = store
        .pending
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .map_or_else(
            || store.credentials.load_pending(),
            |pending| Ok(Some(pending)),
        )?;
    Ok(user_data_view(profile.as_ref(), pending.as_ref()))
}

pub(super) fn confirm_profile(
    app: tauri::AppHandle,
    input: AccountProfileInput,
    store: &AuthStore,
) -> Result<(), String> {
    let current = store.profile.lock().map_err(|_| "状态锁失败")?.clone();
    let pending = store.pending.lock().map_err(|_| "状态锁失败")?.clone();
    let current_password = current
        .as_ref()
        .map(|profile| profile.password.as_str())
        .or_else(|| pending.as_ref().map(|pending| pending.password.as_str()));
    input.validate(current_password)?;
    crate::campus_payments::cancel(&app);
    let profile = StoredAccountProfile {
        student_id: input.student_id,
        campus_card_number: input.campus_card_number,
        name: input
            .name
            .or_else(|| pending.as_ref().and_then(|pending| pending.name.clone())),
        password: input
            .password
            .or_else(|| current.map(|profile| profile.password))
            .or_else(|| pending.map(|pending| pending.password))
            .unwrap_or_default(),
    };
    store.credentials.save_profile(&profile)?;
    store.credentials.delete_pending()?;
    *store.profile.lock().map_err(|_| "状态锁失败")? = Some(profile.clone());
    *store.pending.lock().map_err(|_| "状态锁失败")? = None;
    let state = AuthState::Authenticated {
        name: profile.name.clone(),
        student_id: profile.student_id.clone(),
    };
    *store.state.lock().map_err(|_| "状态锁失败")? = state.clone();
    emit_state(&app, state);
    Ok(())
}

pub(super) fn navigate_to_authentication(
    app: tauri::AppHandle,
    user_agent: Option<&str>,
) -> Result<(), String> {
    let store = app.try_state::<AuthStore>().ok_or("认证状态不可用")?;
    let _operation = store
        .session_operations
        .lock()
        .map_err(|_| "会话操作锁失败")?;
    if matches!(
        *store.state.lock().map_err(|_| "状态锁失败")?,
        AuthState::Authenticating
    ) {
        return Ok(());
    }
    let result = navigate_to_authentication_locked(&app, &store, user_agent);
    if result.is_err() {
        set_terminal_state(&app, &store, AuthState::SessionExpired);
    }
    result
}

// Called only while session_operations is held, including background expiry.
fn recover_expired_session(app: &tauri::AppHandle, store: &AuthStore) {
    let can_recover = store
        .profile
        .lock()
        .ok()
        .and_then(|profile| profile.clone())
        .is_some_and(|profile| is_student_id(&profile.student_id) && !profile.password.is_empty());
    if !can_recover {
        emit_state(app, AuthState::SessionExpired);
        return;
    }
    if navigate_to_authentication_locked(app, store, None).is_err() {
        set_terminal_state(app, store, AuthState::SessionExpired);
    }
}

fn navigate_to_authentication_locked(
    app: &tauri::AppHandle,
    store: &AuthStore,
    user_agent: Option<&str>,
) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
    store.cancel_session_refresh();
    let payments_return = crate::campus_payments::cancel(&app);
    if let Some(current_url) = payments_return.or_else(|| window.url().ok()) {
        if current_url.host_str() != Some(AUTH_HOST) {
            *store.resume_url.lock().map_err(|_| "状态锁失败")? = Some(current_url);
        }
    }
    #[cfg(target_os = "android")]
    if !super::android_cookies::supports_cookie_info()? {
        set_terminal_state(&app, &store, AuthState::WebViewUpdateRequired);
        return Ok(());
    }
    // The initial `get_auth_state` normally warms these caches. Load from the
    // credential store as a fallback so a reauthentication click still gets
    // the saved form values after an app/window restart.
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .map_or_else(
            || manual_login_credentials(store, store.credentials.load_profile()),
            |profile| Ok(Some(profile)),
        )?;
    let pending = store
        .pending
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .map_or_else(
            || manual_login_credentials(store, store.credentials.load_pending()),
            |pending| Ok(Some(pending)),
        )?;
    store.reset_auth_flow_guards();
    let prefill = login_form_prefill(profile.as_ref(), pending.as_ref());
    let automatic = profile
        .as_ref()
        .is_some_and(|profile| !profile.password.is_empty());
    let attempt = match profile.as_ref() {
        Some(profile) => AuthAttempt::ReauthenticateExisting {
            student_id: profile.student_id.clone(),
        },
        None => AuthAttempt::InitialSetup,
    };
    *store.profile.lock().map_err(|_| "状态锁失败")? = profile;
    *store.pending.lock().map_err(|_| "状态锁失败")? = pending;
    *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::Authenticating;
    *store.attempt.lock().map_err(|_| "状态锁失败")? = attempt;
    *store.captured.lock().map_err(|_| "状态锁失败")? = None;
    *store.submitted_password.lock().map_err(|_| "状态锁失败")? = None;
    *store.login_prefill.lock().map_err(|_| "状态锁失败")? = prefill;
    store.credentials.delete_session()?;
    if clear_webview_session(&app).is_err() {
        set_terminal_state(&app, &store, AuthState::SessionCaptureFailed);
        return Ok(());
    }
    super::webview::show_authentication_window(app, store)?;
    store
        .recovery
        .lock()
        .map_err(|_| "状态锁失败")?
        .begin(automatic, Instant::now());
    window
        .navigate(authentication_url(user_agent))
        .map_err(|error| error.to_string())?;
    emit_state(&app, AuthState::Authenticating);
    super::webview::start_recovery_deadline(app, store);
    Ok(())
}

// Explicitly opening login may start with unreadable keyring credentials when
// the authorized file backend is usable. Corrupt files never take this path.
fn manual_login_credentials<T>(
    _store: &AuthStore,
    result: Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    match result {
        Err(error) if error == crate::storage::CREDENTIALS_UNAVAILABLE => Ok(None),
        result => result,
    }
}

pub(super) fn begin_authentication(
    app: tauri::AppHandle,
    user_agent: Option<&str>,
) -> Result<(), String> {
    navigate_to_authentication(app, user_agent)
}

pub(super) fn logout(app: tauri::AppHandle, store: &AuthStore) -> Result<(), String> {
    let _operation = store
        .session_operations
        .lock()
        .map_err(|_| "会话操作锁失败")?;
    let was_authenticating = store.recovery.lock().map_err(|_| "状态锁失败")?.active;
    store.cancel_session_refresh();
    store.reset_auth_flow_guards();
    crate::campus_payments::cancel(&app);
    store.credentials.delete_profile()?;
    store.credentials.delete_pending()?;
    store.credentials.delete_session()?;
    *store.profile.lock().map_err(|_| "状态锁失败")? = None;
    *store.pending.lock().map_err(|_| "状态锁失败")? = None;
    *store.captured.lock().map_err(|_| "状态锁失败")? = None;
    *store.submitted_password.lock().map_err(|_| "状态锁失败")? = None;
    *store.login_prefill.lock().map_err(|_| "状态锁失败")? = None;
    *store.attempt.lock().map_err(|_| "状态锁失败")? = AuthAttempt::None;
    *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::Unauthenticated;
    clear_webview_session(&app)?;
    emit_state(&app, AuthState::Unauthenticated);
    if was_authenticating {
        resume_application_window(&app, store);
    }
    Ok(())
}

pub(super) fn cancel_departed_authentication(app: tauri::AppHandle, generation: u64) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Ok(_operation) = store.session_operations.lock() else {
        return;
    };
    if !store
        .recovery
        .lock()
        .ok()
        .is_some_and(|flow| !flow.active && flow.generation == generation)
        || !store
            .state
            .lock()
            .ok()
            .is_some_and(|state| matches!(*state, AuthState::Authenticating))
    {
        return;
    }
    let _ = clear_webview_session(&app);
    let _ = store.credentials.delete_session();
    let state = if store
        .profile
        .lock()
        .ok()
        .is_some_and(|profile| profile.is_some())
    {
        AuthState::SessionExpired
    } else {
        AuthState::Unauthenticated
    };
    store.finish_attempt(state.clone());
    emit_state(&app, state);
    #[cfg(target_os = "android")]
    let _ = super::android_cookies::finish_authentication(true);
}
