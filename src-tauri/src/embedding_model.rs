use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

const MANIFEST: &str = include_str!("../resources/embedding/bge-small-zh-v1.5/model.json");
const GITHUB_ACCELERATORS: &str = include_str!("../../github-accelerator.json");
const HUGGINGFACE_ACCELERATORS: &str = include_str!("../../huggingface-accelerator.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    revision: String,
    sha256: String,
    bytes: u64,
    url: String,
}

impl Manifest {
    fn load() -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(MANIFEST).map_err(|_| "Embedding 模型配置无效")?;
        if manifest.revision.len() != 40
            || !manifest.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || manifest.sha256.len() != 64
            || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || manifest.bytes == 0
            || manifest.bytes > 256 * 1024 * 1024
        {
            return Err("Embedding 模型配置未通过校验".into());
        }
        let url = Url::parse(&manifest.url).map_err(|_| "Embedding 模型下载地址无效")?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Embedding 模型下载地址必须是无凭据 HTTPS 地址".into());
        }
        Ok(manifest)
    }

    fn verify(&self, path: &Path) -> bool {
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        if file.metadata().map(|m| m.len()).ok() != Some(self.bytes) {
            return false;
        }
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => hash.update(&buffer[..count]),
                Err(_) => return false,
            }
        }
        format!("{:x}", hash.finalize()) == self.sha256.to_ascii_lowercase()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelDownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmbeddingModelStatus {
    pub status: String,
    pub revision: String,
    pub sha256: String,
    pub bytes: u64,
    pub source: Option<String>,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_bytes_per_sec: Option<u64>,
}

struct ModelStore {
    manifest: Manifest,
    cache: PathBuf,
    bundled: Option<PathBuf>,
    state: Mutex<EmbeddingModelStatus>,
    operation: Mutex<()>,
}

static STORE: OnceLock<Arc<ModelStore>> = OnceLock::new();

pub(crate) mod commands;
pub(crate) mod mcp;

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let manifest = Manifest::load()?;
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
    let initial_status = EmbeddingModelStatus {
        status: "missing".into(),
        revision: manifest.revision.clone(),
        sha256: manifest.sha256.clone(),
        bytes: manifest.bytes,
        source: None,
        error: None,
        downloaded_bytes: None,
        total_bytes: None,
        speed_bytes_per_sec: None,
    };
    let store = Arc::new(ModelStore {
        manifest,
        cache,
        bundled,
        state: Mutex::new(initial_status),
        operation: Mutex::new(()),
    });
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

fn store() -> Result<Arc<ModelStore>, String> {
    if let Some(store) = STORE.get() {
        return Ok(store.clone());
    }
    #[cfg(test)]
    {
        let manifest = Manifest::load()?;
        let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/embedding-model-cache");
        let store = Arc::new(ModelStore {
            bundled: None,
            state: Mutex::new(EmbeddingModelStatus {
                status: "missing".into(),
                revision: manifest.revision.clone(),
                sha256: manifest.sha256.clone(),
                bytes: manifest.bytes,
                source: None,
                error: None,
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_sec: None,
            }),
            manifest,
            cache,
            operation: Mutex::new(()),
        });
        store.refresh_from_disk();
        let _ = STORE.set(store);
        return STORE
            .get()
            .cloned()
            .ok_or_else(|| "Embedding 模型服务尚未初始化".into());
    }
    #[cfg(not(test))]
    Err("Embedding 模型服务尚未初始化".into())
}

impl ModelStore {
    fn target(&self) -> PathBuf {
        self.cache.join(format!("{}.onnx", self.manifest.sha256))
    }

    fn set_status(&self, status: &str, source: Option<String>, error: Option<String>) {
        let mut value = self.state.lock().unwrap();
        value.status = status.into();
        value.source = source;
        value.error = error;
        if status != "downloading" {
            value.downloaded_bytes = None;
            value.total_bytes = None;
            value.speed_bytes_per_sec = None;
        }
    }

    fn refresh_from_disk(&self) {
        let target = self.target();
        if self.manifest.verify(&target) {
            self.set_status("ready", Some("cache".into()), None);
            return;
        }
        if let Some(bundled) = &self.bundled {
            if self.manifest.verify(bundled) {
                if let Err(error) = self.install_file(bundled) {
                    self.set_status("error", Some("bundled".into()), Some(error));
                } else {
                    self.set_status("ready", Some("bundled".into()), None);
                }
                return;
            }
        }
        self.set_status("missing", None, None);
    }

    fn install_file(&self, source: &Path) -> Result<(), String> {
        fs::create_dir_all(&self.cache).map_err(|e| format!("无法创建 Embedding 模型目录：{e}"))?;
        let temporary = self.cache.join(format!("{}.install", self.manifest.sha256));
        let mut input = File::open(source).map_err(|e| format!("无法读取内置 Embedding 模型：{e}"))?;
        let mut output = File::create(&temporary).map_err(|e| format!("无法写入 Embedding 模型：{e}"))?;
        std::io::copy(&mut input, &mut output).map_err(|e| format!("复制 Embedding 模型失败：{e}"))?;
        output
            .flush()
            .map_err(|e| format!("保存 Embedding 模型失败：{e}"))?;
        output
            .sync_all()
            .map_err(|e| format!("同步 Embedding 模型失败：{e}"))?;
        drop(output);
        if !self.manifest.verify(&temporary) {
            let _ = fs::remove_file(&temporary);
            return Err("内置 Embedding 模型校验失败".into());
        }
        if self.target().exists() {
            let _ = fs::remove_file(self.target());
        }
        fs::rename(temporary, self.target()).map_err(|e| format!("安装 Embedding 模型失败：{e}"))
    }

    fn sources(&self) -> Vec<(String, Url)> {
        let mut sources = Vec::new();
        let Ok(primary) = Url::parse(&self.manifest.url) else {
            return sources;
        };
        let host = primary.host_str().unwrap_or_default();

        if host.contains("huggingface") {
            sources.push(("huggingface".into(), primary.clone()));
            if let Ok(values) = serde_json::from_str::<Vec<String>>(HUGGINGFACE_ACCELERATORS) {
                for prefix in values {
                    let Ok(base) = Url::parse(&prefix) else {
                        continue;
                    };
                    if base.scheme() != "https"
                        || !base.username().is_empty()
                        || base.password().is_some()
                    {
                        continue;
                    }
                    let path = primary.path().trim_start_matches('/');
                    if let Ok(url) = base.join(path) {
                        sources.push(("huggingface-accelerator".into(), url));
                    }
                }
            }
        } else if host.contains("github") {
            sources.push(("github".into(), primary.clone()));
            if let Ok(values) = serde_json::from_str::<Vec<String>>(GITHUB_ACCELERATORS) {
                for prefix in values {
                    let candidate = format!("{prefix}{primary}");
                    if let Ok(url) = Url::parse(&candidate) {
                        if url.scheme() == "https"
                            && url.username().is_empty()
                            && url.password().is_none()
                        {
                            sources.push(("github-accelerator".into(), url));
                        }
                    }
                }
            }
        } else {
            sources.push(("direct".into(), primary.clone()));
        }

        sources
    }

    fn download(&self, app: Option<&tauri::AppHandle>, force: bool) -> Result<(), String> {
        let _guard = self.operation.lock().unwrap();
        if !force {
            self.refresh_from_disk();
        }
        if !force && self.state.lock().unwrap().status == "ready" {
            return Ok(());
        }
        self.set_status("downloading", None, None);
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| format!("无法初始化模型下载器：{e}"))?;
        let temporary = self
            .cache
            .join(format!("{}.download", self.manifest.sha256));
        let mut last_error = String::from("没有可用的模型下载源");
        for (source, url) in self.sources() {
            let result = (|| -> Result<(), String> {
                let mut response = client
                    .get(url.clone())
                    .send()
                    .and_then(reqwest::blocking::Response::error_for_status)
                    .map_err(|e| e.to_string())?;
                let content_len = response.content_length();
                if content_len.is_some_and(|size| size > self.manifest.bytes) {
                    return Err("模型响应超过固定大小".into());
                }
                let total_bytes = content_len.unwrap_or(self.manifest.bytes);
                let mut file = File::create(&temporary).map_err(|e| e.to_string())?;

                let mut buffer = [0_u8; 64 * 1024];
                let mut downloaded_bytes: u64 = 0;
                let mut last_emit = Instant::now();
                let mut window_start = Instant::now();
                let mut window_bytes: u64 = 0;
                let mut current_speed: u64 = 0;

                loop {
                    let n = match response.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) => return Err(e.to_string()),
                    };
                    file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                    downloaded_bytes += n as u64;
                    window_bytes += n as u64;

                    if downloaded_bytes > self.manifest.bytes {
                        return Err("模型响应超过固定大小".into());
                    }

                    let now = Instant::now();
                    let window_elapsed = now.duration_since(window_start);
                    if window_elapsed >= Duration::from_millis(400) {
                        current_speed = (window_bytes as f64 / window_elapsed.as_secs_f64()) as u64;
                        window_bytes = 0;
                        window_start = now;
                    }

                    if now.duration_since(last_emit) >= Duration::from_millis(100) {
                        last_emit = now;
                        let payload = ModelDownloadProgress {
                            downloaded_bytes,
                            total_bytes,
                            speed_bytes_per_sec: current_speed,
                        };
                        if let Some(app) = app {
                            let _ = app.emit("embedding-model-download-progress", payload);
                        }
                        let mut val = self.state.lock().unwrap();
                        val.downloaded_bytes = Some(downloaded_bytes);
                        val.total_bytes = Some(total_bytes);
                        val.speed_bytes_per_sec = Some(current_speed);
                    }
                }

                file.flush().map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                drop(file);

                let final_payload = ModelDownloadProgress {
                    downloaded_bytes,
                    total_bytes,
                    speed_bytes_per_sec: current_speed,
                };
                if let Some(app) = app {
                    let _ = app.emit("embedding-model-download-progress", final_payload);
                }

                if !self.manifest.verify(&temporary) {
                    let _ = fs::remove_file(&temporary);
                    return Err("模型哈希或大小校验失败".into());
                }
                if self.target().exists() {
                    let _ = fs::remove_file(self.target());
                }
                fs::rename(&temporary, self.target()).map_err(|e| e.to_string())?;
                self.set_status("ready", Some(source.clone()), None);
                Ok(())
            })();
            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    last_error = format!("{source}: {error}");
                }
            }
        }
        self.set_status("error", None, Some(last_error.clone()));
        Err(last_error)
    }
}

pub(crate) fn status() -> Result<EmbeddingModelStatus, String> {
    let store = store()?;
    let status = store.state.lock().unwrap().clone();
    Ok(status)
}

pub(crate) async fn download_command(
    app: tauri::AppHandle,
    force: bool,
) -> Result<EmbeddingModelStatus, String> {
    let store = store()?;
    store.set_status("downloading", None, None);
    let mut downloading_status = store.state.lock().unwrap().clone();
    downloading_status.downloaded_bytes = Some(0);
    downloading_status.total_bytes = Some(downloading_status.bytes);
    downloading_status.speed_bytes_per_sec = Some(0);
    let _ = app.emit("embedding-model-status-changed", downloading_status);
    let background = store.clone();
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let result = background.download(Some(&app_handle), force);
        let status = background.state.lock().unwrap().clone();
        let _ = app_handle.emit("embedding-model-status-changed", status);
        result
    })
    .await
    .map_err(|_| "Embedding 模型下载任务崩溃".to_owned())??;
    status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_loads_and_validates() {
        let manifest = Manifest::load().expect("Manifest should load successfully");
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
        let manifest = Manifest::load().expect("Manifest should load");
        let store = ModelStore {
            manifest,
            cache: PathBuf::from("/tmp"),
            bundled: None,
            state: Mutex::new(EmbeddingModelStatus {
                status: "missing".into(),
                revision: "".into(),
                sha256: "".into(),
                bytes: 0,
                source: None,
                error: None,
                downloaded_bytes: None,
                total_bytes: None,
                speed_bytes_per_sec: None,
            }),
            operation: Mutex::new(()),
        };
        let sources = store.sources();
        assert!(!sources.is_empty());
        assert_eq!(sources[0].0, "huggingface");
        assert!(sources.iter().any(|(name, _)| name == "huggingface-accelerator"));
    }

    #[test]
    fn test_verify_detects_hash_and_size_mismatch() {
        let manifest = Manifest::load().expect("Manifest should load");
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_embedding_verify.bin");
        // Incorrect size
        fs::write(&test_file, b"corrupted data").unwrap();
        assert!(!manifest.verify(&test_file));
        let _ = fs::remove_file(&test_file);
    }
}
