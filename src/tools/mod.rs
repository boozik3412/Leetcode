pub mod asset_generation;
pub mod desktop;
pub mod filesystem;
pub mod game_workflows;
pub mod orchestration;
pub mod policy;
pub mod project_commands;
pub mod project_preview;
pub mod shell;
pub mod terminal;

use crate::agent::types::{ActRequest, AppEvent, ToolAction, ToolCall, ToolResult};
use crate::config::AppConfig;
use crate::tools::asset_generation::{
    AttachAssetArgs, ExportAssetArgs, GenerateAudioAssetArgs, GenerateImageAssetArgs,
    GenerateSpritesheetAssetArgs, GenerateVideoAssetArgs, OpenAssetFolderArgs,
    RegenerateImageAssetArgs, UpscaleAssetArgs, UseAssetAsAppIconArgs, VaryImageAssetArgs,
};
use crate::tools::desktop::{
    DesktopStepArgs, FocusWindowArgs, HotkeyArgs, MouseClickArgs, TypeTextArgs,
};
use crate::tools::filesystem::{
    ApplyPatchArgs, EditFileArgs, GrepArgs, ListFilesArgs, ReadFileArgs, WriteFileArgs,
};
use crate::tools::game_workflows::GameWorkflowArgs;
use crate::tools::orchestration::{
    CreateReplayEvalArgs, DelegateAgentArgs, RunSubagentArgs, RunSummaryArgs,
    UpdateWorkspaceContextArgs,
};
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::project_commands::ProjectCommandArgs;
use crate::tools::project_preview::OpenProjectPreviewArgs;
use crate::tools::shell::RunShellArgs;
use crate::tools::terminal::{TerminalReadArgs, TerminalStartArgs, TerminalWriteArgs};
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
            ToolAction::ActiveWindow => desktop::active_window(),
            ToolAction::FocusWindow => {
                match serde_json::from_value::<FocusWindowArgs>(request.args) {
                    Ok(args) => desktop::focus_window(args, &self.events, &self.approvals),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::DesktopStep => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<DesktopStepArgs>(request.args) {
                    Ok(args) => {
                        desktop::desktop_step(workspace, args, &self.events, &self.approvals)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
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
            ToolAction::ProjectCommand => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<ProjectCommandArgs>(request.args) {
                    Ok(args) => {
                        project_commands::run_project_command(
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
            ToolAction::GameWorkflow => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<GameWorkflowArgs>(request.args) {
                    Ok(args) => game_workflows::create_game_workflow(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::OpenProjectPreview => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<OpenProjectPreviewArgs>(request.args) {
                    Ok(args) => project_preview::open_project_preview(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RunSubagent => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<RunSubagentArgs>(request.args) {
                    Ok(args) => {
                        Box::pin(orchestration::run_subagent(
                            workspace,
                            args,
                            &self.config,
                            self.events.clone(),
                            self.approvals.clone(),
                            self.cancel.clone(),
                            self.policy.clone(),
                        ))
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::DelegateAgent => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<DelegateAgentArgs>(request.args) {
                    Ok(args) => orchestration::delegate_agent(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UpdateWorkspaceContext => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<UpdateWorkspaceContextArgs>(request.args) {
                    Ok(args) => orchestration::update_context(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RecordRunSummary => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<RunSummaryArgs>(request.args) {
                    Ok(args) => orchestration::record_summary(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ExportTrace => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                orchestration::export_orchestration_trace(workspace)
            }
            ToolAction::CreateReplayEval => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<CreateReplayEvalArgs>(request.args) {
                    Ok(args) => {
                        orchestration::create_eval(workspace, args, &self.events, &self.approvals)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::OrchestrationSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                orchestration::snapshot(workspace)
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
            ToolAction::GenerateSpritesheetAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<GenerateSpritesheetAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_spritesheet_asset(
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
            ToolAction::GenerateAudioAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<GenerateAudioAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_audio_asset(
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
            ToolAction::GenerateVideoAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<GenerateVideoAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_video_asset(
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
            ToolAction::UpscaleAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<UpscaleAssetArgs>(request.args) {
                    Ok(args) => asset_generation::upscale_existing_asset(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ExportAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<ExportAssetArgs>(request.args) {
                    Ok(args) => asset_generation::export_existing_asset(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::AttachAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<AttachAssetArgs>(request.args) {
                    Ok(args) => asset_generation::attach_asset(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                    ),
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
            ToolAction::TerminalStart => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("No workspace selected");
                };
                match serde_json::from_value::<TerminalStartArgs>(request.args) {
                    Ok(args) => terminal::terminal_start(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::TerminalWrite => {
                match serde_json::from_value::<TerminalWriteArgs>(request.args) {
                    Ok(args) => {
                        terminal::terminal_write(args, &self.events, &self.approvals, &self.policy)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::TerminalRead => {
                match serde_json::from_value::<TerminalReadArgs>(request.args) {
                    Ok(args) => terminal::terminal_read(args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::TerminalStop => {
                terminal::terminal_stop(&self.events, &self.approvals, &self.policy)
            }
            ToolAction::TerminalClear => terminal::terminal_clear(),
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
