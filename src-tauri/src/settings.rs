use crate::storage::SharedCredentialStore;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const FORMAT: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const MAX_PACKAGE: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SettingKind {
    Fixed,
    Runtime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SettingEntry {
    pub kind: SettingKind,
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    version: u8,
    entries: BTreeMap<String, SettingEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsSnapshot {
    pub entries: BTreeMap<String, SettingEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsImportPreview {
    pub conflicts: Vec<String>,
    pub incoming: usize,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportOptions {
    pub overwrite: bool,
}

pub(crate) struct SettingsStore {
    credentials: SharedCredentialStore,
}

pub(crate) mod commands;

impl SettingsStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self { credentials }
    }

    fn snapshot_map(&self) -> Result<BTreeMap<String, SettingEntry>, String> {
        let c = &self.credentials;
        let mut out = BTreeMap::new();
        macro_rules! put { ($key:literal, $kind:expr, $value:expr) => {
            if let Some(value) = $value? { out.insert($key.into(), SettingEntry { kind: $kind, value: serde_json::to_value(value).map_err(|_| "设置序列化失败")? }); }
        }}
        put!(
            "fixed.account.profile",
            SettingKind::Fixed,
            c.load_profile()
        );
        put!(
            "runtime.auth.session",
            SettingKind::Runtime,
            c.load_session()
        );
        put!("fixed.uooc.ai", SettingKind::Fixed, c.load_uooc_ai());
        put!(
            "runtime.uooc.session",
            SettingKind::Runtime,
            c.load_uooc_session()
        );
        if let Some(raw) = c.load_oj_accounts()? {
            let secrets: BTreeMap<String, crate::oj::model::Secret> =
                serde_json::from_str(&raw).map_err(|_| "OJ 账号数据损坏")?;
            let accounts: BTreeMap<_, _> = secrets.iter().map(|(id, secret)| (id.clone(), serde_json::json!({"username":secret.username,"password":secret.password,"url":secret.url}))).collect();
            let sessions: BTreeMap<_, _> = secrets
                .iter()
                .filter(|(_, secret)| !secret.cookies.is_empty())
                .map(|(id, secret)| (id.clone(), secret.cookies.clone()))
                .collect();
            out.insert(
                "fixed.oj.accounts".into(),
                SettingEntry {
                    kind: SettingKind::Fixed,
                    value: serde_json::to_value(accounts).map_err(|_| "设置序列化失败")?,
                },
            );
            out.insert(
                "runtime.oj.sessions".into(),
                SettingEntry {
                    kind: SettingKind::Runtime,
                    value: serde_json::to_value(sessions).map_err(|_| "设置序列化失败")?,
                },
            );
        }
        put!(
            "runtime.academic.sessions",
            SettingKind::Runtime,
            c.load_academic_sessions()
        );
        put!(
            "fixed.network.dorm.settings",
            SettingKind::Fixed,
            c.load_dorm_settings()
        );
        put!(
            "fixed.network.dorm.credentials",
            SettingKind::Fixed,
            c.load_dorm_credentials()
        );
        put!(
            "fixed.network.staff.settings",
            SettingKind::Fixed,
            c.load_staff_settings()
        );
        put!(
            "fixed.network.staff.credentials",
            SettingKind::Fixed,
            c.load_staff_credentials()
        );
        put!(
            "fixed.electricity.location",
            SettingKind::Fixed,
            c.load_electricity_settings()
        );
        Ok(out)
    }

    pub(crate) fn snapshot(&self) -> Result<SettingsSnapshot, String> {
        Ok(SettingsSnapshot {
            entries: self.snapshot_map()?,
        })
    }

    pub(crate) fn export(&self, password: &str) -> Result<String, String> {
        validate_password(password)?;
        let plain = serde_json::to_vec(&Package {
            version: FORMAT,
            entries: self.snapshot_map()?,
        })
        .map_err(|_| "设置编码失败")?;
        if plain.len() > MAX_PACKAGE {
            return Err("设置包过大".into());
        }
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let key = derive(password, &salt)?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let encrypted = cipher
            .encrypt((&nonce).into(), plain.as_ref())
            .map_err(|_| "设置加密失败")?;
        let mut bytes = Vec::with_capacity(1 + SALT_LEN + NONCE_LEN + encrypted.len());
        bytes.push(FORMAT);
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&encrypted);
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            bytes,
        ))
    }

    fn decrypt(&self, text: &str, password: &str) -> Result<Package, String> {
        validate_password(password)?;
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            text.trim(),
        )
        .map_err(|_| "设置包格式无效")?;
        if bytes.len() < 1 + SALT_LEN + NONCE_LEN + 16 || bytes.len() > MAX_PACKAGE {
            return Err("设置包格式无效".into());
        }
        if bytes[0] != FORMAT {
            return Err("不支持的设置包版本".into());
        }
        let key = derive(password, &bytes[1..1 + SALT_LEN])?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let plain = cipher
            .decrypt(
                bytes[1 + SALT_LEN..1 + SALT_LEN + NONCE_LEN].into(),
                &bytes[1 + SALT_LEN + NONCE_LEN..],
            )
            .map_err(|_| "密码错误或设置包已损坏")?;
        let package: Package = serde_json::from_slice(&plain).map_err(|_| "设置包内容无效")?;
        if package.version != FORMAT {
            return Err("不支持的设置包版本".into());
        }
        Ok(package)
    }

    pub(crate) fn preview(
        &self,
        text: &str,
        password: &str,
    ) -> Result<SettingsImportPreview, String> {
        let package = self.decrypt(text, password)?;
        self.validate_package(&package)?;
        let current = self.snapshot_map()?;
        Ok(SettingsImportPreview {
            conflicts: package
                .entries
                .keys()
                .filter(|k| current.contains_key(*k))
                .cloned()
                .collect(),
            incoming: package.entries.len(),
        })
    }

    pub(crate) fn import(
        &self,
        text: &str,
        password: &str,
        overwrite: bool,
    ) -> Result<SettingsImportPreview, String> {
        let package = self.decrypt(text, password)?;
        self.validate_package(&package)?;
        let incoming = package.entries.len();
        let current = self.snapshot_map()?;
        let conflicts: Vec<String> = package
            .entries
            .keys()
            .filter(|k| current.contains_key(*k))
            .cloned()
            .collect();
        for (key, entry) in package.entries {
            if !overwrite && current.contains_key(&key) {
                continue;
            }
            self.write_entry(&key, entry)?;
        }
        Ok(SettingsImportPreview {
            conflicts,
            incoming,
        })
    }

    fn validate_package(&self, package: &Package) -> Result<(), String> {
        for (key, entry) in &package.entries {
            let expected = if key.starts_with("runtime.") {
                SettingKind::Runtime
            } else {
                SettingKind::Fixed
            };
            if entry.kind != expected {
                return Err("设置分类与键不匹配".into());
            }
            let encoded = serde_json::to_string(&entry.value).map_err(|_| "设置值格式无效")?;
            let value: Value = serde_json::from_str(&encoded).map_err(|_| "设置值格式无效")?;
            self.validate_entry(key, entry.kind, value)?;
        }
        Ok(())
    }

    fn validate_entry(&self, key: &str, kind: SettingKind, value: Value) -> Result<(), String> {
        let expected = if key.starts_with("runtime.") {
            SettingKind::Runtime
        } else {
            SettingKind::Fixed
        };
        if kind != expected {
            return Err("设置分类与键不匹配".into());
        }
        match key {
            "fixed.account.profile" => {
                let _: crate::auth::model::StoredAccountProfile =
                    serde_json::from_value(value).map_err(|_| "账户设置格式无效")?;
            }
            "runtime.auth.session" => {
                let _: crate::auth::session::AuthSessionSecret =
                    serde_json::from_value(value).map_err(|_| "登录会话格式无效")?;
            }
            "fixed.uooc.ai" | "runtime.uooc.session" | "runtime.academic.sessions" => {
                let _: String = serde_json::from_value(value).map_err(|_| "设置格式无效")?;
            }
            "fixed.oj.accounts" => {
                let _: BTreeMap<String, OjAccount> =
                    serde_json::from_value(value).map_err(|_| "OJ 账号设置格式无效")?;
            }
            "runtime.oj.sessions" => {
                let _: BTreeMap<String, String> =
                    serde_json::from_value(value).map_err(|_| "OJ 会话格式无效")?;
            }
            "fixed.network.dorm.settings" => {
                let _: crate::campus_network::model::DormNetworkSettings =
                    serde_json::from_value(value).map_err(|_| "校园网设置格式无效")?;
            }
            "fixed.network.dorm.credentials" | "fixed.network.staff.credentials" => {
                let _: crate::campus_network::model::DormCustomCredentials =
                    serde_json::from_value(value).map_err(|_| "校园网凭据格式无效")?;
            }
            "fixed.network.staff.settings" => {
                let _: crate::campus_network::model::StaffNetworkSettings =
                    serde_json::from_value(value).map_err(|_| "校园网设置格式无效")?;
            }
            "fixed.electricity.location" => {
                let _: crate::campus_payments::model::ElectricityLocationSettings =
                    serde_json::from_value(value).map_err(|_| "电费设置格式无效")?;
            }
            _ => return Err("设置键无效".into()),
        }
        let _ = kind;
        Ok(())
    }

    fn write_entry(&self, key: &str, entry: SettingEntry) -> Result<(), String> {
        let v = entry.value;
        macro_rules! save {
            ($ty:ty, $method:ident) => {{
                let value: $ty = serde_json::from_value(v).map_err(|_| "设置值格式无效")?;
                self.credentials.$method(&value)
            }};
        }
        match key {
            "fixed.account.profile" => {
                save!(crate::auth::model::StoredAccountProfile, save_profile)
            }
            "runtime.auth.session" => save!(crate::auth::session::AuthSessionSecret, save_session),
            "fixed.uooc.ai" => save!(String, save_uooc_ai),
            "runtime.uooc.session" => save!(String, save_uooc_session),
            "fixed.oj.accounts" | "runtime.oj.sessions" => self.save_oj_entry(key, v),
            "runtime.academic.sessions" => save!(String, save_academic_sessions),
            "fixed.network.dorm.settings" => save!(
                crate::campus_network::model::DormNetworkSettings,
                save_dorm_settings
            ),
            "fixed.network.dorm.credentials" => save!(
                crate::campus_network::model::DormCustomCredentials,
                save_dorm_credentials
            ),
            "fixed.network.staff.settings" => save!(
                crate::campus_network::model::StaffNetworkSettings,
                save_staff_settings
            ),
            "fixed.network.staff.credentials" => save!(
                crate::campus_network::model::DormCustomCredentials,
                save_staff_credentials
            ),
            "fixed.electricity.location" => save!(
                crate::campus_payments::model::ElectricityLocationSettings,
                save_electricity_settings
            ),
            _ => Err("设置键无效".into()),
        }
    }

    fn save_oj_entry(&self, key: &str, value: Value) -> Result<(), String> {
        let mut secrets: BTreeMap<String, crate::oj::model::Secret> = self
            .credentials
            .load_oj_accounts()?
            .map(|raw| serde_json::from_str(&raw).map_err(|_| "OJ 账号数据损坏"))
            .transpose()?
            .unwrap_or_default();
        if key == "fixed.oj.accounts" {
            let accounts: BTreeMap<String, OjAccount> =
                serde_json::from_value(value).map_err(|_| "OJ 账号设置格式无效")?;
            for (id, account) in accounts {
                let cookies = secrets
                    .get(&id)
                    .map(|secret| secret.cookies.clone())
                    .unwrap_or_default();
                secrets.insert(
                    id,
                    crate::oj::model::Secret {
                        username: account.username,
                        password: account.password,
                        url: account.url,
                        cookies,
                    },
                );
            }
        } else {
            let sessions: BTreeMap<String, String> =
                serde_json::from_value(value).map_err(|_| "OJ 会话格式无效")?;
            for (id, cookies) in sessions {
                secrets.entry(id).or_default().cookies = cookies;
            }
        }
        self.credentials
            .save_oj_accounts(&serde_json::to_string(&secrets).map_err(|_| "OJ 账号序列化失败")?)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OjAccount {
    username: String,
    password: String,
    url: String,
}

fn validate_password(password: &str) -> Result<(), String> {
    if password.chars().count() < 8 || password.len() > 256 {
        Err("一次性密码长度需为 8 至 256 个字符".into())
    } else {
        Ok(())
    }
}
fn derive(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|_| "加密参数无效")?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| "密码派生失败")?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{testing::MemoryCredentialStore, CredentialStore};
    use std::sync::Arc;

    fn store() -> (SettingsStore, Arc<MemoryCredentialStore>) {
        let credentials = Arc::new(MemoryCredentialStore::default());
        (SettingsStore::new(credentials.clone()), credentials)
    }

    #[test]
    fn export_roundtrip_rejects_wrong_password_and_tampering() {
        let (store, _) = store();
        let text = store.export("password-123").unwrap();
        assert_eq!(store.preview(&text, "password-123").unwrap().incoming, 0);
        assert!(store.preview(&text, "wrong-password").is_err());
        let mut bytes =
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &text)
                .unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        let tampered =
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);
        assert!(store.preview(&tampered, "password-123").is_err());
        assert!(store.export("short").is_err());
    }

    #[test]
    fn merge_preserves_existing_conflicts_and_overwrite_replaces_them() {
        let original = crate::auth::model::StoredAccountProfile {
            student_id: "111111".into(),
            campus_card_number: "111111".into(),
            name: Some("local".into()),
            password: "local-pass".into(),
        };
        let imported = Package {
            version: FORMAT,
            entries: BTreeMap::from([(
                "fixed.account.profile".into(),
                SettingEntry {
                    kind: SettingKind::Fixed,
                    value: serde_json::to_value(crate::auth::model::StoredAccountProfile {
                        student_id: "222222".into(),
                        campus_card_number: "222222".into(),
                        name: Some("other".into()),
                        password: "other-pass".into(),
                    })
                    .unwrap(),
                },
            )]),
        };
        let (target, target_credentials) = store();
        target_credentials.save_profile(&original).unwrap();
        let encoded = encode_package(&imported, "password-123");
        let summary = target.import(&encoded, "password-123", false).unwrap();
        assert_eq!(summary.conflicts, ["fixed.account.profile"]);
        assert_eq!(
            target_credentials.load_profile().unwrap(),
            Some(original.clone())
        );
        target.import(&encoded, "password-123", true).unwrap();
        assert_eq!(
            target_credentials
                .load_profile()
                .unwrap()
                .unwrap()
                .student_id,
            "222222"
        );
    }

    fn encode_package(package: &Package, password: &str) -> String {
        let salt = [7u8; SALT_LEN];
        let nonce = [8u8; NONCE_LEN];
        let key = derive(password, &salt).unwrap();
        let cipher = XChaCha20Poly1305::new((&key).into());
        let encrypted = cipher
            .encrypt(
                (&nonce).into(),
                serde_json::to_vec(package).unwrap().as_ref(),
            )
            .unwrap();
        let mut bytes = vec![FORMAT];
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&encrypted);
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
    }
}
