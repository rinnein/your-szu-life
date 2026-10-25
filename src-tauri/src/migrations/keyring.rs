//! 旧版「每个用途一个钥匙环条目」到单一加密凭据文件的迁移。
//!
//! 迁移只在 `vault.bin` 不存在时执行一次。写入后先回读校验，确认新文件
//! 可解密且内容一致，才删除旧条目；旧后端残留的密钥文件随后清理。
//!
//! 旧条目访问通过 [`LegacySource`] 注入：生产环境使用系统钥匙环，
//! 单元测试只能使用内存实现，从结构上避免测试触碰真实凭据。

use crate::storage::{
    entries::{self, VaultDocument},
    vault::Vault,
    SERVICE_NAME,
};
use std::path::Path;

/// 旧版条目的读取与删除能力。
pub(crate) trait LegacySource {
    /// 读取原始值；已自动重组分块，缺失返回 `None`。
    fn read(&self, name: &str) -> Result<Option<String>, String>;
    /// 删除条目及其全部分块；本就不存在视为成功。
    fn purge(&self, name: &str) -> Result<(), String>;
}

/// 真实系统钥匙环实现。仅由 [`crate::storage::initialize`] 使用。
pub(crate) struct KeyringSource;

impl LegacySource for KeyringSource {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        crate::storage::vault::legacy_read(SERVICE_NAME, name)
    }
    fn purge(&self, name: &str) -> Result<(), String> {
        crate::storage::vault::legacy_purge(SERVICE_NAME, name)
    }
}

/// 迁移结果，便于启动日志区分「已迁移」「无需迁移」「部分跳过」。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MigrationReport {
    pub migrated: bool,
    pub imported: usize,
    pub skipped: Vec<&'static str>,
    pub purged: usize,
}

fn purged_marker(dir: &Path) -> std::path::PathBuf {
    dir.join(".legacy-purged")
}

/// 用系统钥匙环执行迁移。
pub(crate) fn migrate_keyring_entries(vault: &Vault) -> Result<MigrationReport, String> {
    migrate_legacy_entries(vault, &KeyringSource)
}

/// 执行迁移。`vault` 必须已完成主密钥加载。
pub(crate) fn migrate_legacy_entries(
    vault: &Vault,
    legacy: &dyn LegacySource,
) -> Result<MigrationReport, String> {
    let mut report = MigrationReport::default();

    if vault.exists() {
        // 新文件已在，绝不覆盖；仅补做一次旧条目清理。
        report.purged = purge_legacy(vault.dir(), legacy)?;
        return Ok(report);
    }

    let mut document = VaultDocument::default();
    for (legacy_name, _) in entries::LEGACY_KEYS {
        match legacy.read(legacy_name) {
            Ok(Some(raw)) => match entries::insert_legacy(&mut document, legacy_name, &raw) {
                Ok(()) => report.imported += 1,
                Err(error) => {
                    log::warn!("旧版凭据 {legacy_name} 解析失败，已跳过: {error}");
                    report.skipped.push(legacy_name_name(legacy_name));
                }
            },
            Ok(None) => {}
            Err(error) => {
                log::warn!("旧版凭据 {legacy_name} 读取失败，已跳过: {error}");
                report.skipped.push(legacy_name_name(legacy_name));
            }
        }
    }

    document.validate()?;
    vault.write_verified(&document)?;
    report.migrated = report.imported > 0;
    report.purged = purge_legacy(vault.dir(), legacy)?;
    if report.migrated {
        log::info!(
            "凭据迁移完成：导入 {} 项，跳过 {} 项，清理旧条目 {} 个",
            report.imported,
            report.skipped.len(),
            report.purged
        );
    }
    Ok(report)
}

fn legacy_name_name(name: &str) -> &'static str {
    entries::LEGACY_KEYS
        .iter()
        .find(|(legacy, _)| *legacy == name)
        .map(|(legacy, _)| *legacy)
        .unwrap_or("unknown")
}

/// 删除全部旧条目；成功后落一个标记，避免每次启动重复访问钥匙环。
fn purge_legacy(dir: &Path, legacy: &dyn LegacySource) -> Result<usize, String> {
    let marker = purged_marker(dir);
    if marker.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    let mut failures = 0;
    for (legacy_name, _) in entries::LEGACY_KEYS {
        match legacy.purge(legacy_name) {
            Ok(()) => removed += 1,
            Err(error) => {
                failures += 1;
                log::warn!("清理旧版凭据 {legacy_name} 失败: {error}");
            }
        }
    }
    crate::storage::vault::cleanup_legacy_files(dir);
    // 清理未全部成功时不落标记，下次启动会重试，避免残留条目永久留存。
    if failures == 0 {
        if let Err(error) = write_marker(&marker) {
            log::warn!("无法写入凭据迁移标记: {error}");
        }
    }
    Ok(removed)
}

fn write_marker(path: &Path) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(path)?.write_all(b"1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::vault::{KEY_LEN, LEGACY_CHUNK_PREFIX};
    use std::{collections::BTreeMap, sync::Mutex};

    /// 内存旧条目源。测试绝不接触真实钥匙环。
    #[derive(Default)]
    struct MemorySource {
        entries: Mutex<BTreeMap<String, String>>,
        purged: Mutex<Vec<String>>,
        fail: Vec<&'static str>,
        fail_purge: bool,
    }

    impl MemorySource {
        fn with(entries: &[(&str, &str)]) -> Self {
            let source = Self::default();
            for (name, value) in entries {
                source
                    .entries
                    .lock()
                    .unwrap()
                    .insert((*name).to_owned(), (*value).to_owned());
            }
            source
        }
    }

    impl LegacySource for MemorySource {
        fn read(&self, name: &str) -> Result<Option<String>, String> {
            if self.fail.contains(&name) {
                return Err("simulated legacy failure".into());
            }
            Ok(self.entries.lock().unwrap().get(name).cloned())
        }
        fn purge(&self, name: &str) -> Result<(), String> {
            if self.fail_purge {
                return Err("simulated purge failure".into());
            }
            self.entries.lock().unwrap().remove(name);
            self.purged.lock().unwrap().push(name.to_owned());
            Ok(())
        }
    }

    fn vault() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().to_path_buf(), [5u8; KEY_LEN]);
        (dir, vault)
    }

    #[test]
    fn fresh_install_writes_empty_document_and_marks_purged() {
        let (dir, vault) = vault();
        let source = MemorySource::default();
        let report = migrate_legacy_entries(&vault, &source).unwrap();
        assert!(!report.migrated);
        assert_eq!(report.imported, 0);
        assert!(vault.exists());
        assert!(dir.path().join(".legacy-purged").exists());
        assert_eq!(
            source.purged.lock().unwrap().len(),
            entries::LEGACY_KEYS.len()
        );
    }

    #[test]
    fn existing_vault_is_never_overwritten() {
        let (_dir, vault) = vault();
        let mut document = VaultDocument::default();
        document
            .set(entries::UOOC_SESSION, &"keep".to_owned())
            .unwrap();
        vault.write_verified(&document).unwrap();
        let source = MemorySource::with(&[("uooc-session", r#""legacy""#)]);
        let report = migrate_legacy_entries(&vault, &source).unwrap();
        assert!(!report.migrated);
        assert_eq!(report.imported, 0);
        let value: Option<String> = vault.load(entries::UOOC_SESSION).unwrap();
        assert_eq!(value.as_deref(), Some("keep"));
    }

    #[test]
    fn legacy_values_are_imported_then_purged() {
        let (dir, vault) = vault();
        let source = MemorySource::with(&[
            (
                "profile",
                r#"{"student_id":"20230001","campus_card_number":"123456","name":"测试","password":"secret"}"#,
            ),
            (
                "oj-accounts",
                r#"{"szu-1":{"username":"alice","password":"secret","url":"http://example/","cookies":"c"}}"#,
            ),
            ("uooc-session", "\"cookies\""),
        ]);
        let report = migrate_legacy_entries(&vault, &source).unwrap();
        assert!(report.migrated);
        assert_eq!(report.imported, 3);
        let document = vault.document().unwrap();
        assert!(document.entries.contains_key(entries::PROFILE));
        assert!(document.entries.contains_key(entries::OJ_ACCOUNTS));
        assert!(document.entries.contains_key(entries::OJ_SESSIONS));
        // 迁移后旧条目全部删除，且标记落盘。
        assert!(source.entries.lock().unwrap().is_empty());
        assert!(dir.path().join(".legacy-purged").exists());
    }

    #[test]
    fn broken_entry_is_skipped_without_blocking_others() {
        let (_dir, vault) = vault();
        let source =
            MemorySource::with(&[("uooc-session", "\"cookies\""), ("profile", "not-json")]);
        let report = migrate_legacy_entries(&vault, &source).unwrap();
        assert!(report.migrated);
        assert_eq!(report.imported, 1);
        assert_eq!(report.skipped.len(), 1);
        let document = vault.document().unwrap();
        assert!(document.entries.contains_key(entries::UOOC_SESSION));
        assert!(!document.entries.contains_key(entries::PROFILE));
    }

    #[test]
    fn read_failure_is_skipped_and_reported() {
        let (_dir, vault) = vault();
        let mut source = MemorySource::with(&[("uooc-session", "\"cookies\"")]);
        source.fail.push("auth-session");
        let report = migrate_legacy_entries(&vault, &source).unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.skipped, ["auth-session"]);
    }

    #[test]
    fn chunk_prefix_contract_stays_stable() {
        // 旧数据重组依赖该前缀，改动会破坏升级用户的迁移。
        assert_eq!(LEGACY_CHUNK_PREFIX, "__CHUNKED__:");
    }

    #[test]
    fn purge_failure_leaves_no_marker_so_next_start_retries() {
        let (dir, vault) = vault();
        let mut source = MemorySource::with(&[("uooc-session", "\"cookies\"")]);
        source.fail_purge = true;
        let report = migrate_legacy_entries(&vault, &source).unwrap();
        assert_eq!(report.purged, 0);
        assert!(
            !dir.path().join(".legacy-purged").exists(),
            "清理失败时不应写入标记"
        );
        // 下次启动仍会尝试清理。
        source.fail_purge = false;
        let retry = migrate_legacy_entries(&vault, &source).unwrap();
        assert_eq!(retry.purged, entries::LEGACY_KEYS.len());
        assert!(dir.path().join(".legacy-purged").exists());
    }

    #[test]
    #[cfg(unix)]
    fn purge_marker_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, vault) = vault();
        let source = MemorySource::default();
        migrate_legacy_entries(&vault, &source).unwrap();
        let mode = std::fs::metadata(dir.path().join(".legacy-purged"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
