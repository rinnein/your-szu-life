use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OcrImageParams {
    /// 待识别图像的 Base64 编码字符串（支持 data:image/...;base64, 前缀或纯 base64 文本）
    pub image_base64: String,
}

#[tool_router(router = ocr_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "ocr_recognize_image",
        description = "使用本地轻量级 ONNX OCR 模型对验证码或文档图片进行快速离线字符识别"
    )]
    pub async fn ocr_recognize_image(
        &self,
        Parameters(params): Parameters<OcrImageParams>,
    ) -> Result<String, String> {
        let raw = if let Some((_, data)) = params.image_base64.split_once(',') {
            data
        } else {
            &params.image_base64
        };
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw.trim())
            .map_err(|e| format!("Base64 解码失败: {e}"))?;

        let result = tokio::task::spawn_blocking(move || crate::ocr::recognize(&bytes))
            .await
            .map_err(|_| "OCR 推理任务失败".to_string())??;

        Ok(result)
    }

    #[tool(
        name = "ocr_get_status",
        description = "查询本地 OCR 模型的装载状态、文件校验哈希与可用性"
    )]
    pub async fn ocr_get_status(&self) -> Result<String, String> {
        let status = crate::ocr::status()?;
        serde_json::to_string_pretty(&status).map_err(|e| format!("序列化失败: {e}"))
    }
}
