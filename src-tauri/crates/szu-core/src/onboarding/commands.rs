use super::{complete_step, restart, skip_step, state_view, OnboardingStateView};
use tauri::Emitter;

#[tauri::command]
pub fn get_onboarding_state() -> Result<OnboardingStateView, String> {
    state_view()
}

#[tauri::command]
pub fn complete_onboarding_step(
    app: tauri::AppHandle,
    id: String,
) -> Result<OnboardingStateView, String> {
    let state = complete_step(&id)?;
    let _ = app.emit("onboarding-state-changed", &state);
    Ok(state)
}

#[tauri::command]
pub fn skip_onboarding_step(
    app: tauri::AppHandle,
    id: String,
) -> Result<OnboardingStateView, String> {
    let state = skip_step(&id)?;
    let _ = app.emit("onboarding-state-changed", &state);
    Ok(state)
}

/// 重新开始初始化向导：不重置任何设置，只把步骤进度写回 pending。
#[tauri::command]
pub fn restart_onboarding(app: tauri::AppHandle) -> Result<OnboardingStateView, String> {
    let state = restart()?;
    let _ = app.emit("onboarding-restarted", &state);
    let _ = app.emit("onboarding-state-changed", &state);
    Ok(state)
}
