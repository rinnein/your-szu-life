#[cfg(not(target_os = "android"))]
use super::{model::PageKind, urls::is_login_url};
use super::{
    model::{parse_observation, NetworkFeePortalNotice, NoticeCode, PageObservation},
    scripts::{cancel_script, page_script},
    state::{Action, CampusPaymentsStore, Phase, MANUAL_MESSAGE},
    urls::same_application_origin,
    PORTAL_URL,
};
use crate::auth;
use std::time::Instant;
#[cfg(not(target_os = "android"))]
use std::{sync::mpsc, thread, time::Duration};
#[cfg(target_os = "android")]
use tauri::Webview;
use tauri::{Manager, Url, WebviewWindow};

#[cfg(not(target_os = "android"))]
const POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(not(target_os = "android"))]
const CALLBACK_TIMEOUT: Duration = Duration::from_millis(750);

pub(super) fn local_return_url(window: &WebviewWindow) -> Result<Url, String> {
    if window.label() != "main" && !window.label().starts_with("feature-") {
        return Err("请从应用内的校园卡/电费/网费页面打开网费服务。".into());
    }
    let mut current = window.url().map_err(|_| "无法确认当前应用页面")?;
    current.set_query(None);
    current.set_fragment(None);
    Ok(current)
}

pub(super) fn open(parent: WebviewWindow) -> Result<(), String> {
    let return_url = local_return_url(&parent)?;
    let app = parent.app_handle();
    let account = auth::authenticated_campus_card_number(app)?;
    let store = app
        .try_state::<CampusPaymentsStore>()
        .ok_or("网费服务状态不可用")?;
    let _opening = store
        .opening
        .try_lock()
        .map_err(|_| "网费官网正在打开，请勿重复操作。")?;
    let (generation, changed_account) =
        store.reserve(account.clone(), return_url.clone(), Instant::now())?;
    let label = format!("network-fee-{generation}");
    {
        let mut runtime = store.runtime.lock().map_err(|_| "网费服务状态不可用")?;
        let session = runtime.active.as_mut().ok_or("网费服务已取消")?;
        session.window_label = label.clone();
        session.parent_label = parent.label().to_owned();
    }
    let result = (|| -> Result<(), String> {
        let return_app = app.clone();
        let return_label = label.clone();
        let expected_return = return_url.clone();
        let builder = tauri::WebviewWindowBuilder::new(
            app,
            &label,
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .title("官方网费服务")
        .on_navigation(move |url| {
            if url == &expected_return {
                let app = return_app.clone();
                let label = return_label.clone();
                // Never re-enter Android's synchronous WebView API on its UI callback.
                std::thread::spawn(move || close_portal(&app, &label));
                return false;
            }
            true
        });
        #[cfg(target_os = "android")]
        let builder = builder
            .activity_name("FeatureActivity")
            .created_by_activity_name(parent.activity_name().map_err(|_| "无法确认父窗口")?);
        let window = builder.build().map_err(|_| "无法创建网费服务窗口")?;
        if changed_account {
            window
                .clear_all_browsing_data()
                .map_err(|_| "无法清理上一个网费账户的网页会话，请重试。")?;
        }
        if auth::authenticated_campus_card_number(app)? != account
            || local_return_url(&parent)? != return_url
        {
            return Err("当前账户或页面已改变，请重新打开网费服务。".into());
        }
        store.prepare_navigation(generation, Instant::now())?;
        {
            let runtime = store.runtime.lock().map_err(|_| "网费服务状态不可用")?;
            if runtime.active.as_ref().map(|s| s.generation) != Some(generation) {
                return Err("当前网费任务已取消，请重新打开。".into());
            }
            window
                .navigate(PORTAL_URL.parse().expect("fixed portal URL"))
                .map_err(|_| NetworkFeePortalNotice::from(NoticeCode::NavigationFailed).message)?;
        }
        #[cfg(not(target_os = "android"))]
        thread::spawn(move || run(window, generation));
        Ok(())
    })();
    if result.is_err() {
        cancel_matching(app, Some(generation));
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    }
    result
}

/// A portal's return link and the system closing its Activity both remove the
/// same session. The originating feature window is never navigated or closed.
fn close_portal(app: &tauri::AppHandle, label: &str) {
    window_closed(app, label);
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.destroy();
    }
}

pub(crate) fn window_closed(app: &tauri::AppHandle, label: &str) {
    let Some(store) = app.try_state::<CampusPaymentsStore>() else {
        return;
    };
    let session = {
        let Ok(mut runtime) = store.runtime.lock() else {
            return;
        };
        if !runtime
            .active
            .as_ref()
            .is_some_and(|s| s.window_label == label || s.parent_label == label)
        {
            return;
        }
        runtime.active.take()
    };
    if let Some(session) = session {
        if session.window_label != label {
            if let Some(window) = app.get_webview_window(&session.window_label) {
                let _ = window.destroy();
            }
        } else if let Some(parent) = app.get_webview_window(&session.parent_label) {
            let _ = parent.set_focus();
        }
    }
}

/// Cancels only this domain, without changing unified-auth state or its return
/// address. Auth callers may use the returned, previously validated local URL.
pub(crate) fn cancel(app: &tauri::AppHandle) -> Option<Url> {
    cancel_matching(app, None)
}

fn cancel_matching(app: &tauri::AppHandle, expected_generation: Option<u64>) -> Option<Url> {
    let store = app.try_state::<CampusPaymentsStore>()?;
    let mut runtime = store.runtime.lock().ok()?;
    if let Some(expected) = expected_generation {
        if runtime.active.as_ref().map(|session| session.generation) != Some(expected) {
            return None;
        }
    }
    runtime.notice = None;
    let session = runtime.active.take()?;
    // Dispatch invalidation while holding the same lock as script dispatch.
    // A callback which observes cancellation can no longer queue credentials.
    drop(runtime);
    if let Some(window) = app.get_webview_window(&session.window_label) {
        let _ = window.eval(cancel_script(session.generation));
        if session.window_label != session.parent_label {
            let _ = window.destroy();
        }
    }
    Some(session.return_url)
}

/// Queue under the runtime lock, but never hold it while waiting for a native
/// callback. Only synchronous page-check JSON is evaluated, not a Promise.
#[cfg(not(target_os = "android"))]
fn start_evaluation(window: &WebviewWindow, script: String) -> Option<mpsc::Receiver<String>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .eval_with_callback(script, move |value| {
            let _ = sender.try_send(value);
        })
        .ok()?;
    Some(receiver)
}

#[cfg(not(target_os = "android"))]
fn finish_evaluation(receiver: Option<mpsc::Receiver<String>>) -> Option<PageObservation> {
    let value = receiver?.recv_timeout(CALLBACK_TIMEOUT).ok()?;
    parse_observation(&value)
}

#[cfg(not(target_os = "android"))]
fn fail(window: &WebviewWindow, generation: u64, code: NoticeCode) {
    let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
        return;
    };
    let receiver = {
        let Ok(mut runtime) = store.runtime.lock() else {
            return;
        };
        let Some(session) = runtime
            .active
            .as_mut()
            .filter(|session| session.generation == generation && !session.is_manual())
        else {
            return;
        };
        let script = page_script(session, "return", None, None, None);
        // Stop all worker deadlines immediately, including if this callback is
        // lost. Keep domain ownership until local navigation actually happens.
        let notice = super::model::NetworkFeePortalNotice::from(code);
        session.hand_off(notice.message);
        runtime.notice = Some(notice);
        start_evaluation(window, script)
    };
    if let Some(page) = finish_evaluation(receiver) {
        if matches!(page.kind, PageKind::Manual | PageKind::Stale) {
            if let Ok(mut runtime) = store.runtime.lock() {
                // An old callback must not erase a new task's failure notice.
                if runtime.generation == generation {
                    runtime.notice = None;
                    if let Some(session) = runtime.active.as_mut() {
                        session.hand_off(MANUAL_MESSAGE);
                    }
                }
            }
        }
    }
    // Never follow a failed/late probe with an unconditional native navigate:
    // the user may have clicked Manual/Return before that callback arrived.
    // The credential-free return script checks those flags synchronously.
}

#[cfg(not(target_os = "android"))]
fn check_timeout(window: &WebviewWindow, generation: u64) {
    let code = window
        .app_handle()
        .try_state::<CampusPaymentsStore>()
        .and_then(|store| {
            let runtime = store.runtime.lock().ok()?;
            let session = runtime
                .active
                .as_ref()
                .filter(|session| session.generation == generation)?;
            session.timeout(Instant::now())
        });
    if let Some(code) = code {
        fail(window, generation, code);
    }
}

#[cfg(not(target_os = "android"))]
fn run(window: WebviewWindow, generation: u64) {
    loop {
        let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
            return;
        };
        let snapshot = store.runtime.lock().ok().and_then(|runtime| {
            let session = runtime
                .active
                .as_ref()
                .filter(|session| session.generation == generation)?;
            (!session.is_manual()).then(|| (session.account.clone(), session.return_url.clone()))
        });
        let Some((account, return_url)) = snapshot else {
            return;
        };
        if auth::authenticated_campus_card_number(window.app_handle())
            .ok()
            .as_deref()
            != Some(&account)
        {
            cancel_matching(window.app_handle(), Some(generation));
            return;
        }
        let Ok(url) = window.url() else {
            check_timeout(&window, generation);
            thread::sleep(POLL_INTERVAL);
            continue;
        };
        if same_application_origin(&url, &return_url) {
            let finished = store.runtime.lock().ok().is_some_and(|mut runtime| {
                if runtime.finish_local_navigation(&url, generation) {
                    let _ = window.eval(cancel_script(generation));
                    true
                } else {
                    false
                }
            });
            if finished {
                return;
            }
            check_timeout(&window, generation);
            thread::sleep(POLL_INTERVAL);
            continue;
        }
        let receiver = store.runtime.lock().ok().and_then(|mut runtime| {
            let session = runtime
                .active
                .as_mut()
                .filter(|session| session.generation == generation)?;
            session.seen_remote = true;
            start_evaluation(
                &window,
                page_script(session, "probe", None, None, Some(url.as_str())),
            )
        });
        if let Some(page) = finish_evaluation(receiver) {
            // A callback from the previous navigation is never evidence about
            // the current main document. JS checks its per-document token too.
            if window.url().ok().as_ref() == Some(&url) {
                let action = store.runtime.lock().ok().and_then(|mut runtime| {
                    let session = runtime
                        .active
                        .as_mut()
                        .filter(|session| session.generation == generation)?;
                    Some(session.observe(&page, &url, Instant::now()))
                });
                let Some(action) = action else {
                    return;
                };
                if !apply_action(&window, generation, &url, action) {
                    return;
                }
            }
        }
        // Redirects can make every callback refer to a previous document. They
        // must not keep the initial/login deadline alive indefinitely either.
        check_timeout(&window, generation);
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(not(target_os = "android"))]
fn apply_action(window: &WebviewWindow, generation: u64, url: &Url, action: Action) -> bool {
    let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
        return false;
    };
    match action {
        Action::Wait => true,
        Action::Fail(code) => {
            fail(window, generation, code);
            false
        }
        Action::HandOff => {
            if let Ok(runtime) = store.runtime.lock() {
                if let Some(session) = runtime
                    .active
                    .as_ref()
                    .filter(|session| session.generation == generation)
                {
                    let _ = window.eval(page_script(session, "toolbar", None, None, None));
                }
            }
            false
        }
        Action::Submit(document_token) => {
            if !is_login_url(url) || window.url().ok().as_ref() != Some(url) {
                return true;
            }
            let credentials = auth::current_network_credentials(window.app_handle());
            let Ok((account, password)) = credentials else {
                fail(window, generation, NoticeCode::CredentialsUnavailable);
                return false;
            };
            if password.is_empty()
                || auth::authenticated_campus_card_number(window.app_handle())
                    .ok()
                    .as_deref()
                    != Some(&account)
            {
                fail(window, generation, NoticeCode::CredentialsUnavailable);
                return false;
            }
            let receiver = store.runtime.lock().ok().and_then(|runtime| {
                let session = runtime.active.as_ref().filter(|session| {
                    session.generation == generation
                        && session.account == account
                        && session.phase == Phase::Submitting
                })?;
                if window.url().ok().as_ref() != Some(url) {
                    return None;
                }
                start_evaluation(
                    window,
                    page_script(
                        session,
                        "submit",
                        Some(&document_token),
                        Some((&account, &password)),
                        None,
                    ),
                )
            });
            // The callback carries only a safe enum and document token. The
            // reservation in Session survives an absent/lost callback.
            if let Some(page) = finish_evaluation(receiver) {
                if window.url().ok().as_ref() == Some(url) {
                    let action = store.runtime.lock().ok().and_then(|mut runtime| {
                        let session = runtime
                            .active
                            .as_mut()
                            .filter(|session| session.generation == generation)?;
                        Some(session.observe(&page, url, Instant::now()))
                    });
                    if let Some(action) = action {
                        return apply_action(window, generation, url, action);
                    }
                }
            }
            true
        }
        Action::RestoreTarget(document_token) => {
            if window.url().ok().as_ref() != Some(url) {
                return true;
            }
            if let Ok(runtime) = store.runtime.lock() {
                if let Some(session) = runtime
                    .active
                    .as_ref()
                    .filter(|session| session.generation == generation)
                {
                    // Navigate in the inspected document, so a synchronous
                    // Return/manual click can invalidate even a queued restore.
                    let _ = window.eval(page_script(
                        session,
                        "restore",
                        Some(&document_token),
                        None,
                        None,
                    ));
                }
            }
            true
        }
    }
}

/// Returns true while this flow owns the main WebView, so an official redirect
/// cannot accidentally start the unrelated unified-auth credential capture.
pub(crate) fn on_page_load(
    window: &tauri::Webview,
    payload: &tauri::webview::PageLoadPayload<'_>,
) -> bool {
    let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
        return false;
    };
    let Ok(mut runtime) = store.runtime.lock() else {
        return false;
    };
    let Some(session) = runtime.active.as_mut() else {
        return false;
    };
    if window.label() != session.window_label {
        return false;
    }
    // Android delivers this hook from WebViewClient.onPageStarted/onPageFinished
    // on the WebView main thread. Calling the synchronous `window.url()` getter
    // here re-enters wry's ForeignLooper and aborts the process. The Android
    // callback URL is already the current main-document URL; keep the getter
    // check on desktop where it is safe and useful for filtering subframes.
    #[cfg(not(target_os = "android"))]
    if window.url().ok().as_ref() != Some(payload.url()) {
        return true;
    }
    if same_application_origin(payload.url(), &session.return_url) {
        let generation = session.generation;
        if runtime.finish_local_navigation(payload.url(), generation) {
            let _ = window.eval(cancel_script(generation));
        }
        return false;
    }
    session.seen_remote = true;
    if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
        session.observe_location(payload.url(), Instant::now());
    }
    // Mount the credential-free mask/return controls at the first matching
    // document, without waiting for all resources. Provisional documents are
    // rejected by expected_url; bounded polling still owns all login actions.
    let script = page_script(session, "probe", None, None, Some(payload.url().as_str()));
    #[cfg(target_os = "android")]
    {
        let generation = session.generation;
        let url = payload.url().clone();
        let probe_window = window.clone();
        drop(runtime);
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
            let _ = window.eval_with_callback(script, move |value| {
                if let Some(page) = parse_observation(&value) {
                    android_observe(&probe_window, generation, url.clone(), page);
                }
            });
        } else {
            let _ = window.eval(script);
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = window.eval(script);
    true
}

#[cfg(target_os = "android")]
fn android_observe(window: &Webview, generation: u64, url: Url, page: PageObservation) {
    let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
        return;
    };
    let action = store.runtime.lock().ok().and_then(|mut runtime| {
        let session = runtime
            .active
            .as_mut()
            .filter(|session| session.generation == generation)?;
        Some(session.observe(&page, &url, Instant::now()))
    });
    if let Some(action) = action {
        android_apply_action(window, generation, url, action);
    }
}

#[cfg(target_os = "android")]
fn android_apply_action(window: &Webview, generation: u64, url: Url, action: Action) {
    let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
        return;
    };
    match action {
        Action::Wait => {}
        Action::HandOff => {
            if let Ok(runtime) = store.runtime.lock() {
                if let Some(session) = runtime
                    .active
                    .as_ref()
                    .filter(|session| session.generation == generation)
                {
                    let _ = window.eval(page_script(session, "toolbar", None, None, None));
                }
            }
        }
        Action::Fail(code) => {
            if let Ok(mut runtime) = store.runtime.lock() {
                let script = if let Some(session) = runtime
                    .active
                    .as_mut()
                    .filter(|session| session.generation == generation)
                {
                    session.hand_off(MANUAL_MESSAGE);
                    Some(page_script(session, "toolbar", None, None, None))
                } else {
                    None
                };
                if let Some(script) = script {
                    runtime.notice = Some(super::model::NetworkFeePortalNotice::from(code));
                    let _ = window.eval(script);
                }
            }
        }
        Action::RestoreTarget(document_token) => {
            if let Ok(runtime) = store.runtime.lock() {
                if let Some(session) = runtime
                    .active
                    .as_ref()
                    .filter(|session| session.generation == generation)
                {
                    let _ = window.eval(page_script(
                        session,
                        "restore",
                        Some(&document_token),
                        None,
                        None,
                    ));
                }
            }
        }
        Action::Submit(document_token) => {
            let Ok((account, password)) = auth::current_network_credentials(window.app_handle())
            else {
                android_apply_action(
                    window,
                    generation,
                    url,
                    Action::Fail(NoticeCode::CredentialsUnavailable),
                );
                return;
            };
            if password.is_empty()
                || auth::authenticated_campus_card_number(window.app_handle())
                    .ok()
                    .as_deref()
                    != Some(&account)
            {
                android_apply_action(
                    window,
                    generation,
                    url,
                    Action::Fail(NoticeCode::CredentialsUnavailable),
                );
                return;
            }
            let script = store.runtime.lock().ok().and_then(|runtime| {
                let session = runtime.active.as_ref().filter(|session| {
                    session.generation == generation && session.phase == Phase::Submitting
                })?;
                Some(page_script(
                    session,
                    "submit",
                    Some(&document_token),
                    Some((&account, &password)),
                    None,
                ))
            });
            let Some(script) = script else { return };
            let submit_window = window.clone();
            let submit_url = url.clone();
            let _ = window.eval_with_callback(script, move |value| {
                if let Some(page) = parse_observation(&value) {
                    android_observe(&submit_window, generation, submit_url.clone(), page);
                }
            });
        }
    }
}
