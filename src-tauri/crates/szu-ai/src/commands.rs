use crate::config::{
    ConfigView, DetectInput, DetectionView, ModelConfig, PreviewModelInput, SetDefaultInput,
    UpsertProviderInput, Vendor, MAX_MODEL,
};
use crate::{capabilities, config, detect, store};
use szu_core::storage::PlatformCredentialStore;
use tauri::WebviewWindow;

fn local_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" && !window.label().starts_with("feature-") {
        return Err("请从应用内页面管理 AI 配置".into());
    }
    Ok(())
}

#[tauri::command]
pub fn get_ai_config(window: WebviewWindow) -> Result<ConfigView, String> {
    local_window(&window)?;
    Ok(store::load(&PlatformCredentialStore)?.view())
}

/// 丢弃现有 AI 配置（含所有 API Key），恢复内置的 Dify 公共聊天。用于配置
/// 损坏导致无法读取时的自愈。
#[tauri::command]
pub fn reset_ai_config(window: WebviewWindow) -> Result<ConfigView, String> {
    local_window(&window)?;
    Ok(store::reset(&PlatformCredentialStore)?.view())
}

#[tauri::command]
pub async fn detect_ai_provider(
    window: WebviewWindow,
    input: DetectInput,
) -> Result<DetectionView, String> {
    local_window(&window)?;
    let store = PlatformCredentialStore;
    let key = match input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        Some(key) => key.to_string(),
        None => match input.provider_id.as_deref() {
            Some(id) => {
                let config = store::load(&store)?;
                config
                    .provider(id)
                    .map(|provider| provider.api_key.clone())
                    .ok_or_else(|| "AI 提供商不存在".to_string())?
            }
            None => String::new(),
        },
    };
    detect::detect_with_key(&input.base_url, &key, input.protocol).await
}

#[tauri::command]
pub fn upsert_ai_provider(
    window: WebviewWindow,
    input: UpsertProviderInput,
) -> Result<ConfigView, String> {
    local_window(&window)?;
    let store = PlatformCredentialStore;
    let mut config = store::load(&store)?;
    config.upsert(input)?;
    let config = store::save(&store, &config)?;
    Ok(config.view())
}

#[tauri::command]
pub fn remove_ai_provider(
    window: WebviewWindow,
    provider_id: String,
) -> Result<ConfigView, String> {
    local_window(&window)?;
    let store = PlatformCredentialStore;
    let mut config = store::load(&store)?;
    config.remove(&provider_id)?;
    let config = store::save(&store, &config)?;
    Ok(config.view())
}

#[tauri::command]
pub async fn list_ai_models(
    window: WebviewWindow,
    provider_id: String,
) -> Result<Vec<String>, String> {
    local_window(&window)?;
    let store = PlatformCredentialStore;
    let config = store::load(&store)?;
    let provider = config
        .provider(&provider_id)
        .ok_or_else(|| "AI 提供商不存在".to_string())?;
    let view = detect::detect_with_key(
        &provider.base_url,
        &provider.api_key,
        Some(provider.vendor.protocol()),
    )
    .await?;
    Ok(view.models.into_iter().map(|model| model.id).collect())
}

/// 按供应商与模型名预览自动匹配的能力，供设置页在保存前显示 checkbox /
/// token 限制。纯本地计算，不发起网络请求。
#[tauri::command]
pub fn preview_ai_model(
    window: WebviewWindow,
    input: PreviewModelInput,
) -> Result<ModelConfig, String> {
    local_window(&window)?;
    if input.vendor == Vendor::Dify {
        return Err("Dify 公共聊天使用内置模型，能力不可修改".into());
    }
    let model = input.model.trim();
    if model.is_empty() || model.chars().count() > MAX_MODEL {
        return Err("模型名称无效".into());
    }
    let base_url = config::normalize_base_url(&input.base_url, input.vendor.protocol())?;
    Ok(capabilities::model_config(input.vendor, model, &base_url))
}

#[tauri::command]
pub fn set_ai_default(window: WebviewWindow, input: SetDefaultInput) -> Result<ConfigView, String> {
    local_window(&window)?;
    let store = PlatformCredentialStore;
    let mut config = store::load(&store)?;
    config.set_default(&input.provider_id, &input.model)?;
    let config = store::save(&store, &config)?;
    Ok(config.view())
}
