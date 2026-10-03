//! Secret Service with an encrypted, transactional fallback journal.
//! Pending records (including deletions) always take precedence over old stores.
use super::{CredentialStorageStatus, SERVICE_NAME};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    XChaCha20Poly1305, XNonce,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::Mutex,
};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"SZUCRED1";
const AAD: &[u8] = b"in.rinne.your-szu-life:credentials:v1";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
type Journal = BTreeMap<String, Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StoreError {
    Unavailable,
    Corrupt,
    File,
}
impl StoreError {
    pub fn message(self) -> String {
        match self {
            Self::Unavailable => super::CREDENTIALS_UNAVAILABLE,
            Self::Corrupt => "保存的凭据或加密文件损坏，已停止读取和覆盖；请恢复原文件或密钥",
            Self::File => "无法访问本机凭据文件，请检查应用数据目录权限和磁盘空间",
        }
        .into()
    }
}
type Result<T> = std::result::Result<T, StoreError>;

pub(super) trait SecretBackend: Send + Sync {
    fn read(&self, key: &str) -> Result<Option<String>>;
    fn write(&self, key: &str, value: Option<&str>) -> Result<()>;
}

pub(super) struct KeyringBackend {
    pub legacy: bool,
}
impl KeyringBackend {
    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        if self.legacy {
            let credential =
                keyring::keyutils::KeyutilsCredential::new_with_target(None, SERVICE_NAME, key)
                    .map_err(classify)?;
            Ok(keyring::Entry::new_with_credential(Box::new(credential)))
        } else {
            // Avoid opening an unlock dialog from background workers. A locked
            // or absent collection uses the already-authorized file fallback.
            self.check_unlocked(key)?;
            keyring::Entry::new(SERVICE_NAME, key).map_err(classify)
        }
    }

    fn check_unlocked(&self, key: &str) -> Result<()> {
        use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
        let connection =
            dbus::blocking::Connection::new_session().map_err(|_| StoreError::Unavailable)?;
        let timeout = std::time::Duration::from_secs(2);
        let proxy = connection.with_proxy(
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets",
            timeout,
        );
        let (collection,): (dbus::Path<'static>,) = proxy
            .method_call("org.freedesktop.Secret.Service", "ReadAlias", ("default",))
            .map_err(|_| StoreError::Unavailable)?;
        if collection.to_string() == "/" {
            return Err(StoreError::Unavailable);
        }
        let locked: bool = connection
            .with_proxy("org.freedesktop.secrets", collection, timeout)
            .get("org.freedesktop.Secret.Collection", "Locked")
            .map_err(|_| StoreError::Unavailable)?;
        if locked {
            return Err(StoreError::Unavailable);
        }
        let attributes =
            std::collections::HashMap::from([("service", SERVICE_NAME), ("username", key)]);
        let (_, locked): (Vec<dbus::Path<'static>>, Vec<dbus::Path<'static>>) = proxy
            .method_call(
                "org.freedesktop.Secret.Service",
                "SearchItems",
                (attributes,),
            )
            .map_err(|_| StoreError::Unavailable)?;
        if locked.is_empty() {
            Ok(())
        } else {
            Err(StoreError::Unavailable)
        }
    }
}
fn classify(error: keyring::Error) -> StoreError {
    match error {
        keyring::Error::BadEncoding(_) | keyring::Error::Ambiguous(_) => StoreError::Corrupt,
        _ => StoreError::Unavailable,
    }
}
impl SecretBackend for KeyringBackend {
    fn read(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(classify(error)),
        }
    }
    fn write(&self, key: &str, value: Option<&str>) -> Result<()> {
        let entry = self.entry(key)?;
        match value.map_or_else(
            || entry.delete_credential(),
            |value| entry.set_password(value),
        ) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(classify(error)),
        }
    }
}

pub(super) struct LinuxStore<P = KeyringBackend, L = KeyringBackend> {
    directory: PathBuf,
    primary: P,
    legacy: L,
    status: Mutex<CredentialStorageStatus>,
}
impl<P: SecretBackend, L: SecretBackend> LinuxStore<P, L> {
    pub fn new(directory: PathBuf, primary: P, legacy: L) -> Self {
        Self {
            directory,
            primary,
            legacy,
            status: Mutex::new(CredentialStorageStatus::system()),
        }
    }
    pub fn status(&self) -> CredentialStorageStatus {
        self.status
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| CredentialStorageStatus::file(false))
    }
    fn operation<T>(&self, work: impl FnOnce(&mut Journal) -> Result<(T, bool)>) -> Result<T> {
        let result = (|| {
            self.prepare_directory()?;
            let lock = self.open("store.lock", true)?;
            lock.lock().map_err(|_| StoreError::File)?;
            let mut journal = self.read_journal()?;
            self.read_catalog()?;
            let (value, primary_available) = work(&mut journal)?;
            let status = if journal.is_empty() && primary_available {
                CredentialStorageStatus::system()
            } else {
                CredentialStorageStatus::file(true)
            };
            *self.status.lock().map_err(|_| StoreError::File)? = status;
            Ok(value)
        })();
        if let Err(error) = &result {
            if let Ok(mut status) = self.status.lock() {
                if *error == StoreError::Unavailable {
                    *status = CredentialStorageStatus::file(true);
                } else {
                    status.available = false;
                }
            }
        }
        result
    }
    #[cfg(test)]
    pub fn read(&self, key: &str) -> Result<Option<String>> {
        self.read_validated(key, validate_json)
    }
    pub fn read_validated(
        &self,
        key: &str,
        validate: impl Fn(&str) -> Result<()>,
    ) -> Result<Option<String>> {
        self.operation(|journal| {
            if let Some(value) = journal.get(key).cloned() {
                if let Some(value) = &value {
                    validate(value)?;
                }
                let synced = self.reconcile(key, value.as_deref(), journal)?;
                return Ok((value, synced));
            }
            let primary_available = match self.primary.read(key) {
                Ok(Some(value)) => {
                    validate(&value)?;
                    self.record_primary(key, true)?;
                    return Ok((Some(value), true));
                }
                Ok(None) => true,
                Err(StoreError::Unavailable) => false,
                Err(error) => return Err(error),
            };
            match self.legacy.read(key) {
                Ok(Some(value)) => {
                    validate(&value)?;
                    journal.insert(key.into(), Some(value.clone()));
                    self.write_journal(journal)?;
                    let synced = self.reconcile(key, Some(&value), journal)?;
                    Ok((Some(value), synced))
                }
                Ok(None) | Err(StoreError::Unavailable) => {
                    if !primary_available && self.read_catalog()?.contains(key) {
                        return Err(StoreError::Unavailable);
                    }
                    Ok((None, primary_available))
                }
                Err(error) => Err(error),
            }
        })
    }
    pub fn write(&self, key: &str, value: Option<&str>) -> Result<()> {
        self.operation(|journal| {
            if let Some(value) = value {
                validate_json(value)?;
            }
            // Journal first: if the process stops between the two backends,
            // neither an old password nor a deleted account can reappear.
            journal.insert(key.into(), value.map(str::to_owned));
            self.write_journal(journal)?;
            let synced = self.reconcile(key, value, journal)?;
            Ok(((), synced))
        })
    }
    fn reconcile(&self, key: &str, value: Option<&str>, journal: &mut Journal) -> Result<bool> {
        if self.primary.write(key, value).is_err()
            || self
                .primary
                .read(key)
                .ok()
                .as_ref()
                .map(|value| value.as_deref())
                != Some(value)
            || self.legacy.write(key, None).is_err()
            || !matches!(self.legacy.read(key), Ok(None))
        {
            return Ok(false);
        }
        self.record_primary(key, value.is_some())?;
        journal.remove(key);
        self.write_journal(journal)?;
        Ok(true)
    }
    // Names only. Distinguish a new installation from credentials that exist
    // in a temporarily locked keyring but have no readable fallback copy.
    fn read_catalog(&self) -> Result<BTreeSet<String>> {
        if !self.exists("catalog.json")? {
            return Ok(BTreeSet::new());
        }
        serde_json::from_slice(&self.read_bytes("catalog.json")?).map_err(|_| StoreError::Corrupt)
    }
    fn record_primary(&self, key: &str, present: bool) -> Result<()> {
        let mut catalog = self.read_catalog()?;
        let changed = if present {
            catalog.insert(key.into())
        } else {
            catalog.remove(key)
        };
        if changed {
            self.atomic_write(
                "catalog.json",
                &serde_json::to_vec(&catalog).map_err(|_| StoreError::Corrupt)?,
            )?;
        }
        Ok(())
    }
    fn prepare_directory(&self) -> Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.directory)
            .map_err(|_| StoreError::File)?;
        let metadata = fs::symlink_metadata(&self.directory).map_err(|_| StoreError::File)?;
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(StoreError::File);
        }
        fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| StoreError::File)
    }
    fn open(&self, name: &str, create: bool) -> Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(create)
            .create(create)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(self.directory.join(name))
            .map_err(|_| StoreError::File)?;
        let metadata = file.metadata().map_err(|_| StoreError::File)?;
        if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(StoreError::File);
        }
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| StoreError::File)?;
        Ok(file)
    }
    fn exists(&self, name: &str) -> Result<bool> {
        match fs::symlink_metadata(self.directory.join(name)) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(StoreError::File),
        }
    }
    fn read_bytes(&self, name: &str) -> Result<Zeroizing<Vec<u8>>> {
        let mut bytes = Zeroizing::new(Vec::new());
        self.open(name, false)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| StoreError::File)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(StoreError::Corrupt);
        }
        Ok(bytes)
    }
    fn key(&self, create: bool) -> Result<Zeroizing<Vec<u8>>> {
        if !self.exists("master.key")? {
            if !create {
                return Err(StoreError::Corrupt);
            }
            let key = XChaCha20Poly1305::generate_key(&mut OsRng);
            self.atomic_write("master.key", &key)?;
        }
        let key = self.read_bytes("master.key")?;
        if key.len() != 32 {
            return Err(StoreError::Corrupt);
        }
        Ok(key)
    }
    fn read_journal(&self) -> Result<Journal> {
        if !self.exists("vault.bin")? {
            return Ok(Journal::new());
        }
        let bytes = self.read_bytes("vault.bin")?;
        if bytes.len() < MAGIC.len() + 24 + 16 || !bytes.starts_with(MAGIC) {
            return Err(StoreError::Corrupt);
        }
        let key = self.key(false)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| StoreError::Corrupt)?;
        let nonce: [u8; 24] = bytes[MAGIC.len()..MAGIC.len() + 24]
            .try_into()
            .map_err(|_| StoreError::Corrupt)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &bytes[MAGIC.len() + 24..],
                        aad: AAD,
                    },
                )
                .map_err(|_| StoreError::Corrupt)?,
        );
        serde_json::from_slice(&plaintext).map_err(|_| StoreError::Corrupt)
    }
    fn write_journal(&self, journal: &Journal) -> Result<()> {
        if journal.is_empty() {
            if self.exists("vault.bin")? {
                fs::remove_file(self.directory.join("vault.bin")).map_err(|_| StoreError::File)?;
            }
            return self.sync_directory();
        }
        let key = self.key(true)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| StoreError::Corrupt)?;
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let plaintext =
            Zeroizing::new(serde_json::to_vec(journal).map_err(|_| StoreError::Corrupt)?);
        if plaintext.len() as u64 > MAX_BYTES - 64 {
            return Err(StoreError::File);
        }
        let encrypted = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: AAD,
                },
            )
            .map_err(|_| StoreError::File)?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend(nonce);
        bytes.extend(encrypted);
        self.atomic_write("vault.bin", &bytes)
    }
    fn sync_directory(&self) -> Result<()> {
        File::open(&self.directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| StoreError::File)
    }
    fn atomic_write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let temporary = self
            .directory
            .join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, self.directory.join(name))?;
            Ok::<_, std::io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|_| StoreError::File)?;
        self.sync_directory()
    }
}
fn validate_json(value: &str) -> Result<()> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|_| StoreError::Corrupt)
}

#[cfg(test)]
mod tests;
