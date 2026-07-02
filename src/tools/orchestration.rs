use crate::agent::subagent::{run_subagent as run_subagent_loop, SubagentRequest};
use crate::agent::types::{AppEvent, ToolResult};
use crate::config::AppConfig;
use crate::orchestration::{
    create_replay_eval, export_trace, orchestration_snapshot, parse_agent_role, record_handoff,
    record_run_summary, update_shared_context, SharedWorkspaceContext,
};
use crate::tools::policy::{request_approval, ApprovalMap, PolicyConfig};
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
) -> ToolResult {
    let Some(role) = parse_agent_role(&args.role) else {
        return ToolResult::error(format!("Unknown agent role: {}", args.role));
    };
    if args.task.trim().is_empty() {
        return ToolResult::error("delegate_agent task is empty");
    }
    if !request_approval(
        events,
        approvals,
        format!("Record handoff to {}", args.role),
        format!(
            "Task:\n{}\n\nContext:\n{}",
            args.task,
            args.context.as_deref().unwrap_or("")
        ),
    ) {
        return ToolResult::error("delegate_agent denied by user");
    }

    match record_handoff(
        workspace,
        role,
        args.from.unwrap_or_else(|| "Leetcode".to_string()),
        args.task,
        args.context.unwrap_or_default(),
        args.expected_output
            .unwrap_or_else(|| "Specialist recommendation and next actions".to_string()),
    ) {
        Ok(record) => ToolResult::ok(
            serde_json::to_string_pretty(&record)
                .unwrap_or_else(|_| "handoff recorded".to_string()),
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
        return ToolResult::error(format!("Unknown subagent role: {}", args.role));
    };
    if args.task.trim().is_empty() {
        return ToolResult::error("run_subagent task is empty");
    }

    let max_rounds = args.max_rounds.unwrap_or(4).clamp(1, 8);
    if !request_approval(
        &events,
        &approvals,
        format!("Run {} subagent", args.role),
        format!(
            "Task:\n{}\n\nContext:\n{}\n\nMax rounds: {}",
            args.task,
            args.context.as_deref().unwrap_or(""),
            max_rounds
        ),
    ) {
        return ToolResult::error("run_subagent denied by user");
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
                .unwrap_or_else(|_| "subagent run finished".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn update_context(
    workspace: &Workspace,
    args: UpdateWorkspaceContextArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    if !request_approval(
        events,
        approvals,
        "Update shared workspace context",
        serde_json::to_string_pretty(&json!({
            "summary": args.summary,
            "decisions": args.decisions,
            "open_questions": args.open_questions,
            "important_files": args.important_files,
            "important_assets": args.important_assets
        }))
        .unwrap_or_else(|_| "Update shared context".to_string()),
    ) {
        return ToolResult::error("update_workspace_context denied by user");
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
                .unwrap_or_else(|_| "workspace context updated".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn record_summary(
    workspace: &Workspace,
    args: RunSummaryArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    if args.summary.trim().is_empty() {
        return ToolResult::error("record_run_summary summary is empty");
    }
    if !request_approval(
        events,
        approvals,
        "Record run summary",
        args.summary.clone(),
    ) {
        return ToolResult::error("record_run_summary denied by user");
    }

    match record_run_summary(
        workspace,
        args.title.unwrap_or_else(|| "Agent run".to_string()),
        args.summary,
        args.completed,
        args.next_steps,
        args.risks,
    ) {
        Ok(summary) => ToolResult::ok(
            serde_json::to_string_pretty(&summary)
                .unwrap_or_else(|_| "run summary recorded".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn export_orchestration_trace(workspace: &Workspace) -> ToolResult {
    match export_trace(workspace) {
        Ok(path) => ToolResult::ok(
            serde_json::to_string_pretty(&json!({ "trace_path": path }))
                .unwrap_or_else(|_| "trace exported".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn create_eval(
    workspace: &Workspace,
    args: CreateReplayEvalArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    if args.prompt.trim().is_empty() {
        return ToolResult::error("create_replay_eval prompt is empty");
    }
    if !request_approval(
        events,
        approvals,
        format!("Create replay eval: {}", args.name),
        args.prompt.clone(),
    ) {
        return ToolResult::error("create_replay_eval denied by user");
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
                .unwrap_or_else(|_| "replay eval created".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&orchestration_snapshot(workspace))
            .unwrap_or_else(|_| "orchestration snapshot".to_string()),
    )
}
