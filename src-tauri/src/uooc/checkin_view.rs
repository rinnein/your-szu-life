use super::{
    checkin::Job,
    client::{Client, ORIGIN},
    session, Service,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tauri::{Manager, Url, WebviewWindow};

pub(super) type Async<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub(super) trait Page: Send + Sync {
    fn ready(&self) -> Async<'_, Result<bool, String>>;
    fn synchronize(&self, service: Arc<Service>, job: Job) -> Async<'_, Result<(), String>>;
}
#[cfg(test)]
pub(super) trait Factory: Send + Sync {
    fn open(&self, service: Arc<Service>, job: Job) -> Async<'_, Result<Box<dyn Page>, String>>;
}
pub(super) fn course_url(cid: &str) -> Result<Url, String> {
    if cid
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
        .as_deref()
        != Some(cid)
    {
        return Err("课程编号无效".into());
    }
    format!("{ORIGIN}/home/course/{cid}")
        .parse()
        .map_err(|_| "课程地址无效".into())
}
pub(super) fn is_course_url(url: &Url, cid: &str) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("www.uooc.net.cn")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == format!("/home/course/{cid}")
}
struct NativePage {
    service: Arc<Service>,
    job: Job,
    window: Option<WebviewWindow>,
    cookie_window: WebviewWindow,
    loaded: Arc<Mutex<Result<bool, String>>>,
    lease: Option<tokio::sync::OwnedMutexGuard<()>>,
}
impl Drop for NativePage {
    fn drop(&mut self) {
        // Destroying our own window is cleanup, not a user cancellation.
        self.service
            .runtime
            .lock()
            .unwrap()
            .checkins
            .closing
            .insert(self.job.id.clone());
        #[cfg(target_os = "android")]
        if self.window.is_none() {
            let _ = super::android_checkin::close(&self.job.id);
        }
        if let Some(window) = &self.window {
            let _ = window.destroy();
        }
        if self.job.visible {
            if let Some(app) = &self.service.app {
                if let Some(parent) = app.get_webview_window(&self.job.parent) {
                    let _ = parent.set_focus();
                }
            }
        }
    }
}
impl Page for NativePage {
    fn ready(&self) -> Async<'_, Result<bool, String>> {
        Box::pin(async move {
            #[cfg(target_os = "android")]
            if self.window.is_none() {
                return super::android_checkin::ready(&self.job.id);
            }
            let window = self.window.as_ref().ok_or("签到窗口已关闭")?;
            let url = window.url().map_err(|_| "签到窗口已关闭")?;
            if url.as_str() != "about:blank" && !is_course_url(&url, &self.job.cid) {
                return Err("官方页面离开了当前课程，请确认登录状态后重试".into());
            }
            self.loaded.lock().unwrap().clone()
        })
    }
    fn synchronize(&self, service: Arc<Service>, job: Job) -> Async<'_, Result<(), String>> {
        Box::pin(async move {
            let window = self.cookie_window.clone();
            let jar = tauri::async_runtime::spawn_blocking(move || session::capture(&window))
                .await
                .map_err(|_| "读取签到会话失败")??;
            let candidate = Client::new(jar)?;
            let user = candidate.user(&job.cancel).await.map_err(|e| e.message())?;
            let runtime = service.runtime.lock().unwrap();
            if runtime.epoch != job.epoch
                || job.cancel.is_cancelled()
                || runtime.snapshot.session != super::model::SessionStatus::Ready
                || user.id != job.account
                || runtime.snapshot.user.as_ref().map(|u| &u.id) != Some(&job.account)
            {
                return Err("签到账户已变化，请重新登录 UOOC".into());
            }
            let current = service.client();
            *current.jar.lock().map_err(|_| "UOOC 会话不可用")? =
                candidate.jar.lock().map_err(|_| "UOOC 会话不可用")?.clone();
            service.save_session(&current)
        })
    }
}
pub(super) async fn open(service: Arc<Service>, job: Job) -> Result<Box<dyn Page>, String> {
    #[cfg(test)]
    {
        let factory = service.checkin_factory.lock().unwrap().clone();
        if let Some(factory) = factory {
            return factory.open(service, job).await;
        }
    }
    let url = course_url(&job.cid)?;
    let lease = tokio::select! { biased; _ = job.cancel.cancelled() => return Err("签到已取消".into()), lock = service.native_session.clone().lock_owned() => lock };
    if !service.checkin_current(&job) {
        return Err("签到会话已变化".into());
    }
    let app = service.app.as_ref().ok_or("签到需要原生应用环境")?.clone();
    let parent = app
        .get_webview_window(&job.parent)
        .ok_or("签到父窗口已关闭")?;
    let loaded = Arc::new(Mutex::new(Ok(false)));
    #[cfg(target_os = "android")]
    let hidden_native = !job.visible;
    #[cfg(not(target_os = "android"))]
    let hidden_native = false;
    let window = if hidden_native {
        None
    } else {
        let state = loaded.clone();
        let cid = job.cid.clone();
        let builder = tauri::WebviewWindowBuilder::new(
            &app,
            &job.id,
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .title("UOOC 课程签到")
        .on_page_load(move |_, payload| {
            if payload.url().as_str() == "about:blank" {
                return;
            }
            *state.lock().unwrap() = if !is_course_url(payload.url(), &cid) {
                Err("UOOC 登录已失效或课程页面发生跳转".into())
            } else {
                Ok(matches!(
                    payload.event(),
                    tauri::webview::PageLoadEvent::Finished
                ))
            };
        });
        #[cfg(not(target_os = "android"))]
        let builder = builder
            .visible(job.visible)
            .focused(job.visible)
            .skip_taskbar(!job.visible);
        #[cfg(target_os = "android")]
        let builder = builder
            .activity_name("FeatureActivity")
            .created_by_activity_name(parent.activity_name().map_err(|_| "无法确认签到父窗口")?);
        Some(builder.build().map_err(|_| "无法创建签到窗口")?)
    };
    let mut page = NativePage {
        service: service.clone(),
        job: job.clone(),
        cookie_window: window.clone().unwrap_or(parent),
        window,
        loaded,
        lease: None,
    };
    let installing = page.cookie_window.clone();
    let jar = service.client().jar.clone();
    // Retain the native-session lease on the blocking worker even if this future is cancelled.
    let (result, lease) = tauri::async_runtime::spawn_blocking(move || {
        (session::install(&installing, &jar, false), lease)
    })
    .await
    .map_err(|_| "无法恢复签到会话")?;
    page.lease = Some(lease);
    result?;
    if !service.checkin_current(&job) {
        return Err("签到已取消或账户已变化".into());
    }
    if let Some(window) = &page.window {
        window
            .navigate(url.clone())
            .map_err(|_| "无法加载签到课程页面")?;
    }
    #[cfg(target_os = "android")]
    if page.window.is_none() {
        super::android_checkin::open(&job.id, url.as_str())?;
    }
    Ok(Box::new(page))
}
