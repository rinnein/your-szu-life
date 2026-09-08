use super::*;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

/// In-memory keyring double. Tests never access the user's system keyring.
#[derive(Default)]
pub(crate) struct MemoryCredentialStore {
    raw: Mutex<HashMap<&'static str, String>>,
    pub(crate) fail_settings_save: AtomicBool,
    pub(crate) fail_settings_load: AtomicBool,
    pub(crate) dorm_credential_reads: AtomicUsize,
}

impl MemoryCredentialStore {
    pub(crate) fn seed(&self, name: &'static str, raw: &str) {
        self.raw.lock().unwrap().insert(name, raw.to_owned());
    }
    fn load<T: for<'a> Deserialize<'a>>(&self, key: SecretKey) -> Result<Option<T>, String> {
        if matches!(key, SecretKey::DormNetworkSettings)
            && self.fail_settings_load.load(Ordering::Relaxed)
        {
            return Err("simulated settings read failure".into());
        }
        if matches!(key, SecretKey::DormNetworkCredentials) {
            self.dorm_credential_reads.fetch_add(1, Ordering::Relaxed);
        }
        self.raw
            .lock()
            .unwrap()
            .get(key.name())
            .map(|raw| serde_json::from_str(raw).map_err(|error| error.to_string()))
            .transpose()
    }
    fn save<T: Serialize>(&self, key: SecretKey, value: &T) -> Result<(), String> {
        if matches!(key, SecretKey::DormNetworkSettings)
            && self.fail_settings_save.load(Ordering::Relaxed)
        {
            return Err("simulated settings write failure".into());
        }
        self.raw.lock().unwrap().insert(
            key.name(),
            serde_json::to_string(value).map_err(|error| error.to_string())?,
        );
        Ok(())
    }
    fn delete(&self, key: SecretKey) -> Result<(), String> {
        self.raw.lock().unwrap().remove(key.name());
        Ok(())
    }
}
macro_rules! stored_value {
    ($load:ident,$save:ident,$value:ty,$key:ident) => {
        fn $load(&self) -> Result<Option<$value>, String> {
            self.load(SecretKey::$key)
        }
        fn $save(&self, value: &$value) -> Result<(), String> {
            self.save(SecretKey::$key, value)
        }
    };
}
impl CredentialStore for MemoryCredentialStore {
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
    fn delete_profile(&self) -> Result<(), String> {
        self.delete(SecretKey::Profile)
    }
    fn delete_pending(&self) -> Result<(), String> {
        self.delete(SecretKey::PendingAuth)
    }
    fn delete_session(&self) -> Result<(), String> {
        self.delete(SecretKey::AuthSession)
    }
}

#[test]
fn storage_namespace_and_legacy_json_remain_compatible() {
    assert_eq!(SERVICE_NAME, "in.rinne.your-szu-life");
    assert_eq!(
        [
            SecretKey::Profile,
            SecretKey::PendingAuth,
            SecretKey::AuthSession,
            SecretKey::DormNetworkSettings,
            SecretKey::DormNetworkCredentials
        ]
        .map(SecretKey::name),
        [
            "profile",
            "pending-auth",
            "auth-session",
            "dorm-network-settings",
            "dorm-network-credentials"
        ]
    );
    let store = MemoryCredentialStore::default();
    store.seed("profile",r#"{"student_id":"1234567890","campus_card_number":"123456","name":"测试","password":"test-only"}"#);
    store.seed(
        "pending-auth",
        r#"{"student_id":"1234567890","password":"test-only","name":"测试"}"#,
    );
    store.seed(
        "auth-session",
        r#"{"student_id":"1234567890","cookies":[]}"#,
    );
    store.seed("dorm-network-settings",r#"{"enabled":true,"credential_mode":"custom","pending_failure_notice":{"message":"test-only","timestampMs":123}}"#);
    store.seed(
        "dorm-network-credentials",
        r#"{"campus_card_number":"654321","password":"custom-test-only"}"#,
    );
    let original = store.raw.lock().unwrap().clone();
    store
        .save_profile(&store.load_profile().unwrap().unwrap())
        .unwrap();
    store
        .save_pending(&store.load_pending().unwrap().unwrap())
        .unwrap();
    store
        .save_session(&store.load_session().unwrap().unwrap())
        .unwrap();
    store
        .save_dorm_settings(&store.load_dorm_settings().unwrap().unwrap())
        .unwrap();
    store
        .save_dorm_credentials(&store.load_dorm_credentials().unwrap().unwrap())
        .unwrap();
    for (key, raw) in original {
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&raw).unwrap(),
            serde_json::from_str::<serde_json::Value>(&store.raw.lock().unwrap()[key]).unwrap()
        );
    }
    store.delete_profile().unwrap();
    store.delete_pending().unwrap();
    store.delete_session().unwrap();
    assert!(store.load_profile().unwrap().is_none());
    assert!(store.load_dorm_settings().unwrap().unwrap().enabled);
    assert_eq!(
        store
            .load_dorm_credentials()
            .unwrap()
            .unwrap()
            .campus_card_number,
        "654321"
    );
}
