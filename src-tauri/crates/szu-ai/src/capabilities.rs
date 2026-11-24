//! 模型能力目录：根据供应商与模型名自动匹配 `toolCalling` / `vision` /
//! 上下文与输出限制，并推导 VS Code schema 里的 model `url`。
//!
//! 这里只是尽力而为的静态目录：未识别的模型回退到供应商级默认值，用户仍可在
//! 设置里用 checkbox 与数字输入覆盖。

use crate::config::{ModelConfig, Vendor};

/// 自动匹配一个模型的能力，返回 `(toolCalling, vision, maxInputTokens,
/// maxOutputTokens)`。上下文总数按输入 + 输出计算。
pub fn match_capabilities(vendor: Vendor, model: &str) -> (bool, bool, u64, u64) {
    let id = model.trim().to_ascii_lowercase();
    let known = |tool, vision, context, output| (tool, vision, context - output, output);
    if id.is_empty() {
        return fallback(vendor);
    }
    if id.contains("gpt-4.1") || id.contains("gpt-4.1-mini") || id.contains("gpt-4.1-nano") {
        return known(true, true, 1_047_576, 32_768);
    }
    if id.contains("gpt-4o-mini") || id.contains("gpt-4o") {
        return known(true, true, 128_000, 16_384);
    }
    if id.contains("gpt-4-turbo") {
        return known(true, true, 128_000, 4_096);
    }
    if id == "gpt-4" || id.starts_with("gpt-4-") {
        return known(true, false, 8_192, 4_096);
    }
    if id.contains("gpt-3.5") {
        return known(true, false, 16_385, 4_096);
    }
    if id.contains("gpt-5") {
        return known(true, true, 400_000, 128_000);
    }
    if id.contains("o4-mini") || id.contains("o3") || id.starts_with("o1") {
        return known(true, true, 200_000, 100_000);
    }
    if id.contains("claude")
        && (id.contains("opus-4") || id.contains("sonnet-4") || id.contains("3-7"))
    {
        return known(true, true, 200_000, 64_000);
    }
    if id.contains("claude-3-5") {
        return known(true, true, 200_000, 8_192);
    }
    if id.contains("claude") {
        return known(true, true, 200_000, 4_096);
    }
    if id.contains("gemini-2.5") {
        return known(true, true, 1_048_576, 65_536);
    }
    if id.contains("gemini-2.0") {
        return known(true, true, 1_048_576, 8_192);
    }
    if id.contains("gemini-1.5-pro") {
        return known(true, true, 2_097_152, 8_192);
    }
    if id.contains("gemini-1.5") || id.contains("gemini") {
        return known(true, true, 1_048_576, 8_192);
    }
    if id.contains("deepseek-reasoner") || id.contains("deepseek-r1") {
        return known(false, false, 128_000, 8_192);
    }
    if id.contains("deepseek") {
        return known(true, false, 128_000, 8_192);
    }
    if id.contains("qwen") && (id.contains("-vl") || id.contains("vision")) {
        return known(true, true, 131_072, 8_192);
    }
    if id.contains("qwen") {
        return known(true, false, 131_072, 8_192);
    }
    if id.contains("llava") || id.contains("vision") {
        return known(true, true, 32_768, 4_096);
    }
    if id.contains("llama-3.3") || id.contains("llama-3.1") {
        return known(true, false, 128_000, 4_096);
    }
    if id.contains("mixtral") || id.contains("mistral") {
        return known(true, false, 32_768, 8_192);
    }
    fallback(vendor)
}

fn fallback(vendor: Vendor) -> (bool, bool, u64, u64) {
    match vendor {
        Vendor::Anthropic => (true, true, 191_808, 8_192),
        Vendor::Gemini => (true, true, 1_040_384, 8_192),
        Vendor::Ollama => (true, false, 28_672, 4_096),
        Vendor::Dify => (false, false, 128_000, 8_192),
        Vendor::OpenAi
        | Vendor::Azure
        | Vendor::CustomEndpoint
        | Vendor::Deepseek
        | Vendor::OpenRouter => (true, false, 119_808, 8_192),
    }
}

/// 按 VS Code `chatLanguageModels.json` 的 model 结构生成一个条目。`url` 只由
/// vendor 与 provider 根地址推导，不读取用户的输入。
pub fn model_config(vendor: Vendor, model: &str, base_url: &str) -> ModelConfig {
    let model = model.trim();
    let (tool_calling, vision, max_input_tokens, max_output_tokens) =
        match_capabilities(vendor, model);
    ModelConfig {
        id: model.to_string(),
        name: model.to_string(),
        url: model_url(vendor, base_url, model),
        tool_calling,
        vision,
        max_input_tokens: Some(max_input_tokens),
        max_output_tokens: Some(max_output_tokens),
        context_window: None,
    }
}

/// VS Code schema 的 model `url`。Gemini 用原生 generateContent 路径，Ollama
/// 用原生 `/api/chat`，Dify 直接用聊天分享地址。
pub fn model_url(vendor: Vendor, base_url: &str, model: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return String::new();
    }
    match vendor.protocol() {
        crate::config::Protocol::OpenAiCompatible => format!("{base}/chat/completions"),
        crate::config::Protocol::Anthropic => format!("{base}/v1/messages"),
        crate::config::Protocol::Gemini => {
            format!("{base}/v1beta/models/{}:generateContent", model.trim())
        }
        crate::config::Protocol::Ollama => format!("{base}/api/chat"),
        crate::config::Protocol::DifyChat => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Protocol;

    #[test]
    fn matches_common_models() {
        let (tool, vision, input, output) = match_capabilities(Vendor::OpenAi, "gpt-4.1");
        assert!(tool);
        assert!(vision);
        assert_eq!(input, 1_014_808);
        assert_eq!(output, 32_768);

        let (tool, vision, _, _) = match_capabilities(Vendor::Deepseek, "deepseek-reasoner");
        assert!(!tool);
        assert!(!vision);

        let (tool, vision, _, _) = match_capabilities(Vendor::Anthropic, "claude-sonnet-4");
        assert!(tool);
        assert!(vision);

        let (tool, vision, _, _) = match_capabilities(Vendor::Gemini, "gemini-2.5-pro");
        assert!(tool);
        assert!(vision);
    }

    #[test]
    fn falls_back_by_vendor() {
        let (tool, vision, input, output) = match_capabilities(Vendor::Ollama, "unknown");
        assert!(tool);
        assert!(!vision);
        assert_eq!(input + output, 32_768);

        let (tool, vision, _, _) = match_capabilities(Vendor::Anthropic, "some-model");
        assert!(tool);
        assert!(vision);

        let (tool, vision, _, _) = match_capabilities(Vendor::Dify, DIFY_TEST_MODEL);
        assert!(!tool);
        assert!(!vision);
    }

    const DIFY_TEST_MODEL: &str = "chatbot";

    #[test]
    fn derives_model_urls() {
        assert_eq!(
            model_url(Vendor::OpenAi, "https://api.openai.com/v1", "gpt-4.1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            model_url(Vendor::Anthropic, "https://api.anthropic.com", "claude"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            model_url(
                Vendor::Gemini,
                "https://generativelanguage.googleapis.com",
                "gemini-2.5-pro"
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(
            model_url(Vendor::Ollama, "http://127.0.0.1:11434", "qwen3"),
            "http://127.0.0.1:11434/api/chat"
        );
        assert_eq!(
            model_url(
                Vendor::Dify,
                "https://dify.example.com/chatbot/abc",
                "chatbot"
            ),
            "https://dify.example.com/chatbot/abc"
        );
        assert_eq!(model_url(Vendor::OpenAi, "", "gpt"), "");
        assert_eq!(
            crate::config::normalize_base_url(
                "https://api.openai.com/v1",
                Protocol::OpenAiCompatible
            )
            .unwrap(),
            "https://api.openai.com/v1"
        );
    }
}
