use crate::agent::types::{AppEvent, ToolResult};
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::shell::{run_shell, RunShellArgs};
use crate::unreal::{build_unreal_command, parse_unreal_log, UnrealCommandArgs};
use crate::workspace::Workspace;
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Serialize)]
struct UnrealCommandReport {
    command: String,
    label: String,
    success: bool,
    detached: bool,
    issues: Vec<crate::unreal::UnrealLogIssue>,
    output: String,
}

pub async fn run_unreal_command(
    workspace: &Workspace,
    args: UnrealCommandArgs,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
    tool_id: String,
) -> ToolResult {
    let spec = match build_unreal_command(workspace, &args) {
        Ok(spec) => spec,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    let shell_result = run_shell(
        workspace,
        RunShellArgs {
            cmd: spec.shell_command.clone(),
            cwd: Some(spec.cwd.clone()),
            shell: None,
            timeout_secs: Some(spec.timeout_secs),
        },
        events,
        approvals,
        cancel,
        policy,
        tool_id,
    )
    .await;
    let success = shell_result.ok && shell_result.output.starts_with("код выхода: 0\n");
    let issues = parse_unreal_log(&shell_result.output);
    let report = UnrealCommandReport {
        command: spec.id,
        label: spec.label,
        success,
        detached: spec.detached,
        issues,
        output: truncate_output(&shell_result.output, 120_000),
    };
    let rendered =
        serde_json::to_string_pretty(&report).unwrap_or_else(|_| shell_result.output.clone());
    if success {
        ToolResult::ok(rendered)
    } else {
        ToolResult::error(rendered)
    }
}

fn truncate_output(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut result = value.chars().take(max_chars).collect::<String>();
    result.push_str("\n... вывод Unreal обрезан ...");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_unreal_output_on_char_boundary() {
        assert_eq!(
            truncate_output("тест", 2),
            "те\n... вывод Unreal обрезан ..."
        );
        assert_eq!(truncate_output("ok", 10), "ok");
    }
}
