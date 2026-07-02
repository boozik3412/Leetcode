use crate::agent::models::{GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID};
use crate::agent::types::{AppEvent, ToolResult};
use crate::assets::{
    default_image_model, image_provider_env_var, image_provider_name, normalize_image_provider,
    run_image_job, AssetJob, AssetStatus, ImageAssetRequest, GEMINI_IMAGE_PROVIDER_ID,
    OPENAI_IMAGE_PROVIDER_ID, REPLICATE_IMAGE_PROVIDER_ID, STABILITY_IMAGE_PROVIDER_ID,
};
use crate::config::AppConfig;
use crate::tools::policy::{request_approval, ApprovalMap};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
pub struct GenerateImageAssetArgs {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub aspect_ratio: Option<String>,
    pub image_size: Option<String>,
}

pub async fn generate_image_asset(
    workspace: &Workspace,
    args: GenerateImageAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let prompt = args.prompt.trim().to_string();
    if prompt.is_empty() {
        return ToolResult::error("generate_image_asset prompt is empty");
    }

    let provider = args
        .provider
        .as_deref()
        .map(normalize_image_provider)
        .unwrap_or_else(|| default_configured_image_provider(config));
    let model = args
        .model
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| image_model_from_config(config, &provider));
    let aspect_ratio = args.aspect_ratio.unwrap_or_else(|| "1:1".to_string());
    let image_size = args.image_size.unwrap_or_else(|| "1K".to_string());
    let api_key = image_api_key_from_config(config, &provider);

    if api_key.trim().is_empty() {
        return ToolResult::error(format!(
            "{} key is empty. Save it in the Assets panel or set {}.",
            image_provider_name(&provider),
            image_provider_env_var(&provider)
        ));
    }

    if !request_approval(
        events,
        approvals,
        format!(
            "Generate image asset with {} ({model})",
            image_provider_name(&provider)
        ),
        format!(
            "Provider: {}\nModel: {model}\nAspect ratio: {aspect_ratio}\nImage size: {image_size}\n\nPrompt:\n{prompt}",
            image_provider_name(&provider)
        ),
    ) {
        return ToolResult::error("generate_image_asset denied by user");
    }

    let request = ImageAssetRequest {
        provider,
        prompt,
        model,
        aspect_ratio,
        image_size,
    };
    let job = AssetJob::new_image(&request);
    let final_job = run_image_job(workspace.clone(), api_key, request, job).await;

    match final_job.status {
        AssetStatus::Done => ToolResult::ok(
            serde_json::to_string_pretty(&json!({
                "job_id": final_job.id,
                "provider": final_job.provider,
                "model": final_job.model,
                "output_files": final_job.output_files,
                "metadata": final_job.metadata
            }))
            .unwrap_or_else(|_| "image asset generated".to_string()),
        ),
        AssetStatus::Failed => ToolResult::error(format!(
            "generate_image_asset failed: {}",
            final_job
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        )),
        AssetStatus::Pending | AssetStatus::Running => ToolResult::error(
            "generate_image_asset ended before the image job reached a final state",
        ),
    }
}

fn default_configured_image_provider(config: &AppConfig) -> String {
    for provider_id in [
        OPENAI_IMAGE_PROVIDER_ID,
        GEMINI_IMAGE_PROVIDER_ID,
        STABILITY_IMAGE_PROVIDER_ID,
        REPLICATE_IMAGE_PROVIDER_ID,
    ] {
        if !image_api_key_from_config(config, provider_id)
            .trim()
            .is_empty()
        {
            return provider_id.to_string();
        }
    }

    OPENAI_IMAGE_PROVIDER_ID.to_string()
}

fn image_api_key_from_config(config: &AppConfig, provider_id: &str) -> String {
    let direct_key = config.api_key_for_provider(provider_id);
    if !direct_key.trim().is_empty() {
        return direct_key;
    }

    match provider_id {
        OPENAI_IMAGE_PROVIDER_ID => config.api_key_for_provider(OPENAI_PROVIDER_ID),
        GEMINI_IMAGE_PROVIDER_ID => config.api_key_for_provider(GEMINI_PROVIDER_ID),
        _ => String::new(),
    }
}

fn image_model_from_config(config: &AppConfig, provider_id: &str) -> String {
    config
        .providers
        .get(provider_id)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_image_model(provider_id).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderSettings;
    use std::collections::BTreeMap;

    #[test]
    fn defaults_to_first_configured_image_provider() {
        let mut providers = BTreeMap::new();
        providers.insert(
            STABILITY_IMAGE_PROVIDER_ID.to_string(),
            ProviderSettings {
                api_key: "sk-stability".to_string(),
                model: "stable-image-core".to_string(),
            },
        );
        let config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: String::new(),
            model: "gpt-5.5".to_string(),
            providers,
            last_workspace: None,
            require_shell_approval: true,
            require_write_approval: true,
        };

        assert_eq!(
            default_configured_image_provider(&config),
            STABILITY_IMAGE_PROVIDER_ID
        );
    }

    #[test]
    fn openai_image_provider_reuses_chat_key() {
        let config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: "sk-openai".to_string(),
            model: "gpt-5.5".to_string(),
            providers: BTreeMap::new(),
            last_workspace: None,
            require_shell_approval: true,
            require_write_approval: true,
        };

        assert_eq!(
            image_api_key_from_config(&config, OPENAI_IMAGE_PROVIDER_ID),
            "sk-openai"
        );
    }
}
