//! 系统钥匙环与本地加密凭据文件的唯一接触点。
//!
//! 钥匙环里只保留一个条目（`vault-key`），其内容是 32 字节随机主密钥；
//! 全部凭据存于 `<app_data>/credentials/vault.bin`，以该主密钥加密。
//! 其它模块只能通过 [`super::CredentialStore`] 访问凭据。

use super::entries::{VaultDocument, FORMAT, MAX_DOCUMENT};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const MAGIC: &[u8; 8] = b"SZUVAULT";
const VAULT_VERSION: u8 = 1;
pub(crate) const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = MAGIC.len() + 1;
const VAULT_FILE: &str = "vault.bin";
const VAULT_KEY_ENTRY: &str = "vault-key";

/// 旧版钥匙环分块标记；读取与清理都集中在本文件。
pub(crate) const LEGACY_CHUNK_PREFIX: &str = "__CHUNKED__:";
/// 旧版分块清理的探测上限，避免异常数据导致过量访问。
pub(crate) const LEGACY_MAX_CHUNKS: usize = 128;

static VAULT: OnceLock<Vault> = OnceLock::new();

#[cfg(not(target_os = "android"))]
fn keyring_entry(service: &str, user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, user).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
fn read_keyring_secret(service: &str, user: &str) -> Result<Option<Vec<u8>>, String> {
    match keyring_entry(service, user)?.get_secret() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(target_os = "android"))]
fn write_keyring_secret(service: &str, user: &str, secret: &[u8]) -> Result<(), String> {
    keyring_entry(service, user)?
        .set_secret(secret)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
fn delete_keyring_secret(service: &str, user: &str) -> Result<(), String> {
    match keyring_entry(service, user)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "android")]
type AndroidStore = std::sync::Arc<android_native_keyring_store::Store>;

#[cfg(target_os = "android")]
fn android_store() -> Result<AndroidStore, String> {
    static STORE: OnceLock<Result<AndroidStore, String>> = OnceLock::new();
    STORE
        .get_or_init(|| android_native_keyring_store::Store::new().map_err(|e| e.to_string()))
        .clone()
}

#[cfg(target_os = "android")]
fn android_entry(service: &str, user: &str) -> Result<keyring_core::Entry, String> {
    use keyring_core::api::CredentialStoreApi;
    android_store()?
        .build(service, user, None)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn read_keyring_secret(service: &str, user: &str) -> Result<Option<Vec<u8>>, String> {
    match android_entry(service, user)?.get_secret() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "android")]
fn write_keyring_secret(service: &str, user: &str, secret: &[u8]) -> Result<(), String> {
    android_entry(service, user)?
        .set_secret(secret)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn delete_keyring_secret(service: &str, user: &str) -> Result<(), String> {
    match android_entry(service, user)?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// 读取旧版条目原文（自动重组 `__CHUNKED__:` 分块）。迁移专用。
pub(crate) fn legacy_read(service: &str, name: &str) -> Result<Option<String>, String> {
    let first = match read_keyring_secret(service, name)? {
        Some(bytes) => String::from_utf8(bytes).map_err(|_| "旧版凭据编码无效")?,
        None => return Ok(None),
    };
    let Some(rest) = first.strip_prefix(LEGACY_CHUNK_PREFIX) else {
        return Ok(Some(first));
    };
    let count: usize = rest.parse().map_err(|_| "旧版分块凭据格式无效")?;
    if count > LEGACY_MAX_CHUNKS {
        return Err("旧版分块凭据数量异常".into());
    }
    let mut value = String::new();
    for index in 0..count {
        let chunk = read_keyring_secret(service, &format!("{name}#{index}"))?
            .ok_or_else(|| format!("旧版凭据分块 {index} 缺失"))?;
        value.push_str(&String::from_utf8(chunk).map_err(|_| "旧版分块编码无效")?);
    }
    Ok(Some(value))
}

/// 删除旧版条目及其分块。条目本就不存在视为成功。
pub(crate) fn legacy_purge(service: &str, name: &str) -> Result<(), String> {
    if let Some(bytes) = read_keyring_secret(service, name)? {
        if let Ok(text) = String::from_utf8(bytes) {
            if let Some(rest) = text.strip_prefix(LEGACY_CHUNK_PREFIX) {
                if let Ok(count) = rest.parse::<usize>() {
                    for index in 0..count.min(LEGACY_MAX_CHUNKS) {
                        delete_keyring_secret(service, &format!("{name}#{index}"))?;
                    }
                }
            }
        }
    }
    delete_keyring_secret(service, name)
}

/// 凭据库：主密钥 + 内存文档 + 加密文件。
pub(crate) struct Vault {
    dir: PathBuf,
    key: [u8; KEY_LEN],
    document: Mutex<Option<VaultDocument>>,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("dir", &self.dir).finish()
    }
}

impl Vault {
    pub(crate) fn open(dir: PathBuf, key: [u8; KEY_LEN]) -> Self {
        Self {
            dir,
            key,
            document: Mutex::new(None),
        }
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.dir.join(VAULT_FILE)
    }

    pub(crate) fn exists(&self) -> bool {
        self.path().exists()
    }

    fn locked(&self) -> Result<std::sync::MutexGuard<'_, Option<VaultDocument>>, String> {
        let mut guard = self
            .document
            .lock()
            .map_err(|_| "凭据存储锁失败".to_string())?;
        if guard.is_none() {
            *guard = Some(load_document(&self.path(), &self.key)?);
        }
        Ok(guard)
    }

    pub(crate) fn load<T: for<'a> serde::Deserialize<'a>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, String> {
        let guard = self.locked()?;
        guard.as_ref().expect("凭据文档已加载").get(key)
    }

    /// 读-改-写在同一把锁内完成，避免并发丢更新。
    pub(crate) fn update<T>(
        &self,
        action: impl FnOnce(&mut VaultDocument) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.locked()?;
        let mut next = guard.as_ref().expect("凭据文档已加载").clone();
        let output = action(&mut next)?;
        write_document(&self.path(), &next, &self.key)?;
        *guard = Some(next);
        Ok(output)
    }

    /// 用给定文档整体替换（导入设置时使用）。
    pub(crate) fn replace(&self, document: VaultDocument) -> Result<(), String> {
        document.validate()?;
        write_document(&self.path(), &document, &self.key)?;
        *self.locked()? = Some(document);
        Ok(())
    }

    /// 当前文档快照，供设置导出使用。
    pub(crate) fn document(&self) -> Result<VaultDocument, String> {
        let guard = self.locked()?;
        Ok(guard.as_ref().expect("凭据文档已加载").clone())
    }

    /// 用给定文档覆盖写入并回读校验；迁移在删除旧条目之前调用。
    pub(crate) fn write_verified(&self, document: &VaultDocument) -> Result<(), String> {
        write_document(&self.path(), document, &self.key)?;
        let bytes = fs::read(self.path()).map_err(|e| format!("无法回读凭据文件: {e}"))?;
        let reread = decrypt(&bytes, &self.key)?;
        let expected = serde_json::to_vec(document).map_err(|e| e.to_string())?;
        let actual = serde_json::to_vec(&reread).map_err(|e| e.to_string())?;
        if expected != actual {
            return Err("凭据文件写入校验失败".into());
        }
        *self.locked()? = Some(document.clone());
        Ok(())
    }
}

pub(crate) fn install(vault: Vault) -> Result<(), String> {
    VAULT
        .set(vault)
        .map_err(|_| "凭据存储已经初始化".to_string())
}

pub(crate) fn vault() -> Option<&'static Vault> {
    VAULT.get()
}

/// 解析主密钥：优先钥匙环，缺失时生成并写入。
pub(crate) fn load_or_create_key(service: &str) -> Result<[u8; KEY_LEN], String> {
    match read_keyring_secret(service, VAULT_KEY_ENTRY)? {
        Some(bytes) => {
            if bytes.len() != KEY_LEN {
                return Err("系统密钥环中的主密钥长度异常".into());
            }
            let mut key = [0u8; KEY_LEN];
            key.copy_from_slice(&bytes);
            Ok(key)
        }
        None => {
            let mut key = [0u8; KEY_LEN];
            rand::rng().fill_bytes(&mut key);
            write_keyring_secret(service, VAULT_KEY_ENTRY, &key)?;
            Ok(key)
        }
    }
}

fn aad() -> Vec<u8> {
    let mut aad = Vec::with_capacity(HEADER_LEN);
    aad.extend_from_slice(MAGIC);
    aad.push(VAULT_VERSION);
    aad
}

pub(crate) fn encrypt(document: &VaultDocument, key: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    let plain = serde_json::to_vec(document).map_err(|e| format!("凭据文档序列化失败: {e}"))?;
    if plain.len() > MAX_DOCUMENT {
        return Err("凭据文档过大".into());
    }
    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let encrypted = XChaCha20Poly1305::new(key.into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &aad(),
            },
        )
        .map_err(|_| "凭据文档加密失败".to_string())?;
    let mut bytes = Vec::with_capacity(HEADER_LEN + NONCE_LEN + encrypted.len());
    bytes.extend_from_slice(&aad());
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&encrypted);
    Ok(bytes)
}

pub(crate) fn decrypt(bytes: &[u8], key: &[u8; KEY_LEN]) -> Result<VaultDocument, String> {
    if bytes.len() < HEADER_LEN + NONCE_LEN + TAG_LEN || bytes.len() > MAX_DOCUMENT + 256 {
        return Err("凭据文件格式无效".into());
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err("凭据文件格式无效".into());
    }
    if bytes[MAGIC.len()] != VAULT_VERSION {
        return Err("不支持的凭据文件版本".into());
    }
    let nonce = &bytes[HEADER_LEN..HEADER_LEN + NONCE_LEN];
    let plain = XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: &bytes[HEADER_LEN + NONCE_LEN..],
                aad: &aad(),
            },
        )
        .map_err(|_| "凭据文件已损坏或主密钥不匹配".to_string())?;
    let document: VaultDocument =
        serde_json::from_slice(&plain).map_err(|_| "凭据文档内容无效".to_string())?;
    if document.version != FORMAT {
        return Err("不支持的凭据文档版本".into());
    }
    Ok(document)
}

/// 加载文档；不存在时返回空文档，损坏时隔离后返回空文档。
fn load_document(path: &Path, key: &[u8; KEY_LEN]) -> Result<VaultDocument, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(VaultDocument::default()),
        Err(e) => return Err(format!("无法读取凭据文件: {e}")),
    };
    match decrypt(&bytes, key) {
        Ok(document) => Ok(document),
        Err(error) => {
            let stamp = crate::logging::timestamp_now().replace([':', ' ', '.'], "-");
            let quarantine = path.with_extension(format!("bin.corrupt-{stamp}"));
            log::error!(
                "凭据文件无法解密，已隔离到 {}: {error}",
                quarantine.display()
            );
            let _ = fs::rename(path, &quarantine);
            Ok(VaultDocument::default())
        }
    }
}

fn write_document(
    path: &Path,
    document: &VaultDocument,
    key: &[u8; KEY_LEN],
) -> Result<(), String> {
    let bytes = encrypt(document, key)?;
    let parent = path.parent().ok_or("凭据目录无效")?;
    fs::create_dir_all(parent).map_err(|e| format!("无法创建凭据目录: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("无法保存凭据文件: {e}"))
}

/// 迁移完成后清除旧后端残留的主密钥与索引文件。
pub(crate) fn cleanup_legacy_files(dir: &Path) {
    for name in ["master.key", "catalog.json", "store.lock"] {
        let path = dir.join(name);
        if path.exists() {
            match fs::remove_file(&path) {
                Ok(()) => log::info!("已清理旧版凭据文件 {}", path.display()),
                Err(error) => log::warn!("无法清理旧版凭据文件 {}: {error}", path.display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::entries;

    fn vault() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path().to_path_buf(), [7u8; KEY_LEN]);
        (dir, vault)
    }

    #[test]
    fn roundtrip_and_reopen() {
        let (dir, vault) = vault();
        vault
            .update(|document| {
                document.set(entries::PROFILE, &serde_json::json!({"student_id":"1"}))
            })
            .unwrap();
        assert!(vault.path().exists());
        let reopened = Vault::open(dir.path().to_path_buf(), [7u8; KEY_LEN]);
        let stored: Option<serde_json::Value> = reopened.load(entries::PROFILE).unwrap();
        assert_eq!(stored.unwrap()["student_id"], "1");
    }

    #[test]
    fn wrong_key_is_rejected_and_file_quarantined() {
        let (dir, vault) = vault();
        vault.replace(VaultDocument::default()).unwrap();
        let bytes = fs::read(vault.path()).unwrap();
        assert!(decrypt(&bytes, &[9u8; KEY_LEN]).is_err());
        let reopened = Vault::open(dir.path().to_path_buf(), [9u8; KEY_LEN]);
        assert!(reopened.document().unwrap().entries.is_empty());
        assert!(!vault.path().exists(), "损坏文件应被隔离");
        assert!(fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-")));
    }

    #[test]
    fn concurrent_updates_do_not_lose_writes() {
        let (_dir, vault) = vault();
        let vault = std::sync::Arc::new(vault);
        vault
            .update(|document| document.set(entries::UOOC_SESSION, &"0".to_owned()))
            .unwrap();
        let handles: Vec<_> = (0..8u32)
            .map(|index| {
                let vault = vault.clone();
                std::thread::spawn(move || {
                    vault
                        .update(|document| document.set(entries::UOOC_SESSION, &index.to_string()))
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let final_value: String = vault.load(entries::UOOC_SESSION).unwrap().unwrap();
        assert!(final_value.parse::<u32>().is_ok());
    }

    #[test]
    fn verified_write_detects_content() {
        let (_dir, vault) = vault();
        let mut document = VaultDocument::default();
        document
            .set(
                entries::ELECTRICITY_LOCATION,
                &serde_json::json!({"roomName":"101"}),
            )
            .unwrap();
        vault.write_verified(&document).unwrap();
        let loaded = vault.document().unwrap();
        assert_eq!(loaded.entries.len(), 1);
    }
}
