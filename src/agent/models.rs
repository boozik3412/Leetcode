#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelCapability {
    Code,
    Reasoning,
    Tools,
    Vision,
    Image,
    Audio,
    Video,
    Realtime,
    Embeddings,
}

#[derive(Clone, Copy, Debug)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub default_model: &'static str,
    pub implemented: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    pub provider_id: &'static str,
    pub id: &'static str,
    pub name: &'static str,
    pub capabilities: &'static [ModelCapability],
}

pub const OPENAI_PROVIDER_ID: &str = "openai";
pub const ANTHROPIC_PROVIDER_ID: &str = "anthropic";
pub const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
pub const GEMINI_PROVIDER_ID: &str = "gemini";

pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5.5";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-5";
pub const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-v4-flash";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash";

const OPENAI_CORE_CAPABILITIES: &[ModelCapability] = &[
    ModelCapability::Code,
    ModelCapability::Reasoning,
    ModelCapability::Tools,
    ModelCapability::Vision,
];

const OPENAI_FAST_CAPABILITIES: &[ModelCapability] = &[
    ModelCapability::Code,
    ModelCapability::Tools,
    ModelCapability::Vision,
];

const TEXT_AGENT_CAPABILITIES: &[ModelCapability] = &[
    ModelCapability::Code,
    ModelCapability::Reasoning,
    ModelCapability::Tools,
    ModelCapability::Vision,
];

const TEXT_FAST_AGENT_CAPABILITIES: &[ModelCapability] = &[
    ModelCapability::Code,
    ModelCapability::Tools,
    ModelCapability::Vision,
];

const TEXT_ONLY_AGENT_CAPABILITIES: &[ModelCapability] = &[
    ModelCapability::Code,
    ModelCapability::Reasoning,
    ModelCapability::Tools,
];

const IMAGE_CAPABILITIES: &[ModelCapability] = &[ModelCapability::Image];

const AUDIO_CAPABILITIES: &[ModelCapability] = &[ModelCapability::Audio];

const VIDEO_CAPABILITIES: &[ModelCapability] = &[ModelCapability::Video];

const REALTIME_CAPABILITIES: &[ModelCapability] = &[
    ModelCapability::Audio,
    ModelCapability::Realtime,
    ModelCapability::Tools,
];

const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: OPENAI_PROVIDER_ID,
        name: "OpenAI",
        default_model: DEFAULT_OPENAI_MODEL,
        implemented: true,
    },
    ProviderSpec {
        id: ANTHROPIC_PROVIDER_ID,
        name: "Claude",
        default_model: DEFAULT_ANTHROPIC_MODEL,
        implemented: true,
    },
    ProviderSpec {
        id: DEEPSEEK_PROVIDER_ID,
        name: "DeepSeek",
        default_model: DEFAULT_DEEPSEEK_MODEL,
        implemented: true,
    },
    ProviderSpec {
        id: GEMINI_PROVIDER_ID,
        name: "Gemini",
        default_model: DEFAULT_GEMINI_MODEL,
        implemented: true,
    },
];

const MODELS: &[ModelSpec] = &[
    ModelSpec {
        provider_id: OPENAI_PROVIDER_ID,
        id: "gpt-5.5",
        name: "GPT-5.5",
        capabilities: OPENAI_CORE_CAPABILITIES,
    },
    ModelSpec {
        provider_id: OPENAI_PROVIDER_ID,
        id: "gpt-5.4",
        name: "GPT-5.4",
        capabilities: OPENAI_CORE_CAPABILITIES,
    },
    ModelSpec {
        provider_id: OPENAI_PROVIDER_ID,
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        capabilities: OPENAI_FAST_CAPABILITIES,
    },
    ModelSpec {
        provider_id: ANTHROPIC_PROVIDER_ID,
        id: "claude-sonnet-5",
        name: "Claude Sonnet 5",
        capabilities: TEXT_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: ANTHROPIC_PROVIDER_ID,
        id: "claude-opus-4-8",
        name: "Claude Opus 4.8",
        capabilities: TEXT_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: ANTHROPIC_PROVIDER_ID,
        id: "claude-haiku-4-5",
        name: "Claude Haiku 4.5",
        capabilities: TEXT_FAST_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: DEEPSEEK_PROVIDER_ID,
        id: "deepseek-v4-flash",
        name: "DeepSeek V4 Flash",
        capabilities: TEXT_ONLY_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: DEEPSEEK_PROVIDER_ID,
        id: "deepseek-chat",
        name: "DeepSeek Chat (legacy alias)",
        capabilities: TEXT_ONLY_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: DEEPSEEK_PROVIDER_ID,
        id: "deepseek-reasoner",
        name: "DeepSeek Reasoner (legacy alias)",
        capabilities: &[ModelCapability::Reasoning],
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-3.5-flash",
        name: "Gemini 3.5 Flash",
        capabilities: TEXT_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-3.1-pro-preview-customtools",
        name: "Gemini 3.1 Pro Custom Tools",
        capabilities: TEXT_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-3.1-pro-preview",
        name: "Gemini 3.1 Pro Preview",
        capabilities: TEXT_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-3.1-flash-lite",
        name: "Gemini 3.1 Flash-Lite",
        capabilities: TEXT_FAST_AGENT_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-3.1-flash-image",
        name: "Gemini 3.1 Flash Image",
        capabilities: IMAGE_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-3.1-flash-tts-preview",
        name: "Gemini 3.1 Flash TTS",
        capabilities: AUDIO_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-2.5-flash-native-audio-preview-12-2025",
        name: "Gemini 2.5 Flash Live",
        capabilities: REALTIME_CAPABILITIES,
    },
    ModelSpec {
        provider_id: GEMINI_PROVIDER_ID,
        id: "gemini-omni-flash-preview",
        name: "Gemini Omni Flash",
        capabilities: VIDEO_CAPABILITIES,
    },
];

pub fn provider_specs() -> &'static [ProviderSpec] {
    PROVIDERS
}

pub fn model_specs() -> &'static [ModelSpec] {
    MODELS
}

pub fn models_for_provider(provider_id: &str) -> impl Iterator<Item = &'static ModelSpec> + '_ {
    let provider_id = provider_id.trim();
    model_specs()
        .iter()
        .filter(move |model| model.provider_id == provider_id)
}

pub fn provider_name(provider_id: &str) -> &'static str {
    provider_specs()
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.name)
        .unwrap_or("Custom")
}

pub fn default_model_for_provider(provider_id: &str) -> &'static str {
    provider_specs()
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.default_model)
        .unwrap_or(DEFAULT_OPENAI_MODEL)
}

pub fn model_has_capability(
    provider_id: &str,
    model_id: &str,
    capability: ModelCapability,
) -> bool {
    model_specs()
        .iter()
        .find(|model| model.provider_id == provider_id && model.id == model_id)
        .map(|model| model.capabilities.contains(&capability))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_is_default_implemented_provider() {
        let provider = provider_specs()
            .iter()
            .find(|provider| provider.id == OPENAI_PROVIDER_ID)
            .expect("openai provider exists");

        assert!(provider.implemented);
        assert_eq!(provider.default_model, DEFAULT_OPENAI_MODEL);
    }

    #[test]
    fn default_openai_model_supports_tools() {
        assert!(model_has_capability(
            OPENAI_PROVIDER_ID,
            DEFAULT_OPENAI_MODEL,
            ModelCapability::Tools
        ));
    }

    #[test]
    fn primary_providers_are_registered_and_implemented() {
        for provider_id in [
            OPENAI_PROVIDER_ID,
            ANTHROPIC_PROVIDER_ID,
            DEEPSEEK_PROVIDER_ID,
            GEMINI_PROVIDER_ID,
        ] {
            let provider = provider_specs()
                .iter()
                .find(|provider| provider.id == provider_id)
                .expect("provider exists");

            assert!(provider.implemented);
            assert!(model_has_capability(
                provider.id,
                provider.default_model,
                ModelCapability::Tools
            ));
        }
    }
}
