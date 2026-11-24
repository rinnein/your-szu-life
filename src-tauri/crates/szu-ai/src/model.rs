use crate::config::{Config, Protocol, ProviderConfig, Vendor};
use rig_core::operation::Completion;
use rig_core::providers::{
    anthropic::AnthropicConfig, gemini::GeminiConfig, ollama::OllamaConfig, openai::OpenAIConfig,
};
use rig_core::DynModel;
use rig_reqwest::ReqwestClient;
use std::time::Duration;

/// 所有 rig HTTP 请求都必须走应用自己的 netproxy 客户端，保留代理、校园网
/// DNS/绕过与 Android 平台 TLS；不要改用 rig 的 shared client。
pub fn transport(timeout: Duration, connect_timeout: Duration) -> Result<ReqwestClient, String> {
    #[cfg(target_os = "android")]
    szu_core::platform_tls::initialize_platform_verifier()?;
    let client = szu_core::netproxy::client_builder()
        .timeout(timeout)
        .connect_timeout(connect_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化 AI HTTP 客户端")?;
    Ok(ReqwestClient::from(client))
}

pub fn chat_transport() -> Result<ReqwestClient, String> {
    transport(Duration::from_secs(180), Duration::from_secs(15))
}

pub fn detection_transport() -> Result<ReqwestClient, String> {
    transport(Duration::from_secs(10), Duration::from_secs(5))
}

pub fn build(
    config: &Config,
    provider_id: &str,
    model: &str,
) -> Result<DynModel<Completion>, String> {
    let provider = config
        .provider(provider_id)
        .ok_or_else(|| "默认 AI 提供商不存在，请重新配置".to_string())?;
    build_provider(provider, model)
}

pub fn build_provider(
    provider: &ProviderConfig,
    model: &str,
) -> Result<DynModel<Completion>, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("请选择或填写默认模型".into());
    }
    if provider.vendor.needs_api_key() && provider.api_key.trim().is_empty() {
        return Err("默认 AI 提供商缺少 API Key".into());
    }
    let transport = chat_transport()?;
    let key = provider.api_key.as_str();
    let base = provider.base_url.as_str();
    Ok(match provider.vendor.protocol() {
        Protocol::OpenAiCompatible => OpenAIConfig::new(key)
            .with_base_url(base)
            .connect(transport)
            .chat(model)
            .into(),
        Protocol::Anthropic => AnthropicConfig::new(key)
            .with_base_url(base)
            .connect(transport)
            .completion(model)
            .into(),
        Protocol::Gemini => GeminiConfig::new(key)
            .with_base_url(base)
            .connect(transport)
            .completion(model)
            .into(),
        Protocol::Ollama => OllamaConfig::new()
            .with_base_url(base)
            .with_api_key(key)
            .connect(transport)
            .native_completion(model)
            .into(),
        Protocol::DifyChat => {
            return Err("Dify 公共聊天需要通过异步入口建立连接".into());
        }
    })
}

/// 建立模型。Dify 公共聊天需要先异步换取 passport，因此单独提供异步入口；
/// 其它协议直接复用同步构建。
pub async fn build_provider_async(
    provider: &ProviderConfig,
    model: &str,
) -> Result<DynModel<Completion>, String> {
    if provider.vendor != Vendor::Dify {
        return build_provider(provider, model);
    }
    let model = model.trim();
    if model.is_empty() {
        return Err("请选择或填写默认模型".into());
    }
    Ok(crate::dify::connect(provider, model).await?.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(vendor: Vendor, base_url: &str, key: &str) -> ProviderConfig {
        ProviderConfig {
            id: "p".into(),
            vendor,
            name: "p".into(),
            api_key: key.into(),
            base_url: base_url.into(),
            builtin: false,
            app_code: None,
            models: vec![],
        }
    }

    #[test]
    fn ollama_does_not_require_a_key() {
        let provider = provider(Vendor::Ollama, "http://127.0.0.1:11434", "");
        assert!(build_provider(&provider, "test-model").is_ok());
    }

    #[test]
    fn remote_protocols_require_a_key_and_model() {
        let openai = provider(Vendor::OpenAi, "https://example.com/v1", "");
        assert!(build_provider(&openai, "gpt").is_err());
        let openai = provider(Vendor::OpenAi, "https://example.com/v1", "key");
        assert!(build_provider(&openai, " ").is_err());
        assert!(build_provider(&openai, "gpt").is_ok());
        assert!(build_provider(
            &provider(Vendor::Anthropic, "https://api.anthropic.com", "key"),
            "claude"
        )
        .is_ok());
        assert!(build_provider(
            &provider(
                Vendor::Gemini,
                "https://generativelanguage.googleapis.com",
                "key"
            ),
            "gemini"
        )
        .is_ok());
    }
}
