use ddddocr_core::{traits::Loader, ModelMetadata, Normalization, Ocr, Resize};
use ddddocr_tract2::{loader::ModelLoader, OcrRuntime};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
static ENGINE: Mutex<Option<OcrRuntime>> = Mutex::new(None);
const MODEL: &[u8] = include_bytes!(env!("SZU_LIFE_DDDDOCR_MODEL_PATH"));
pub(crate) fn recognize(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > 1024 * 1024 {
        return Err("验证码图片过大".into());
    }
    let image = image::load_from_memory(bytes).map_err(|_| "验证码图片无效")?;
    if image.width() > 2048 || image.height() > 2048 {
        return Err("验证码尺寸无效".into());
    }
    let mut engine = ENGINE.lock().map_err(|_| "验证码识别器不可用")?;
    if engine.is_none() {
        let metadata: serde_json::Value =
            serde_json::from_str(include_str!("../resources/ocr/ddddocr/model.json"))
                .map_err(|_| "模型配置无效")?;
        if format!("{:x}", Sha256::digest(MODEL)) != metadata["sha256"].as_str().unwrap_or("") {
            return Err("验证码模型校验失败".into());
        }
        let session = ModelLoader::default()
            .build_from_bytes(MODEL)
            .map_err(|_| "验证码模型加载失败")?;
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
        .map_err(|_| "验证码识别失败".into())
}
