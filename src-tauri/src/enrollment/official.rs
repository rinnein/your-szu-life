use super::{
    client::Client,
    model::{Admission, Authentication, Mode},
    protocol::{Error, Failure, Result},
    school_cookies as cookies, session, Service,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::Manager;
#[derive(Deserialize)]
struct PageState {
    generation: u64,
    account: String,
    token: String,
    changed: u64,
    logout: bool,
    expired: bool,
}
pub(super) fn is_school_url(url: &reqwest::Url, mode: Mode) -> bool {
    url.origin() == reqwest::Url::parse(mode.origin()).unwrap().origin()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().starts_with("/xsxkapp/sys/xsxkapp/")
}
fn script(bootstrap: &Value) -> Result<String> {
    let raw = serde_json::to_string(bootstrap)
        .map_err(|_| Error::new(Failure::Protocol, "官网初始化数据无效"))?;
    let literal = serde_json::to_string(&raw).unwrap();
    Ok(format!(
        "({})(JSON.parse({literal}));",
        include_str!("official-page.js")
            .trim()
            .trim_end_matches(';')
    ))
}
fn bootstrap(c: &Client, student: &str, generation: u64) -> Result<Value> {
    let info = match c.student_info(student) {
        Ok(v) => Some(v),
        Err(e) if e.kind == Failure::AdmissionRestricted => None,
        Err(e) => return Err(e),
    };
    // Optional public metadata can be fetched directly again by the official page.
    let dictionary = c
        .post("publicinfo/dictionary.do", &[])
        .ok()
        .and_then(|v| v.get("data").cloned())
        .and_then(|v| v.get("dictionaryList").cloned());
    let sys = c
        .post("publicinfo/sysparam.do", &[])
        .ok()
        .and_then(|v| v.get("data").cloned());
    Ok(
        json!({"origin":c.mode.origin(),"student":student,"token":c.token,"generation":generation,"initial":true,"studentInfo":info,"currentBatch":info.as_ref().and_then(|v|v.get("electiveBatch")),"currentCampus":info.as_ref().map(|v|json!({"code":v["campus"],"name":v["campusName"]})),"campusList":dictionary.as_ref().and_then(|v|v.get("XQ")),"dictionary":dictionary,"sysParam":sys}),
    )
}
fn read_page(window: &tauri::WebviewWindow, mode: Mode) -> Option<PageState> {
    if !is_school_url(&window.url().ok()?, mode) {
        return None;
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    window
        .eval_with_callback(
            "window.__SZU_OFFICIAL_SESSION__?.read() || null",
            move |raw| {
                let value = serde_json::from_str::<Value>(&raw).ok().and_then(|v| {
                    if let Some(s) = v.as_str() {
                        serde_json::from_str(s).ok()
                    } else {
                        Some(v)
                    }
                });
                let _ = tx.send(value.and_then(|v| serde_json::from_value::<PageState>(v).ok()));
            },
        )
        .ok()?;
    rx.recv_timeout(Duration::from_secs(3)).ok().flatten()
}
pub async fn open(
    service: Arc<Service>,
    parent: tauri::WebviewWindow,
) -> std::result::Result<(), String> {
    #[cfg(not(target_os = "android"))]
    let _ = &parent;
    let opening = service.official_opener.lock().await;
    let prepare = service.clone();
    let (student, mode, shared) = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let profile = {
            let _operation = prepare.operation.lock().unwrap();
            prepare.identity()?
        };
        let mode = prepare.snapshot().mode;
        Ok((profile.student_id, mode, prepare.shared_client(mode)?))
    })
    .await
    .map_err(|_| "官网初始化失败")?
    .map_err(|e| e.message)?;
    let existing = service.official_windows.lock().unwrap().get(&mode).cloned();
    if let Some(window) = existing.and_then(|label| service.app.get_webview_window(&label)) {
        window.set_focus().map_err(|_| "无法切换官网窗口")?;
        let inspecting = window.clone();
        let account = student.clone();
        let mismatch = tauri::async_runtime::spawn_blocking(move || {
            read_page(&inspecting, mode).is_some_and(|p| p.account != account)
        })
        .await
        .unwrap_or(false);
        return if mismatch {
            Err("官网窗口当前是其他账号，请关闭该窗口后重新打开".into())
        } else {
            Ok(())
        };
    }
    if shared.logged_out || shared.client.token.is_empty() {
        return Err("请先登录选课系统，再共享到学校官网".into());
    }
    let c = shared.client.clone();
    let account = student.clone();
    let gen = shared.generation;
    let data = tauri::async_runtime::spawn_blocking(move || bootstrap(&c, &account, gen))
        .await
        .map_err(|_| "官网初始化失败")?
        .map_err(|e| e.message)?;
    if service
        .shared_client(mode)
        .map_err(|e| e.message)?
        .generation
        != gen
    {
        return Err("会话已更新，请重新打开官网".into());
    }
    let label = format!("school-official-{}", uuid::Uuid::new_v4());
    let builder = tauri::WebviewWindowBuilder::new(
        &service.app,
        &label,
        tauri::WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .title("学校官方选课系统")
    .initialization_script(&script(&data).map_err(|e| e.message)?);
    #[cfg(target_os = "android")]
    let builder = builder
        .activity_name("FeatureActivity")
        .created_by_activity_name(parent.activity_name().map_err(|_| "无法确认父窗口")?);
    let window = builder.build().map_err(|_| "无法创建官网窗口")?;
    service
        .official_windows
        .lock()
        .unwrap()
        .insert(mode, label.clone());
    let target = window.clone();
    let c = shared.client.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let desired = cookies::records(&c.jar, mode);
        let browser = cookies::capture(&target, mode, &desired)?;
        cookies::install(
            &target,
            cookies::patch(&cookies::records(&browser, mode), &desired),
        )?;
        Ok(desired)
    })
    .await
    .map_err(|_| "无法恢复官网会话".to_string())?
    .map_err(|e| e.message);
    let baseline = match installed {
        Ok(v) => v,
        Err(error) => {
            let _ = window.destroy();
            service.official_windows.lock().unwrap().remove(&mode);
            return Err(error);
        }
    };
    if window
        .navigate(
            format!("{}/xsxkapp/sys/xsxkapp/*default/index.do", mode.origin())
                .parse()
                .unwrap(),
        )
        .is_err()
    {
        let _ = window.destroy();
        service.official_windows.lock().unwrap().remove(&mode);
        return Err("无法打开学校官网".into());
    }
    drop(opening);
    std::thread::spawn(move || synchronize(service, window, label, student, mode, baseline));
    Ok(())
}
fn synchronize(
    service: Arc<Service>,
    window: tauri::WebviewWindow,
    label: String,
    student: String,
    mode: Mode,
    baseline: cookies::Records,
) {
    let mut previous_browser = baseline.clone();
    let mut previous_native = baseline;
    let mut revision = 0;
    let mut dirty_until = Instant::now();
    let mut next_refresh = Instant::now();
    let mut detached = false;
    loop {
        std::thread::sleep(Duration::from_millis(800));
        if service.app.get_webview_window(&label).is_none() {
            break;
        }
        if service
            .credentials
            .load_profile()
            .ok()
            .flatten()
            .is_none_or(|p| p.student_id != student)
        {
            let _ = window.destroy();
            break;
        }
        let Ok(shared) = service.shared_client(mode) else {
            continue;
        };
        let Some(page) = read_page(&window, mode) else {
            continue;
        };
        if page.account != student {
            service.message("官网账户已变化，已停止共享；请关闭该窗口后从当前账户重新打开");
            detached = true;
            break;
        }
        if shared.logged_out && page.logout {
            let _ = window.eval(format!(
                "window.__SZU_OFFICIAL_SESSION__?.invalidate({})",
                shared.generation
            ));
            continue;
        }
        let mut paths = cookies::records(&shared.client.jar, mode);
        paths.extend(previous_browser.clone());
        let Ok(browser) = cookies::capture(&window, mode, &paths) else {
            continue;
        };
        if !shared.logged_out && page.generation < shared.generation {
            let desired = cookies::records(&shared.client.jar, mode);
            if cookies::install(
                &window,
                cookies::patch(&cookies::records(&browser, mode), &desired),
            )
            .is_err()
            {
                continue;
            }
            if service
                .shared_client(mode)
                .ok()
                .is_none_or(|v| v.generation != shared.generation)
            {
                continue;
            }
            if let Ok(js) = script(
                &json!({"origin":mode.origin(),"student":student,"token":shared.client.token,"generation":shared.generation,"initial":false}),
            ) {
                let _ = window.eval(js);
            }
            previous_browser = desired.clone();
            previous_native = desired;
            continue;
        }
        if !shared.logged_out && page.generation != shared.generation {
            continue;
        }
        if page.logout {
            let mut sessions = service.shared_sessions.lock().unwrap();
            let mut revoked = None;
            if let Some(current) = sessions.get_mut(&mode) {
                if current.generation == shared.generation && !current.logged_out {
                    let generation = service
                        .session_serial
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                        + 1;
                    if let Ok(jar) = session::jar("") {
                        if let Ok(mut client) =
                            Client::from_jar(mode, String::new(), jar, student.clone())
                        {
                            client.generation = generation;
                            current.client = client;
                            current.generation = generation;
                            current.logged_out = true;
                            revoked = Some(generation);
                        }
                    }
                }
            }
            drop(sessions);
            if let Some(generation) = revoked {
                let _ = window.eval(format!(
                    "window.__SZU_OFFICIAL_SESSION__?.invalidate({generation})"
                ));
                if service.snapshot().mode == mode {
                    let mut s = service.state.lock().unwrap();
                    s.authentication = Authentication::Expired;
                    s.logged_in = false;
                    s.admission = Admission::Unchecked;
                    s.admission_message = "已从学校官网退出登录".into();
                    drop(s);
                    service.pause("已从学校官网退出登录，请手动登录后继续");
                }
                let _ = service.save_shared_cookies();
            }
            continue;
        }
        if !page.token.is_empty() && (shared.logged_out || page.token != shared.client.token) {
            if let Ok(mut candidate) =
                Client::from_jar(mode, page.token.clone(), browser.clone(), student.clone())
            {
                candidate.generation = shared.generation;
                let validation = candidate.student_info(&student);
                if validation.is_ok()
                    || validation
                        .as_ref()
                        .is_err_and(|e| e.kind == Failure::AdmissionRestricted)
                {
                    if let Ok(_operation) = service.operation.try_lock() {
                        if service.save_client(&candidate).is_ok()
                            && service.snapshot().mode == mode
                        {
                            let _ = service.refresh_context(&candidate, &student);
                        }
                    }
                }
            }
            continue;
        }
        if shared.logged_out {
            continue;
        }
        let browser_records = cookies::records(&browser, mode);
        cookies::merge_browser_delta(
            &shared.client.jar,
            mode,
            &previous_browser,
            &browser_records,
            &previous_native,
        );
        let native = cookies::records(&shared.client.jar, mode);
        if service
            .shared_client(mode)
            .ok()
            .is_none_or(|v| v.generation != shared.generation)
        {
            continue;
        }
        if cookies::install(&window, cookies::patch(&browser_records, &native)).is_err() {
            continue;
        }
        previous_browser = native.clone();
        previous_native = native;
        let _ = service.save_shared_cookies();
        if page.changed != revision {
            revision = page.changed;
            dirty_until = Instant::now() + Duration::from_secs(15);
        }
        if Instant::now() < dirty_until
            && Instant::now() >= next_refresh
            && service.snapshot().mode == mode
        {
            next_refresh = Instant::now() + Duration::from_secs(2);
            if let Ok(_operation) = service.operation.try_lock() {
                if service.snapshot().can_select() {
                    let _ = service.reconcile(&shared.client);
                }
            }
        }
        if page.expired && service.snapshot().mode == mode {
            if let Ok(_operation) = service.operation.try_lock() {
                if let Err(e) = shared.client.student_info(&student) {
                    if e.kind == Failure::Session {
                        let _ = service.record_admission_error(&e);
                    }
                }
            }
        }
    }
    if !detached {
        let mut windows = service.official_windows.lock().unwrap();
        if windows.get(&mode) == Some(&label) {
            windows.remove(&mode);
        }
    }
}
