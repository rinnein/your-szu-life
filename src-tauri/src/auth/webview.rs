use super::{
    model::{AuthAttempt, AuthState},
    probe::{is_auth_login_url, is_authserver_url},
    scripts::*,
    service::{complete_authentication, session_restore_expired},
    AuthStore,
};
use std::{thread, time::Duration};
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "android"))]
const LOCAL_ENTRY_URL: &str = "http://tauri.localhost/";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
const LOCAL_ENTRY_URL: &str = "tauri://localhost/";

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

pub(crate) fn on_page_load(window: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
    if !matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
        return;
    }
    if is_auth_login_url(payload.url().as_str()) {
        let app = window.app_handle();
        let restoring_session = app.try_state::<AuthStore>().is_some_and(|store| {
            matches!(
                store.attempt.lock().ok().as_deref(),
                Some(AuthAttempt::SessionRestore { .. })
            )
        });
        if restoring_session {
            let handle = app.clone();
            thread::spawn(move || {
                if let Some(store) = handle.try_state::<AuthStore>() {
                    session_restore_expired(&handle, &store);
                }
            });
            return;
        }
        let (allows_qr_login, prefill) = app
            .try_state::<AuthStore>()
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
            .unwrap_or((false, None));
        let _ = window.eval(&auth_login_capture_script(
            allows_qr_login,
            prefill.as_ref(),
        ));
    }
    if is_authserver_url(payload.url().as_str())
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
        let handle = window.app_handle().clone();
        let _ = window.eval(auth_probe_script());
        let probe_window = window.clone();
        thread::spawn(move || {
            for _ in 0..20 {
                thread::sleep(Duration::from_millis(250));
                let callback_handle = handle.clone();
                let _ = probe_window.eval_with_callback(auth_probe_result_script(), move |value| {
                    if value != "null" && value != "undefined" {
                        let completion_handle = callback_handle.clone();
                        thread::spawn(move || complete_authentication(completion_handle, value));
                    }
                });
            }
        });
    }
}
