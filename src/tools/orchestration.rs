use crate::agent::subagent::{run_subagent as run_subagent_loop, SubagentRequest};
use crate::agent::types::{AppEvent, ToolResult};
use crate::config::AppConfig;
use crate::orchestration::{
    create_replay_eval, export_trace, orchestration_snapshot, parse_agent_role, record_handoff,
    record_run_summary, update_shared_context, SharedWorkspaceContext,
};
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct DelegateAgentArgs {
    pub role: String,
    pub task: String,
    pub context: Option<String>,
    pub expected_output: Option<String>,
    pub from: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunSubagentArgs {
    pub role: String,
    pub task: String,
    pub context: Option<String>,
    pub max_rounds: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceContextArgs {
    pub summary: Option<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub important_files: Vec<String>,
    #[serde(default)]
    pub important_assets: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunSummaryArgs {
    pub title: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub completed: Vec<String>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReplayEvalArgs {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub expected_tools: Vec<String>,
    #[serde(default)]
    pub success_criteria: Vec<String>,
}

pub fn delegate_agent(
    workspace: &Workspace,
    args: DelegateAgentArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let Some(role) = parse_agent_role(&args.role) else {
        return ToolResult::error(format!("Неизвестная роль агента: {}", args.role));
    };
    if args.task.trim().is_empty() {
        return ToolResult::error("задача delegate_agent пустая");
    }
    if !request_approval_if(
        policy.require_orchestration_approval,
        events,
        approvals,
        format!("Записать передачу агенту {}", args.role),
        format!(
            "Задача:\n{}\n\nКонтекст:\n{}",
            args.task,
            args.context.as_deref().unwrap_or("")
        ),
    ) {
        return ToolResult::error("delegate_agent отклонён пользователем");
    }

    match record_handoff(
        workspace,
        role,
        args.from.unwrap_or_else(|| "Leetcode".to_string()),
        args.task,
        args.context.unwrap_or_default(),
        args.expected_output
            .unwrap_or_else(|| "Рекомендация специалиста и следующие действия".to_string()),
    ) {
        Ok(record) => ToolResult::ok(
            serde_json::to_string_pretty(&record)
                .unwrap_or_else(|_| "передача записана".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub async fn run_subagent(
    workspace: &Workspace,
    args: RunSubagentArgs,
    config: &AppConfig,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
) -> ToolResult {
    let Some(role) = parse_agent_role(&args.role) else {
        return ToolResult::error(format!("Неизвестная роль субагента: {}", args.role));
    };
    if args.task.trim().is_empty() {
        return ToolResult::error("задача run_subagent пустая");
    }

    let max_rounds = args.max_rounds.unwrap_or(4).clamp(1, 8);
    if !request_approval_if(
        policy.require_orchestration_approval,
        &events,
        &approvals,
        format!("Запустить {} субагента", args.role),
        format!(
            "Задача:\n{}\n\nКонтекст:\n{}\n\nМакс. раундов: {}",
            args.task,
            args.context.as_deref().unwrap_or(""),
            max_rounds
        ),
    ) {
        return ToolResult::error("run_subagent отклонён пользователем");
    }

    match run_subagent_loop(
        SubagentRequest {
            role,
            task: args.task,
            context: args.context.unwrap_or_default(),
            max_rounds,
        },
        config.clone(),
        workspace.clone(),
        events,
        approvals,
        cancel,
        policy,
    )
    .await
    {
        Ok(run) => ToolResult::ok(
            serde_json::to_string_pretty(&run)
                .unwrap_or_else(|_| "запуск субагента завершён".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn update_context(
    workspace: &Workspace,
    args: UpdateWorkspaceContextArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if !request_approval_if(
        policy.require_orchestration_approval,
        events,
        approvals,
        "Обновить общий контекст рабочей папки",
        serde_json::to_string_pretty(&json!({
            "summary": args.summary,
            "decisions": args.decisions,
            "open_questions": args.open_questions,
            "important_files": args.important_files,
            "important_assets": args.important_assets
        }))
        .unwrap_or_else(|_| "обновить общий контекст".to_string()),
    ) {
        return ToolResult::error("update_workspace_context отклонён пользователем");
    }

    match update_shared_context(
        workspace,
        SharedWorkspaceContext {
            summary: args.summary.unwrap_or_default(),
            decisions: args.decisions,
            open_questions: args.open_questions,
            important_files: args.important_files,
            important_assets: args.important_assets,
            updated_at: 0,
        },
    ) {
        Ok(context) => ToolResult::ok(
            serde_json::to_string_pretty(&context)
                .unwrap_or_else(|_| "контекст рабочей папки обновлён".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn record_summary(
    workspace: &Workspace,
    args: RunSummaryArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.summary.trim().is_empty() {
        return ToolResult::error("summary для record_run_summary пустой");
    }
    if !request_approval_if(
        policy.require_orchestration_approval,
        events,
        approvals,
        "Записать итог запуска",
        args.summary.clone(),
    ) {
        return ToolResult::error("record_run_summary отклонён пользователем");
    }

    match record_run_summary(
        workspace,
        args.title.unwrap_or_else(|| "Запуск агента".to_string()),
        args.summary,
        args.completed,
        args.next_steps,
        args.risks,
    ) {
        Ok(summary) => ToolResult::ok(
            serde_json::to_string_pretty(&summary)
                .unwrap_or_else(|_| "итог запуска записан".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn export_orchestration_trace(workspace: &Workspace) -> ToolResult {
    match export_trace(workspace) {
        Ok(path) => ToolResult::ok(
            serde_json::to_string_pretty(&json!({ "trace_path": path }))
                .unwrap_or_else(|_| "трасса экспортирована".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn create_eval(
    workspace: &Workspace,
    args: CreateReplayEvalArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.prompt.trim().is_empty() {
        return ToolResult::error("prompt для create_replay_eval пустой");
    }
    if !request_approval_if(
        policy.require_orchestration_approval,
        events,
        approvals,
        format!("Создать replay-проверку: {}", args.name),
        args.prompt.clone(),
    ) {
        return ToolResult::error("create_replay_eval отклонён пользователем");
    }

    match create_replay_eval(
        workspace,
        args.name,
        args.prompt,
        args.expected_tools,
        args.success_criteria,
    ) {
        Ok(eval) => ToolResult::ok(
            serde_json::to_string_pretty(&eval)
                .unwrap_or_else(|_| "replay-проверка создана".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&orchestration_snapshot(workspace))
            .unwrap_or_else(|_| "снимок оркестрации".to_string()),
    )
}
