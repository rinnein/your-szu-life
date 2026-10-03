use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
struct MemoryBackend {
    values: Mutex<BTreeMap<String, String>>,
    offline: AtomicBool,
    ignore_writes: AtomicBool,
}
impl SecretBackend for Arc<MemoryBackend> {
    fn read(&self, key: &str) -> Result<Option<String>> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        Ok(self.values.lock().unwrap().get(key).cloned())
    }
    fn write(&self, key: &str, value: Option<&str>) -> Result<()> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        if !self.ignore_writes.load(Ordering::SeqCst) {
            let mut values = self.values.lock().unwrap();
            if let Some(value) = value {
                values.insert(key.into(), value.into());
            } else {
                values.remove(key);
            }
        }
        Ok(())
    }
}
fn backends() -> (Arc<MemoryBackend>, Arc<MemoryBackend>) {
    (
        Arc::new(MemoryBackend::default()),
        Arc::new(MemoryBackend::default()),
    )
}
const SECRET: &str = r#"{"password":"temporary-offline-test-password"}"#;

#[test]
fn prefers_system_store_and_removes_legacy_only_after_verified_save() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    legacy.write("profile", Some(SECRET)).unwrap();
    let store = LinuxStore::new(
        directory.path().join("credentials"),
        primary.clone(),
        legacy.clone(),
    );
    assert_eq!(store.read("profile").unwrap().as_deref(), Some(SECRET));
    assert_eq!(primary.read("profile").unwrap().as_deref(), Some(SECRET));
    assert_eq!(legacy.read("profile").unwrap(), None);
    assert!(!store.directory.join("vault.bin").exists());
    assert_eq!(store.status().backend, "systemKeyring");
}

#[test]
fn fallback_is_encrypted_owner_only_and_survives_new_store_instances() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("credentials");
    let (primary, legacy) = backends();
    primary.offline.store(true, Ordering::SeqCst);
    let store = LinuxStore::new(root.clone(), primary.clone(), legacy.clone());
    store.write("profile", Some(SECRET)).unwrap();
    assert_eq!(store.status().backend, "localEncryptedFile");
    let first = fs::read(root.join("vault.bin")).unwrap();
    assert!(!first.windows(8).any(|slice| slice == b"password"));
    store.write("profile", Some(SECRET)).unwrap();
    assert_ne!(first, fs::read(root.join("vault.bin")).unwrap());
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for name in ["master.key", "vault.bin", "store.lock"] {
        assert_eq!(
            fs::metadata(root.join(name)).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(store);
    let reopened = LinuxStore::new(root, primary, legacy);
    assert_eq!(reopened.read("profile").unwrap().as_deref(), Some(SECRET));
}

#[test]
fn pending_password_wins_after_failed_verification_and_later_syncs() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    primary.write("profile", Some(r#""old""#)).unwrap();
    primary.ignore_writes.store(true, Ordering::SeqCst);
    let store = LinuxStore::new(
        directory.path().join("credentials"),
        primary.clone(),
        legacy,
    );
    store.write("profile", Some(SECRET)).unwrap();
    assert_eq!(store.read("profile").unwrap().as_deref(), Some(SECRET));
    assert!(store.directory.join("vault.bin").exists());
    primary.ignore_writes.store(false, Ordering::SeqCst);
    assert_eq!(store.read("profile").unwrap().as_deref(), Some(SECRET));
    assert_eq!(primary.read("profile").unwrap().as_deref(), Some(SECRET));
    assert!(!store.directory.join("vault.bin").exists());
}

#[test]
fn tombstones_prevent_deleted_credentials_from_reappearing() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    primary.write("profile", Some(SECRET)).unwrap();
    legacy.write("profile", Some(SECRET)).unwrap();
    primary.offline.store(true, Ordering::SeqCst);
    legacy.offline.store(true, Ordering::SeqCst);
    let store = LinuxStore::new(
        directory.path().join("credentials"),
        primary.clone(),
        legacy.clone(),
    );
    store.write("profile", None).unwrap();
    primary.offline.store(false, Ordering::SeqCst);
    assert_eq!(store.read("profile").unwrap(), None);
    assert!(store.directory.join("vault.bin").exists());
    legacy.offline.store(false, Ordering::SeqCst);
    assert_eq!(store.read("profile").unwrap(), None);
    assert_eq!(legacy.read("profile").unwrap(), None);
    assert_eq!(primary.read("profile").unwrap(), None);
    assert!(!store.directory.join("vault.bin").exists());
}

#[test]
fn legacy_values_reach_fallback_without_destroying_the_old_copy_early() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    primary.offline.store(true, Ordering::SeqCst);
    legacy.write("profile", Some(SECRET)).unwrap();
    let store = LinuxStore::new(
        directory.path().join("credentials"),
        primary,
        legacy.clone(),
    );
    assert_eq!(store.read("profile").unwrap().as_deref(), Some(SECRET));
    assert_eq!(legacy.read("profile").unwrap().as_deref(), Some(SECRET));
    assert!(store.directory.join("vault.bin").exists());
}

#[test]
fn corrupt_ciphertext_or_missing_key_cannot_be_overwritten() {
    for remove_key in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (primary, legacy) = backends();
        primary.offline.store(true, Ordering::SeqCst);
        let store = LinuxStore::new(directory.path().join("credentials"), primary, legacy);
        store.write("profile", Some(SECRET)).unwrap();
        if remove_key {
            fs::remove_file(store.directory.join("master.key")).unwrap();
        } else {
            let mut bytes = fs::read(store.directory.join("vault.bin")).unwrap();
            let end = bytes.len() - 1;
            bytes[end] ^= 1;
            fs::write(store.directory.join("vault.bin"), bytes).unwrap();
        }
        let before = fs::read(store.directory.join("vault.bin")).unwrap();
        assert_eq!(store.read("profile"), Err(StoreError::Corrupt));
        assert_eq!(
            store.write("profile", Some(r#""replacement""#)),
            Err(StoreError::Corrupt)
        );
        assert_eq!(before, fs::read(store.directory.join("vault.bin")).unwrap());
        assert!(!store.status().available);
        if remove_key {
            assert!(!store.directory.join("master.key").exists());
        }
    }
}

#[test]
fn invalid_legacy_schema_is_not_migrated_and_errors_have_no_secrets() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    legacy.write("profile", Some(SECRET)).unwrap();
    let store = LinuxStore::new(
        directory.path().join("credentials"),
        primary.clone(),
        legacy.clone(),
    );
    assert_eq!(
        store.read_validated("profile", |_| Err(StoreError::Corrupt)),
        Err(StoreError::Corrupt)
    );
    assert_eq!(primary.read("profile").unwrap(), None);
    assert_eq!(legacy.read("profile").unwrap().as_deref(), Some(SECRET));
    assert!(!StoreError::Corrupt.message().contains("temporary"));
}

#[test]
fn concurrent_store_instances_do_not_lose_other_keys() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("credentials");
    let (primary, legacy) = backends();
    primary.offline.store(true, Ordering::SeqCst);
    let workers: Vec<_> = (0..12)
        .map(|i| {
            let store = LinuxStore::new(root.clone(), primary.clone(), legacy.clone());
            std::thread::spawn(move || store.write(&format!("test-{i}"), Some(SECRET)).unwrap())
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let store = LinuxStore::new(root, primary, legacy);
    for i in 0..12 {
        assert_eq!(
            store.read(&format!("test-{i}")).unwrap().as_deref(),
            Some(SECRET)
        );
    }
}

#[test]
fn refuses_symlinked_credential_files() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    let store = LinuxStore::new(directory.path().join("credentials"), primary, legacy);
    store.prepare_directory().unwrap();
    let destination = directory.path().join("untouched");
    fs::write(&destination, b"unchanged").unwrap();
    std::os::unix::fs::symlink(&destination, store.directory.join("store.lock")).unwrap();
    assert_eq!(store.write("profile", Some(SECRET)), Err(StoreError::File));
    assert_eq!(fs::read(destination).unwrap(), b"unchanged");
}

#[test]
fn a_locked_existing_keyring_is_not_mistaken_for_a_missing_account() {
    let directory = tempfile::tempdir().unwrap();
    let (primary, legacy) = backends();
    let store = LinuxStore::new(
        directory.path().join("credentials"),
        primary.clone(),
        legacy,
    );
    store.write("profile", Some(SECRET)).unwrap();
    primary.offline.store(true, Ordering::SeqCst);
    assert_eq!(store.read("profile"), Err(StoreError::Unavailable));
    assert_eq!(store.read("never-saved-key").unwrap(), None);
    assert!(store.status().available);
    assert_eq!(store.status().backend, "localEncryptedFile");
    assert_eq!(
        primary
            .values
            .lock()
            .unwrap()
            .get("profile")
            .map(String::as_str),
        Some(SECRET)
    );
}

#[test]
fn file_store_child() {
    let Ok(root) = std::env::var("SZU_CREDENTIAL_TEST_DIRECTORY") else {
        return;
    };
    let (primary, legacy) = backends();
    primary.offline.store(true, Ordering::SeqCst);
    let store = LinuxStore::new(root.into(), primary, legacy);
    if std::env::var("SZU_CREDENTIAL_TEST_MODE").unwrap() == "write" {
        store.write("profile", Some(SECRET)).unwrap();
    } else {
        assert_eq!(store.read("profile").unwrap().as_deref(), Some(SECRET));
    }
}

#[test]
fn file_fallback_survives_a_complete_process_restart() {
    let directory = tempfile::tempdir().unwrap();
    for mode in ["write", "read"] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "storage::linux::tests::file_store_child"])
            .env(
                "SZU_CREDENTIAL_TEST_DIRECTORY",
                directory.path().join("credentials"),
            )
            .env("SZU_CREDENTIAL_TEST_MODE", mode)
            .output()
            .unwrap();
        assert!(result.status.success(), "isolated {mode} process failed");
    }
}
