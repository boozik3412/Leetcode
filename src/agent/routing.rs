use crate::agent::models::{
    model_specs, provider_name, ModelCapability, ModelSpec, ANTHROPIC_PROVIDER_ID,
    DEEPSEEK_PROVIDER_ID, GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID,
};
use crate::config::AppConfig;
use serde::{Deserialize, Serialize};

pub const ROUTE_AUTO: &str = "auto";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRoute {
    Coding,
    Planning,
    CheapFast,
    Vision,
    Image,
    Audio,
    Video,
    Realtime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteCandidate {
    pub provider_id: String,
    pub model_id: String,
    pub task: TaskRoute,
    pub reason: String,
}

pub fn route_labels() -> &'static [(&'static str, &'static str)] {
    &[
        (ROUTE_AUTO, "Auto"),
        ("coding", "Coding"),
        ("planning", "Planning"),
        ("cheap_fast", "Fast"),
        ("vision", "Vision"),
        ("image", "Image"),
        ("audio", "Audio"),
        ("video", "Video"),
        ("realtime", "Realtime"),
    ]
}

pub fn parse_task_route(value: &str) -> Option<TaskRoute> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "coding" | "code" => Some(TaskRoute::Coding),
        "planning" | "plan" | "reasoning" => Some(TaskRoute::Planning),
        "cheap_fast" | "cheap" | "fast" | "quick" => Some(TaskRoute::CheapFast),
        "vision" | "image_input" => Some(TaskRoute::Vision),
        "image" | "image_generation" => Some(TaskRoute::Image),
        "audio" | "sound" | "speech" => Some(TaskRoute::Audio),
        "video" | "movie" => Some(TaskRoute::Video),
        "realtime" | "live" | "voice" => Some(TaskRoute::Realtime),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn route_id(task: TaskRoute) -> &'static str {
    match task {
        TaskRoute::Coding => "coding",
        TaskRoute::Planning => "planning",
        TaskRoute::CheapFast => "cheap_fast",
        TaskRoute::Vision => "vision",
        TaskRoute::Image => "image",
        TaskRoute::Audio => "audio",
        TaskRoute::Video => "video",
        TaskRoute::Realtime => "realtime",
    }
}

pub fn route_name(task: TaskRoute) -> &'static str {
    match task {
        TaskRoute::Coding => "Coding",
        TaskRoute::Planning => "Planning",
        TaskRoute::CheapFast => "Fast",
        TaskRoute::Vision => "Vision",
        TaskRoute::Image => "Image",
        TaskRoute::Audio => "Audio",
        TaskRoute::Video => "Video",
        TaskRoute::Realtime => "Realtime",
    }
}

pub fn resolve_task_route(config: &AppConfig, user_input: &str) -> TaskRoute {
    if let Some(task) = parse_task_route(&config.task_route) {
        return task;
    }

    infer_task_route(user_input)
}

pub fn infer_task_route(user_input: &str) -> TaskRoute {
    let text = user_input.to_ascii_lowercase();
    if any_contains(&text, &["video", "sora", "mp4", "clip", "animation"]) {
        TaskRoute::Video
    } else if any_contains(
        &text,
        &["audio", "sound", "sfx", "music", "voice", "tts", "speech"],
    ) {
        TaskRoute::Audio
    } else if any_contains(
        &text,
        &[
            "image",
            "icon",
            "logo",
            "sprite",
            "spritesheet",
            "asset art",
        ],
    ) {
        TaskRoute::Image
    } else if any_contains(&text, &["screenshot", "look at", "vision", "visible"]) {
        TaskRoute::Vision
    } else if any_contains(&text, &["plan", "architecture", "roadmap", "design"]) {
        TaskRoute::Planning
    } else if any_contains(&text, &["quick", "fast", "cheap", "simple answer"]) {
        TaskRoute::CheapFast
    } else {
        TaskRoute::Coding
    }
}

pub fn route_candidates(config: &AppConfig, task: TaskRoute) -> Vec<RouteCandidate> {
    let mut candidates = Vec::new();
    let active_provider = config.provider_id();
    let active_model = config.model_for_provider(active_provider);
    if model_supports_agent_route(active_provider, &active_model, task)
        || is_unknown_active_text_model(active_provider, &active_model, task)
    {
        candidates.push(RouteCandidate {
            provider_id: active_provider.to_string(),
            model_id: active_model,
            task,
            reason: format!("selected {} model", provider_name(active_provider)),
        });
    }

    for model in preferred_models_for_task(task) {
        if !provider_has_key(config, model.provider_id) {
            continue;
        }
        if candidates.iter().any(|candidate| {
            candidate.provider_id == model.provider_id && candidate.model_id == model.id
        }) {
            continue;
        }
        candidates.push(RouteCandidate {
            provider_id: model.provider_id.to_string(),
            model_id: model.id.to_string(),
            task,
            reason: format!(
                "{} fallback with {:?}",
                provider_name(model.provider_id),
                task
            ),
        });
    }

    candidates
}

pub fn describe_route_plan(candidates: &[RouteCandidate]) -> String {
    if candidates.is_empty() {
        return "No provider/model route is available for this task.".to_string();
    }

    candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            format!(
                "{}. {} / {} ({})",
                idx + 1,
                provider_name(&candidate.provider_id),
                candidate.model_id,
                candidate.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(dead_code)]
pub fn model_supports_task(provider_id: &str, model_id: &str, task: TaskRoute) -> bool {
    let requirements = task_requirements(task);
    model_specs()
        .iter()
        .find(|model| model.provider_id == provider_id && model.id == model_id)
        .map(|model| {
            requirements
                .iter()
                .all(|required| model.capabilities.contains(required))
        })
        .unwrap_or(false)
}

pub fn provider_has_key(config: &AppConfig, provider_id: &str) -> bool {
    !config.api_key_for_provider(provider_id).trim().is_empty()
}

pub fn provider_env_hint(provider_id: &str) -> &'static str {
    match provider_id {
        OPENAI_PROVIDER_ID => "OPENAI_API_KEY",
        ANTHROPIC_PROVIDER_ID => "ANTHROPIC_API_KEY",
        DEEPSEEK_PROVIDER_ID => "DEEPSEEK_API_KEY",
        GEMINI_PROVIDER_ID => "GEMINI_API_KEY",
        "openai-image" | "openai-audio" | "openai-video" => "OPENAI_API_KEY",
        "gemini-image" => "GEMINI_API_KEY",
        "stability-image" => "STABILITY_API_KEY",
        "replicate-image" | "replicate-video" => "REPLICATE_API_TOKEN",
        _ => "API_KEY",
    }
}

pub fn friendly_provider_error(provider_id: &str, model_id: &str, error: &anyhow::Error) -> String {
    let raw = error.to_string();
    let lower = raw.to_ascii_lowercase();
    let hint =
        if lower.contains("401") || lower.contains("unauthorized") || lower.contains("api key") {
            format!(
                "Check the saved {} key or set {}.",
                provider_name(provider_id),
                provider_env_hint(provider_id)
            )
        } else if lower.contains("404") || lower.contains("model") {
            format!(
                "The model '{}' may be unavailable for {}. Pick another model or use Auto routing.",
                model_id,
                provider_name(provider_id)
            )
        } else if lower.contains("429") || lower.contains("rate") || lower.contains("quota") {
            "Rate limit or quota was hit. Try a fallback provider/model or wait before retrying."
                .to_string()
        } else if lower.contains("tool") || lower.contains("function") {
            format!(
                "{} / {} may not support the required tool-calling pattern.",
                provider_name(provider_id),
                model_id
            )
        } else {
            "Provider request failed. The raw API message is included for debugging.".to_string()
        };

    format!(
        "{} / {} failed: {}\n{}",
        provider_name(provider_id),
        model_id,
        raw,
        hint
    )
}

fn preferred_models_for_task(task: TaskRoute) -> Vec<&'static ModelSpec> {
    let provider_order = match task {
        TaskRoute::CheapFast => [
            OPENAI_PROVIDER_ID,
            GEMINI_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
            DEEPSEEK_PROVIDER_ID,
        ],
        TaskRoute::Audio | TaskRoute::Video | TaskRoute::Image | TaskRoute::Realtime => [
            OPENAI_PROVIDER_ID,
            GEMINI_PROVIDER_ID,
            DEEPSEEK_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
        ],
        _ => [
            OPENAI_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
            GEMINI_PROVIDER_ID,
            DEEPSEEK_PROVIDER_ID,
        ],
    };

    let mut models = model_specs()
        .iter()
        .filter(|model| model_supports_agent_route(model.provider_id, model.id, task))
        .collect::<Vec<_>>();
    models.sort_by_key(|model| {
        provider_order
            .iter()
            .position(|provider| *provider == model.provider_id)
            .unwrap_or(usize::MAX)
    });
    models
}

fn task_requirements(task: TaskRoute) -> &'static [ModelCapability] {
    match task {
        TaskRoute::Coding => &[ModelCapability::Code, ModelCapability::Tools],
        TaskRoute::Planning => &[ModelCapability::Reasoning, ModelCapability::Tools],
        TaskRoute::CheapFast => &[ModelCapability::Tools],
        TaskRoute::Vision => &[ModelCapability::Vision, ModelCapability::Tools],
        TaskRoute::Image => &[ModelCapability::Image],
        TaskRoute::Audio => &[ModelCapability::Audio],
        TaskRoute::Video => &[ModelCapability::Video],
        TaskRoute::Realtime => &[ModelCapability::Realtime],
    }
}

fn agent_route_requirements(task: TaskRoute) -> &'static [ModelCapability] {
    match task {
        TaskRoute::Image | TaskRoute::Audio | TaskRoute::Video | TaskRoute::Realtime => {
            &[ModelCapability::Reasoning, ModelCapability::Tools]
        }
        _ => task_requirements(task),
    }
}

fn model_supports_agent_route(provider_id: &str, model_id: &str, task: TaskRoute) -> bool {
    let requirements = agent_route_requirements(task);
    model_specs()
        .iter()
        .find(|model| model.provider_id == provider_id && model.id == model_id)
        .map(|model| {
            requirements
                .iter()
                .all(|required| model.capabilities.contains(required))
        })
        .unwrap_or(false)
}

fn is_unknown_active_text_model(provider_id: &str, model_id: &str, task: TaskRoute) -> bool {
    let known = model_specs()
        .iter()
        .any(|model| model.provider_id == provider_id && model.id == model_id);
    !known
        && matches!(
            task,
            TaskRoute::Coding | TaskRoute::Planning | TaskRoute::CheapFast | TaskRoute::Vision
        )
}

fn any_contains(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ProviderSettings};
    use std::collections::BTreeMap;

    #[test]
    fn infers_media_tasks_from_prompt() {
        assert_eq!(infer_task_route("generate a UI sound"), TaskRoute::Audio);
        assert_eq!(infer_task_route("make an mp4 trailer"), TaskRoute::Video);
        assert_eq!(infer_task_route("create a sprite icon"), TaskRoute::Image);
    }

    #[test]
    fn route_candidates_skip_unkeyed_fallbacks() {
        let mut providers = BTreeMap::new();
        providers.insert(
            OPENAI_PROVIDER_ID.to_string(),
            ProviderSettings {
                api_key: "sk".to_string(),
                model: "gpt-5.5".to_string(),
            },
        );
        let config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: "sk".to_string(),
            model: "gpt-5.5".to_string(),
            providers,
            last_workspace: None,
            policy_profile: "normal".to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            task_route: ROUTE_AUTO.to_string(),
        };

        let candidates = route_candidates(&config, TaskRoute::Coding);

        assert_eq!(candidates[0].provider_id, OPENAI_PROVIDER_ID);
        assert!(candidates.iter().all(|candidate| {
            candidate.provider_id == OPENAI_PROVIDER_ID
                || provider_has_key(&config, &candidate.provider_id)
        }));
    }
}
