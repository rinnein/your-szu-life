use super::super::quiz::model::Mode;
use serde::{Deserialize, Serialize};
use szu_ai::{Protocol, ProviderConfig, Vendor};
use szu_core::settings_sync_items::UOOC_AI;
use szu_core::storage::CredentialStore;

pub const DEFAULT_BATCH_SIZE: usize = 5;
const MAX_BATCH_SIZE: usize = 20;
const MAX_REASONING: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub batch_size: usize,
    pub reasoning_effort: String,
    pub include_reasoning: bool,
    pub web_search: bool,
    pub default_mode: Mode,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            reasoning_effort: "default".into(),
            include_reasoning: true,
            web_search: true,
            default_mode: Mode::Supervised,
        }
    }
}

impl Prefs {
    /// 旧 `fixed.uooc.ai` 里还留着 main/jev/defaultMode 等字段；serde 会忽略
    /// 未知字段，因此旧配置可以直接反序列化成这里的运行偏好。
    pub fn load(store: &dyn CredentialStore) -> Self {
        store
            .load_uooc_ai()
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
            .normalized()
    }

    pub fn save(&self, store: &dyn CredentialStore) -> Result<(), String> {
        let normalized = self.clone().normalized();
        let raw = serde_json::to_string(&normalized).map_err(|_| "UOOC AI 设置无法序列化")?;
        store.save_uooc_ai(&raw)?;
        szu_core::settings_sync_signal::mark_dirty(UOOC_AI);
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.batch_size = self.batch_size.clamp(1, MAX_BATCH_SIZE);
        self.reasoning_effort = normalize_reasoning(&self.reasoning_effort);
        self
    }

    pub fn update(&mut self, input: Input) -> Result<(), String> {
        if let Some(batch_size) = input.batch_size {
            if !(1..=MAX_BATCH_SIZE).contains(&batch_size) {
                return Err("题目批次大小必须在 1 到 20 之间".into());
            }
            self.batch_size = batch_size;
        }
        if let Some(effort) = input.reasoning_effort {
            let effort = effort.trim();
            if !matches!(effort, "default" | "low" | "medium" | "high") {
                return Err("AI 推理强度取值无效".into());
            }
            self.reasoning_effort = effort.to_string();
        }
        if let Some(include_reasoning) = input.include_reasoning {
            self.include_reasoning = include_reasoning;
        }
        if let Some(web_search) = input.web_search {
            self.web_search = web_search;
        }
        if let Some(default_mode) = input.default_mode {
            self.default_mode = default_mode;
        }
        Ok(())
    }

    pub fn view(&self, selected: Option<(ProviderConfig, String)>) -> View {
        View {
            provider: selected.map(|(provider, model)| {
                let model_name = provider
                    .models
                    .iter()
                    .find(|item| item.id == model)
                    .map(|item| item.name.clone())
                    .unwrap_or_else(|| model.clone());
                ProviderSummary {
                    id: provider.id,
                    name: provider.name,
                    vendor: provider.vendor,
                    protocol: provider.vendor.protocol(),
                    base_url: provider.base_url,
                    model,
                    model_name,
                    has_key: !provider.api_key.is_empty(),
                    builtin: provider.builtin,
                }
            }),
            batch_size: self.batch_size,
            reasoning_effort: self.reasoning_effort.clone(),
            include_reasoning: self.include_reasoning,
            web_search: self.web_search,
            default_mode: self.default_mode,
        }
    }
}

fn normalize_reasoning(raw: &str) -> String {
    let raw = raw.trim();
    if matches!(raw, "default" | "low" | "medium" | "high") {
        raw.to_string()
    } else if raw.is_empty() || raw.chars().count() > MAX_REASONING {
        "default".into()
    } else {
        // 兼容旧配置里的供应商自定义值；前端只提供四项，保留原值避免丢信息。
        raw.to_string()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub vendor: Vendor,
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    /// 模型在 VS Code schema 里的显示名。
    pub model_name: String,
    pub has_key: bool,
    pub builtin: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub provider: Option<ProviderSummary>,
    pub batch_size: usize,
    pub reasoning_effort: String,
    pub include_reasoning: bool,
    pub web_search: bool,
    pub default_mode: Mode,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    #[serde(default)]
    pub batch_size: Option<usize>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub include_reasoning: Option<bool>,
    #[serde(default)]
    pub web_search: Option<bool>,
    #[serde(default)]
    pub default_mode: Option<Mode>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use szu_ai::ModelConfig;
    use szu_core::storage::testing::MemoryCredentialStore;

    #[test]
    fn legacy_config_migrates_runtime_fields_and_ignores_main_and_jev() {
        let store = Arc::new(MemoryCredentialStore::default());
        store
            .save_uooc_ai(
                &serde_json::json!({
                    "main": {"baseUrl": "https://example.com/v1", "model": "m", "key": "k"},
                    "jev": {"baseUrl": "https://api.typesafe.ai/v1", "model": "j", "key": "j"},
                    "jevEnabled": true,
                    "confidence": 0.8,
                    "defaultMode": "approval",
                    "batchSize": 9,
                    "reasoningEffort": "high",
                    "includeReasoning": false,
                    "webSearch": false
                })
                .to_string(),
            )
            .unwrap();
        let prefs = Prefs::load(store.as_ref());
        assert_eq!(prefs.batch_size, 9);
        assert_eq!(prefs.reasoning_effort, "high");
        assert!(!prefs.include_reasoning);
        assert!(!prefs.web_search);
        assert_eq!(prefs.default_mode, Mode::Approval);
    }

    #[test]
    fn update_rejects_out_of_range_batch_and_unknown_effort() {
        let mut prefs = Prefs::default();
        assert!(prefs
            .update(Input {
                batch_size: Some(0),
                ..Default::default()
            })
            .is_err());
        assert!(prefs
            .update(Input {
                reasoning_effort: Some("ultra".into()),
                ..Default::default()
            })
            .is_err());
        prefs
            .update(Input {
                batch_size: Some(12),
                reasoning_effort: Some("medium".into()),
                include_reasoning: Some(false),
                web_search: Some(false),
                default_mode: Some(Mode::Yolo),
            })
            .unwrap();
        assert_eq!(prefs.batch_size, 12);
        assert_eq!(prefs.reasoning_effort, "medium");
        assert!(!prefs.include_reasoning);
        assert!(!prefs.web_search);
        assert_eq!(prefs.default_mode, Mode::Yolo);
    }

    #[test]
    fn view_exposes_provider_summary_without_key() {
        let prefs = Prefs::default();
        let view = prefs.view(Some((
            ProviderConfig {
                id: "p".into(),
                vendor: Vendor::OpenAi,
                name: "provider".into(),
                base_url: "https://example.com/v1".into(),
                api_key: "secret-key".into(),
                builtin: false,
                app_code: None,
                models: vec![ModelConfig {
                    id: "m".into(),
                    name: "M 模型".into(),
                    ..ModelConfig::default()
                }],
            },
            "m".into(),
        )));
        let provider = view.provider.as_ref().unwrap();
        assert_eq!(provider.model, "m");
        assert_eq!(provider.model_name, "M 模型");
        assert_eq!(provider.vendor, Vendor::OpenAi);
        assert!(!provider.builtin);
        assert!(provider.has_key);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("secret-key"));
    }
}
