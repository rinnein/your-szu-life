use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DormCredentialMode {
    CurrentAccount,
    Custom,
}

impl Default for DormCredentialMode {
    fn default() -> Self {
        Self::CurrentAccount
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DormFailureNotice {
    pub(crate) message: String,
    pub(crate) timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct DormNetworkSettings {
    pub(crate) enabled: bool,
    pub(crate) credential_mode: DormCredentialMode,
    pub(crate) pending_failure_notice: Option<DormFailureNotice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct DormCustomCredentials {
    pub(crate) campus_card_number: String,
    pub(crate) password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DormNetworkSettingsView {
    pub enabled: bool,
    pub credential_mode: DormCredentialMode,
    pub custom_configured: bool,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DormNetworkSettingsInput {
    pub enabled: bool,
    pub credential_mode: DormCredentialMode,
    pub campus_card_number: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DormLoginState {
    Disabled,
    Monitoring,
    Probing,
    LoggingIn,
    Online,
    Paused,
    Error,
}

impl Default for DormLoginState {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DormLoginResultView {
    pub ok: bool,
    pub already_online: bool,
    pub message: Option<String>,
    pub result: Option<i64>,
    pub ret_code: Option<i64>,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DormLoginStatusView {
    pub state: DormLoginState,
    pub ssid: Option<String>,
    pub connected: bool,
    pub portal_reachable: bool,
    pub consecutive_failures: u32,
    pub next_probe_at_ms: Option<u64>,
    pub last_result: Option<DormLoginResultView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DormLoginLogView {
    #[serde(default)]
    pub region: Option<NetworkRegion>,
    pub timestamp_ms: u64,
    pub stage: String,
    pub outcome: String,
    pub portal_reachable: bool,
    pub http_status: Option<u16>,
    pub result: Option<i64>,
    pub message: Option<String>,
    pub ret_code: Option<i64>,
    pub consecutive_failures: u32,
    pub next_probe_at_ms: Option<u64>,
    pub response: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NetworkRegion {
    None,
    Staff,
    Dormitory,
}

impl Default for NetworkRegion {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectionState {
    NoNetwork,
    Unknown,
    NotLoggedIn,
    Connected,
}

impl Default for NetworkConnectionState {
    fn default() -> Self {
        Self::NoNetwork
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DormPortalSessionInfo {
    pub online_username: Option<String>,
    pub campus_card_number: Option<String>,
    pub online_duration: Option<u64>,
    pub online_flow: Option<u64>,
    pub login_ip: Option<String>,
    pub login_started_at: Option<String>,
    pub login_expires_at: Option<String>,
    pub charset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DebugModeView {
    pub enabled: bool,
    pub log_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DebugLogEntry {
    pub timestamp_ms: u64,
    pub monitor_generation: Option<u64>,
    pub level: String,
    pub source: String,
    pub stage: String,
    pub connected: Option<bool>,
    pub transport: Option<String>,
    pub capabilities_available: Option<bool>,
    pub wifi_permission: Option<String>,
    pub location_enabled: Option<bool>,
    pub wifi_connected: Option<bool>,
    pub wifi_ip: Option<String>,
    pub wifi_has_private_172_ipv4: Option<bool>,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub portal: Option<String>,
    pub http_status: Option<u16>,
    pub elapsed_ms: Option<u64>,
    pub retry_after_ms: Option<u64>,
    pub error_kind: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkOverviewView {
    pub state: NetworkConnectionState,
    pub connected: bool,
    pub region: NetworkRegion,
    pub ssid: Option<String>,
    pub portal_reachable: bool,
    pub staff_host_reachable: bool,
    pub portal_session: Option<DormPortalSessionInfo>,
    pub staff_session: Option<StaffPortalSessionInfo>,
}

/// Display-only gateway data. Never persisted with credentials or written to logs.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StaffPortalSessionInfo {
    pub username: String,
    pub real_name: Option<String>,
    pub login_ip: String,
    pub login_ipv6: Option<String>,
    pub user_mac: Option<String>,
    pub used_bytes: Option<u64>,
    pub used_seconds: Option<u64>,
    pub login_started_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NetworkLogoutOutcome {
    Disconnected,
    AlreadyOffline,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkLogoutResultView {
    pub outcome: NetworkLogoutOutcome,
    pub region: NetworkRegion,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PortalOnlineState {
    #[default]
    Unavailable,
    Unknown,
    Offline,
    Online,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StaffLoginMode {
    #[default]
    MacAuth,
    AppAuto,
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct StaffNetworkSettings {
    pub(crate) mode: StaffLoginMode,
    pub(crate) enabled: bool,
    pub(crate) credential_mode: DormCredentialMode,
    pub(crate) revision: u64,
    pub(crate) applied_revision: Option<u64>,
    pub(crate) applied_mode: Option<StaffLoginMode>,
    // An equality stamp inside the keyring, not a password authentication/storage scheme.
    pub(crate) applied_credentials: Option<String>,
    pub(crate) pending_failure_notice: Option<DormFailureNotice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StaffNetworkSettingsView {
    pub revision: u64,
    pub mode: StaffLoginMode,
    pub enabled: bool,
    pub credential_mode: DormCredentialMode,
    pub custom_configured: bool,
    pub needs_apply: bool,
    pub applied_mode: Option<StaffLoginMode>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffNetworkSettingsInput {
    pub mode: StaffLoginMode,
    pub enabled: bool,
    pub credential_mode: DormCredentialMode,
    pub campus_card_number: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StaffLoginStatusView {
    pub state: DormLoginState,
    pub consecutive_failures: u32,
    pub next_probe_at_ms: Option<u64>,
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StaffApplyOutcome {
    Applied,
    RequiresConfirmation,
    AlreadyOnline,
    Failed,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StaffApplyResultView {
    pub outcome: StaffApplyOutcome,
    pub message: String,
    pub confirmation_token: Option<String>,
}
