//! Authentication failure policy; transport belongs to webview_cookies.
pub(super) fn is_cookie_failure(error: &str) -> bool {
    error.starts_with("webview-cookie:")
}
pub(super) fn handle_install_failure(
    store: &super::AuthStore,
    error: &str,
    recover: impl FnOnce(),
) {
    store.finish_attempt(super::model::AuthState::SessionCaptureFailed);
    // A read/write failure is not evidence that the saved session expired.
    if !is_cookie_failure(error) {
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
