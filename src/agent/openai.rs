use crate::agent::models::OPENAI_PROVIDER_ID;
use crate::agent::provider::{ModelProvider, ProviderInput, ProviderTurn};
use crate::agent::types::{AppEvent, ToolCall};
use anyhow::Context;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub struct OpenAiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

pub enum ResponseInput {
    Text(String),
    ToolOutputs(Vec<Value>),
}

impl From<ProviderInput> for ResponseInput {
    fn from(input: ProviderInput) -> Self {
        match input {
            ProviderInput::Text(text) => Self::Text(text),
            ProviderInput::ToolOutputs(outputs) => Self::ToolOutputs(outputs),
        }
    }
}

impl OpenAiClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }

    #[allow(dead_code)]
    pub async fn create_response(
        &self,
        instructions: &str,
        input: ResponseInput,
        previous_response_id: Option<&str>,
    ) -> anyhow::Result<OpenAiResponse> {
        let input_value = match input {
            ResponseInput::Text(text) => Value::String(text),
            ResponseInput::ToolOutputs(outputs) => Value::Array(outputs),
        };

        let mut body = json!({
            "model": self.model,
            "instructions": instructions,
            "input": input_value,
            "tools": [act_tool_schema()],
            "parallel_tool_calls": false
        });

        if let Some(previous_response_id) = previous_response_id {
            body["previous_response_id"] = Value::String(previous_response_id.to_string());
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("OpenAI request failed")?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("OpenAI API error {status}: {text}");
        }

        serde_json::from_str::<OpenAiResponse>(&text).with_context(|| {
            format!(
                "Could not parse OpenAI response. First bytes: {}",
                text.chars().take(500).collect::<String>()
            )
        })
    }

    pub async fn stream_response(
        &self,
        instructions: &str,
        input: ResponseInput,
        previous_response_id: Option<&str>,
        events: &Sender<AppEvent>,
    ) -> anyhow::Result<StreamedOpenAiResponse> {
        let input_value = match input {
            ResponseInput::Text(text) => Value::String(text),
            ResponseInput::ToolOutputs(outputs) => Value::Array(outputs),
        };

        let mut body = json!({
            "model": self.model,
            "instructions": instructions,
            "input": input_value,
            "tools": [act_tool_schema()],
            "parallel_tool_calls": false,
            "stream": true
        });

        if let Some(previous_response_id) = previous_response_id {
            body["previous_response_id"] = Value::String(previous_response_id.to_string());
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("OpenAI streaming request failed")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {status}: {text}");
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut completed = None;
        let mut emitted_text = false;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("OpenAI streaming chunk failed")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(split_at) = find_sse_event_boundary(&buffer) {
                let raw_event = buffer[..split_at].to_string();
                let rest_start = if buffer[split_at..].starts_with("\r\n\r\n") {
                    split_at + 4
                } else {
                    split_at + 2
                };
                buffer = buffer[rest_start..].to_string();

                if let Some(response) = handle_sse_event(&raw_event, events, &mut emitted_text)? {
                    completed = Some(response);
                }
            }
        }

        if completed.is_none() && !buffer.trim().is_empty() {
            if let Some(response) = handle_sse_event(&buffer, events, &mut emitted_text)? {
                completed = Some(response);
            }
        }

        let Some(response) = completed else {
            anyhow::bail!("OpenAI stream ended without response.completed");
        };

        Ok(StreamedOpenAiResponse {
            response,
            emitted_text,
        })
    }
}

#[async_trait]
impl ModelProvider for OpenAiClient {
    fn id(&self) -> &'static str {
        OPENAI_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "OpenAI Responses"
    }

    async fn stream_turn(
        &self,
        instructions: &str,
        input: ProviderInput,
        previous_response_id: Option<&str>,
        events: &Sender<AppEvent>,
    ) -> anyhow::Result<ProviderTurn> {
        let streamed = self
            .stream_response(instructions, input.into(), previous_response_id, events)
            .await?;
        let response = streamed.response;

        Ok(ProviderTurn {
            response_id: response.id.clone(),
            text_chunks: response.text_chunks(),
            tool_calls: response.tool_calls(),
            emitted_text: streamed.emitted_text,
        })
    }
}

#[derive(Debug)]
pub struct StreamedOpenAiResponse {
    pub response: OpenAiResponse,
    pub emitted_text: bool,
}

pub fn act_tool_schema() -> Value {
    json!({
        "type": "function",
        "name": "act",
        "description": "Execute one local workspace, project, terminal, or desktop action. Paths must be relative to the selected workspace. Prefer project_command for check/test/run/build when a project profile is detected. run_shell uses PowerShell by default on Windows; pass shell=\"cmd\" only when needed.",
        "parameters": {
            "type": "object",
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list_files"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "depth": { "type": "integer", "minimum": 1, "maximum": 12 },
                                "limit": { "type": "integer", "minimum": 1, "maximum": 2000 }
                            },
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["read_file"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "offset": { "type": "integer", "minimum": 0 },
                                "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                            },
                            "required": ["path"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["write_file"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "content": { "type": "string" }
                            },
                            "required": ["path", "content"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["edit_file"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "old": { "type": "string" },
                                "new": { "type": "string" },
                                "all": { "type": "boolean" }
                            },
                            "required": ["path", "old", "new"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["apply_patch"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "patch": {
                                    "type": "string",
                                    "description": "Unified diff to apply from the workspace root."
                                }
                            },
                            "required": ["patch"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["grep"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "pattern": { "type": "string" },
                                "path": { "type": "string" },
                                "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                            },
                            "required": ["pattern"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["project_command"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "command": {
                                    "type": "string",
                                    "description": "Project command id or label, such as check, test, run, build, dev, preview, lint, editor, release."
                                },
                                "profile": {
                                    "type": "string",
                                    "description": "Optional project kind/name filter such as rust, node, python, godot, unity, unreal."
                                }
                            },
                            "required": ["command"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["run_shell"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "cmd": { "type": "string" },
                                "cwd": { "type": "string" },
                                "shell": { "type": "string", "enum": ["powershell", "cmd", "sh"] },
                                "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 1800 }
                            },
                            "required": ["cmd"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["generate_image_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "prompt": {
                                    "type": "string",
                                    "description": "Prompt for a game/app image asset."
                                },
                                "provider": {
                                    "type": "string",
                                    "enum": [
                                        "openai-image",
                                        "gemini-image",
                                        "stability-image",
                                        "replicate-image"
                                    ],
                                    "description": "Optional image provider. Defaults to the first configured image provider."
                                },
                                "model": {
                                    "type": "string",
                                    "description": "Optional provider model override."
                                },
                                "aspect_ratio": {
                                    "type": "string",
                                    "enum": ["1:1", "3:2", "2:3", "4:3", "3:4", "16:9", "9:16"]
                                },
                                "image_size": {
                                    "type": "string",
                                    "enum": ["0.5K", "1K", "2K", "4K"]
                                }
                            },
                            "required": ["prompt"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["regenerate_image_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "job_id": {
                                    "type": "string",
                                    "description": "Asset job id to regenerate."
                                }
                            },
                            "required": ["job_id"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["vary_image_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "job_id": {
                                    "type": "string",
                                    "description": "Asset job id to use as the base for a variation."
                                },
                                "prompt": {
                                    "type": "string",
                                    "description": "Optional custom variation prompt."
                                }
                            },
                            "required": ["job_id"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["use_asset_as_app_icon"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "source_path": {
                                    "type": "string",
                                    "description": "Relative path to an existing generated image asset."
                                },
                                "target_path": {
                                    "type": "string",
                                    "description": "Optional relative target path. Defaults to assets/app-icon.png."
                                }
                            },
                            "required": ["source_path"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["open_asset_folder"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "description": "Optional relative folder or asset path to open/reveal. Defaults to assets/generated/images."
                                }
                            },
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["screenshot"] },
                        "args": {
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["mouse_click"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "x": {
                                    "type": "integer",
                                    "description": "Absolute desktop x coordinate in pixels."
                                },
                                "y": {
                                    "type": "integer",
                                    "description": "Absolute desktop y coordinate in pixels."
                                },
                                "button": {
                                    "type": "string",
                                    "enum": ["left", "right", "middle"]
                                },
                                "clicks": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": 3
                                }
                            },
                            "required": ["x", "y"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["type_text"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "text": {
                                    "type": "string",
                                    "description": "Text to type into the active desktop window."
                                }
                            },
                            "required": ["text"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["hotkey"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "keys": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "minItems": 1,
                                    "maxItems": 6,
                                    "description": "Keys such as ctrl, shift, alt, win, enter, escape, tab, f5, a, 1, arrowleft."
                                }
                            },
                            "required": ["keys"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                }
            ]
        },
        "strict": false
    })
}

pub fn act_function_schema() -> Value {
    json!({
        "name": "act",
        "description": "Execute one local workspace, project, terminal, or desktop action. Paths must be relative to the selected workspace. Prefer project_command for check/test/run/build when a project profile is detected. run_shell uses PowerShell by default on Windows; pass shell=\"cmd\" only when needed.",
        "parameters": act_compatible_parameters_schema()
    })
}

pub fn chat_completion_act_tool_schema() -> Value {
    json!({
        "type": "function",
        "function": act_function_schema()
    })
}

pub fn anthropic_act_tool_schema() -> Value {
    json!({
        "name": "act",
        "description": "Execute one local workspace, project, terminal, or desktop action. Paths must be relative to the selected workspace. Prefer project_command for check/test/run/build when a project profile is detected. run_shell uses PowerShell by default on Windows; pass shell=\"cmd\" only when needed.",
        "input_schema": act_compatible_parameters_schema()
    })
}

pub fn gemini_act_function_declaration() -> Value {
    json!({
        "name": "act",
        "description": "Execute one local workspace, project, terminal, or desktop action. Paths must be relative to the selected workspace. Prefer project_command for check/test/run/build when a project profile is detected. run_shell uses PowerShell by default on Windows; pass shell=\"cmd\" only when needed.",
        "parameters": {
            "type": "OBJECT",
            "properties": {
                "action": {
                    "type": "STRING",
                    "enum": [
                        "list_files",
                        "read_file",
                        "write_file",
                        "edit_file",
                        "apply_patch",
                        "grep",
                        "project_command",
                        "run_shell",
                        "generate_image_asset",
                        "regenerate_image_asset",
                        "vary_image_asset",
                        "use_asset_as_app_icon",
                        "open_asset_folder",
                        "screenshot",
                        "mouse_click",
                        "type_text",
                        "hotkey"
                    ]
                },
                "args": {
                    "type": "OBJECT",
                    "description": "Arguments for the selected action. Use the action-specific fields expected by the local Leetcode tool dispatcher."
                }
            },
            "required": ["action", "args"]
        }
    })
}

fn act_compatible_parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": [
                    "list_files",
                    "read_file",
                    "write_file",
                    "edit_file",
                    "apply_patch",
                    "grep",
                    "project_command",
                    "run_shell",
                    "generate_image_asset",
                    "regenerate_image_asset",
                    "vary_image_asset",
                    "use_asset_as_app_icon",
                    "open_asset_folder",
                    "screenshot",
                    "mouse_click",
                    "type_text",
                    "hotkey"
                ]
            },
            "args": {
                "type": "object",
                "description": "Arguments for the selected action. Use the action-specific fields expected by the local Leetcode tool dispatcher."
            }
        },
        "required": ["action", "args"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponse {
    pub id: String,
    #[serde(default)]
    pub output: Vec<ResponseOutputItem>,
    #[serde(default)]
    pub output_text: Option<String>,
}

impl OpenAiResponse {
    pub fn text_chunks(&self) -> Vec<String> {
        let mut chunks = Vec::new();
        if let Some(text) = &self.output_text {
            if !text.trim().is_empty() {
                chunks.push(text.clone());
            }
        }

        for item in &self.output {
            if let ResponseOutputItem::Message { content, .. } = item {
                for content_item in content {
                    match content_item {
                        ResponseContentItem::OutputText { text }
                        | ResponseContentItem::Text { text } => {
                            if !text.trim().is_empty() && !chunks.iter().any(|known| known == text)
                            {
                                chunks.push(text.clone());
                            }
                        }
                        ResponseContentItem::Refusal { refusal } => {
                            if !refusal.trim().is_empty() {
                                chunks.push(refusal.clone());
                            }
                        }
                        ResponseContentItem::Other => {}
                    }
                }
            }
        }

        chunks
    }

    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.output
            .iter()
            .filter_map(|item| match item {
                ResponseOutputItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                    ..
                } => Some(ToolCall {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseOutputItem {
    #[serde(rename = "message")]
    Message {
        #[serde(default)]
        content: Vec<ResponseContentItem>,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseContentItem {
    #[serde(rename = "output_text")]
    OutputText { text: String },
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "refusal")]
    Refusal { refusal: String },
    #[serde(other)]
    Other,
}

fn find_sse_event_boundary(buffer: &str) -> Option<usize> {
    buffer.find("\r\n\r\n").or_else(|| buffer.find("\n\n"))
}

fn handle_sse_event(
    raw_event: &str,
    events: &Sender<AppEvent>,
    emitted_text: &mut bool,
) -> anyhow::Result<Option<OpenAiResponse>> {
    let Some(data) = extract_sse_data(raw_event) else {
        return Ok(None);
    };

    if data.trim() == "[DONE]" {
        return Ok(None);
    }

    let value: Value = serde_json::from_str(&data)
        .with_context(|| format!("Could not parse SSE event: {data}"))?;
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match event_type {
        "response.output_text.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                *emitted_text = true;
                let _ = events.send(AppEvent::AssistantDelta(delta.to_string()));
            }
            Ok(None)
        }
        "response.function_call_arguments.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                let _ = events.send(AppEvent::ToolOutput {
                    id: "model".to_string(),
                    chunk: format!("function args delta: {delta}"),
                });
            }
            Ok(None)
        }
        "response.completed" => {
            let Some(response_value) = value.get("response") else {
                anyhow::bail!("response.completed missing response payload");
            };
            Ok(Some(serde_json::from_value::<OpenAiResponse>(
                response_value.clone(),
            )?))
        }
        "error" => {
            anyhow::bail!("OpenAI stream error: {value}");
        }
        _ => Ok(None),
    }
}

fn extract_sse_data(raw_event: &str) -> Option<String> {
    let data = raw_event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");

    if data.is_empty() {
        None
    } else {
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn parses_output_text_delta_event() {
        let (tx, rx) = mpsc::channel();
        let mut emitted_text = false;

        let result = handle_sse_event(
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}",
            &tx,
            &mut emitted_text,
        )
        .unwrap();

        assert!(result.is_none());
        assert!(emitted_text);
        match rx.try_recv().unwrap() {
            AppEvent::AssistantDelta(delta) => assert_eq!(delta, "hi"),
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[test]
    fn finds_crlf_or_lf_sse_boundary() {
        assert_eq!(find_sse_event_boundary("a\r\n\r\nb"), Some(1));
        assert_eq!(find_sse_event_boundary("a\n\nb"), Some(1));
    }

    #[test]
    fn act_schemas_expose_asset_and_desktop_actions() {
        let openai_schema = act_tool_schema().to_string();
        let compatible_schema = act_compatible_parameters_schema().to_string();
        let gemini_schema = gemini_act_function_declaration().to_string();

        for schema in [openai_schema, compatible_schema, gemini_schema] {
            assert!(schema.contains("generate_image_asset"));
            assert!(schema.contains("project_command"));
            assert!(schema.contains("regenerate_image_asset"));
            assert!(schema.contains("vary_image_asset"));
            assert!(schema.contains("use_asset_as_app_icon"));
            assert!(schema.contains("open_asset_folder"));
            assert!(schema.contains("screenshot"));
            assert!(schema.contains("mouse_click"));
            assert!(schema.contains("type_text"));
            assert!(schema.contains("hotkey"));
        }
    }
}
