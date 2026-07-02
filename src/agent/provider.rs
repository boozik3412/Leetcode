use crate::agent::anthropic::AnthropicClient;
use crate::agent::deepseek::DeepSeekClient;
use crate::agent::gemini::GeminiClient;
use crate::agent::models::{
    model_has_capability, models_for_provider, provider_name, ModelCapability,
    ANTHROPIC_PROVIDER_ID, DEEPSEEK_PROVIDER_ID, GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID,
};
use crate::agent::openai::OpenAiClient;
use crate::agent::types::{AppEvent, ToolCall};
use crate::config::AppConfig;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum ProviderInput {
    Text(String),
    ToolOutputs(Vec<Value>),
}

#[derive(Debug)]
pub struct ProviderTurn {
    pub response_id: String,
    pub text_chunks: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    pub emitted_text: bool,
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;

    fn import_state(&self, _state: Option<Value>) -> anyhow::Result<()> {
        Ok(())
    }

    fn export_state(&self) -> Option<Value> {
        None
    }

    async fn stream_turn(
        &self,
        instructions: &str,
        input: ProviderInput,
        previous_response_id: Option<&str>,
        events: &Sender<AppEvent>,
    ) -> anyhow::Result<ProviderTurn>;
}

pub fn normalized_tool_outputs(outputs: &[Value]) -> Vec<(String, String)> {
    outputs
        .iter()
        .filter_map(|output| {
            let call_id = output.get("call_id").and_then(Value::as_str)?.to_string();
            let content = output
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some((call_id, content))
        })
        .collect()
}

pub fn build_provider(config: &AppConfig) -> anyhow::Result<Box<dyn ModelProvider>> {
    let provider_id = config.provider_id();
    ensure_provider_supports_tools(provider_id, &config.model_for_provider(provider_id))?;

    match provider_id {
        OPENAI_PROVIDER_ID => {
            let api_key = config.api_key_for_provider(OPENAI_PROVIDER_ID);
            if api_key.trim().is_empty() {
                anyhow::bail!(
                    "OpenAI API key is empty. Paste a key in the top bar, click Save, or set OPENAI_API_KEY."
                );
            }

            let model = config.model_for_provider(OPENAI_PROVIDER_ID);
            Ok(Box::new(OpenAiClient::new(api_key, model)))
        }
        ANTHROPIC_PROVIDER_ID => {
            let api_key = config.api_key_for_provider(ANTHROPIC_PROVIDER_ID);
            if api_key.trim().is_empty() {
                anyhow::bail!(
                    "Anthropic API key is empty. Select Claude, paste a key, click Save, or set ANTHROPIC_API_KEY."
                );
            }

            Ok(Box::new(AnthropicClient::new(
                api_key,
                config.model_for_provider(ANTHROPIC_PROVIDER_ID),
            )))
        }
        DEEPSEEK_PROVIDER_ID => {
            let api_key = config.api_key_for_provider(DEEPSEEK_PROVIDER_ID);
            if api_key.trim().is_empty() {
                anyhow::bail!(
                    "DeepSeek API key is empty. Select DeepSeek, paste a key, click Save, or set DEEPSEEK_API_KEY."
                );
            }

            Ok(Box::new(DeepSeekClient::new(
                api_key,
                config.model_for_provider(DEEPSEEK_PROVIDER_ID),
            )))
        }
        GEMINI_PROVIDER_ID => {
            let api_key = config.api_key_for_provider(GEMINI_PROVIDER_ID);
            if api_key.trim().is_empty() {
                anyhow::bail!(
                    "Gemini API key is empty. Select Gemini, paste a key, click Save, or set GEMINI_API_KEY."
                );
            }

            Ok(Box::new(GeminiClient::new(
                api_key,
                config.model_for_provider(GEMINI_PROVIDER_ID),
            )))
        }
        unsupported => anyhow::bail!(
            "Provider '{}' is not implemented yet. Current implemented provider: {}.",
            unsupported,
            provider_name(OPENAI_PROVIDER_ID)
        ),
    }
}

fn ensure_provider_supports_tools(provider_id: &str, model: &str) -> anyhow::Result<()> {
    let is_known_model = models_for_provider(provider_id).any(|known| known.id == model);
    if is_known_model && !model_has_capability(provider_id, model, ModelCapability::Tools) {
        anyhow::bail!(
            "{} model '{}' is registered but does not support tool calling, which this coding agent requires.",
            provider_name(provider_id),
            model
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_openai_style_tool_outputs() {
        let outputs = vec![json!({
            "type": "function_call_output",
            "call_id": "call_1",
            "output": "{\"ok\":true,\"output\":\"done\"}"
        })];

        assert_eq!(
            normalized_tool_outputs(&outputs),
            vec![(
                "call_1".to_string(),
                "{\"ok\":true,\"output\":\"done\"}".to_string()
            )]
        );
    }
}
