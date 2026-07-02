pub mod asset_generation;
pub mod desktop;
pub mod filesystem;
pub mod policy;
pub mod shell;

use crate::agent::types::{ActRequest, AppEvent, ToolAction, ToolCall, ToolResult};
use crate::config::AppConfig;
use crate::tools::asset_generation::{
    GenerateImageAssetArgs, OpenAssetFolderArgs, RegenerateImageAssetArgs, UseAssetAsAppIconArgs,
    VaryImageAssetArgs,
};
use crate::tools::desktop::{HotkeyArgs, MouseClickArgs, TypeTextArgs};
use crate::tools::filesystem::{
    ApplyPatchArgs, EditFileArgs, GrepArgs, ListFilesArgs, ReadFileArgs, WriteFileArgs,
};
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::shell::RunShellArgs;
use crate::workspace::Workspace;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ToolDispatcher {
    workspace: Option<Workspace>,
    config: AppConfig,
    events: Sender<AppEvent>,
    approvals: ApprovalMap,
    cancel: Arc<AtomicBool>,
    policy: PolicyConfig,
}

impl ToolDispatcher {
    pub fn new(
        workspace: Option<Workspace>,
        config: AppConfig,
        events: Sender<AppEvent>,
        approvals: ApprovalMap,
        cancel: Arc<AtomicBool>,
        policy: PolicyConfig,
    ) -> Self {
        Self {
            workspace,
            config,
            events,
            approvals,
            cancel,
            policy,
        }
    }

    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let tool_id = Uuid::new_v4().to_string();
        let summary = summarize_call(call);
        let _ = self.events.send(AppEvent::ToolStarted {
            id: tool_id.clone(),
            name: call.name.clone(),
            summary,
        });

        let result = self.execute_inner(call, &tool_id).await;
        let _ = self.events.send(AppEvent::ToolFinished {
            id: tool_id,
            output: result.output.clone(),
        });
        result
    }

    async fn execute_inner(&self, call: &ToolCall, tool_id: &str) -> ToolResult {
        if call.name != "act" {
            return ToolResult::error(format!("Unknown tool: {}", call.name));
        }

        let request = match serde_json::from_str::<ActRequest>(&call.arguments) {
            Ok(request) => request,
            Err(err) => return ToolResult::error(format!("Invalid act arguments: {err}")),
        };

        match request.action {
            ToolAction::Screenshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                desktop::screenshot(workspace, &self.events, &self.approvals)
            }
            ToolAction::MouseClick => {
                match serde_json::from_value::<MouseClickArgs>(request.args) {
                    Ok(args) => desktop::mouse_click(args, &self.events, &self.approvals),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::TypeText => match serde_json::from_value::<TypeTextArgs>(request.args) {
                Ok(args) => desktop::type_text(args, &self.events, &self.approvals),
                Err(err) => ToolResult::error(err.to_string()),
            },
            ToolAction::Hotkey => match serde_json::from_value::<HotkeyArgs>(request.args) {
                Ok(args) => desktop::hotkey(args, &self.events, &self.approvals),
                Err(err) => ToolResult::error(err.to_string()),
            },
            ToolAction::ListFiles => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<ListFilesArgs>(request.args) {
                    Ok(args) => filesystem::list_files(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ReadFile => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<ReadFileArgs>(request.args) {
                    Ok(args) => filesystem::read_file(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::WriteFile => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<WriteFileArgs>(request.args) {
                    Ok(args) => filesystem::write_file(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::EditFile => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<EditFileArgs>(request.args) {
                    Ok(args) => filesystem::edit_file(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ApplyPatch => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<ApplyPatchArgs>(request.args) {
                    Ok(args) => filesystem::apply_patch(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::Grep => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<GrepArgs>(request.args) {
                    Ok(args) => filesystem::grep(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::GenerateImageAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<GenerateImageAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_image_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RegenerateImageAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<RegenerateImageAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::regenerate_image_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::VaryImageAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<VaryImageAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::vary_image_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UseAssetAsAppIcon => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<UseAssetAsAppIconArgs>(request.args) {
                    Ok(args) => asset_generation::use_asset_as_app_icon(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::OpenAssetFolder => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<OpenAssetFolderArgs>(request.args) {
                    Ok(args) => asset_generation::open_asset_folder(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RunShell => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<RunShellArgs>(request.args) {
                    Ok(args) => {
                        shell::run_shell(
                            workspace,
                            args,
                            self.events.clone(),
                            self.approvals.clone(),
                            self.cancel.clone(),
                            self.policy.clone(),
                            tool_id.to_string(),
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
        }
    }
}

fn summarize_call(call: &ToolCall) -> String {
    let preview = call
        .arguments
        .chars()
        .take(180)
        .collect::<String>()
        .replace('\n', " ");
    format!("{}({preview})", call.name)
}
