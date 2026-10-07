use super::{session, Service};
use std::{sync::Arc, time::Duration};
use tauri::Manager;
pub async fn open(
    service: Arc<Service>,
    parent: tauri::WebviewWindow,
    kind: String,
    target: Option<String>,
) -> Result<(), String> {
    let vpn = match kind.as_str() {
        "webvpn" => true,
        "academic" => false,
        _ => return Err("认证类型无效".into()),
    };
    let student = service.identity().map_err(|e| e.message)?.student_id;
    let return_external = target.is_some();
    let target = target.unwrap_or_else(|| {
        "https://bkxk.webvpn.szu.edu.cn/xsxkapp/sys/xsxkapp/*default/index.do".into()
    });
    let url = if vpn {
        session::auth_url(&target).map_err(|e| e.message)?
    } else {
        session::EHALL.parse().unwrap()
    };
    {
        let attempt = service.login_attempt.lock().unwrap();
        if let Some(label) = attempt.as_ref() {
            if let Some(w) = parent.app_handle().get_webview_window(label) {
                let _ = w.set_focus();
                return Ok(());
            }
        }
    }
    let label = format!("academic-auth-{}", uuid::Uuid::new_v4());
    let builder = tauri::WebviewWindowBuilder::new(
        parent.app_handle(),
        &label,
        tauri::WebviewUrl::External(url),
    )
    .initialization_script(include_str!("academic-page.js"))
    .title(if vpn {
        "WebVPN 高风险操作认证"
    } else {
        "学业完成查询认证"
    });
    #[cfg(target_os = "android")]
    let builder = builder
        .activity_name("FeatureActivity")
        .created_by_activity_name(parent.activity_name().map_err(|_| "无法确认父窗口")?);
    let window = builder.build().map_err(|_| "无法打开认证窗口")?;
    *service.login_attempt.lock().unwrap() = Some(label.clone());
    service.message("请在官方窗口完成认证；成功后自动返回");
    tauri::async_runtime::spawn(async move {
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        while std::time::Instant::now() < deadline
            && service.app.get_webview_window(&label).is_some()
        {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let s = service.clone();
            let w = window.clone();
            let expected = student.clone();
            let result = tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
                let urls = if vpn {
                    vec![session::VPN, "https://bkxk.webvpn.szu.edu.cn/"]
                } else {
                    vec![session::EHALL, "https://ehall.szu.edu.cn/"]
                };
                let raw = session::capture(&w, &urls, vpn).map_err(|e| e.message)?;
                let context = if vpn { None } else { academic_context(&w) };
                let verified = if vpn {
                    session::gateway_valid(&raw).map_err(|e| e.message)?
                } else {
                    super::academic::Academic::new(&raw)
                        .and_then(|a| a.with_context(context.clone()).verify(&expected))
                        .is_ok()
                };
                if !verified {
                    return Ok(false);
                }
                let _operation = s.operation.lock().unwrap();
                if s.identity().map_err(|e| e.message)?.student_id != expected {
                    return Err("账户已切换，取消认证".into());
                }
                if vpn {
                    let profile = s.identity().map_err(|e| e.message)?;
                    if !session::matches_account(&raw, &expected, &profile.campus_card_number)
                        .map_err(|e| e.message)?
                    {
                        return Err("WebVPN账户与当前应用账户不一致，请切换到当前账户认证".into());
                    }
                }
                let mut secrets = s.secrets.lock().unwrap();
                if vpn {
                    secrets.vpn = raw;
                } else {
                    secrets.ehall = raw;
                    secrets.academic_context = context;
                }
                s.credentials.save_academic_sessions(
                    &serde_json::to_string(&*secrets).map_err(|_| "无法保存认证")?,
                )?;
                Ok(true)
            })
            .await;
            if let Ok(Err(message)) = &result {
                service.message(message);
            }
            if matches!(result, Ok(Ok(true))) {
                service.message("认证成功");
                let _ = window.destroy();
                if vpn && return_external {
                    if let Ok(url) = target.parse() {
                        let _ = parent.navigate(url);
                    }
                }
                let _ = parent.set_focus();
                if vpn
                    && service
                        .resume_after_auth
                        .load(std::sync::atomic::Ordering::SeqCst)
                {
                    let s = service.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        s.execute(super::commands::Action::Login)
                    })
                    .await;
                }
                break;
            }
        }
        let mut attempt = service.login_attempt.lock().unwrap();
        if attempt.as_deref() == Some(&label) {
            *attempt = None;
        }
    });
    Ok(())
}

pub(crate) fn on_page_load(window: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
    if window.label().starts_with("academic-auth-")
        || !matches!(payload.event(), tauri::webview::PageLoadEvent::Started)
        || !session::vpn_url(payload.url())
    {
        return;
    }
    let Some(store) = window.app_handle().try_state::<super::Store>() else {
        return;
    };
    let Some(parent) = window.app_handle().get_webview_window(window.label()) else {
        return;
    };
    let service = store.0.clone();
    let target = payload.url().to_string();
    tauri::async_runtime::spawn(async move {
        let s = service.clone();
        let valid = tauri::async_runtime::spawn_blocking(move || {
            session::gateway_valid(&s.secrets.lock().unwrap().vpn)
        })
        .await;
        if matches!(valid, Ok(Ok(false))) {
            let _ = open(service, parent, "webvpn".into(), Some(target)).await;
        }
    });
}

fn academic_context(window: &tauri::WebviewWindow) -> Option<serde_json::Value> {
    if window.url().ok()?.host_str() != Some("ehall.szu.edu.cn") {
        return None;
    }
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    window
        .eval_with_callback("window.__SZU_ACADEMIC_CONTEXT__ || null", move |raw| {
            let value = serde_json::from_str::<serde_json::Value>(&raw).ok();
            let value = value.and_then(|v| {
                if let Some(s) = v.as_str() {
                    serde_json::from_str(s).ok()
                } else {
                    Some(v)
                }
            });
            let _ = send.send(value);
        })
        .ok()?;
    receive
        .recv_timeout(Duration::from_secs(3))
        .ok()
        .flatten()
        .filter(|v| v.is_object())
}
