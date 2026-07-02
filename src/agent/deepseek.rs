use crate::agent::models::DEEPSEEK_PROVIDER_ID;
use crate::agent::openai::chat_completion_act_tool_schema;
use crate::agent::provider::{normalized_tool_outputs, ModelProvider, ProviderInput, ProviderTurn};
use crate::agent::types::{AppEvent, ToolCall};
use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

pub struct DeepSeekClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
    messages: Mutex<Vec<Value>>,
}

impl DeepSeekClient {
    pub fn new(api_key: String, model: String, client: reqwest::Client) -> Self {
        Self {
            client,
            api_key,
            model,
            messages: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ModelProvider for DeepSeekClient {
    fn id(&self) -> &'static str {
        DEEPSEEK_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "DeepSeek Chat Completions"
    }

    fn import_state(&self, state: Option<Value>) -> anyhow::Result<()> {
        if let Some(Value::Array(messages)) = state {
            *self.messages.lock().expect("deepseek messages poisoned") = messages;
        }
        Ok(())
    }

    fn export_state(&self) -> Option<Value> {
        Some(Value::Array(
            self.messages
                .lock()
                .expect("deepseek messages poisoned")
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
            let mut messages = self.messages.lock().expect("deepseek messages poisoned");
            match input {
                ProviderInput::Text(text) => messages.push(json!({
                    "role": "user",
                    "content": text
                })),
                ProviderInput::ToolOutputs(outputs) => {
                    for (call_id, output) in normalized_tool_outputs(&outputs) {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": output
                        }));
                    }
                }
            }
        }

        let request_messages = {
            let messages = self.messages.lock().expect("deepseek messages poisoned");
            let mut request_messages = vec![json!({
                "role": "system",
                "content": instructions
            })];
            request_messages.extend(messages.iter().cloned());
            request_messages
        };

        let body = json!({
            "model": self.model,
            "messages": request_messages,
            "tools": [chat_completion_act_tool_schema()],
            "tool_choice": "auto",
            "stream": false
        });

        let response = self
            .client
            .post("https://api.deepseek.com/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("запрос DeepSeek не выполнен")?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("DeepSeek API error {status}: {text}");
        }

        let parsed = serde_json::from_str::<ChatCompletionResponse>(&text).with_context(|| {
            format!(
                "Could not parse DeepSeek response. First bytes: {}",
                text.chars().take(500).collect::<String>()
            )
        })?;
        let Some(choice) = parsed.choices.into_iter().next() else {
            anyhow::bail!("DeepSeek response did not include choices");
        };

        let message = choice.message;
        let text_chunks = message
            .content
            .clone()
            .filter(|content| !content.trim().is_empty())
            .into_iter()
            .collect::<Vec<_>>();
        let tool_calls = message
            .tool_calls
            .iter()
            .map(|call| ToolCall {
                call_id: call.id.clone(),
                name: call.function.name.clone(),
                arguments: if call.function.arguments.trim().is_empty() {
                    "{}".to_string()
                } else {
                    call.function.arguments.clone()
                },
            })
            .collect::<Vec<_>>();

        self.messages
            .lock()
            .expect("deepseek messages poisoned")
            .push(serde_json::to_value(&message)?);

        Ok(ProviderTurn {
            response_id: parsed.id,
            text_chunks,
            tool_calls,
            emitted_text: false,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChatMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChatToolCall {
    id: String,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    function: ChatFunctionCall,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChatFunctionCall {
    name: String,
    #[serde(default)]
    arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deepseek_tool_call() {
        let raw = r#"{
          "id": "abc",
          "choices": [{
            "message": {
              "role": "assistant",
              "content": null,
              "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                  "name": "act",
                  "arguments": "{\"action\":\"list_files\",\"args\":{}}"
                }
              }]
            }
          }]
        }"#;

        let parsed = serde_json::from_str::<ChatCompletionResponse>(raw).expect("valid response");
        let call = &parsed.choices[0].message.tool_calls[0];

        assert_eq!(call.id, "call_1");
        assert_eq!(call.function.name, "act");
        assert!(call.function.arguments.contains("list_files"));
    }
}
