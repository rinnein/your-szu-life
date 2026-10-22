use crate::local_model::{self, LocalModelStatus, LocalModelStore, Manifest};
use ddddocr_core::{traits::Loader, ModelMetadata, Normalization, Ocr, Resize};
use ddddocr_tract2::{loader::ModelLoader, OcrRuntime};
use std::{
    fs,
    sync::{Arc, Mutex, OnceLock},
};
use tauri::{Emitter, Manager};

const MANIFEST: &str = include_str!("../resources/ocr/ddddocr/model.json");
const MAX_IMAGE_BYTES: usize = 1024 * 1024;

pub(crate) type OcrModelStatus = LocalModelStatus;
#[allow(dead_code)]
pub(crate) type ModelDownloadProgress = local_model::ModelDownloadProgress;

pub(crate) struct OcrService {
    pub(crate) model: Arc<LocalModelStore>,
    pub(crate) engine: Mutex<Option<OcrRuntime>>,
}

static STORE: OnceLock<Arc<OcrService>> = OnceLock::new();

pub(crate) mod commands;
pub(crate) mod mcp;

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let manifest = Manifest::load(MANIFEST, "OCR")?;
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
    let model = Arc::new(LocalModelStore::new(
        manifest,
        cache,
        bundled,
        "OCR",
        "ocr-model-download-progress",
        "ocr-model-status-changed",
    ));
    model.set_status("checking", None, None);
    let service = Arc::new(OcrService {
        model: model.clone(),
        engine: Mutex::new(None),
    });
    STORE
        .set(service)
        .map_err(|_| "OCR 模型服务重复初始化".to_owned())?;
    let app = app.clone();
    std::thread::Builder::new()
        .name("ocr-model-startup".into())
        .spawn(move || {
            model.refresh_from_disk();
            if let Ok(status) = status() {
                let _ = app.emit("ocr-model-status-changed", status);
            }
        })
        .map_err(|e| format!("无法启动 OCR 模型检查：{e}"))?;
    Ok(())
}

fn store() -> Result<Arc<OcrService>, String> {
    if let Some(store) = STORE.get() {
        return Ok(store.clone());
    }
    #[cfg(test)]
    {
        let manifest = Manifest::load(MANIFEST, "OCR")?;
        let cache = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/model-cache");
        let model = Arc::new(LocalModelStore::new(
            manifest,
            cache,
            None,
            "OCR",
            "ocr-model-download-progress",
            "ocr-model-status-changed",
        ));
        let service = Arc::new(OcrService {
            model,
            engine: Mutex::new(None),
        });
        let _ = STORE.set(service.clone());
        return STORE
            .get()
            .cloned()
            .ok_or_else(|| "OCR 模型服务尚未初始化".into());
    }
    #[cfg(not(test))]
    Err("OCR 模型服务尚未初始化".into())
}

pub(crate) fn status() -> Result<OcrModelStatus, String> {
    let service = store()?;
    Ok(service.model.status())
}

pub(crate) async fn download_command(
    app: tauri::AppHandle,
    force: bool,
) -> Result<OcrModelStatus, String> {
    let service = store()?;
    if force {
        let mut engine = service.engine.lock().map_err(|_| "验证码识别器不可用")?;
        *engine = None;
    }
    service.model.clone().download_command(app, force).await
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
        let model = service.model.bytes()?;
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
