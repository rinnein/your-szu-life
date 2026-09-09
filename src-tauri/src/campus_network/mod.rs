mod android;
pub(crate) mod commands;
pub(crate) mod diagnostics;
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
pub(crate) mod staff_login;
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
    app.try_state::<CampusNetworkStore>().is_some_and(|store| {
        settings::dorm_settings(&store).is_ok_and(|settings| settings.enabled)
            || staff_login::keep_running_in_background(&store)
    })
}
