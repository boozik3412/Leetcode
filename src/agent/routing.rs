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
        (ROUTE_AUTO, "Авто"),
        ("coding", "Код"),
        ("planning", "План"),
        ("cheap_fast", "Быстро"),
        ("vision", "Зрение"),
        ("image", "Изображения"),
        ("audio", "Аудио"),
        ("video", "Видео"),
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
        TaskRoute::Coding => "Код",
        TaskRoute::Planning => "План",
        TaskRoute::CheapFast => "Быстро",
        TaskRoute::Vision => "Зрение",
        TaskRoute::Image => "Изображения",
        TaskRoute::Audio => "Аудио",
        TaskRoute::Video => "Видео",
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
            reason: format!("выбранная модель {}", provider_name(active_provider)),
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
                "запасной маршрут {} для {}",
                provider_name(model.provider_id),
                route_name(task)
            ),
        });
    }

    candidates
}

pub fn describe_route_plan(candidates: &[RouteCandidate]) -> String {
    if candidates.is_empty() {
        return "Для этой задачи нет доступного маршрута провайдер/модель.".to_string();
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
    let hint = if lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("api key")
    {
        format!(
            "Проверьте сохранённый ключ {} или задайте {}.",
            provider_name(provider_id),
            provider_env_hint(provider_id)
        )
    } else if lower.contains("404") || lower.contains("model") {
        format!(
                "Модель '{}' может быть недоступна для {}. Выберите другую модель или используйте автомаршрутизацию.",
                model_id,
                provider_name(provider_id)
            )
    } else if lower.contains("429") || lower.contains("rate") || lower.contains("quota") {
        "Сработал rate limit или квота. Попробуйте запасного провайдера/модель или повторите позже."
            .to_string()
    } else if lower.contains("tool") || lower.contains("function") {
        format!(
            "{} / {} может не поддерживать нужный формат вызова инструментов.",
            provider_name(provider_id),
            model_id
        )
    } else {
        "Запрос к провайдеру не выполнен. Ниже оставлено сырое API-сообщение для диагностики."
            .to_string()
    };

    format!(
        "{} / {} не выполнен: {}\n{}",
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
            projects: Vec::new(),
            agent_id: String::new(),
            policy_profile: "ask".to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: ROUTE_AUTO.to_string(),
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: "http".to_string(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: "127.0.0.1".to_string(),
            remote_port: 17890,
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: 120,
            remote_audit_enabled: true,
            context_recent_messages: 14,
            context_relevant_messages: 8,
            context_recent_runs: 5,
            layout_workspace_mode: "chat".to_string(),
            layout_right_panel_view: "context".to_string(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
        };

        let candidates = route_candidates(&config, TaskRoute::Coding);

        assert_eq!(candidates[0].provider_id, OPENAI_PROVIDER_ID);
        assert!(candidates.iter().all(|candidate| {
            candidate.provider_id == OPENAI_PROVIDER_ID
                || provider_has_key(&config, &candidate.provider_id)
        }));
    }
}
