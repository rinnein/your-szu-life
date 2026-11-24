use crate::capabilities;
use crate::config::{
    normalize_base_url, Config, Protocol, ProviderConfig, SelectedModel, Vendor,
};
use serde::Deserialize;
use szu_core::settings_sync_items::{AI_CONFIG, AI_DEFAULT, UOOC_AI};
use szu_core::storage::CredentialStore;

/// 读取全局 AI 配置。`fixed.ai.config` 是 VS Code `chatLanguageModels.json`
/// 形式的 provider 数组，默认模型单独存放在 `fixed.ai.default`。
///
/// 新键不存在时，从已发布的旧 `fixed.uooc.ai` 的 `main` 迁移一次；更早的
/// 未发布中间格式没有用户，不再兼容。
pub fn load(store: &dyn CredentialStore) -> Result<Config, String> {
    let stored_default = load_default(store)?;
    if let Some(raw) = store.load_ai_config()? {
        let providers: Vec<ProviderConfig> =
            serde_json::from_str(&raw).map_err(|_| "AI 配置格式无效，请重新配置提供商")?;
        let mut config = Config {
            providers,
            default: stored_default,
        };
        config.sanitize();
        config.ensure_builtin();
        config.validate()?;
        return Ok(config);
    }

    let Some(legacy_raw) = store.load_uooc_ai()? else {
        let mut config = Config::builtin();
        if stored_default.is_some() {
            config.default = stored_default;
        }
        config.sanitize();
        config.ensure_builtin();
        return Ok(config);
    };

    let mut config = match migrate_legacy(&legacy_raw) {
        Some(config) => config,
        None => Config::builtin(),
    };
    if config.default.is_none() {
        config.default = stored_default;
    }
    scrub_legacy_prefs(store, &legacy_raw);
    config.sanitize();
    config.ensure_builtin();
    save(store, &config)
}

fn load_default(store: &dyn CredentialStore) -> Result<Option<SelectedModel>, String> {
    let Some(raw) = store.load_ai_default()? else {
        return Ok(None);
    };
    serde_json::from_str::<Option<SelectedModel>>(&raw)
        .map_err(|_| "AI 默认模型配置格式无效".to_string())
}

/// 旧键同时装着 main/jev 与 UOOC 运行偏好；迁移后只保留偏好字段，
/// 把旧 provider 的明文密钥从该条目里去掉。
fn scrub_legacy_prefs(store: &dyn CredentialStore, raw: &str) {
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(raw)
    else {
        return;
    };
    if !object.contains_key("main") && !object.contains_key("jev") {
        return;
    }
    let mut prefs = serde_json::Map::new();
    for key in [
        "batchSize",
        "reasoningEffort",
        "includeReasoning",
        "webSearch",
        "defaultMode",
    ] {
        if let Some(value) = object.get(key) {
            prefs.insert(key.to_string(), value.clone());
        }
    }
    match serde_json::to_string(&prefs) {
        Ok(raw) => {
            if let Err(error) = store.save_uooc_ai(&raw) {
                log::warn!("清理旧 UOOC AI 配置失败: {error}");
            } else {
                szu_core::settings_sync_signal::mark_dirty(UOOC_AI);
            }
        }
        Err(error) => log::warn!("清理旧 UOOC AI 配置失败: {error}"),
    }
}

/// 保存配置，并返回补全内置 provider / 默认模型后的实际落盘版本，供命令
/// 直接把同一个视图回给前端。
pub fn save(store: &dyn CredentialStore, config: &Config) -> Result<Config, String> {
    let mut config = config.clone();
    config.sanitize();
    config.ensure_builtin();
    config.validate()?;
    let providers = serde_json::to_string(&config.providers).map_err(|_| "AI 配置无法序列化")?;
    let default = serde_json::to_string(&config.default).map_err(|_| "AI 默认模型无法序列化")?;
    store.save_ai_config(&providers)?;
    store.save_ai_default(&default)?;
    szu_core::settings_sync_signal::mark_dirty(AI_CONFIG);
    szu_core::settings_sync_signal::mark_dirty(AI_DEFAULT);
    Ok(config)
}

/// 忽略现有内容，把 AI 配置重置为内置 Dify 默认值。用于 `fixed.ai.config`
/// 损坏导致无法读取时的自愈；会一并清掉旧 `fixed.uooc.ai` 里的 provider 密钥。
pub fn reset(store: &dyn CredentialStore) -> Result<Config, String> {
    let config = Config::builtin();
    config.validate()?;
    let providers = serde_json::to_string(&config.providers).map_err(|_| "AI 配置无法序列化")?;
    let default = serde_json::to_string(&config.default).map_err(|_| "AI 默认模型无法序列化")?;
    store.save_ai_config(&providers)?;
    store.save_ai_default(&default)?;
    if let Some(legacy) = store.load_uooc_ai()? {
        scrub_legacy_prefs(store, &legacy);
    }
    szu_core::settings_sync_signal::mark_dirty(AI_CONFIG);
    szu_core::settings_sync_signal::mark_dirty(AI_DEFAULT);
    Ok(config)
}

pub fn selected(store: &dyn CredentialStore) -> Result<Option<(ProviderConfig, String)>, String> {
    let config = load(store)?;
    Ok(config
        .selected()
        .map(|(provider, model)| (provider.clone(), model)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyConfig {
    #[serde(default)]
    main: LegacyProvider,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProvider {
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    key: String,
}

fn migrate_legacy(raw: &str) -> Option<Config> {
    let legacy: LegacyConfig = serde_json::from_str(raw).ok()?;
    let base_url = legacy.main.base_url.trim();
    if base_url.is_empty() {
        return None;
    }
    let vendor = Vendor::infer(Protocol::OpenAiCompatible, base_url);
    let base_url = normalize_base_url(base_url, vendor.protocol()).ok()?;
    let model = legacy.main.model.trim().to_string();
    let provider = ProviderConfig {
        id: uuid::Uuid::new_v4().to_string(),
        vendor,
        name: "迁移的 AI 配置".into(),
        api_key: legacy.main.key,
        base_url: base_url.clone(),
        builtin: false,
        app_code: None,
        models: if model.is_empty() {
            Vec::new()
        } else {
            vec![capabilities::model_config(vendor, &model, &base_url)]
        },
    };
    let default = (!model.is_empty()).then(|| SelectedModel {
        provider_id: provider.id.clone(),
        model,
    });
    Some(Config {
        providers: vec![provider],
        default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use szu_core::storage::testing::MemoryCredentialStore;

    #[test]
    fn migrates_legacy_main_and_drops_jev() {
        let store = Arc::new(MemoryCredentialStore::default());
        let legacy = serde_json::json!({
            "main": {
                "baseUrl": "https://example.com/v1/chat/completions",
                "model": "test-model",
                "key": "test-key"
            },
            "jev": {
                "baseUrl": "https://api.typesafe.ai/v1",
                "model": "jev-latest",
                "key": "jev-key"
            },
            "jevEnabled": true,
            "confidence": 0.8,
            "defaultMode": "supervised",
            "batchSize": 7,
            "reasoningEffort": "high",
            "includeReasoning": false,
            "webSearch": true
        })
        .to_string();
        store.save_uooc_ai(&legacy).unwrap();

        let config = load(store.as_ref()).unwrap();
        assert_eq!(config.providers.len(), 2);
        assert_eq!(config.providers[0].vendor, Vendor::Dify);
        let migrated = config
            .providers
            .iter()
            .find(|provider| provider.vendor != Vendor::Dify)
            .unwrap();
        assert_eq!(migrated.base_url, "https://example.com/v1");
        assert_eq!(migrated.api_key, "test-key");
        assert_eq!(migrated.models[0].id, "test-model");
        assert_eq!(migrated.models[0].name, "test-model");
        let default = config.default.unwrap();
        assert_eq!(default.provider_id, migrated.id);
        assert_eq!(default.model, "test-model");

        let raw = store.load_ai_config().unwrap().unwrap();
        assert!(raw.starts_with('['));
        assert!(!raw.contains("jev"));
        assert!(store.load_ai_default().unwrap().is_some());
        let scrubbed = store.load_uooc_ai().unwrap().unwrap();
        assert!(!scrubbed.contains("jev"));
        assert!(!scrubbed.contains("main"));
        assert!(!scrubbed.contains("test-key"));
        assert!(scrubbed.contains("batchSize"));
    }

    #[test]
    fn empty_store_falls_back_to_the_builtin_dify_provider() {
        let store = Arc::new(MemoryCredentialStore::default());
        let config = load(store.as_ref()).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].vendor, Vendor::Dify);
        assert_eq!(config.providers[0].id, crate::config::BUILTIN_DIFY_PROVIDER_ID);
        let default = config.default.unwrap();
        assert_eq!(
            default.provider_id,
            crate::config::BUILTIN_DIFY_PROVIDER_ID
        );
        assert_eq!(default.model, crate::config::DIFY_MODEL_ID);
        // 内置默认值不落盘：用户没有编辑前不产生同步写入。
        assert!(store.load_ai_config().unwrap().is_none());
        assert!(store.load_ai_default().unwrap().is_none());
    }

    #[test]
    fn new_config_takes_precedence_over_legacy() {
        let store = Arc::new(MemoryCredentialStore::default());
        let mut config = Config::default();
        config
            .upsert(crate::config::UpsertProviderInput {
                id: None,
                name: "Local".into(),
                vendor: Vendor::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                api_key: None,
                clear_key: false,
                models: Some(vec![capabilities::model_config(
                    Vendor::Ollama,
                    "qwen3",
                    "http://127.0.0.1:11434",
                )]),
                model: None,
            })
            .unwrap();
        save(store.as_ref(), &config).unwrap();
        store
            .save_uooc_ai(
                &serde_json::json!({"main": {"baseUrl": "https://old.example/v1"}}).to_string(),
            )
            .unwrap();

        let loaded = load(store.as_ref()).unwrap();
        assert!(loaded
            .providers
            .iter()
            .any(|provider| provider.base_url == "http://127.0.0.1:11434"));
        assert!(!loaded
            .providers
            .iter()
            .any(|provider| provider.base_url == "https://old.example/v1"));
        assert!(loaded.providers.iter().any(|provider| provider.builtin));
    }

    #[test]
    fn saved_config_is_vs_code_shaped() {
        let store = Arc::new(MemoryCredentialStore::default());
        let mut config = Config::default();
        config
            .upsert(crate::config::UpsertProviderInput {
                id: None,
                name: "OpenAI".into(),
                vendor: Vendor::OpenAi,
                base_url: "https://api.openai.com/v1".into(),
                api_key: Some("secret".into()),
                clear_key: false,
                models: None,
                model: Some("gpt-4.1".into()),
            })
            .unwrap();
        save(store.as_ref(), &config).unwrap();

        let raw = store.load_ai_config().unwrap().unwrap();
        assert!(raw.starts_with('['));
        let providers: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let providers = providers.as_array().unwrap();
        let openai = providers
            .iter()
            .find(|provider| provider["vendor"] == "openai")
            .unwrap();
        assert_eq!(openai["baseUrl"], "https://api.openai.com/v1");
        assert_eq!(
            openai["models"][0]["url"],
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(openai["models"][0]["toolCalling"], true);
        assert_eq!(openai["models"][0]["vision"], true);
        assert!(openai["models"][0]["maxInputTokens"].is_number());
        assert!(openai["models"][0]["maxOutputTokens"].is_number());
        assert_eq!(openai["models"][0]["name"], "gpt-4.1");

        let dify = providers
            .iter()
            .find(|provider| provider["vendor"] == "dify")
            .unwrap();
        assert_eq!(dify["builtin"], true);
        assert_eq!(dify["appCode"], crate::config::BUILTIN_DIFY_APP_CODE);
        assert_eq!(
            dify["models"][0]["name"],
            format!(
                "dify-chatbot-{}",
                crate::config::BUILTIN_DIFY_APP_CODE
            )
        );
        assert_eq!(dify["models"][0]["toolCalling"], false);
        assert_eq!(dify["models"][0]["vision"], false);

        let default = store.load_ai_default().unwrap().unwrap();
        let default: serde_json::Value = serde_json::from_str(&default).unwrap();
        assert_eq!(default["model"], "gpt-4.1");
        assert!(default["providerId"].is_string());
    }

    #[test]
    fn removing_the_default_provider_falls_back_to_builtin() {
        let store = Arc::new(MemoryCredentialStore::default());
        let mut config = Config::default();
        config
            .upsert(crate::config::UpsertProviderInput {
                id: None,
                name: "Local".into(),
                vendor: Vendor::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                api_key: None,
                clear_key: false,
                models: None,
                model: Some("qwen3".into()),
            })
            .unwrap();
        let mut config = save(store.as_ref(), &config).unwrap();
        let removed = config
            .providers
            .iter()
            .find(|provider| !provider.builtin)
            .unwrap()
            .id
            .clone();
        assert_eq!(config.default.as_ref().unwrap().provider_id, removed);

        config.remove(&removed).unwrap();
        let config = save(store.as_ref(), &config).unwrap();
        assert_eq!(
            config.default.as_ref().unwrap().provider_id,
            crate::config::BUILTIN_DIFY_PROVIDER_ID
        );
    }

    #[test]
    fn reset_replaces_a_corrupt_config_with_the_builtin() {
        let store = Arc::new(MemoryCredentialStore::default());
        store
            .save_ai_config(&"{not json".to_string())
            .unwrap();
        store.save_ai_default(&"broken".to_string()).unwrap();
        assert!(load(store.as_ref()).is_err());

        let config = reset(store.as_ref()).unwrap();
        assert_eq!(config.providers[0].vendor, Vendor::Dify);
        assert_eq!(
            config.default.as_ref().unwrap().provider_id,
            crate::config::BUILTIN_DIFY_PROVIDER_ID
        );
        let loaded = load(store.as_ref()).unwrap();
        assert_eq!(loaded.providers.len(), 1);
        assert!(loaded.providers[0].builtin);
    }

    #[test]
    fn saved_default_is_read_back() {
        let store = Arc::new(MemoryCredentialStore::default());
        let mut config = Config::default();
        config
            .upsert(crate::config::UpsertProviderInput {
                id: None,
                name: "Local".into(),
                vendor: Vendor::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                api_key: None,
                clear_key: false,
                models: None,
                model: Some("qwen3".into()),
            })
            .unwrap();
        save(store.as_ref(), &config).unwrap();
        let loaded = load(store.as_ref()).unwrap();
        let default = loaded.default.as_ref().unwrap();
        assert_eq!(default.model, "qwen3");
        let provider = loaded.provider(&default.provider_id).unwrap();
        assert_eq!(provider.models[0].id, "qwen3");
        assert_eq!(provider.models[0].max_input_tokens.is_some(), true);
    }
}
