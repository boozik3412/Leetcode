use crate::agent::types::{AppEvent, ToolResult};
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::tools::shell::{run_shell, RunShellArgs};
use crate::unreal_gameplay::{
    build_gameplay_apply_command, build_gameplay_playtest_command, create_gameplay_plan,
    gameplay_snapshot, record_apply_result, record_playtest_result, ApplyGameplayPlanArgs,
    CreateGameplayPlanArgs, RunGameplayPlaytestArgs,
};
use crate::workspace::Workspace;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

pub fn snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&gameplay_snapshot(workspace))
            .unwrap_or_else(|_| "Unreal gameplay snapshot".to_string()),
    )
}

pub fn create_plan(
    workspace: &Workspace,
    args: CreateGameplayPlanArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if !request_approval_if(
        policy.require_write_approval,
        events,
        approvals,
        "Создать gameplay-план Unreal",
        format!("Recipe: {:?}\nBrief: {}", args.recipe, args.brief),
    ) {
        return ToolResult::error("create_gameplay_plan отклонён пользователем");
    }
    match create_gameplay_plan(workspace, args) {
        Ok(plan) => ToolResult::ok(
            serde_json::to_string_pretty(&plan).unwrap_or_else(|_| plan.file_path.clone()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub async fn apply_plan(
    workspace: &Workspace,
    args: ApplyGameplayPlanArgs,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
    tool_id: String,
) -> ToolResult {
    if !request_approval_if(
        policy.require_write_approval,
        &events,
        &approvals,
        "Изменить уровень Unreal",
        format!(
            "Map: {}\nOperations: {}\nPlan: {}",
            args.map_path,
            args.operations.len(),
            args.plan_id.as_deref().unwrap_or("нет")
        ),
    ) {
        return ToolResult::error("apply_gameplay_plan отклонён пользователем");
    }
    let plan_id = args.plan_id.clone();
    let command = match build_gameplay_apply_command(workspace, args) {
        Ok(command) => command,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    let shell = run_shell(
        workspace,
        RunShellArgs {
            cmd: command.shell_command.clone(),
            cwd: Some(".".to_string()),
            shell: Some("powershell".to_string()),
            timeout_secs: Some(command.timeout_secs),
        },
        events,
        approvals,
        cancel,
        policy,
        tool_id,
    )
    .await;
    let result = workspace
        .read_text(&command.result_path, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let unreal_ok = result
        .as_ref()
        .and_then(|value| value.get("ok"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let success = shell.ok && shell.output.starts_with("код выхода: 0\n") && unreal_ok;
    let _ = record_apply_result(workspace, plan_id.as_deref(), success);
    let payload = json!({
        "success": success,
        "manifest_path": command.manifest_path,
        "result_path": command.result_path,
        "unreal_result": result,
        "command_output": shell.output,
    });
    let rendered = serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|_| "Unreal gameplay apply finished".to_string());
    if success {
        ToolResult::ok(rendered)
    } else {
        ToolResult::error(rendered)
    }
}

pub async fn run_playtest(
    workspace: &Workspace,
    args: RunGameplayPlaytestArgs,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
    tool_id: String,
) -> ToolResult {
    let command = match build_gameplay_playtest_command(workspace, args) {
        Ok(command) => command,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    let started = Instant::now();
    let shell = run_shell(
        workspace,
        RunShellArgs {
            cmd: command.shell_command.clone(),
            cwd: Some(".".to_string()),
            shell: Some("powershell".to_string()),
            timeout_secs: Some(command.timeout_secs),
        },
        events,
        approvals,
        cancel,
        policy,
        tool_id,
    )
    .await;
    let success = shell.ok && shell.output.starts_with("код выхода: 0\n");
    let run = match record_playtest_result(
        workspace,
        &command,
        success,
        &shell.output,
        started.elapsed().as_millis() as u64,
    ) {
        Ok(run) => run,
        Err(err) => return ToolResult::error(format!("Не удалось сохранить playtest: {err}")),
    };
    let rendered = serde_json::to_string_pretty(&run)
        .unwrap_or_else(|_| "Unreal gameplay playtest finished".to_string());
    if success {
        ToolResult::ok(rendered)
    } else {
        ToolResult::error(rendered)
    }
}
