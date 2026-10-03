use super::{
    model::AuthState,
    probe::{auth_probe_reached_verified_endpoint, is_auth_login_url, is_auth_profile_url},
    scripts::*,
    service::{complete_authentication, credential_capture_failure},
    AuthStore,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "android"))]
const LOCAL_ENTRY_URL: &str = "http://tauri.localhost/";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
const LOCAL_ENTRY_URL: &str = "tauri://localhost/";

const LOGIN_CAPTURE_RETRY_DELAYS_MS: &[u64] = &[100, 250, 500, 1_000, 2_000];
const AUTH_PROBE_INTERVAL: Duration = Duration::from_millis(250);

pub(super) fn resume_application_window(app: &tauri::AppHandle, store: &AuthStore) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.navigate(application_resume_url(store));
    let success = store
        .state
        .lock()
        .ok()
        .is_some_and(|state| matches!(*state, AuthState::Authenticated { .. }));
    #[cfg(target_os = "android")]
    let _ = super::android_cookies::finish_authentication(success);
    let previous = store
        .resume_window
        .lock()
        .ok()
        .and_then(|mut label| label.take());
    if success {
        if let Some(previous) = previous.and_then(|label| app.get_webview_window(&label)) {
            let _ = previous.show();
            let _ = previous.set_focus();
        }
    }
}

pub(super) fn application_resume_url(store: &AuthStore) -> tauri::Url {
    store
        .resume_url
        .lock()
        .ok()
        .and_then(|mut url| url.take())
        .unwrap_or_else(|| LOCAL_ENTRY_URL.parse().expect("valid local entry URL"))
}

fn inject_login_capture(window: &tauri::Webview) {
    let app = window.app_handle();
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Some((generation, document)) = store
        .recovery
        .lock()
        .ok()
        .filter(|flow| flow.active)
        .map(|flow| (flow.generation, flow.document))
    else {
        return;
    };
    let retry_window = window.clone();
    thread::spawn(move || {
        for delay in std::iter::once(&0).chain(LOGIN_CAPTURE_RETRY_DELAYS_MS) {
            thread::sleep(Duration::from_millis(*delay));
            let app = retry_window.app_handle();
            let Some(store) = app.try_state::<AuthStore>() else {
                return;
            };
            let (automatic, allow_prefill) = {
                let Ok(flow) = store.recovery.lock() else {
                    return;
                };
                if !flow.matches(&generation.to_string(), &document.to_string()) {
                    return;
                }
                (
                    flow.automatic,
                    !flow.automatic
                        || matches!(
                            flow.phase,
                            super::recovery::RecoveryPhase::Loading
                                | super::recovery::RecoveryPhase::Ready
                        ),
                )
            };
            let allows_qr_login = store
                .attempt
                .lock()
                .ok()
                .is_some_and(|attempt| attempt.allows_qr_login());
            let prefill = if allow_prefill {
                store
                    .login_prefill
                    .lock()
                    .ok()
                    .and_then(|value| value.clone())
            } else {
                None
            };
            let script = auth_login_capture_script(
                allows_qr_login,
                prefill.as_ref(),
                generation,
                document,
                automatic,
            );
            let _ = retry_window.eval(&script);
        }
    });
}

pub(super) fn show_authentication_window(
    app: &tauri::AppHandle,
    store: &AuthStore,
) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        // This marker contains a local route only, allowing memory-history
        // restoration after the round trip to the official origin.
        let _ = main.eval(
            "try { sessionStorage.setItem('szu.auth-return.v1', location.href); } catch (_) {} ",
        );
    }
    #[cfg(target_os = "android")]
    {
        let return_url = store
            .resume_url
            .lock()
            .map_err(|_| "状态锁失败")?
            .clone()
            .unwrap_or_else(|| LOCAL_ENTRY_URL.parse().expect("local URL"));
        super::android_cookies::show_authentication(return_url.as_str())?;
    }
    #[cfg(not(target_os = "android"))]
    {
        let previous = app
            .webview_windows()
            .into_iter()
            .find_map(|(label, window)| {
                (label != "main" && window.is_focused().unwrap_or(false)).then_some(label)
            });
        *store.resume_window.lock().map_err(|_| "状态锁失败")? = previous;
        let main = app.get_webview_window("main").ok_or("主窗口不存在")?;
        main.show().map_err(|_| "无法显示认证窗口")?;
        main.set_focus().map_err(|_| "无法显示认证窗口")?;
    }
    Ok(())
}

pub(super) fn start_recovery_deadline(app: &tauri::AppHandle, store: &AuthStore) {
    let Some(generation) = store
        .recovery
        .lock()
        .ok()
        .filter(|flow| flow.automatic)
        .map(|flow| flow.generation)
    else {
        return;
    };
    let app = app.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(250));
        let Some(store) = app.try_state::<AuthStore>() else {
            return;
        };
        let expired = {
            let Ok(mut flow) = store.recovery.lock() else {
                return;
            };
            if !flow.active
                || flow.generation != generation
                || flow.phase == super::recovery::RecoveryPhase::Manual
            {
                return;
            }
            flow.expire(std::time::Instant::now())
        };
        if expired {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval(format!("if (window.__SZU_AUTH_LOGIN__?.generation === {:?}) window.__SZU_AUTH_LOGIN__.stop('自动登录等待超时，请在官方页面手动继续。');", generation.to_string()));
            }
            return;
        }
    });
}

pub(crate) fn close_authentication_window(app: &tauri::AppHandle) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let generation = {
        let Ok(mut flow) = store.recovery.lock() else {
            return;
        };
        if !flow.active {
            return;
        }
        flow.invalidate();
        flow.generation
    };
    store.cancel_auth_probe();
    store.cancel_session_refresh();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("window.__SZU_AUTH_LOGIN__?.dispose()");
    }
    let app = app.clone();
    thread::spawn(move || {
        super::service::cancel_departed_authentication(app.clone(), generation);
        // A desktop close may hide the window while campus-network monitoring
        // continues. Its next opening must show local recovery UI, not stale JS.
        if let Some(store) = app.try_state::<AuthStore>() {
            let Ok(_operation) = store.session_operations.lock() else {
                return;
            };
            if store
                .recovery
                .lock()
                .ok()
                .is_some_and(|flow| !flow.active && flow.generation == generation + 1)
            {
                resume_application_window(&app, &store);
            }
        }
    });
}

fn start_auth_probe(window: &tauri::Webview, app: &tauri::AppHandle) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Some(generation) = store.try_start_auth_probe() else {
        return;
    };

    let budget = store
        .recovery
        .lock()
        .ok()
        .map(|flow| flow.probe_budget(std::time::Instant::now()))
        .unwrap_or(super::recovery::SUBMIT_TIMEOUT);
    let _ = window.eval(auth_probe_script(generation, budget, "start"));
    let handle = app.clone();
    let probe_window = window.clone();
    let saw_verified_response = Arc::new(AtomicBool::new(false));
    thread::spawn(move || {
        let deadline = std::time::Instant::now() + budget;
        let callback_pending = Arc::new(AtomicBool::new(false));
        while std::time::Instant::now() < deadline {
            thread::sleep(AUTH_PROBE_INTERVAL);
            if !handle
                .try_state::<AuthStore>()
                .is_some_and(|store| store.auth_probe_is_current(generation))
            {
                break;
            }
            let callback_handle = handle.clone();

            let verified_flag = saw_verified_response.clone();
            if callback_pending.swap(true, Ordering::AcqRel) {
                continue;
            }
            let pending = callback_pending.clone();
            let result = probe_window.eval_with_callback(
                auth_probe_script(generation, budget, "poll"),
                move |value| {
                    pending.store(false, Ordering::Release);
                    if !callback_handle
                        .try_state::<AuthStore>()
                        .is_some_and(|store| store.auth_probe_is_current(generation))
                    {
                        return;
                    }
                    if auth_probe_reached_verified_endpoint(&value) {
                        verified_flag.store(true, Ordering::Release);
                    }
                    if value != "null" && value != "undefined" {
                        let completion_handle = callback_handle.clone();
                        let payload = value.clone();
                        thread::spawn(move || {
                            complete_authentication(completion_handle, payload, generation)
                        });
                    }
                },
            );
            if result.is_err() {
                callback_pending.store(false, Ordering::Release);
            }
            if handle.try_state::<AuthStore>().is_some_and(|store| {
                !matches!(
                    store.state.lock().ok().as_deref(),
                    Some(AuthState::Authenticating)
                )
            }) {
                break;
            }
        }
        if let Some(store) = handle.try_state::<AuthStore>() {
            if store.auth_probe_is_current(generation)
                && matches!(
                    store.state.lock().ok().as_deref(),
                    Some(AuthState::Authenticating)
                )
                && saw_verified_response.load(Ordering::Acquire)
                && !store.auth_completion_is_claimed()
            {
                credential_capture_failure(&handle, &store);
            }
            store.finish_auth_probe(generation);
        }
    });
}

pub(crate) fn on_page_load(window: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
    let event = payload.event();
    let is_started = matches!(event, tauri::webview::PageLoadEvent::Started);
    let is_finished = matches!(event, tauri::webview::PageLoadEvent::Finished);
    if !is_started && !is_finished {
        return;
    }

    let url = payload.url().as_str();
    // A reload or full navigation invalidates every callback from the old document.
    if is_started {
        if let Some(store) = window.app_handle().try_state::<AuthStore>() {
            store.cancel_auth_probe();
            let cancelled = if let Ok(mut flow) = store.recovery.lock() {
                flow.document += 1;
                if flow.active && !is_auth_login_url(url) && !is_auth_profile_url(url) {
                    flow.invalidate();
                    Some(flow.generation)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(generation) = cancelled {
                let app = window.app_handle().clone();
                thread::spawn(move || {
                    super::service::cancel_departed_authentication(app, generation)
                });
            }
        }
    }
    if is_finished && is_auth_login_url(url) {
        let app = window.app_handle();
        if let Some(store) = app.try_state::<AuthStore>() {
            store.cancel_auth_probe();
        }
        if let Some(store) = app.try_state::<AuthStore>() {
            // A committed login document is a fresh form or a failed submission.
            if let Ok(mut capture) = store.captured.lock() {
                *capture = None;
            }
            if let Ok(mut submitted) = store.submitted_password.lock() {
                *submitted = None;
            }
        }
        inject_login_capture(window);
    }

    if is_finished
        && is_auth_profile_url(url)
        && window
            .app_handle()
            .try_state::<AuthStore>()
            .is_some_and(|store| {
                matches!(
                    store.state.lock().ok().as_deref(),
                    Some(AuthState::Authenticating)
                )
            })
    {
        start_auth_probe(window, &window.app_handle().clone());
    }
}
