use super::{
    model::AuthState,
    probe::{
        auth_probe_reached_verified_endpoint, auth_probe_succeeded, is_auth_login_url,
        is_auth_profile_url,
    },
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
const AUTH_PROBE_ATTEMPTS: usize = 40;
const AUTH_PROBE_INTERVAL: Duration = Duration::from_millis(250);

pub(super) fn resume_application_window(app: &tauri::AppHandle, store: &AuthStore) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.navigate(application_resume_url(store));
}

pub(super) fn application_resume_url(store: &AuthStore) -> tauri::Url {
    store
        .resume_url
        .lock()
        .ok()
        .and_then(|mut url| url.take())
        .unwrap_or_else(|| LOCAL_ENTRY_URL.parse().expect("valid local entry URL"))
}

fn login_capture_context(app: &tauri::AppHandle) -> (bool, Option<super::model::LoginFormPrefill>) {
    app.try_state::<AuthStore>()
        .map(|store| {
            let allows_qr_login = store
                .attempt
                .lock()
                .ok()
                .is_some_and(|attempt| attempt.allows_qr_login());
            let prefill = store
                .login_prefill
                .lock()
                .ok()
                .and_then(|value| value.clone());
            (allows_qr_login, prefill)
        })
        .unwrap_or((false, None))
}

fn inject_login_capture(
    window: &tauri::Webview,
    allows_qr_login: bool,
    prefill: Option<&super::model::LoginFormPrefill>,
) {
    let script = auth_login_capture_script(allows_qr_login, prefill);
    let _ = window.eval(&script);
    let retry_window = window.clone();
    thread::spawn(move || {
        for delay in LOGIN_CAPTURE_RETRY_DELAYS_MS {
            thread::sleep(Duration::from_millis(*delay));
            let _ = retry_window.eval(&script);
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

    let _ = window.eval(auth_probe_script());
    let handle = app.clone();
    let probe_window = window.clone();
    let saw_verified_response = Arc::new(AtomicBool::new(false));
    thread::spawn(move || {
        for _ in 0..AUTH_PROBE_ATTEMPTS {
            thread::sleep(AUTH_PROBE_INTERVAL);
            if !handle
                .try_state::<AuthStore>()
                .is_some_and(|store| store.auth_probe_is_current(generation))
            {
                break;
            }
            let callback_handle = handle.clone();
            let retry_window = probe_window.clone();
            let verified_flag = saw_verified_response.clone();
            let _ = probe_window.eval_with_callback(auth_probe_result_script(), move |value| {
                if auth_probe_reached_verified_endpoint(&value) {
                    verified_flag.store(true, Ordering::Release);
                }
                let should_retry =
                    value == "null" || value == "undefined" || !auth_probe_succeeded(&value);
                if value != "null" && value != "undefined" {
                    let completion_handle = callback_handle.clone();
                    let payload = value.clone();
                    thread::spawn(move || complete_authentication(completion_handle, payload));
                }
                if should_retry {
                    let _ = retry_window.eval(auth_probe_script());
                }
            });
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
            if matches!(
                store.state.lock().ok().as_deref(),
                Some(AuthState::Authenticating)
            ) && saw_verified_response.load(Ordering::Acquire)
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
    if is_auth_login_url(url) {
        let app = window.app_handle();
        if let Some(store) = app.try_state::<AuthStore>() {
            store.cancel_auth_probe();
        }
        let (allows_qr_login, prefill) = login_capture_context(&app);
        inject_login_capture(window, allows_qr_login, prefill.as_ref());
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
