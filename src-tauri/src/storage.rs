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

pub(crate) type SharedCredentialStore = Arc<dyn CredentialStore>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretKey {
    Profile,
    PendingAuth,
    AuthSession,
    UoocSession,
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

pub(crate) struct PlatformCredentialStore;

impl PlatformCredentialStore {
    #[cfg(not(target_os = "android"))]
    fn entry(key: SecretKey) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE_NAME, key.name()).map_err(|e| e.to_string())
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
    KeyringStore::new(SERVICE_NAME)
}

impl CredentialStore for PlatformCredentialStore {
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
