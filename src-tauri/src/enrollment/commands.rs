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
fn trusted_source(url: &reqwest::Url, label: &str, dev: Option<&reqwest::Url>) -> bool {
    let window = label == "main" || label.starts_with("feature-");
    let origin =
        (url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none())
            || (matches!(url.scheme(), "http" | "https")
                && url.host_str() == Some("tauri.localhost")
                && url.port().is_none())
            || dev.is_some_and(|dev| dev.origin() == url.origin());
    window && origin && url.username().is_empty() && url.password().is_none()
}
fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    let url = window
        .url()
        .map_err(|_| "无法连接本地服务，请重新打开页面")?;
    #[cfg(debug_assertions)]
    {
        let route = match url.path() {
            "/"
            | "/enrollment"
            | "/enrollment/"
            | "/enrollment/login"
            | "/enrollment/tasks"
            | "/enrollment/selected"
            | "/enrollment/timetable"
            | "/enrollment/settings"
            | "/academic"
            | "/webvpn" => url.path(),
            _ => "[other]",
        };
        eprintln!(
            "academic IPC source: origin={}, route={}, window_kind={}",
            url.origin().ascii_serialization(),
            route,
            if window.label() == "main" {
                "main"
            } else if window.label().starts_with("feature-") {
                "feature"
            } else {
                "other"
            }
        );
    }
    if !trusted_source(
        &url,
        window.label(),
        window.app_handle().config().build.dev_url.as_ref(),
    ) {
        // Do not log query/hash, which can contain authentication tickets.
        eprintln!(
            "academic IPC source rejected: origin={}, window_kind={}",
            url.origin().ascii_serialization(),
            if window.label().starts_with("academic-auth-") {
                "authentication"
            } else {
                "other"
            }
        );
        return Err("当前窗口无法访问本地服务，请从工作台重新打开".into());
    }
    Ok(())
}
#[cfg(test)]
mod source_tests {
    use super::trusted_source;
    #[test]
    fn accepts_logical_route_timing_and_nested_pages() {
        for path in ["/", "/enrollment", "/enrollment/tasks", "/academic/"] {
            assert!(trusted_source(
                &format!("http://tauri.localhost{path}").parse().unwrap(),
                "main",
                None
            ));
            assert!(trusted_source(
                &format!("tauri://localhost{path}").parse().unwrap(),
                "feature-test",
                None
            ));
        }
    }
    #[test]
    fn rejects_external_origins_and_authentication_windows() {
        for url in [
            "https://ehall.szu.edu.cn/enrollment",
            "https://tauri.localhost.evil.test/",
            "https://user@tauri.localhost/",
        ] {
            assert!(!trusted_source(&url.parse().unwrap(), "main", None));
        }
        assert!(!trusted_source(
            &"http://tauri.localhost/".parse().unwrap(),
            "academic-auth-test",
            None
        ));
        let dev = "http://localhost:1420".parse().unwrap();
        assert!(trusted_source(
            &"http://localhost:1420/".parse().unwrap(),
            "main",
            Some(&dev)
        ));
        assert!(!trusted_source(
            &"http://localhost:1421/".parse().unwrap(),
            "main",
            Some(&dev)
        ));
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
