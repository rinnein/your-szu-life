//! 外部集成状态文件。
//!
//! WorkBuddy 专家等本机外部程序无法直接调用 Tauri command，也不知道动态 MCP 端口。
//! 这里把「已安装/运行中的泥深当前能提供什么」写成一个只读 JSON 契约，放在
//! `app_data_dir()/integration.json`，由外部程序自行读取。
//!
//! 约定：
//! - 只写状态与布尔值，绝不写姓名、学号、校园卡号、Cookie、密码或密钥；
//! - Unix 下文件权限固定 0600；
//! - 写入使用临时文件 + rename，读方不会看到半截 JSON；
//! - 由启动、MCP 开关、初始化/认证/UOOC 事件驱动，不做高频轮询。

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Listener, Manager};

use szu_core::auth::model::AuthState;
use szu_uooc::uooc::model::SessionStatus;

pub const SCHEMA_VERSION: u32 = 1;
pub const STATUS_FILE: &str = "integration.json";

/// 进程启动时间：只用于让外部程序区分「本次运行」与旧文件。
static STARTED_AT_MS: OnceLock<i64> = OnceLock::new();
/// 合并短时间内的多次刷新请求；只有最后一次会真正落盘。
static SCHEDULE_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub schema_version: u32,
    pub updated_at_ms: i64,
    pub started_at_ms: i64,
    pub pid: u32,
    pub app: AppInfo,
    pub mcp: McpInfo,
    pub onboarding: OnboardingInfo,
    pub auth: AuthInfo,
    pub uooc: UoocInfo,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub arch: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInfo {
    /// 用户上次开启过 MCP（偏好里保存的开关），不代表本次一定监听成功。
    pub enabled: bool,
    pub running: bool,
    pub port: Option<u16>,
    pub preferred_port: u16,
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingInfo {
    /// `state_view()` 是否成功。失败时 `needs_setup` 为 null，外部应按「未知」处理。
    pub available: bool,
    pub needs_setup: Option<bool>,
    pub current_step: Option<String>,
    pub steps: Vec<OnboardingStepInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStepInfo {
    pub id: String,
    pub status: String,
    pub skippable: bool,
    pub revision: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthInfo {
    pub state: String,
    pub logged_in: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UoocInfo {
    pub session: String,
    pub logged_in: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

/// 构建当前状态快照。可能读取 keyring（初始化进度），调用方应放在阻塞任务里。
pub fn build(app: &AppHandle) -> IntegrationStatus {
    let started_at_ms = *STARTED_AT_MS.get_or_init(now_ms);
    let mcp_status = crate::mcp::get_mcp_server_status();
    let mcp_enabled = szu_core::preferences::load::<bool>(szu_core::preferences::MCP_ENABLED)
        .ok()
        .flatten()
        .unwrap_or(false);

    IntegrationStatus {
        schema_version: SCHEMA_VERSION,
        updated_at_ms: now_ms(),
        started_at_ms,
        pid: std::process::id(),
        app: AppInfo {
            version: app.package_info().version.to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        },
        mcp: McpInfo {
            enabled: mcp_enabled,
            running: mcp_status.running,
            port: mcp_status.port,
            preferred_port: mcp_status.preferred_port,
            endpoint: mcp_status.endpoint,
        },
        onboarding: onboarding_info(),
        auth: auth_info(app),
        uooc: uooc_info(app),
    }
}

fn onboarding_info() -> OnboardingInfo {
    match szu_core::onboarding::state_view() {
        Ok(state) => OnboardingInfo {
            available: true,
            needs_setup: Some(state.needs_setup),
            current_step: state.current_step,
            steps: state
                .steps
                .into_iter()
                .map(|step| OnboardingStepInfo {
                    id: step.id,
                    status: step.status.as_str().to_string(),
                    skippable: step.skippable,
                    revision: step.revision,
                })
                .collect(),
        },
        Err(_) => OnboardingInfo {
            available: false,
            needs_setup: None,
            current_step: None,
            steps: Vec::new(),
        },
    }
}

fn auth_info(app: &AppHandle) -> AuthInfo {
    let Some(store) = app.try_state::<szu_core::auth::AuthStore>() else {
        return AuthInfo {
            state: "unavailable".to_string(),
            logged_in: false,
        };
    };
    let state = match store.state.lock() {
        Ok(state) => state.clone(),
        Err(_) => {
            return AuthInfo {
                state: "unavailable".to_string(),
                logged_in: false,
            };
        }
    };
    AuthInfo {
        state: auth_state_label(&state).to_string(),
        logged_in: matches!(&state, AuthState::Authenticated { .. }),
    }
}

fn auth_state_label(state: &AuthState) -> &'static str {
    match state {
        AuthState::Checking => "checking",
        AuthState::Unauthenticated => "unauthenticated",
        AuthState::Authenticating => "authenticating",
        AuthState::AwaitingProfile { .. } => "awaitingProfile",
        AuthState::Authenticated { .. } => "authenticated",
        AuthState::CredentialCaptureFailed => "credentialCaptureFailed",
        AuthState::SessionCaptureFailed => "sessionCaptureFailed",
        AuthState::CredentialStorageFailed => "credentialStorageFailed",
        AuthState::AccountMismatch => "accountMismatch",
        AuthState::SessionExpired => "sessionExpired",
        AuthState::WebViewUpdateRequired => "webViewUpdateRequired",
    }
}

fn uooc_info(app: &AppHandle) -> UoocInfo {
    let Ok(service) = szu_uooc::uooc::service(app) else {
        return UoocInfo {
            session: "unavailable".to_string(),
            logged_in: false,
        };
    };
    let session = service.snapshot().session;
    UoocInfo {
        session: session_status_label(&session).to_string(),
        logged_in: session == SessionStatus::Ready,
    }
}

fn session_status_label(session: &SessionStatus) -> &'static str {
    match session {
        SessionStatus::Checking => "checking",
        SessionStatus::Ready => "ready",
        SessionStatus::LoginRequired => "loginRequired",
        SessionStatus::NetworkError => "networkError",
        SessionStatus::LoggingIn => "loggingIn",
    }
}

fn status_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定应用数据目录".to_string())?;
    Ok(dir.join(STATUS_FILE))
}

/// 同步构建并写入状态文件。调用方应使用阻塞任务，避免 keyring I/O 卡住主线程。
pub fn write(app: &AppHandle) -> Result<(), String> {
    let path = status_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建状态目录: {error}"))?;
    }
    let status = build(app);
    let body = serde_json::to_vec_pretty(&status)
        .map_err(|error| format!("序列化集成状态失败: {error}"))?;
    write_atomic(&path, &body)
}

/// 请求一次合并刷新。800ms 内的多次事件只落盘一次。
pub fn schedule(app: &AppHandle) {
    let generation = SCHEDULE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        if SCHEDULE_GENERATION.load(Ordering::Acquire) != generation {
            return;
        }
        let write_app = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = write(&write_app) {
                log::warn!("写入外部集成状态失败: {error}");
            }
        })
        .await;
    });
}

/// 注册事件监听并做首次刷新。必须在各 store 初始化完成之后调用。
pub fn setup(app: &AppHandle) {
    for event in [
        "onboarding-state-changed",
        "onboarding-restarted",
        "auth-state-changed",
        "uooc-state-changed",
    ] {
        let handle = app.clone();
        app.listen(event, move |_| schedule(&handle));
    }

    // 事件漏掉时仍有兜底刷新；10 分钟一次，不会给 keyring 造成压力。
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(600)).await;
            schedule(&handle);
        }
    });

    schedule(app);
}

fn write_atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(STATUS_FILE);
    let tmp = path.with_file_name(format!(
        "{file_name}.{}.{}.tmp",
        std::process::id(),
        now_ms()
    ));

    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&tmp)
            .map_err(|error| format!("无法创建临时状态文件: {error}"))?;
        file.write_all(body)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("写入临时状态文件失败: {error}"))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("设置状态文件权限失败: {error}"));
        }
    }

    #[cfg(windows)]
    {
        // Windows 的 rename 不覆盖已存在文件；先删除旧文件，读方会短暂看到缺失。
        let _ = fs::remove_file(path);
    }

    fs::rename(&tmp, path).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        format!("替换状态文件失败: {error}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use szu_core::onboarding::OnboardingStatus;

    fn sample_status() -> IntegrationStatus {
        IntegrationStatus {
            schema_version: SCHEMA_VERSION,
            updated_at_ms: 1,
            started_at_ms: 1,
            pid: 42,
            app: AppInfo {
                version: "0.19.0".into(),
                platform: "linux".into(),
                arch: "x64".into(),
            },
            mcp: McpInfo {
                enabled: true,
                running: true,
                port: Some(53901),
                preferred_port: 53901,
                endpoint: Some("http://127.0.0.1:53901/mcp".into()),
            },
            onboarding: OnboardingInfo {
                available: true,
                needs_setup: Some(false),
                current_step: None,
                steps: vec![OnboardingStepInfo {
                    id: "auth".into(),
                    status: "configured".into(),
                    skippable: false,
                    revision: 1,
                }],
            },
            auth: AuthInfo {
                state: "authenticated".into(),
                logged_in: true,
            },
            uooc: UoocInfo {
                session: "ready".into(),
                logged_in: true,
            },
        }
    }

    #[test]
    fn status_json_contains_no_profile_or_secret_fields() {
        let json = serde_json::to_string(&sample_status()).unwrap();
        for forbidden in [
            "studentId",
            "student_id",
            "campusCard",
            "password",
            "cookie",
            "token",
            "profile",
        ] {
            assert!(
                !json.contains(forbidden),
                "integration.json 不应包含 {forbidden}: {json}"
            );
        }
        assert!(json.contains("\"loggedIn\":true"));
        assert!(json.contains("\"endpoint\":\"http://127.0.0.1:53901/mcp\""));
    }

    #[test]
    fn status_maps_only_string_labels_for_auth_and_uooc() {
        let status = sample_status();
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["auth"]["state"], "authenticated");
        assert_eq!(json["uooc"]["session"], "ready");
        assert_eq!(json["onboarding"]["steps"][0]["id"], "auth");
    }

    #[test]
    fn atomic_write_replaces_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATUS_FILE);
        write_atomic(&path, b"{\"first\":true}").unwrap();
        write_atomic(&path, b"{\"second\":true}").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"second\""));
        assert!(!text.contains("\"first\""));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_sets_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATUS_FILE);
        write_atomic(&path, b"{}").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn onboarding_status_labels_are_stable() {
        assert_eq!(OnboardingStatus::Pending.as_str(), "pending");
        assert_eq!(OnboardingStatus::Configured.as_str(), "configured");
        assert_eq!(OnboardingStatus::Skipped.as_str(), "skipped");
    }
}
