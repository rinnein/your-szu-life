use super::{session, Service};
use std::{sync::Arc, time::Duration};
use tauri::Manager;

/// 打开 WebVPN 官方认证窗口。
///
/// 学业完成查询已改为 authserver 自动认证（见 `academic.rs`），这里只剩
/// 选课等需要人工完成双重认证的高风险操作。
pub async fn open(
    service: Arc<Service>,
    parent: tauri::WebviewWindow,
    target: Option<String>,
) -> Result<(), String> {
    let student = service.identity().map_err(|e| e.message)?.student_id;
    let return_external = target.is_some();
    let target = target.unwrap_or_else(|| {
        "https://bkxk.webvpn.szu.edu.cn/xsxkapp/sys/xsxkapp/*default/index.do".into()
    });
    let url = session::auth_url(&target).map_err(|e| e.message)?;
    {
        let attempt = service.login_attempt.lock().unwrap();
        if let Some(label) = attempt.as_ref() {
            if let Some(w) = parent.app_handle().get_webview_window(label) {
                let _ = w.set_focus();
                return Ok(());
            }
        }
    }
    let label = format!("webvpn-auth-{}", uuid::Uuid::new_v4());
    let builder = tauri::WebviewWindowBuilder::new(
        parent.app_handle(),
        &label,
        tauri::WebviewUrl::External(url),
    )
    .title("WebVPN 高风险操作认证");
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
                let raw = session::capture_vpn(&w, &[session::VPN, "https://bkxk.webvpn.szu.edu.cn/"])
                    .map_err(|e| e.message)?;
                if !session::gateway_valid(&raw).map_err(|e| e.message)? {
                    return Ok(false);
                }
                let _operation = s.operation.lock().unwrap();
                if s.identity().map_err(|e| e.message)?.student_id != expected {
                    return Err("账户已切换，取消认证".into());
                }
                let profile = s.identity().map_err(|e| e.message)?;
                if !session::matches_account(&raw, &expected, &profile.campus_card_number)
                    .map_err(|e| e.message)?
                {
                    return Err("WebVPN账户与当前应用账户不一致，请切换到当前账户认证".into());
                }
                let mut secrets = s.secrets.lock().unwrap();
                secrets.vpn = raw;
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
                if return_external {
                    if let Ok(url) = target.parse() {
                        let _ = parent.navigate(url);
                    }
                }
                let _ = parent.set_focus();
                if service
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

pub fn on_page_load(window: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
    if window.label().starts_with("webvpn-auth-")
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
            let _ = open(service, parent, Some(target)).await;
        }
    });
}