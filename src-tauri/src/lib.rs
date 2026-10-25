#![recursion_limit = "512"]

mod app;
mod app_content;
pub(crate) mod aria2;
mod auth;
mod campus_network;
mod campus_payments;
#[cfg(desktop)]
mod desktop;
mod embedding_model;
mod enrollment;
pub(crate) mod ipc_channel;
pub(crate) mod local_model;
pub mod mcp;
pub(crate) mod migrations;
mod notification;
pub(crate) mod logging;
mod ocr;
mod oj;
#[cfg(target_os = "android")]
mod platform_tls;
mod settings;
mod storage;
mod uooc;

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
