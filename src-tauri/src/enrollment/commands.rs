use super::{model::*, Service, Store};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OnlineUsersEvent {
    pub online: Option<u64>,
    pub updated_at: Option<i64>,
    pub error: Option<String>,
}
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Action {
    Snapshot,
    CacheSnapshot,
    Cache {
        query: Query,
    },
    Session,
    Options,
    BatchInfo {
        mode: Mode,
    },
    OnlineUsers {
        mode: Mode,
    },
    Context {
        batch: String,
        campus: String,
    },
    Login,
    CaptchaPrepare {
        flow: String,
        revision: u64,
    },
    CaptchaPoint {
        flow: String,
        revision: u64,
        challenge_id: String,
        points: Vec<[u32; 2]>,
    },
    CaptchaCancel {
        flow: String,
        revision: u64,
        #[serde(default)]
        discard: bool,
    },
    CaptchaSubmit {
        flow: String,
        revision: u64,
        challenge_id: String,
        points: Vec<[u32; 2]>,
    },
    Captcha,
    Solve {
        points: Vec<[u32; 2]>,
    },
    Mode {
        mode: Mode,
    },
    Probe,
    Labs {
        course: Course,
    },
    Refresh {
        query: Query,
    },
    Selected,
    Pin {
        id: String,
        pinned: bool,
    },
    Add {
        input: TaskInput,
    },
    Control {
        id: Option<String>,
        action: String,
        gear: Option<Gear>,
        priority: Option<i32>,
    },
    Drop {
        course: Course,
        confirmed: bool,
    },
    Academic {
        recalculate: bool,
    },
    AcademicProgress,
}
fn trusted_source(label: &str) -> bool {
    label == "main" || label.starts_with("feature-")
}
fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !trusted_source(window.label()) {
        return Err("当前窗口无法访问本地服务，请从工作台重新打开".into());
    }
    Ok(())
}
#[cfg(test)]
mod source_tests {
    use super::trusted_source;
    #[test]
    fn accepts_logical_route_timing_and_nested_pages() {
        for _path in ["/", "/study/enrollment", "/study/enrollment/tasks", "/study/academic/"] {
            assert!(trusted_source("main"));
            assert!(trusted_source("feature-test"));
        }
    }
    #[test]
    fn source_scope_uses_window_lifecycle_only() {
        assert!(trusted_source("main"));
        assert!(trusted_source("feature-test"));
        assert!(!trusted_source("academic-auth-test"));
        assert!(!trusted_source("external"));
    }
}
#[tauri::command]
pub(crate) async fn enrollment_command(
    window: tauri::WebviewWindow,
    action: Action,
) -> Result<Value, String> {
    local(&window)?;
    let service = window.state::<Store>().0.clone();
    tauri::async_runtime::spawn_blocking(move || service.execute(action).map_err(|e| e.message))
        .await
        .map_err(|_| "选课任务异常".to_owned())?
}
#[tauri::command]
pub(crate) async fn open_academic_portal(
    window: tauri::WebviewWindow,
    kind: String,
    target: Option<String>,
) -> Result<(), String> {
    local(&window)?;
    let service: Arc<Service> = window.state::<Store>().0.clone();
    super::portal::open(service, window, kind, target).await
}

#[tauri::command]
pub(crate) async fn open_enrollment_official(window: tauri::WebviewWindow) -> Result<(), String> {
    local(&window)?;
    let service = window.state::<Store>().0.clone();
    super::official::open(service, window).await
}

#[tauri::command]
pub(crate) fn subscribe_enrollment_state(
    window: tauri::WebviewWindow,
    channel: tauri::ipc::Channel<super::EnrollmentDeltaEvent>,
) -> Result<(), String> {
    local(&window)?;
    let service = window.state::<Store>().0.clone();
    let snapshot = service.snapshot();
    let _ = channel.send(super::EnrollmentDeltaEvent::Initial(Box::new(snapshot)));
    service.state_subscribers.subscribe(channel);
    Ok(())
}

/// 后端负责在线人数轮询，前端只收 Channel；通道关闭后在下一次推送时退出。
#[tauri::command]
pub(crate) async fn watch_enrollment_online_users(
    window: tauri::WebviewWindow,
    mode: Mode,
    channel: tauri::ipc::Channel<OnlineUsersEvent>,
) -> Result<(), String> {
    local(&window)?;
    let service = window.state::<Store>().0.clone();
    let _ = channel.send(OnlineUsersEvent {
        online: None,
        updated_at: None,
        error: None,
    });
    loop {
        let poll = service.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            poll.execute(Action::OnlineUsers { mode })
        })
        .await
        .map_err(|_| "在线人数查询异常".to_owned())?;
        let event = match result {
            Ok(value) => OnlineUsersEvent {
                online: value["data"]["onlineUsers"]
                    .as_u64()
                    .or_else(|| value["onlineUsers"].as_u64()),
                updated_at: Some(chrono::Utc::now().timestamp_millis()),
                error: None,
            },
            Err(error) => OnlineUsersEvent {
                online: None,
                updated_at: None,
                error: Some(error.message),
            },
        };
        if channel.send(event).is_err() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

fn count(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .unwrap_or(0)
}

/// 学业重算由后端启动并轮询进度，完成后返回最新快照。
#[tauri::command]
pub(crate) async fn recalculate_academic(
    window: tauri::WebviewWindow,
    on_progress: tauri::ipc::Channel<Value>,
) -> Result<Value, String> {
    local(&window)?;
    let service = window.state::<Store>().0.clone();
    let start = service.clone();
    let started = tauri::async_runtime::spawn_blocking(move || {
        start.execute(Action::Academic { recalculate: true })
    })
    .await
    .map_err(|_| "选课任务异常".to_owned())?
    .map_err(|e| e.message)?;
    if started["calculating"].as_bool() != Some(true) {
        return Ok(started);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let poll = service.clone();
        let value = tauri::async_runtime::spawn_blocking(move || {
            poll.execute(Action::AcademicProgress)
        })
        .await
        .map_err(|_| "选课任务异常".to_owned())?
        .map_err(|e| e.message)?;
        if on_progress.send(value.clone()).is_err() {
            return Err("学业进度订阅已关闭".into());
        }
        let done = value["datas"]["byscjd"]["rows"]
            .as_array()
            .and_then(|rows| rows.first())
            .map(|row| {
                let total = count(&row["ZS"]);
                let done = count(&row["YWCS"]);
                total > 0 && done >= total
            })
            .unwrap_or(false);
        if done {
            let load = service.clone();
            return tauri::async_runtime::spawn_blocking(move || {
                load.execute(Action::Academic { recalculate: false })
            })
            .await
            .map_err(|_| "选课任务异常".to_owned())?
            .map_err(|e| e.message);
        }
        if std::time::Instant::now() > deadline {
            return Err("计算仍未完成，请稍后查询进度".into());
        }
    }
}
