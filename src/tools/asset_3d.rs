use crate::agent::types::{AppEvent, ToolResult};
use crate::asset_3d::{
    asset_3d_snapshot, build_unreal_import_command, refresh_3d_asset, submit_3d_asset,
    validate_3d_asset_path, RefreshThreeDAssetArgs, SubmitThreeDAssetArgs, UnrealImportThreeDArgs,
    ValidateThreeDAssetArgs,
};
use crate::config::AppConfig;
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::tools::shell::{run_shell, RunShellArgs};
use crate::workspace::Workspace;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub fn snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&asset_3d_snapshot(workspace))
            .unwrap_or_else(|_| "3D asset snapshot".to_string()),
    )
}

pub async fn submit(
    workspace: &Workspace,
    args: SubmitThreeDAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if !request_approval_if(
        policy.require_paid_api_approval,
        events,
        approvals,
        "Submit a paid 3D generation job",
        format!(
            "Provider: {}\nInput: {}\nPrompt: {}\nTarget polycount: {}",
            args.provider.as_deref().unwrap_or("meshy-3d"),
            args.image_path.as_deref().unwrap_or("text"),
            args.prompt,
            args.target_polycount.unwrap_or(20_000)
        ),
    ) {
        return ToolResult::error("submit_3d_asset was denied by the user");
    }

    match submit_3d_asset(workspace, args, config).await {
        Ok(job) => {
            ToolResult::ok(serde_json::to_string_pretty(&job).unwrap_or_else(|_| job.id.clone()))
        }
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub async fn refresh(
    workspace: &Workspace,
    args: RefreshThreeDAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if !request_approval_if(
        policy.require_write_approval,
        events,
        approvals,
        "Refresh and download a 3D generation job",
        format!("Job: {}", args.job_id),
    ) {
        return ToolResult::error("refresh_3d_asset was denied by the user");
    }
    match refresh_3d_asset(workspace, args, config).await {
        Ok(job) => {
            ToolResult::ok(serde_json::to_string_pretty(&job).unwrap_or_else(|_| job.id.clone()))
        }
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn validate(workspace: &Workspace, args: ValidateThreeDAssetArgs) -> ToolResult {
    match validate_3d_asset_path(workspace, &args.source_path) {
        Ok(report) => ToolResult::ok(
            serde_json::to_string_pretty(&report)
                .unwrap_or_else(|_| "3D validation passed".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub async fn import_unreal(
    workspace: &Workspace,
    args: UnrealImportThreeDArgs,
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
        "Prepare a 3D import into Unreal",
        format!(
            "Source: {}\nDestination: {}\nType: {}",
            args.source_path,
            args.destination_path
                .as_deref()
                .unwrap_or("/Game/Generated/Leetcode"),
            args.asset_type.as_deref().unwrap_or("static_mesh")
        ),
    ) {
        return ToolResult::error("import_3d_asset_unreal was denied by the user");
    }

    let command = match build_unreal_import_command(workspace, args) {
        Ok(command) => command,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    let shell_result = run_shell(
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
        .read_text(&command.result_path, 1_000_000)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let payload = json!({
        "manifest_path": command.manifest_path,
        "result_path": command.result_path,
        "unreal_result": result,
        "command_output": shell_result.output,
    });
    let rendered = serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|_| "Unreal 3D import finished".to_string());
    if shell_result.ok
        && payload
            .pointer("/unreal_result/ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        ToolResult::ok(rendered)
    } else {
        ToolResult::error(rendered)
    }
}
