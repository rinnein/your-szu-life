//! Shared WebView cookie transport. Secret data is never an IPC/MCP payload.
mod jar;
mod model;
mod scope;
pub(crate) use jar::project;
pub(crate) use jar::{
    copy_snapshot, http_provider, load as load_jar, merge_native, native_snapshot,
    serialize as serialize_jar, update_http,
};
#[cfg(any(target_os = "android", test))]
pub(crate) use jar::{merge_browser_snapshot, store_snapshot};
#[cfg(target_os = "android")]
mod android;
#[cfg(any(target_os = "android", all(test, unix)))]
mod cdp;
#[cfg(any(target_os = "android", test))]
mod native;
#[cfg(any(target_os = "android", test))]
mod ops;
#[cfg(any(target_os = "android", test))]
pub(crate) use model::Patch;
pub(crate) use model::{CookieRecord, Identity, SessionEnvelope, Snapshot};
#[cfg(any(target_os = "android", test))]
pub(crate) use model::{Extensions, PartitionKey};
pub(crate) use scope::{Kind, Scope};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Error {
    Unavailable,
    Protocol,
    UnsupportedMethod,
    UnsupportedPartition,
    InvalidMetadata,
    OutOfScope,
    WriteUncertain,
}
impl From<Error> for String {
    fn from(error: Error) -> Self {
        let code = match error {
            Error::Unavailable => "unavailable",
            Error::Protocol => "protocol",
            Error::UnsupportedMethod => "unsupportedMethod",
            Error::UnsupportedPartition => "unsupportedPartition",
            Error::InvalidMetadata => "invalidMetadata",
            Error::OutOfScope => "outOfScope",
            Error::WriteUncertain => "writeUncertain",
        };
        format!("webview-cookie:{code}")
    }
}

#[cfg(target_os = "android")]
pub(crate) fn capture(window: &tauri::WebviewWindow, scope: &Scope) -> Result<Snapshot, String> {
    window.url().map_err(|_| String::from(Error::Unavailable))?;
    android::capture(window, scope).map_err(|error| {
        log::warn!("WebView Cookie capture failed: category={error:?}");
        String::from(error)
    })
}
#[cfg(target_os = "android")]
pub(crate) fn apply(
    window: &tauri::WebviewWindow,
    scope: &Scope,
    patches: &[Patch],
) -> Result<(), String> {
    window.url().map_err(|_| String::from(Error::Unavailable))?;
    for p in patches {
        p.cookie.validate().map_err(String::from)?;
        if !scope.allows(&p.cookie) {
            return Err(Error::OutOfScope.into());
        }
    }
    android::apply(window, scope, patches).map_err(|error| {
        log::warn!("WebView Cookie apply failed: category={error:?}");
        String::from(error)
    })
}
#[cfg(target_os = "android")]
pub(crate) fn replace(
    window: &tauri::WebviewWindow,
    scope: &Scope,
    snapshot: &Snapshot,
) -> Result<(), String> {
    window.url().map_err(|_| String::from(Error::Unavailable))?;
    snapshot.validate(scope).map_err(String::from)?;
    android::replace(window, scope, snapshot).map_err(|error| {
        log::warn!("WebView Cookie replace failed: category={error:?}");
        String::from(error)
    })
}
#[cfg(target_os = "android")]
pub(crate) fn clear(window: &tauri::WebviewWindow, scope: &Scope) -> Result<(), String> {
    replace(window, scope, &Snapshot::default())
}
