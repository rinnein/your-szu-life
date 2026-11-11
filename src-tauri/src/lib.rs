#![recursion_limit = "512"]

mod app;
#[cfg(desktop)]
mod desktop;
pub mod mcp;

pub use szu_core::auth::{
    model::{AccountProfileInput, AccountProfileView, AuthState, UserDataView},
    AuthStore,
};
pub use szu_core::campus_network::model::{
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
mod migration_tests;
#[cfg(test)]
mod tests;
