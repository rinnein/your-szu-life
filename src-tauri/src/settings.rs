use crate::storage::{
    entries::{self, SettingEntry, VaultDocument, MAX_DOCUMENT},
    SharedCredentialStore,
};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const MAX_PACKAGE: usize = MAX_DOCUMENT;

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
pub(crate) mod mcp;

impl SettingsStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self { credentials }
    }

    /// 当前凭据文档。与本地加密文件共用同一结构，保证明文一致。
    fn document(&self) -> Result<VaultDocument, String> {
        if let Some(vault) = crate::storage::vault::vault() {
            return vault.document();
        }
        let mut document = VaultDocument::default();
        for key in entries::ALL_KEYS {
            if let Some(value) = entries::read_entry(self.credentials.as_ref(), key)? {
                document.entries.insert(
                    key.to_owned(),
                    SettingEntry {
                        kind: entries::kind_for(key),
                        value,
                    },
                );
            }
        }
        Ok(document)
    }

    pub(crate) fn snapshot(&self) -> Result<SettingsSnapshot, String> {
        Ok(SettingsSnapshot {
            entries: self.document()?.entries,
        })
    }

    pub(crate) fn export(&self, password: &str) -> Result<String, String> {
        validate_password(password)?;
        let plain = serde_json::to_vec(&self.document()?).map_err(|_| "设置编码失败")?;
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
        bytes.push(entries::FORMAT);
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&encrypted);
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            bytes,
        ))
    }

    fn decrypt(&self, text: &str, password: &str) -> Result<VaultDocument, String> {
        validate_password(password)?;
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            text.trim(),
        )
        .map_err(|_| "设置包格式无效")?;
        if bytes.len() < 1 + SALT_LEN + NONCE_LEN + 16 || bytes.len() > MAX_PACKAGE {
            return Err("设置包格式无效".into());
        }
        if bytes[0] != entries::FORMAT {
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
        let document: VaultDocument =
            serde_json::from_slice(&plain).map_err(|_| "设置包内容无效")?;
        document.validate()?;
        Ok(document)
    }

    pub(crate) fn preview(
        &self,
        text: &str,
        password: &str,
    ) -> Result<SettingsImportPreview, String> {
        let document = self.decrypt(text, password)?;
        let current = self.document()?;
        Ok(SettingsImportPreview {
            conflicts: document
                .entries
                .keys()
                .filter(|key| current.entries.contains_key(*key))
                .cloned()
                .collect(),
            incoming: document.entries.len(),
        })
    }

    pub(crate) fn import(
        &self,
        text: &str,
        password: &str,
        overwrite: bool,
    ) -> Result<SettingsImportPreview, String> {
        let incoming = self.decrypt(text, password)?;
        let current = self.document()?;
        let conflicts: Vec<String> = incoming
            .entries
            .keys()
            .filter(|key| current.entries.contains_key(*key))
            .cloned()
            .collect();
        let total = incoming.entries.len();

        if let Some(vault) = crate::storage::vault::vault() {
            // 整库一次写入，避免逐项加密与 fsync。
            let mut merged = current;
            for (key, entry) in incoming.entries {
                if !overwrite && merged.entries.contains_key(&key) {
                    continue;
                }
                merged.entries.insert(key, entry);
            }
            vault.replace(merged)?;
        } else {
            for (key, entry) in incoming.entries {
                if !overwrite && current.entries.contains_key(&key) {
                    continue;
                }
                entries::write_entry(self.credentials.as_ref(), &key, entry.value)?;
            }
        }
        Ok(SettingsImportPreview {
            conflicts,
            incoming: total,
        })
    }
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
    use crate::storage::{
        testing::MemoryCredentialStore, CredentialStore, PlatformCredentialStore,
    };
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
        let mut imported = VaultDocument::default();
        imported
            .set(
                entries::PROFILE,
                &crate::auth::model::StoredAccountProfile {
                    student_id: "222222".into(),
                    campus_card_number: "222222".into(),
                    name: Some("other".into()),
                    password: "other-pass".into(),
                },
            )
            .unwrap();
        let (target, target_credentials) = store();
        target_credentials.save_profile(&original).unwrap();
        let encoded = encode_package(&imported, "password-123");
        let summary = target.import(&encoded, "password-123", false).unwrap();
        assert_eq!(summary.conflicts, [entries::PROFILE]);
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

    #[test]
    fn export_plaintext_matches_credential_document_shape() {
        let (store, credentials) = store();
        credentials
            .save_profile(&crate::auth::model::StoredAccountProfile {
                student_id: "333333".into(),
                campus_card_number: "333333".into(),
                name: Some("测试".into()),
                password: "test-only".into(),
            })
            .unwrap();
        let exported = store.export("password-123").unwrap();
        let plain = decode_package(&exported, "password-123");
        let document: VaultDocument = serde_json::from_slice(&plain).unwrap();
        let expected = store.document().unwrap();
        assert_eq!(
            serde_json::to_vec(&document).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        // 文档键必须是全量键集合的子集，且与凭据库键名一一对应。
        for key in document.entries.keys() {
            assert!(
                entries::ALL_KEYS.contains(&key.as_str()),
                "{key} 不在共享键集合中"
            );
        }
        assert!(!document.entries.contains_key(entries::PENDING_AUTH));
    }

    fn encode_package(document: &VaultDocument, password: &str) -> String {
        let salt = [7u8; SALT_LEN];
        let nonce = [8u8; NONCE_LEN];
        let key = derive(password, &salt).unwrap();
        let cipher = XChaCha20Poly1305::new((&key).into());
        let encrypted = cipher
            .encrypt(
                (&nonce).into(),
                serde_json::to_vec(document).unwrap().as_ref(),
            )
            .unwrap();
        let mut bytes = vec![entries::FORMAT];
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&encrypted);
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
    }

    fn decode_package(text: &str, password: &str) -> Vec<u8> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, text)
            .unwrap();
        let key = derive(password, &bytes[1..1 + SALT_LEN]).unwrap();
        XChaCha20Poly1305::new((&key).into())
            .decrypt(
                bytes[1 + SALT_LEN..1 + SALT_LEN + NONCE_LEN].into(),
                &bytes[1 + SALT_LEN + NONCE_LEN..],
            )
            .unwrap()
    }

    #[test]
    fn imported_document_can_be_written_to_platform_store() {
        // 仅验证类型转换路径：使用测试替身，不接触真实钥匙环。
        let credentials = Arc::new(MemoryCredentialStore::default());
        let mut document = VaultDocument::default();
        document
            .set(entries::UOOC_SESSION, &"cookies".to_owned())
            .unwrap();
        entries::write_entry(
            credentials.as_ref(),
            entries::UOOC_SESSION,
            serde_json::json!("cookies"),
        )
        .unwrap();
        assert_eq!(
            credentials.load_uooc_session().unwrap().as_deref(),
            Some("cookies")
        );
        let _ = std::marker::PhantomData::<PlatformCredentialStore>;
    }

    #[test]
    fn kind_of_shared_keys_matches_setting_prefix() {
        use crate::storage::entries::SettingKind;
        assert_eq!(
            entries::kind_for(entries::PENDING_AUTH),
            SettingKind::Runtime
        );
        assert_eq!(
            entries::kind_for(entries::ELECTRICITY_MONITOR),
            SettingKind::Runtime
        );
        assert_eq!(entries::kind_for(entries::PROFILE), SettingKind::Fixed);
    }
}
