#![recursion_limit = "512"]

pub(crate) mod activity;
mod analytics;
mod app;
mod app_content;
pub(crate) mod app_paths;
pub(crate) mod appearance;
pub(crate) mod aria2;
mod auth;
mod campus_network;
mod campus_payments;
mod demo;
#[cfg(desktop)]
mod desktop;
mod embedding_model;
mod enrollment;
pub(crate) mod file_manager;
pub(crate) mod ipc_channel;
mod jxxk;
mod lms;
pub(crate) mod local_model;
pub(crate) mod logging;
pub mod mcp;
pub(crate) mod mesh;
pub(crate) mod migrations;
pub(crate) mod netproxy;
mod notification;
mod ocr;
mod oj;
mod onboarding;
#[cfg(target_os = "android")]
mod platform_tls;
mod ports;
mod preferences;
mod settings;
mod settings_sync;
mod storage;
mod todos;
mod uooc;
pub(crate) mod webview_cookies;

pub use auth::{
    model::{AccountProfileInput, AccountProfileView, AuthState, UserDataView},
    AuthStore,
};
pub use campus_network::model::{
    DormCredentialMode, DormLoginLogView, DormLoginResultView, DormLoginState, DormLoginStatusView,
    DormNetworkSettingsInput, DormNetworkSettingsView, DormPortalSessionInfo,
    NetworkConnectionState, NetworkLogoutOutcome, NetworkLogoutResultView, NetworkOverviewView,
    NetworkRegion, StaffApplyOutcome, StaffApplyResultView, StaffLoginMode, StaffLoginStatusView,
    StaffNetworkSettingsInput, StaffNetworkSettingsView, StaffPortalSessionInfo,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}

#[cfg(test)]
mod tests;
