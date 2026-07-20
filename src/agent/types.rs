use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatLine {
    pub role: ChatRole,
    pub content: String,
    #[serde(default)]
    pub elapsed: Option<String>,
}

impl ChatLine {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            elapsed: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            elapsed: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
            elapsed: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolLogLine {
    pub title: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    AssistantText(String),
    AssistantDelta(String),
    ToolStarted {
        id: String,
        name: String,
        summary: String,
    },
    ToolOutput {
        id: String,
        chunk: String,
    },
    ToolFinished {
        id: String,
        output: String,
    },
    ApprovalRequested {
        id: String,
        summary: String,
        detail: String,
    },
    Error(String),
    Done,
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActRequest {
    pub action: ToolAction,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAction {
    ListFiles,
    ReadFile,
    WriteFile,
    EditFile,
    ApplyPatch,
    Grep,
    ProjectCommand,
    UnrealSnapshot,
    UnrealCommand,
    GameProductionSnapshot,
    CreateGameProductionPlan,
    UpdateProductionItem,
    EvaluateProductionGate,
    VerticalSliceSnapshot,
    StartVerticalSliceRun,
    AdvanceVerticalSlicePhase,
    EvaluateVerticalSliceReadiness,
    GameplaySnapshot,
    CreateGameplayPlan,
    ApplyGameplayPlan,
    RunGameplayPlaytest,
    McpSnapshot,
    McpDiscover,
    McpCall,
    GameWorkflow,
    OpenProjectPreview,
    RunSubagent,
    DelegateAgent,
    UpdateWorkspaceContext,
    RecordRunSummary,
    ExportTrace,
    CreateReplayEval,
    OrchestrationSnapshot,
    RunShell,
    TerminalStart,
    TerminalWrite,
    TerminalRead,
    TerminalStop,
    TerminalClear,
    GenerateImageAsset,
    GenerateSpritesheetAsset,
    GenerateAudioAsset,
    GenerateVideoAsset,
    #[serde(rename = "asset_3d_snapshot")]
    Asset3dSnapshot,
    #[serde(rename = "submit_3d_asset")]
    Submit3dAsset,
    #[serde(rename = "refresh_3d_asset")]
    Refresh3dAsset,
    #[serde(rename = "validate_3d_asset")]
    Validate3dAsset,
    #[serde(rename = "import_3d_asset_unreal")]
    Import3dAssetUnreal,
    RegenerateImageAsset,
    VaryImageAsset,
    UpscaleAsset,
    ExportAsset,
    AttachAsset,
    UseAssetAsAppIcon,
    OpenAssetFolder,
    Screenshot,
    ActiveWindow,
    FocusWindow,
    DesktopStep,
    MouseClick,
    TypeText,
    Hotkey,
    GovernanceSnapshot,
    SetToolEnabled,
    SetCategoryEnabled,
    AddShellDenyPattern,
    MemorySnapshot,
    UpsertTask,
    UpdateTaskStatus,
    RecordDecision,
    RecordProjectGoal,
    RecordMemorySource,
    RemoveMemorySource,
    ProjectGraphSnapshot,
    ProjectMapReadiness,
    RefreshProjectMapDeep,
    GameTaskCatalogSnapshot,
    ResolveGameTaskTargets,
    EvaluateGameTaskPrerequisites,
    PrepareGameTaskProposal,
    ProposeProjectRelation,
    GameTaskSnapshot,
    SemanticCatalogSnapshot,
    AnalyzeProjectSemantics,
    SemanticNodeSnapshot,
    ResolveSemanticTargets,
    ProposeSemanticLabels,
    DecideSemanticProposals,
    UpdateSemanticLabels,
    ExportSemanticIndex,
    RoadmapSnapshot,
    RecordMilestone,
    UpdateRoadmapItem,
    PlanRoadmapItem,
    ExportRoadmap,
    AssetLibrarySnapshot,
    TagAsset,
    FavoriteAsset,
    ExportAssetPack,
    RunReplayEval,
    EvalSnapshot,
    SelfImprovementSnapshot,
    StartSelfImprovementExperiment,
    DecideSelfImprovementExperiment,
    PrepareSelfImprovementWorktree,
    ApplySelfImprovementPatch,
    RegisterSelfImprovementBenchmark,
    RunSelfImprovementBenchmarks,
    PromoteSelfImprovementExperiment,
    RollbackSelfImprovementExperiment,
    CleanupSelfImprovementExperiment,
    ProductionValidationSnapshot,
    UpdateProjectMapGolden,
    VisualRegressionSnapshot,
    RecordVisualBaseline,
    CompareVisualSnapshot,
    ProviderHealthSnapshot,
    EnvironmentSnapshot,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            ok: true,
            output: output.into(),
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            ok: false,
            output: output.into(),
        }
    }

    pub fn as_model_output(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            "{\"ok\":false,\"output\":\"не удалось сериализовать результат инструмента\"}"
                .to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generate_image_asset_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_image_asset","args":{"prompt":"pixel art chest"}}"#,
        )
        .expect("valid act request");

        assert!(matches!(request.action, ToolAction::GenerateImageAsset));
    }

    #[test]
    fn parses_use_asset_as_app_icon_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"use_asset_as_app_icon","args":{"source_path":"assets/generated/images/icon.png"}}"#,
        )
        .expect("valid act request");

        assert!(matches!(request.action, ToolAction::UseAssetAsAppIcon));
    }

    #[test]
    fn parses_open_asset_folder_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"open_asset_folder","args":{"path":"assets/generated/images"}}"#,
        )
        .expect("valid act request");

        assert!(matches!(request.action, ToolAction::OpenAssetFolder));
    }

    #[test]
    fn parses_mcp_call_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"mcp_call","args":{"server":"unreal-mcp","tool":"list_toolsets","arguments":{}}}"#,
        )
        .expect("valid MCP act request");

        assert!(matches!(request.action, ToolAction::McpCall));
    }

    #[test]
    fn parses_unreal_gameplay_actions() {
        let snapshot =
            serde_json::from_str::<ActRequest>(r#"{"action":"gameplay_snapshot","args":{}}"#)
                .expect("valid gameplay snapshot request");
        let plan = serde_json::from_str::<ActRequest>(
            r#"{"action":"create_gameplay_plan","args":{"recipe":"interaction","brief":"door interaction"}}"#,
        )
        .expect("valid gameplay plan request");

        assert!(matches!(snapshot.action, ToolAction::GameplaySnapshot));
        assert!(matches!(plan.action, ToolAction::CreateGameplayPlan));
    }

    #[test]
    fn parses_image_job_followup_actions() {
        let regenerate = serde_json::from_str::<ActRequest>(
            r#"{"action":"regenerate_image_asset","args":{"job_id":"img-1"}}"#,
        )
        .expect("valid regenerate request");
        let variation = serde_json::from_str::<ActRequest>(
            r#"{"action":"vary_image_asset","args":{"job_id":"img-1"}}"#,
        )
        .expect("valid variation request");

        assert!(matches!(
            regenerate.action,
            ToolAction::RegenerateImageAsset
        ));
        assert!(matches!(variation.action, ToolAction::VaryImageAsset));
    }

    #[test]
    fn parses_desktop_control_actions() {
        let active = serde_json::from_str::<ActRequest>(r#"{"action":"active_window","args":{}}"#)
            .expect("valid active window request");
        let focus = serde_json::from_str::<ActRequest>(
            r#"{"action":"focus_window","args":{"title":"Leetcode"}}"#,
        )
        .expect("valid focus request");
        let step = serde_json::from_str::<ActRequest>(
            r#"{"action":"desktop_step","args":{"action":"observe"}}"#,
        )
        .expect("valid desktop step request");
        let click = serde_json::from_str::<ActRequest>(
            r#"{"action":"mouse_click","args":{"x":100,"y":200}}"#,
        )
        .expect("valid click request");
        let typing =
            serde_json::from_str::<ActRequest>(r#"{"action":"type_text","args":{"text":"hello"}}"#)
                .expect("valid type request");
        let hotkey = serde_json::from_str::<ActRequest>(
            r#"{"action":"hotkey","args":{"keys":["ctrl","l"]}}"#,
        )
        .expect("valid hotkey request");

        assert!(matches!(active.action, ToolAction::ActiveWindow));
        assert!(matches!(focus.action, ToolAction::FocusWindow));
        assert!(matches!(step.action, ToolAction::DesktopStep));
        assert!(matches!(click.action, ToolAction::MouseClick));
        assert!(matches!(typing.action, ToolAction::TypeText));
        assert!(matches!(hotkey.action, ToolAction::Hotkey));
    }

    #[test]
    fn parses_project_command_action() {
        let request = serde_json::from_str::<ActRequest>(
            r#"{"action":"project_command","args":{"command":"check","profile":"rust"}}"#,
        )
        .expect("valid project command request");

        assert!(matches!(request.action, ToolAction::ProjectCommand));
    }

    #[test]
    fn parses_terminal_actions() {
        let start = serde_json::from_str::<ActRequest>(
            r#"{"action":"terminal_start","args":{"cwd":".","shell":"powershell"}}"#,
        )
        .expect("valid terminal start");
        let write = serde_json::from_str::<ActRequest>(
            r#"{"action":"terminal_write","args":{"input":"cargo check","enter":true}}"#,
        )
        .expect("valid terminal write");
        let read =
            serde_json::from_str::<ActRequest>(r#"{"action":"terminal_read","args":{"lines":50}}"#)
                .expect("valid terminal read");
        let stop = serde_json::from_str::<ActRequest>(r#"{"action":"terminal_stop","args":{}}"#)
            .expect("valid terminal stop");
        let clear = serde_json::from_str::<ActRequest>(r#"{"action":"terminal_clear","args":{}}"#)
            .expect("valid terminal clear");

        assert!(matches!(start.action, ToolAction::TerminalStart));
        assert!(matches!(write.action, ToolAction::TerminalWrite));
        assert!(matches!(read.action, ToolAction::TerminalRead));
        assert!(matches!(stop.action, ToolAction::TerminalStop));
        assert!(matches!(clear.action, ToolAction::TerminalClear));
    }

    #[test]
    fn parses_game_workflow_and_preview_actions() {
        let workflow = serde_json::from_str::<ActRequest>(
            r#"{"action":"game_workflow","args":{"workflow":"prototype_mechanic","title":"Dash","brief":"fast movement"}}"#,
        )
        .expect("valid workflow request");
        let preview = serde_json::from_str::<ActRequest>(
            r#"{"action":"open_project_preview","args":{"preview":"dev-server"}}"#,
        )
        .expect("valid preview request");

        assert!(matches!(workflow.action, ToolAction::GameWorkflow));
        assert!(matches!(preview.action, ToolAction::OpenProjectPreview));
    }

    #[test]
    fn parses_orchestration_actions() {
        let handoff = serde_json::from_str::<ActRequest>(
            r#"{"action":"delegate_agent","args":{"role":"qa","task":"test combat loop"}}"#,
        )
        .expect("valid handoff request");
        let subagent = serde_json::from_str::<ActRequest>(
            r#"{"action":"run_subagent","args":{"role":"code_agent","task":"inspect parser","max_rounds":3}}"#,
        )
        .expect("valid subagent request");
        let context = serde_json::from_str::<ActRequest>(
            r#"{"action":"update_workspace_context","args":{"summary":"prototype","decisions":["ship demo"]}}"#,
        )
        .expect("valid context request");
        let summary = serde_json::from_str::<ActRequest>(
            r#"{"action":"record_run_summary","args":{"summary":"implemented first pass"}}"#,
        )
        .expect("valid run summary request");
        let trace = serde_json::from_str::<ActRequest>(r#"{"action":"export_trace","args":{}}"#)
            .expect("valid trace request");
        let eval = serde_json::from_str::<ActRequest>(
            r#"{"action":"create_replay_eval","args":{"name":"asset flow","prompt":"make icon"}}"#,
        )
        .expect("valid eval request");
        let snapshot =
            serde_json::from_str::<ActRequest>(r#"{"action":"orchestration_snapshot","args":{}}"#)
                .expect("valid snapshot request");

        assert!(matches!(handoff.action, ToolAction::DelegateAgent));
        assert!(matches!(subagent.action, ToolAction::RunSubagent));
        assert!(matches!(context.action, ToolAction::UpdateWorkspaceContext));
        assert!(matches!(summary.action, ToolAction::RecordRunSummary));
        assert!(matches!(trace.action, ToolAction::ExportTrace));
        assert!(matches!(eval.action, ToolAction::CreateReplayEval));
        assert!(matches!(snapshot.action, ToolAction::OrchestrationSnapshot));
    }

    #[test]
    fn parses_expanded_asset_actions() {
        let spritesheet = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_spritesheet_asset","args":{"prompt":"hero run cycle"}}"#,
        )
        .expect("valid spritesheet request");
        let audio = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_audio_asset","args":{"prompt":"coin pickup sfx"}}"#,
        )
        .expect("valid audio request");
        let video = serde_json::from_str::<ActRequest>(
            r#"{"action":"generate_video_asset","args":{"prompt":"game trailer shot"}}"#,
        )
        .expect("valid video request");
        let upscale = serde_json::from_str::<ActRequest>(
            r#"{"action":"upscale_asset","args":{"source_path":"assets/generated/images/a.png"}}"#,
        )
        .expect("valid upscale request");
        let export = serde_json::from_str::<ActRequest>(
            r#"{"action":"export_asset","args":{"source_path":"assets/generated/images/a.png"}}"#,
        )
        .expect("valid export request");
        let attach = serde_json::from_str::<ActRequest>(
            r#"{"action":"attach_asset","args":{"source_path":"assets/generated/images/a.png"}}"#,
        )
        .expect("valid attach request");

        assert!(matches!(
            spritesheet.action,
            ToolAction::GenerateSpritesheetAsset
        ));
        assert!(matches!(audio.action, ToolAction::GenerateAudioAsset));
        assert!(matches!(video.action, ToolAction::GenerateVideoAsset));
        assert!(matches!(upscale.action, ToolAction::UpscaleAsset));
        assert!(matches!(export.action, ToolAction::ExportAsset));
        assert!(matches!(attach.action, ToolAction::AttachAsset));
    }

    #[test]
    fn parses_governance_memory_eval_and_health_actions() {
        for (json, expected) in [
            (
                r#"{"action":"governance_snapshot","args":{}}"#,
                "governance",
            ),
            (
                r#"{"action":"set_tool_enabled","args":{"tool":"run_shell","enabled":false}}"#,
                "governance",
            ),
            (r#"{"action":"memory_snapshot","args":{}}"#, "memory"),
            (
                r#"{"action":"upsert_task","args":{"title":"Ship MVP"}}"#,
                "memory",
            ),
            (
                r#"{"action":"record_memory_source","args":{"title":"Brief","content":"Use cozy art direction."}}"#,
                "memory",
            ),
            (
                r#"{"action":"project_graph_snapshot","args":{"save_if_missing":true,"refresh":true}}"#,
                "graph",
            ),
            (r#"{"action":"roadmap_snapshot","args":{}}"#, "roadmap"),
            (
                r#"{"action":"record_milestone","args":{"title":"Stage 22","detail":"Roadmap module","status":"done"}}"#,
                "roadmap",
            ),
            (
                r#"{"action":"update_roadmap_item","args":{"id":"stage-22","status":"done"}}"#,
                "roadmap",
            ),
            (
                r#"{"action":"plan_roadmap_item","args":{"title":"Stage 23"}}"#,
                "roadmap",
            ),
            (r#"{"action":"export_roadmap","args":{}}"#, "roadmap"),
            (r#"{"action":"asset_library_snapshot","args":{}}"#, "assets"),
            (r#"{"action":"run_replay_eval","args":{}}"#, "evals"),
            (
                r#"{"action":"provider_health_snapshot","args":{}}"#,
                "providers",
            ),
            (
                r#"{"action":"environment_snapshot","args":{}}"#,
                "environment",
            ),
            (
                r#"{"action":"self_improvement_snapshot","args":{}}"#,
                "self_improvement",
            ),
        ] {
            let request = serde_json::from_str::<ActRequest>(json).expect(expected);
            match expected {
                "governance" => assert!(matches!(
                    request.action,
                    ToolAction::GovernanceSnapshot | ToolAction::SetToolEnabled
                )),
                "memory" => assert!(matches!(
                    request.action,
                    ToolAction::MemorySnapshot
                        | ToolAction::UpsertTask
                        | ToolAction::RecordMemorySource
                )),
                "graph" => assert!(matches!(request.action, ToolAction::ProjectGraphSnapshot)),
                "roadmap" => assert!(matches!(
                    request.action,
                    ToolAction::RoadmapSnapshot
                        | ToolAction::RecordMilestone
                        | ToolAction::UpdateRoadmapItem
                        | ToolAction::PlanRoadmapItem
                        | ToolAction::ExportRoadmap
                )),
                "assets" => assert!(matches!(request.action, ToolAction::AssetLibrarySnapshot)),
                "evals" => assert!(matches!(request.action, ToolAction::RunReplayEval)),
                "providers" => {
                    assert!(matches!(request.action, ToolAction::ProviderHealthSnapshot))
                }
                "environment" => {
                    assert!(matches!(request.action, ToolAction::EnvironmentSnapshot))
                }
                "self_improvement" => {
                    assert!(matches!(
                        request.action,
                        ToolAction::SelfImprovementSnapshot
                    ))
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn parses_production_validation_actions() {
        for (json, expected) in [
            (
                r#"{"action":"production_validation_snapshot","args":{}}"#,
                "production",
            ),
            (
                r#"{"action":"update_project_map_golden","args":{}}"#,
                "golden",
            ),
            (
                r#"{"action":"visual_regression_snapshot","args":{}}"#,
                "visual_snapshot",
            ),
            (
                r#"{"action":"record_visual_baseline","args":{"scenario":"desktop_main","path":"screens/main.png"}}"#,
                "visual_record",
            ),
            (
                r#"{"action":"compare_visual_snapshot","args":{"scenario":"remote_client","path":"screens/client.png"}}"#,
                "visual_compare",
            ),
        ] {
            let request = serde_json::from_str::<ActRequest>(json).expect(expected);
            match expected {
                "production" => assert!(matches!(
                    request.action,
                    ToolAction::ProductionValidationSnapshot
                )),
                "golden" => assert!(matches!(request.action, ToolAction::UpdateProjectMapGolden)),
                "visual_snapshot" => assert!(matches!(
                    request.action,
                    ToolAction::VisualRegressionSnapshot
                )),
                "visual_record" => {
                    assert!(matches!(request.action, ToolAction::RecordVisualBaseline))
                }
                "visual_compare" => {
                    assert!(matches!(request.action, ToolAction::CompareVisualSnapshot))
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn parses_game_production_actions() {
        for action in [
            "game_production_snapshot",
            "create_game_production_plan",
            "update_production_item",
            "evaluate_production_gate",
        ] {
            let request = serde_json::from_value::<ActRequest>(serde_json::json!({
                "action": action,
                "args": {}
            }))
            .expect(action);
            assert!(matches!(
                request.action,
                ToolAction::GameProductionSnapshot
                    | ToolAction::CreateGameProductionPlan
                    | ToolAction::UpdateProductionItem
                    | ToolAction::EvaluateProductionGate
            ));
        }
    }

    #[test]
    fn parses_vertical_slice_orchestrator_actions() {
        for action in [
            "vertical_slice_snapshot",
            "start_vertical_slice_run",
            "advance_vertical_slice_phase",
            "evaluate_vertical_slice_readiness",
        ] {
            let request = serde_json::from_value::<ActRequest>(serde_json::json!({
                "action": action,
                "args": {}
            }))
            .expect(action);
            assert!(matches!(
                request.action,
                ToolAction::VerticalSliceSnapshot
                    | ToolAction::StartVerticalSliceRun
                    | ToolAction::AdvanceVerticalSlicePhase
                    | ToolAction::EvaluateVerticalSliceReadiness
            ));
        }
    }

    #[test]
    fn parses_game_task_builder_actions() {
        for action in [
            "project_map_readiness",
            "refresh_project_map_deep",
            "game_task_catalog_snapshot",
            "resolve_game_task_targets",
            "evaluate_game_task_prerequisites",
            "prepare_game_task_proposal",
            "propose_project_relation",
            "game_task_snapshot",
            "semantic_catalog_snapshot",
            "analyze_project_semantics",
            "semantic_node_snapshot",
            "resolve_semantic_targets",
            "propose_semantic_labels",
            "decide_semantic_proposals",
            "update_semantic_labels",
            "export_semantic_index",
        ] {
            serde_json::from_value::<ActRequest>(serde_json::json!({
                "action": action,
                "args": {}
            }))
            .expect(action);
        }
    }
}
