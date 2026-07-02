pub mod anthropic;
pub mod deepseek;
pub mod gemini;
pub mod models;
pub mod openai;
pub mod provider;
pub mod routing;
pub mod subagent;
pub mod types;

use crate::agent::provider::{build_routed_provider, ProviderInput};
use crate::agent::routing::{
    describe_route_plan, resolve_task_route, route_candidates, route_name,
};
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
    pub model_id: Option<String>,
    pub previous_response_id: Option<String>,
    pub provider_state: Option<serde_json::Value>,
}

impl AgentState {
    pub fn reset(&mut self) {
        self.provider_id = None;
        self.model_id = None;
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
    let task = resolve_task_route(&config, &user_input);
    let candidates = route_candidates(&config, task);
    if candidates.is_empty() {
        anyhow::bail!(
            "No available provider/model route for {}. Save a compatible API key or switch Route to Auto/Coding.",
            route_name(task)
        );
    }
    let _ = events.send(AppEvent::ToolOutput {
        id: "routing".to_string(),
        chunk: format!(
            "Task route: {}\n{}",
            route_name(task),
            describe_route_plan(&candidates)
        ),
    });
    let mut candidate_index = 0usize;
    let mut provider = build_routed_provider(&config, &candidates[candidate_index])?;
    let mut provider_id = candidates[candidate_index].provider_id.clone();
    let mut model_id = candidates[candidate_index].model_id.clone();
    let (mut previous_response_id, provider_state) = {
        let state = state.lock().expect("agent state poisoned");
        if state.provider_id.as_deref() == Some(provider_id.as_str())
            && state.model_id.as_deref() == Some(model_id.as_str())
        {
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
Use the act tool whenever you need to inspect files, edit files, search code, run project commands, run shell commands, capture a screenshot, control the desktop, or generate image assets. \
All file paths must be relative to the selected workspace. Current workspace root: {workspace_text}. \
Before writing code, inspect the relevant files. Prefer apply_patch for multi-line code edits, \
and use edit_file only for small unique string replacements. Prefer project_command for common project lifecycle tasks such as check, test, run, build, dev, preview, lint, editor, or release; use open_project_preview for local browser/app preview hooks; use run_shell for one-off custom commands. Use terminal_start, terminal_write, terminal_read, and terminal_stop for persistent interactive sessions such as dev servers, REPLs, watchers, game engine logs, or commands where cwd/env/session state should persist. Use game_workflow for game/app workflows such as prototype mechanic, spritesheet plan, UI sounds, item icons, vertical slice, or playtest checklist. For broad tasks that touch several domains, many files, validation plus implementation, or assets plus code, first propose a compact subagent plan to the user instead of immediately executing it: name the roles, their bounded tasks, expected benefit, and ask for confirmation. If the user already asked to proceed, approved subagents, or used phrases like use subagents/parallelize/split this up, call run_subagent directly. Use run_subagent when a bounded specialist can handle a small part of the work and return findings to you; choose code_agent, game_designer, art_director, audio_agent, qa_agent, or build_agent and keep the task focused. Use delegate_agent only when you want to record a handoff without executing it. Use update_workspace_context to preserve durable project facts and decisions; use record_run_summary at useful milestones; use orchestration_snapshot before planning across roles; use export_trace or create_replay_eval when the user asks for auditability or repeatable validation. Use generate_image_asset for requested game/app visuals, generate_spritesheet_asset for game animation sheets, generate_audio_asset for UI/game sounds or narration, and generate_video_asset for short clips. Use regenerate_image_asset or vary_image_asset for existing image jobs, upscale_asset/export_asset/attach_asset for asset pipeline follow-up, use_asset_as_app_icon to apply a generated icon asset, and open_asset_folder when the user wants to reveal generated assets. For desktop work, prefer active_window and desktop_step: observe first, then focus_window if needed, then perform one click/type_text/hotkey step with before and after screenshots. Use raw screenshot, mouse_click, type_text, or hotkey only for small direct actions when the active window and coordinates are already clear. Keep user-facing explanations short and concrete.",
        provider.display_name(),
        provider.id()
    );

    let mut input = ProviderInput::Text(user_input);

    for _ in 0..24 {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("Run cancelled");
        }

        let streamed = match provider
            .stream_turn(
                &instructions,
                input.clone(),
                previous_response_id.as_deref(),
                &events,
            )
            .await
        {
            Ok(streamed) => streamed,
            Err(err) if candidate_index + 1 < candidates.len() => {
                let failed = candidates[candidate_index].clone();
                candidate_index += 1;
                let next = candidates[candidate_index].clone();
                let _ = events.send(AppEvent::Error(format!(
                    "{} / {} failed, falling back to {} / {}: {}",
                    failed.provider_id, failed.model_id, next.provider_id, next.model_id, err
                )));
                provider = build_routed_provider(&config, &next)?;
                provider_id = next.provider_id;
                model_id = next.model_id;
                {
                    let mut state = state.lock().expect("agent state poisoned");
                    state.provider_id = None;
                    state.model_id = None;
                    state.previous_response_id = None;
                    state.provider_state = None;
                }
                provider
                    .stream_turn(&instructions, input.clone(), None, &events)
                    .await?
            }
            Err(err) => return Err(err),
        };
        previous_response_id = Some(streamed.response_id.clone());
        {
            let mut state = state.lock().expect("agent state poisoned");
            state.provider_id = Some(provider_id.clone());
            state.model_id = Some(model_id.clone());
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
