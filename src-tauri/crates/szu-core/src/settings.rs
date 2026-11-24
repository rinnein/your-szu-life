//! 设置快照（只读）。
//!
//! 跨设备配置改由设备同步模块按白名单逐项应用；这里只保留 MCP 与资源接口需要的
//! 只读快照。加密导出/导入已经移除：它会把不可共享的 authserver / lms / jxxk
//! 会话一起打包，且无法表达「某项冲突就不同步」的语义。

use crate::storage::{
    entries::{self, SettingEntry},
    SharedCredentialStore,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub entries: BTreeMap<String, SettingEntry>,
}

pub struct SettingsStore {
    credentials: SharedCredentialStore,
}

pub mod commands;

impl SettingsStore {
    pub fn new(credentials: SharedCredentialStore) -> Self {
        Self { credentials }
    }

    /// 当前凭据文档。与本地加密文件共用同一结构，保证明文一致。
    fn document(&self) -> Result<crate::storage::entries::VaultDocument, String> {
        if let Some(vault) = crate::storage::vault::vault() {
            return vault.document();
        }
        let mut document = crate::storage::entries::VaultDocument::default();
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

    pub fn snapshot(&self) -> Result<SettingsSnapshot, String> {
        let mut document = self.document()?;
        // 快照经 MCP 暴露给外部客户端，必须剔除带密钥/会话的条目；否则
        // fixed.ai.config 与 fixed.uooc.ai 会把 API Key 一起送出去。
        document
            .entries
            .retain(|key, _| !SENSITIVE_SNAPSHOT_KEYS.contains(&key.as_str()));
        Ok(SettingsSnapshot {
            entries: document.entries,
        })
    }
}

const SENSITIVE_SNAPSHOT_KEYS: &[&str] = &[
    entries::PROFILE,
    entries::PENDING_AUTH,
    entries::AUTH_SESSION,
    entries::AI_CONFIG,
    entries::AI_DEFAULT,
    entries::UOOC_AI,
    entries::UOOC_SESSION,
    entries::LMS_SESSION,
    entries::JXXK_SESSION,
    entries::QW_SESSION,
    entries::OJ_ACCOUNTS,
    entries::OJ_SESSIONS,
    entries::ACADEMIC_SESSIONS,
    entries::DORM_CREDENTIALS,
    entries::STAFF_CREDENTIALS,
    entries::PROXY_SETTINGS,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::testing::MemoryCredentialStore;
    use crate::storage::CredentialStore;
    use std::sync::Arc;

    #[test]
    fn empty_store_snapshots_no_entries() {
        let store = SettingsStore::new(Arc::new(MemoryCredentialStore::default()));
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.entries.is_empty());
    }

    #[test]
    fn snapshot_omits_ai_and_session_secrets() {
        let store = Arc::new(MemoryCredentialStore::default());
        store
            .save_ai_config(
                &r#"{"providers":[{"id":"p","name":"p","protocol":"openAiCompatible","baseUrl":"https://example.com/v1","apiKey":"secret-key","models":[]}],"default":{"providerId":"p","model":"m"}}"#
                    .to_string(),
            )
            .unwrap();
        store
            .save_uooc_ai(&r#"{"batchSize":5,"webSearch":true}"#.to_string())
            .unwrap();
        let snapshot = SettingsStore::new(store).snapshot().unwrap();
        assert!(!snapshot.entries.contains_key(entries::AI_CONFIG));
        assert!(!snapshot.entries.contains_key(entries::UOOC_AI));
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("secret-key"));
    }
}
