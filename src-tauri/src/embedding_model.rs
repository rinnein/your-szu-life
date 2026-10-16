use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tauri::{Emitter, Manager};

const MANIFEST: &str = include_str!("../resources/embedding/bge-small-zh-v1.5/model.json");
const ACCELERATORS: &str = include_str!("../../huggingface-accelerator.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Manifest {
    repository: String,
    file: String,
    revision: String,
    sha256: String,
    bytes: u64,
    #[serde(default)]
    huggingface_url: Option<String>,
    #[serde(default)]
    upstream_url: Option<String>,
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
        for value in [
            manifest.huggingface_url.as_deref(),
            manifest.upstream_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let url = Url::parse(value).map_err(|_| "Embedding 模型下载地址无效")?;
            if url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("Embedding 模型下载地址必须是无凭据 HTTPS 地址".into());
            }
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmbeddingModelStatus {
    pub status: String,
    pub revision: String,
    pub sha256: String,
    pub bytes: u64,
    pub source: Option<String>,
    pub error: Option<String>,
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
        let primary = self
            .manifest
            .huggingface_url
            .as_deref()
            .and_then(|v| Url::parse(v).ok());
        if let Some(primary) = primary {
            sources.push(("huggingface".into(), primary.clone()));
            if let Ok(values) = serde_json::from_str::<Vec<String>>(ACCELERATORS) {
                for prefix in values {
                    let Ok(prefix) = Url::parse(&prefix) else {
                        continue;
                    };
                    if prefix.scheme() != "https"
                        || !prefix.username().is_empty()
                        || prefix.password().is_some()
                    {
                        continue;
                    }
                    let path = primary.path().trim_start_matches('/');
                    if let Ok(url) = prefix.join(path) {
                        sources.push(("accelerator".into(), url));
                    }
                }
            }
        }
        if let Some(upstream) = self
            .manifest
            .upstream_url
            .as_deref()
            .and_then(|v| Url::parse(v).ok())
        {
            sources.push(("upstream".into(), upstream));
        }
        sources
    }

    fn download(&self, force: bool) -> Result<(), String> {
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
                let response = client
                    .get(url.clone())
                    .send()
                    .and_then(reqwest::blocking::Response::error_for_status)
                    .map_err(|e| e.to_string())?;
                if response
                    .content_length()
                    .is_some_and(|size| size > self.manifest.bytes)
                {
                    return Err("模型响应超过固定大小".into());
                }
                let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
                std::io::copy(&mut response.take(self.manifest.bytes + 1), &mut file)
                    .map_err(|e| e.to_string())?;
                file.flush().map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                drop(file);
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
    let downloading_status = store.state.lock().unwrap().clone();
    let _ = app.emit("embedding-model-status-changed", downloading_status);
    let background = store.clone();
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let result = background.download(force);
        let status = background.state.lock().unwrap().clone();
        let _ = app_handle.emit("embedding-model-status-changed", status);
        result
    })
    .await
    .map_err(|_| "Embedding 模型下载任务崩溃".to_owned())??;
    status()
}
