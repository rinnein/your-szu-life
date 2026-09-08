pub(crate) mod commands;
mod http;
mod login;
mod logout;
mod logs;
pub(crate) mod model;
pub(crate) mod monitor;
mod network;
mod operations;
mod portal;
mod service;
pub(crate) mod settings;
mod staff;
mod state;

pub(crate) use state::CampusNetworkStore;

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

#[cfg(desktop)]
pub(crate) fn keep_running_in_background(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<CampusNetworkStore>()
        .and_then(|store| settings::dorm_settings(&store).ok())
        .is_some_and(|settings| settings.enabled)
}
