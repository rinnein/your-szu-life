use super::{
    model::{parse_observation, NoticeCode, PageKind, PageObservation},
    scripts::{cancel_script, page_script},
    state::{Action, CampusPaymentsStore, Phase, MANUAL_MESSAGE},
    urls::{application_return_url, is_login_url, same_application_origin},
    PORTAL_URL,
};
use crate::auth;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri::{Manager, Url, WebviewWindow};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const CALLBACK_TIMEOUT: Duration = Duration::from_millis(750);

pub(super) fn local_return_url(window: &WebviewWindow) -> Result<Url, String> {
    if window.label() != "main" {
        return Err("请从应用内的校园卡/电费/网费页面打开网费服务。".into());
    }
    let current = window.url().map_err(|_| "无法确认当前应用页面")?;
    let dev_url = if cfg!(debug_assertions) {
        window.app_handle().config().build.dev_url.as_ref()
    } else {
        None
    };
    application_return_url(&current, dev_url)
        .ok_or_else(|| "请从应用内的校园卡/电费/网费页面打开网费服务。".into())
}

pub(super) fn open(window: WebviewWindow) -> Result<(), String> {
    let return_url = local_return_url(&window)?;
    let app = window.app_handle();
    let account = auth::authenticated_campus_card_number(app)?;
    let store = app
        .try_state::<CampusPaymentsStore>()
        .ok_or("网费服务状态不可用")?;
    // Cancellation invalidates runtime state immediately, but a new opener
    // must not overlap an older, still-running native cookie cleanup.
    let opening = store
        .opening
        .try_lock()
        .map_err(|_| "网费官网正在打开，请勿重复操作。")?;
    let (generation, changed_account) =
        store.reserve(account.clone(), return_url.clone(), Instant::now())?;
    let prepare = || -> Result<(), String> {
        if changed_account {
            // Cookie enumeration is unsupported on Android. This is the same
            // cross-platform cleanup used by auth, without deleting its keyring.
            window
                .clear_all_browsing_data()
                .map_err(|_| "无法清理上一个网费账户的网页会话，请重试。")?;
        }
        if auth::authenticated_campus_card_number(app)? != account {
            return Err("当前账户已改变，请重新打开网费服务。".into());
        }
        if local_return_url(&window)? != return_url {
            return Err("当前页面已改变，请重新打开网费服务。".into());
        }
        store.prepare_navigation(generation, Instant::now())
    };
    if let Err(message) = prepare() {
        store.cancel_generation(generation);
        return Err(message);
    }
    // Keep the generation check and native dispatch in the same critical
    // section. Cancellation cannot slip between them and let an old opener
    // navigate over a newer authentication/account flow.
    let navigation = (|| -> Result<(), String> {
        let runtime = store.runtime.lock().map_err(|_| "网费服务状态不可用")?;
        if runtime.active.as_ref().map(|session| session.generation) != Some(generation) {
            return Err("当前网费任务已取消，请重新打开。".into());
        }
        if auth::authenticated_campus_card_number(app)? != account
            || local_return_url(&window)? != return_url
        {
            return Err("当前账户或页面已改变，请重新打开网费服务。".into());
        }
        window
            .navigate(PORTAL_URL.parse().expect("fixed portal URL"))
            .map_err(|_| "无法打开网费官网，请检查网络后重试。".into())
    })();
    if let Err(message) = navigation {
        store.fail(generation, NoticeCode::NavigationFailed);
        return Err(message);
    }
    drop(opening);
    thread::spawn(move || run(window, generation));
    Ok(())
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
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(cancel_script(session.generation));
    }
    Some(session.return_url)
}

/// Queue under the runtime lock, but never hold it while waiting for a native
/// callback. Only synchronous page-check JSON is evaluated, not a Promise.
fn start_evaluation(window: &WebviewWindow, script: String) -> Option<mpsc::Receiver<String>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .eval_with_callback(script, move |value| {
            let _ = sender.try_send(value);
        })
        .ok()?;
    Some(receiver)
}

fn finish_evaluation(receiver: Option<mpsc::Receiver<String>>) -> Option<PageObservation> {
    let value = receiver?.recv_timeout(CALLBACK_TIMEOUT).ok()?;
    parse_observation(&value)
}

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
    if window.label() != "main" {
        return false;
    }
    let Some(store) = window.app_handle().try_state::<CampusPaymentsStore>() else {
        return false;
    };
    let Ok(mut runtime) = store.runtime.lock() else {
        return false;
    };
    let Some(session) = runtime.active.as_mut() else {
        return false;
    };
    // Some engines also report subframe loads. Only the current main URL counts.
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
    let _ = window.eval(page_script(
        session,
        "probe",
        None,
        None,
        Some(payload.url().as_str()),
    ));
    true
}
