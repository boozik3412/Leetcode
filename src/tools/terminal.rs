use crate::agent::types::{AppEvent, ToolResult};
use crate::terminal::{
    clear_terminal_output, read_terminal_snapshot, start_terminal_session, stop_terminal_session,
    write_terminal_input,
};
use crate::tools::policy::{request_approval, ApprovalMap, PolicyConfig};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
pub struct TerminalStartArgs {
    pub cwd: Option<String>,
    pub shell: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalWriteArgs {
    pub input: String,
    pub enter: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalReadArgs {
    pub lines: Option<usize>,
    pub since_seq: Option<u64>,
}

pub fn terminal_start(
    workspace: &Workspace,
    args: TerminalStartArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let cwd_label = args.cwd.as_deref().unwrap_or(".");
    let shell_label = args.shell.as_deref().unwrap_or("powershell");
    if policy.require_shell_approval
        && !request_approval(
            events,
            approvals,
            format!("Start persistent terminal: {shell_label}"),
            format!("Working directory: {cwd_label}"),
        )
    {
        return ToolResult::error("terminal_start denied by user");
    }

    match start_terminal_session(workspace, args.cwd.as_deref(), args.shell.as_deref()) {
        Ok(snapshot) => ToolResult::ok(serialize_snapshot(snapshot)),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn terminal_write(
    args: TerminalWriteArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.input.is_empty() {
        return ToolResult::error("terminal_write input is empty");
    }
    let needs_approval = policy.require_shell_for(&args.input);
    if needs_approval
        && !request_approval(
            events,
            approvals,
            "Write to persistent terminal",
            format!(
                "Input:\n{}\n\nEnter: {}",
                args.input,
                args.enter.unwrap_or(true)
            ),
        )
    {
        return ToolResult::error("terminal_write denied by user");
    }

    match write_terminal_input(&args.input, args.enter.unwrap_or(true)) {
        Ok(snapshot) => ToolResult::ok(serialize_snapshot(snapshot)),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn terminal_read(args: TerminalReadArgs) -> ToolResult {
    ToolResult::ok(serialize_snapshot(read_terminal_snapshot(
        args.lines,
        args.since_seq,
    )))
}

pub fn terminal_stop(
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if policy.require_shell_approval
        && !request_approval(
            events,
            approvals,
            "Stop persistent terminal",
            "The agent wants to terminate the current persistent terminal session.",
        )
    {
        return ToolResult::error("terminal_stop denied by user");
    }

    match stop_terminal_session() {
        Ok(snapshot) => ToolResult::ok(serialize_snapshot(snapshot)),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn terminal_clear() -> ToolResult {
    ToolResult::ok(serialize_snapshot(clear_terminal_output()))
}

fn serialize_snapshot(snapshot: crate::terminal::TerminalSnapshot) -> String {
    serde_json::to_string_pretty(&json!(snapshot))
        .unwrap_or_else(|_| "terminal snapshot".to_string())
}
