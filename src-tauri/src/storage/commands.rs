use super::{CredentialStorageStatus, CredentialStore, PlatformCredentialStore};

#[tauri::command]
pub(crate) fn get_credential_storage_status() -> CredentialStorageStatus {
    PlatformCredentialStore.storage_status()
}
