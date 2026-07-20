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
    pub fn new(api_key: String, model: String, client: reqwest::Client) -> Self {
        Self {
            client,
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
            .context("запрос OpenAI не выполнен")?;

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
            .context("streaming-запрос OpenAI не выполнен")?;

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
            let chunk = chunk.context("streaming-чанк OpenAI не получен")?;
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
    let mut schema = json!({
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
                        "action": { "type": "string", "enum": ["unreal_snapshot"] },
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
                        "action": { "type": "string", "enum": ["unreal_command"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "command": {
                                    "type": "string",
                                    "enum": [
                                        "generate_project_files",
                                        "build_editor",
                                        "open_editor",
                                        "automation_tests",
                                        "cook",
                                        "package",
                                        "validate",
                                        "build_plugin"
                                    ]
                                },
                                "target": { "type": "string" },
                                "platform": { "type": "string" },
                                "configuration": { "type": "string" },
                                "test_filter": { "type": "string" },
                                "output_dir": { "type": "string" }
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
                        "action": { "type": "string", "enum": ["game_production_snapshot"] },
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
                        "action": { "type": "string", "enum": ["create_game_production_plan"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "brief": { "type": "string" },
                                "genre": { "type": "string" },
                                "target_platform": { "type": "string" },
                                "scope": {
                                    "type": "string",
                                    "enum": ["prototype", "vertical_slice", "full_game"]
                                },
                                "source_task_ids": { "type": "array", "items": { "type": "string" } },
                                "roadmap_ids": { "type": "array", "items": { "type": "string" } },
                                "project_node_id": { "type": "string" }
                            },
                            "required": ["title", "brief", "scope"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["update_production_item"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "plan_id": { "type": "string" },
                                "item_id": { "type": "string" },
                                "status": {
                                    "type": "string",
                                    "enum": ["planned", "ready", "in_progress", "blocked", "done"]
                                },
                                "artifact": { "type": "string", "description": "Optional existing workspace-relative artifact path." },
                                "validation": { "type": "string" }
                            },
                            "required": ["plan_id", "item_id", "status"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["evaluate_production_gate"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "plan_id": { "type": "string" },
                                "milestone": {
                                    "type": "string",
                                    "enum": ["prototype", "vertical_slice", "alpha", "beta", "release"]
                                }
                            },
                            "required": ["plan_id"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["vertical_slice_snapshot"] },
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
                        "action": { "type": "string", "enum": ["start_vertical_slice_run"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "production_plan_id": { "type": "string" },
                                "title": { "type": "string" }
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
                        "action": { "type": "string", "enum": ["advance_vertical_slice_phase"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "run_id": { "type": "string" },
                                "phase": {
                                    "type": "string",
                                    "enum": [
                                        "preflight",
                                        "gameplay_foundation",
                                        "visual_assets",
                                        "level_integration",
                                        "experience",
                                        "playtest",
                                        "production_gate"
                                    ]
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["in_progress", "blocked", "completed"]
                                },
                                "evidence": { "type": "string" },
                                "artifact": {
                                    "type": "string",
                                    "description": "Optional existing workspace-relative artifact path."
                                },
                                "notes": { "type": "string" }
                            },
                            "required": ["run_id", "phase", "status"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["evaluate_vertical_slice_readiness"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "run_id": { "type": "string" }
                            },
                            "required": ["run_id"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["gameplay_snapshot"] },
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
                        "action": { "type": "string", "enum": ["create_gameplay_plan"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "recipe": {
                                    "type": "string",
                                    "enum": [
                                        "level_bootstrap",
                                        "third_person_loop",
                                        "interaction",
                                        "pickup_and_inventory",
                                        "checkpoint",
                                        "enemy_encounter",
                                        "pcg_environment",
                                        "niagara_feedback",
                                        "enhanced_input",
                                        "game_hud"
                                    ]
                                },
                                "title": { "type": "string" },
                                "brief": { "type": "string" },
                                "map_path": { "type": "string" },
                                "task_ids": { "type": "array", "items": { "type": "string" } },
                                "roadmap_ids": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["recipe", "brief"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["apply_gameplay_plan"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "plan_id": { "type": "string" },
                                "map_path": { "type": "string" },
                                "create_map": { "type": "boolean" },
                                "save_level": { "type": "boolean" },
                                "operations": {
                                    "type": "array",
                                    "maxItems": 128,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "operation": {
                                                "type": "string",
                                                "enum": [
                                                    "load_level",
                                                    "create_level",
                                                    "spawn_actor",
                                                    "add_actor_component",
                                                    "delete_actor",
                                                    "set_actor_transform",
                                                    "set_actor_property",
                                                    "create_data_asset",
                                                    "save_level"
                                                ]
                                            },
                                            "actor_label": { "type": "string" },
                                            "component_name": { "type": "string" },
                                            "class_path": { "type": "string" },
                                            "asset_path": { "type": "string" },
                                            "package_path": { "type": "string" },
                                            "property": { "type": "string" },
                                            "value": {},
                                            "location": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
                                            "rotation": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
                                            "scale": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }
                                        },
                                        "required": ["operation"],
                                        "additionalProperties": false
                                    }
                                }
                            },
                            "required": ["map_path", "operations"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["run_gameplay_playtest"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "plan_id": { "type": "string" },
                                "mode": { "type": "string", "enum": ["automation", "map_smoke", "movie_render"] },
                                "map_path": { "type": "string" },
                                "test_filter": { "type": "string" },
                                "level_sequence": { "type": "string" },
                                "movie_pipeline_config": { "type": "string" },
                                "capture_screenshot": { "type": "boolean" },
                                "timeout_secs": { "type": "integer", "minimum": 30, "maximum": 1800 }
                            },
                            "required": ["mode"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["mcp_snapshot"] },
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
                        "action": { "type": "string", "enum": ["mcp_discover"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "server": { "type": "string" }
                            },
                            "required": ["server"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["mcp_call"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "server": { "type": "string" },
                                "tool": { "type": "string" },
                                "arguments": {
                                    "type": "object",
                                    "description": "JSON arguments accepted by the selected MCP tool."
                                },
                                "context_node_id": {
                                    "type": ["string", "null"],
                                    "description": "Optional Project Map node id. If omitted, Leetcode attaches the persisted selected node through MCP request _meta."
                                }
                            },
                            "required": ["server", "tool", "arguments"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["game_workflow"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "workflow": {
                                    "type": "string",
                                    "enum": [
                                        "prototype_mechanic",
                                        "generate_spritesheet",
                                        "generate_ui_sounds",
                                        "create_item_icons",
                                        "build_vertical_slice",
                                        "run_playtest_checklist"
                                    ]
                                },
                                "title": { "type": "string" },
                                "brief": { "type": "string" }
                            },
                            "required": ["workflow"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["open_project_preview"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "preview": {
                                    "type": "string",
                                    "description": "Preview hook id or label such as dev-server, preview-server, trunk-local, godot-editor."
                                },
                                "profile": {
                                    "type": "string",
                                    "description": "Optional project kind/name filter."
                                },
                                "url": {
                                    "type": "string",
                                    "description": "Optional explicit local URL to open."
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
                        "action": { "type": "string", "enum": ["run_subagent"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "role": {
                                    "type": "string",
                                    "enum": [
                                        "code_agent",
                                        "game_designer",
                                        "art_director",
                                        "audio_agent",
                                        "qa_agent",
                                        "build_agent"
                                    ],
                                    "description": "Specialist role to run as a bounded helper."
                                },
                                "task": {
                                    "type": "string",
                                    "description": "Focused task for the subagent."
                                },
                                "context": {
                                    "type": "string",
                                    "description": "Relevant local context, files, constraints, or expected output."
                                },
                                "max_rounds": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": 8,
                                    "description": "Maximum model/tool rounds for the subagent. Defaults to 4."
                                }
                            },
                            "required": ["role", "task"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["delegate_agent"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "role": {
                                    "type": "string",
                                    "enum": [
                                        "code_agent",
                                        "game_designer",
                                        "art_director",
                                        "audio_agent",
                                        "qa_agent",
                                        "build_agent"
                                    ],
                                    "description": "Specialist role to hand off work to."
                                },
                                "task": { "type": "string" },
                                "context": { "type": "string" },
                                "expected_output": { "type": "string" },
                                "from": { "type": "string" }
                            },
                            "required": ["role", "task"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["update_workspace_context"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "summary": { "type": "string" },
                                "decisions": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "open_questions": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "important_files": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "important_assets": {
                                    "type": "array",
                                    "items": { "type": "string" }
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
                        "action": { "type": "string", "enum": ["record_run_summary"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "summary": { "type": "string" },
                                "completed": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "next_steps": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "risks": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                }
                            },
                            "required": ["summary"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["export_trace"] },
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
                        "action": { "type": "string", "enum": ["create_replay_eval"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "prompt": { "type": "string" },
                                "expected_tools": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "success_criteria": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                }
                            },
                            "required": ["name", "prompt"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["orchestration_snapshot"] },
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
                        "action": { "type": "string", "enum": ["terminal_start"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "cwd": {
                                    "type": "string",
                                    "description": "Optional workspace-relative working directory. Defaults to the workspace root."
                                },
                                "shell": {
                                    "type": "string",
                                    "enum": ["powershell", "cmd"],
                                    "description": "Persistent shell to start. Defaults to powershell on Windows."
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
                        "action": { "type": "string", "enum": ["terminal_write"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "input": {
                                    "type": "string",
                                    "description": "Text or command to write to the persistent terminal."
                                },
                                "enter": {
                                    "type": "boolean",
                                    "description": "Append Enter after the input. Defaults to true."
                                }
                            },
                            "required": ["input"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["terminal_read"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "lines": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": 1000
                                },
                                "since_seq": {
                                    "type": "integer",
                                    "minimum": 0
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
                        "action": { "type": "string", "enum": ["terminal_stop"] },
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
                        "action": { "type": "string", "enum": ["terminal_clear"] },
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
                        "action": { "type": "string", "enum": ["generate_spritesheet_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "prompt": { "type": "string" },
                                "provider": { "type": "string" },
                                "model": { "type": "string" },
                                "aspect_ratio": { "type": "string" },
                                "image_size": { "type": "string" },
                                "columns": { "type": "integer", "minimum": 1, "maximum": 12 },
                                "rows": { "type": "integer", "minimum": 1, "maximum": 12 }
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
                        "action": { "type": "string", "enum": ["generate_audio_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "prompt": { "type": "string" },
                                "model": { "type": "string" },
                                "voice": { "type": "string" },
                                "format": { "type": "string", "enum": ["wav", "mp3", "opus"] }
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
                        "action": { "type": "string", "enum": ["generate_video_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "prompt": { "type": "string" },
                                "model": { "type": "string" },
                                "size": { "type": "string", "enum": ["1280x720", "720x1280", "1920x1080", "1080x1920"] },
                                "seconds": { "type": "integer", "minimum": 1, "maximum": 20 }
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
                                    "description": "ID задачи ассета для повторной генерации."
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
                        "action": { "type": "string", "enum": ["upscale_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "source_path": { "type": "string" },
                                "scale": { "type": "integer", "minimum": 2, "maximum": 4 }
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
                        "action": { "type": "string", "enum": ["export_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "source_path": { "type": "string" },
                                "target_name": { "type": "string" },
                                "target_dir": {
                                    "type": "string",
                                    "description": "Optional relative project folder for copying the asset, for example assets/images, public/assets/images, src/assets/audio, or Assets/Art."
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
                        "action": { "type": "string", "enum": ["attach_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "source_path": { "type": "string" }
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
                        "action": { "type": "string", "enum": ["vary_image_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "job_id": {
                                    "type": "string",
                                    "description": "ID задачи ассета, которую нужно взять за основу вариации."
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
                                    "description": "Необязательный относительный путь назначения. По умолчанию assets/app-icon.png."
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
                        "action": { "type": "string", "enum": ["asset_3d_snapshot"] },
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
                        "action": { "type": "string", "enum": ["submit_3d_asset"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "prompt": { "type": "string" },
                                "image_path": {
                                    "type": "string",
                                    "description": "Optional workspace-relative PNG/JPEG/WebP reference. When present, image-to-3D is used."
                                },
                                "provider": { "type": "string", "enum": ["meshy-3d", "tripo-3d"] },
                                "model": { "type": "string" },
                                "target_format": { "type": "string", "enum": ["glb", "gltf", "fbx", "usd"] },
                                "target_polycount": { "type": "integer", "minimum": 48, "maximum": 500000 },
                                "enable_pbr": { "type": "boolean" },
                                "pose_mode": { "type": "string", "enum": ["", "a-pose", "t-pose"] },
                                "license_confirmed": {
                                    "type": "boolean",
                                    "description": "True only when the user has confirmed the provider terms/license for project use."
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
                        "action": { "type": "string", "enum": ["refresh_3d_asset"] },
                        "args": {
                            "type": "object",
                            "properties": { "job_id": { "type": "string" } },
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
                        "action": { "type": "string", "enum": ["validate_3d_asset"] },
                        "args": {
                            "type": "object",
                            "properties": { "source_path": { "type": "string" } },
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
                        "action": { "type": "string", "enum": ["import_3d_asset_unreal"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "source_path": { "type": "string" },
                                "destination_path": { "type": "string", "description": "Unreal content path under /Game/." },
                                "asset_type": { "type": "string", "enum": ["static_mesh", "skeletal_mesh", "animation"] },
                                "skeleton_path": { "type": "string" },
                                "replace_existing": { "type": "boolean" },
                                "import_lods": { "type": "boolean" },
                                "enable_nanite": { "type": "boolean" },
                                "collision": { "type": "string", "enum": ["auto", "simple", "complex", "none"] },
                                "license_confirmed": { "type": "boolean" },
                                "allow_validation_warnings": { "type": "boolean" }
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
                        "action": {
                            "type": "string",
                            "enum": [
                                "production_validation_snapshot",
                                "update_project_map_golden",
                                "visual_regression_snapshot"
                            ]
                        },
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
                        "action": {
                            "type": "string",
                            "enum": ["record_visual_baseline", "compare_visual_snapshot"]
                        },
                        "args": {
                            "type": "object",
                            "properties": {
                                "scenario": {
                                    "type": "string",
                                    "enum": [
                                        "desktop_main",
                                        "desktop_context",
                                        "desktop_roadmap",
                                        "desktop_release",
                                        "remote_client",
                                        "remote_pwa"
                                    ]
                                },
                                "path": {
                                    "type": "string",
                                    "description": "Путь к PNG-снимку относительно выбранной рабочей папки."
                                }
                            },
                            "required": ["scenario", "path"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["action", "args"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": [
                                "governance_snapshot",
                                "set_tool_enabled",
                                "set_category_enabled",
                                "add_shell_deny_pattern",
                                "memory_snapshot",
                                "upsert_task",
                                "update_task_status",
                                "record_decision",
                                "record_project_goal",
                                "record_memory_source",
                                "remove_memory_source",
                                "project_graph_snapshot",
                                "roadmap_snapshot",
                                "record_milestone",
                                "update_roadmap_item",
                                "plan_roadmap_item",
                                "export_roadmap",
                                "asset_library_snapshot",
                                "tag_asset",
                                "favorite_asset",
                                "export_asset_pack",
                                "run_replay_eval",
                                "eval_snapshot",
                                "self_improvement_snapshot",
                                "start_self_improvement_experiment",
                                "decide_self_improvement_experiment",
                                "prepare_self_improvement_worktree",
                                "apply_self_improvement_patch",
                                "register_self_improvement_benchmark",
                                "run_self_improvement_benchmarks",
                                "promote_self_improvement_experiment",
                                "rollback_self_improvement_experiment",
                                "cleanup_self_improvement_experiment",
                                "provider_health_snapshot",
                                "environment_snapshot"
                            ]
                        },
                        "args": {
                            "type": "object",
                            "description": "Аргументы для действий доступа, памяти, библиотеки ассетов, проверок и статуса провайдеров.",
                            "additionalProperties": true
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
                        "action": { "type": "string", "enum": ["active_window"] },
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
                        "action": { "type": "string", "enum": ["focus_window"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "title": {
                                    "type": "string",
                                    "description": "Visible window title or substring to focus."
                                },
                                "process": {
                                    "type": "string",
                                    "description": "Process name or substring to focus."
                                },
                                "exact": {
                                    "type": "boolean",
                                    "description": "Use exact title/process matching instead of substring matching."
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
                        "action": { "type": "string", "enum": ["desktop_step"] },
                        "args": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": ["observe", "click", "type_text", "hotkey", "focus_window"]
                                },
                                "x": {
                                    "type": "integer",
                                    "description": "Absolute desktop x coordinate for click."
                                },
                                "y": {
                                    "type": "integer",
                                    "description": "Absolute desktop y coordinate for click."
                                },
                                "button": {
                                    "type": "string",
                                    "enum": ["left", "right", "middle"]
                                },
                                "clicks": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": 3
                                },
                                "text": {
                                    "type": "string",
                                    "description": "Text for type_text desktop step."
                                },
                                "keys": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "minItems": 1,
                                    "maxItems": 6
                                },
                                "title": {
                                    "type": "string",
                                    "description": "Window title or substring for focus_window desktop step."
                                },
                                "process": {
                                    "type": "string",
                                    "description": "Process name or substring for focus_window desktop step."
                                },
                                "exact": {
                                    "type": "boolean"
                                },
                                "note": {
                                    "type": "string",
                                    "description": "Short reason for the step."
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
    });
    if let Some(one_of) = schema["parameters"]["oneOf"].as_array_mut() {
        one_of.extend(game_task_builder_schema_variants());
    }
    schema
}

fn game_task_builder_schema_variants() -> Vec<Value> {
    vec![
        strict_act_variant(
            "project_map_readiness",
            vec![("refresh_if_stale", json!({ "type": "boolean" }))],
            &[],
        ),
        strict_act_variant(
            "refresh_project_map_deep",
            vec![("run_unreal_scan", json!({ "type": "boolean" }))],
            &[],
        ),
        strict_act_variant(
            "game_task_catalog_snapshot",
            vec![
                ("domain_id", json!({ "type": "string" })),
                ("direction_id", json!({ "type": "string" })),
            ],
            &[],
        ),
        strict_act_variant(
            "resolve_game_task_targets",
            vec![
                ("operation_id", json!({ "type": "string" })),
                ("query", json!({ "type": "string" })),
                (
                    "limit",
                    json!({ "type": "integer", "minimum": 1, "maximum": 100 }),
                ),
            ],
            &["operation_id"],
        ),
        strict_act_variant(
            "evaluate_game_task_prerequisites",
            vec![
                ("operation_id", json!({ "type": "string" })),
                (
                    "target_node_ids",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
            ],
            &["operation_id", "target_node_ids"],
        ),
        strict_act_variant(
            "prepare_game_task_proposal",
            vec![
                ("operation_id", json!({ "type": "string" })),
                (
                    "target_node_ids",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
                (
                    "remediation_ids",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
                ("custom_request", json!({ "type": "string" })),
            ],
            &["operation_id", "target_node_ids", "remediation_ids"],
        ),
        strict_act_variant(
            "propose_project_relation",
            vec![
                ("from_node_id", json!({ "type": "string" })),
                ("to_node_id", json!({ "type": "string" })),
                (
                    "kind",
                    json!({ "type": "string", "enum": ["uses_skeleton", "animates", "controlled_by", "has_component", "compatible_with", "spawned_by", "owned_by", "bound_to_input", "produces", "consumes"] }),
                ),
                ("reason", json!({ "type": "string" })),
            ],
            &["from_node_id", "to_node_id", "kind", "reason"],
        ),
        strict_act_variant("game_task_snapshot", Vec::new(), &[]),
        strict_act_variant(
            "semantic_catalog_snapshot",
            vec![(
                "group",
                json!({ "type": "string", "enum": ["domain", "system", "entity", "role", "capability", "importance", "state", "scope"] }),
            )],
            &[],
        ),
        strict_act_variant(
            "analyze_project_semantics",
            vec![("force", json!({ "type": "boolean" }))],
            &[],
        ),
        strict_act_variant(
            "semantic_node_snapshot",
            vec![("node_id", json!({ "type": "string" }))],
            &["node_id"],
        ),
        strict_act_variant(
            "resolve_semantic_targets",
            vec![
                ("operation_id", json!({ "type": "string" })),
                ("query", json!({ "type": "string" })),
                (
                    "limit",
                    json!({ "type": "integer", "minimum": 1, "maximum": 100 }),
                ),
            ],
            &["operation_id"],
        ),
        strict_act_variant(
            "propose_semantic_labels",
            vec![
                ("node_id", json!({ "type": "string" })),
                (
                    "tag_ids",
                    json!({ "type": "array", "items": { "type": "string" }, "minItems": 1 }),
                ),
                ("reason", json!({ "type": "string" })),
                (
                    "confidence",
                    json!({ "type": "number", "minimum": 0, "maximum": 1 }),
                ),
            ],
            &["node_id", "tag_ids", "reason"],
        ),
        strict_act_variant(
            "decide_semantic_proposals",
            vec![
                (
                    "proposal_ids",
                    json!({ "type": "array", "items": { "type": "string" }, "minItems": 1 }),
                ),
                ("accept", json!({ "type": "boolean" })),
            ],
            &["proposal_ids", "accept"],
        ),
        strict_act_variant(
            "update_semantic_labels",
            vec![
                ("node_id", json!({ "type": "string" })),
                (
                    "add_tag_ids",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
                (
                    "remove_tag_ids",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
            ],
            &["node_id"],
        ),
        strict_act_variant("export_semantic_index", Vec::new(), &[]),
    ]
}

fn strict_act_variant(action: &str, properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let properties = properties
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect::<serde_json::Map<String, Value>>();
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": [action] },
            "args": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            }
        },
        "required": ["action", "args"],
        "additionalProperties": false
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
                        "unreal_snapshot",
                        "unreal_command",
                        "game_production_snapshot",
                        "create_game_production_plan",
                        "update_production_item",
                        "evaluate_production_gate",
                        "vertical_slice_snapshot",
                        "start_vertical_slice_run",
                        "advance_vertical_slice_phase",
                        "evaluate_vertical_slice_readiness",
                        "gameplay_snapshot",
                        "create_gameplay_plan",
                        "apply_gameplay_plan",
                        "run_gameplay_playtest",
                        "mcp_snapshot",
                        "mcp_discover",
                        "mcp_call",
                        "game_workflow",
                        "open_project_preview",
                        "run_subagent",
                        "delegate_agent",
                        "update_workspace_context",
                        "record_run_summary",
                        "export_trace",
                        "create_replay_eval",
                        "orchestration_snapshot",
                        "run_shell",
                        "terminal_start",
                        "terminal_write",
                        "terminal_read",
                        "terminal_stop",
                        "terminal_clear",
                        "generate_image_asset",
                        "generate_spritesheet_asset",
                        "generate_audio_asset",
                        "generate_video_asset",
                        "asset_3d_snapshot",
                        "submit_3d_asset",
                        "refresh_3d_asset",
                        "validate_3d_asset",
                        "import_3d_asset_unreal",
                        "regenerate_image_asset",
                        "vary_image_asset",
                        "upscale_asset",
                        "export_asset",
                        "attach_asset",
                        "use_asset_as_app_icon",
                        "open_asset_folder",
                        "governance_snapshot",
                        "set_tool_enabled",
                        "set_category_enabled",
                        "add_shell_deny_pattern",
                        "memory_snapshot",
                        "upsert_task",
                        "update_task_status",
                        "record_decision",
                        "record_project_goal",
                        "record_memory_source",
                        "remove_memory_source",
                        "project_graph_snapshot",
                        "project_map_readiness",
                        "refresh_project_map_deep",
                        "game_task_catalog_snapshot",
                        "resolve_game_task_targets",
                        "evaluate_game_task_prerequisites",
                        "prepare_game_task_proposal",
                        "propose_project_relation",
                        "game_task_snapshot",
                        "semantic_catalog_snapshot",
                        "analyze_project_semantics",
                        "semantic_node_snapshot",
                        "resolve_semantic_targets",
                        "propose_semantic_labels",
                        "decide_semantic_proposals",
                        "update_semantic_labels",
                        "export_semantic_index",
                        "roadmap_snapshot",
                        "record_milestone",
                        "update_roadmap_item",
                        "plan_roadmap_item",
                        "export_roadmap",
                        "asset_library_snapshot",
                        "tag_asset",
                        "favorite_asset",
                        "export_asset_pack",
                        "run_replay_eval",
                        "eval_snapshot",
                        "self_improvement_snapshot",
                        "start_self_improvement_experiment",
                        "decide_self_improvement_experiment",
                        "prepare_self_improvement_worktree",
                        "apply_self_improvement_patch",
                        "register_self_improvement_benchmark",
                        "run_self_improvement_benchmarks",
                        "promote_self_improvement_experiment",
                        "rollback_self_improvement_experiment",
                        "cleanup_self_improvement_experiment",
                        "provider_health_snapshot",
                        "environment_snapshot",
                        "production_validation_snapshot",
                        "update_project_map_golden",
                        "visual_regression_snapshot",
                        "record_visual_baseline",
                        "compare_visual_snapshot",
                        "screenshot",
                        "active_window",
                        "focus_window",
                        "desktop_step",
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
                    "unreal_snapshot",
                    "unreal_command",
                    "game_production_snapshot",
                    "create_game_production_plan",
                    "update_production_item",
                    "evaluate_production_gate",
                    "vertical_slice_snapshot",
                    "start_vertical_slice_run",
                    "advance_vertical_slice_phase",
                    "evaluate_vertical_slice_readiness",
                    "gameplay_snapshot",
                    "create_gameplay_plan",
                    "apply_gameplay_plan",
                    "run_gameplay_playtest",
                    "mcp_snapshot",
                    "mcp_discover",
                    "mcp_call",
                    "game_workflow",
                    "open_project_preview",
                    "run_subagent",
                    "delegate_agent",
                    "update_workspace_context",
                    "record_run_summary",
                    "export_trace",
                    "create_replay_eval",
                    "orchestration_snapshot",
                    "run_shell",
                    "terminal_start",
                    "terminal_write",
                    "terminal_read",
                    "terminal_stop",
                    "terminal_clear",
                    "generate_image_asset",
                    "generate_spritesheet_asset",
                    "generate_audio_asset",
                    "generate_video_asset",
                    "asset_3d_snapshot",
                    "submit_3d_asset",
                    "refresh_3d_asset",
                    "validate_3d_asset",
                    "import_3d_asset_unreal",
                    "regenerate_image_asset",
                    "vary_image_asset",
                    "upscale_asset",
                    "export_asset",
                    "attach_asset",
                    "use_asset_as_app_icon",
                    "open_asset_folder",
                    "governance_snapshot",
                    "set_tool_enabled",
                    "set_category_enabled",
                    "add_shell_deny_pattern",
                    "memory_snapshot",
                    "upsert_task",
                    "update_task_status",
                    "record_decision",
                    "record_project_goal",
                    "record_memory_source",
                    "remove_memory_source",
                    "project_graph_snapshot",
                    "project_map_readiness",
                    "refresh_project_map_deep",
                    "game_task_catalog_snapshot",
                    "resolve_game_task_targets",
                    "evaluate_game_task_prerequisites",
                    "prepare_game_task_proposal",
                    "propose_project_relation",
                    "game_task_snapshot",
                    "semantic_catalog_snapshot",
                    "analyze_project_semantics",
                    "semantic_node_snapshot",
                    "resolve_semantic_targets",
                    "propose_semantic_labels",
                    "decide_semantic_proposals",
                    "update_semantic_labels",
                    "export_semantic_index",
                    "roadmap_snapshot",
                    "record_milestone",
                    "update_roadmap_item",
                    "plan_roadmap_item",
                    "export_roadmap",
                    "asset_library_snapshot",
                    "tag_asset",
                    "favorite_asset",
                    "export_asset_pack",
                    "run_replay_eval",
                    "eval_snapshot",
                    "self_improvement_snapshot",
                    "start_self_improvement_experiment",
                    "decide_self_improvement_experiment",
                    "prepare_self_improvement_worktree",
                    "apply_self_improvement_patch",
                    "register_self_improvement_benchmark",
                    "run_self_improvement_benchmarks",
                    "promote_self_improvement_experiment",
                    "rollback_self_improvement_experiment",
                    "cleanup_self_improvement_experiment",
                    "provider_health_snapshot",
                    "environment_snapshot",
                    "production_validation_snapshot",
                    "update_project_map_golden",
                    "visual_regression_snapshot",
                    "record_visual_baseline",
                    "compare_visual_snapshot",
                    "screenshot",
                    "active_window",
                    "focus_window",
                    "desktop_step",
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
            assert!(schema.contains("game_workflow"));
            assert!(schema.contains("open_project_preview"));
            assert!(schema.contains("run_subagent"));
            assert!(schema.contains("delegate_agent"));
            assert!(schema.contains("update_workspace_context"));
            assert!(schema.contains("record_run_summary"));
            assert!(schema.contains("export_trace"));
            assert!(schema.contains("create_replay_eval"));
            assert!(schema.contains("orchestration_snapshot"));
            assert!(schema.contains("terminal_start"));
            assert!(schema.contains("terminal_write"));
            assert!(schema.contains("terminal_read"));
            assert!(schema.contains("terminal_stop"));
            assert!(schema.contains("terminal_clear"));
            assert!(schema.contains("generate_spritesheet_asset"));
            assert!(schema.contains("generate_audio_asset"));
            assert!(schema.contains("generate_video_asset"));
            assert!(schema.contains("upscale_asset"));
            assert!(schema.contains("export_asset"));
            assert!(schema.contains("attach_asset"));
            assert!(schema.contains("project_command"));
            assert!(schema.contains("unreal_snapshot"));
            assert!(schema.contains("unreal_command"));
            assert!(schema.contains("game_production_snapshot"));
            assert!(schema.contains("create_game_production_plan"));
            assert!(schema.contains("update_production_item"));
            assert!(schema.contains("evaluate_production_gate"));
            assert!(schema.contains("vertical_slice_snapshot"));
            assert!(schema.contains("start_vertical_slice_run"));
            assert!(schema.contains("advance_vertical_slice_phase"));
            assert!(schema.contains("evaluate_vertical_slice_readiness"));
            assert!(schema.contains("gameplay_snapshot"));
            assert!(schema.contains("create_gameplay_plan"));
            assert!(schema.contains("apply_gameplay_plan"));
            assert!(schema.contains("run_gameplay_playtest"));
            assert!(schema.contains("mcp_snapshot"));
            assert!(schema.contains("mcp_discover"));
            assert!(schema.contains("mcp_call"));
            assert!(schema.contains("regenerate_image_asset"));
            assert!(schema.contains("vary_image_asset"));
            assert!(schema.contains("use_asset_as_app_icon"));
            assert!(schema.contains("open_asset_folder"));
            assert!(schema.contains("asset_3d_snapshot"));
            assert!(schema.contains("submit_3d_asset"));
            assert!(schema.contains("refresh_3d_asset"));
            assert!(schema.contains("validate_3d_asset"));
            assert!(schema.contains("import_3d_asset_unreal"));
            assert!(schema.contains("governance_snapshot"));
            assert!(schema.contains("set_tool_enabled"));
            assert!(schema.contains("set_category_enabled"));
            assert!(schema.contains("add_shell_deny_pattern"));
            assert!(schema.contains("memory_snapshot"));
            assert!(schema.contains("upsert_task"));
            assert!(schema.contains("update_task_status"));
            assert!(schema.contains("record_decision"));
            assert!(schema.contains("record_project_goal"));
            assert!(schema.contains("record_memory_source"));
            assert!(schema.contains("remove_memory_source"));
            assert!(schema.contains("project_graph_snapshot"));
            assert!(schema.contains("project_map_readiness"));
            assert!(schema.contains("refresh_project_map_deep"));
            assert!(schema.contains("game_task_catalog_snapshot"));
            assert!(schema.contains("resolve_game_task_targets"));
            assert!(schema.contains("evaluate_game_task_prerequisites"));
            assert!(schema.contains("prepare_game_task_proposal"));
            assert!(schema.contains("propose_project_relation"));
            assert!(schema.contains("game_task_snapshot"));
            assert!(schema.contains("roadmap_snapshot"));
            assert!(schema.contains("record_milestone"));
            assert!(schema.contains("update_roadmap_item"));
            assert!(schema.contains("plan_roadmap_item"));
            assert!(schema.contains("export_roadmap"));
            assert!(schema.contains("asset_library_snapshot"));
            assert!(schema.contains("tag_asset"));
            assert!(schema.contains("favorite_asset"));
            assert!(schema.contains("export_asset_pack"));
            assert!(schema.contains("run_replay_eval"));
            assert!(schema.contains("eval_snapshot"));
            assert!(schema.contains("self_improvement_snapshot"));
            assert!(schema.contains("start_self_improvement_experiment"));
            assert!(schema.contains("decide_self_improvement_experiment"));
            assert!(schema.contains("prepare_self_improvement_worktree"));
            assert!(schema.contains("apply_self_improvement_patch"));
            assert!(schema.contains("register_self_improvement_benchmark"));
            assert!(schema.contains("run_self_improvement_benchmarks"));
            assert!(schema.contains("promote_self_improvement_experiment"));
            assert!(schema.contains("rollback_self_improvement_experiment"));
            assert!(schema.contains("cleanup_self_improvement_experiment"));
            assert!(schema.contains("provider_health_snapshot"));
            assert!(schema.contains("environment_snapshot"));
            assert!(schema.contains("production_validation_snapshot"));
            assert!(schema.contains("update_project_map_golden"));
            assert!(schema.contains("visual_regression_snapshot"));
            assert!(schema.contains("record_visual_baseline"));
            assert!(schema.contains("compare_visual_snapshot"));
            assert!(schema.contains("screenshot"));
            assert!(schema.contains("active_window"));
            assert!(schema.contains("focus_window"));
            assert!(schema.contains("desktop_step"));
            assert!(schema.contains("mouse_click"));
            assert!(schema.contains("type_text"));
            assert!(schema.contains("hotkey"));
        }
    }
}
