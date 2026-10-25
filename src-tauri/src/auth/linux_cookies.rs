//! WebKitGTK's cookie API uses a canonical domain, including for host cookies.
//! Going through Wry's generic Cookie conversion loses the leading-dot flag
//! and passes an empty native domain for host-only HTTP cookies.
use super::{
    session::{cookie_in_auth_scope, StoredAuthCookie},
    AUTH_LOGIN_URL, AUTH_MOBILE_PROFILE_URL, AUTH_PROFILE_URL, AUTH_USER_CONF_URL,
};
use std::{collections::VecDeque, sync::mpsc, time::Duration};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};
use webkit2gtk::{
    gio::{self, prelude::*},
    glib, CookieManager, CookieManagerExt, WebContextExt, WebViewExt,
};

fn from_native(mut cookie: soup::Cookie) -> Option<StoredAuthCookie> {
    let domain = cookie.domain()?.to_string();
    let stored = StoredAuthCookie {
        name: cookie.name()?.to_string(),
        value: cookie.value()?.to_string(),
        host_only: !domain.starts_with('.'),
        domain: domain.trim_start_matches('.').to_owned(),
        path: cookie.path()?.to_string(),
        expires: cookie
            .expires()
            .and_then(|date| OffsetDateTime::from_unix_timestamp(date.to_unix()).ok())
            .and_then(|date| date.format(&Rfc2822).ok()),
        http_only: Some(cookie.is_http_only()),
        secure: Some(cookie.is_secure()),
        same_site: match cookie.same_site_policy() {
            soup::SameSitePolicy::Lax => Some("Lax".into()),
            soup::SameSitePolicy::Strict => Some("Strict".into()),
            soup::SameSitePolicy::None => Some("None".into()),
            _ => None,
        },
        partitioned: None,
    };
    (cookie_in_auth_scope(&stored) && stored.to_webview_cookie().is_ok()).then_some(stored)
}

fn to_native(cookie: &StoredAuthCookie) -> Result<soup::Cookie, String> {
    cookie.to_webview_cookie()?;
    let domain = if cookie.host_only {
        cookie.domain.clone()
    } else {
        format!(".{}", cookie.domain.trim_start_matches('.'))
    };
    let mut native = soup::Cookie::new(&cookie.name, &cookie.value, &domain, &cookie.path, -1);
    if let Some(expires) = &cookie.expires {
        let date =
            OffsetDateTime::parse(expires, &Rfc2822).map_err(|_| "认证 Cookie 有效期无效")?;
        native.set_expires(
            &glib::DateTime::from_unix_utc(date.unix_timestamp())
                .map_err(|_| "认证 Cookie 有效期无效")?,
        );
    }
    native.set_secure(cookie.secure.unwrap_or(false));
    native.set_http_only(cookie.http_only.unwrap_or(false));
    if let Some(policy) = &cookie.same_site {
        native.set_same_site_policy(match policy.as_str() {
            "Strict" => soup::SameSitePolicy::Strict,
            "Lax" => soup::SameSitePolicy::Lax,
            "None" => soup::SameSitePolicy::None,
            _ => return Err("认证 Cookie SameSite 无效".into()),
        });
    }
    Ok(native)
}

fn with_manager<T: Send + 'static>(
    window: &tauri::WebviewWindow,
    operation: impl FnOnce(CookieManager, gio::Cancellable, mpsc::SyncSender<Result<T, String>>)
        + Send
        + 'static,
) -> Result<T, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancel = gio::Cancellable::new();
    let task_cancel = cancel.clone();
    window
        .with_webview(move |view| {
            let Some(manager) = view
                .inner()
                .context()
                .and_then(|context| context.cookie_manager())
            else {
                let _ = sender.send(Err("WebKit Cookie 服务不可用".into()));
                return;
            };
            operation(manager, task_cancel, sender);
        })
        .map_err(|_| "无法访问 WebKit Cookie 服务")?;
    match receiver.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result,
        Err(_) => {
            cancel.cancel();
            Err("WebKit Cookie 操作超时".into())
        }
    }
}

fn read_native(
    manager: CookieManager,
    cancel: gio::Cancellable,
    index: usize,
    mut cookies: Vec<soup::Cookie>,
    done: impl FnOnce(Result<Vec<soup::Cookie>, String>) + 'static,
) {
    let urls = [
        AUTH_LOGIN_URL,
        AUTH_PROFILE_URL,
        AUTH_MOBILE_PROFILE_URL,
        AUTH_USER_CONF_URL,
    ];
    let Some(url) = urls.get(index) else {
        let mut seen = std::collections::HashSet::new();
        cookies.reverse();
        cookies.retain_mut(|cookie| {
            from_native(cookie.clone()).is_some()
                && seen.insert((
                    cookie.name().map(|value| value.to_string()),
                    cookie.domain().map(|value| value.to_string()),
                    cookie.path().map(|value| value.to_string()),
                ))
        });
        cookies.reverse();
        done(Ok(cookies));
        return;
    };
    let next_manager = manager.clone();
    let next_cancel = cancel.clone();
    manager.cookies(url, Some(&cancel), move |result| match result {
        Ok(next) => {
            cookies.extend(next);
            read_native(next_manager, next_cancel, index + 1, cookies, done);
        }
        Err(_) => done(Err("无法读取 WebKit 认证 Cookie".into())),
    });
}

pub(super) fn read(window: &tauri::WebviewWindow) -> Result<Vec<StoredAuthCookie>, String> {
    with_manager(window, |manager, cancel, sender| {
        read_native(manager, cancel, 0, Vec::new(), move |result| {
            let _ = sender.send(
                result
                    .map(|cookies| cookies.into_iter().filter_map(from_native).collect())
                    .map_err(|_| "无法读取 WebKit 认证 Cookie".into()),
            );
        });
    })
}

fn write_next(
    manager: CookieManager,
    cancel: gio::Cancellable,
    mut operations: VecDeque<(bool, soup::Cookie)>,
    sender: mpsc::SyncSender<Result<(), String>>,
) {
    if cancel.is_cancelled() {
        let _ = sender.send(Err("Cookie 操作已取消".into()));
        return;
    }
    let Some((delete, mut cookie)) = operations.pop_front() else {
        let _ = sender.send(Ok(()));
        return;
    };
    let next_manager = manager.clone();
    let next_cancel = cancel.clone();
    let callback = move |result: Result<(), glib::Error>| match result {
        Ok(()) => write_next(next_manager, next_cancel, operations, sender),
        Err(_) => {
            let _ = sender.send(Err("无法更新 WebKit 认证 Cookie".into()));
        }
    };
    if delete {
        manager.delete_cookie(&mut cookie, Some(&cancel), callback);
    } else {
        manager.add_cookie(&mut cookie, Some(&cancel), callback);
    }
}

pub(super) fn replace(
    window: &tauri::WebviewWindow,
    cookies: Vec<StoredAuthCookie>,
) -> Result<(), String> {
    with_manager(window, move |manager, cancel, sender| {
        replace_native(manager, cancel, cookies, sender)
    })
}

fn replace_native(
    manager: CookieManager,
    cancel: gio::Cancellable,
    cookies: Vec<StoredAuthCookie>,
    sender: mpsc::SyncSender<Result<(), String>>,
) {
    let additions = match cookies.iter().map(to_native).collect::<Result<Vec<_>, _>>() {
        Ok(cookies) => cookies,
        Err(error) => {
            let _ = sender.send(Err(error));
            return;
        }
    };
    let next_manager = manager.clone();
    let next_cancel = cancel.clone();
    read_native(manager, cancel, 0, Vec::new(), move |result| {
        let Ok(existing) = result else {
            let _ = sender.send(Err("无法读取 WebKit 认证 Cookie".into()));
            return;
        };
        let mut operations: VecDeque<_> = existing
            .into_iter()
            .filter(|cookie| from_native(cookie.clone()).is_some())
            .map(|cookie| (true, cookie))
            .collect();
        operations.extend(additions.into_iter().map(|cookie| (false, cookie)));
        write_next(next_manager, next_cancel, operations, sender);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait<T: 'static>(receiver: mpsc::Receiver<Result<T, String>>) -> T {
        let loop_ = glib::MainLoop::new(None, false);
        let output = std::rc::Rc::new(std::cell::RefCell::new(None));
        let result = output.clone();
        let loop_handle = loop_.clone();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        glib::timeout_add_local(Duration::from_millis(10), move || {
            match receiver.try_recv() {
                Ok(value) => {
                    *result.borrow_mut() = Some(value);
                    loop_handle.quit();
                    glib::ControlFlow::Break
                }
                Err(_) if std::time::Instant::now() >= deadline => {
                    *result.borrow_mut() = Some(Err("isolated native test timed out".into()));
                    loop_handle.quit();
                    glib::ControlFlow::Break
                }
                _ => glib::ControlFlow::Continue,
            }
        });
        loop_.run();
        let value = output.borrow_mut().take().expect("native callback missing");
        value.expect("isolated native operation failed")
    }

    fn replace_in(manager: &CookieManager, cookies: Vec<StoredAuthCookie>) {
        let (sender, receiver) = mpsc::sync_channel(1);
        replace_native(manager.clone(), gio::Cancellable::new(), cookies, sender);
        wait(receiver);
    }

    fn read_in(manager: &CookieManager) -> Vec<StoredAuthCookie> {
        let (sender, receiver) = mpsc::sync_channel(1);
        read_native(
            manager.clone(),
            gio::Cancellable::new(),
            0,
            Vec::new(),
            move |result| {
                let _ = sender.send(
                    result.map(|cookies| cookies.into_iter().filter_map(from_native).collect()),
                );
            },
        );
        wait(receiver)
    }

    #[test]
    #[ignore = "Requires an isolated DISPLAY; optional live session read requires explicit authorization"]
    fn ephemeral_webkit_restores_cookies_across_two_contexts() {
        use gtk::prelude::*;
        assert!(gtk::init().is_ok(), "isolated display unavailable");
        let context = webkit2gtk::WebContext::new_ephemeral();
        let manager = context.cookie_manager().unwrap();
        let fixture = from_native(soup::Cookie::new(
            "host-fixture",
            "test-only",
            super::super::AUTH_HOST,
            "/personalInfo",
            -1,
        ))
        .unwrap();
        let domain = from_native(soup::Cookie::new(
            "domain-fixture",
            "test-only",
            ".szu.edu.cn",
            "/",
            3600,
        ))
        .unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut unrelated = soup::Cookie::new("unrelated", "test-only", "www.uooc.net.cn", "/", -1);
        manager.add_cookie(&mut unrelated, None::<&gio::Cancellable>, move |result| {
            let _ = sender.send(result.map_err(|_| "fixture setup failed".into()));
        });
        wait(receiver);
        replace_in(&manager, vec![fixture.clone(), domain.clone()]);
        let snapshot = read_in(&manager);
        assert!(snapshot
            .iter()
            .any(|cookie| cookie.name == fixture.name && cookie.host_only));
        assert!(snapshot
            .iter()
            .any(|cookie| cookie.name == domain.name && !cookie.host_only));
        replace_in(&manager, Vec::new());
        assert!(read_in(&manager).is_empty());
        let (sender, receiver) = mpsc::sync_channel(1);
        manager.cookies(
            "https://www.uooc.net.cn/",
            None::<&gio::Cancellable>,
            move |result| {
                let _ = sender.send(
                    result
                        .map(|cookies| cookies.len())
                        .map_err(|_| "fixture read failed".into()),
                );
            },
        );
        assert_eq!(
            wait(receiver),
            1,
            "auth cleanup erased an unrelated session"
        );
        let restored_context = webkit2gtk::WebContext::new_ephemeral();
        let restored_manager = restored_context.cookie_manager().unwrap();
        replace_in(&restored_manager, snapshot);
        assert_eq!(read_in(&restored_manager).len(), 2);
        eprintln!("Native cookie round trip and scoped cleanup: passed");

        if std::env::var("SZU_READ_SAVED_SESSION").as_deref() != Ok("1") {
            return;
        }
        let saved =
            crate::storage::read_saved_session_for_diagnostic().expect("saved session unavailable");
        let mut saved = saved
            .restorable_snapshot(&saved.student_id, OffsetDateTime::now_utc())
            .expect("session metadata invalid")
            .expect("session expired");
        for _ in 0..2 {
            let context = webkit2gtk::WebContext::new_ephemeral();
            let manager = context.cookie_manager().unwrap();
            replace_in(&manager, saved.cookies.clone());
            let view = webkit2gtk::WebView::with_context(&context);
            let window = gtk::OffscreenWindow::new();
            window.add(&view);
            window.show_all();
            let (sender, receiver) = mpsc::sync_channel(1);
            let failure = sender.clone();
            view.connect_title_notify(move |view| {
                if view.title().as_deref() == Some("__szu_test_verified__") {
                    let _ = sender.try_send(Ok(true));
                }
                if view.title().as_deref() == Some("__szu_test_rejected__") {
                    let _ = sender.try_send(Ok(false));
                }
            });
            let expected = serde_json::to_string(&saved.student_id).unwrap();
            view.connect_load_changed(move |view, event| {
                if event != webkit2gtk::LoadEvent::Finished { return; }
                let url = view.uri().unwrap_or_default();
                if super::super::probe::is_auth_login_url(&url) { let _ = failure.try_send(Ok(false)); return; }
                if !super::super::probe::is_auth_profile_url(&url) { return; }
                let script = super::super::scripts::auth_probe_script(1, Duration::from_secs(10), "start");
                let check = format!(r#"{script}; (() => {{const t=setInterval(()=>{{const r=window.__SZU_AUTH_PROBE_STATE__?.result;if(!r)return;clearInterval(t);let ok=false;try{{const j=JSON.parse(r.text);ok=r.ok&&(j.datas?.uid||j.data?.uid)==={expected}}}catch{{}}document.title=ok?'__szu_test_verified__':'__szu_test_rejected__'}},100)}})()"#);
                view.evaluate_javascript(&check, None, None, None::<&gio::Cancellable>, |_| {});
            });
            view.load_uri(AUTH_PROFILE_URL);
            assert!(
                wait(receiver),
                "saved session did not reach the verified profile without login"
            );
            saved.cookies = read_in(&manager);
            saved.format_version = 3;
            window.close();
        }
        eprintln!("Saved session accepted in two independent WebKit contexts; no password submitted or keyring written");
    }

    #[test]
    fn native_cookie_round_trip_preserves_host_domain_and_expiry() {
        for domain in [
            "authserver.szu.edu.cn",
            ".authserver.szu.edu.cn",
            ".szu.edu.cn",
        ] {
            let mut native = soup::Cookie::new("sid", "test-only", domain, "/personalInfo", 3600);
            native.set_http_only(true);
            native.set_secure(true);
            let stored = from_native(native).unwrap();
            assert_eq!(stored.host_only, !domain.starts_with('.'));
            let mut restored = to_native(&stored).unwrap();
            assert_eq!(restored.domain().as_deref(), Some(domain));
            assert!(restored.is_http_only() && restored.is_secure());
            assert!(restored.expires().is_some());
            assert_eq!(from_native(restored).unwrap(), stored);
        }
    }
}
