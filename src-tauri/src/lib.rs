use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
    process::Command,
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{webview::cookie::Cookie, Emitter, Manager, State};
#[cfg(target_os = "android")]
use tauri_plugin_keyring_store::KeyringStore;
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

const AUTH_LOGIN_URL: &str = "https://authserver.szu.edu.cn/authserver/login";
const AUTH_PROFILE_URL: &str =
    "https://authserver.szu.edu.cn/personalInfo/personCenter/index.html#/accountsecurity";
const AUTH_HOST: &str = "authserver.szu.edu.cn";
const DORM_PORTAL_HOST: &str = "172.30.255.42";
const STAFF_NETWORK_HOST: &str = "net.szu.edu.cn";
const DORM_LOG_FILE: &str = "dorm-network-login.jsonl";
const DORM_MONITOR_TICK: Duration = Duration::from_secs(10);
#[cfg(any(target_os = "windows", target_os = "android"))]
const LOCAL_ENTRY_URL: &str = "http://tauri.localhost/";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
const LOCAL_ENTRY_URL: &str = "tauri://localhost/";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AccountProfileView {
    #[serde(rename = "studentId")]
    pub student_id: String,
    #[serde(rename = "campusCardNumber")]
    pub campus_card_number: String,
    pub name: Option<String>,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UserDataView {
    #[serde(rename = "studentId")]
    pub student_id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AccountProfileInput {
    #[serde(rename = "studentId")]
    pub student_id: String,
    #[serde(rename = "campusCardNumber")]
    pub campus_card_number: String,
    pub name: Option<String>,
    pub password: Option<String>,
}

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
struct DormFailureNotice {
    message: String,
    timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct DormNetworkSettings {
    enabled: bool,
    credential_mode: DormCredentialMode,
    pending_failure_notice: Option<DormFailureNotice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct DormCustomCredentials {
    campus_card_number: String,
    password: String,
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
    #[serde(skip_serializing, skip_deserializing)]
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
pub struct NetworkOverviewView {
    pub state: NetworkConnectionState,
    pub region: NetworkRegion,
    pub ssid: Option<String>,
    pub portal_reachable: bool,
    pub staff_host_reachable: bool,
    pub portal_session: Option<DormPortalSessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct StoredAccountProfile {
    student_id: String,
    campus_card_number: String,
    name: Option<String>,
    password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct PendingAuthCredentials {
    student_id: String,
    password: String,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CapturedCredentials {
    identifier: String,
    password: String,
}

/// Exists only while the official login page is being prepared. This is
/// serialized directly into an `eval` script for the trusted official login
/// document, never into a command result, event, log, or local app state.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LoginFormPrefill {
    identifier: String,
    password: String,
}

/// Kept exclusively in the system credential store. It is intentionally not
/// serializable through a Tauri command or event payload.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct AuthSessionSecret {
    student_id: String,
    cookies: Vec<StoredAuthCookie>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredAuthCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    expires: Option<String>,
    http_only: Option<bool>,
    secure: Option<bool>,
    same_site: Option<String>,
    partitioned: Option<bool>,
}

impl StoredAuthCookie {
    fn from_webview_cookie(cookie: Cookie<'static>) -> Option<Self> {
        let domain = cookie
            .domain()
            .unwrap_or(AUTH_HOST)
            .trim()
            .to_ascii_lowercase();
        if !is_auth_cookie_domain(&domain) || cookie.name().is_empty() || cookie.value().is_empty()
        {
            return None;
        }
        Some(Self {
            name: cookie.name().to_owned(),
            value: cookie.value().to_owned(),
            domain,
            path: cookie.path().unwrap_or("/").to_owned(),
            expires: cookie
                .expires_datetime()
                .and_then(|expires| expires.format(&Rfc2822).ok()),
            http_only: cookie.http_only(),
            secure: cookie.secure(),
            same_site: cookie.same_site().map(|site| site.to_string()),
            partitioned: cookie.partitioned(),
        })
    }

    fn to_webview_cookie(&self) -> Result<Cookie<'static>, String> {
        if !is_auth_cookie_domain(&self.domain)
            || self.name.is_empty()
            || self.name.contains([';', '\r', '\n'])
            || self.value.contains([';', '\r', '\n'])
            || !is_valid_cookie_path(&self.path)
        {
            return Err("认证会话格式无效".into());
        }
        let mut raw = format!(
            "{}={}; Domain={}; Path={}",
            self.name, self.value, self.domain, self.path
        );
        if self.secure == Some(true) {
            raw.push_str("; Secure");
        }
        if self.http_only == Some(true) {
            raw.push_str("; HttpOnly");
        }
        if let Some(same_site) = &self.same_site {
            raw.push_str("; SameSite=");
            raw.push_str(same_site);
        }
        if self.partitioned == Some(true) {
            raw.push_str("; Partitioned");
        }
        if let Some(expires) = &self.expires {
            raw.push_str("; Expires=");
            raw.push_str(expires);
        }
        Cookie::parse(raw)
            .map(Cookie::into_owned)
            .map_err(|_| "认证会话格式无效".into())
    }

    fn is_definitely_expired(&self, now: OffsetDateTime) -> bool {
        self.expires
            .as_deref()
            .and_then(|value| OffsetDateTime::parse(value, &Rfc2822).ok())
            .is_some_and(|expires| expires <= now)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AuthState {
    Checking,
    Unauthenticated,
    Authenticating,
    AwaitingProfile {
        name: Option<String>,
        student_id: Option<String>,
    },
    Authenticated {
        name: Option<String>,
        student_id: String,
    },
    CredentialCaptureFailed,
    SessionCaptureFailed,
    CredentialStorageFailed,
    AccountMismatch,
    SessionExpired,
}

impl Default for AuthState {
    fn default() -> Self {
        Self::Checking
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthAttempt {
    None,
    InitialSetup,
    ReauthenticateExisting { student_id: String },
    SessionRestore { student_id: String },
}

impl AuthAttempt {
    fn allows_qr_login(&self) -> bool {
        matches!(self, Self::ReauthenticateExisting { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretKey {
    Profile,
    PendingAuth,
    AuthSession,
    DormNetworkSettings,
    DormNetworkCredentials,
}

trait CredentialStore: Send + Sync {
    fn load_profile(&self) -> Result<Option<StoredAccountProfile>, String>;
    fn save_profile(&self, profile: &StoredAccountProfile) -> Result<(), String>;
    fn delete_profile(&self) -> Result<(), String>;
    fn load_pending(&self) -> Result<Option<PendingAuthCredentials>, String>;
    fn save_pending(&self, pending: &PendingAuthCredentials) -> Result<(), String>;
    fn delete_pending(&self) -> Result<(), String>;
    fn load_session(&self) -> Result<Option<AuthSessionSecret>, String>;
    fn save_session(&self, session: &AuthSessionSecret) -> Result<(), String>;
    fn delete_session(&self) -> Result<(), String>;
    fn load_dorm_settings(&self) -> Result<Option<DormNetworkSettings>, String>;
    fn save_dorm_settings(&self, settings: &DormNetworkSettings) -> Result<(), String>;
    fn load_dorm_credentials(&self) -> Result<Option<DormCustomCredentials>, String>;
    fn save_dorm_credentials(&self, credentials: &DormCustomCredentials) -> Result<(), String>;
}

struct PlatformCredentialStore;

impl PlatformCredentialStore {
    #[cfg(not(target_os = "android"))]
    fn entry(key: SecretKey) -> Result<keyring::Entry, String> {
        let name = match key {
            SecretKey::Profile => "profile",
            SecretKey::PendingAuth => "pending-auth",
            SecretKey::AuthSession => "auth-session",
            SecretKey::DormNetworkSettings => "dorm-network-settings",
            SecretKey::DormNetworkCredentials => "dorm-network-credentials",
        };
        keyring::Entry::new("in.rinne.your-szu-life", name).map_err(|e| e.to_string())
    }

    #[cfg(not(target_os = "android"))]
    fn load<T: for<'a> Deserialize<'a>>(key: SecretKey) -> Result<Option<T>, String> {
        match Self::entry(key)?.get_password() {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|e| e.to_string()),
            Err(_) => Ok(None),
        }
    }

    #[cfg(not(target_os = "android"))]
    fn save<T: Serialize>(key: SecretKey, value: &T) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|e| e.to_string())?;
        Self::entry(key)?
            .set_password(&raw)
            .map_err(|e| e.to_string())
    }

    #[cfg(not(target_os = "android"))]
    fn delete(key: SecretKey) -> Result<(), String> {
        let _ = Self::entry(key)?.delete_credential();
        Ok(())
    }
}

#[cfg(target_os = "android")]
fn keyring_store() -> KeyringStore {
    KeyringStore::new("in.rinne.your-szu-life")
}

#[cfg(target_os = "android")]
fn android_secret_name(key: SecretKey) -> &'static str {
    match key {
        SecretKey::Profile => "profile",
        SecretKey::PendingAuth => "pending-auth",
        SecretKey::AuthSession => "auth-session",
        SecretKey::DormNetworkSettings => "dorm-network-settings",
        SecretKey::DormNetworkCredentials => "dorm-network-credentials",
    }
}

impl CredentialStore for PlatformCredentialStore {
    #[cfg(not(target_os = "android"))]
    fn load_profile(&self) -> Result<Option<StoredAccountProfile>, String> {
        Self::load(SecretKey::Profile)
    }
    #[cfg(not(target_os = "android"))]
    fn save_profile(&self, profile: &StoredAccountProfile) -> Result<(), String> {
        Self::save(SecretKey::Profile, profile)
    }
    #[cfg(not(target_os = "android"))]
    fn delete_profile(&self) -> Result<(), String> {
        Self::delete(SecretKey::Profile)
    }
    #[cfg(not(target_os = "android"))]
    fn load_pending(&self) -> Result<Option<PendingAuthCredentials>, String> {
        Self::load(SecretKey::PendingAuth)
    }
    #[cfg(not(target_os = "android"))]
    fn save_pending(&self, pending: &PendingAuthCredentials) -> Result<(), String> {
        Self::save(SecretKey::PendingAuth, pending)
    }
    #[cfg(not(target_os = "android"))]
    fn delete_pending(&self) -> Result<(), String> {
        Self::delete(SecretKey::PendingAuth)
    }
    #[cfg(not(target_os = "android"))]
    fn load_session(&self) -> Result<Option<AuthSessionSecret>, String> {
        Self::load(SecretKey::AuthSession)
    }
    #[cfg(not(target_os = "android"))]
    fn save_session(&self, session: &AuthSessionSecret) -> Result<(), String> {
        Self::save(SecretKey::AuthSession, session)
    }
    #[cfg(not(target_os = "android"))]
    fn delete_session(&self) -> Result<(), String> {
        Self::delete(SecretKey::AuthSession)
    }

    #[cfg(not(target_os = "android"))]
    fn load_dorm_settings(&self) -> Result<Option<DormNetworkSettings>, String> {
        Self::load(SecretKey::DormNetworkSettings)
    }
    #[cfg(not(target_os = "android"))]
    fn save_dorm_settings(&self, settings: &DormNetworkSettings) -> Result<(), String> {
        Self::save(SecretKey::DormNetworkSettings, settings)
    }
    #[cfg(not(target_os = "android"))]
    fn load_dorm_credentials(&self) -> Result<Option<DormCustomCredentials>, String> {
        Self::load(SecretKey::DormNetworkCredentials)
    }
    #[cfg(not(target_os = "android"))]
    fn save_dorm_credentials(&self, credentials: &DormCustomCredentials) -> Result<(), String> {
        Self::save(SecretKey::DormNetworkCredentials, credentials)
    }

    #[cfg(target_os = "android")]
    fn load_profile(&self) -> Result<Option<StoredAccountProfile>, String> {
        load_android_secret(SecretKey::Profile)
    }
    #[cfg(target_os = "android")]
    fn save_profile(&self, profile: &StoredAccountProfile) -> Result<(), String> {
        save_android_secret(SecretKey::Profile, profile)
    }
    #[cfg(target_os = "android")]
    fn delete_profile(&self) -> Result<(), String> {
        delete_android_secret(SecretKey::Profile)
    }
    #[cfg(target_os = "android")]
    fn load_pending(&self) -> Result<Option<PendingAuthCredentials>, String> {
        load_android_secret(SecretKey::PendingAuth)
    }
    #[cfg(target_os = "android")]
    fn save_pending(&self, pending: &PendingAuthCredentials) -> Result<(), String> {
        save_android_secret(SecretKey::PendingAuth, pending)
    }
    #[cfg(target_os = "android")]
    fn delete_pending(&self) -> Result<(), String> {
        delete_android_secret(SecretKey::PendingAuth)
    }
    #[cfg(target_os = "android")]
    fn load_session(&self) -> Result<Option<AuthSessionSecret>, String> {
        load_android_secret(SecretKey::AuthSession)
    }
    #[cfg(target_os = "android")]
    fn save_session(&self, session: &AuthSessionSecret) -> Result<(), String> {
        save_android_secret(SecretKey::AuthSession, session)
    }
    #[cfg(target_os = "android")]
    fn delete_session(&self) -> Result<(), String> {
        delete_android_secret(SecretKey::AuthSession)
    }
    #[cfg(target_os = "android")]
    fn load_dorm_settings(&self) -> Result<Option<DormNetworkSettings>, String> {
        load_android_secret(SecretKey::DormNetworkSettings)
    }
    #[cfg(target_os = "android")]
    fn save_dorm_settings(&self, settings: &DormNetworkSettings) -> Result<(), String> {
        save_android_secret(SecretKey::DormNetworkSettings, settings)
    }
    #[cfg(target_os = "android")]
    fn load_dorm_credentials(&self) -> Result<Option<DormCustomCredentials>, String> {
        load_android_secret(SecretKey::DormNetworkCredentials)
    }
    #[cfg(target_os = "android")]
    fn save_dorm_credentials(&self, credentials: &DormCustomCredentials) -> Result<(), String> {
        save_android_secret(SecretKey::DormNetworkCredentials, credentials)
    }
}

#[cfg(target_os = "android")]
fn load_android_secret<T: for<'a> Deserialize<'a>>(key: SecretKey) -> Result<Option<T>, String> {
    keyring_store()
        .get_password(android_secret_name(key))
        .map_err(|e| e.to_string())
        .and_then(|value| {
            value
                .map(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()))
                .transpose()
        })
}

#[cfg(target_os = "android")]
fn save_android_secret<T: Serialize>(key: SecretKey, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string(value).map_err(|e| e.to_string())?;
    keyring_store()
        .set_password(android_secret_name(key), &raw)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn delete_android_secret(key: SecretKey) -> Result<(), String> {
    keyring_store()
        .delete(android_secret_name(key))
        .map_err(|e| e.to_string())
}

pub struct AuthStore {
    state: Mutex<AuthState>,
    attempt: Mutex<AuthAttempt>,
    profile: Mutex<Option<StoredAccountProfile>>,
    pending: Mutex<Option<PendingAuthCredentials>>,
    captured: Mutex<Option<CapturedCredentials>>,
    login_prefill: Mutex<Option<LoginFormPrefill>>,
    resume_url: Mutex<Option<tauri::Url>>,
    credentials: PlatformCredentialStore,
    dorm_settings: Mutex<Option<DormNetworkSettings>>,
    dorm_status: Mutex<DormLoginStatusView>,
    dorm_logs: Mutex<VecDeque<DormLoginLogView>>,
    dorm_login_in_flight: Mutex<bool>,
    dorm_monitor: Mutex<Option<mpsc::Sender<()>>>,
    dorm_last_fingerprint: Mutex<Option<String>>,
    network_overview: Mutex<NetworkOverviewView>,
}

fn profile_view(profile: &StoredAccountProfile) -> AccountProfileView {
    AccountProfileView {
        student_id: profile.student_id.clone(),
        campus_card_number: profile.campus_card_number.clone(),
        name: profile.name.clone(),
        has_password: !profile.password.is_empty(),
    }
}

fn user_data_view(
    profile: Option<&StoredAccountProfile>,
    pending: Option<&PendingAuthCredentials>,
) -> Option<UserDataView> {
    profile
        .map(|profile| UserDataView {
            student_id: profile.student_id.clone(),
            name: profile.name.clone(),
        })
        .or_else(|| {
            pending.map(|pending| UserDataView {
                student_id: pending.student_id.clone(),
                name: pending.name.clone(),
            })
        })
}

fn login_form_prefill(
    profile: Option<&StoredAccountProfile>,
    pending: Option<&PendingAuthCredentials>,
) -> Option<LoginFormPrefill> {
    profile
        .map(|profile| LoginFormPrefill {
            identifier: profile.student_id.clone(),
            password: profile.password.clone(),
        })
        .or_else(|| {
            pending.map(|pending| LoginFormPrefill {
                identifier: pending.student_id.clone(),
                password: pending.password.clone(),
            })
        })
}

fn emit_state(app: &tauri::AppHandle, state: AuthState) {
    let _ = app.emit("auth-state-changed", state);
}

fn auth_probe_script() -> &'static str {
    r#"(()=>{window.__SZU_AUTH_PROBE__=null;(async()=>{try{const r=await fetch('/personalInfo/common/getUserConf',{credentials:'include'});const t=await r.text();window.__SZU_AUTH_PROBE__={ok:r.ok,status:r.status,url:r.url,text:t}}catch(e){window.__SZU_AUTH_PROBE__={ok:false,error:String(e)}}})();return null})()"#
}

fn auth_probe_result_script() -> &'static str {
    "window.__SZU_AUTH_PROBE__"
}

fn auth_login_capture_script(allow_qr_login: bool, prefill: Option<&LoginFormPrefill>) -> String {
    let saved_credentials = prefill.map_or_else(
        || "null".to_owned(),
        |credentials| serde_json::to_string(credentials).expect("login prefill is serializable"),
    );
    format!(
        r#"
(() => {{
  if (location.protocol !== 'https:' || location.host !== '{AUTH_HOST}' ||
      !location.pathname.startsWith('/authserver/login') || window.__SZU_LOGIN_CAPTURE_INSTALLED__) return;
  window.__SZU_LOGIN_CAPTURE_INSTALLED__ = true;

  const qrAllowed = {allow_qr_login};
  const savedCredentials = {saved_credentials};
  const autofilledForms = new WeakSet();
  let submitting = false;
  let allowOfficialSubmit = false;
  let candidateCapture = null;

  // The official page retains hidden templates with duplicated IDs. Only the
  // form copied into .login-main is live, so never fall back to global fields.
  const loginForm = () => document.querySelector('.login-main #pwdFromId');
  const accountInput = () => document.querySelector('.login-main .m-account #username');
  const passwordInput = () => document.querySelector('.login-main .m-account #password');
  const loginButton = () => document.querySelector('.login-main #login_submit');
  const rememberInput = () => document.querySelector('.login-main #rememberMe');
  const isActiveFormTarget = target => {{
    const form = loginForm();
    return !!form && target instanceof Node && form.contains(target);
  }};
  const isActiveLoginButton = target => {{
    if (!(target instanceof Element)) return false;
    const button = target.closest('.login-main #login_submit');
    return !!button && button === loginButton();
  }};
  const setCaptureError = message => {{
    const error = document.querySelector('.login-main #showErrorTip, .login-main #pwdErrorTip, .login-main .m-account .error-tip');
    if (error) {{ error.textContent = message; error.style.display = 'block'; }}
  }};
  const applySavedCredentials = () => {{
    if (!savedCredentials) return;
    const form = loginForm();
    const account = accountInput();
    const password = passwordInput();
    if (!form || !account || !password || autofilledForms.has(form)) return;
    // The values are only injected into the active account/password form on
    // the HTTPS official login origin. Mark this specific form so the user can
    // still deliberately edit or clear either field afterwards.
    account.value = savedCredentials.identifier;
    password.value = savedCredentials.password;
    for (const input of [account, password]) {{
      input.dispatchEvent(new Event('input', {{ bubbles: true }}));
      input.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }}
    autofilledForms.add(form);
  }};
  const disableQrLogin = () => {{
    if (qrAllowed) return;
    for (const area of document.querySelectorAll('.qrcode-middle')) {{
      area.setAttribute('aria-disabled', 'true');
      if (getComputedStyle(area).position === 'static') area.style.position = 'relative';
      if (area.querySelector('[data-szu-app-qr-disabled]')) continue;
      const overlay = document.createElement('div');
      overlay.dataset.szuAppQrDisabled = 'true';
      overlay.setAttribute('role', 'note');
      overlay.textContent = '二维码登录已禁用\n请使用学号/校园卡号和密码登录，以便完成账户设置';
      Object.assign(overlay.style, {{
        position: 'absolute', inset: '0', zIndex: '2147483647', display: 'flex',
        alignItems: 'center', justifyContent: 'center', padding: '20px', boxSizing: 'border-box',
        textAlign: 'center', whiteSpace: 'pre-line', color: '#6b2b2b', background: 'rgba(255,248,248,.97)',
        border: '1px solid #e6b6b6', borderRadius: '6px', fontSize: '14px', lineHeight: '1.6',
        fontWeight: '600', cursor: 'not-allowed', pointerEvents: 'auto'
      }});
      area.append(overlay);
    }}
  }};
  const prepare = () => {{
    disableQrLogin();
    const form = loginForm();
    const account = accountInput();
    const password = passwordInput();
    const button = loginButton();
    const remember = rememberInput();
    if (!form || !account || !password || !button) return null;
    if (remember && !remember.checked) {{
      remember.checked = true;
      remember.dispatchEvent(new Event('input', {{ bubbles: true }}));
      remember.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }}
    if (!account.value.trim() || !password.value) return null;
    return {{ identifier: account.value.trim(), password: password.value }};
  }};
  const captureCandidate = async () => {{
    const capture = prepare();
    if (!capture) return false;
    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (typeof invoke !== 'function') return false;
    const key = `${{capture.identifier}}\u0000${{capture.password}}`;
    if (candidateCapture?.key === key) return candidateCapture.promise;
    const promise = invoke('capture_auth_credentials', {{ capture }}).then(() => true).catch(() => false);
    candidateCapture = {{ key, promise }};
    const captured = await promise;
    if (!captured && candidateCapture?.key === key) candidateCapture = null;
    return captured;
  }};
  const continueOfficialLogin = button => {{
    if (!button || button !== loginButton() || typeof window.startLogin !== 'function') {{
      throw new Error('active login button or startLogin unavailable');
    }}
    allowOfficialSubmit = true;
    window.startLogin(button);
  }};
  const bridgeSubmit = async () => {{
    if (submitting) return;
    const capture = prepare();
    const button = loginButton();
    if (!capture || !button) {{
      setCaptureError('未能读取当前账号密码表单，请刷新后重新输入');
      return;
    }}
    submitting = true;
    button.setAttribute('aria-disabled', 'true');
    try {{
      if (!await captureCandidate()) throw new Error('credential capture failed');
      continueOfficialLogin(button);
      setTimeout(() => {{ allowOfficialSubmit = false; submitting = false; button.removeAttribute('aria-disabled'); }}, 3000);
    }} catch (_) {{
      submitting = false;
      button.removeAttribute('aria-disabled');
      setCaptureError('未能保存本次登录凭据，请刷新后重新输入');
    }}
  }};
  const intercept = event => {{
    if (allowOfficialSubmit) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    void bridgeSubmit();
  }};
  const interceptEnter = event => {{
    if ((event.key === 'Enter' || event.keyCode === 13) && isActiveFormTarget(event.target)) intercept(event);
  }};

  applySavedCredentials();
  prepare();
  const refreshLoginForm = () => {{ applySavedCredentials(); prepare(); }};
  new MutationObserver(refreshLoginForm).observe(document.documentElement, {{ childList: true, subtree: true }});
  setInterval(refreshLoginForm, 500);
  document.addEventListener('click', event => {{
    if (!qrAllowed && event.target instanceof Element && event.target.closest('.qrcode-middle, #qrLogin_a, #qrLoginForm')) {{
      event.preventDefault(); event.stopImmediatePropagation(); return;
    }}
    if (isActiveLoginButton(event.target)) intercept(event);
  }}, true);
  document.addEventListener('keydown', interceptEnter, true);
  document.addEventListener('keyup', interceptEnter, true);
  document.addEventListener('submit', event => {{
    if (!qrAllowed && event.target instanceof Element && event.target.matches('#qrLoginForm')) {{
      event.preventDefault(); event.stopImmediatePropagation(); return;
    }}
    if (event.target === loginForm()) intercept(event);
  }}, true);
}})()
"#
    )
}

fn is_auth_login_url(raw: &str) -> bool {
    let Ok(url) = raw.parse::<tauri::Url>() else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == Some(AUTH_HOST)
        && url.path().starts_with("/authserver/login")
}

fn is_authserver_url(raw: &str) -> bool {
    let Ok(url) = raw.parse::<tauri::Url>() else {
        return false;
    };
    url.scheme() == "https" && url.host_str() == Some(AUTH_HOST)
}

fn is_auth_profile_url(raw: &str) -> bool {
    let Ok(url) = raw.parse::<tauri::Url>() else {
        return false;
    };
    let path = url.path();
    is_authserver_url(raw) && (path == "/personalInfo" || path.starts_with("/personalInfo/"))
}

fn is_auth_cookie_domain(domain: &str) -> bool {
    // `cookies_for_url` already returns only cookies applicable to the official
    // host. Keep the persisted allow-list equally narrow so a corrupted secret
    // cannot make us inject unrelated SZU subdomain cookies on restore.
    matches!(
        domain.trim_start_matches('.').trim_end_matches('.'),
        AUTH_HOST | "szu.edu.cn"
    )
}

fn is_valid_cookie_path(path: &str) -> bool {
    path.starts_with('/') && !path.contains([';', '\r', '\n'])
}

#[cfg(target_os = "android")]
fn auth_cookie_url(cookie: &StoredAuthCookie) -> Result<tauri::Url, String> {
    if !is_auth_cookie_domain(&cookie.domain) || !is_valid_cookie_path(&cookie.path) {
        return Err("认证会话格式无效".into());
    }
    format!("https://{AUTH_HOST}{}", cookie.path)
        .parse::<tauri::Url>()
        .map_err(|_| "认证会话格式无效".into())
}

fn decode_callback_payload(payload: &str) -> Option<serde_json::Value> {
    let mut parsed = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    if let Some(encoded) = parsed.as_str() {
        parsed = serde_json::from_str(encoded).ok()?;
    }
    Some(parsed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifiedUserIdentity {
    student_id: String,
    name: Option<String>,
}

fn normalized_user_name(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty() && value.chars().count() <= 80).then(|| value.to_owned())
    })
}

fn probe_identity(payload: &str) -> Option<VerifiedUserIdentity> {
    let parsed = decode_callback_payload(payload)?;
    let text = parsed.get("text")?.as_str()?;
    let user_conf = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let student_id = ["/datas/uid", "/data/uid"].into_iter().find_map(|path| {
        user_conf
            .pointer(path)
            .and_then(|value| value.as_str())
            .filter(|id| is_student_id(id))
            .map(str::to_owned)
    })?;

    // SZU's current response exposes the name as `res.data.cn`. Keep that
    // exact field first, while accepting the known compatibility aliases. Do
    // not recursively scan or persist the raw official response.
    let name = [
        "/data/cn",
        "/datas/cn",
        "/datas/name",
        "/datas/userName",
        "/datas/username",
        "/datas/user_name",
        "/datas/realName",
        "/datas/real_name",
        "/datas/xm",
        "/datas/user/name",
        "/datas/user/userName",
        "/datas/user/realName",
        "/datas/userInfo/name",
        "/datas/userInfo/userName",
        "/datas/userInfo/realName",
        "/datas/userInfo/xm",
    ]
    .into_iter()
    .find_map(|path| {
        normalized_user_name(user_conf.pointer(path).and_then(|value| value.as_str()))
    });

    Some(VerifiedUserIdentity { student_id, name })
}

fn auth_probe_reached_verified_endpoint(payload: &str) -> bool {
    let Some(parsed) = decode_callback_payload(payload) else {
        return false;
    };
    parsed
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        && parsed
            .get("status")
            .and_then(|value| value.as_u64())
            .is_some_and(|status| (200..300).contains(&status))
        && parsed
            .get("url")
            .and_then(|value| value.as_str())
            .is_some_and(is_auth_profile_url)
}

fn auth_probe_succeeded(payload: &str) -> bool {
    auth_probe_reached_verified_endpoint(payload) && probe_identity(payload).is_some()
}

fn is_student_id(value: &str) -> bool {
    value.len() == 10 && value.chars().all(|character| character.is_ascii_digit())
}

fn is_campus_card_number(value: &str) -> bool {
    value.len() == 6 && value.chars().all(|character| character.is_ascii_digit())
}

impl AccountProfileInput {
    fn validate(&self, current_password: Option<&str>) -> Result<(), String> {
        if !is_student_id(&self.student_id) {
            return Err("学号必须是 10 位数字".into());
        }
        if !is_campus_card_number(&self.campus_card_number) {
            return Err("校园卡号必须是 6 位数字".into());
        }
        if self
            .password
            .as_deref()
            .unwrap_or(current_password.unwrap_or(""))
            .is_empty()
        {
            return Err("密码不能为空".into());
        }
        Ok(())
    }
}

fn session_matches_account(
    session: &AuthSessionSecret,
    profile: Option<&StoredAccountProfile>,
    pending: Option<&PendingAuthCredentials>,
) -> bool {
    let expected_student_id = profile
        .map(|profile| profile.student_id.as_str())
        .or_else(|| pending.map(|pending| pending.student_id.as_str()));
    expected_student_id == Some(session.student_id.as_str())
        && !session.cookies.is_empty()
        && !session
            .cookies
            .iter()
            .all(|cookie| cookie.is_definitely_expired(OffsetDateTime::now_utc()))
}

fn capture_auth_session(
    window: &tauri::WebviewWindow,
    student_id: String,
) -> Result<AuthSessionSecret, String> {
    let login_url = AUTH_LOGIN_URL
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    let profile_url = AUTH_PROFILE_URL
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();
    let mut stored = Vec::new();
    for cookie in window
        .cookies_for_url(login_url)
        .map_err(|error| error.to_string())?
        .into_iter()
        .chain(
            window
                .cookies_for_url(profile_url)
                .map_err(|error| error.to_string())?
                .into_iter(),
        )
    {
        let Some(cookie) = StoredAuthCookie::from_webview_cookie(cookie) else {
            continue;
        };
        let key = format!("{}\u{0}{}\u{0}{}", cookie.name, cookie.domain, cookie.path);
        if seen.insert(key) {
            stored.push(cookie);
        }
    }
    if stored.is_empty() {
        return Err("未读取到官方认证会话".into());
    }
    Ok(AuthSessionSecret {
        student_id,
        cookies: stored,
    })
}

#[cfg(not(target_os = "android"))]
fn install_auth_cookie(
    window: &tauri::WebviewWindow,
    cookie: &StoredAuthCookie,
) -> Result<(), String> {
    window
        .set_cookie(cookie.to_webview_cookie()?)
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "android")]
fn install_auth_cookie(
    window: &tauri::WebviewWindow,
    cookie: &StoredAuthCookie,
) -> Result<(), String> {
    use jni::objects::JValue;
    use std::sync::mpsc;

    let cookie_header = cookie.to_webview_cookie()?.to_string();
    let target_url = auth_cookie_url(cookie)?.to_string();
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |environment, _, _| {
                let result = (|| -> Result<(), String> {
                    let manager = environment
                        .call_static_method(
                            "android/webkit/CookieManager",
                            "getInstance",
                            "()Landroid/webkit/CookieManager;",
                            &[],
                        )
                        .map_err(|error| error.to_string())?
                        .l()
                        .map_err(|error| error.to_string())?;
                    let url = environment
                        .new_string(target_url)
                        .map_err(|error| error.to_string())?;
                    let header = environment
                        .new_string(cookie_header)
                        .map_err(|error| error.to_string())?;
                    environment
                        .call_method(
                            &manager,
                            "setCookie",
                            "(Ljava/lang/String;Ljava/lang/String;)V",
                            &[JValue::Object(&url), JValue::Object(&header)],
                        )
                        .map_err(|error| error.to_string())?;
                    environment
                        .call_method(&manager, "flush", "()V", &[])
                        .map_err(|error| error.to_string())?;
                    Ok(())
                })();
                let _ = sender.send(result);
            });
        })
        .map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "写入 Android 认证会话超时".to_string())?
}

fn restore_auth_session(app: &tauri::AppHandle, session: &AuthSessionSecret) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
    window
        .clear_all_browsing_data()
        .map_err(|error| error.to_string())?;
    for cookie in &session.cookies {
        install_auth_cookie(&window, cookie)?;
    }
    window
        .navigate(
            AUTH_PROFILE_URL
                .parse::<tauri::Url>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
}

fn clear_webview_session(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .clear_all_browsing_data()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn resume_application_window(app: &tauri::AppHandle, store: &AuthStore) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let resume_url = store
        .resume_url
        .lock()
        .ok()
        .and_then(|mut url| url.take())
        .unwrap_or_else(|| LOCAL_ENTRY_URL.parse().expect("valid local entry URL"));
    let _ = window.navigate(resume_url);
}

fn set_terminal_state(app: &tauri::AppHandle, store: &AuthStore, state: AuthState) {
    if let Ok(mut current) = store.state.lock() {
        *current = state.clone();
    }
    if let Ok(mut attempt) = store.attempt.lock() {
        *attempt = AuthAttempt::None;
    }
    if let Ok(mut captured) = store.captured.lock() {
        *captured = None;
    }
    if let Ok(mut prefill) = store.login_prefill.lock() {
        *prefill = None;
    }
    emit_state(app, state);
    resume_application_window(app, store);
}

fn session_failure(app: &tauri::AppHandle, store: &AuthStore) {
    let _ = store.credentials.delete_session();
    let _ = clear_webview_session(app);
    set_terminal_state(app, store, AuthState::SessionCaptureFailed);
}

fn session_restore_expired(app: &tauri::AppHandle, store: &AuthStore) {
    let _ = store.credentials.delete_session();
    let _ = clear_webview_session(app);
    set_terminal_state(app, store, AuthState::SessionExpired);
}

fn credential_capture_failure(app: &tauri::AppHandle, store: &AuthStore) {
    // Even a failed first-time setup may already have obtained official
    // cookies. The keyring must remain the only durable session copy.
    let _ = store.credentials.delete_session();
    let _ = clear_webview_session(app);
    set_terminal_state(app, store, AuthState::CredentialCaptureFailed);
}

fn complete_authentication(app: tauri::AppHandle, payload: String) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    if !matches!(
        store.state.lock().ok().as_deref(),
        Some(AuthState::Authenticating)
    ) || !auth_probe_reached_verified_endpoint(&payload)
    {
        return;
    }
    if !auth_probe_succeeded(&payload) {
        credential_capture_failure(&app, &store);
        return;
    }
    let Some(identity) = probe_identity(&payload) else {
        credential_capture_failure(&app, &store);
        return;
    };
    let student_id = identity.student_id;
    let authenticated_name = identity.name;

    let profile = store
        .profile
        .lock()
        .ok()
        .and_then(|profile| profile.clone());
    let pending = store
        .pending
        .lock()
        .ok()
        .and_then(|pending| pending.clone());
    let attempt = store
        .attempt
        .lock()
        .map(|attempt| attempt.clone())
        .unwrap_or(AuthAttempt::None);
    let captured = store
        .captured
        .lock()
        .ok()
        .and_then(|mut captured| captured.take());

    enum Completion {
        Existing(StoredAccountProfile),
        Pending(PendingAuthCredentials),
        NewProfile(StoredAccountProfile),
        NewPending(PendingAuthCredentials),
    }

    let completion = match (profile, pending, attempt) {
        (
            Some(mut profile),
            _,
            AuthAttempt::ReauthenticateExisting { .. } | AuthAttempt::SessionRestore { .. },
        ) => {
            if profile.student_id != student_id {
                let _ = store.credentials.delete_session();
                let _ = clear_webview_session(&app);
                set_terminal_state(&app, &store, AuthState::AccountMismatch);
                return;
            }
            if authenticated_name.is_some() {
                profile.name = authenticated_name.clone();
            }
            Completion::Existing(profile)
        }
        (_, Some(mut pending), AuthAttempt::InitialSetup | AuthAttempt::SessionRestore { .. }) => {
            if pending.student_id != student_id {
                let _ = store.credentials.delete_session();
                let _ = clear_webview_session(&app);
                set_terminal_state(&app, &store, AuthState::AccountMismatch);
                return;
            }
            if authenticated_name.is_some() {
                pending.name = authenticated_name.clone();
            }
            Completion::Pending(pending)
        }
        (None, None, AuthAttempt::InitialSetup) => {
            let Some(capture) = captured else {
                set_terminal_state(&app, &store, AuthState::CredentialCaptureFailed);
                return;
            };
            if capture.password.is_empty() {
                set_terminal_state(&app, &store, AuthState::CredentialCaptureFailed);
                return;
            }
            if is_campus_card_number(&capture.identifier) {
                Completion::NewProfile(StoredAccountProfile {
                    student_id: student_id.clone(),
                    campus_card_number: capture.identifier,
                    name: authenticated_name.clone(),
                    password: capture.password,
                })
            } else if is_student_id(&capture.identifier) && capture.identifier == student_id {
                Completion::NewPending(PendingAuthCredentials {
                    student_id: capture.identifier,
                    password: capture.password,
                    name: authenticated_name.clone(),
                })
            } else {
                credential_capture_failure(&app, &store);
                return;
            }
        }
        _ => {
            credential_capture_failure(&app, &store);
            return;
        }
    };

    let Some(window) = app.get_webview_window("main") else {
        session_failure(&app, &store);
        return;
    };
    let Ok(session) = capture_auth_session(&window, student_id.clone()) else {
        session_failure(&app, &store);
        return;
    };
    if store.credentials.save_session(&session).is_err() || clear_webview_session(&app).is_err() {
        session_failure(&app, &store);
        return;
    }

    let terminal = match completion {
        Completion::Existing(profile) => {
            if store.credentials.save_profile(&profile).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            if let Ok(mut stored) = store.profile.lock() {
                *stored = Some(profile.clone());
            }
            AuthState::Authenticated {
                name: profile.name,
                student_id: profile.student_id,
            }
        }
        Completion::Pending(pending) => {
            if store.credentials.save_pending(&pending).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            if let Ok(mut stored) = store.pending.lock() {
                *stored = Some(pending.clone());
            }
            AuthState::AwaitingProfile {
                name: pending.name,
                student_id: Some(pending.student_id),
            }
        }
        Completion::NewProfile(profile) => {
            if store.credentials.save_profile(&profile).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            let _ = store.credentials.delete_pending();
            if let Ok(mut stored) = store.profile.lock() {
                *stored = Some(profile.clone());
            }
            if let Ok(mut stored) = store.pending.lock() {
                *stored = None;
            }
            AuthState::Authenticated {
                name: profile.name,
                student_id: profile.student_id,
            }
        }
        Completion::NewPending(pending) => {
            if store.credentials.save_pending(&pending).is_err() {
                let _ = store.credentials.delete_session();
                set_terminal_state(&app, &store, AuthState::CredentialStorageFailed);
                return;
            }
            if let Ok(mut stored) = store.pending.lock() {
                *stored = Some(pending.clone());
            }
            AuthState::AwaitingProfile {
                name: pending.name,
                student_id: Some(pending.student_id),
            }
        }
    };
    set_terminal_state(&app, &store, terminal);
}

fn record_credential_capture_failure(category: &str) {
    // Never log the captured identifier or password. Categories are useful for
    // diagnosing remote-page drift and capability failures without secrets.
    eprintln!("authentication credential capture rejected: {category}");
}

#[tauri::command]
fn capture_auth_credentials(
    window: tauri::WebviewWindow,
    capture: CapturedCredentials,
    store: State<'_, AuthStore>,
) -> Result<(), String> {
    let page_url = match window.url() {
        Ok(url) => url.to_string(),
        Err(_) => {
            record_credential_capture_failure("page-url-unavailable");
            return Err("无法验证认证页面".into());
        }
    };
    if !is_auth_login_url(&page_url) {
        record_credential_capture_failure("untrusted-page");
        return Err("仅可在深大统一认证账号密码页面保存凭据".into());
    }
    let state = store.state.lock().map_err(|_| "状态锁失败")?.clone();
    let attempt = store.attempt.lock().map_err(|_| "状态锁失败")?.clone();
    if !matches!(state, AuthState::Authenticating) || !matches!(attempt, AuthAttempt::InitialSetup)
    {
        return Ok(());
    }
    if !(is_student_id(&capture.identifier) || is_campus_card_number(&capture.identifier)) {
        record_credential_capture_failure("invalid-identifier");
        return Err("请输入 10 位学号或 6 位校园卡号".into());
    }
    if capture.password.is_empty() {
        record_credential_capture_failure("empty-password");
        return Err("密码不能为空".into());
    }
    *store.captured.lock().map_err(|_| "状态锁失败")? = Some(capture);
    Ok(())
}

fn start_session_restore(
    app: tauri::AppHandle,
    store: &AuthStore,
    session: AuthSessionSecret,
) -> Result<AuthState, String> {
    *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::Authenticating;
    *store.attempt.lock().map_err(|_| "状态锁失败")? = AuthAttempt::SessionRestore {
        student_id: session.student_id.clone(),
    };
    if let Err(_) = restore_auth_session(&app, &session) {
        let _ = store.credentials.delete_session();
        let _ = clear_webview_session(&app);
        let next = if store.profile.lock().map_err(|_| "状态锁失败")?.is_some()
            || store.pending.lock().map_err(|_| "状态锁失败")?.is_some()
        {
            AuthState::SessionExpired
        } else {
            AuthState::Unauthenticated
        };
        *store.state.lock().map_err(|_| "状态锁失败")? = next.clone();
        *store.attempt.lock().map_err(|_| "状态锁失败")? = AuthAttempt::None;
        emit_state(&app, next.clone());
        return Ok(next);
    }
    emit_state(&app, AuthState::Authenticating);
    Ok(AuthState::Authenticating)
}

#[tauri::command]
fn get_auth_state(app: tauri::AppHandle, store: State<'_, AuthStore>) -> Result<AuthState, String> {
    let current = store.state.lock().map_err(|_| "状态锁失败")?.clone();
    if !matches!(current, AuthState::Checking) {
        return Ok(current);
    }

    let profile = store.credentials.load_profile()?;
    let pending = store.credentials.load_pending()?;
    let session = store.credentials.load_session()?;
    *store.profile.lock().map_err(|_| "状态锁失败")? = profile.clone();
    *store.pending.lock().map_err(|_| "状态锁失败")? = pending.clone();

    if let Some(session) = session
        .filter(|session| session_matches_account(session, profile.as_ref(), pending.as_ref()))
    {
        if let Some(window) = app.get_webview_window("main") {
            if let Ok(current_url) = window.url() {
                if current_url.host_str() != Some(AUTH_HOST) {
                    *store.resume_url.lock().map_err(|_| "状态锁失败")? = Some(current_url);
                }
            }
        }
        return start_session_restore(app, &store, session);
    }
    let _ = store.credentials.delete_session();
    let next = if profile.is_some() || pending.is_some() {
        AuthState::SessionExpired
    } else {
        AuthState::Unauthenticated
    };
    *store.state.lock().map_err(|_| "状态锁失败")? = next.clone();
    emit_state(&app, next.clone());
    Ok(next)
}

#[tauri::command]
fn get_account_profile(store: State<'_, AuthStore>) -> Result<Option<AccountProfileView>, String> {
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_profile()?);
    Ok(profile.as_ref().map(profile_view))
}

#[tauri::command]
fn get_user_data(store: State<'_, AuthStore>) -> Result<Option<UserDataView>, String> {
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_profile()?);
    let pending = store
        .pending
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_pending()?);
    Ok(user_data_view(profile.as_ref(), pending.as_ref()))
}

#[tauri::command]
fn confirm_profile(
    app: tauri::AppHandle,
    input: AccountProfileInput,
    store: State<'_, AuthStore>,
) -> Result<(), String> {
    let current = store.profile.lock().map_err(|_| "状态锁失败")?.clone();
    let pending = store.pending.lock().map_err(|_| "状态锁失败")?.clone();
    let current_password = current
        .as_ref()
        .map(|profile| profile.password.as_str())
        .or_else(|| pending.as_ref().map(|pending| pending.password.as_str()));
    input.validate(current_password)?;
    let profile = StoredAccountProfile {
        student_id: input.student_id,
        campus_card_number: input.campus_card_number,
        name: input
            .name
            .or_else(|| pending.as_ref().and_then(|pending| pending.name.clone())),
        password: input
            .password
            .or_else(|| current.map(|profile| profile.password))
            .or_else(|| pending.map(|pending| pending.password))
            .unwrap_or_default(),
    };
    store.credentials.save_profile(&profile)?;
    store.credentials.delete_pending()?;
    *store.profile.lock().map_err(|_| "状态锁失败")? = Some(profile.clone());
    *store.pending.lock().map_err(|_| "状态锁失败")? = None;
    let state = AuthState::Authenticated {
        name: profile.name.clone(),
        student_id: profile.student_id.clone(),
    };
    *store.state.lock().map_err(|_| "状态锁失败")? = state.clone();
    emit_state(&app, state);
    Ok(())
}

fn navigate_to_authentication(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
    let store = app.try_state::<AuthStore>().ok_or("认证状态不可用")?;
    if let Ok(current_url) = window.url() {
        if current_url.host_str() != Some(AUTH_HOST) {
            *store.resume_url.lock().map_err(|_| "状态锁失败")? = Some(current_url);
        }
    }
    // The initial `get_auth_state` normally warms these caches. Load from the
    // credential store as a fallback so a reauthentication click still gets
    // the saved form values after an app/window restart.
    let profile = store
        .profile
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_profile()?);
    let pending = store
        .pending
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
        .or(store.credentials.load_pending()?);
    let prefill = login_form_prefill(profile.as_ref(), pending.as_ref());
    let attempt = match profile.as_ref() {
        Some(profile) => AuthAttempt::ReauthenticateExisting {
            student_id: profile.student_id.clone(),
        },
        None => AuthAttempt::InitialSetup,
    };
    *store.profile.lock().map_err(|_| "状态锁失败")? = profile;
    *store.pending.lock().map_err(|_| "状态锁失败")? = pending;
    *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::Authenticating;
    *store.attempt.lock().map_err(|_| "状态锁失败")? = attempt;
    *store.captured.lock().map_err(|_| "状态锁失败")? = None;
    *store.login_prefill.lock().map_err(|_| "状态锁失败")? = prefill;
    store.credentials.delete_session()?;
    clear_webview_session(&app)?;
    window
        .navigate(
            AUTH_LOGIN_URL
                .parse::<tauri::Url>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    emit_state(&app, AuthState::Authenticating);
    Ok(())
}

#[tauri::command]
fn begin_authentication(app: tauri::AppHandle) -> Result<(), String> {
    navigate_to_authentication(app)
}

#[tauri::command]
fn logout(app: tauri::AppHandle, store: State<'_, AuthStore>) -> Result<(), String> {
    store.credentials.delete_profile()?;
    store.credentials.delete_pending()?;
    store.credentials.delete_session()?;
    *store.profile.lock().map_err(|_| "状态锁失败")? = None;
    *store.pending.lock().map_err(|_| "状态锁失败")? = None;
    *store.captured.lock().map_err(|_| "状态锁失败")? = None;
    *store.login_prefill.lock().map_err(|_| "状态锁失败")? = None;
    *store.attempt.lock().map_err(|_| "状态锁失败")? = AuthAttempt::None;
    *store.state.lock().map_err(|_| "状态锁失败")? = AuthState::Unauthenticated;
    clear_webview_session(&app)?;
    emit_state(&app, AuthState::Unauthenticated);
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn dorm_settings(store: &AuthStore) -> Result<DormNetworkSettings, String> {
    if let Some(settings) = store
        .dorm_settings
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone()
    {
        return Ok(settings);
    }
    let loaded = store.credentials.load_dorm_settings()?.unwrap_or_default();
    *store.dorm_settings.lock().map_err(|_| "状态锁失败")? = Some(loaded.clone());
    Ok(loaded)
}

fn dorm_settings_view(
    store: &AuthStore,
    settings: &DormNetworkSettings,
) -> Result<DormNetworkSettingsView, String> {
    Ok(DormNetworkSettingsView {
        enabled: settings.enabled,
        credential_mode: settings.credential_mode.clone(),
        custom_configured: store
            .credentials
            .load_dorm_credentials()?
            .is_some_and(|value| {
                is_campus_card_number(&value.campus_card_number) && !value.password.is_empty()
            }),
    })
}

fn dorm_log_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_log_dir()
        .map_err(|error| error.to_string())?
        .join(DORM_LOG_FILE))
}

fn append_dorm_log(app: &tauri::AppHandle, log: DormLoginLogView) {
    let Ok(path) = dorm_log_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let Ok(line) = serde_json::to_string(&log) else {
        return;
    };
    let _ = writeln!(file, "{line}");
    // Keep the file bounded without retaining secrets in application state.
    if let Ok(content) = fs::read_to_string(&path) {
        let mut lines: Vec<&str> = content.lines().collect();
        if lines.len() > 1000 {
            lines = lines.split_off(lines.len() - 1000);
            let _ = fs::write(path, format!("{}\n", lines.join("\n")));
        }
    }
    if let Some(store) = app.try_state::<AuthStore>() {
        if let Ok(mut logs) = store.dorm_logs.lock() {
            logs.push_back(log.clone());
            while logs.len() > 1000 {
                logs.pop_front();
            }
        }
        let _ = app.emit("dorm-login-log-added", &log);
    }
}

fn load_dorm_logs(app: &tauri::AppHandle) -> Vec<DormLoginLogView> {
    let Ok(path) = dorm_log_path(app) else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .rev()
        .take(1000)
        .filter_map(|line| serde_json::from_str::<DormLoginLogView>(line).ok())
        .collect()
}

fn set_dorm_status(app: &tauri::AppHandle, update: impl FnOnce(&mut DormLoginStatusView)) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Ok(mut status) = store.dorm_status.lock() else {
        return;
    };
    update(&mut status);
    let next = status.clone();
    drop(status);
    let _ = app.emit("dorm-login-status-changed", next);
}

fn dorm_notify_failure(app: &tauri::AppHandle, message: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("校园网登录失败")
        .body(message)
        .show();
}

fn save_dorm_failure_notice(app: &tauri::AppHandle, message: &str) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let Ok(mut settings) = dorm_settings(&store) else {
        return;
    };
    settings.pending_failure_notice = Some(DormFailureNotice {
        message: message.to_owned(),
        timestamp_ms: now_ms(),
    });
    let _ = store.credentials.save_dorm_settings(&settings);
    {
        if let Ok(mut cached) = store.dorm_settings.lock() {
            *cached = Some(settings);
        };
    }
}

fn encode_query_component(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn read_http_response_bytes(host: &str, port: u16, path: &str) -> Result<(u16, Vec<u8>), String> {
    let address = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or("网关地址不可解析")?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: your-szu-life/0.1\r\nAccept: */*\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let _ = stream.shutdown(Shutdown::Write);
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let separator = b"\r\n\r\n";
    let split = bytes
        .windows(separator.len())
        .position(|window| window == separator)
        .ok_or("网关响应格式无效")?;
    let header = String::from_utf8_lossy(&bytes[..split]);
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .ok_or("网关 HTTP 状态无效")?;
    Ok((status, bytes[split + separator.len()..].to_vec()))
}

fn read_http_response(host: &str, port: u16, path: &str) -> Result<(u16, String), String> {
    let (status, body) = read_http_response_bytes(host, port, path)?;
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

fn decode_portal_html(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    let charset = extract_portal_value(&raw, "charset").unwrap_or_default();
    if charset.eq_ignore_ascii_case("gb2312") || charset.eq_ignore_ascii_case("gbk") {
        let (decoded, _, _) = encoding_rs::GB18030.decode(bytes);
        return decoded.into_owned();
    }
    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
}

fn extract_portal_value(html: &str, name: &str) -> Option<String> {
    let start = html.find(name)?;
    let rest = &html[start + name.len()..];
    let equals = rest.find('=')?;
    let rest = rest[equals + 1..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let end = rest[1..].find(quote)? + 1;
    let value = rest[1..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn extract_portal_numeric(html: &str, name: &str) -> Option<u64> {
    let start = html.find(name)?;
    let rest = &html[start + name.len()..];
    let equals = rest.find('=')?;
    let rest = rest[equals + 1..].trim_start();
    if let Some(quote) = rest
        .chars()
        .next()
        .filter(|quote| *quote == '\'' || *quote == '"')
    {
        let end = rest[1..].find(quote)? + 1;
        return rest[1..end].trim().parse().ok();
    }
    let end = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    (end > 0).then(|| rest[..end].parse().ok()).flatten()
}

fn extract_portal_session_info(html: &str) -> DormPortalSessionInfo {
    DormPortalSessionInfo {
        online_username: extract_portal_value(html, "NID"),
        campus_card_number: extract_portal_value(html, "uid"),
        online_duration: extract_portal_numeric(html, "oltime"),
        online_flow: extract_portal_numeric(html, "olflow"),
        login_ip: extract_portal_value(html, "lip"),
        login_started_at: extract_portal_value(html, "stime"),
        login_expires_at: extract_portal_value(html, "etime"),
        charset: extract_portal_value(html, "charset"),
    }
}

fn has_online_portal_marker(html: &str) -> bool {
    html.contains("Dr.COMWebLoginID_1.htm")
}

fn has_login_portal_marker(html: &str) -> bool {
    html.contains("Dr.COMWebLoginID_0.htm")
}

fn has_online_session_fields(session: &DormPortalSessionInfo) -> bool {
    session.online_username.is_some()
        || session.login_ip.is_some()
        || session.login_started_at.is_some()
        || session.login_expires_at.is_some()
}

fn valid_ipv4(value: &str) -> bool {
    let parts: Vec<_> = value.trim().split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| part.parse::<u8>().is_ok())
        && value != "000.000.000.000"
        && value != "0.0.0.0"
}

fn extract_terminal_ip(html: &str) -> Option<String> {
    ["v4ip", "v46ip", "ss5"]
        .into_iter()
        .filter_map(|key| extract_portal_value(html, key))
        .find(|value| valid_ipv4(value))
}

#[derive(Debug, Clone, Default)]
struct NetworkSnapshot {
    connected: bool,
    ssid: Option<String>,
    ip: Option<String>,
}

#[cfg(not(target_os = "android"))]
fn command_text(command: &str, args: &[&str]) -> Option<String> {
    Command::new(command)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
}

fn network_snapshot() -> NetworkSnapshot {
    #[cfg(target_os = "linux")]
    {
        let ssid =
            command_text("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"]).and_then(|text| {
                text.lines().find_map(|line| {
                    let (active, ssid) = line.split_once(':')?;
                    (active == "yes" && !ssid.is_empty()).then(|| ssid.to_owned())
                })
            });
        let ip = command_text("hostname", &["-I"]).and_then(|text| {
            text.split_whitespace()
                .find(|value| valid_ipv4(value))
                .map(str::to_owned)
        });
        return NetworkSnapshot {
            connected: ip.is_some() || ssid.is_some(),
            ssid,
            ip,
        };
    }
    #[cfg(target_os = "macos")]
    {
        let ssid = command_text("networksetup", &["-getairportnetwork", "en0"])
            .and_then(|text| {
                text.split_once(": ")
                    .map(|(_, value)| value.trim().to_owned())
            })
            .filter(|value| !value.is_empty() && !value.contains("not associated"));
        let ip = command_text("ipconfig", &["getifaddr", "en0"])
            .and_then(|text| text.trim().parse::<std::net::Ipv4Addr>().ok())
            .map(|value| value.to_string());
        return NetworkSnapshot {
            connected: ip.is_some() || ssid.is_some(),
            ssid,
            ip,
        };
    }
    #[cfg(target_os = "windows")]
    {
        let ssid = command_text("netsh", &["wlan", "show", "interfaces"]).and_then(|text| {
            text.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim().eq_ignore_ascii_case("SSID") && !value.trim().is_empty())
                    .then(|| value.trim().to_owned())
            })
        });
        return NetworkSnapshot {
            connected: ssid.is_some(),
            ssid,
            ip: None,
        };
    }
    #[cfg(target_os = "android")]
    {
        NetworkSnapshot::default()
    }
}

fn tcp_host_reachable(host: &str, port: u16) -> bool {
    format!("{host}:{port}")
        .to_socket_addrs()
        .ok()
        .into_iter()
        .flatten()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok())
}

fn staff_host_reachable() -> bool {
    tcp_host_reachable(STAFF_NETWORK_HOST, 80) || tcp_host_reachable(STAFF_NETWORK_HOST, 443)
}

#[derive(Debug, Clone, Default)]
struct DormPortalProbe {
    reachable: bool,
    authenticated: bool,
    session: Option<DormPortalSessionInfo>,
}

fn probe_dorm_portal() -> DormPortalProbe {
    let Ok((status, body)) = read_http_response_bytes(DORM_PORTAL_HOST, 80, "/") else {
        return DormPortalProbe::default();
    };
    if !(200..300).contains(&status) {
        return DormPortalProbe {
            reachable: false,
            ..DormPortalProbe::default()
        };
    }
    let decoded = decode_portal_html(&body);
    let session = extract_portal_session_info(&decoded);
    let marker_authenticated = has_online_portal_marker(&decoded);
    let marker_logged_out = has_login_portal_marker(&decoded);
    let status_authenticated =
        read_http_response(DORM_PORTAL_HOST, 80, "/drcom/chkstatus?callback=dr1003")
            .ok()
            .and_then(|(_, response)| parse_jsonp_response(&response).ok())
            .and_then(|value| value.get("result").and_then(|value| value.as_i64()))
            .is_some_and(|result| result == 1);
    DormPortalProbe {
        reachable: true,
        authenticated: !marker_logged_out
            && (marker_authenticated
                || (status_authenticated && has_online_session_fields(&session))),
        session: has_online_session_fields(&session).then_some(session),
    }
}

fn classify_network_overview(
    snapshot: &NetworkSnapshot,
    portal_reachable: bool,
    staff_reachable: bool,
    dorm_connected: bool,
    portal_session: Option<DormPortalSessionInfo>,
) -> NetworkOverviewView {
    let region = if snapshot.ssid.as_deref() == Some("SZU_WLAN") {
        NetworkRegion::Staff
    } else if portal_reachable {
        NetworkRegion::Dormitory
    } else if staff_reachable {
        NetworkRegion::Staff
    } else {
        NetworkRegion::None
    };
    let state = match region {
        NetworkRegion::None => NetworkConnectionState::NoNetwork,
        NetworkRegion::Staff => NetworkConnectionState::NotLoggedIn,
        NetworkRegion::Dormitory => {
            if dorm_connected {
                NetworkConnectionState::Connected
            } else {
                NetworkConnectionState::NotLoggedIn
            }
        }
    };
    NetworkOverviewView {
        state,
        region,
        ssid: snapshot.ssid.clone(),
        portal_reachable,
        staff_host_reachable: staff_reachable,
        portal_session,
    }
}

fn probe_network_overview(snapshot: &NetworkSnapshot) -> (NetworkOverviewView, DormPortalProbe) {
    let portal = probe_dorm_portal();
    let staff_reachable = if snapshot.ssid.as_deref() == Some("SZU_WLAN") {
        true
    } else {
        staff_host_reachable()
    };
    let overview = classify_network_overview(
        snapshot,
        portal.reachable,
        staff_reachable,
        portal.authenticated,
        portal.session.clone(),
    );
    (overview, portal)
}

fn update_network_overview(app: &tauri::AppHandle, overview: NetworkOverviewView) {
    let Some(store) = app.try_state::<AuthStore>() else {
        return;
    };
    let changed = store
        .network_overview
        .lock()
        .map(|mut current| {
            let changed = *current != overview;
            if changed {
                *current = overview.clone();
            }
            changed
        })
        .unwrap_or(false);
    if changed {
        let _ = app.emit("network-overview-changed", &overview);
    }
}

fn parse_jsonp_response(body: &str) -> Result<serde_json::Value, String> {
    let body = body.trim();
    let start = body.find('(').ok_or("登录响应不是 JSONP")?;
    let end = body.rfind(')').ok_or("登录响应不是 JSONP")?;
    serde_json::from_str(body[start + 1..end].trim()).map_err(|error| error.to_string())
}

fn sanitized_response(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(key, _)| {
                    let key = key.to_ascii_lowercase();
                    !key.contains("password")
                        && !key.contains("passwd")
                        && !key.contains("token")
                        && !key.contains("cookie")
                        && !key.contains("secret")
                })
                .map(|(key, value)| (key.clone(), sanitized_response(value)))
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(sanitized_response).collect())
        }
        other => other.clone(),
    }
}

fn backoff_delay(failures: u32) -> Duration {
    if failures <= 5 {
        Duration::from_secs(10)
    } else if failures <= 10 {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(600)
    }
}

fn dormant_credentials(
    store: &AuthStore,
    settings: &DormNetworkSettings,
) -> Result<(String, String), String> {
    let (card, password) = match settings.credential_mode {
        DormCredentialMode::CurrentAccount => {
            let profile = store
                .profile
                .lock()
                .map_err(|_| "状态锁失败")?
                .clone()
                .or(store.credentials.load_profile()?)
                .ok_or("当前账户尚未保存校园卡号和密码")?;
            (profile.campus_card_number, profile.password)
        }
        DormCredentialMode::Custom => {
            let value = store
                .credentials
                .load_dorm_credentials()?
                .ok_or("尚未配置宿舍区校园网凭据")?;
            (value.campus_card_number, value.password)
        }
    };
    if !is_campus_card_number(&card) || password.is_empty() {
        return Err("校园卡号必须是 6 位数字且密码不能为空".into());
    }
    Ok((card, password))
}

fn record_dorm_transport_failure(
    app: &tauri::AppHandle,
    error: String,
    portal_reachable: bool,
    http_status: Option<u16>,
) -> String {
    let failures = app
        .try_state::<AuthStore>()
        .and_then(|store| {
            let mut status = store.dorm_status.lock().ok()?;
            status.portal_reachable = portal_reachable;
            status.consecutive_failures = status.consecutive_failures.saturating_add(1);
            let next = now_ms() + backoff_delay(status.consecutive_failures).as_millis() as u64;
            status.next_probe_at_ms = Some(next);
            status.state = DormLoginState::Monitoring;
            Some((status.consecutive_failures, next))
        })
        .unwrap_or((0, now_ms() + 600_000));
    set_dorm_status(app, |status| {
        status.consecutive_failures = failures.0;
        status.next_probe_at_ms = Some(failures.1);
        status.state = DormLoginState::Monitoring;
    });
    append_dorm_log(
        app,
        DormLoginLogView {
            timestamp_ms: now_ms(),
            stage: "probe".into(),
            outcome: "error".into(),
            portal_reachable,
            http_status,
            result: None,
            message: Some(error.clone()),
            ret_code: None,
            consecutive_failures: failures.0,
            next_probe_at_ms: Some(failures.1),
            response: None,
        },
    );
    error
}

fn perform_dorm_login(app: tauri::AppHandle, manual: bool) -> Result<DormLoginResultView, String> {
    let store = app.try_state::<AuthStore>().ok_or("校园网状态不可用")?;
    if !manual && !dorm_settings(&store)?.enabled {
        return Err("自动登录未开启".into());
    }
    let settings = dorm_settings(&store)?;
    let (card, password) = dormant_credentials(&store, &settings)?;
    let snapshot = network_snapshot();
    set_dorm_status(&app, |status| {
        status.state = DormLoginState::Probing;
        status.ssid = snapshot.ssid.clone();
        status.connected = snapshot.connected;
    });
    let portal = read_http_response_bytes(DORM_PORTAL_HOST, 80, "/");
    let (http_status, portal_body) = match portal {
        Ok(value) => value,
        Err(error) => {
            let failures = {
                let mut status = store.dorm_status.lock().map_err(|_| "状态锁失败")?;
                status.portal_reachable = false;
                status.consecutive_failures = status.consecutive_failures.saturating_add(1);
                status.next_probe_at_ms =
                    Some(now_ms() + backoff_delay(status.consecutive_failures).as_millis() as u64);
                status.state = DormLoginState::Monitoring;
                status.consecutive_failures
            };
            let next = now_ms() + backoff_delay(failures).as_millis() as u64;
            set_dorm_status(&app, |status| status.next_probe_at_ms = Some(next));
            append_dorm_log(
                &app,
                DormLoginLogView {
                    timestamp_ms: now_ms(),
                    stage: "probe".into(),
                    outcome: "unreachable".into(),
                    portal_reachable: false,
                    http_status: None,
                    result: None,
                    message: Some(error.clone()),
                    ret_code: None,
                    consecutive_failures: failures,
                    next_probe_at_ms: Some(next),
                    response: None,
                },
            );
            return Err(error);
        }
    };
    let html = decode_portal_html(&portal_body);
    if !(200..300).contains(&http_status) {
        return Err(record_dorm_transport_failure(
            &app,
            format!("门户返回 HTTP {http_status}"),
            false,
            Some(http_status),
        ));
    }
    let ip = extract_terminal_ip(&html)
        .or(snapshot.ip.clone())
        .ok_or("未能从门户页面读取终端 IP")?;
    let callback = format!("dr{}", 1000 + (now_ms() % 9000));
    let desktop_terminal = cfg!(target_os = "windows") || cfg!(target_os = "macos");
    let prefix = if desktop_terminal { ",0," } else { ",1," };
    let terminal_type = if cfg!(target_os = "android") || cfg!(target_os = "linux") {
        2
    } else {
        1
    };
    let account = format!("{prefix}{card}");
    let path = format!(
        "/eportal/portal/login?callback={}&login_method=1&user_account={}&user_password={}&wlan_user_ip={}&wlan_user_ipv6=&wlan_user_mac=000000000000&wlan_ac_ip=&wlan_ac_name=&jsVersion=4.1.3&terminal_type={}&lang=zh-cn&v={}",
        encode_query_component(&callback), encode_query_component(&account), encode_query_component(&password),
        encode_query_component(&ip), terminal_type, now_ms() % 10000 + 500
    );
    set_dorm_status(&app, |status| {
        status.state = DormLoginState::LoggingIn;
        status.portal_reachable = true;
    });
    let (login_status, body) = match read_http_response(DORM_PORTAL_HOST, 801, &path) {
        Ok(value) => value,
        Err(error) => return Err(record_dorm_transport_failure(&app, error, true, None)),
    };
    let parsed = match parse_jsonp_response(&body) {
        Ok(value) => value,
        Err(error) => {
            return Err(record_dorm_transport_failure(
                &app,
                error,
                true,
                Some(login_status),
            ))
        }
    };
    let result = parsed.get("result").and_then(|v| v.as_i64());
    let ret_code = parsed.get("ret_code").and_then(|v| v.as_i64());
    let message = parsed
        .get("msg")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let success = result == Some(1) || (result == Some(0) && ret_code == Some(2));
    let already_online = result == Some(0) && ret_code == Some(2);
    let view = DormLoginResultView {
        ok: success,
        already_online,
        message: message.clone(),
        result,
        ret_code,
        http_status: Some(login_status),
    };
    if success {
        set_dorm_status(&app, |status| {
            status.state = DormLoginState::Online;
            status.portal_reachable = true;
            status.consecutive_failures = 0;
            status.next_probe_at_ms = Some(now_ms() + 600_000);
            status.last_result = Some(view.clone());
        });
        update_network_overview(
            &app,
            classify_network_overview(&snapshot, true, false, true, None),
        );
        append_dorm_log(
            &app,
            DormLoginLogView {
                timestamp_ms: now_ms(),
                stage: "login".into(),
                outcome: if already_online {
                    "already-online".into()
                } else {
                    "success".into()
                },
                portal_reachable: true,
                http_status: Some(login_status),
                result,
                message,
                ret_code,
                consecutive_failures: 0,
                next_probe_at_ms: Some(now_ms() + 600_000),
                response: Some(sanitized_response(&parsed)),
            },
        );
        Ok(view)
    } else {
        set_dorm_status(&app, |status| {
            status.state = DormLoginState::Paused;
            status.portal_reachable = true;
            status.last_result = Some(view.clone());
        });
        update_network_overview(
            &app,
            classify_network_overview(&snapshot, true, false, false, None),
        );
        let notice = view
            .message
            .clone()
            .unwrap_or_else(|| "宿舍区校园网认证失败".into());
        save_dorm_failure_notice(&app, &notice);
        dorm_notify_failure(&app, &notice);
        append_dorm_log(
            &app,
            DormLoginLogView {
                timestamp_ms: now_ms(),
                stage: "login".into(),
                outcome: "business-failure".into(),
                portal_reachable: true,
                http_status: Some(login_status),
                result,
                message: view.message.clone(),
                ret_code,
                consecutive_failures: 0,
                next_probe_at_ms: None,
                response: Some(sanitized_response(&parsed)),
            },
        );
        Err(notice)
    }
}

fn start_dorm_login(app: tauri::AppHandle, manual: bool) -> Result<(), String> {
    let store = app.try_state::<AuthStore>().ok_or("校园网状态不可用")?;
    if !manual && !dorm_settings(&store)?.enabled {
        return Err("自动登录未开启".into());
    }
    let settings = dorm_settings(&store)?;
    let _ = dormant_credentials(&store, &settings)?;
    {
        let mut busy = store
            .dorm_login_in_flight
            .lock()
            .map_err(|_| "状态锁失败")?;
        if *busy {
            return Err("校园网登录正在进行中".into());
        }
        *busy = true;
    }
    thread::spawn(move || {
        let result = perform_dorm_login(app.clone(), manual);
        if let Some(store) = app.try_state::<AuthStore>() {
            if let Ok(mut busy) = store.dorm_login_in_flight.lock() {
                *busy = false;
            }
        }
        if let Err(error) = result {
            set_dorm_status(&app, |status| {
                if !matches!(status.state, DormLoginState::Paused) {
                    status.state = DormLoginState::Monitoring;
                }
                if !error.is_empty() {
                    status.last_result.get_or_insert_with(Default::default);
                }
            });
        }
    });
    Ok(())
}

fn monitor_dorm_network(app: tauri::AppHandle, stop: mpsc::Receiver<()>) {
    loop {
        if stop.recv_timeout(DORM_MONITOR_TICK).is_ok() {
            break;
        }
        let Some(store) = app.try_state::<AuthStore>() else {
            break;
        };
        let snapshot = network_snapshot();
        let (overview, _portal_probe) = probe_network_overview(&snapshot);
        update_network_overview(&app, overview.clone());
        let Ok(settings) = dorm_settings(&store) else {
            continue;
        };
        if !settings.enabled || !matches!(overview.region, NetworkRegion::Dormitory) {
            set_dorm_status(&app, |status| {
                if !settings.enabled {
                    status.state = DormLoginState::Disabled;
                } else if !matches!(overview.region, NetworkRegion::Dormitory)
                    && !matches!(status.state, DormLoginState::Online)
                {
                    status.state = DormLoginState::Monitoring;
                }
            });
            continue;
        }
        let fingerprint = format!(
            "{}|{}|{}",
            snapshot.ssid.as_deref().unwrap_or(""),
            snapshot.ip.as_deref().unwrap_or(""),
            snapshot.connected
        );
        let changed = store
            .dorm_last_fingerprint
            .lock()
            .ok()
            .and_then(|mut last| {
                let changed = last.as_deref() != Some(fingerprint.as_str());
                *last = Some(fingerprint);
                Some(changed)
            })
            .unwrap_or(false);
        if changed {
            set_dorm_status(&app, |status| {
                status.consecutive_failures = 0;
                status.next_probe_at_ms = Some(now_ms());
                if matches!(status.state, DormLoginState::Paused | DormLoginState::Error) {
                    status.state = DormLoginState::Monitoring;
                }
            });
        }
        let due = store
            .dorm_status
            .lock()
            .ok()
            .and_then(|status| status.next_probe_at_ms)
            .is_none_or(|next| next <= now_ms());
        let paused = store
            .dorm_status
            .lock()
            .ok()
            .is_some_and(|status| matches!(status.state, DormLoginState::Paused));
        if due
            && !paused
            && !store
                .dorm_login_in_flight
                .lock()
                .ok()
                .is_some_and(|busy| *busy)
        {
            let _ = start_dorm_login(app.clone(), false);
        }
    }
}

fn start_dorm_monitor(app: tauri::AppHandle) {
    let app_for_state = app.clone();
    let Some(store) = app_for_state.try_state::<AuthStore>() else {
        return;
    };
    let Ok(mut monitor) = store.dorm_monitor.lock() else {
        return;
    };
    if monitor.is_some() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    *monitor = Some(sender);
    drop(monitor);
    thread::spawn(move || monitor_dorm_network(app, receiver));
}

#[tauri::command]
fn get_network_overview(store: State<'_, AuthStore>) -> Result<NetworkOverviewView, String> {
    Ok(store
        .network_overview
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone())
}

#[tauri::command]
fn get_dorm_network_settings(
    app: tauri::AppHandle,
    store: State<'_, AuthStore>,
) -> Result<DormNetworkSettingsView, String> {
    let settings = dorm_settings(&store)?;
    let view = dorm_settings_view(&store, &settings)?;
    start_dorm_monitor(app);
    Ok(view)
}

#[tauri::command]
fn save_dorm_network_settings(
    app: tauri::AppHandle,
    input: DormNetworkSettingsInput,
    store: State<'_, AuthStore>,
) -> Result<DormNetworkSettingsView, String> {
    if input.enabled && input.credential_mode == DormCredentialMode::CurrentAccount {
        let profile = store
            .profile
            .lock()
            .map_err(|_| "状态锁失败")?
            .clone()
            .or(store.credentials.load_profile()?)
            .ok_or("当前账户尚未保存校园卡号和密码")?;
        if !is_campus_card_number(&profile.campus_card_number) || profile.password.is_empty() {
            return Err("当前账户尚未保存完整校园网凭据".into());
        }
    }
    if input.credential_mode == DormCredentialMode::Custom {
        let existing = store.credentials.load_dorm_credentials()?;
        let card = input
            .campus_card_number
            .or_else(|| existing.as_ref().map(|v| v.campus_card_number.clone()))
            .unwrap_or_default();
        let password = input
            .password
            .or_else(|| existing.as_ref().map(|v| v.password.clone()))
            .unwrap_or_default();
        if !is_campus_card_number(&card) || password.is_empty() {
            return Err("校园卡号必须是 6 位数字且密码不能为空".into());
        }
        store
            .credentials
            .save_dorm_credentials(&DormCustomCredentials {
                campus_card_number: card,
                password,
            })?;
    }
    let settings = DormNetworkSettings {
        enabled: input.enabled,
        credential_mode: input.credential_mode,
        pending_failure_notice: dorm_settings(&store)?.pending_failure_notice,
    };
    store.credentials.save_dorm_settings(&settings)?;
    *store.dorm_settings.lock().map_err(|_| "状态锁失败")? = Some(settings.clone());
    set_dorm_status(&app, |status| {
        status.state = if settings.enabled {
            DormLoginState::Monitoring
        } else {
            DormLoginState::Disabled
        };
        status.next_probe_at_ms = if settings.enabled {
            Some(now_ms())
        } else {
            None
        };
    });
    start_dorm_monitor(app.clone());
    dorm_settings_view(&store, &settings)
}

#[tauri::command]
fn get_dorm_login_status(store: State<'_, AuthStore>) -> Result<DormLoginStatusView, String> {
    Ok(store.dorm_status.lock().map_err(|_| "状态锁失败")?.clone())
}

#[tauri::command]
fn trigger_dorm_login(app: tauri::AppHandle) -> Result<(), String> {
    start_dorm_login(app, true)
}

#[tauri::command]
fn get_dorm_login_logs(
    app: tauri::AppHandle,
    store: State<'_, AuthStore>,
) -> Result<Vec<DormLoginLogView>, String> {
    let mut logs = load_dorm_logs(&app);
    logs.reverse();
    if let Ok(mut cached) = store.dorm_logs.lock() {
        *cached = logs.iter().cloned().collect();
    }
    Ok(logs)
}

#[tauri::command]
fn consume_dorm_login_failure_notice(
    app: tauri::AppHandle,
    store: State<'_, AuthStore>,
) -> Result<Option<DormFailureNotice>, String> {
    let mut settings = dorm_settings(&store)?;
    let notice = settings.pending_failure_notice.take();
    store.credentials.save_dorm_settings(&settings)?;
    *store.dorm_settings.lock().map_err(|_| "状态锁失败")? = Some(settings);
    let _ = app;
    Ok(notice)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AuthStore {
            state: Mutex::new(AuthState::Checking),
            attempt: Mutex::new(AuthAttempt::None),
            profile: Mutex::new(None),
            pending: Mutex::new(None),
            captured: Mutex::new(None),
            login_prefill: Mutex::new(None),
            resume_url: Mutex::new(None),
            credentials: PlatformCredentialStore,
            dorm_settings: Mutex::new(None),
            dorm_status: Mutex::new(DormLoginStatusView::default()),
            dorm_logs: Mutex::new(VecDeque::new()),
            dorm_login_in_flight: Mutex::new(false),
            dorm_monitor: Mutex::new(None),
            dorm_last_fingerprint: Mutex::new(None),
            network_overview: Mutex::new(NetworkOverviewView::default()),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            start_dorm_monitor(app.handle().clone());
            #[cfg(desktop)]
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;
                let show = MenuItem::with_id(app, "show", "打开窗口", true, None::<&str>)?;
                let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &exit])?;
                TrayIconBuilder::new()
                    .menu(&menu)
                    .tooltip("深大生活")
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "exit" => app.exit(0),
                        _ => {}
                    })
                    .build(app)?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(store) = window.app_handle().try_state::<AuthStore>() {
                    if dorm_settings(&store)
                        .map(|settings| settings.enabled)
                        .unwrap_or(false)
                    {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_auth_state,
            get_account_profile,
            get_user_data,
            begin_authentication,
            capture_auth_credentials,
            confirm_profile,
            logout,
            get_network_overview,
            get_dorm_network_settings,
            save_dorm_network_settings,
            get_dorm_login_status,
            trigger_dorm_login,
            get_dorm_login_logs,
            consume_dorm_login_failure_notice
        ])
        .on_page_load(|window, payload| {
            if !matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                return;
            }
            if is_auth_login_url(payload.url().as_str()) {
                let app = window.app_handle();
                let restoring_session = app.try_state::<AuthStore>().is_some_and(|store| {
                    matches!(
                        store.attempt.lock().ok().as_deref(),
                        Some(AuthAttempt::SessionRestore { .. })
                    )
                });
                if restoring_session {
                    let handle = app.clone();
                    thread::spawn(move || {
                        if let Some(store) = handle.try_state::<AuthStore>() {
                            session_restore_expired(&handle, &store);
                        }
                    });
                    return;
                }
                let (allows_qr_login, prefill) = app
                    .try_state::<AuthStore>()
                    .map(|store| {
                        let allows_qr_login = store
                            .attempt
                            .lock()
                            .ok()
                            .is_some_and(|attempt| attempt.allows_qr_login());
                        let prefill = store
                            .login_prefill
                            .lock()
                            .ok()
                            .and_then(|value| value.clone());
                        (allows_qr_login, prefill)
                    })
                    .unwrap_or((false, None));
                let _ = window.eval(&auth_login_capture_script(
                    allows_qr_login,
                    prefill.as_ref(),
                ));
            }
            if is_authserver_url(payload.url().as_str())
                && window
                    .app_handle()
                    .try_state::<AuthStore>()
                    .is_some_and(|store| {
                        matches!(
                            store.state.lock().ok().as_deref(),
                            Some(AuthState::Authenticating)
                        )
                    })
            {
                let handle = window.app_handle().clone();
                let _ = window.eval(auth_probe_script());
                let probe_window = window.clone();
                thread::spawn(move || {
                    for _ in 0..20 {
                        thread::sleep(Duration::from_millis(250));
                        let callback_handle = handle.clone();
                        let _ = probe_window.eval_with_callback(
                            auth_probe_result_script(),
                            move |value| {
                                if value != "null" && value != "undefined" {
                                    let completion_handle = callback_handle.clone();
                                    thread::spawn(move || {
                                        complete_authentication(completion_handle, value)
                                    });
                                }
                            },
                        );
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_cookie() -> StoredAuthCookie {
        StoredAuthCookie {
            name: "JSESSIONID".into(),
            value: "opaque-session".into(),
            domain: AUTH_HOST.into(),
            path: "/".into(),
            expires: Some("Sun, 13 Sep 2026 00:00:00 +0000".into()),
            http_only: Some(true),
            secure: Some(true),
            same_site: Some("Lax".into()),
            partitioned: None,
        }
    }

    #[test]
    fn validates_fields_and_reuses_verified_password() {
        let input = AccountProfileInput {
            student_id: "1234567890".into(),
            campus_card_number: "123456".into(),
            name: None,
            password: None,
        };
        assert!(input.validate(Some("secret")).is_ok());
        assert!(input.validate(None).is_err());
    }

    #[test]
    fn cookie_session_round_trip_keeps_sensitive_attributes_internal() {
        let cookie = session_cookie();
        let restored = cookie.to_webview_cookie().expect("valid cookie");
        assert_eq!(restored.name(), "JSESSIONID");
        assert_eq!(restored.domain(), Some(AUTH_HOST));
        assert_eq!(restored.http_only(), Some(true));
        assert_eq!(restored.secure(), Some(true));
        let secret = AuthSessionSecret {
            student_id: "1234567890".into(),
            cookies: vec![cookie],
        };
        let public = serde_json::to_string(&AccountProfileView {
            student_id: secret.student_id,
            campus_card_number: "123456".into(),
            name: None,
            has_password: true,
        })
        .unwrap();
        assert!(!public.contains("opaque-session"));
        assert!(!public.contains("password"));
    }

    #[test]
    fn session_cookie_rejects_external_domains_and_expired_sessions() {
        assert!(!is_auth_cookie_domain("example.com"));
        assert!(!is_auth_cookie_domain("other.szu.edu.cn"));
        assert!(is_auth_cookie_domain(".szu.edu.cn"));
        let mut expired = session_cookie();
        expired.expires = Some("Sun, 13 Sep 2020 00:00:00 +0000".into());
        let session = AuthSessionSecret {
            student_id: "1234567890".into(),
            cookies: vec![expired],
        };
        let profile = StoredAccountProfile {
            student_id: "1234567890".into(),
            campus_card_number: "123456".into(),
            name: None,
            password: "secret".into(),
        };
        assert!(!session_matches_account(&session, Some(&profile), None));
    }

    #[test]
    fn auth_cookie_serialization_requires_a_safe_matching_path() {
        let mut cookie = session_cookie();
        cookie.path = "personalInfo".into();
        assert!(cookie.to_webview_cookie().is_err());
        cookie.path = "/personalInfo".into();
        assert!(cookie.to_webview_cookie().is_ok());
        cookie.domain = "other.szu.edu.cn".into();
        assert!(cookie.to_webview_cookie().is_err());
    }

    #[test]
    fn profile_and_pending_must_match_the_verified_uid() {
        let profile = StoredAccountProfile {
            student_id: "1234567890".into(),
            campus_card_number: "123456".into(),
            name: None,
            password: "secret".into(),
        };
        let pending = PendingAuthCredentials {
            student_id: "1234567890".into(),
            password: "secret".into(),
            name: None,
        };
        assert_eq!(profile.student_id, pending.student_id);
        assert_ne!(profile.student_id, "9999999999");
    }

    #[test]
    fn reauthentication_prefill_uses_saved_profile_or_pending_credentials() {
        let profile = StoredAccountProfile {
            student_id: "1234567890".into(),
            campus_card_number: "123456".into(),
            name: None,
            password: "saved-password".into(),
        };
        let pending = PendingAuthCredentials {
            student_id: "9999999999".into(),
            password: "pending-password".into(),
            name: None,
        };
        assert_eq!(
            login_form_prefill(Some(&profile), Some(&pending)),
            Some(LoginFormPrefill {
                identifier: "1234567890".into(),
                password: "saved-password".into(),
            })
        );
        assert_eq!(
            login_form_prefill(None, Some(&pending)),
            Some(LoginFormPrefill {
                identifier: "9999999999".into(),
                password: "pending-password".into(),
            })
        );
    }

    #[test]
    fn remote_credential_capture_capability_matches_authserver_page_origin() {
        let capability = include_str!("../capabilities/authserver-credential-capture.json");
        // Tauri evaluates remote IPC against the document origin it reports
        // (`https://authserver.szu.edu.cn/`), not the full login pathname.
        // The Rust command below still enforces /authserver/login exactly.
        assert!(capability.contains(r#"https://authserver.szu.edu.cn"#));
        assert!(!capability.contains("authserver/login*"));
    }

    #[test]
    fn login_capture_script_requires_capture_before_official_submission() {
        let script = auth_login_capture_script(false, None);
        assert!(script.contains("const savedCredentials = null"));
        assert!(script.contains("const applySavedCredentials"));
        assert!(script.contains("autofilledForms = new WeakSet"));
        assert!(script.contains("account.value = savedCredentials.identifier"));
        assert!(script.contains("password.value = savedCredentials.password"));
        assert!(script.contains("autofilledForms.add(form)"));
        assert!(script.contains("await captureCandidate()"));
        assert!(script.contains("window.startLogin(button)"));
        assert!(script.contains(".login-main .m-account #username"));
        assert!(script.contains(".login-main .m-account #password"));
        assert!(script.contains(".login-main #login_submit"));
        assert!(script.contains(".login-main #pwdFromId"));
        assert!(script.contains(".login-main #rememberMe"));
        assert!(!script.contains("document.querySelector('#username,"));
        assert!(!script.contains("document.querySelector('#password,"));
        assert!(!script.contains("button[type=\"submit\"]"));
        assert!(script.contains("if (isActiveLoginButton(event.target)) intercept(event)"));
        assert!(script.contains("document.addEventListener('keydown', interceptEnter, true)"));
        assert!(script.contains("document.addEventListener('keyup', interceptEnter, true)"));
        assert!(script.contains("if (event.target === loginForm()) intercept(event)"));
        assert!(script.contains("await captureCandidate()"));
        let bridge = &script[script.find("const bridgeSubmit = async").unwrap()..];
        assert!(
            bridge.find("await captureCandidate()").unwrap()
                < bridge.find("continueOfficialLogin(button)").unwrap()
        );
        assert!(script.contains(".qrcode-middle"));
        assert!(script.contains("二维码登录已禁用"));
        assert!(script.contains("stopImmediatePropagation"));
        assert!(script.contains("未能读取当前账号密码表单，请刷新后重新输入"));
        let reauth = auth_login_capture_script(true, None);
        assert!(reauth.contains("const qrAllowed = true"));
    }

    #[test]
    fn extracts_the_public_name_from_verified_user_conf() {
        let current_response = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\"},\"data\":{\"cn\":\" 张三 \"}}"}"#;
        assert_eq!(
            probe_identity(current_response),
            Some(VerifiedUserIdentity {
                student_id: "1234567890".into(),
                name: Some("张三".into()),
            })
        );

        let nested = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\",\"userInfo\":{\"realName\":\"李四\"}}}"}"#;
        assert_eq!(
            probe_identity(nested),
            Some(VerifiedUserIdentity {
                student_id: "1234567890".into(),
                name: Some("李四".into()),
            })
        );

        let absent = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\"}}"}"#;
        assert_eq!(
            probe_identity(absent).and_then(|identity| identity.name),
            None
        );
    }

    #[test]
    fn accepts_only_strict_successful_probe_results() {
        let valid = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"1234567890\"}}"}"#;
        let login_redirect = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/authserver/login","text":"{\"datas\":{\"uid\":\"1234567890\"}}"}"#;
        let invalid_uid = r#"{"ok":true,"status":200,"url":"https://authserver.szu.edu.cn/personalInfo/common/getUserConf","text":"{\"datas\":{\"uid\":\"bad\"}}"}"#;
        assert!(auth_probe_succeeded(valid));
        assert!(!auth_probe_succeeded(login_redirect));
        assert!(auth_probe_reached_verified_endpoint(invalid_uid));
        assert!(!auth_probe_succeeded(invalid_uid));
    }

    #[test]
    fn auth_state_uses_camel_case_data_fields_for_the_frontend() {
        let awaiting = serde_json::to_value(AuthState::AwaitingProfile {
            name: None,
            student_id: Some("1234567890".into()),
        })
        .unwrap();
        assert_eq!(awaiting["awaitingProfile"]["studentId"], "1234567890");
        assert!(awaiting["awaitingProfile"].get("student_id").is_none());

        let authenticated = serde_json::to_value(AuthState::Authenticated {
            name: None,
            student_id: "1234567890".into(),
        })
        .unwrap();
        assert_eq!(authenticated["authenticated"]["studentId"], "1234567890");
        assert!(authenticated["authenticated"].get("student_id").is_none());
    }

    #[test]
    fn extracts_terminal_ip_by_portal_priority_and_rejects_invalid_values() {
        let html =
            "<script>v4ip='000.000.000.000';v46ip='172.24.75.190';ss5='172.24.1.1';</script>";
        assert_eq!(extract_terminal_ip(html).as_deref(), Some("172.24.75.190"));
        assert_eq!(extract_terminal_ip("v4ip='bad';v46ip='0.0.0.0';"), None);
    }

    #[test]
    fn parses_portal_jsonp_and_classifies_success_and_already_online() {
        let success = parse_jsonp_response("dr1003({\"result\":1,\"msg\":\"ok\"});").unwrap();
        assert_eq!(success["result"], 1);
        let online = parse_jsonp_response("dr1003({\"result\":0,\"ret_code\":2});").unwrap();
        assert_eq!(online["ret_code"], 2);
        assert!(parse_jsonp_response("not-jsonp").is_err());
    }

    #[test]
    fn parses_portal_online_session_fields_and_gb2312_html() {
        let html = "charset='gb2312';NID='张三';uid='123456';oltime='4294967295';olflow=4294967295;lip='172.24.111.161';stime='2026-09-05 09:06:33';etime='2026-09-07 19:30:49';<!--Dr.COMWebLoginID_1.htm-->";
        let session = extract_portal_session_info(html);
        assert_eq!(session.online_username.as_deref(), Some("张三"));
        assert_eq!(session.campus_card_number.as_deref(), Some("123456"));
        assert_eq!(session.online_duration, Some(4_294_967_295));
        assert_eq!(session.online_flow, Some(4_294_967_295));
        assert_eq!(session.login_ip.as_deref(), Some("172.24.111.161"));
        assert!(has_online_portal_marker(html));
        assert!(!has_login_portal_marker(html));

        let (encoded, _, _) = encoding_rs::GB18030.encode("charset='gb2312';<title>注销页</title>");
        let decoded = decode_portal_html(encoded.as_ref());
        assert!(decoded.contains("注销页"));
    }

    #[test]
    fn online_portal_session_drives_connected_state_without_cached_login_status() {
        let snapshot = NetworkSnapshot {
            connected: true,
            ssid: Some("SZU_CTC&CMCC".into()),
            ip: Some("172.24.111.161".into()),
        };
        let session = DormPortalSessionInfo {
            online_username: Some("张三".into()),
            campus_card_number: Some("123456".into()),
            online_duration: Some(120),
            online_flow: Some(4096),
            login_ip: Some("172.24.111.161".into()),
            login_started_at: Some("2026-09-05 09:06:33".into()),
            login_expires_at: Some("2026-09-07 19:30:49".into()),
            charset: Some("gb2312".into()),
        };
        let overview =
            classify_network_overview(&snapshot, true, false, true, Some(session.clone()));
        assert_eq!(overview.state, NetworkConnectionState::Connected);
        assert_eq!(overview.portal_session, Some(session));
    }

    #[test]
    fn classifies_network_regions_and_connection_states() {
        let staff = NetworkSnapshot {
            connected: true,
            ssid: Some("SZU_WLAN".into()),
            ip: Some("10.0.0.2".into()),
        };
        let overview = classify_network_overview(&staff, false, true, false, None);
        assert_eq!(overview.region, NetworkRegion::Staff);
        assert_eq!(overview.state, NetworkConnectionState::NotLoggedIn);

        let dorm = NetworkSnapshot {
            connected: true,
            ssid: Some("SZU_CTC&CMCC".into()),
            ip: Some("172.24.75.190".into()),
        };
        let online = classify_network_overview(&dorm, true, false, true, None);
        assert_eq!(online.region, NetworkRegion::Dormitory);
        assert_eq!(online.state, NetworkConnectionState::Connected);
        let logged_out = classify_network_overview(&dorm, true, false, false, None);
        assert_eq!(logged_out.state, NetworkConnectionState::NotLoggedIn);

        let fallback_staff = classify_network_overview(&dorm, false, true, false, None);
        assert_eq!(fallback_staff.region, NetworkRegion::Staff);
        assert_eq!(fallback_staff.state, NetworkConnectionState::NotLoggedIn);

        let offline = NetworkSnapshot::default();
        let no_network = classify_network_overview(&offline, false, false, false, None);
        assert_eq!(no_network.region, NetworkRegion::None);
        assert_eq!(no_network.state, NetworkConnectionState::NoNetwork);
    }

    #[test]
    fn backoff_has_the_requested_boundaries() {
        assert_eq!(backoff_delay(1), Duration::from_secs(10));
        assert_eq!(backoff_delay(5), Duration::from_secs(10));
        assert_eq!(backoff_delay(6), Duration::from_secs(120));
        assert_eq!(backoff_delay(10), Duration::from_secs(120));
        assert_eq!(backoff_delay(11), Duration::from_secs(600));
    }

    #[test]
    fn response_sanitization_removes_credentials_and_tokens() {
        let value = serde_json::json!({
            "result": 1,
            "msg": "ok",
            "user_password": "secret",
            "token": "opaque",
            "nested": {"cookie": "private", "safe": true}
        });
        let clean = sanitized_response(&value);
        assert!(clean.get("user_password").is_none());
        assert!(clean.get("token").is_none());
        assert!(clean["nested"].get("cookie").is_none());
        assert_eq!(clean["nested"]["safe"], true);
    }

    #[test]
    fn dorm_settings_default_to_disabled_current_account() {
        let settings = DormNetworkSettings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.credential_mode, DormCredentialMode::CurrentAccount);
        assert!(settings.pending_failure_notice.is_none());
    }

    #[test]
    fn public_auth_state_has_distinct_terminal_failures() {
        assert_ne!(
            AuthState::CredentialCaptureFailed,
            AuthState::SessionCaptureFailed
        );
        assert_ne!(
            AuthState::CredentialStorageFailed,
            AuthState::AccountMismatch
        );
    }
}
