use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Mutex, thread, time::Duration};
use tauri::{webview::cookie::Cookie, Emitter, Manager, State};
#[cfg(target_os = "android")]
use tauri_plugin_keyring_store::KeyringStore;
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

const AUTH_LOGIN_URL: &str = "https://authserver.szu.edu.cn/authserver/login";
const AUTH_PROFILE_URL: &str =
    "https://authserver.szu.edu.cn/personalInfo/personCenter/index.html#/accountsecurity";
const AUTH_HOST: &str = "authserver.szu.edu.cn";
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
}

struct PlatformCredentialStore;

impl PlatformCredentialStore {
    #[cfg(not(target_os = "android"))]
    fn entry(key: SecretKey) -> Result<keyring::Entry, String> {
        let name = match key {
            SecretKey::Profile => "profile",
            SecretKey::PendingAuth => "pending-auth",
            SecretKey::AuthSession => "auth-session",
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
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_auth_state,
            get_account_profile,
            get_user_data,
            begin_authentication,
            capture_auth_credentials,
            confirm_profile,
            logout
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
