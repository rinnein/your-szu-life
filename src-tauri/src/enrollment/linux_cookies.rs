use std::{sync::mpsc, time::Duration};
use webkit2gtk::{CookieManagerExt, WebContextExt, WebViewExt};
// Soup's canonical domain preserves the leading-dot distinction that Wry's
// generic cookie conversion loses. Return metadata, never document.cookie.
pub fn read(window: &tauri::WebviewWindow, url: &str) -> Result<Vec<(String, String)>, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    let url = url.to_string();
    window
        .with_webview(move |view| {
            let Some(manager) = view.inner().context().and_then(|c| c.cookie_manager()) else {
                let _ = tx.send(Err("Cookie服务不可用".into()));
                return;
            };
            manager.cookies(&url, None::<&webkit2gtk::gio::Cancellable>, move |result| {
                let result = result
                    .map(|cookies| {
                        cookies
                            .into_iter()
                            .filter_map(|mut c| {
                                Some((
                                    c.domain()?.to_string(),
                                    c.to_set_cookie_header()?.to_string(),
                                ))
                            })
                            .collect()
                    })
                    .map_err(|_| "无法读取认证Cookie".into());
                let _ = tx.send(result);
            });
        })
        .map_err(|_| "无法访问Cookie服务")?;
    rx.recv_timeout(Duration::from_secs(15))
        .map_err(|_| "Cookie读取超时")?
}

pub fn write(
    window: &tauri::WebviewWindow,
    changes: Vec<super::school_cookies::Patch>,
) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel(1);
    let cancel = webkit2gtk::gio::Cancellable::new();
    let task_cancel = cancel.clone();
    window
        .with_webview(move |view| {
            let Some(manager) = view.inner().context().and_then(|c| c.cookie_manager()) else {
                let _ = tx.send(Err("Cookie服务不可用".into()));
                return;
            };
            write_next(manager, task_cancel, changes.into(), tx);
        })
        .map_err(|_| "无法恢复官网Cookie")?;
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(r) => r,
        Err(_) => {
            use webkit2gtk::gio::prelude::CancellableExt;
            cancel.cancel();
            Err("官网Cookie写入超时".into())
        }
    }
}
fn write_next(
    manager: webkit2gtk::CookieManager,
    cancel: webkit2gtk::gio::Cancellable,
    mut changes: std::collections::VecDeque<super::school_cookies::Patch>,
    tx: mpsc::SyncSender<Result<(), String>>,
) {
    let Some(change) = changes.pop_front() else {
        let _ = tx.send(Ok(()));
        return;
    };
    let raw = match cookie_store::RawCookie::parse(change.record.header) {
        Ok(v) => v,
        Err(_) => {
            let _ = tx.send(Err("Cookie格式无效".into()));
            return;
        }
    };
    let domain = if change.record.key.3 {
        change.record.key.1
    } else {
        format!(".{}", change.record.key.1)
    };
    let mut cookie = soup::Cookie::new(
        raw.name(),
        raw.value(),
        &domain,
        raw.path().unwrap_or("/"),
        -1,
    );
    cookie.set_secure(raw.secure().unwrap_or(false));
    cookie.set_http_only(raw.http_only().unwrap_or(false));
    if let Some(date) = raw.expires_datetime() {
        if let Ok(date) = webkit2gtk::glib::DateTime::from_unix_utc(date.unix_timestamp()) {
            cookie.set_expires(&date);
        }
    }
    if let Some(policy) = raw.same_site() {
        cookie.set_same_site_policy(match policy {
            tauri::webview::cookie::SameSite::Strict => soup::SameSitePolicy::Strict,
            tauri::webview::cookie::SameSite::Lax => soup::SameSitePolicy::Lax,
            _ => soup::SameSitePolicy::None,
        });
    }
    let next = manager.clone();
    let next_cancel = cancel.clone();
    let callback = move |result: Result<(), webkit2gtk::glib::Error>| {
        if result.is_err() {
            let _ = tx.send(Err("官网Cookie写入失败".into()));
        } else {
            write_next(next, next_cancel, changes, tx);
        }
    };
    if change.remove {
        manager.delete_cookie(&mut cookie, Some(&cancel), callback);
    } else {
        manager.add_cookie(&mut cookie, Some(&cancel), callback);
    }
}
