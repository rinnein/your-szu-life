use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EmbeddingTextParams {
    /// 待生成向量的输入文本
    pub text: String,
}

#[tool_router(router = embedding_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "embedding_get_vector",
        description = "使用本地 BGE-small-zh-v1.5 模型提取输入文本的 512 维语义嵌入向量"
    )]
    pub async fn embedding_get_vector(
        &self,
        Parameters(params): Parameters<EmbeddingTextParams>,
    ) -> Result<String, String> {
        let text = params.text;
        let vector = tokio::task::spawn_blocking(move || {
            crate::uooc::quiz::qa::compute_local_embedding(&text)
        })
        .await
        .map_err(|_| "向量计算任务失败".to_string())?;

        serde_json::to_string(&serde_json::json!({
            "dimension": vector.len(),
            "vector": vector,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "embedding_get_status",
        description = "查询本地 BGE 语义嵌入模型的加载状态、哈希校验及就绪指标"
    )]
    pub async fn embedding_get_status(&self) -> Result<String, String> {
        let status = crate::embedding_model::status()?;
        serde_json::to_string_pretty(&status).map_err(|e| format!("序列化失败: {e}"))
    }
}
