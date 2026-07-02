use crate::agent::provider::{build_routed_provider, ProviderInput};
use crate::agent::routing::{describe_route_plan, route_candidates, route_name, TaskRoute};
use crate::agent::types::{AppEvent, ToolCall, ToolResult};
use crate::config::AppConfig;
use crate::orchestration::{
    load_orchestration_state, record_subagent_run, AgentRole, SubagentRun, SubagentRunDraft,
};
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::ToolDispatcher;
use crate::workspace::Workspace;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SubagentRequest {
    pub role: AgentRole,
    pub task: String,
    pub context: String,
    pub max_rounds: usize,
}

pub async fn run_subagent(
    request: SubagentRequest,
    config: AppConfig,
    workspace: Workspace,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
) -> anyhow::Result<SubagentRun> {
    let max_rounds = request.max_rounds.clamp(1, 8);
    let route = route_for_role(request.role);
    let candidates = route_candidates(&config, route);
    if candidates.is_empty() {
        anyhow::bail!(
            "Нет доступного маршрута провайдер/модель для субагента {}.",
            route_name(route)
        );
    }

    let allowed_actions = allowed_actions_for_role(request.role);
    let allowed_actions_text = allowed_actions
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let orchestration_context = load_orchestration_state(&workspace).context;
    let workspace_root = workspace.root().display().to_string();
    let instructions = format!(
        "Ты ограниченный субагент-специалист Leetcode: {}. \
Тебя вызывает менеджер-агент как инструмент, не пользователь напрямую. \
Выполни только переданную задачу и верни менеджеру краткий результат. \
Разрешённые локальные действия: {allowed_actions_text}. Если нужно действие вне списка, объясни блокер вместо обходного пути. \
Никогда не вызывай run_subagent. Держи правки и shell-команды минимальными и релевантными. \
Корень рабочей папки: {workspace_root}. Общий summary рабочей папки: {}",
        role_label(request.role),
        if orchestration_context.summary.trim().is_empty() {
            "нет"
        } else {
            orchestration_context.summary.as_str()
        }
    );
    let mut input = ProviderInput::Text(format!(
        "Задача:\n{}\n\nКонтекст:\n{}\n\nФормат ответа:\nИтог:\nВыполненные действия:\nЗатронутые файлы/ассеты:\nРиски:\nРекомендуемый следующий шаг:",
        request.task, request.context
    ));

    let dispatcher = ToolDispatcher::new(
        Some(workspace.clone()),
        config.clone(),
        events.clone(),
        approvals,
        cancel.clone(),
        policy,
    );
    let mut final_text = String::new();
    let mut tool_calls = Vec::new();
    let mut denied_tools = Vec::new();
    let mut rounds_completed = 0usize;
    let mut candidate_index = 0usize;
    let mut provider = build_routed_provider(&config, &candidates[candidate_index])?;
    let mut previous_response_id: Option<String> = None;

    let _ = events.send(AppEvent::ToolOutput {
        id: subagent_event_id(request.role),
        chunk: format!(
            "Субагент {} запущен.\nМаршрут: {}\n{}",
            role_label(request.role),
            route_name(route),
            describe_route_plan(&candidates)
        ),
    });

    for round in 0..max_rounds {
        rounds_completed = round + 1;
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("Запуск субагента отменён");
        }

        let (model_events_tx, model_events_rx) = mpsc::channel();
        let streamed = match provider
            .stream_turn(
                &instructions,
                input.clone(),
                previous_response_id.as_deref(),
                &model_events_tx,
            )
            .await
        {
            Ok(streamed) => streamed,
            Err(_err) if candidate_index + 1 < candidates.len() => {
                candidate_index += 1;
                provider = build_routed_provider(&config, &candidates[candidate_index])?;
                let (retry_tx, retry_rx) = mpsc::channel();
                let streamed = provider
                    .stream_turn(&instructions, input.clone(), None, &retry_tx)
                    .await?;
                final_text.push_str(&collect_model_text(retry_rx));
                streamed
            }
            Err(err) => return Err(err),
        };

        previous_response_id = Some(streamed.response_id.clone());
        final_text.push_str(&collect_model_text(model_events_rx));
        if !streamed.emitted_text {
            for text in &streamed.text_chunks {
                append_text(&mut final_text, text);
            }
        }

        if streamed.tool_calls.is_empty() {
            return record_subagent_run(
                &workspace,
                SubagentRunDraft {
                    role: request.role,
                    task: request.task,
                    context: request.context,
                    status: "completed".to_string(),
                    output: final_text.trim().to_string(),
                    tool_calls,
                    denied_tools,
                    rounds: rounds_completed,
                },
            );
        }

        let mut tool_outputs = Vec::new();
        for call in streamed.tool_calls {
            if cancel.load(Ordering::SeqCst) {
                anyhow::bail!("Запуск субагента отменён");
            }

            let action = action_name(&call).unwrap_or_else(|| "неизвестно".to_string());
            if !allowed_actions.contains(action.as_str()) {
                let denied = format!("{} запрещён для {}", action, role_label(request.role));
                denied_tools.push(action.clone());
                let result = ToolResult::error(denied);
                tool_outputs.push(tool_output(call.call_id, result));
                continue;
            }

            tool_calls.push(action);
            let result = dispatcher.execute(&call).await;
            tool_outputs.push(tool_output(call.call_id, result));
        }

        input = ProviderInput::ToolOutputs(tool_outputs);
    }

    record_subagent_run(
        &workspace,
        SubagentRunDraft {
            role: request.role,
            task: request.task,
            context: request.context,
            status: "max_rounds_reached".to_string(),
            output: if final_text.trim().is_empty() {
                "Субагент достиг лимита раундов без финального ответа.".to_string()
            } else {
                final_text.trim().to_string()
            },
            tool_calls,
            denied_tools,
            rounds: rounds_completed,
        },
    )
}

pub fn allowed_actions_for_role(role: AgentRole) -> BTreeSet<&'static str> {
    let mut actions = BTreeSet::from([
        "list_files",
        "read_file",
        "grep",
        "orchestration_snapshot",
        "update_workspace_context",
        "record_run_summary",
    ]);

    match role {
        AgentRole::CodeAgent => {
            actions.extend([
                "write_file",
                "edit_file",
                "apply_patch",
                "project_command",
                "run_shell",
                "terminal_start",
                "terminal_write",
                "terminal_read",
                "terminal_stop",
            ]);
        }
        AgentRole::GameDesigner => {
            actions.extend(["game_workflow", "project_command"]);
        }
        AgentRole::ArtDirector => {
            actions.extend([
                "generate_image_asset",
                "generate_spritesheet_asset",
                "generate_video_asset",
                "regenerate_image_asset",
                "vary_image_asset",
                "upscale_asset",
                "export_asset",
                "attach_asset",
                "use_asset_as_app_icon",
                "open_asset_folder",
            ]);
        }
        AgentRole::AudioAgent => {
            actions.extend([
                "generate_audio_asset",
                "export_asset",
                "attach_asset",
                "open_asset_folder",
            ]);
        }
        AgentRole::QaAgent => {
            actions.extend([
                "project_command",
                "open_project_preview",
                "game_workflow",
                "screenshot",
                "active_window",
                "focus_window",
                "desktop_step",
                "terminal_read",
            ]);
        }
        AgentRole::BuildAgent => {
            actions.extend([
                "project_command",
                "run_shell",
                "terminal_start",
                "terminal_write",
                "terminal_read",
                "terminal_stop",
                "open_project_preview",
                "export_trace",
            ]);
        }
    }

    actions
}

fn route_for_role(role: AgentRole) -> TaskRoute {
    match role {
        AgentRole::CodeAgent | AgentRole::QaAgent | AgentRole::BuildAgent => TaskRoute::Coding,
        AgentRole::GameDesigner => TaskRoute::Planning,
        AgentRole::ArtDirector => TaskRoute::Image,
        AgentRole::AudioAgent => TaskRoute::Audio,
    }
}

fn role_label(role: AgentRole) -> &'static str {
    match role {
        AgentRole::CodeAgent => "Код-агент",
        AgentRole::GameDesigner => "Гейм-дизайнер",
        AgentRole::ArtDirector => "Арт-директор",
        AgentRole::AudioAgent => "Аудио-агент",
        AgentRole::QaAgent => "QA-агент",
        AgentRole::BuildAgent => "Build-агент",
    }
}

fn subagent_event_id(role: AgentRole) -> String {
    format!(
        "subagent-{}",
        role_label(role).replace(' ', "-").to_ascii_lowercase()
    )
}

fn action_name(call: &ToolCall) -> Option<String> {
    let value = serde_json::from_str::<Value>(&call.arguments).ok()?;
    value
        .get("action")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn tool_output(call_id: String, result: ToolResult) -> Value {
    json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": result.as_model_output()
    })
}

fn collect_model_text(rx: Receiver<AppEvent>) -> String {
    let mut text = String::new();
    while let Ok(event) = rx.try_recv() {
        match event {
            AppEvent::AssistantText(chunk) | AppEvent::AssistantDelta(chunk) => {
                append_text(&mut text, &chunk);
            }
            _ => {}
        }
    }
    text
}

fn append_text(target: &mut String, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if !target.is_empty() && !target.ends_with('\n') {
        target.push('\n');
    }
    target.push_str(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_allowlists_deny_recursive_subagents() {
        for role in [
            AgentRole::CodeAgent,
            AgentRole::GameDesigner,
            AgentRole::ArtDirector,
            AgentRole::AudioAgent,
            AgentRole::QaAgent,
            AgentRole::BuildAgent,
        ] {
            assert!(!allowed_actions_for_role(role).contains("run_subagent"));
        }
    }

    #[test]
    fn code_agent_can_patch_but_audio_agent_cannot() {
        assert!(allowed_actions_for_role(AgentRole::CodeAgent).contains("apply_patch"));
        assert!(!allowed_actions_for_role(AgentRole::AudioAgent).contains("apply_patch"));
    }
}
