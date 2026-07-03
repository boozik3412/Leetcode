use crate::agent::models::{
    model_specs, provider_name, provider_specs, ModelCapability, GEMINI_PROVIDER_ID,
    OPENAI_PROVIDER_ID,
};
use crate::agent::provider::{build_provider_for, ProviderInput};
use crate::agent::types::{AppEvent, ToolResult};
use crate::assets::{
    asset_provider_env_var, audio_provider_name, image_provider_specs, video_provider_name,
    GEMINI_IMAGE_PROVIDER_ID, OPENAI_AUDIO_PROVIDER_ID, OPENAI_IMAGE_PROVIDER_ID,
    OPENAI_VIDEO_PROVIDER_ID,
};
use crate::config::AppConfig;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const PROVIDER_VALIDATION_RESULTS_PATH: &str =
    "assets/generated/leetcode/provider_validation_results.json";

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderValidationStep {
    pub provider_id: String,
    pub provider_name: String,
    pub check: String,
    pub requires_key: bool,
    pub destructive_or_paid: bool,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderValidationRun {
    pub id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub ok: bool,
    pub elapsed_ms: u128,
    pub created_at: u64,
    pub checks: Vec<ProviderValidationCheckResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderValidationCheckResult {
    pub check: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderValidationHistory {
    #[serde(default)]
    pub runs: Vec<ProviderValidationRun>,
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

pub fn provider_validation_plan(config: &AppConfig) -> Vec<ProviderValidationStep> {
    let mut steps = Vec::new();
    for provider in provider_specs()
        .iter()
        .filter(|provider| provider.implemented)
    {
        let key_present = !config.api_key_for_provider(provider.id).trim().is_empty();
        steps.push(ProviderValidationStep {
            provider_id: provider.id.to_string(),
            provider_name: provider.name.to_string(),
            check: "streaming text response".to_string(),
            requires_key: true,
            destructive_or_paid: true,
            status: validation_status(key_present),
        });
        steps.push(ProviderValidationStep {
            provider_id: provider.id.to_string(),
            provider_name: provider.name.to_string(),
            check: "tool-call roundtrip".to_string(),
            requires_key: true,
            destructive_or_paid: true,
            status: validation_status(key_present),
        });
    }

    for provider in image_provider_specs() {
        let key_present = asset_key_present(config, provider.id);
        steps.push(ProviderValidationStep {
            provider_id: provider.id.to_string(),
            provider_name: provider.name.to_string(),
            check: "small image generation smoke test".to_string(),
            requires_key: true,
            destructive_or_paid: true,
            status: validation_status(key_present),
        });
    }
    for (id, name) in [
        (
            OPENAI_AUDIO_PROVIDER_ID,
            audio_provider_name(OPENAI_AUDIO_PROVIDER_ID),
        ),
        (
            OPENAI_VIDEO_PROVIDER_ID,
            video_provider_name(OPENAI_VIDEO_PROVIDER_ID),
        ),
    ] {
        let key_present = asset_key_present(config, id);
        steps.push(ProviderValidationStep {
            provider_id: id.to_string(),
            provider_name: name.to_string(),
            check: if id == OPENAI_AUDIO_PROVIDER_ID {
                "short audio generation smoke test".to_string()
            } else {
                "short video generation smoke test".to_string()
            },
            requires_key: true,
            destructive_or_paid: true,
            status: validation_status(key_present),
        });
    }
    steps
}

pub fn provider_health_snapshot(config: &AppConfig) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&json!(provider_health_report(config)))
            .unwrap_or_else(|_| "provider health".to_string()),
    )
}

pub fn load_provider_validation_history(workspace: &Workspace) -> ProviderValidationHistory {
    workspace
        .read_text(PROVIDER_VALIDATION_RESULTS_PATH, 1_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn record_provider_validation_run(
    workspace: &Workspace,
    run: ProviderValidationRun,
) -> anyhow::Result<ProviderValidationHistory> {
    let mut history = load_provider_validation_history(workspace);
    history.runs.push(run);
    if history.runs.len() > 100 {
        let overflow = history.runs.len() - 100;
        history.runs.drain(0..overflow);
    }
    workspace.write_text(
        PROVIDER_VALIDATION_RESULTS_PATH,
        &serde_json::to_string_pretty(&history)?,
    )?;
    Ok(history)
}

pub async fn run_provider_live_validation(
    config: AppConfig,
    provider_id: String,
) -> ProviderValidationRun {
    let started = Instant::now();
    let provider_name = provider_name(&provider_id).to_string();
    let model = config.model_for_provider(&provider_id);
    let mut run = ProviderValidationRun {
        id: format!("provider-validation-{}", uuid::Uuid::new_v4()),
        provider_id: provider_id.clone(),
        provider_name,
        model: model.clone(),
        ok: false,
        elapsed_ms: 0,
        created_at: unix_timestamp(),
        checks: Vec::new(),
    };

    if config.api_key_for_provider(&provider_id).trim().is_empty() {
        run.checks.push(ProviderValidationCheckResult {
            check: "API key".to_string(),
            ok: false,
            detail: "API-ключ не сохранён для этого провайдера".to_string(),
        });
        run.elapsed_ms = started.elapsed().as_millis();
        return run;
    }

    let provider = match build_provider_for(&config, &provider_id, &model) {
        Ok(provider) => provider,
        Err(err) => {
            run.checks.push(ProviderValidationCheckResult {
                check: "provider setup".to_string(),
                ok: false,
                detail: compact_validation_detail(&err.to_string()),
            });
            run.elapsed_ms = started.elapsed().as_millis();
            return run;
        }
    };

    let (events, _rx) = mpsc::channel::<AppEvent>();
    let instructions = "You are running a tiny provider validation for a desktop coding agent. Keep responses minimal and deterministic.";
    let mut previous_response_id = None;

    match provider
        .stream_turn(
            instructions,
            ProviderInput::Text("Return exactly: LEETCODE_PROVIDER_OK".to_string()),
            None,
            &events,
        )
        .await
    {
        Ok(turn) => {
            let text = turn.text_chunks.join("");
            let ok = text.contains("LEETCODE_PROVIDER_OK");
            previous_response_id = Some(turn.response_id);
            run.checks.push(ProviderValidationCheckResult {
                check: "text response".to_string(),
                ok,
                detail: if ok {
                    "модель ответила ожидаемым текстом".to_string()
                } else {
                    format!(
                        "получен неожиданный текст: {}",
                        compact_validation_detail(&text)
                    )
                },
            });
        }
        Err(err) => {
            run.checks.push(ProviderValidationCheckResult {
                check: "text response".to_string(),
                ok: false,
                detail: compact_validation_detail(&err.to_string()),
            });
        }
    }

    let previous_response_id_ref = previous_response_id.as_deref();
    match provider
        .stream_turn(
            instructions,
            ProviderInput::Text(
                "For validation, call the function tool `act` with arguments {\"action\":\"list_files\",\"args\":{\"path\":\".\",\"depth\":1,\"limit\":1}}. Do not run anything outside the tool call."
                    .to_string(),
            ),
            previous_response_id_ref,
            &events,
        )
        .await
    {
        Ok(turn) => {
            let ok = turn.tool_calls.iter().any(|call| {
                call.name == "act"
                    && call.arguments.contains("list_files")
                    && call.arguments.contains("\"path\"")
            });
            run.checks.push(ProviderValidationCheckResult {
                check: "tool-call shape".to_string(),
                ok,
                detail: if ok {
                    "модель вернула корректный вызов инструмента без локального выполнения"
                        .to_string()
                } else if turn.tool_calls.is_empty() {
                    format!(
                        "tool call не получен; текст: {}",
                        compact_validation_detail(&turn.text_chunks.join(""))
                    )
                } else {
                    format!("получен другой tool call: {:?}", turn.tool_calls)
                },
            });
        }
        Err(err) => {
            run.checks.push(ProviderValidationCheckResult {
                check: "tool-call shape".to_string(),
                ok: false,
                detail: compact_validation_detail(&err.to_string()),
            });
        }
    }

    run.ok = !run.checks.is_empty() && run.checks.iter().all(|check| check.ok);
    run.elapsed_ms = started.elapsed().as_millis();
    run
}

fn validation_status(key_present: bool) -> String {
    if key_present {
        "готово к ручному live-тесту".to_string()
    } else {
        "нужен API-ключ".to_string()
    }
}

fn compact_validation_detail(text: &str) -> String {
    const MAX_CHARS: usize = 500;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let mut compacted = text.chars().take(MAX_CHARS).collect::<String>();
    compacted.push_str(" ...");
    compacted
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_provider_validation_history_with_limit() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        for idx in 0..105 {
            let run = ProviderValidationRun {
                id: format!("run-{idx}"),
                provider_id: OPENAI_PROVIDER_ID.to_string(),
                provider_name: "OpenAI".to_string(),
                model: "gpt-test".to_string(),
                ok: idx % 2 == 0,
                elapsed_ms: idx,
                created_at: idx as u64,
                checks: vec![ProviderValidationCheckResult {
                    check: "text response".to_string(),
                    ok: true,
                    detail: "ok".to_string(),
                }],
            };
            record_provider_validation_run(&workspace, run).unwrap();
        }

        let history = load_provider_validation_history(&workspace);
        assert_eq!(history.runs.len(), 100);
        assert_eq!(history.runs.first().unwrap().id, "run-5");
        assert_eq!(history.runs.last().unwrap().id, "run-104");
    }
}
