use super::{
    client::{Client, LOGIN},
    model::{SessionStatus, TaskStatus},
    session, Service,
};
use std::{sync::Arc, time::Duration};
use tauri::{Manager, WebviewWindow};
use tokio_util::sync::CancellationToken;

pub(super) struct Attempt {
    pub label: String,
    parent: String,
    pub token: CancellationToken,
}

pub(super) fn local_parent(parent: &WebviewWindow) -> Result<(), String> {
    let url = parent.url().map_err(|_| "无法确认 UOOC 来源页面")?;
    let official = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"));
    let dev = parent
        .app_handle()
        .config()
        .build
        .dev_url
        .as_ref()
        .is_some_and(|dev| dev.origin() == url.origin());
    if (!official && !dev)
        || url.path() != "/uooc"
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("请从应用内的 UOOC 页面打开登录".into());
    }
    Ok(())
}

pub(super) async fn open(service: Arc<Service>, parent: WebviewWindow) -> Result<(), String> {
    local_parent(&parent)?;
    let (label, token, epoch) = {
        let mut runtime = service.runtime.lock().unwrap();
        if let Some(attempt) = &runtime.login {
            if let Some(window) = parent.app_handle().get_webview_window(&attempt.label) {
                let _ = window.set_focus();
            }
            return Ok(());
        }
        runtime.epoch += 1;
        runtime.session_cancel.cancel();
        runtime.session_cancel = CancellationToken::new();
        super::checkin::cancel_locked(&mut runtime, &[], "正在重新登录 UOOC");
        runtime.snapshot.progress.clear();
        runtime.snapshot.checkins.clear();
        Service::freeze(
            &mut runtime,
            TaskStatus::AwaitingLogin,
            "等待 UOOC 登录完成",
        );
        runtime.snapshot.session = SessionStatus::LoggingIn;
        runtime.snapshot.message = "请在官方窗口完成登录，成功后会自动返回".into();
        let label = format!("uooc-login-{}", uuid::Uuid::new_v4());
        let token = CancellationToken::new();
        runtime.login = Some(Attempt {
            label: label.clone(),
            parent: parent.label().into(),
            token: token.clone(),
        });
        service.publish(&mut runtime);
        (label, token, runtime.epoch)
    };
    let _ = service.sync_platform();
    #[cfg(target_os = "android")]
    let navigation = {
        let owner = Arc::downgrade(&service);
        let failure_owner = owner.clone();
        let expected_label = label.clone();
        let failure_label = label.clone();
        let current = token.clone();
        super::user_agent::AndroidNavigation::new(
            parent.app_handle().clone(),
            label.clone(),
            move || {
                if current.is_cancelled() {
                    return false;
                }
                owner.upgrade().is_some_and(|service| {
                    service.runtime.lock().ok().is_some_and(|runtime| {
                        runtime.epoch == epoch
                            && runtime
                                .login
                                .as_ref()
                                .is_some_and(|attempt| attempt.label == expected_label)
                    })
                })
            },
            move || {
                if let Some(service) = failure_owner.upgrade() {
                    let mut runtime = service.runtime.lock().unwrap();
                    if runtime.epoch == epoch
                        && runtime
                            .login
                            .as_ref()
                            .is_some_and(|attempt| attempt.label == failure_label)
                    {
                        runtime.snapshot.message =
                            "UOOC 访问模式切换失败，请关闭登录窗口后重试".into();
                        service.publish(&mut runtime);
                    }
                }
            },
        )
    };
    let result = (|| {
        let builder = tauri::WebviewWindowBuilder::new(
            parent.app_handle(),
            &label,
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .title("UOOC 官方登录");
        #[cfg(not(target_os = "android"))]
        let builder = builder.user_agent(super::user_agent::DESKTOP);
        #[cfg(target_os = "android")]
        let builder = builder
            .on_navigation(move |url| navigation.navigate(url))
            .activity_name("FeatureActivity")
            .created_by_activity_name(parent.activity_name().map_err(|_| "无法确认 UOOC 父窗口")?);
        builder
            .build()
            .map_err(|_| "无法创建 UOOC 登录窗口".to_owned())
    })();
    let window = match result {
        Ok(window) => window,
        Err(error) => {
            window_closed(&service, &label);
            return Err(error);
        }
    };
    let restoring = window.clone();
    let jar = service.client().jar.clone();
    let lease = service.native_session.clone().lock_owned().await;
    let restored = tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        session::install(&restoring, &jar, false)
    })
    .await
    .map_err(|_| "无法恢复 UOOC Cookie".to_owned())
    .and_then(|v| v);
    if let Err(error) = restored {
        service.change(|s| s.message = error.clone());
        let _ = window.destroy();
        window_closed(&service, &label);
        return Err(error);
    }
    if token.is_cancelled() {
        let _ = window.destroy();
        return Ok(());
    }
    if window.navigate(LOGIN.parse().unwrap()).is_err() {
        let _ = window.destroy();
        window_closed(&service, &label);
        return Err("无法打开 UOOC 官网".into());
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! { biased; _ = token.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(2)) => {} }
            let capturing = window.clone();
            let jar =
                match tauri::async_runtime::spawn_blocking(move || session::capture(&capturing))
                    .await
                {
                    Ok(Ok(jar)) => jar,
                    _ => {
                        service.change(|s| {
                            s.message = "无法读取原生 UOOC Cookie，请检查 WebView 版本后重试".into()
                        });
                        break;
                    }
                };
            let Ok(client) = Client::new(jar).map(Arc::new) else {
                break;
            };
            let Ok(user) = client.user(&token).await else {
                continue;
            };
            let Ok(courses) = client.courses(&token).await else {
                continue;
            };
            {
                let mut runtime = service.runtime.lock().unwrap();
                if runtime.epoch != epoch || token.is_cancelled() {
                    break;
                }
                if let Err(error) = service.save_session(&client) {
                    runtime.snapshot.message = error;
                    service.publish(&mut runtime);
                    continue;
                }
                *service.client.write().unwrap() = client;
                runtime.snapshot.update_courses(user, courses);
                runtime.snapshot.session = SessionStatus::Ready;
                runtime.snapshot.message.clear();
                runtime.login.take();
                service.publish(&mut runtime);
            }
            let _ = window.destroy();
            let _ = parent.set_focus();
            service.reconcile();
            service.session_ready().await;
            return;
        }
        let _ = window.destroy();
        window_closed(&service, &label);
    });
    Ok(())
}

pub(super) fn window_closed(service: &Arc<Service>, label: &str) {
    let attempt = {
        let mut runtime = service.runtime.lock().unwrap();
        if !runtime
            .login
            .as_ref()
            .is_some_and(|a| a.label == label || a.parent == label)
        {
            return;
        }
        let attempt = runtime.login.take().unwrap();
        attempt.token.cancel();
        runtime.epoch += 1;
        runtime.snapshot.session = SessionStatus::LoginRequired;
        if runtime.snapshot.message.starts_with("请在官方") {
            runtime.snapshot.message = "UOOC 登录已取消".into();
        }
        service.publish(&mut runtime);
        attempt
    };
    if let Some(app) = &service.app {
        if attempt.label != label {
            if let Some(window) = app.get_webview_window(&attempt.label) {
                let _ = window.destroy();
            }
        }
        if let Some(parent) = app.get_webview_window(&attempt.parent) {
            let _ = parent.set_focus();
        }
    }
}

pub(super) async fn logout(service: Arc<Service>, window: WebviewWindow) -> Result<(), String> {
    let attempt;
    {
        let mut runtime = service.runtime.lock().unwrap();
        runtime.epoch += 1;
        runtime.session_cancel.cancel();
        runtime.session_cancel = CancellationToken::new();
        Service::freeze(&mut runtime, TaskStatus::Stopped, "已退出 UOOC 登录");
        super::checkin::cancel_locked(&mut runtime, &[], "已退出 UOOC 登录");
        runtime.snapshot.progress.clear();
        runtime.snapshot.checkins.clear();
        for task in &mut runtime.snapshot.tasks {
            task.desired_running = false;
        }
        attempt = runtime.login.take();
        if let Some(attempt) = &attempt {
            attempt.token.cancel();
        }
        runtime.snapshot.session = SessionStatus::LoginRequired;
        runtime.snapshot.user = None;
        runtime.snapshot.courses.clear();
        runtime.snapshot.message.clear();
        service.publish(&mut runtime);
    }
    let _ = service.sync_platform();
    if let Some(attempt) = attempt {
        if let Some(login) = window.app_handle().get_webview_window(&attempt.label) {
            let _ = login.destroy();
        }
    }
    let old = service.client();
    *service.client.write().unwrap() = Arc::new(Client::new(session::load(None)?)?);
    service
        .credentials
        .delete_uooc_session()
        .map_err(|_| "无法清除 UOOC 安全存储")?;
    let lease = service.native_session.clone().lock_owned().await;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let native = session::capture(&window)?;
        session::install(&window, &native, true)?;
        session::install(&window, &old.jar, true)
    })
    .await
    .map_err(|_| "无法清除 UOOC 网页会话".to_owned())?
}
