use crate::{
    auth::{
        model::{PendingAuthCredentials, StoredAccountProfile},
        session::AuthSessionSecret,
    },
    campus_network::model::{DormCustomCredentials, DormNetworkSettings, StaffNetworkSettings},
    campus_payments::model::{ElectricityLocationSettings, ElectricityMonitorRecord},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[cfg(target_os = "android")]
use tauri_plugin_keyring_store::KeyringStore;

const SERVICE_NAME: &str = "in.rinne.your-szu-life";
pub(crate) const CREDENTIALS_UNAVAILABLE: &str = "系统密钥环暂时不可用";

pub(crate) type SharedCredentialStore = Arc<dyn CredentialStore>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretKey {
    Profile,
    PendingAuth,
    AuthSession,
    UoocSession,
    UoocAi,
    OjAccounts,
    AcademicSessions,
    DormNetworkSettings,
    DormNetworkCredentials,
    StaffNetworkSettings,
    StaffNetworkCredentials,
    ElectricitySettings,
    ElectricityMonitor,
}

impl SecretKey {
    fn name(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::PendingAuth => "pending-auth",
            Self::AuthSession => "auth-session",
            Self::UoocSession => "uooc-session",
            Self::UoocAi => "uooc-ai",
            Self::OjAccounts => "oj-accounts",
            Self::AcademicSessions => "academic-sessions",
            Self::DormNetworkSettings => "dorm-network-settings",
            Self::DormNetworkCredentials => "dorm-network-credentials",
            Self::StaffNetworkSettings => "staff-network-settings",
            Self::StaffNetworkCredentials => "staff-network-credentials",
            Self::ElectricitySettings => "electricity-settings",
            Self::ElectricityMonitor => "electricity-monitor",
        }
    }
}

pub(crate) trait CredentialStore: Send + Sync {
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
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CredentialStorageStatus {
    pub backend: &'static str,
    pub available: bool,
}
impl CredentialStorageStatus {
    fn system() -> Self {
        Self {
            backend: "systemKeyring",
            available: true,
        }
    }
    #[cfg(target_os = "linux")]
    fn file(available: bool) -> Self {
        Self {
            backend: "localEncryptedFile",
            available,
        }
    }
}

pub(crate) mod commands;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
static LINUX_STORE: std::sync::OnceLock<linux::LinuxStore> = std::sync::OnceLock::new();

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub(crate) mod keyring_store {
    use super::SERVICE_NAME;
    use keyring::Entry;

    // Windows Credential Manager limit is 512 bytes for CREDENTIALW blob.
    // In UTF-16, 240 chars = 480 bytes, safely below 512.
    pub const CHUNK_SIZE: usize = 240;
    pub const CHUNK_PREFIX: &str = "__CHUNKED__:";

    fn entry(key: &str) -> Result<Entry, String> {
        Entry::new(SERVICE_NAME, key).map_err(|e| e.to_string())
    }

    pub fn read(key: &str) -> Result<Option<String>, String> {
        let main_entry = entry(key)?;
        let first = match main_entry.get_password() {
            Ok(val) => val,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };

        if let Some(rest) = first.strip_prefix(CHUNK_PREFIX) {
            let count: usize = rest
                .parse()
                .map_err(|_| "分块凭据格式无效".to_string())?;
            let mut result = String::new();
            for i in 0..count {
                let chunk_key = format!("{key}#{i}");
                let chunk = entry(&chunk_key)?
                    .get_password()
                    .map_err(|e| format!("读取凭据分块 {i} 失败: {e}"))?;
                result.push_str(&chunk);
            }
            Ok(Some(result))
        } else {
            Ok(Some(first))
        }
    }

    pub fn write(key: &str, value: &str) -> Result<(), String> {
        let main_entry = entry(key)?;

        if value.len() <= CHUNK_SIZE && !value.starts_with(CHUNK_PREFIX) {
            // Clean up any old chunks if previously chunked
            if let Ok(old) = main_entry.get_password() {
                if let Some(rest) = old.strip_prefix(CHUNK_PREFIX) {
                    if let Ok(count) = rest.parse::<usize>() {
                        for i in 0..count {
                            let _ = entry(&format!("{key}#{i}")).and_then(|e| {
                                e.delete_credential().map_err(|err| err.to_string())
                            });
                        }
                    }
                }
            }
            main_entry
                .set_password(value)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        // Split into chunks on valid UTF-8 character boundaries
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < value.len() {
            let mut end = (start + CHUNK_SIZE).min(value.len());
            while end > start && !value.is_char_boundary(end) {
                end -= 1;
            }
            chunks.push(&value[start..end]);
            start = end;
        }

        let count = chunks.len();

        for (i, chunk) in chunks.into_iter().enumerate() {
            let chunk_key = format!("{key}#{i}");
            entry(&chunk_key)?
                .set_password(chunk)
                .map_err(|e| format!("写入凭据分块 {i} 失败: {e}"))?;
        }

        // Clean up any old higher-index chunks from previously larger entries
        if let Ok(old) = main_entry.get_password() {
            if let Some(rest) = old.strip_prefix(CHUNK_PREFIX) {
                if let Ok(old_count) = rest.parse::<usize>() {
                    for i in count..old_count {
                        let _ = entry(&format!("{key}#{i}")).and_then(|e| {
                            e.delete_credential().map_err(|err| err.to_string())
                        });
                    }
                }
            }
        }

        main_entry
            .set_password(&format!("{CHUNK_PREFIX}{count}"))
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn delete(key: &str) -> Result<(), String> {
        let main_entry = entry(key)?;
        if let Ok(val) = main_entry.get_password() {
            if let Some(rest) = val.strip_prefix(CHUNK_PREFIX) {
                if let Ok(count) = rest.parse::<usize>() {
                    for i in 0..count {
                        let _ = entry(&format!("{key}#{i}")).and_then(|e| {
                            e.delete_credential().map_err(|err| err.to_string())
                        });
                    }
                }
            }
        }
        let _ = main_entry.delete_credential();
        Ok(())
    }
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法确定凭据存储目录")?
            .join("credentials");
        let _ = LINUX_STORE.set(linux::LinuxStore::new(
            directory,
            linux::KeyringBackend { legacy: false },
            linux::KeyringBackend { legacy: true },
        ));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_store() -> Result<&'static linux::LinuxStore, String> {
    LINUX_STORE.get().ok_or_else(|| "凭据存储尚未初始化".into())
}

pub(crate) struct PlatformCredentialStore;

impl PlatformCredentialStore {
    #[cfg(target_os = "linux")]
    fn load<T: for<'a> Deserialize<'a>>(key: SecretKey) -> Result<Option<T>, String> {
        linux_store()?
            .read_validated(key.name(), |raw| {
                serde_json::from_str::<T>(raw)
                    .map(|_| ())
                    .map_err(|_| linux::StoreError::Corrupt)
            })
            .map_err(linux::StoreError::message)?
            .map(|raw| {
                serde_json::from_str(&raw).map_err(|_| "保存的凭据格式无效，未修改原数据".into())
            })
            .transpose()
    }
    #[cfg(target_os = "linux")]
    fn save<T: Serialize>(key: SecretKey, value: &T) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|_| "凭据格式无效")?;
        linux_store()?
            .write(key.name(), Some(&raw))
            .map_err(linux::StoreError::message)
    }
    #[cfg(target_os = "linux")]
    fn delete(key: SecretKey) -> Result<(), String> {
        linux_store()?
            .write(key.name(), None)
            .map_err(linux::StoreError::message)
    }

    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    fn load<T: for<'a> Deserialize<'a>>(key: SecretKey) -> Result<Option<T>, String> {
        match keyring_store::read(key.name())? {
            Some(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|e| e.to_string()),
            None => Ok(None),
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    fn save<T: Serialize>(key: SecretKey, value: &T) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|e| e.to_string())?;
        keyring_store::write(key.name(), &raw)
    }

    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    fn delete(key: SecretKey) -> Result<(), String> {
        keyring_store::delete(key.name())
    }
}

#[cfg(target_os = "android")]
fn keyring_store() -> KeyringStore {
    KeyringStore::new(SERVICE_NAME)
}

impl CredentialStore for PlatformCredentialStore {
    fn load_academic_sessions(&self) -> Result<Option<String>, String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::load(SecretKey::AcademicSessions)
        }
        #[cfg(target_os = "android")]
        {
            load_android_secret(SecretKey::AcademicSessions)
        }
    }
    fn save_academic_sessions(&self, value: &String) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::save(SecretKey::AcademicSessions, value)
        }
        #[cfg(target_os = "android")]
        {
            save_android_secret(SecretKey::AcademicSessions, value)
        }
    }

    fn storage_status(&self) -> CredentialStorageStatus {
        #[cfg(target_os = "linux")]
        {
            linux_store()
                .map(|store| store.status())
                .unwrap_or_else(|_| CredentialStorageStatus::file(false))
        }
        #[cfg(not(target_os = "linux"))]
        {
            CredentialStorageStatus::system()
        }
    }

    fn load_oj_accounts(&self) -> Result<Option<String>, String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::load(SecretKey::OjAccounts)
        }
        #[cfg(target_os = "android")]
        {
            load_android_secret(SecretKey::OjAccounts)
        }
    }
    fn save_oj_accounts(&self, value: &String) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::save(SecretKey::OjAccounts, value)
        }
        #[cfg(target_os = "android")]
        {
            save_android_secret(SecretKey::OjAccounts, value)
        }
    }
    fn load_uooc_ai(&self) -> Result<Option<String>, String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::load(SecretKey::UoocAi)
        }
        #[cfg(target_os = "android")]
        {
            load_android_secret(SecretKey::UoocAi)
        }
    }
    fn save_uooc_ai(&self, value: &String) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::save(SecretKey::UoocAi, value)
        }
        #[cfg(target_os = "android")]
        {
            save_android_secret(SecretKey::UoocAi, value)
        }
    }
    fn load_uooc_session(&self) -> Result<Option<String>, String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::load(SecretKey::UoocSession)
        }
        #[cfg(target_os = "android")]
        {
            load_android_secret(SecretKey::UoocSession)
        }
    }
    fn save_uooc_session(&self, session: &String) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::save(SecretKey::UoocSession, session)
        }
        #[cfg(target_os = "android")]
        {
            save_android_secret(SecretKey::UoocSession, session)
        }
    }
    fn delete_uooc_session(&self) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::delete(SecretKey::UoocSession)
        }
        #[cfg(target_os = "android")]
        {
            delete_android_secret(SecretKey::UoocSession)
        }
    }
    fn load_staff_settings(&self) -> Result<Option<StaffNetworkSettings>, String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::load(SecretKey::StaffNetworkSettings)
        }
        #[cfg(target_os = "android")]
        {
            load_android_secret(SecretKey::StaffNetworkSettings)
        }
    }
    fn save_staff_settings(&self, settings: &StaffNetworkSettings) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::save(SecretKey::StaffNetworkSettings, settings)
        }
        #[cfg(target_os = "android")]
        {
            save_android_secret(SecretKey::StaffNetworkSettings, settings)
        }
    }
    fn load_staff_credentials(&self) -> Result<Option<DormCustomCredentials>, String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::load(SecretKey::StaffNetworkCredentials)
        }
        #[cfg(target_os = "android")]
        {
            load_android_secret(SecretKey::StaffNetworkCredentials)
        }
    }
    fn save_staff_credentials(&self, credentials: &DormCustomCredentials) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        {
            Self::save(SecretKey::StaffNetworkCredentials, credentials)
        }
        #[cfg(target_os = "android")]
        {
            save_android_secret(SecretKey::StaffNetworkCredentials, credentials)
        }
    }
    #[cfg(not(target_os = "android"))]
    fn load_electricity_settings(&self) -> Result<Option<ElectricityLocationSettings>, String> {
        Self::load(SecretKey::ElectricitySettings)
    }
    #[cfg(not(target_os = "android"))]
    fn save_electricity_settings(
        &self,
        settings: &ElectricityLocationSettings,
    ) -> Result<(), String> {
        Self::save(SecretKey::ElectricitySettings, settings)
    }
    #[cfg(not(target_os = "android"))]
    fn load_electricity_monitor(&self) -> Result<Option<ElectricityMonitorRecord>, String> {
        Self::load(SecretKey::ElectricityMonitor)
    }
    #[cfg(not(target_os = "android"))]
    fn save_electricity_monitor(&self, record: &ElectricityMonitorRecord) -> Result<(), String> {
        Self::save(SecretKey::ElectricityMonitor, record)
    }

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
    #[cfg(target_os = "android")]
    fn load_electricity_settings(&self) -> Result<Option<ElectricityLocationSettings>, String> {
        load_android_secret(SecretKey::ElectricitySettings)
    }
    #[cfg(target_os = "android")]
    fn save_electricity_settings(
        &self,
        settings: &ElectricityLocationSettings,
    ) -> Result<(), String> {
        save_android_secret(SecretKey::ElectricitySettings, settings)
    }
    #[cfg(target_os = "android")]
    fn load_electricity_monitor(&self) -> Result<Option<ElectricityMonitorRecord>, String> {
        load_android_secret(SecretKey::ElectricityMonitor)
    }
    #[cfg(target_os = "android")]
    fn save_electricity_monitor(&self, record: &ElectricityMonitorRecord) -> Result<(), String> {
        save_android_secret(SecretKey::ElectricityMonitor, record)
    }
}

#[cfg(target_os = "android")]
fn load_android_secret<T: for<'a> Deserialize<'a>>(key: SecretKey) -> Result<Option<T>, String> {
    keyring_store()
        .get_password(key.name())
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
        .set_password(key.name(), &raw)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn delete_android_secret(key: SecretKey) -> Result<(), String> {
    keyring_store()
        .delete(key.name())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod keyring_chunk_tests {
    use super::keyring_store::*;

    #[test]
    fn test_chunk_split_and_boundaries() {
        // Test UTF-8 multi-byte characters (Chinese characters are 3 bytes each)
        let s = "深圳大学统一身份认证会话测试Cookie数据包含各种符号!@#$%^&*()_+{}[]:;".repeat(20);
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < s.len() {
            let mut end = (start + CHUNK_SIZE).min(s.len());
            while end > start && !s.is_char_boundary(end) {
                end -= 1;
            }
            chunks.push(&s[start..end]);
            start = end;
        }

        // Each chunk must be valid UTF-8 and <= CHUNK_SIZE
        for chunk in &chunks {
            assert!(chunk.len() <= CHUNK_SIZE);
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }

        // Reassembled must match exactly
        let reassembled: String = chunks.concat();
        assert_eq!(reassembled, s);
    }
}
