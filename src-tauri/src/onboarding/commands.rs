use super::{complete_step, skip_step, state_view, OnboardingStateView};

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
