use super::{model::*, Service, Store};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;
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
        for _path in ["/", "/enrollment", "/enrollment/tasks", "/academic/"] {
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
