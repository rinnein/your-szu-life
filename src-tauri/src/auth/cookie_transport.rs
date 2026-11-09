//! Private JNI result contract. Only fixed error categories leave the bridge.
use serde::Deserialize;

const LEGACY_FAILURE: &str = "legacy-cookie:";

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
enum CookieReadResponse {
    Ok {
        cookies: Vec<String>,
    },
    Error {
        backend: CookieBackend,
        code: CookieReadError,
    },
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum CookieBackend {
    Native,
    Legacy,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum CookieReadError {
    UnsupportedStore,
    InvalidMetadata,
    SnapshotUnavailable,
    NativeFailure,
}

impl CookieReadError {
    fn name(&self) -> &'static str {
        match self {
            Self::UnsupportedStore => "unsupportedStore",
            Self::InvalidMetadata => "invalidMetadata",
            Self::SnapshotUnavailable => "snapshotUnavailable",
            Self::NativeFailure => "nativeFailure",
        }
    }
}

pub(super) fn legacy_native_failure() -> String {
    format!("{LEGACY_FAILURE}nativeFailure")
}

pub(super) fn is_legacy_cookie_failure(error: &str) -> bool {
    matches!(
        error.strip_prefix(LEGACY_FAILURE),
        Some("unsupportedStore" | "invalidMetadata" | "snapshotUnavailable" | "nativeFailure")
    )
}

pub(super) fn handle_install_failure(
    store: &super::AuthStore,
    error: &str,
    recover: impl FnOnce(),
) {
    store.finish_attempt(super::model::AuthState::SessionCaptureFailed);
    // A read/write failure is not evidence that the saved session expired.
    if !is_legacy_cookie_failure(error) {
        recover();
    }
}

pub(super) fn clear_then_forget_session(
    clear: impl FnOnce() -> Result<(), String>,
    forget: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    clear()?;
    forget()
}

pub(super) fn decode_cookie_read(raw: &str) -> Result<Vec<String>, String> {
    let result: CookieReadResponse =
        serde_json::from_str(raw).map_err(|_| "Android 认证会话格式无效".to_owned())?;
    match result {
        CookieReadResponse::Ok { cookies } => Ok(cookies),
        CookieReadResponse::Error { backend, code } => {
            log::warn!(
                "Android Cookie read failed: backend={}, category={}",
                if backend == CookieBackend::Legacy {
                    "legacy"
                } else {
                    "native"
                },
                code.name()
            );
            if backend == CookieBackend::Legacy {
                Err(format!("{LEGACY_FAILURE}{}", code.name()))
            } else {
                Err("读取 Android 认证会话失败".into())
            }
        }
    }
}
