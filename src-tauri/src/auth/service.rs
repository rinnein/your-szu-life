use super::{
    model::*, probe::*, refresh::*, session::*, webview::resume_application_window, AuthStore,
    AUTH_HOST, AUTH_LOGIN_URL,
};
use std::thread;
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

pub(super) fn complete_authentication(app: tauri::AppHandle, payload: String) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    if !matches!(
        store.state.lock().ok().as_deref(),
        Some(AuthState::Authenticating)
    ) || !auth_probe_reached_verified_endpoint(&payload)
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
        .captured
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
    if store.credentials.save_session(&session).is_err() || clear_webview_session(&app).is_err() {
        session_failure(&app, &store);
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
    store: &AuthStore,
) -> Result<(), String> {
    let page_url = match window.url() {
        Ok(url) => url.to_string(),
        Err(_) => {
            record_credential_capture_failure("page-url-unavailable");
            return Err("无法验证认证页面".into());
        }
    };
    if !is_auth_login_url(&page_url) {
        record_credential_capture_failure("untrusted-page");
        return Err("仅可在深大统一认证账号密码页面保存凭据".into());
    }
    let state = store.state.lock().map_err(|_| "状态锁失败")?.clone();
    let attempt = store.attempt.lock().map_err(|_| "状态锁失败")?.clone();
    if !matches!(state, AuthState::Authenticating) || !matches!(attempt, AuthAttempt::InitialSetup)
    {
        return Ok(());
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

pub(super) fn get_auth_state(
    app: tauri::AppHandle,
    store: &AuthStore,
) -> Result<AuthState, String> {
    let current = store.state.lock().map_err(|_| "状态锁失败")?.clone();
    if !matches!(current, AuthState::Checking) {
        return Ok(current);
    }

    let profile = store.credentials.load_profile()?;
    let pending = store.credentials.load_pending()?;
    // A missing or unreadable session must not delay local account startup.
    // The background refresh can recover it once the network and keyring are
    // available again.
    let session = store.credentials.load_session().unwrap_or(None);
    *store.profile.lock().map_err(|_| "状态锁失败")? = profile.clone();
    *store.pending.lock().map_err(|_| "状态锁失败")? = pending.clone();

    if let Some(profile) = profile {
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
        .or(store.credentials.load_profile()?);
    let Some(profile) = profile else {
        return Ok(());
    };
    let session = store.credentials.load_session().unwrap_or(None);
    start_session_refresh(app, profile.student_id, session);
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
        if !store.session_refresh_is_current(generation) {
            return;
        }
        match outcome {
            RefreshOutcome::Refreshed(session) => {
                if store.credentials.save_session(&session).is_ok() {
                    let _ = install_auth_session_cookies(&worker_app, &session);
                }
            }
            RefreshOutcome::Invalid => {
                let _ = store.credentials.delete_session();
                store.finish_attempt(AuthState::SessionExpired);
                emit_state(&worker_app, AuthState::SessionExpired);
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
        .or(store.credentials.load_profile()?);
    Ok(profile.as_ref().map(profile_view))
}

pub(super) fn get_user_data(store: &AuthStore) -> Result<Option<UserDataView>, String> {
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_profile()?);
    let pending = store
        .pending
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_pending()?);
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

pub(super) fn navigate_to_authentication(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
    let store = app.try_state::<AuthStore>().ok_or("认证状态不可用")?;
    store.cancel_session_refresh();
    let payments_return = crate::campus_payments::cancel(&app);
    if let Some(current_url) = payments_return.or_else(|| window.url().ok()) {
        if current_url.host_str() != Some(AUTH_HOST) {
            *store.resume_url.lock().map_err(|_| "状态锁失败")? = Some(current_url);
        }
    }
    // The initial `get_auth_state` normally warms these caches. Load from the
    // credential store as a fallback so a reauthentication click still gets
    // the saved form values after an app/window restart.
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_profile()?);
    let pending = store
        .pending
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_pending()?);
    store.reset_auth_flow_guards();
    let prefill = login_form_prefill(profile.as_ref(), pending.as_ref());
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
    *store.login_prefill.lock().map_err(|_| "状态锁失败")? = prefill;
    store.credentials.delete_session()?;
    clear_webview_session(&app)?;
    window
        .navigate(
            AUTH_LOGIN_URL
                .parse::<tauri::Url>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    emit_state(&app, AuthState::Authenticating);
    Ok(())
}

pub(super) fn begin_authentication(app: tauri::AppHandle) -> Result<(), String> {
    navigate_to_authentication(app)
}

pub(super) fn logout(app: tauri::AppHandle, store: &AuthStore) -> Result<(), String> {
    store.cancel_session_refresh();
    crate::campus_payments::cancel(&app);
    store.credentials.delete_profile()?;
    store.credentials.delete_pending()?;
    store.credentials.delete_session()?;
    *store.profile.lock().map_err(|_| "状态锁失败")? = None;
    *store.pending.lock().map_err(|_| "状态锁失败")? = None;
    *store.captured.lock().map_err(|_| "状态锁失败")? = None;
    *store.login_prefill.lock().map_err(|_| "状态锁失败")? = None;
    *store.attempt.lock().map_err(|_| "状态锁失败")? = AuthAttempt::None;
    *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::Unauthenticated;
    clear_webview_session(&app)?;
    emit_state(&app, AuthState::Unauthenticated);
    Ok(())
}
