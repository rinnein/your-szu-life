#![recursion_limit = "512"]

mod app;
mod app_content;
pub(crate) mod aria2;
#[cfg(desktop)]
mod desktop;
mod embedding_model;
mod enrollment;
pub(crate) mod file_manager;
mod jxxk;
mod lms;
pub(crate) mod local_model;
pub mod mcp;
pub(crate) mod mesh;
pub(crate) mod migrations;
mod ocr;
mod oj;
mod settings_sync;
mod todos;
mod uooc;

// 过渡期重导出：M2 把核心域搬进 `szu-core` 后，尚未搬走的模块仍以
// `crate::<module>` 引用它们。全部功能模块下沉到各自 crate 后（M6）删除这段，
// 让引用显式写成 `szu_core::<module>`。
pub(crate) use szu_core::{
    activity, analytics, app_paths, appearance, auth, campus_network, campus_payments, demo,
    ipc_channel, logging, netproxy, notification, onboarding, ports, preferences, settings,
    storage, webview_cookies,
};
#[cfg(target_os = "android")]
pub(crate) use szu_core::platform_tls;

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
