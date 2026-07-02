use crate::agent::models::ANTHROPIC_PROVIDER_ID;
use crate::agent::openai::anthropic_act_tool_schema;
use crate::agent::provider::{normalized_tool_outputs, ModelProvider, ProviderInput, ProviderTurn};
use crate::agent::types::{AppEvent, ToolCall};
use anyhow::Context;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

pub struct AnthropicClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
    messages: Mutex<Vec<Value>>,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            messages: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ModelProvider for AnthropicClient {
    fn id(&self) -> &'static str {
        ANTHROPIC_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Claude Messages"
    }

    fn import_state(&self, state: Option<Value>) -> anyhow::Result<()> {
        if let Some(Value::Array(messages)) = state {
            *self.messages.lock().expect("anthropic messages poisoned") = messages;
        }
        Ok(())
    }

    fn export_state(&self) -> Option<Value> {
        Some(Value::Array(
            self.messages
                .lock()
                .expect("anthropic messages poisoned")
                .clone(),
        ))
    }

    async fn stream_turn(
        &self,
        instructions: &str,
        input: ProviderInput,
        _previous_response_id: Option<&str>,
        _events: &Sender<AppEvent>,
    ) -> anyhow::Result<ProviderTurn> {
        {
            let mut messages = self.messages.lock().expect("anthropic messages poisoned");
            match input {
                ProviderInput::Text(text) => messages.push(json!({
                    "role": "user",
                    "content": text
                })),
                ProviderInput::ToolOutputs(outputs) => {
                    let content = normalized_tool_outputs(&outputs)
                        .into_iter()
                        .map(|(call_id, output)| {
                            json!({
                                "type": "tool_result",
                                "tool_use_id": call_id,
                                "content": output
                            })
                        })
                        .collect::<Vec<_>>();
                    messages.push(json!({
                        "role": "user",
                        "content": content
                    }));
                }
            }
        }

        let messages = self
            .messages
            .lock()
            .expect("anthropic messages poisoned")
            .clone();
        let body = json!({
            "model": self.model,
            "max_tokens": 8192,
            "system": instructions,
            "messages": messages,
            "tools": [anthropic_act_tool_schema()]
        });

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Anthropic request failed")?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Anthropic API error {status}: {text}");
        }

        let parsed = serde_json::from_str::<AnthropicResponse>(&text).with_context(|| {
            format!(
                "Could not parse Anthropic response. First bytes: {}",
                text.chars().take(500).collect::<String>()
            )
        })?;
        let text_chunks = anthropic_text_chunks(&parsed.content);
        let tool_calls = anthropic_tool_calls(&parsed.content)?;

        self.messages
            .lock()
            .expect("anthropic messages poisoned")
            .push(json!({
                "role": "assistant",
                "content": parsed.content
            }));

        Ok(ProviderTurn {
            response_id: parsed.id,
            text_chunks,
            tool_calls,
            emitted_text: false,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    #[serde(default)]
    content: Vec<Value>,
}

fn anthropic_text_chunks(content: &[Value]) -> Vec<String> {
    content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

fn anthropic_tool_calls(content: &[Value]) -> anyhow::Result<Vec<ToolCall>> {
    let mut calls = Vec::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }

        let call_id = block
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("anthropic-tool-call")
            .to_string();
        let name = block
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("act")
            .to_string();
        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
        calls.push(ToolCall {
            call_id,
            name,
            arguments: serde_json::to_string(&input)?,
        });
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_anthropic_text_and_tool_use() {
        let raw = r#"{
          "id": "msg_1",
          "content": [
            {"type": "text", "text": "I will inspect files."},
            {
              "type": "tool_use",
              "id": "toolu_1",
              "name": "act",
              "input": {"action": "list_files", "args": {}}
            }
          ]
        }"#;

        let parsed = serde_json::from_str::<AnthropicResponse>(raw).expect("valid response");
        assert_eq!(
            anthropic_text_chunks(&parsed.content),
            vec!["I will inspect files."]
        );

        let calls = anthropic_tool_calls(&parsed.content).expect("tool calls");
        assert_eq!(calls[0].call_id, "toolu_1");
        assert!(calls[0].arguments.contains("list_files"));
    }
}
