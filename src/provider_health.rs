use crate::agent::models::{
    model_specs, provider_specs, ModelCapability, GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID,
};
use crate::agent::types::ToolResult;
use crate::assets::{
    asset_provider_env_var, audio_provider_name, image_provider_specs, video_provider_name,
    GEMINI_IMAGE_PROVIDER_ID, OPENAI_AUDIO_PROVIDER_ID, OPENAI_IMAGE_PROVIDER_ID,
    OPENAI_VIDEO_PROVIDER_ID,
};
use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderHealthReport {
    pub chat_providers: Vec<ProviderHealth>,
    pub asset_providers: Vec<AssetProviderHealth>,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub id: String,
    pub name: String,
    pub implemented: bool,
    pub key_present: bool,
    pub selected_model: String,
    pub model_known: bool,
    pub capabilities: Vec<String>,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetProviderHealth {
    pub id: String,
    pub name: String,
    pub key_present: bool,
    pub selected_model: String,
    pub env_var: String,
    pub issues: Vec<String>,
}

pub fn provider_health_report(config: &AppConfig) -> ProviderHealthReport {
    let chat_providers = provider_specs()
        .iter()
        .map(|provider| {
            let key_present = !config.api_key_for_provider(provider.id).trim().is_empty();
            let selected_model = config.model_for_provider(provider.id);
            let model = model_specs()
                .iter()
                .find(|model| model.provider_id == provider.id && model.id == selected_model);
            let mut issues = Vec::new();
            if !provider.implemented {
                issues.push("провайдер не реализован".to_string());
            }
            if !key_present {
                issues.push("API-ключ отсутствует".to_string());
            }
            if model.is_none() {
                issues.push("выбранной модели нет в реестре".to_string());
            }
            ProviderHealth {
                id: provider.id.to_string(),
                name: provider.name.to_string(),
                implemented: provider.implemented,
                key_present,
                selected_model,
                model_known: model.is_some(),
                capabilities: model
                    .map(|model| model.capabilities.iter().map(capability_name).collect())
                    .unwrap_or_default(),
                issues,
            }
        })
        .collect::<Vec<_>>();

    let mut asset_providers = image_provider_specs()
        .iter()
        .map(|provider| asset_health(config, provider.id, provider.name, provider.env_var))
        .collect::<Vec<_>>();
    asset_providers.push(asset_health(
        config,
        OPENAI_AUDIO_PROVIDER_ID,
        audio_provider_name(OPENAI_AUDIO_PROVIDER_ID),
        asset_provider_env_var(OPENAI_AUDIO_PROVIDER_ID),
    ));
    asset_providers.push(asset_health(
        config,
        OPENAI_VIDEO_PROVIDER_ID,
        video_provider_name(OPENAI_VIDEO_PROVIDER_ID),
        asset_provider_env_var(OPENAI_VIDEO_PROVIDER_ID),
    ));

    let mut issues = Vec::new();
    if !chat_providers
        .iter()
        .any(|provider| provider.implemented && provider.key_present && provider.model_known)
    {
        issues.push("нет полностью настроенного чат-провайдера".to_string());
    }
    if !asset_providers.iter().any(|provider| provider.key_present) {
        issues.push("нет настроенного ключа asset-провайдера".to_string());
    }

    ProviderHealthReport {
        chat_providers,
        asset_providers,
        issues,
    }
}

pub fn provider_health_snapshot(config: &AppConfig) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&json!(provider_health_report(config)))
            .unwrap_or_else(|_| "provider health".to_string()),
    )
}

fn asset_health(
    config: &AppConfig,
    provider_id: &str,
    name: &str,
    env_var: &str,
) -> AssetProviderHealth {
    let key_present = asset_key_present(config, provider_id);
    let selected_model = config.model_for_provider(provider_id);
    let mut issues = Vec::new();
    if !key_present {
        issues.push("API-ключ отсутствует".to_string());
    }
    AssetProviderHealth {
        id: provider_id.to_string(),
        name: name.to_string(),
        key_present,
        selected_model,
        env_var: env_var.to_string(),
        issues,
    }
}

fn asset_key_present(config: &AppConfig, provider_id: &str) -> bool {
    if !config.api_key_for_provider(provider_id).trim().is_empty() {
        return true;
    }
    match provider_id {
        OPENAI_IMAGE_PROVIDER_ID | OPENAI_AUDIO_PROVIDER_ID | OPENAI_VIDEO_PROVIDER_ID => !config
            .api_key_for_provider(OPENAI_PROVIDER_ID)
            .trim()
            .is_empty(),
        GEMINI_IMAGE_PROVIDER_ID => !config
            .api_key_for_provider(GEMINI_PROVIDER_ID)
            .trim()
            .is_empty(),
        _ => false,
    }
}

fn capability_name(capability: &ModelCapability) -> String {
    match capability {
        ModelCapability::Code => "code",
        ModelCapability::Reasoning => "reasoning",
        ModelCapability::Tools => "tools",
        ModelCapability::Vision => "vision",
        ModelCapability::Image => "image",
        ModelCapability::Audio => "audio",
        ModelCapability::Video => "video",
        ModelCapability::Realtime => "realtime",
        ModelCapability::Embeddings => "embeddings",
    }
    .to_string()
}
