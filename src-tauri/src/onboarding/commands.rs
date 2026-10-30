use super::{complete_step, restart, skip_step, state_view, OnboardingStateView};
use tauri::Emitter;

#[tauri::command]
pub(crate) fn get_onboarding_state() -> Result<OnboardingStateView, String> {
    state_view()
}

#[tauri::command]
pub(crate) fn complete_onboarding_step(id: String) -> Result<OnboardingStateView, String> {
    complete_step(&id)
}

#[tauri::command]
pub(crate) fn skip_onboarding_step(id: String) -> Result<OnboardingStateView, String> {
    skip_step(&id)
}

/// 重新开始初始化向导：不重置任何设置，只把步骤进度写回 pending。
#[tauri::command]
pub(crate) fn restart_onboarding(app: tauri::AppHandle) -> Result<OnboardingStateView, String> {
    let state = restart()?;
    let _ = app.emit("onboarding-restarted", &state);
    Ok(state)
}