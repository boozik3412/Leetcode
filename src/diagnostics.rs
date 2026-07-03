use crate::agent::types::ToolResult;
use crate::config::{config_path, journal_path, AppConfig};
use crate::http::{proxy_status_label, proxy_system_status_label};
use crate::workspace::Workspace;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, Serialize)]
pub struct EnvironmentDiagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub current_dir: String,
    pub executable: String,
    pub workspace: Option<String>,
    pub config_path: Option<String>,
    pub journal_path: Option<String>,
    pub proxy: String,
    pub system_proxy: String,
    pub tools: Vec<DiagnosticItem>,
    pub release_notes: Vec<DiagnosticItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticItem {
    pub name: String,
    pub status: String,
    pub detail: String,
}

pub fn environment_diagnostics(
    config: &AppConfig,
    workspace: Option<&Workspace>,
) -> EnvironmentDiagnostics {
    let current_dir = std::env::current_dir()
        .map(display_path)
        .unwrap_or_else(|err| format!("unavailable: {err}"));
    let executable = std::env::current_exe()
        .map(display_path)
        .unwrap_or_else(|err| format!("unavailable: {err}"));
    let config_path = config_path().map(display_path);
    let journal_path = journal_path().map(display_path);
    let workspace_root = workspace.map(|workspace| workspace.root().display().to_string());

    EnvironmentDiagnostics {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        current_dir,
        executable,
        workspace: workspace_root.clone(),
        config_path: config_path.clone(),
        journal_path: journal_path.clone(),
        proxy: proxy_status_label(config),
        system_proxy: proxy_system_status_label(config).to_string(),
        tools: vec![
            command_diagnostic("git", &["--version"]),
            command_diagnostic("cargo", &["--version"]),
            command_diagnostic("rustup", &["--version"]),
            command_diagnostic(
                "powershell",
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-Command",
                    "$PSVersionTable.PSVersion.ToString()",
                ],
            ),
        ],
        release_notes: release_diagnostics(config_path, journal_path, workspace_root),
    }
}

pub fn environment_snapshot(config: &AppConfig, workspace: Option<&Workspace>) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&environment_diagnostics(config, workspace))
            .unwrap_or_else(|_| "environment diagnostics".to_string()),
    )
}

fn command_diagnostic(command: &str, args: &[&str]) -> DiagnosticItem {
    match Command::new(command).args(args).output() {
        Ok(output) if output.status.success() => DiagnosticItem {
            name: command.to_string(),
            status: "ok".to_string(),
            detail: first_non_empty_line(&String::from_utf8_lossy(&output.stdout))
                .unwrap_or_else(|| "available".to_string()),
        },
        Ok(output) => DiagnosticItem {
            name: command.to_string(),
            status: "error".to_string(),
            detail: first_non_empty_line(&String::from_utf8_lossy(&output.stderr))
                .or_else(|| first_non_empty_line(&String::from_utf8_lossy(&output.stdout)))
                .unwrap_or_else(|| format!("exit status {}", output.status)),
        },
        Err(err) => DiagnosticItem {
            name: command.to_string(),
            status: "missing".to_string(),
            detail: err.to_string(),
        },
    }
}

fn release_diagnostics(
    config_path: Option<String>,
    journal_path: Option<String>,
    workspace: Option<String>,
) -> Vec<DiagnosticItem> {
    vec![
        DiagnosticItem {
            name: "config".to_string(),
            status: if config_path.is_some() { "ok" } else { "missing" }.to_string(),
            detail: config_path.unwrap_or_else(|| "config directory is unavailable".to_string()),
        },
        DiagnosticItem {
            name: "journal".to_string(),
            status: if journal_path.is_some() { "ok" } else { "missing" }.to_string(),
            detail: journal_path.unwrap_or_else(|| "data directory is unavailable".to_string()),
        },
        DiagnosticItem {
            name: "workspace data".to_string(),
            status: if workspace.is_some() { "ok" } else { "not selected" }.to_string(),
            detail: workspace
                .map(|root| format!("{root}/assets/generated/leetcode"))
                .unwrap_or_else(|| "select a project to enable workspace-local history".to_string()),
        },
        DiagnosticItem {
            name: "crash policy".to_string(),
            status: "documented".to_string(),
            detail: "fatal crashes are not intercepted yet; local audit data is written to the journal and selected workspace".to_string(),
        },
    ]
}

fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_first_non_empty_line() {
        assert_eq!(
            first_non_empty_line("\n\n  hello\nworld"),
            Some("hello".to_string())
        );
        assert_eq!(first_non_empty_line("\n\t"), None);
    }
}
