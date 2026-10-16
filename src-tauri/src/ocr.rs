use ddddocr_core::{traits::Loader, ModelMetadata, Normalization, Ocr, Resize};
use ddddocr_tract2::{loader::ModelLoader, OcrRuntime};
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

const MANIFEST: &str = include_str!("../resources/ocr/ddddocr/model.json");
const GITHUB_ACCELERATORS: &str = include_str!("../../github-accelerator.json");
const HUGGINGFACE_ACCELERATORS: &str = include_str!("../../huggingface-accelerator.json");
const MAX_IMAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    repository: String,
    file: String,
    revision: String,
    sha256: String,
    bytes: u64,
    url: String,
}

impl Manifest {
    fn load() -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(MANIFEST).map_err(|_| "模型配置无效")?;
        if manifest.revision.len() != 40
            || !manifest.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || manifest.sha256.len() != 64
            || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || manifest.bytes == 0
            || manifest.bytes > 256 * 1024 * 1024
            || manifest.repository != "https://github.com/sml2h3/ddddocr"
            || manifest.file != "ddddocr/common.onnx"
        {
            return Err("模型配置未通过固定校验".into());
        }
        let url = Url::parse(&manifest.url).map_err(|_| "模型下载地址无效")?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("模型下载地址必须是无凭据 HTTPS 地址".into());
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
pub(crate) struct OcrModelStatus {
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
    state: Mutex<OcrModelStatus>,
    operation: Mutex<()>,
    engine: Mutex<Option<OcrRuntime>>,
}

static STORE: OnceLock<Arc<ModelStore>> = OnceLock::new();

pub(crate) mod commands;

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let manifest = Manifest::load()?;
    let cache = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定应用数据目录")?
        .join("ocr")
        .join("ddddocr");
    fs::create_dir_all(&cache).map_err(|e| format!("无法创建 OCR 模型目录：{e}"))?;
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|root| root.join("resources/ocr/ddddocr/common.onnx"))
        .filter(|path| path.is_file());
    let initial_status = OcrModelStatus {
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
        engine: Mutex::new(None),
    });
    store.set_status("checking", None, None);
    STORE
        .set(store.clone())
        .map_err(|_| "OCR 模型服务重复初始化".to_owned())?;
    let background = store;
    let app = app.clone();
    std::thread::Builder::new()
        .name("ocr-model-startup".into())
        .spawn(move || {
            background.refresh_from_disk();
            if let Ok(status) = status() {
                let _ = app.emit("ocr-model-status-changed", status);
            }
        })
        .map_err(|e| format!("无法启动 OCR 模型检查：{e}"))?;
    Ok(())
}

fn store() -> Result<Arc<ModelStore>, String> {
    if let Some(store) = STORE.get() {
        return Ok(store.clone());
    }
    #[cfg(test)]
    {
        let manifest = Manifest::load()?;
        let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/model-cache");
        let store = Arc::new(ModelStore {
            bundled: None,
            state: Mutex::new(OcrModelStatus {
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
            engine: Mutex::new(None),
        });
        store.refresh_from_disk();
        let _ = STORE.set(store);
        return STORE
            .get()
            .cloned()
            .ok_or_else(|| "OCR 模型服务尚未初始化".into());
    }
    #[cfg(not(test))]
    Err("OCR 模型服务尚未初始化".into())
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
        fs::create_dir_all(&self.cache).map_err(|e| format!("无法创建 OCR 模型目录：{e}"))?;
        let temporary = self.cache.join(format!("{}.install", self.manifest.sha256));
        let mut input = File::open(source).map_err(|e| format!("无法读取内置 OCR 模型：{e}"))?;
        let mut output = File::create(&temporary).map_err(|e| format!("无法写入 OCR 模型：{e}"))?;
        std::io::copy(&mut input, &mut output).map_err(|e| format!("复制 OCR 模型失败：{e}"))?;
        output
            .flush()
            .map_err(|e| format!("保存 OCR 模型失败：{e}"))?;
        output
            .sync_all()
            .map_err(|e| format!("同步 OCR 模型失败：{e}"))?;
        drop(output);
        if !self.manifest.verify(&temporary) {
            let _ = fs::remove_file(&temporary);
            return Err("内置 OCR 模型校验失败".into());
        }
        if self.target().exists() {
            let _ = fs::remove_file(self.target());
        }
        fs::rename(temporary, self.target()).map_err(|e| format!("安装 OCR 模型失败：{e}"))
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
                    return Err("下载的模型校验失败".into());
                }
                if self.target().exists() {
                    let _ = fs::remove_file(self.target());
                }
                fs::rename(&temporary, self.target()).map_err(|e| e.to_string())?;
                self.set_status("ready", Some(source), None);
                Ok(())
            })();
            if result.is_ok() {
                return Ok(());
            }
            last_error = format!("{url}: {}", result.unwrap_err());
            let _ = fs::remove_file(&temporary);
        }
        self.set_status("error", None, Some(last_error.clone()));
        Err(last_error)
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        let target = self.target();
        if !self.manifest.verify(&target) {
            return Err("ocr_model_missing".into());
        }
        fs::read(target).map_err(|_| "ocr_model_invalid".into())
    }
}

pub(crate) fn status() -> Result<OcrModelStatus, String> {
    Ok(store()?.state.lock().unwrap().clone())
}
async fn download_command(app: tauri::AppHandle, update: bool) -> Result<OcrModelStatus, String> {
    let initial = status()?;
    let _ = app.emit(
        "ocr-model-status-changed",
        OcrModelStatus {
            status: "downloading".into(),
            ..initial
        },
    );
    let result = tauri::async_runtime::spawn_blocking(move || {
        let s = store()?;
        if update {
            s.set_status("missing", None, None);
        }
        s.download(update)
    })
    .await
    .map_err(|e| format!("模型下载任务失败：{e}"))?;
    let current = status()?;
    let _ = app.emit("ocr-model-status-changed", current.clone());
    result.map(|_| current)
}

pub(crate) fn recognize(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("验证码图片过大".into());
    }
    let image = image::load_from_memory(bytes).map_err(|_| "验证码图片无效")?;
    if image.width() > 2048 || image.height() > 2048 {
        return Err("验证码尺寸无效".into());
    }
    let service = store()?;
    let mut engine = service.engine.lock().map_err(|_| "验证码识别器不可用")?;
    if engine.is_none() {
        let model = service.bytes()?;
        let session = ModelLoader::default()
            .build_from_bytes(&model)
            .map_err(|_| "ocr_model_invalid")?;
        *engine = Some(OcrRuntime::new(
            session,
            ModelMetadata::from_builtin_beta(
                false,
                Resize::DynamicWidth(64),
                1,
                Normalization::MinusOneToOne,
            ),
        ));
    }
    Ocr::builder()
        .build_with(engine.as_ref().unwrap())
        .predict(&image)
        .and_then(|result| match result {
            ddddocr_core::OcrResult::Text(text)
            | ddddocr_core::OcrResult::Probability { text, .. } => Ok(text),
            ddddocr_core::OcrResult::Unsupported { .. } => Ok(String::new()),
        })
        .map_err(|_| "captcha_recognition_failed".into())
}
