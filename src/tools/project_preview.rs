use crate::agent::types::{AppEvent, ToolResult};
use crate::project::{detect_project_profiles, find_project_preview};
use crate::tools::policy::{request_approval, ApprovalMap};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::process::Command;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
pub struct OpenProjectPreviewArgs {
    pub preview: Option<String>,
    pub profile: Option<String>,
    pub url: Option<String>,
}

pub fn open_project_preview(
    workspace: &Workspace,
    args: OpenProjectPreviewArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let hook = if args
        .url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .is_some()
    {
        None
    } else {
        let profiles = detect_project_profiles(workspace);
        let selected = args.preview.as_deref().unwrap_or("dev-server");
        match find_project_preview(&profiles, selected, args.profile.as_deref())
            .or_else(|| profiles.iter().flat_map(|profile| profile.previews.iter()).next().cloned())
        {
            Some(hook) => Some(hook),
            None => {
                return ToolResult::error(
                    "No project preview hook found. Start a known dev server with project_command or provide a url.",
                )
            }
        }
    };

    let url = args
        .url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .map(str::trim)
        .map(ToString::to_string)
        .or_else(|| hook.as_ref().and_then(|hook| hook.url.clone()));

    if let Some(url) = url {
        if !request_approval(
            events,
            approvals,
            "Open project preview",
            format!("Open URL:\n{url}"),
        ) {
            return ToolResult::error("open_project_preview denied by user");
        }
        return match open_url(&url) {
            Ok(()) => ToolResult::ok(
                serde_json::to_string_pretty(&json!({ "opened_url": url }))
                    .unwrap_or_else(|_| format!("opened {url}")),
            ),
            Err(err) => ToolResult::error(err.to_string()),
        };
    }

    if let Some(hook) = hook {
        return ToolResult::ok(
            serde_json::to_string_pretty(&json!({
                "preview": hook.id,
                "label": hook.label,
                "command_id": hook.command_id,
                "description": hook.description,
                "next_step": "Call project_command with command_id to open this app/editor preview."
            }))
            .unwrap_or_else(|_| "preview hook found".to_string()),
        );
    }

    ToolResult::error("No preview URL or command hook found")
}

#[cfg(target_os = "windows")]
fn open_url(url: &str) -> anyhow::Result<()> {
    Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(url)
        .spawn()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) -> anyhow::Result<()> {
    Command::new("open").arg(url).spawn()?;
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_url(url: &str) -> anyhow::Result<()> {
    Command::new("xdg-open").arg(url).spawn()?;
    Ok(())
}
