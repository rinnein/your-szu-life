//! 凭据存储。全部落盘细节集中在 [`vault`]，其它模块只依赖 [`CredentialStore`]。

use crate::{
    auth::{
        model::{PendingAuthCredentials, StoredAccountProfile},
        session::AuthSessionSecret,
    },
    campus_network::model::{DormCustomCredentials, DormNetworkSettings, StaffNetworkSettings},
    campus_payments::model::{ElectricityLocationSettings, ElectricityMonitorRecord},
    netproxy::ProxySettings,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const SERVICE_NAME: &str = "in.rinne.your-szu-life";
pub const CREDENTIALS_UNAVAILABLE: &str = "系统密钥环暂时不可用";

pub type SharedCredentialStore = Arc<dyn CredentialStore>;

pub mod entries;
pub mod model;
pub mod vault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKey {
    Profile,
    PendingAuth,
    AuthSession,
    UoocSession,
    LmsSession,
    JxxkSession,
    QwSession,
    UoocAi,
    OjAccounts,
    AcademicSessions,
    DormNetworkSettings,
    DormNetworkCredentials,
    StaffNetworkSettings,
    StaffNetworkCredentials,
    ElectricitySettings,
    ElectricityMonitor,
    NetworkProxy,
}

impl SecretKey {
    /// 该条目在凭据文档中的键。
    pub fn document_key(self) -> Option<&'static str> {
        Some(match self {
            Self::Profile => entries::PROFILE,
            Self::PendingAuth => entries::PENDING_AUTH,
            Self::AuthSession => entries::AUTH_SESSION,
            Self::UoocSession => entries::UOOC_SESSION,
            Self::LmsSession => entries::LMS_SESSION,
            Self::JxxkSession => entries::JXXK_SESSION,
            Self::QwSession => entries::QW_SESSION,
            Self::UoocAi => entries::UOOC_AI,
            Self::OjAccounts => entries::OJ_ACCOUNTS,
            Self::AcademicSessions => entries::ACADEMIC_SESSIONS,
            Self::DormNetworkSettings => entries::DORM_SETTINGS,
            Self::DormNetworkCredentials => entries::DORM_CREDENTIALS,
            Self::StaffNetworkSettings => entries::STAFF_SETTINGS,
            Self::StaffNetworkCredentials => entries::STAFF_CREDENTIALS,
            Self::ElectricitySettings => entries::ELECTRICITY_LOCATION,
            Self::ElectricityMonitor => entries::ELECTRICITY_MONITOR,
            Self::NetworkProxy => entries::PROXY_SETTINGS,
        })
    }

    /// 旧版钥匙环条目名，用作测试替身的内存键。
    ///
    /// 兄弟 crate 的测试也要用，因此不挂在 `cfg(test)` 上。
    pub fn legacy_name(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::PendingAuth => "pending-auth",
            Self::AuthSession => "auth-session",
            Self::UoocSession => "uooc-session",
            Self::LmsSession => "lms-session",
            Self::JxxkSession => "jxxk-session",
            Self::QwSession => "qw-session",
            Self::UoocAi => "uooc-ai",
            Self::OjAccounts => "oj-accounts",
            Self::AcademicSessions => "academic-sessions",
            Self::DormNetworkSettings => "dorm-network-settings",
            Self::DormNetworkCredentials => "dorm-network-credentials",
            Self::StaffNetworkSettings => "staff-network-settings",
            Self::StaffNetworkCredentials => "staff-network-credentials",
            Self::ElectricitySettings => "electricity-settings",
            Self::ElectricityMonitor => "electricity-monitor",
            Self::NetworkProxy => "network-proxy",
        }
    }

    /// 旧版钥匙环条目名到 [`SecretKey`] 的反查，迁移专用。
    pub fn from_legacy_name(name: &str) -> Option<Self> {
        Some(match name {
            "profile" => Self::Profile,
            "pending-auth" => Self::PendingAuth,
            "auth-session" => Self::AuthSession,
            "uooc-ai" => Self::UoocAi,
            "uooc-session" => Self::UoocSession,
            "lms-session" => Self::LmsSession,
            "jxxk-session" => Self::JxxkSession,
            "qw-session" => Self::QwSession,
            "oj-accounts" => Self::OjAccounts,
            "academic-sessions" => Self::AcademicSessions,
            "dorm-network-settings" => Self::DormNetworkSettings,
            "dorm-network-credentials" => Self::DormNetworkCredentials,
            "staff-network-settings" => Self::StaffNetworkSettings,
            "staff-network-credentials" => Self::StaffNetworkCredentials,
            "electricity-settings" => Self::ElectricitySettings,
            "electricity-monitor" => Self::ElectricityMonitor,
            _ => return None,
        })
    }
}

pub trait CredentialStore: Send + Sync {
    fn load_academic_sessions(&self) -> Result<Option<String>, String>;
    fn save_academic_sessions(&self, value: &String) -> Result<(), String>;
    fn storage_status(&self) -> CredentialStorageStatus {
        CredentialStorageStatus::system()
    }
    fn load_oj_accounts(&self) -> Result<Option<String>, String>;
    fn save_oj_accounts(&self, value: &String) -> Result<(), String>;
    fn load_uooc_ai(&self) -> Result<Option<String>, String>;
    fn save_uooc_ai(&self, value: &String) -> Result<(), String>;
    fn load_uooc_session(&self) -> Result<Option<String>, String>;
    fn save_uooc_session(&self, session: &String) -> Result<(), String>;
    fn delete_uooc_session(&self) -> Result<(), String>;
    fn load_lms_session(&self) -> Result<Option<String>, String>;
    fn save_lms_session(&self, session: &String) -> Result<(), String>;
    fn load_qw_session(&self) -> Result<Option<String>, String>;
    fn save_qw_session(&self, session: &String) -> Result<(), String>;
    fn load_jxxk_session(&self) -> Result<Option<String>, String>;
    fn save_jxxk_session(&self, session: &String) -> Result<(), String>;
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
    fn load_staff_settings(&self) -> Result<Option<StaffNetworkSettings>, String>;
    fn save_staff_settings(&self, settings: &StaffNetworkSettings) -> Result<(), String>;
    fn load_staff_credentials(&self) -> Result<Option<DormCustomCredentials>, String>;
    fn save_staff_credentials(&self, credentials: &DormCustomCredentials) -> Result<(), String>;
    fn load_electricity_settings(&self) -> Result<Option<ElectricityLocationSettings>, String>;
    fn save_electricity_settings(
        &self,
        settings: &ElectricityLocationSettings,
    ) -> Result<(), String>;
    fn load_electricity_monitor(&self) -> Result<Option<ElectricityMonitorRecord>, String>;
    fn save_electricity_monitor(&self, record: &ElectricityMonitorRecord) -> Result<(), String>;
    fn load_proxy_settings(&self) -> Result<Option<ProxySettings>, String>;
    fn save_proxy_settings(&self, settings: &ProxySettings) -> Result<(), String>;
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStorageStatus {
    pub backend: &'static str,
    pub available: bool,
}
impl CredentialStorageStatus {
    fn system() -> Self {
        Self {
            backend: "encryptedVault",
            available: vault::vault().is_some(),
        }
    }
}

pub mod commands;

/// 当前账户的稳定指纹（学号散列前 16 个十六进制字符）。
///
/// 只用于本机隔离，不进入同步数据：同步检查点按账户分文件存放，切换账号后不会
/// 复用上一个账号的凭据。
pub fn account_fingerprint() -> Result<Option<String>, String> {
    use sha2::{Digest, Sha256};
    let Some(profile) = PlatformCredentialStore.load_profile()? else {
        return Ok(None);
    };
    let student_id = profile.student_id.trim();
    if student_id.is_empty() {
        return Ok(None);
    }
    let digest = Sha256::digest(format!("your-szu-life:account:v1:{student_id}").as_bytes());
    let mut out = String::with_capacity(16);
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    Ok(Some(out))
}

/// 解析数据目录、准备主密钥并加载/迁移凭据文件。
pub fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    if vault::vault().is_some() {
        return Ok(());
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定应用数据目录".to_string())?
        .join("credentials");
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建凭据目录: {e}"))?;
    let key = vault::load_or_create_key(SERVICE_NAME)?;
    let store = vault::Vault::open(dir, key);
    let report = crate::migrations::keyring::migrate_keyring_entries(&store)?;
    if !report.skipped.is_empty() {
        log::warn!(
            "以下旧版凭据未能迁移，需要重新登录：{}",
            report.skipped.join("、")
        );
    }
    vault::install(store)
}

pub struct PlatformCredentialStore;

impl PlatformCredentialStore {
    fn with_vault<T>(action: impl FnOnce(&vault::Vault) -> Result<T, String>) -> Result<T, String> {
        let store = vault::vault().ok_or_else(|| CREDENTIALS_UNAVAILABLE.to_string())?;
        action(store)
    }

    fn load<T: for<'a> Deserialize<'a>>(key: SecretKey) -> Result<Option<T>, String> {
        Self::with_vault(|store| {
            let document_key = key.document_key().ok_or("未知的凭据条目")?;
            store.load(document_key)
        })
    }

    /// OJ 账号在文档中拆成「账号」与「会话」两键，读取时需要重新合并。
    fn load_oj() -> Result<Option<String>, String> {
        Self::with_vault(entries::load_oj_accounts)
    }

    fn save<T: Serialize>(key: SecretKey, value: &T) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|e| e.to_string())?;
        Self::with_vault(|store| store.update(|document| entries::set_raw(document, key, &raw)))
    }

    /// OJ 账号存的是 JSON 文本本身，需要先解包再拆分写入。
    fn save_oj(value: &str) -> Result<(), String> {
        Self::with_vault(|store| store.update(|document| entries::set_oj_raw(document, value)))
    }

    fn delete(key: SecretKey) -> Result<(), String> {
        let document_key = key.document_key().ok_or("未知的凭据条目")?;
        Self::with_vault(|store| {
            store.update(|document| {
                document.remove(document_key);
                if document_key == entries::OJ_ACCOUNTS {
                    document.remove(entries::OJ_SESSIONS);
                }
                Ok(())
            })
        })
    }
}

macro_rules! stored_value {
    ($load:ident, $save:ident, $value:ty, $key:ident) => {
        fn $load(&self) -> Result<Option<$value>, String> {
            Self::load(SecretKey::$key)
        }
        fn $save(&self, value: &$value) -> Result<(), String> {
            Self::save(SecretKey::$key, value)
        }
    };
}

impl CredentialStore for PlatformCredentialStore {
    stored_value!(
        load_academic_sessions,
        save_academic_sessions,
        String,
        AcademicSessions
    );
    stored_value!(load_uooc_ai, save_uooc_ai, String, UoocAi);
    stored_value!(load_uooc_session, save_uooc_session, String, UoocSession);
    stored_value!(load_lms_session, save_lms_session, String, LmsSession);
    stored_value!(load_qw_session, save_qw_session, String, QwSession);
    stored_value!(load_jxxk_session, save_jxxk_session, String, JxxkSession);
    stored_value!(load_profile, save_profile, StoredAccountProfile, Profile);
    stored_value!(
        load_pending,
        save_pending,
        PendingAuthCredentials,
        PendingAuth
    );
    stored_value!(load_session, save_session, AuthSessionSecret, AuthSession);
    stored_value!(
        load_dorm_settings,
        save_dorm_settings,
        DormNetworkSettings,
        DormNetworkSettings
    );
    stored_value!(
        load_dorm_credentials,
        save_dorm_credentials,
        DormCustomCredentials,
        DormNetworkCredentials
    );
    stored_value!(
        load_staff_settings,
        save_staff_settings,
        StaffNetworkSettings,
        StaffNetworkSettings
    );
    stored_value!(
        load_staff_credentials,
        save_staff_credentials,
        DormCustomCredentials,
        StaffNetworkCredentials
    );
    stored_value!(
        load_electricity_settings,
        save_electricity_settings,
        ElectricityLocationSettings,
        ElectricitySettings
    );
    stored_value!(
        load_electricity_monitor,
        save_electricity_monitor,
        ElectricityMonitorRecord,
        ElectricityMonitor
    );
    stored_value!(
        load_proxy_settings,
        save_proxy_settings,
        ProxySettings,
        NetworkProxy
    );

    fn storage_status(&self) -> CredentialStorageStatus {
        CredentialStorageStatus::system()
    }

    fn load_oj_accounts(&self) -> Result<Option<String>, String> {
        Self::load_oj()
    }
    fn save_oj_accounts(&self, value: &String) -> Result<(), String> {
        Self::save_oj(value)
    }

    fn delete_uooc_session(&self) -> Result<(), String> {
        Self::delete(SecretKey::UoocSession)
    }
    fn delete_profile(&self) -> Result<(), String> {
        Self::delete(SecretKey::Profile)
    }
    fn delete_pending(&self) -> Result<(), String> {
        Self::delete(SecretKey::PendingAuth)
    }
    fn delete_session(&self) -> Result<(), String> {
        Self::delete(SecretKey::AuthSession)
    }
}

pub mod testing;

/// 只读诊断入口：调用方必须先通过环境变量显式授权。
/// 不参与生产代码路径。
#[cfg(all(test, target_os = "linux"))]
pub fn read_saved_session_for_diagnostic() -> Option<AuthSessionSecret> {
    vault::vault()?.load(entries::AUTH_SESSION).ok().flatten()
}

/// 只读诊断入口（跨 crate 版本）：直接从系统密钥环取主密钥并加载真实
/// 统一认证会话。**绝不写入**——密钥缺失时返回 None 而不是生成新钥，
/// 避免破坏用户现有 vault 的可解密性。调用方必须先通过环境变量显式授权。
#[cfg(target_os = "linux")]
pub fn load_saved_session_readonly() -> Option<AuthSessionSecret> {
    let key = vault::read_keyring_secret(SERVICE_NAME, vault::VAULT_KEY_ENTRY)
        .ok()?
        .filter(|bytes| bytes.len() == vault::KEY_LEN)?;
    let key: [u8; vault::KEY_LEN] = key.try_into().ok()?;
    let data_root = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| std::path::PathBuf::from(home).join(".local/share"))
        })?;
    let dir = data_root.join("in.rinne.your-szu-life").join("credentials");
    let vault = vault::Vault::open(dir, key);
    vault.load(entries::AUTH_SESSION).ok().flatten()
}
