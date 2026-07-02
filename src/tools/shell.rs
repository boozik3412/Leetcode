use crate::agent::types::{AppEvent, ToolResult};
use crate::tools::policy::{request_approval, ApprovalMap, PolicyConfig};
use crate::workspace::Workspace;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

#[derive(Debug, Deserialize)]
pub struct RunShellArgs {
    pub cmd: String,
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub timeout_secs: Option<u64>,
}

pub async fn run_shell(
    workspace: &Workspace,
    args: RunShellArgs,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
    tool_id: String,
) -> ToolResult {
    let cwd = match workspace.resolve_existing(args.cwd.as_deref().unwrap_or(".")) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if !cwd.is_dir() {
        return ToolResult::error("cwd для run_shell должен быть папкой");
    }

    let needs_approval = policy.require_shell_for(&args.cmd);
    if needs_approval
        && !request_approval(
            &events,
            &approvals,
            format!("Запустить shell-команду в {}", cwd.display()),
            args.cmd.clone(),
        )
    {
        return ToolResult::error("run_shell отклонён пользователем");
    }

    let (mut command, cleanup_path) = match shell_command(&args.cmd, args.shell.as_deref()) {
        Ok(command) => command,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    command
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            cleanup_temp_script(&cleanup_path);
            return ToolResult::error(err.to_string());
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = stdout.map(|stdout| {
        tokio::spawn(read_stream(
            stdout,
            events.clone(),
            tool_id.clone(),
            "stdout",
        ))
    });
    let stderr_task = stderr.map(|stderr| {
        tokio::spawn(read_stream(
            stderr,
            events.clone(),
            tool_id.clone(),
            "stderr",
        ))
    });

    let timeout = Duration::from_secs(args.timeout_secs.unwrap_or(120).clamp(1, 1_800));
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill().await;
            cleanup_temp_script(&cleanup_path);
            return ToolResult::error("команда отменена");
        }
        if started.elapsed() > timeout {
            let _ = child.kill().await;
            cleanup_temp_script(&cleanup_path);
            return ToolResult::error(format!(
                "команда превысила таймаут {} сек.",
                timeout.as_secs()
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(err) => {
                cleanup_temp_script(&cleanup_path);
                return ToolResult::error(err.to_string());
            }
        }
    };

    let mut output = String::new();
    if let Some(task) = stdout_task {
        output.push_str(&task.await.unwrap_or_default());
    }
    if let Some(task) = stderr_task {
        output.push_str(&task.await.unwrap_or_default());
    }

    let code = status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "завершён сигналом".to_string());
    if output.trim().is_empty() {
        output = "(пусто)".to_string();
    }

    cleanup_temp_script(&cleanup_path);
    ToolResult::ok(format!("код выхода: {code}\n{output}"))
}

async fn read_stream<R>(
    stream: R,
    events: Sender<AppEvent>,
    tool_id: String,
    label: &'static str,
) -> String
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut output = String::new();
    let mut lines = BufReader::new(stream).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let rendered = format!("[{label}] {line}");
        output.push_str(&rendered);
        output.push('\n');
        let _ = events.send(AppEvent::ToolOutput {
            id: tool_id.clone(),
            chunk: rendered,
        });
    }

    output
}

#[cfg(target_os = "windows")]
fn shell_command(cmd: &str, shell: Option<&str>) -> anyhow::Result<(Command, Option<PathBuf>)> {
    if shell
        .map(|shell| shell.eq_ignore_ascii_case("cmd"))
        .unwrap_or(false)
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(cmd);
        return Ok((command, None));
    }

    let script_path = std::env::temp_dir().join(format!("leetcode-{}.ps1", uuid::Uuid::new_v4()));
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(cmd.as_bytes());
    bytes.extend_from_slice(b"\r\n");
    fs::write(&script_path, bytes)?;

    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path);
    Ok((command, Some(script_path)))
}

#[cfg(not(target_os = "windows"))]
fn shell_command(cmd: &str, _shell: Option<&str>) -> anyhow::Result<(Command, Option<PathBuf>)> {
    let mut command = Command::new("sh");
    command.arg("-lc").arg(cmd);
    Ok((command, None))
}

fn cleanup_temp_script(path: &Option<PathBuf>) {
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}
