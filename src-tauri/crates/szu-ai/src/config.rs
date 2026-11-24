use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_PROVIDERS: usize = 32;
pub const MAX_MODELS: usize = 2000;
const MAX_NAME: usize = 80;
const MAX_KEY: usize = 8192;
pub(crate) const MAX_MODEL: usize = 256;

/// 模型提供商的线协议。对用户可见的是 [`Vendor`]；协议由 vendor 推导，只影响
/// rig 如何发送请求。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    #[default]
    OpenAiCompatible,
    Anthropic,
    Gemini,
    Ollama,
    /// Dify 公共聊天（WebApp API）：地址形如
    /// `https://host/chatbot/<分享码>`，没有 API Key，也不能选择模型。
    DifyChat,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI 兼容",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Google Gemini",
            Self::Ollama => "Ollama",
            Self::DifyChat => "Dify 公共聊天",
        }
    }

    pub fn needs_api_key(self) -> bool {
        !matches!(self, Self::Ollama | Self::DifyChat)
    }
}

/// 模型供应商。取值与 VS Code `chatLanguageModels.json` 的 `vendor` 对齐；
/// `dify` 是应用内置的扩展 vendor。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    #[default]
    CustomEndpoint,
    OpenAi,
    Azure,
    Anthropic,
    Gemini,
    Deepseek,
    OpenRouter,
    Ollama,
    Dify,
}

impl Vendor {
    pub fn label(self) -> &'static str {
        match self {
            Self::CustomEndpoint => "自定义 OpenAI 兼容",
            Self::OpenAi => "OpenAI",
            Self::Azure => "Azure OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Google Gemini",
            Self::Deepseek => "DeepSeek",
            Self::OpenRouter => "OpenRouter",
            Self::Ollama => "Ollama",
            Self::Dify => "Dify 公共聊天",
        }
    }

    pub fn protocol(self) -> Protocol {
        match self {
            Self::Anthropic => Protocol::Anthropic,
            Self::Gemini => Protocol::Gemini,
            Self::Ollama => Protocol::Ollama,
            Self::Dify => Protocol::DifyChat,
            Self::CustomEndpoint
            | Self::OpenAi
            | Self::Azure
            | Self::Deepseek
            | Self::OpenRouter => Protocol::OpenAiCompatible,
        }
    }

    pub fn needs_api_key(self) -> bool {
        self.protocol().needs_api_key()
    }

    /// 从探测出的协议与根地址推断供应商。OpenAI 兼容端点按已知 host 归类，
    /// 其余落到 `customendpoint`。
    pub fn infer(protocol: Protocol, base_url: &str) -> Self {
        match protocol {
            Protocol::Anthropic => Self::Anthropic,
            Protocol::Gemini => Self::Gemini,
            Protocol::Ollama => Self::Ollama,
            Protocol::DifyChat => Self::Dify,
            Protocol::OpenAiCompatible => {
                let host = reqwest::Url::parse(base_url.trim())
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
                    .unwrap_or_default();
                if host.contains("openai.azure.com") || host.contains("cognitiveservices.azure.com")
                {
                    Self::Azure
                } else if host == "api.openai.com" {
                    Self::OpenAi
                } else if host.contains("deepseek") {
                    Self::Deepseek
                } else if host.contains("openrouter") {
                    Self::OpenRouter
                } else {
                    Self::CustomEndpoint
                }
            }
        }
    }
}

/// 内置的 UOOC「智能小优」Dify 公共聊天。分享码本身就是公开信息，不需要
/// 任何密钥；它始终作为不可修改的内置 provider 存在。
pub(crate) const BUILTIN_DIFY_PROVIDER_ID: &str = "uooc-dify-chatbot";
pub(crate) const BUILTIN_DIFY_BASE_URL: &str =
    "https://dify.uoocuniversity.com/chatbot/tlbXtTjn4X8tv7Ty";
/// 锁定的 Dify 公共聊天分享码。
pub(crate) const BUILTIN_DIFY_APP_CODE: &str = "tlbXtTjn4X8tv7Ty";
/// Dify 公共聊天没有模型列表，用固定的伪模型名占位。
pub(crate) const DIFY_MODEL_ID: &str = "chatbot";
/// Dify 公共聊天的固定能力：只接收文本。
const DIFY_MAX_INPUT_TOKENS: u64 = 128_000;
const DIFY_MAX_OUTPUT_TOKENS: u64 = 8_192;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SelectedModel {
    pub provider_id: String,
    pub model: String,
}

/// 一个模型及其能力，字段与 VS Code `chatLanguageModels.json` 的 model
/// 条目一致。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub tool_calling: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// VS Code schema 允许用 contextWindow 代替 maxInputTokens；本应用正常
    /// 写入 maxInputTokens，保留该字段用于导入/导出兼容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

/// 一个供应商分组。`vendor`、`name`、`apiKey`、`models` 与 VS Code
/// `chatLanguageModels.json` 对齐；`id`、`baseUrl`、`builtin`、`appCode` 是
/// 应用自己的扩展字段。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default)]
    pub id: String,
    pub vendor: Vendor,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// 应用扩展：请求根地址。VS Code 的 model.url 会由它和 vendor 推导。
    #[serde(default)]
    pub base_url: String,
    /// 应用扩展：内置 provider 不允许修改或删除。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
    /// 应用扩展：Dify 公共聊天的锁定分享码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_code: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    pub default: Option<SelectedModel>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub vendor: Vendor,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub models: Vec<ModelConfig>,
    pub has_key: bool,
    pub builtin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub providers: Vec<ProviderView>,
    pub default: Option<SelectedModel>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionView {
    pub protocol: Protocol,
    pub vendor: Vendor,
    pub base_url: String,
    pub models: Vec<ModelConfig>,
    /// 模型列表或凭据校验成功时为 true。
    pub verified: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertProviderInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    pub vendor: Vendor,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_key: bool,
    #[serde(default)]
    pub models: Option<Vec<ModelConfig>>,
    /// 保存后若当前没有默认模型，就用它作为默认模型。
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDefaultInput {
    pub provider_id: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewModelInput {
    pub vendor: Vendor,
    pub base_url: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectInput {
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    /// 编辑已有 provider 时复用已保存的 Key。
    #[serde(default)]
    pub provider_id: Option<String>,
    /// 指定协议时只探测该协议；缺省时按主机提示依次探测。
    #[serde(default)]
    pub protocol: Option<Protocol>,
}

/// Dify 公共聊天的固定模型配置：文本能力、锁定的分享码与地址。
pub(crate) fn dify_model(app_code: &str, url: &str) -> ModelConfig {
    ModelConfig {
        id: DIFY_MODEL_ID.to_string(),
        name: format!("dify-chatbot-{app_code}"),
        url: url.to_string(),
        tool_calling: false,
        vision: false,
        max_input_tokens: Some(DIFY_MAX_INPUT_TOKENS),
        max_output_tokens: Some(DIFY_MAX_OUTPUT_TOKENS),
        context_window: None,
    }
}

/// 内置 Dify 公共聊天的模型。
pub(crate) fn builtin_dify_model() -> ModelConfig {
    dify_model(BUILTIN_DIFY_APP_CODE, BUILTIN_DIFY_BASE_URL)
}

fn builtin_dify_provider() -> ProviderConfig {
    ProviderConfig {
        id: BUILTIN_DIFY_PROVIDER_ID.to_string(),
        vendor: Vendor::Dify,
        name: "UOOC 智能小优".to_string(),
        api_key: String::new(),
        base_url: BUILTIN_DIFY_BASE_URL.to_string(),
        builtin: true,
        app_code: Some(BUILTIN_DIFY_APP_CODE.to_string()),
        models: vec![builtin_dify_model()],
    }
}

impl Config {
    /// 首次启动时的内置配置：学校的 Dify 公共聊天作为默认提供商。
    pub(crate) fn builtin() -> Self {
        Self {
            providers: vec![builtin_dify_provider()],
            default: Some(SelectedModel {
                provider_id: BUILTIN_DIFY_PROVIDER_ID.to_string(),
                model: DIFY_MODEL_ID.to_string(),
            }),
        }
    }

    /// 内置 provider 永远存在、内容固定；缺失时插入到最前面。
    pub(crate) fn ensure_builtin(&mut self) {
        let canonical = builtin_dify_provider();
        match self
            .providers
            .iter_mut()
            .find(|provider| provider.id == canonical.id)
        {
            Some(slot) => *slot = canonical,
            None => {
                // Dify 公共聊天只允许内置那一份，避免出现第二个分享码。
                self.providers.retain(|provider| provider.vendor != Vendor::Dify);
                self.providers.insert(0, canonical);
            }
        }
        if self.default.is_none() {
            self.default = Some(SelectedModel {
                provider_id: BUILTIN_DIFY_PROVIDER_ID.to_string(),
                model: DIFY_MODEL_ID.to_string(),
            });
        }
    }

    pub fn view(&self) -> ConfigView {
        ConfigView {
            providers: self
                .providers
                .iter()
                .map(|provider| ProviderView {
                    id: provider.id.clone(),
                    vendor: provider.vendor,
                    name: provider.name.clone(),
                    protocol: provider.vendor.protocol(),
                    base_url: provider.base_url.clone(),
                    models: provider.models.clone(),
                    has_key: !provider.api_key.is_empty(),
                    builtin: provider.builtin,
                    app_code: provider.app_code.clone(),
                })
                .collect(),
            default: self.default.clone(),
        }
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    pub fn provider_mut(&mut self, id: &str) -> Option<&mut ProviderConfig> {
        self.providers.iter_mut().find(|provider| provider.id == id)
    }

    pub fn selected(&self) -> Option<(&ProviderConfig, String)> {
        let default = self.default.as_ref()?;
        let provider = self.provider(&default.provider_id)?;
        let model = provider
            .models
            .iter()
            .find(|model| model.id == default.model)?;
        Some((provider, model.id.clone()))
    }

    pub fn upsert(&mut self, input: UpsertProviderInput) -> Result<(), String> {
        if input.vendor == Vendor::Dify {
            return Err("Dify 公共聊天使用内置提供商，不能新增或修改".into());
        }
        if let Some(id) = input.id.as_deref() {
            if id == BUILTIN_DIFY_PROVIDER_ID
                || self
                    .provider(id)
                    .is_some_and(|provider| provider.builtin)
            {
                return Err("内置 Dify 提供商不可修改".into());
            }
        }
        let vendor = input.vendor;
        let base_url = normalize_base_url(&input.base_url, vendor.protocol())?;
        let name = {
            let name = input.name.trim();
            if name.is_empty() {
                default_provider_name(vendor, &base_url)
            } else {
                name.to_string()
            }
        };
        if name.chars().count() > MAX_NAME {
            return Err("提供商名称过长".into());
        }
        let new_key = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(normalize_key)
            .transpose()?;
        let requested_model = input
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty() && model.chars().count() <= MAX_MODEL)
            .map(str::to_string);

        let new_provider_id = match input.id.as_deref() {
            Some(id) => {
                let provider = self
                    .provider_mut(id)
                    .ok_or_else(|| "AI 提供商不存在".to_string())?;
                provider.name = name;
                provider.vendor = vendor;
                provider.base_url = base_url.clone();
                provider.app_code = None;
                provider.builtin = false;
                if input.clear_key {
                    provider.api_key.clear();
                } else if let Some(key) = new_key {
                    provider.api_key = key;
                }
                provider.models = sanitize_models(
                    input.models.unwrap_or_else(|| provider.models.clone()),
                    vendor,
                    &base_url,
                );
                None
            }
            None => {
                if self.providers.len() >= MAX_PROVIDERS {
                    return Err(format!("最多只能添加 {MAX_PROVIDERS} 个提供商"));
                }
                let models =
                    sanitize_models(input.models.unwrap_or_default(), vendor, &base_url);
                let id = uuid::Uuid::new_v4().to_string();
                self.providers.push(ProviderConfig {
                    id: id.clone(),
                    vendor,
                    name,
                    api_key: new_key.unwrap_or_default(),
                    base_url,
                    builtin: false,
                    app_code: None,
                    models,
                });
                Some(id)
            }
        };

        if let Some(model) = requested_model {
            let provider_id = input.id.clone().or(new_provider_id);
            if let Some(provider_id) = provider_id {
                if let Some(provider) = self.provider_mut(&provider_id) {
                    ensure_model(provider, &model);
                }
                // 新增提供商时若当前默认还是内置 Dify，就切到新提供商；编辑已有
                // 提供商只补空缺的默认，避免意外抢走用户的选择。
                let replaces_builtin = input.id.is_none()
                    && self
                        .default
                        .as_ref()
                        .is_some_and(|default| default.provider_id == BUILTIN_DIFY_PROVIDER_ID);
                if self.default.is_none() || replaces_builtin {
                    self.default = Some(SelectedModel {
                        provider_id: provider_id.clone(),
                        model,
                    });
                }
            }
        }

        self.sanitize();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let provider = self
            .provider(id)
            .ok_or_else(|| "AI 提供商不存在".to_string())?;
        if provider.builtin || id == BUILTIN_DIFY_PROVIDER_ID {
            return Err("内置 Dify 提供商不可删除".into());
        }
        self.providers.retain(|provider| provider.id != id);
        if self
            .default
            .as_ref()
            .is_some_and(|default| default.provider_id == id)
        {
            self.default = None;
        }
        Ok(())
    }

    pub fn set_default(&mut self, provider_id: &str, model: &str) -> Result<(), String> {
        let model = model.trim();
        if model.is_empty() {
            return Err("请选择或填写默认模型".into());
        }
        if model.chars().count() > MAX_MODEL {
            return Err("模型名称过长".into());
        }
        let provider = self
            .provider_mut(provider_id)
            .ok_or_else(|| "AI 提供商不存在".to_string())?;
        if provider.builtin {
            if !provider.models.iter().any(|item| item.id == model) {
                return Err("内置 Dify 提供商的模型不可修改".into());
            }
        } else {
            ensure_model(provider, model);
        }
        self.default = Some(SelectedModel {
            provider_id: provider_id.to_string(),
            model: model.to_string(),
        });
        Ok(())
    }

    /// 加载与保存前的自愈：修正空 id/名称、规范化地址与模型能力，清掉失效
    /// 默认项。内置 provider 会被强制恢复成固定内容。
    pub fn sanitize(&mut self) {
        let mut seen = HashSet::new();
        self.providers.retain_mut(|provider| {
            if provider.id.trim().is_empty() {
                provider.id = uuid::Uuid::new_v4().to_string();
            }
            if !seen.insert(provider.id.clone()) {
                return false;
            }
            if provider.id == BUILTIN_DIFY_PROVIDER_ID {
                *provider = builtin_dify_provider();
                return true;
            }
            // 只允许内置那一份 Dify 公共聊天。
            if provider.vendor == Vendor::Dify {
                return false;
            }
            provider.builtin = false;
            provider.app_code = None;
            if provider.name.trim().is_empty() {
                provider.name = default_provider_name(provider.vendor, &provider.base_url);
            }
            if provider.name.chars().count() > MAX_NAME {
                provider.name = provider.name.chars().take(MAX_NAME).collect();
            }
            let protocol = provider.vendor.protocol();
            let base_url = if provider.base_url.trim().is_empty() {
                provider
                    .models
                    .iter()
                    .find_map(|model| normalize_base_url(&model.url, protocol).ok())
            } else {
                normalize_base_url(&provider.base_url, protocol).ok()
            };
            let Some(base_url) = base_url else {
                return false;
            };
            provider.base_url = base_url;
            if normalize_key(&provider.api_key).is_err() {
                provider.api_key.clear();
            }
            provider.models = sanitize_models(
                std::mem::take(&mut provider.models),
                provider.vendor,
                &provider.base_url,
            );
            true
        });
        if self.providers.len() > MAX_PROVIDERS {
            self.providers.truncate(MAX_PROVIDERS);
        }
        if let Some(default) = self.default.clone() {
            match self.provider_mut(&default.provider_id) {
                Some(provider) if provider.builtin => {
                    if provider.models.iter().any(|model| model.id == default.model) {
                        // 内置模型有效。
                    } else if let Some(model) = provider.models.first() {
                        self.default = Some(SelectedModel {
                            provider_id: default.provider_id,
                            model: model.id.clone(),
                        });
                    } else {
                        self.default = None;
                    }
                }
                Some(provider) => {
                    if default.model.trim().is_empty()
                        || default.model.chars().count() > MAX_MODEL
                    {
                        self.default = None;
                    } else {
                        ensure_model(provider, &default.model);
                    }
                }
                None => self.default = None,
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.providers.len() > MAX_PROVIDERS {
            return Err(format!("最多只能添加 {MAX_PROVIDERS} 个提供商"));
        }
        let mut ids = HashSet::new();
        for provider in &self.providers {
            if provider.id.trim().is_empty() || !ids.insert(provider.id.clone()) {
                return Err("AI 提供商 ID 无效或重复".into());
            }
            if provider.name.trim().is_empty() || provider.name.chars().count() > MAX_NAME {
                return Err("AI 提供商名称无效".into());
            }
            normalize_base_url(&provider.base_url, provider.vendor.protocol())?;
            normalize_key(&provider.api_key)?;
            if provider.vendor == Vendor::Dify
                && (provider.id != BUILTIN_DIFY_PROVIDER_ID || !provider.builtin)
            {
                return Err("Dify 公共聊天使用内置提供商".into());
            }
            if provider.models.len() > MAX_MODELS {
                return Err("AI 模型列表无效".into());
            }
            let mut model_ids = HashSet::new();
            for model in &provider.models {
                if model.id.trim().is_empty()
                    || model.id.chars().count() > MAX_MODEL
                    || !model_ids.insert(model.id.clone())
                {
                    return Err("AI 模型 ID 无效或重复".into());
                }
                if model.name.chars().count() > MAX_NAME {
                    return Err("AI 模型名称过长".into());
                }
                if !model.url.is_empty() {
                    let url = reqwest::Url::parse(&model.url)
                        .map_err(|_| "AI 模型地址无效".to_string())?;
                    if !matches!(url.scheme(), "http" | "https") {
                        return Err("AI 模型地址需使用 HTTP 或 HTTPS".into());
                    }
                }
                if model.max_input_tokens == Some(0)
                    || model.max_output_tokens == Some(0)
                    || model.context_window == Some(0)
                {
                    return Err("AI 模型 token 限制无效".into());
                }
            }
        }
        if let Some(default) = &self.default {
            let provider = self
                .provider(&default.provider_id)
                .ok_or_else(|| "默认模型指向不存在的提供商".to_string())?;
            if default.model.trim().is_empty() || default.model.chars().count() > MAX_MODEL {
                return Err("默认模型名称无效".into());
            }
            if !provider.models.iter().any(|model| model.id == default.model) {
                return Err("默认模型不在提供商的模型列表中".into());
            }
        }
        Ok(())
    }
}

/// 确保 `model` 存在于 provider 的模型列表中；缺失时按 vendor + 模型名自动
/// 匹配能力并补齐。
fn ensure_model(provider: &mut ProviderConfig, model: &str) {
    if provider.models.iter().any(|item| item.id == model) {
        return;
    }
    provider.models.push(crate::capabilities::model_config(
        provider.vendor,
        model,
        &provider.base_url,
    ));
    provider.models.sort_by(|a, b| a.id.cmp(&b.id));
}

/// 规范化模型列表：去空、去重、补齐名称/地址/token 限制。
fn sanitize_models(models: Vec<ModelConfig>, vendor: Vendor, base_url: &str) -> Vec<ModelConfig> {
    let mut seen = HashSet::new();
    let mut out: Vec<ModelConfig> = models
        .into_iter()
        .map(|mut model| {
            model.id = model.id.trim().to_string();
            if model.name.trim().is_empty() {
                model.name = model.id.clone();
            } else {
                model.name = model.name.trim().to_string();
            }
            model.url = crate::capabilities::model_url(vendor, base_url, &model.id);
            let (_, _, input, output) = crate::capabilities::match_capabilities(vendor, &model.id);
            if model.max_output_tokens.is_none() {
                model.max_output_tokens = Some(output);
            }
            if model.max_input_tokens.is_none() {
                model.max_input_tokens = match (model.context_window, model.max_output_tokens) {
                    (Some(context), Some(output)) => Some(context.saturating_sub(output).max(1)),
                    _ => Some(input),
                };
            }
            if model.max_output_tokens == Some(0) {
                model.max_output_tokens = Some(output);
            }
            if model.max_input_tokens == Some(0) {
                model.max_input_tokens = Some(input);
            }
            model
        })
        .filter(|model| !model.id.is_empty() && seen.insert(model.id.clone()))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.truncate(MAX_MODELS);
    out
}

/// 将用户输入整理成 provider 根地址：去掉尾 `/` 与常见完整端点后缀；
/// 拒绝内嵌凭据、query、fragment 与非 HTTP(S) 协议。
pub fn normalize_base_url(raw: &str, protocol: Protocol) -> Result<String, String> {
    if protocol == Protocol::DifyChat {
        return Ok(crate::dify::Endpoint::parse(raw)?.chatbot_url());
    }
    let mut raw = raw.trim().trim_end_matches('/');
    let suffixes: &[&str] = match protocol {
        Protocol::OpenAiCompatible => &["/chat/completions", "/completions"],
        Protocol::Anthropic => &["/v1/messages", "/messages", "/v1"],
        Protocol::Gemini => &["/v1beta/models", "/v1/models", "/v1beta"],
        Protocol::Ollama => &["/api/chat", "/api/generate", "/api", "/v1"],
        Protocol::DifyChat => &[],
    };
    for suffix in suffixes {
        if let Some(stripped) = raw.strip_suffix(suffix) {
            raw = stripped.trim_end_matches('/');
            break;
        }
    }
    if raw.is_empty() {
        return Err("请填写 API 地址".into());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "API 地址无效")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("API 地址需使用 HTTP 或 HTTPS，不能包含凭据或查询参数".into());
    }
    Ok(url.as_str().trim_end_matches('/').into())
}

pub fn normalize_key(raw: &str) -> Result<String, String> {
    let key = raw.trim();
    if key.len() > MAX_KEY || key.contains(['\r', '\n']) {
        return Err("API Key 格式无效".into());
    }
    Ok(key.to_string())
}

fn default_provider_name(vendor: Vendor, base_url: &str) -> String {
    let host = reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default();
    let name = if host.is_empty() {
        vendor.label().to_string()
    } else {
        format!("{} · {host}", vendor.label())
    };
    name.chars().take(MAX_NAME).collect()
}

/// 探测时尝试的根地址：用户输入优先；没有版本段时再试一次最常见的入口。
pub fn base_candidates(protocol: Protocol, base_url: &str) -> Vec<String> {
    let base = base_url.trim().trim_end_matches('/').to_string();
    let mut candidates = vec![base.clone()];
    let has_version = base.ends_with("/v1") || base.ends_with("/v1beta") || base.ends_with("/api");
    if !has_version {
        match protocol {
            Protocol::OpenAiCompatible => candidates.push(format!("{base}/v1")),
            Protocol::Anthropic => candidates.push(format!("{base}/v1")),
            Protocol::Gemini | Protocol::Ollama | Protocol::DifyChat => {}
        }
    }
    candidates.dedup();
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(vendor: Vendor, base_url: &str) -> UpsertProviderInput {
        UpsertProviderInput {
            id: None,
            name: String::new(),
            vendor,
            base_url: base_url.into(),
            api_key: Some("key".into()),
            clear_key: false,
            models: None,
            model: None,
        }
    }

    #[test]
    fn normalize_base_url_strips_known_suffixes() {
        assert_eq!(
            normalize_base_url(
                "https://api.example.com/v1/chat/completions",
                Protocol::OpenAiCompatible
            )
            .unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.anthropic.com/v1/messages", Protocol::Anthropic)
                .unwrap(),
            "https://api.anthropic.com"
        );
        assert_eq!(
            normalize_base_url(
                "https://generativelanguage.googleapis.com/v1beta/models",
                Protocol::Gemini
            )
            .unwrap(),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            normalize_base_url("http://127.0.0.1:11434/api/chat", Protocol::Ollama).unwrap(),
            "http://127.0.0.1:11434"
        );
        assert!(normalize_base_url("file:///tmp/key", Protocol::OpenAiCompatible).is_err());
        assert!(
            normalize_base_url("https://user:pass@example.com", Protocol::OpenAiCompatible)
                .is_err()
        );
    }

    #[test]
    fn builtin_dify_config_is_valid_and_locked() {
        let config = Config::builtin();
        config.validate().unwrap();
        assert!(config.providers[0].builtin);
        assert_eq!(config.providers[0].vendor, Vendor::Dify);
        assert_eq!(config.providers[0].app_code.as_deref(), Some(BUILTIN_DIFY_APP_CODE));
        let model = &config.providers[0].models[0];
        assert_eq!(model.name, format!("dify-chatbot-{BUILTIN_DIFY_APP_CODE}"));
        assert!(!model.tool_calling);
        assert!(!model.vision);
        assert_eq!(
            normalize_base_url(BUILTIN_DIFY_BASE_URL, Protocol::DifyChat).unwrap(),
            BUILTIN_DIFY_BASE_URL
        );
    }

    #[test]
    fn builtin_provider_cannot_be_modified_or_removed() {
        let mut config = Config::builtin();
        assert!(config
            .upsert(UpsertProviderInput {
                id: Some(BUILTIN_DIFY_PROVIDER_ID.into()),
                ..input(Vendor::OpenAi, "https://api.openai.com/v1")
            })
            .is_err());
        assert!(config.remove(BUILTIN_DIFY_PROVIDER_ID).is_err());
        assert!(config.set_default(BUILTIN_DIFY_PROVIDER_ID, "other").is_err());
        assert!(config
            .set_default(BUILTIN_DIFY_PROVIDER_ID, DIFY_MODEL_ID)
            .is_ok());
    }

    #[test]
    fn detect_only_one_dify_provider() {
        let mut config = Config::builtin();
        config
            .providers
            .push(ProviderConfig {
                id: "custom-dify".into(),
                vendor: Vendor::Dify,
                name: "custom".into(),
                api_key: String::new(),
                base_url: "https://example.com/chatbot/abc".into(),
                builtin: false,
                app_code: Some("abc".into()),
                models: vec![],
            });
        config.sanitize();
        assert_eq!(
            config
                .providers
                .iter()
                .filter(|provider| provider.vendor == Vendor::Dify)
                .count(),
            1
        );
    }

    #[test]
    fn upsert_preserves_saved_key_and_masks_it_in_view() {
        let mut config = Config::default();
        config
            .upsert(input(Vendor::OpenAi, "https://example.com/v1"))
            .unwrap();
        let id = config.providers[0].id.clone();
        assert!(config.view().providers[0].has_key);

        let mut update = input(Vendor::OpenAi, "https://example.com/v1");
        update.id = Some(id.clone());
        update.api_key = None;
        config.upsert(update).unwrap();
        assert_eq!(config.providers[0].api_key, "key");

        let mut clear = input(Vendor::OpenAi, "https://example.com/v1");
        clear.id = Some(id);
        clear.api_key = None;
        clear.clear_key = true;
        config.upsert(clear).unwrap();
        assert!(config.providers[0].api_key.is_empty());
        assert!(!config.view().providers[0].has_key);
    }

    #[test]
    fn adding_a_provider_replaces_the_builtin_default() {
        let mut config = Config::builtin();
        config
            .upsert(UpsertProviderInput {
                model: Some("gpt-4.1".into()),
                ..input(Vendor::OpenAi, "https://api.openai.com/v1")
            })
            .unwrap();
        let added = config
            .providers
            .iter()
            .find(|provider| !provider.builtin)
            .unwrap();
        assert_eq!(
            config.default.as_ref().unwrap().provider_id,
            added.id,
            "新增提供商应接管内置默认"
        );
        assert_eq!(config.default.as_ref().unwrap().model, "gpt-4.1");
        assert_eq!(added.models[0].name, "gpt-4.1");
        assert!(added.models[0].tool_calling);
        assert!(added.models[0].vision);
    }

    #[test]
    fn editing_a_provider_does_not_steal_the_default() {
        let mut config = Config::builtin();
        config
            .upsert(UpsertProviderInput {
                model: Some("gpt-4.1".into()),
                ..input(Vendor::OpenAi, "https://api.openai.com/v1")
            })
            .unwrap();
        let added = config
            .providers
            .iter()
            .find(|provider| !provider.builtin)
            .unwrap()
            .clone();
        config.set_default(&added.id, "gpt-4.1").unwrap();
        // 第二个提供商只添加、不设默认。
        config
            .upsert(input(Vendor::Deepseek, "https://api.deepseek.com/v1"))
            .unwrap();
        let second = config
            .providers
            .iter()
            .find(|provider| provider.vendor == Vendor::Deepseek)
            .unwrap()
            .clone();
        config
            .upsert(UpsertProviderInput {
                id: Some(second.id.clone()),
                model: Some("deepseek-chat".into()),
                ..input(Vendor::Deepseek, "https://api.deepseek.com/v1")
            })
            .unwrap();
        assert_eq!(
            config.default.as_ref().unwrap().provider_id,
            added.id,
            "编辑已有提供商不应改变默认"
        );
        assert_eq!(config.default.as_ref().unwrap().model, "gpt-4.1");
        assert!(config.providers.iter().any(|provider| provider.id == second.id));
    }

    #[test]
    fn default_must_reference_an_existing_provider() {
        let mut config = Config::default();
        assert!(config.set_default("missing", "model").is_err());
        config
            .upsert(input(Vendor::Ollama, "http://127.0.0.1:11434"))
            .unwrap();
        let id = config.providers[0].id.clone();
        config.set_default(&id, "test-model").unwrap();
        assert_eq!(config.selected().unwrap().1, "test-model");
        config.remove(&id).unwrap();
        assert!(config.default.is_none());
    }

    #[test]
    fn sanitize_drops_invalid_providers_and_default() {
        let mut config = Config {
            providers: vec![
                ProviderConfig {
                    id: "bad".into(),
                    vendor: Vendor::OpenAi,
                    name: "bad".into(),
                    api_key: String::new(),
                    base_url: "not-a-url".into(),
                    builtin: false,
                    app_code: None,
                    models: vec![],
                },
                ProviderConfig {
                    id: "ok".into(),
                    vendor: Vendor::Ollama,
                    name: "ok".into(),
                    api_key: String::new(),
                    base_url: "http://127.0.0.1:11434/".into(),
                    builtin: false,
                    app_code: None,
                    models: vec![
                        ModelConfig {
                            id: " b ".into(),
                            ..ModelConfig::default()
                        },
                        ModelConfig {
                            id: "a".into(),
                            ..ModelConfig::default()
                        },
                        ModelConfig {
                            id: "a".into(),
                            ..ModelConfig::default()
                        },
                    ],
                },
            ],
            default: Some(SelectedModel {
                provider_id: "bad".into(),
                model: "x".into(),
            }),
        };
        config.sanitize();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].base_url, "http://127.0.0.1:11434");
        assert_eq!(
            config.providers[0]
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(config.default.is_none());
    }

    #[test]
    fn model_capabilities_match_vendor_and_name() {
        let openai = crate::capabilities::model_config(
            Vendor::OpenAi,
            "gpt-4.1",
            "https://api.openai.com/v1",
        );
        assert!(openai.tool_calling);
        assert!(openai.vision);
        assert_eq!(openai.max_input_tokens, Some(1_014_808));
        assert_eq!(openai.max_output_tokens, Some(32_768));
        assert_eq!(
            openai.url,
            "https://api.openai.com/v1/chat/completions"
        );

        let deepseek = crate::capabilities::model_config(
            Vendor::Deepseek,
            "deepseek-reasoner",
            "https://api.deepseek.com/v1",
        );
        assert!(!deepseek.tool_calling);
        assert!(!deepseek.vision);
    }
}
