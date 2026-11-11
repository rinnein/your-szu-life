use super::{CredentialStorageStatus, CredentialStore, PlatformCredentialStore};

#[tauri::command]
pub fn get_credential_storage_status() -> CredentialStorageStatus {
    PlatformCredentialStore.storage_status()
}
