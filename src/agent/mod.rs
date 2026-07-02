pub mod anthropic;
pub mod deepseek;
pub mod gemini;
pub mod models;
pub mod openai;
pub mod provider;
pub mod types;

use crate::agent::provider::{build_provider, ProviderInput};
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
    pub provider_id: Option<String>,
    pub previous_response_id: Option<String>,
    pub provider_state: Option<serde_json::Value>,
}

impl AgentState {
    pub fn reset(&mut self) {
        self.provider_id = None;
        self.previous_response_id = None;
        self.provider_state = None;
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
    let policy = PolicyConfig {
        require_shell_approval: config.require_shell_approval,
        require_write_approval: config.require_write_approval,
    };
    let dispatcher = ToolDispatcher::new(
        workspace.clone(),
        config.clone(),
        events.clone(),
        approvals,
        cancel.clone(),
        policy,
    );
    let provider = build_provider(&config)?;
    let provider_id = config.provider_id().to_string();
    let (mut previous_response_id, provider_state) = {
        let state = state.lock().expect("agent state poisoned");
        if state.provider_id.as_deref() == Some(provider_id.as_str()) {
            (
                state.previous_response_id.clone(),
                state.provider_state.clone(),
            )
        } else {
            (None, None)
        }
    };
    provider.import_state(provider_state)?;

    let workspace_text = workspace
        .as_ref()
        .map(|workspace| workspace.root().display().to_string())
        .unwrap_or_else(|| "no workspace selected".to_string());
    let instructions = format!(
        "You are Leetcode, a concise local coding assistant running inside a Windows desktop app. \
Current model provider: {} ({}). \
Use the act tool whenever you need to inspect files, edit files, search code, run shell commands, capture a screenshot, control the desktop, or generate image assets. \
All file paths must be relative to the selected workspace. Current workspace root: {workspace_text}. \
Before writing code, inspect the relevant files. Prefer apply_patch for multi-line code edits, \
and use edit_file only for small unique string replacements. Use generate_image_asset for requested game/app visuals. For desktop work, call screenshot first, then use mouse_click, type_text, or hotkey only when coordinates or the active window are clear. Keep user-facing explanations short and concrete.",
        provider.display_name(),
        provider.id()
    );

    let mut input = ProviderInput::Text(user_input);

    for _ in 0..24 {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("Run cancelled");
        }

        let streamed = provider
            .stream_turn(
                &instructions,
                input,
                previous_response_id.as_deref(),
                &events,
            )
            .await?;
        previous_response_id = Some(streamed.response_id.clone());
        {
            let mut state = state.lock().expect("agent state poisoned");
            state.provider_id = Some(provider_id.clone());
            state.previous_response_id = Some(streamed.response_id.clone());
            state.provider_state = provider.export_state();
        }

        if !streamed.emitted_text {
            for text in streamed.text_chunks {
                let _ = events.send(AppEvent::AssistantText(text));
            }
        }

        let calls = streamed.tool_calls;
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
            input = ProviderInput::ToolOutputs(vec![json!({
                "type": "function_call_output",
                "call_id": "missing",
                "output": result.as_model_output()
            })]);
        } else {
            input = ProviderInput::ToolOutputs(tool_outputs);
        }
    }

    anyhow::bail!("Agent loop stopped after too many tool rounds");
}
