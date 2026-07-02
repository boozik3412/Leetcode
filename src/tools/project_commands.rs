use crate::agent::types::{AppEvent, ToolResult};
use crate::project::{describe_project_commands, detect_project_profiles, find_project_command};
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::shell::{run_shell, RunShellArgs};
use crate::workspace::Workspace;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ProjectCommandArgs {
    pub command: String,
    pub profile: Option<String>,
}

pub async fn run_project_command(
    workspace: &Workspace,
    args: ProjectCommandArgs,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
    tool_id: String,
) -> ToolResult {
    let profiles = detect_project_profiles(workspace);
    let Some(command) = find_project_command(&profiles, &args.command, args.profile.as_deref())
    else {
        return ToolResult::error(format!(
            "Project command '{}' was not found.\nAvailable commands:\n{}",
            args.command,
            describe_project_commands(&profiles)
        ));
    };

    run_shell(
        workspace,
        RunShellArgs {
            cmd: command.command,
            cwd: Some(command.cwd),
            shell: None,
            timeout_secs: Some(command.timeout_secs),
        },
        events,
        approvals,
        cancel,
        policy,
        tool_id,
    )
    .await
}
