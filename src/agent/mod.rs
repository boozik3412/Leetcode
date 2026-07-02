pub mod openai;
pub mod types;

use crate::agent::openai::{OpenAiClient, ResponseInput};
use crate::agent::types::{AppEvent, ToolResult};
use crate::config::AppConfig;
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::ToolDispatcher;
use crate::workspace::Workspace;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct AgentState {
    pub previous_response_id: Option<String>,
}

impl AgentState {
    pub fn reset(&mut self) {
        self.previous_response_id = None;
    }
}

pub async fn run_user_turn(
    user_input: String,
    config: AppConfig,
    workspace: Option<Workspace>,
    state: Arc<Mutex<AgentState>>,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    if config.api_key.trim().is_empty() {
        anyhow::bail!("OPENAI_API_KEY is empty. Paste a key in the top bar or set the env var.");
    }

    let workspace_text = workspace
        .as_ref()
        .map(|workspace| workspace.root().display().to_string())
        .unwrap_or_else(|| "no workspace selected".to_string());
    let instructions = format!(
        "You are Leetcode, a concise local coding assistant running inside a Windows desktop app. \
Use the act tool whenever you need to inspect files, edit files, search code, or run shell commands. \
All file paths must be relative to the selected workspace. Current workspace root: {workspace_text}. \
Before writing code, inspect the relevant files. Prefer apply_patch for multi-line code edits, \
and use edit_file only for small unique string replacements. Keep user-facing explanations short and concrete."
    );

    let policy = PolicyConfig {
        require_shell_approval: config.require_shell_approval,
        require_write_approval: config.require_write_approval,
    };
    let dispatcher =
        ToolDispatcher::new(workspace, events.clone(), approvals, cancel.clone(), policy);
    let client = OpenAiClient::new(config.api_key.clone(), config.model.clone());

    let mut previous_response_id = state
        .lock()
        .expect("agent state poisoned")
        .previous_response_id
        .clone();
    let mut input = ResponseInput::Text(user_input);

    for _ in 0..24 {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("Run cancelled");
        }

        let streamed = client
            .stream_response(
                &instructions,
                input,
                previous_response_id.as_deref(),
                &events,
            )
            .await?;
        let response = streamed.response;
        previous_response_id = Some(response.id.clone());
        state
            .lock()
            .expect("agent state poisoned")
            .previous_response_id = Some(response.id.clone());

        if !streamed.emitted_text {
            for text in response.text_chunks() {
                let _ = events.send(AppEvent::AssistantText(text));
            }
        }

        let calls = response.tool_calls();
        if calls.is_empty() {
            return Ok(());
        }

        let mut tool_outputs = Vec::new();
        for call in calls {
            if cancel.load(Ordering::SeqCst) {
                anyhow::bail!("Run cancelled");
            }

            let result = dispatcher.execute(&call).await;
            tool_outputs.push(json!({
                "type": "function_call_output",
                "call_id": call.call_id,
                "output": result.as_model_output()
            }));
        }

        if tool_outputs.is_empty() {
            let result = ToolResult::error("No tool outputs were produced");
            input = ResponseInput::ToolOutputs(vec![json!({
                "type": "function_call_output",
                "call_id": "missing",
                "output": result.as_model_output()
            })]);
        } else {
            input = ResponseInput::ToolOutputs(tool_outputs);
        }
    }

    anyhow::bail!("Agent loop stopped after too many tool rounds");
}
