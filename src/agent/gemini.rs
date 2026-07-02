use crate::agent::models::GEMINI_PROVIDER_ID;
use crate::agent::openai::gemini_act_function_declaration;
use crate::agent::provider::{normalized_tool_outputs, ModelProvider, ProviderInput, ProviderTurn};
use crate::agent::types::{AppEvent, ToolCall};
use anyhow::Context;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

pub struct GeminiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
    contents: Mutex<Vec<Value>>,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String, client: reqwest::Client) -> Self {
        Self {
            client,
            api_key,
            model,
            contents: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ModelProvider for GeminiClient {
    fn id(&self) -> &'static str {
        GEMINI_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Gemini Generate Content"
    }

    fn import_state(&self, state: Option<Value>) -> anyhow::Result<()> {
        if let Some(Value::Array(contents)) = state {
            *self.contents.lock().expect("gemini contents poisoned") = contents;
        }
        Ok(())
    }

    fn export_state(&self) -> Option<Value> {
        Some(Value::Array(
            self.contents
                .lock()
                .expect("gemini contents poisoned")
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
            let mut contents = self.contents.lock().expect("gemini contents poisoned");
            match input {
                ProviderInput::Text(text) => contents.push(json!({
                    "role": "user",
                    "parts": [{ "text": text }]
                })),
                ProviderInput::ToolOutputs(outputs) => {
                    let parts = normalized_tool_outputs(&outputs)
                        .into_iter()
                        .map(|(call_id, output)| {
                            json!({
                                "functionResponse": {
                                    "name": "act",
                                    "response": {
                                        "call_id": call_id,
                                        "output": output
                                    }
                                }
                            })
                        })
                        .collect::<Vec<_>>();
                    contents.push(json!({
                        "role": "user",
                        "parts": parts
                    }));
                }
            }
        }

        let contents = self
            .contents
            .lock()
            .expect("gemini contents poisoned")
            .clone();
        let body = json!({
            "systemInstruction": {
                "parts": [{ "text": instructions }]
            },
            "contents": contents,
            "tools": [{
                "functionDeclarations": [gemini_act_function_declaration()]
            }],
            "toolConfig": {
                "functionCallingConfig": {
                    "mode": "AUTO"
                }
            }
        });

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );
        let response = self
            .client
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .context("запрос Gemini не выполнен")?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Gemini API error {status}: {text}");
        }

        let parsed = serde_json::from_str::<Value>(&text).with_context(|| {
            format!(
                "Could not parse Gemini response. First bytes: {}",
                text.chars().take(500).collect::<String>()
            )
        })?;
        let response_id = parsed
            .get("responseId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("gemini-{}", uuid::Uuid::new_v4()));
        let content = parsed
            .pointer("/candidates/0/content")
            .cloned()
            .unwrap_or_else(|| json!({ "role": "model", "parts": [] }));
        let parts = content
            .get("parts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let text_chunks = gemini_text_chunks(&parts);
        let tool_calls = gemini_tool_calls(&response_id, &parts)?;

        self.contents
            .lock()
            .expect("gemini contents poisoned")
            .push(content);

        Ok(ProviderTurn {
            response_id,
            text_chunks,
            tool_calls,
            emitted_text: false,
        })
    }
}

fn gemini_text_chunks(parts: &[Value]) -> Vec<String> {
    parts
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

fn gemini_tool_calls(response_id: &str, parts: &[Value]) -> anyhow::Result<Vec<ToolCall>> {
    let mut calls = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        let Some(call) = part.get("functionCall") else {
            continue;
        };
        let name = call
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("act")
            .to_string();
        let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
        let call_id = call
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("{response_id}-{idx}"));

        calls.push(ToolCall {
            call_id,
            name,
            arguments: serde_json::to_string(&args)?,
        });
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gemini_text_and_function_call() {
        let parts = vec![
            json!({ "text": "I will inspect files." }),
            json!({
                "functionCall": {
                    "name": "act",
                    "args": { "action": "list_files", "args": {} }
                }
            }),
        ];

        assert_eq!(gemini_text_chunks(&parts), vec!["I will inspect files."]);

        let calls = gemini_tool_calls("response", &parts).expect("tool calls");
        assert_eq!(calls[0].call_id, "response-1");
        assert!(calls[0].arguments.contains("list_files"));
    }
}
