mod app;
mod auth;
mod campus_network;
mod campus_payments;
#[cfg(desktop)]
mod desktop;
mod storage;

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
