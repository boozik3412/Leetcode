pub mod asset_3d;
pub mod asset_generation;
pub mod desktop;
pub mod filesystem;
pub mod game_workflows;
pub mod mcp;
pub mod orchestration;
pub mod policy;
pub mod project_commands;
pub mod project_preview;
pub mod shell;
pub mod terminal;
pub mod unreal;
pub mod unreal_gameplay;

use crate::agent::types::{ActRequest, AppEvent, ToolAction, ToolCall, ToolResult};
use crate::asset_3d::{
    RefreshThreeDAssetArgs, SubmitThreeDAssetArgs, UnrealImportThreeDArgs, ValidateThreeDAssetArgs,
};
use crate::asset_library::{ExportAssetPackArgs, FavoriteAssetArgs, TagAssetArgs};
use crate::config::AppConfig;
use crate::diagnostics;
use crate::evals::RunReplayEvalArgs;
use crate::game_production::{
    CreateGameProductionPlanArgs, EvaluateProductionGateArgs, UpdateProductionItemArgs,
};
use crate::game_task_builder::{
    EvaluateGameTaskPrerequisitesArgs, GameTaskCatalogSnapshotArgs, PrepareGameTaskProposalArgs,
    ProjectMapReadinessArgs, ProposeProjectRelationArgs, RefreshProjectMapDeepArgs,
    ResolveGameTaskTargetsArgs,
};
use crate::governance::{AddShellDenyPatternArgs, SetCategoryEnabledArgs, SetToolEnabledArgs};
use crate::memory::{
    RecordDecisionArgs, RecordMemorySourceArgs, RecordProjectGoalArgs, RemoveMemorySourceArgs,
    UpdateTaskStatusArgs, UpsertTaskArgs,
};
use crate::project_graph::ProjectGraphSnapshotArgs;
use crate::provider_health;
use crate::roadmap::{
    ExportRoadmapArgs, PlanRoadmapItemArgs, RecordMilestoneArgs, RoadmapSnapshotArgs,
    UpdateRoadmapItemArgs,
};
use crate::self_improvement::{
    ApplySelfImprovementPatchArgs, CleanupSelfImprovementExperimentArgs,
    DecideSelfImprovementExperimentArgs, PrepareSelfImprovementWorktreeArgs,
    PromoteSelfImprovementExperimentArgs, RegisterSelfImprovementBenchmarkArgs,
    RollbackSelfImprovementExperimentArgs, RunSelfImprovementBenchmarksArgs,
    StartSelfImprovementExperimentArgs,
};
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
use crate::tools::mcp::{McpCallArgs, McpDiscoverArgs, McpSnapshotArgs};
use crate::tools::orchestration::{
    CreateReplayEvalArgs, DelegateAgentArgs, RunSubagentArgs, RunSummaryArgs,
    UpdateWorkspaceContextArgs,
};
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::tools::project_commands::ProjectCommandArgs;
use crate::tools::project_preview::OpenProjectPreviewArgs;
use crate::tools::shell::RunShellArgs;
use crate::tools::terminal::{TerminalReadArgs, TerminalStartArgs, TerminalWriteArgs};
use crate::unreal::UnrealCommandArgs;
use crate::unreal_gameplay::{
    ApplyGameplayPlanArgs, CreateGameplayPlanArgs, RunGameplayPlaytestArgs,
};
use crate::vertical_slice::{
    AdvanceVerticalSlicePhaseArgs, EvaluateVerticalSliceReadinessArgs, StartVerticalSliceRunArgs,
};
use crate::visual_regression::VisualSnapshotArgs;
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
        if let Some(workspace) = self.workspace.as_ref() {
            if let Err(error) =
                crate::game_task_builder::validate_tool_action_against_active_manifest(
                    workspace,
                    &request.action,
                    &request.args,
                )
            {
                return ToolResult::error(format!("TaskManifest отклонил действие: {error}"));
            }
        }
        if let Some(workspace) = self.workspace.as_ref() {
            if let Some((experiment_id, status)) =
                crate::self_improvement::active_experiment(workspace)
            {
                if action_is_blocked_by_active_self_improvement(&request.action) {
                    return ToolResult::error(format!(
                        "Основная копия защищена активным self-improvement экспериментом {} ({}). Используйте apply_self_improvement_patch для candidate, затем benchmark, решение и promotion либо отклоните эксперимент.",
                        experiment_id,
                        status.label()
                    ));
                }
            }
        }
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
            ToolAction::UnrealSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::unreal::unreal_snapshot_tool(workspace)
            }
            ToolAction::UnrealCommand => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UnrealCommandArgs>(request.args) {
                    Ok(args) => {
                        unreal::run_unreal_command(
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
            ToolAction::GameProductionSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::game_production::game_production_snapshot(workspace)
            }
            ToolAction::CreateGameProductionPlan => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<CreateGameProductionPlanArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Создать game production plan", &args.title) {
                            return ToolResult::error(
                                "create_game_production_plan отклонён пользователем",
                            );
                        }
                        match crate::game_production::create_game_production_plan(workspace, args) {
                            Ok(plan) => ToolResult::ok(
                                serde_json::to_string_pretty(&plan)
                                    .unwrap_or_else(|_| "production plan создан".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::UpdateProductionItem => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UpdateProductionItemArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Обновить production item",
                            format!("{} / {}", args.plan_id, args.item_id),
                        ) {
                            return ToolResult::error(
                                "update_production_item отклонён пользователем",
                            );
                        }
                        match crate::game_production::update_production_item(workspace, args) {
                            Ok(plan) => ToolResult::ok(
                                serde_json::to_string_pretty(&plan)
                                    .unwrap_or_else(|_| "production item обновлён".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::EvaluateProductionGate => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<EvaluateProductionGateArgs>(request.args) {
                    Ok(args) => {
                        match crate::game_production::evaluate_production_gate(workspace, args) {
                            Ok(report) => ToolResult::ok(
                                serde_json::to_string_pretty(&report)
                                    .unwrap_or_else(|_| "production gate рассчитан".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::VerticalSliceSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::vertical_slice::vertical_slice_snapshot(workspace)
            }
            ToolAction::StartVerticalSliceRun => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<StartVerticalSliceRunArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Запустить Vertical Slice orchestration",
                            args.production_plan_id
                                .as_deref()
                                .unwrap_or("активный production plan"),
                        ) {
                            return ToolResult::error(
                                "start_vertical_slice_run отклонён пользователем",
                            );
                        }
                        match crate::vertical_slice::start_vertical_slice_run(workspace, args) {
                            Ok(run) => ToolResult::ok(
                                serde_json::to_string_pretty(&run)
                                    .unwrap_or_else(|_| "vertical slice run создан".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::AdvanceVerticalSlicePhase => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<AdvanceVerticalSlicePhaseArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Обновить фазу Vertical Slice",
                            format!(
                                "{} / {} / {}",
                                args.run_id,
                                args.phase.label(),
                                args.status.label()
                            ),
                        ) {
                            return ToolResult::error(
                                "advance_vertical_slice_phase отклонён пользователем",
                            );
                        }
                        match crate::vertical_slice::advance_vertical_slice_phase(workspace, args) {
                            Ok(run) => {
                                ToolResult::ok(serde_json::to_string_pretty(&run).unwrap_or_else(
                                    |_| "vertical slice phase обновлена".to_string(),
                                ))
                            }
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::EvaluateVerticalSliceReadiness => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<EvaluateVerticalSliceReadinessArgs>(request.args) {
                    Ok(args) => match crate::vertical_slice::evaluate_vertical_slice_readiness(
                        workspace, args,
                    ) {
                        Ok(report) => {
                            ToolResult::ok(serde_json::to_string_pretty(&report).unwrap_or_else(
                                |_| "vertical slice readiness рассчитан".to_string(),
                            ))
                        }
                        Err(error) => ToolResult::error(error.to_string()),
                    },
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::GameplaySnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                unreal_gameplay::snapshot(workspace)
            }
            ToolAction::CreateGameplayPlan => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<CreateGameplayPlanArgs>(request.args) {
                    Ok(args) => unreal_gameplay::create_plan(
                        workspace,
                        args,
                        &self.events,
                        &self.approvals,
                        &self.policy,
                    ),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ApplyGameplayPlan => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ApplyGameplayPlanArgs>(request.args) {
                    Ok(args) => {
                        unreal_gameplay::apply_plan(
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
            ToolAction::RunGameplayPlaytest => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RunGameplayPlaytestArgs>(request.args) {
                    Ok(args) => {
                        unreal_gameplay::run_playtest(
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
            ToolAction::McpSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<McpSnapshotArgs>(request.args) {
                    Ok(_) => mcp::snapshot(workspace),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::McpDiscover => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<McpDiscoverArgs>(request.args) {
                    Ok(args) => {
                        mcp::discover(
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
            ToolAction::McpCall => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<McpCallArgs>(request.args) {
                    Ok(args) => {
                        mcp::call(
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
            ToolAction::Asset3dSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Workspace is not selected");
                };
                asset_3d::snapshot(workspace)
            }
            ToolAction::Submit3dAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Workspace is not selected");
                };
                match serde_json::from_value::<SubmitThreeDAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_3d::submit(
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
            ToolAction::Refresh3dAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Workspace is not selected");
                };
                match serde_json::from_value::<RefreshThreeDAssetArgs>(request.args) {
                    Ok(args) => {
                        asset_3d::refresh(
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
            ToolAction::Validate3dAsset => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Workspace is not selected");
                };
                match serde_json::from_value::<ValidateThreeDAssetArgs>(request.args) {
                    Ok(args) => asset_3d::validate(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::Import3dAssetUnreal => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Workspace is not selected");
                };
                match serde_json::from_value::<UnrealImportThreeDArgs>(request.args) {
                    Ok(args) => {
                        asset_3d::import_unreal(
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
            ToolAction::RecordMemorySource => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RecordMemorySourceArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Record project memory source", &args.title) {
                            return ToolResult::error(
                                "record_memory_source отклонён пользователем",
                            );
                        }
                        crate::memory::record_memory_source(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RemoveMemorySource => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RemoveMemorySourceArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Remove project memory source", &args.id) {
                            return ToolResult::error(
                                "remove_memory_source отклонён пользователем",
                            );
                        }
                        crate::memory::remove_memory_source(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ProjectGraphSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ProjectGraphSnapshotArgs>(request.args) {
                    Ok(args) => crate::project_graph::project_graph_snapshot(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ProjectMapReadiness => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ProjectMapReadinessArgs>(request.args) {
                    Ok(args) => ToolResult::ok(
                        serde_json::to_string_pretty(
                            &crate::game_task_builder::project_map_readiness(
                                workspace,
                                args.refresh_if_stale,
                            ),
                        )
                        .unwrap_or_else(|_| "Project Map readiness".to_string()),
                    ),
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::RefreshProjectMapDeep => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RefreshProjectMapDeepArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_shell(
                            "Выполнить глубокий Project Map scan",
                            "UnrealEditor-Cmd будет запущен в headless-режиме; Asset Registry и зависимости будут импортированы в Project Map.",
                        ) {
                            return ToolResult::error(
                                "refresh_project_map_deep отклонён пользователем",
                            );
                        }
                        match crate::game_task_builder::refresh_project_map_deep(workspace, args) {
                            Ok(report) => ToolResult::ok(
                                serde_json::to_string_pretty(&report)
                                    .unwrap_or_else(|_| "Project Map обновлена".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::GameTaskCatalogSnapshot => {
                match serde_json::from_value::<GameTaskCatalogSnapshotArgs>(request.args) {
                    Ok(args) => crate::game_task_builder::game_task_catalog_snapshot(&args),
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::ResolveGameTaskTargets => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ResolveGameTaskTargetsArgs>(request.args) {
                    Ok(args) => {
                        match crate::game_task_builder::resolve_game_task_targets(workspace, &args)
                        {
                            Ok(report) => ToolResult::ok(
                                serde_json::to_string_pretty(&report)
                                    .unwrap_or_else(|_| "Цели разрешены".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::EvaluateGameTaskPrerequisites => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<EvaluateGameTaskPrerequisitesArgs>(request.args) {
                    Ok(args) => match crate::game_task_builder::evaluate_game_task_prerequisites(
                        workspace, &args,
                    ) {
                        Ok(report) => ToolResult::ok(
                            serde_json::to_string_pretty(&report)
                                .unwrap_or_else(|_| "Диагностика завершена".to_string()),
                        ),
                        Err(error) => ToolResult::error(error.to_string()),
                    },
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::PrepareGameTaskProposal => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<PrepareGameTaskProposalArgs>(request.args) {
                    Ok(args) => {
                        match crate::game_task_builder::prepare_game_task_proposal(workspace, args)
                        {
                            Ok(session) => ToolResult::ok(
                                serde_json::to_string_pretty(&session)
                                    .unwrap_or_else(|_| "Предложение подготовлено".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::ProposeProjectRelation => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ProposeProjectRelationArgs>(request.args) {
                    Ok(args) => {
                        match crate::game_task_builder::propose_project_relation(workspace, args) {
                            Ok(proposal) => ToolResult::ok(
                                serde_json::to_string_pretty(&proposal)
                                    .unwrap_or_else(|_| "Связь предложена".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::GameTaskSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::game_task_builder::game_task_snapshot(workspace)
            }
            ToolAction::RoadmapSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RoadmapSnapshotArgs>(request.args) {
                    Ok(args) => crate::roadmap::roadmap_snapshot(workspace, args),
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RecordMilestone => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RecordMilestoneArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Record roadmap milestone", &args.title) {
                            return ToolResult::error("record_milestone отклонён пользователем");
                        }
                        crate::roadmap::record_milestone(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::UpdateRoadmapItem => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<UpdateRoadmapItemArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Update roadmap item", &args.id) {
                            return ToolResult::error("update_roadmap_item отклонён пользователем");
                        }
                        crate::roadmap::update_roadmap_item(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::PlanRoadmapItem => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<PlanRoadmapItemArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write("Plan roadmap item", &args.title) {
                            return ToolResult::error("plan_roadmap_item отклонён пользователем");
                        }
                        crate::roadmap::plan_roadmap_item(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ExportRoadmap => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ExportRoadmapArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Export roadmap",
                            "Записать markdown-снимок дорожной карты",
                        ) {
                            return ToolResult::error("export_roadmap отклонён пользователем");
                        }
                        crate::roadmap::export_roadmap(workspace, args)
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
            ToolAction::SelfImprovementSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::self_improvement::self_improvement_snapshot(workspace)
            }
            ToolAction::StartSelfImprovementExperiment => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<StartSelfImprovementExperimentArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Начать эксперимент самоулучшения",
                            "Записать гипотезу, критерии успеха и baseline проекта",
                        ) {
                            return ToolResult::error(
                                "start_self_improvement_experiment отклонён пользователем",
                            );
                        }
                        crate::self_improvement::start_self_improvement_experiment(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::DecideSelfImprovementExperiment => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<DecideSelfImprovementExperimentArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Зафиксировать решение по самоулучшению",
                            format!("Эксперимент: {}", args.experiment_id),
                        ) {
                            return ToolResult::error(
                                "decide_self_improvement_experiment отклонён пользователем",
                            );
                        }
                        crate::self_improvement::decide_self_improvement_experiment(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::PrepareSelfImprovementWorktree => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<PrepareSelfImprovementWorktreeArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_shell(
                            "Создать изолированный worktree",
                            format!("Эксперимент: {}", args.experiment_id),
                        ) {
                            return ToolResult::error(
                                "prepare_self_improvement_worktree отклонён пользователем",
                            );
                        }
                        crate::self_improvement::prepare_self_improvement_worktree(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ApplySelfImprovementPatch => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<ApplySelfImprovementPatchArgs>(request.args) {
                    Ok(args) => {
                        let candidate = match crate::self_improvement::worktree_workspace(
                            workspace,
                            &args.experiment_id,
                        ) {
                            Ok(candidate) => candidate,
                            Err(err) => return ToolResult::error(err.to_string()),
                        };
                        let result = filesystem::apply_patch(
                            &candidate,
                            ApplyPatchArgs { patch: args.patch },
                            &self.events,
                            &self.approvals,
                            &self.policy,
                        );
                        if result.ok {
                            if let Err(err) = crate::self_improvement::record_worktree_changes(
                                workspace,
                                &args.experiment_id,
                            ) {
                                return ToolResult::error(format!(
                                    "patch применён, но состояние эксперимента не обновлено: {err}"
                                ));
                            }
                        }
                        result
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RegisterSelfImprovementBenchmark => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RegisterSelfImprovementBenchmarkArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Сохранить benchmark самоулучшения",
                            format!("{}: {}", args.id, args.command),
                        ) {
                            return ToolResult::error(
                                "register_self_improvement_benchmark отклонён пользователем",
                            );
                        }
                        crate::self_improvement::register_self_improvement_benchmark(
                            workspace, args,
                        )
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RunSelfImprovementBenchmarks => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RunSelfImprovementBenchmarksArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_shell(
                            "Запустить baseline/candidate benchmarks",
                            format!("Эксперимент: {}", args.experiment_id),
                        ) {
                            return ToolResult::error(
                                "run_self_improvement_benchmarks отклонён пользователем",
                            );
                        }
                        crate::self_improvement::run_self_improvement_benchmarks(workspace, args)
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::PromoteSelfImprovementExperiment => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<PromoteSelfImprovementExperimentArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_shell(
                            "Продвинуть принятый эксперимент",
                            format!("Fast-forward merge эксперимента {}", args.experiment_id),
                        ) {
                            return ToolResult::error(
                                "promote_self_improvement_experiment отклонён пользователем",
                            );
                        }
                        crate::self_improvement::promote_self_improvement_experiment(
                            workspace, args,
                        )
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::RollbackSelfImprovementExperiment => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<RollbackSelfImprovementExperimentArgs>(request.args)
                {
                    Ok(args) => {
                        if !self.approve_shell(
                            "Откатить продвинутый эксперимент",
                            format!("Создать git revert для {}", args.experiment_id),
                        ) {
                            return ToolResult::error(
                                "rollback_self_improvement_experiment отклонён пользователем",
                            );
                        }
                        crate::self_improvement::rollback_self_improvement_experiment(
                            workspace, args,
                        )
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::CleanupSelfImprovementExperiment => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<CleanupSelfImprovementExperimentArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_shell(
                            "Очистить worktree эксперимента",
                            format!("Эксперимент: {}", args.experiment_id),
                        ) {
                            return ToolResult::error(
                                "cleanup_self_improvement_experiment отклонён пользователем",
                            );
                        }
                        crate::self_improvement::cleanup_self_improvement_experiment(
                            workspace, args,
                        )
                    }
                    Err(err) => ToolResult::error(err.to_string()),
                }
            }
            ToolAction::ProductionValidationSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::production_validation::production_validation_tool(workspace, &self.config)
            }
            ToolAction::UpdateProjectMapGolden => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                if !self.approve_write(
                    "Обновить эталон Project Map",
                    "Текущая структура проекта станет новым golden snapshot после ревью.",
                ) {
                    return ToolResult::error("update_project_map_golden отклонён пользователем");
                }
                match crate::production_validation::update_project_map_golden(workspace) {
                    Ok(golden) => ToolResult::ok(
                        serde_json::to_string_pretty(&golden)
                            .unwrap_or_else(|_| "Project Map golden обновлён".to_string()),
                    ),
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::VisualRegressionSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                crate::visual_regression::visual_regression_snapshot(workspace)
            }
            ToolAction::RecordVisualBaseline => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<VisualSnapshotArgs>(request.args) {
                    Ok(args) => {
                        if !self.approve_write(
                            "Обновить визуальный эталон",
                            format!("Сценарий: {}\nСнимок: {}", args.scenario.label(), args.path),
                        ) {
                            return ToolResult::error(
                                "record_visual_baseline отклонён пользователем",
                            );
                        }
                        match crate::visual_regression::record_visual_baseline(workspace, &args) {
                            Ok(baseline) => ToolResult::ok(
                                serde_json::to_string_pretty(&baseline)
                                    .unwrap_or_else(|_| "Визуальный эталон сохранён".to_string()),
                            ),
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
            }
            ToolAction::CompareVisualSnapshot => {
                let Some(workspace) = &self.workspace else {
                    return ToolResult::error("Рабочая папка не выбрана");
                };
                match serde_json::from_value::<VisualSnapshotArgs>(request.args) {
                    Ok(args) => {
                        match crate::visual_regression::compare_visual_snapshot(workspace, &args) {
                            Ok(comparison) => {
                                let rendered = serde_json::to_string_pretty(&comparison)
                                    .unwrap_or_else(|_| {
                                        "Визуальное сравнение завершено".to_string()
                                    });
                                if comparison.passed {
                                    ToolResult::ok(rendered)
                                } else {
                                    ToolResult::error(rendered)
                                }
                            }
                            Err(error) => ToolResult::error(error.to_string()),
                        }
                    }
                    Err(error) => ToolResult::error(error.to_string()),
                }
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

    fn approve_shell(&self, summary: impl Into<String>, detail: impl Into<String>) -> bool {
        request_approval_if(
            self.policy.require_shell_approval,
            &self.events,
            &self.approvals,
            summary,
            detail,
        )
    }
}

fn action_is_blocked_by_active_self_improvement(action: &ToolAction) -> bool {
    !matches!(
        action,
        ToolAction::ListFiles
            | ToolAction::ReadFile
            | ToolAction::Grep
            | ToolAction::OpenProjectPreview
            | ToolAction::OrchestrationSnapshot
            | ToolAction::TerminalRead
            | ToolAction::TerminalStop
            | ToolAction::TerminalClear
            | ToolAction::Screenshot
            | ToolAction::ActiveWindow
            | ToolAction::FocusWindow
            | ToolAction::GovernanceSnapshot
            | ToolAction::MemorySnapshot
            | ToolAction::ProjectGraphSnapshot
            | ToolAction::ProjectMapReadiness
            | ToolAction::GameTaskCatalogSnapshot
            | ToolAction::ResolveGameTaskTargets
            | ToolAction::EvaluateGameTaskPrerequisites
            | ToolAction::GameTaskSnapshot
            | ToolAction::UnrealSnapshot
            | ToolAction::GameProductionSnapshot
            | ToolAction::EvaluateProductionGate
            | ToolAction::VerticalSliceSnapshot
            | ToolAction::EvaluateVerticalSliceReadiness
            | ToolAction::GameplaySnapshot
            | ToolAction::McpSnapshot
            | ToolAction::RoadmapSnapshot
            | ToolAction::AssetLibrarySnapshot
            | ToolAction::EvalSnapshot
            | ToolAction::SelfImprovementSnapshot
            | ToolAction::StartSelfImprovementExperiment
            | ToolAction::DecideSelfImprovementExperiment
            | ToolAction::PrepareSelfImprovementWorktree
            | ToolAction::ApplySelfImprovementPatch
            | ToolAction::RegisterSelfImprovementBenchmark
            | ToolAction::RunSelfImprovementBenchmarks
            | ToolAction::PromoteSelfImprovementExperiment
            | ToolAction::RollbackSelfImprovementExperiment
            | ToolAction::CleanupSelfImprovementExperiment
            | ToolAction::VisualRegressionSnapshot
            | ToolAction::CompareVisualSnapshot
            | ToolAction::ProviderHealthSnapshot
            | ToolAction::EnvironmentSnapshot
            | ToolAction::OpenAssetFolder
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_self_improvement_blocks_main_mutations_only() {
        assert!(action_is_blocked_by_active_self_improvement(
            &ToolAction::WriteFile
        ));
        assert!(action_is_blocked_by_active_self_improvement(
            &ToolAction::RunShell
        ));
        assert!(action_is_blocked_by_active_self_improvement(
            &ToolAction::DesktopStep
        ));
        assert!(!action_is_blocked_by_active_self_improvement(
            &ToolAction::ReadFile
        ));
        assert!(!action_is_blocked_by_active_self_improvement(
            &ToolAction::ApplySelfImprovementPatch
        ));
        assert!(!action_is_blocked_by_active_self_improvement(
            &ToolAction::PromoteSelfImprovementExperiment
        ));
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
