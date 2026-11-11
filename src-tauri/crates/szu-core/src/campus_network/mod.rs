mod android;
pub mod commands;
pub mod diagnostics;
mod http;
mod login;
mod logout;
mod logs;
pub mod model;
pub mod monitor;
mod network;
mod operations;
mod portal;
pub mod service;
pub mod settings;
mod staff;
pub mod staff_login;
mod state;

pub use state::CampusNetworkStore;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

