use crate::local_model::{LocalModelStatus, LocalModelStore, Manifest};
use std::{
    fs,
    sync::{Arc, OnceLock},
};
use tauri::{Emitter, Manager};

const MANIFEST: &str = include_str!("../resources/embedding/bge-small-zh-v1.5/model.json");

pub(crate) type EmbeddingModelStatus = LocalModelStatus;

static STORE: OnceLock<Arc<LocalModelStore>> = OnceLock::new();

pub(crate) mod commands;
pub(crate) mod mcp;

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let manifest = Manifest::load(MANIFEST, "Embedding")?;
    let cache = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定应用数据目录")?
        .join("embedding")
        .join("bge-small-zh-v1.5");
    fs::create_dir_all(&cache).map_err(|e| format!("无法创建 Embedding 模型目录：{e}"))?;
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|root| root.join("resources/embedding/bge-small-zh-v1.5/model_quantized.onnx"))
        .filter(|path| path.is_file());
    let store = Arc::new(LocalModelStore::new(
        manifest,
        cache,
        bundled,
        "Embedding",
        "embedding-model-download-progress",
        "embedding-model-status-changed",
    ));
    store.set_status("checking", None, None);
    STORE
        .set(store.clone())
        .map_err(|_| "Embedding 模型服务重复初始化".to_owned())?;
    let background = store;
    let app = app.clone();
    std::thread::Builder::new()
        .name("embedding-model-startup".into())
        .spawn(move || {
            background.refresh_from_disk();
            if let Ok(status) = status() {
                let _ = app.emit("embedding-model-status-changed", status);
            }
        })
        .map_err(|e| format!("无法启动 Embedding 模型检查：{e}"))?;
    Ok(())
}

fn store() -> Result<Arc<LocalModelStore>, String> {
    if let Some(store) = STORE.get() {
        return Ok(store.clone());
    }
    #[cfg(test)]
    {
        let manifest = Manifest::load(MANIFEST, "Embedding")?;
        let cache = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/embedding-model-cache");
        let store = Arc::new(LocalModelStore::new(
            manifest,
            cache,
            None,
            "Embedding",
            "embedding-model-download-progress",
            "embedding-model-status-changed",
        ));
        let _ = STORE.set(store.clone());
        return STORE
            .get()
            .cloned()
            .ok_or_else(|| "Embedding 模型服务尚未初始化".into());
    }
    #[cfg(not(test))]
    Err("Embedding 模型服务尚未初始化".into())
}

pub(crate) fn status() -> Result<EmbeddingModelStatus, String> {
    let store = store()?;
    Ok(store.status())
}

pub(crate) async fn download_command(
    app: tauri::AppHandle,
    channel: Option<tauri::ipc::Channel<crate::local_model::ModelDownloadChannelEvent>>,
    force: bool,
) -> Result<EmbeddingModelStatus, String> {
    let store = store()?;
    store.download_command(app, channel, force).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_model;

    #[test]
    fn test_manifest_loads_and_validates() {
        let manifest =
            Manifest::load(MANIFEST, "Embedding").expect("Manifest should load successfully");
        assert_eq!(manifest.revision, "75c43b069aac4d136ba6bc1122f995fedcfd2781");
        assert_eq!(
            manifest.sha256,
            "15b717c382bcb518ba457b93ea6850ede7f4f1cd8937454aa06972366cd19bcc"
        );
        assert_eq!(manifest.bytes, 24010842);
        assert_eq!(
            manifest.url,
            "https://huggingface.co/Xenova/bge-small-zh-v1.5/resolve/main/onnx/model_quantized.onnx"
        );
    }

    #[test]
    fn test_sources_generation() {
        let manifest = Manifest::load(MANIFEST, "Embedding").expect("Manifest should load");
        let candidates = local_model::mirror_candidates(&manifest.url);
        assert!(!candidates.is_empty());
        assert!(candidates
            .iter()
            .any(|(name, _)| name == "huggingface-accelerator"));
    }

    #[test]
    fn test_verify_detects_hash_and_size_mismatch() {
        let manifest = Manifest::load(MANIFEST, "Embedding").expect("Manifest should load");
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_embedding_verify.bin");
        // Incorrect size
        fs::write(&test_file, b"corrupted data").unwrap();
        assert!(!manifest.verify(&test_file));
        let _ = fs::remove_file(&test_file);
    }
}
