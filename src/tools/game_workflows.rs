use crate::agent::types::{AppEvent, ToolResult};
use crate::game_workflows::{parse_workflow_kind, run_game_workflow, GameWorkflowRequest};
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
pub struct GameWorkflowArgs {
    pub workflow: String,
    pub title: Option<String>,
    pub brief: Option<String>,
}

pub fn create_game_workflow(
    workspace: &Workspace,
    args: GameWorkflowArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let Some(workflow) = parse_workflow_kind(&args.workflow) else {
        return ToolResult::error(format!("Неизвестный игровой сценарий: {}", args.workflow));
    };
    let title = args
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("Игровой сценарий")
        .to_string();
    let brief = args.brief.unwrap_or_default();

    if !request_approval_if(
        policy.require_write_approval,
        events,
        approvals,
        format!("Создать игровой сценарий: {title}"),
        format!("Сценарий: {}\n\nБриф:\n{}", args.workflow, brief),
    ) {
        return ToolResult::error("game_workflow отклонён пользователем");
    }

    match run_game_workflow(
        workspace,
        GameWorkflowRequest {
            workflow,
            title,
            brief,
        },
    ) {
        Ok(result) => ToolResult::ok(
            serde_json::to_string_pretty(&json!({
                "path": result.path,
                "summary": result.summary
            }))
            .unwrap_or_else(|_| "игровой сценарий создан".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}
