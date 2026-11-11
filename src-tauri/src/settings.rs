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
pub(crate) struct SettingsSnapshot {
    pub entries: BTreeMap<String, SettingEntry>,
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

    pub(crate) fn snapshot(&self) -> Result<SettingsSnapshot, String> {
        Ok(SettingsSnapshot {
            entries: self.document()?.entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::testing::MemoryCredentialStore;
    use std::sync::Arc;

    #[test]
    fn empty_store_snapshots_no_entries() {
        let store = SettingsStore::new(Arc::new(MemoryCredentialStore::default()));
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.entries.is_empty());
    }
}
