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
use crate::asset_library::{ExportAssetPackArgs, FavoriteAssetArgs, TagAssetArgs};
use crate::config::AppConfig;
use crate::diagnostics;
use crate::evals::RunReplayEvalArgs;
use crate::governance::{AddShellDenyPatternArgs, SetCategoryEnabledArgs, SetToolEnabledArgs};
use crate::memory::{
    RecordDecisionArgs, RecordProjectGoalArgs, UpdateTaskStatusArgs, UpsertTaskArgs,
};
use crate::provider_health;
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
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
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
            return ToolResult::error(format!("Неизвестный инструмент: {}", call.name));
        }

        let request = match serde_json::from_str::<ActRequest>(&call.arguments) {
            Ok(request) => request,
            Err(err) => return ToolResult::error(format!("Некорректные аргументы act: {err}")),
        };
        let governance_decision = crate::governance::evaluate_action(
            self.workspace.as_ref(),
            &request.action,
            &request.args,
        );
        if !governance_decision.allowed {
            let rendered = serde_json::to_string_pretty(&governance_decision)
                .unwrap_or_else(|_| governance_decision.reason.clone());
            let _ = self.events.send(AppEvent::ToolOutput {
                id: tool_id.to_string(),
                chunk: rendered.clone(),
            });
            return ToolResult::error(format!("Заблокировано правилами доступа: {rendered}"));
        }

        match request.action {
            ToolAction::Screenshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                desktop::screenshot(workspace, &self.events, &self.approvals, &self.policy)
            }
            ToolAction::ActiveWindow => desktop::active_window(),
            ToolAction::FocusWindow => {
                match serde_json::from_value::<FocusWindowArgs>(request.args) {
                    Ok(args) => {
                        desktop::focus_window(args, &self.events, &self.approvals, &self.policy)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::DesktopStep => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<DesktopStepArgs>(request.args) {
                    Ok(args) => desktop::desktop_step(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::MouseClick => {
                match serde_json::from_value::<MouseClickArgs>(request.args) {
                    Ok(args) => {
                        desktop::mouse_click(args, &self.events, &self.approvals, &self.policy)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::TypeText => match serde_json::from_value::<TypeTextArgs>(request.args) {
                Ok(args) => desktop::type_text(args, &self.events, &self.approvals, &self.policy),
                Err(err) => ToolResult::error(err.to_string()),
            },
            ToolAction::Hotkey => match serde_json::from_value::<HotkeyArgs>(request.args) {
                Ok(args) => desktop::hotkey(args, &self.events, &self.approvals, &self.policy),
                Err(err) => ToolResult::error(err.to_string()),
            },
            ToolAction::ListFiles => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ListFilesArgs>(request.args) {
                    Ok(args) => filesystem::list_files(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ReadFile => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ReadFileArgs>(request.args) {
                    Ok(args) => filesystem::read_file(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::WriteFile => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
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
                    return ToolResult::error("Рабочая папка не выбрана");
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
                    return ToolResult::error("Рабочая папка не выбрана");
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
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<GrepArgs>(request.args) {
                    Ok(args) => filesystem::grep(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ProjectCommand => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
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
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<GameWorkflowArgs>(request.args) {
                    Ok(args) => game_workflows::create_game_workflow(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::OpenProjectPreview => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<OpenProjectPreviewArgs>(request.args) {
                    Ok(args) => project_preview::open_project_preview(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RunSubagent => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
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
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<DelegateAgentArgs>(request.args) {
                    Ok(args) => orchestration::delegate_agent(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UpdateWorkspaceContext => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UpdateWorkspaceContextArgs>(request.args) {
                    Ok(args) => orchestration::update_context(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RecordRunSummary => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RunSummaryArgs>(request.args) {
                    Ok(args) => orchestration::record_summary(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ExportTrace => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                orchestration::export_orchestration_trace(workspace)
            }
            ToolAction::CreateReplayEval => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<CreateReplayEvalArgs>(request.args) {
                    Ok(args) => orchestration::create_eval(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::OrchestrationSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                orchestration::snapshot(workspace)
            }
            ToolAction::GenerateImageAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<GenerateImageAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_image_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::GenerateSpritesheetAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<GenerateSpritesheetAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_spritesheet_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::GenerateAudioAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<GenerateAudioAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_audio_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::GenerateVideoAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<GenerateVideoAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::generate_video_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RegenerateImageAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RegenerateImageAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::regenerate_image_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::VaryImageAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<VaryImageAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_generation::vary_image_asset(
                            workspace,
                            args,
                            &self.config,
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        )
                        .await
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UpscaleAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UpscaleAssetArgs>(request.args) {
                    Ok(args) => asset_generation::upscale_existing_asset(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ExportAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ExportAssetArgs>(request.args) {
                    Ok(args) => asset_generation::export_existing_asset(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::AttachAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<AttachAssetArgs>(request.args) {
                    Ok(args) => asset_generation::attach_asset(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UseAssetAsAppIcon => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UseAssetAsAppIconArgs>(request.args) {
                    Ok(args) => asset_generation::use_asset_as_app_icon(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::OpenAssetFolder => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<OpenAssetFolderArgs>(request.args) {
                    Ok(args) => asset_generation::open_asset_folder(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RunShell => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
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
                    return ToolResult::error("Рабочая папка не выбрана");
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
            ToolAction::GovernanceSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::governance::governance_snapshot(workspace)
            }
            ToolAction::SetToolEnabled => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<SetToolEnabledArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Изменить настройку инструмента", &args.tool)
                        {
                            return ToolResult::error("set_tool_enabled отклонён пользователем");
                        }
                        crate::governance::set_tool_enabled(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::SetCategoryEnabled => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<SetCategoryEnabledArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Изменить настройку категории", &args.category)
                        {
                            return ToolResult::error(
                                "set_category_enabled отклонён пользователем",
                            );
                        }
                        crate::governance::set_category_enabled(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::AddShellDenyPattern => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<AddShellDenyPatternArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Добавить shell-запрет", &args.pattern)
                        {
                            return ToolResult::error(
                                "add_shell_deny_pattern отклонён пользователем",
                            );
                        }
                        crate::governance::add_shell_deny_pattern(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::MemorySnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::memory::memory_snapshot(workspace)
            }
            ToolAction::UpsertTask => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UpsertTaskArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Upsert project memory task", &args.title) {
                            return ToolResult::error("upsert_task отклонён пользователем");
                        }
                        crate::memory::upsert_task(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UpdateTaskStatus => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UpdateTaskStatusArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Update project memory task", &args.id) {
                            return ToolResult::error("update_task_status отклонён пользователем");
                        }
                        crate::memory::update_task_status(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RecordDecision => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RecordDecisionArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Record project decision", &args.title) {
                            return ToolResult::error("record_decision отклонён пользователем");
                        }
                        crate::memory::record_decision(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RecordProjectGoal => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RecordProjectGoalArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Record project goal", &args.title) {
                            return ToolResult::error("record_project_goal отклонён пользователем");
                        }
                        crate::memory::record_project_goal(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::AssetLibrarySnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::asset_library::asset_library_snapshot(workspace)
            }
            ToolAction::TagAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<TagAssetArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Добавить теги ассета", &args.path)
                        {
                            return ToolResult::error("tag_asset отклонён пользователем");
                        }
                        crate::asset_library::tag_asset(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::FavoriteAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<FavoriteAssetArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Обновить избранное ассета", &args.path)
                        {
                            return ToolResult::error("favorite_asset отклонён пользователем");
                        }
                        crate::asset_library::favorite_asset(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ExportAssetPack => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ExportAssetPackArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Экспортировать пак ассетов",
                            "Скопировать выбранные ассеты библиотеки",
                        ) {
                            return ToolResult::error("export_asset_pack отклонён пользователем");
                        }
                        crate::asset_library::export_asset_pack(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RunReplayEval => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RunReplayEvalArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Запустить replay-проверку",
                            "Записать результат проверки",
                        ) {
                            return ToolResult::error("run_replay_eval отклонён пользователем");
                        }
                        crate::evals::run_replay_eval(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::EvalSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::evals::eval_snapshot(workspace)
            }
            ToolAction::ProviderHealthSnapshot => {
                provider_health::provider_health_snapshot(&self.config)
            }
            ToolAction::EnvironmentSnapshot => {
                diagnostics::environment_snapshot(&self.config, self.workspace.as_ref())
            }
        }
    }

    fn approve_write(&self, summary: impl Into<String>, detail: impl Into<String>) -> bool {
        request_approval_if(
            self.policy.require_write_approval,
            &self.events,
            &self.approvals,
            summary,
            detail,
        )
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
