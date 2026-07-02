use crate::agent::types::{AppEvent, ToolResult};
use crate::project::{detect_project_profiles, find_project_preview};
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
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
    policy: &PolicyConfig,
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
                    "Preview hook проекта не найден. Запустите известный dev-сервер через project_command или передайте url.",
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
        if !request_approval_if(
            policy.require_external_approval,
            events,
            approvals,
            "Открыть предпросмотр проекта",
            format!("Открыть URL:\n{url}"),
        ) {
            return ToolResult::error("open_project_preview отклонён пользователем");
        }
        return match open_url(&url) {
            Ok(()) => ToolResult::ok(
                serde_json::to_string_pretty(&json!({ "opened_url": url }))
                    .unwrap_or_else(|_| format!("открыто {url}")),
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
                "next_step": "Вызовите project_command с command_id, чтобы открыть предпросмотр приложения/редактора."
            }))
            .unwrap_or_else(|_| "preview hook найден".to_string()),
        );
    }

    ToolResult::error("URL предпросмотра или command hook не найден")
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
