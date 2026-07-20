use crate::agent::types::ToolResult;
use crate::asset_3d::{load_3d_jobs, ThreeDJobStatus};
use crate::game_production::{load_game_production_state, GAME_PRODUCTION_STATE_PATH};
use crate::memory::load_memory;
use crate::project::detect_project_profiles;
use crate::roadmap::load_roadmap;
use crate::unreal_gameplay::load_gameplay_state;
use crate::unreal_intelligence::{scan_unreal_project, UnrealAssetKind};
use crate::vertical_slice::{load_vertical_slice_state, VERTICAL_SLICE_STATE_PATH};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROJECT_GRAPH_PATH: &str = "assets/generated/leetcode/project_graph.json";
pub const PROJECT_GRAPH_SELECTION_PATH: &str =
    "assets/generated/leetcode/project_graph_selection.json";
const MAX_GRAPH_FILE_BYTES: usize = 256_000_000;
const MAX_TEXT_SCAN_BYTES: usize = 220_000;
const MAX_FILE_ROWS: usize = 1_200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGraphState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub nodes: Vec<ProjectGraphNode>,
    #[serde(default)]
    pub edges: Vec<ProjectGraphEdge>,
    #[serde(default)]
    pub updated_at: u64,
}

impl Default for ProjectGraphState {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            title: "Project Graph".to_string(),
            focus: "Автоматически собранная карта проекта".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            updated_at: unix_timestamp(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGraphNode {
    pub id: String,
    pub label: String,
    pub kind: ProjectGraphNodeKind,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectGraphNodeKind {
    Project,
    Folder,
    File,
    Module,
    Symbol,
    Command,
    Asset,
    ThreeDAsset,
    Memory,
    RoadmapItem,
    UnrealProject,
    UnrealPlugin,
    UnrealModule,
    UnrealTarget,
    UnrealConfig,
    UnrealSource,
    UnrealMap,
    UnrealBlueprint,
    UnrealDataAsset,
    UnrealMaterial,
    UnrealNiagara,
    UnrealAnimation,
    UnrealSkeleton,
    UnrealSkeletalMesh,
    UnrealStaticMesh,
    UnrealAnimationBlueprint,
    UnrealAnimationMontage,
    UnrealControlRig,
    UnrealPhysicsAsset,
    UnrealSound,
    UnrealWidget,
    UnrealInputAsset,
    UnrealAsset,
    GameplayPlan,
    GameplayRun,
    GameProductionPlan,
    ProductionItem,
    VerticalSliceRun,
    VerticalSlicePhase,
}

impl ProjectGraphNodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Folder => "folder",
            Self::File => "file",
            Self::Module => "module",
            Self::Symbol => "symbol",
            Self::Command => "command",
            Self::Asset => "asset",
            Self::ThreeDAsset => "three_d_asset",
            Self::Memory => "memory",
            Self::RoadmapItem => "roadmap_item",
            Self::UnrealProject => "unreal_project",
            Self::UnrealPlugin => "unreal_plugin",
            Self::UnrealModule => "unreal_module",
            Self::UnrealTarget => "unreal_target",
            Self::UnrealConfig => "unreal_config",
            Self::UnrealSource => "unreal_source",
            Self::UnrealMap => "unreal_map",
            Self::UnrealBlueprint => "unreal_blueprint",
            Self::UnrealDataAsset => "unreal_data_asset",
            Self::UnrealMaterial => "unreal_material",
            Self::UnrealNiagara => "unreal_niagara",
            Self::UnrealAnimation => "unreal_animation",
            Self::UnrealSkeleton => "unreal_skeleton",
            Self::UnrealSkeletalMesh => "unreal_skeletal_mesh",
            Self::UnrealStaticMesh => "unreal_static_mesh",
            Self::UnrealAnimationBlueprint => "unreal_animation_blueprint",
            Self::UnrealAnimationMontage => "unreal_animation_montage",
            Self::UnrealControlRig => "unreal_control_rig",
            Self::UnrealPhysicsAsset => "unreal_physics_asset",
            Self::UnrealSound => "unreal_sound",
            Self::UnrealWidget => "unreal_widget",
            Self::UnrealInputAsset => "unreal_input_asset",
            Self::UnrealAsset => "unreal_asset",
            Self::GameplayPlan => "gameplay_plan",
            Self::GameplayRun => "gameplay_run",
            Self::GameProductionPlan => "game_production_plan",
            Self::ProductionItem => "production_item",
            Self::VerticalSliceRun => "vertical_slice_run",
            Self::VerticalSlicePhase => "vertical_slice_phase",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGraphEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: ProjectGraphEdgeKind,
    pub label: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectGraphEdgeKind {
    Contains,
    Imports,
    DependsOn,
    Calls,
    Generates,
    Tests,
    Documents,
    RelatedTo,
    Declares,
    Configures,
    References,
    Loads,
    UsesSkeleton,
    Animates,
    ControlledBy,
    HasComponent,
    CompatibleWith,
    SpawnedBy,
    OwnedBy,
    BoundToInput,
    Produces,
    Consumes,
}

impl ProjectGraphEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::DependsOn => "depends_on",
            Self::Calls => "calls",
            Self::Generates => "generates",
            Self::Tests => "tests",
            Self::Documents => "documents",
            Self::RelatedTo => "related_to",
            Self::Declares => "declares",
            Self::Configures => "configures",
            Self::References => "references",
            Self::Loads => "loads",
            Self::UsesSkeleton => "uses_skeleton",
            Self::Animates => "animates",
            Self::ControlledBy => "controlled_by",
            Self::HasComponent => "has_component",
            Self::CompatibleWith => "compatible_with",
            Self::SpawnedBy => "spawned_by",
            Self::OwnedBy => "owned_by",
            Self::BoundToInput => "bound_to_input",
            Self::Produces => "produces",
            Self::Consumes => "consumes",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Contains => "содержит",
            Self::Imports => "импортирует",
            Self::DependsOn => "зависит от",
            Self::Calls => "вызывает",
            Self::Generates => "генерирует",
            Self::Tests => "проверяет",
            Self::Documents => "документирует",
            Self::RelatedTo => "связано с",
            Self::Declares => "объявляет",
            Self::Configures => "настраивает",
            Self::References => "ссылается на",
            Self::Loads => "загружает",
            Self::UsesSkeleton => "использует Skeleton",
            Self::Animates => "анимирует",
            Self::ControlledBy => "управляется",
            Self::HasComponent => "содержит компонент",
            Self::CompatibleWith => "совместимо с",
            Self::SpawnedBy => "создаётся через",
            Self::OwnedBy => "принадлежит",
            Self::BoundToInput => "привязано к вводу",
            Self::Produces => "производит",
            Self::Consumes => "использует ресурс",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectGraphSelection {
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub selected_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectGraphSnapshotArgs {
    #[serde(default)]
    pub save_if_missing: bool,
    #[serde(default)]
    pub refresh: bool,
    #[serde(default = "default_include_edges")]
    pub include_edges: bool,
    #[serde(default)]
    pub max_nodes: Option<usize>,
}

pub fn load_project_graph(workspace: &Workspace) -> ProjectGraphState {
    workspace
        .read_text(PROJECT_GRAPH_PATH, MAX_GRAPH_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectGraphState>(&text).ok())
        .unwrap_or_else(|| scan_project_graph(workspace))
}

pub fn project_graph_fingerprint(graph: &ProjectGraphState) -> String {
    let mut rows = graph
        .nodes
        .iter()
        .map(|node| {
            format!(
                "node\t{}\t{}\t{}\t{}\t{:?}",
                node.id,
                node.kind.as_str(),
                node.path.as_deref().unwrap_or_default(),
                node.source,
                node.metadata
            )
        })
        .chain(graph.edges.iter().map(|edge| {
            format!(
                "edge\t{}\t{}\t{}\t{}\t{}",
                edge.from,
                edge.to,
                edge.kind.as_str(),
                edge.source,
                edge.confidence
            )
        }))
        .collect::<Vec<_>>();
    rows.sort();
    let mut hasher = Sha256::new();
    for row in rows {
        hasher.update(row.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

pub fn save_project_graph(workspace: &Workspace, graph: &ProjectGraphState) -> anyhow::Result<()> {
    workspace.write_text(PROJECT_GRAPH_PATH, &serde_json::to_string(graph)?)
}

pub fn load_project_graph_selection(workspace: &Workspace) -> Option<String> {
    workspace
        .read_text(PROJECT_GRAPH_SELECTION_PATH, 32_000)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectGraphSelection>(&text).ok())
        .and_then(|selection| selection.node_id)
        .filter(|node_id| {
            load_project_graph(workspace)
                .nodes
                .iter()
                .any(|node| &node.id == node_id)
        })
}

pub fn save_project_graph_selection(
    workspace: &Workspace,
    node_id: Option<&str>,
) -> anyhow::Result<()> {
    let selection = ProjectGraphSelection {
        node_id: node_id.map(ToString::to_string),
        selected_at: unix_timestamp(),
    };
    workspace.write_text(
        PROJECT_GRAPH_SELECTION_PATH,
        &serde_json::to_string_pretty(&selection)?,
    )
}

pub fn selected_project_node_context_value(
    workspace: &Workspace,
    requested_node_id: Option<&str>,
) -> Option<serde_json::Value> {
    let node_id = requested_node_id
        .map(ToString::to_string)
        .or_else(|| load_project_graph_selection(workspace))?;
    let graph = load_project_graph(workspace);
    let node = graph.nodes.iter().find(|node| node.id == node_id)?.clone();
    let edges = graph
        .edges
        .iter()
        .filter(|edge| edge.from == node.id || edge.to == node.id)
        .take(40)
        .cloned()
        .collect::<Vec<_>>();
    let neighbours = edges
        .iter()
        .filter_map(|edge| {
            let other = if edge.from == node.id {
                &edge.to
            } else {
                &edge.from
            };
            graph.nodes.iter().find(|candidate| &candidate.id == other)
        })
        .cloned()
        .collect::<Vec<_>>();
    Some(json!({
        "selection_path": PROJECT_GRAPH_SELECTION_PATH,
        "node": node,
        "edges": edges,
        "neighbours": neighbours,
    }))
}

pub fn selected_project_node_context_for_prompt(workspace: &Workspace) -> Option<String> {
    let value = selected_project_node_context_value(workspace, None)?;
    Some(format!(
        "Выбранный узел Project Map является точным контекстом текущей задачи. Используй его id, path/object_path и связи без угадывания. При MCP-вызове этот же блок передаётся в protocol `_meta`:\n{}",
        serde_json::to_string_pretty(&value).ok()?
    ))
}

pub fn project_graph_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Карта проекта: рабочая папка не выбрана.".to_string();
    };
    let Some(graph) = workspace
        .read_text(PROJECT_GRAPH_PATH, MAX_GRAPH_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectGraphState>(&text).ok())
    else {
        return "Карта проекта: снимок ещё не сохранён. Используй project_graph_snapshot с refresh=true, когда нужно построить архитектурную карту файлов, команд, памяти и roadmap.".to_string();
    };
    let mut counts = BTreeMap::<ProjectGraphNodeKind, usize>::new();
    for node in &graph.nodes {
        *counts.entry(node.kind).or_default() += 1;
    }
    let counts = counts
        .iter()
        .map(|(kind, count)| format!("{}: {}", kind.as_str(), count))
        .collect::<Vec<_>>()
        .join(", ");
    let commands = graph
        .nodes
        .iter()
        .filter(|node| node.kind == ProjectGraphNodeKind::Command)
        .take(6)
        .map(|node| node.label.clone())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Карта проекта: {} узлов, {} связей{}.\nИспользуй project_graph_snapshot, когда нужно понять архитектуру, зависимости, проектные команды, Unreal-модули/ассеты, связь файлов с roadmap или памятью.{}",
        graph.nodes.len(),
        graph.edges.len(),
        if commands.is_empty() {
            format!("; типы: {}", empty_label(&counts))
        } else {
            format!("; команды: {commands}; типы: {}", empty_label(&counts))
        },
        selected_project_node_context_for_prompt(workspace)
            .map(|context| format!("\n{context}"))
            .unwrap_or_default()
    )
}

pub fn project_graph_snapshot(workspace: &Workspace, args: ProjectGraphSnapshotArgs) -> ToolResult {
    let graph_exists = workspace.read_text(PROJECT_GRAPH_PATH, 1).is_ok();
    let graph = if args.refresh {
        let graph = refresh_project_graph(workspace);
        if let Err(err) = save_project_graph(workspace, &graph) {
            return ToolResult::error(err.to_string());
        }
        graph
    } else {
        let graph = load_project_graph(workspace);
        if args.save_if_missing && !graph_exists {
            if let Err(err) = save_project_graph(workspace, &graph) {
                return ToolResult::error(err.to_string());
            }
        }
        graph
    };
    let graph = bounded_graph(graph, args.max_nodes, args.include_edges);
    ToolResult::ok(
        serde_json::to_string_pretty(&graph).unwrap_or_else(|_| "снимок карты проекта".to_string()),
    )
}

pub fn scan_project_graph(workspace: &Workspace) -> ProjectGraphState {
    let now = unix_timestamp();
    let mut builder = GraphBuilder::new(workspace.display_name(), now);
    let rows = workspace.ui_file_rows(MAX_FILE_ROWS);

    add_file_tree_nodes(&mut builder, workspace, &rows);
    add_project_profile_nodes(&mut builder, workspace);
    add_dependency_nodes(&mut builder, workspace);
    add_rust_module_nodes(&mut builder, workspace, &rows);
    add_memory_nodes(&mut builder, workspace);
    add_roadmap_nodes(&mut builder, workspace);
    add_unreal_nodes(&mut builder, workspace);
    add_three_d_asset_nodes(&mut builder, workspace);
    add_gameplay_nodes(&mut builder, workspace);
    add_game_production_nodes(&mut builder, workspace);
    add_vertical_slice_nodes(&mut builder, workspace);

    builder.finish()
}

fn add_vertical_slice_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let state = load_vertical_slice_state(workspace);
    for run in &state.runs {
        let run_node_id = stable_id("game:vertical-slice", &run.id);
        let production_node_id = stable_id("game:production", &run.production_plan_id);
        let completed = run
            .phases
            .iter()
            .filter(|phase| {
                phase.status == crate::vertical_slice::VerticalSlicePhaseStatus::Completed
            })
            .count();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "production_plan_id".to_string(),
            run.production_plan_id.clone(),
        );
        metadata.insert(
            "status".to_string(),
            format!("{:?}", run.status).to_lowercase(),
        );
        metadata.insert(
            "progress".to_string(),
            format!("{completed}/{}", run.phases.len()),
        );
        builder.add_node(ProjectGraphNode {
            id: run_node_id.clone(),
            label: run.title.clone(),
            kind: ProjectGraphNodeKind::VerticalSliceRun,
            path: Some(VERTICAL_SLICE_STATE_PATH.to_string()),
            summary: format!(
                "Vertical Slice orchestration: {} из {} фаз завершено",
                completed,
                run.phases.len()
            ),
            source: "scanner:vertical_slice_orchestrator".to_string(),
            confidence: 1.0,
            metadata,
            updated_at: run.updated_at,
        });
        builder.add_edge(
            if builder.has_node(&production_node_id) {
                &production_node_id
            } else {
                "project:root"
            },
            &run_node_id,
            ProjectGraphEdgeKind::Contains,
            1.0,
            "scanner:vertical_slice_orchestrator",
        );

        for phase in &run.phases {
            let phase_node_id = stable_id(
                "game:vertical-slice:phase",
                &format!("{}:{}", run.id, phase.kind.id()),
            );
            let mut metadata = BTreeMap::new();
            metadata.insert("run_id".to_string(), run.id.clone());
            metadata.insert("phase".to_string(), phase.kind.id().to_string());
            metadata.insert("status".to_string(), phase.status.label().to_string());
            metadata.insert(
                "recommended_tools".to_string(),
                phase.kind.recommended_tools().join(", "),
            );
            metadata.insert("evidence".to_string(), phase.evidence.join(" | "));
            metadata.insert("artifacts".to_string(), phase.artifacts.join(", "));
            builder.add_node(ProjectGraphNode {
                id: phase_node_id.clone(),
                label: phase.title.clone(),
                kind: ProjectGraphNodeKind::VerticalSlicePhase,
                path: phase.artifacts.first().cloned(),
                summary: compact_inline(&phase.description, 240),
                source: "scanner:vertical_slice_phase".to_string(),
                confidence: 1.0,
                metadata,
                updated_at: phase.updated_at,
            });
            builder.add_edge(
                &run_node_id,
                &phase_node_id,
                ProjectGraphEdgeKind::Contains,
                1.0,
                "scanner:vertical_slice_phase",
            );
            for dependency in &phase.depends_on {
                let dependency_node_id = stable_id(
                    "game:vertical-slice:phase",
                    &format!("{}:{}", run.id, dependency.id()),
                );
                builder.add_edge(
                    &phase_node_id,
                    &dependency_node_id,
                    ProjectGraphEdgeKind::DependsOn,
                    1.0,
                    "scanner:vertical_slice_dependency",
                );
            }
            for artifact in &phase.artifacts {
                let artifact_node_id = path_node_id(ProjectGraphNodeKind::Asset, artifact);
                if builder.has_node(&artifact_node_id) {
                    builder.add_edge(
                        &phase_node_id,
                        &artifact_node_id,
                        ProjectGraphEdgeKind::Generates,
                        1.0,
                        "scanner:vertical_slice_artifact",
                    );
                }
            }
        }
    }
}

fn add_game_production_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let state = load_game_production_state(workspace);
    for plan in &state.plans {
        let plan_node_id = stable_id("game:production", &plan.id);
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "scope".to_string(),
            format!("{:?}", plan.scope).to_lowercase(),
        );
        metadata.insert("engine".to_string(), plan.engine.clone());
        metadata.insert("genre".to_string(), plan.genre.clone());
        metadata.insert("platform".to_string(), plan.target_platform.clone());
        metadata.insert(
            "milestone".to_string(),
            plan.current_milestone.id().to_string(),
        );
        metadata.insert(
            "status".to_string(),
            format!("{:?}", plan.status).to_lowercase(),
        );
        metadata.insert("items".to_string(), plan.items.len().to_string());
        builder.add_node(ProjectGraphNode {
            id: plan_node_id.clone(),
            label: plan.title.clone(),
            kind: ProjectGraphNodeKind::GameProductionPlan,
            path: Some(GAME_PRODUCTION_STATE_PATH.to_string()),
            summary: compact_inline(&plan.brief, 240),
            source: "scanner:game_production".to_string(),
            confidence: 1.0,
            metadata,
            updated_at: plan.updated_at,
        });
        builder.add_edge(
            "project:root",
            &plan_node_id,
            ProjectGraphEdgeKind::Contains,
            1.0,
            "scanner:game_production",
        );
        if let Some(project_node_id) = plan
            .project_node_id
            .as_deref()
            .filter(|node_id| builder.has_node(node_id))
        {
            builder.add_edge(
                &plan_node_id,
                project_node_id,
                ProjectGraphEdgeKind::RelatedTo,
                1.0,
                "scanner:game_production_context",
            );
        }
        for task_id in &plan.source_task_ids {
            let task_node_id = stable_id("memory:task", task_id);
            if builder.has_node(&task_node_id) {
                builder.add_edge(
                    &plan_node_id,
                    &task_node_id,
                    ProjectGraphEdgeKind::RelatedTo,
                    0.95,
                    "scanner:game_production_task",
                );
            }
        }
        for roadmap_id in &plan.roadmap_ids {
            let roadmap_node_id = stable_id("roadmap", roadmap_id);
            if builder.has_node(&roadmap_node_id) {
                builder.add_edge(
                    &plan_node_id,
                    &roadmap_node_id,
                    ProjectGraphEdgeKind::RelatedTo,
                    0.95,
                    "scanner:game_production_roadmap",
                );
            }
        }
        for item in &plan.items {
            let item_node_id = stable_id("game:production:item", &item.id);
            let mut metadata = BTreeMap::new();
            metadata.insert("plan_id".to_string(), plan.id.clone());
            metadata.insert("milestone".to_string(), item.milestone.id().to_string());
            metadata.insert(
                "workstream".to_string(),
                item.workstream.label().to_string(),
            );
            metadata.insert("status".to_string(), item.status.label().to_string());
            metadata.insert("priority".to_string(), item.priority.to_string());
            metadata.insert("validation".to_string(), item.validation.clone());
            metadata.insert("artifacts".to_string(), item.artifacts.join(", "));
            builder.add_node(ProjectGraphNode {
                id: item_node_id.clone(),
                label: item.title.clone(),
                kind: ProjectGraphNodeKind::ProductionItem,
                path: item.artifacts.first().cloned(),
                summary: compact_inline(&item.description, 240),
                source: "scanner:game_production_item".to_string(),
                confidence: 1.0,
                metadata,
                updated_at: item.updated_at,
            });
            builder.add_edge(
                &plan_node_id,
                &item_node_id,
                ProjectGraphEdgeKind::Contains,
                1.0,
                "scanner:game_production_item",
            );
            for dependency in &item.depends_on {
                let dependency_node_id = stable_id("game:production:item", dependency);
                builder.add_edge(
                    &item_node_id,
                    &dependency_node_id,
                    ProjectGraphEdgeKind::DependsOn,
                    1.0,
                    "scanner:game_production_dependency",
                );
            }
            for artifact in &item.artifacts {
                let artifact_node_id = path_node_id(ProjectGraphNodeKind::Asset, artifact);
                if builder.has_node(&artifact_node_id) {
                    builder.add_edge(
                        &item_node_id,
                        &artifact_node_id,
                        ProjectGraphEdgeKind::Generates,
                        1.0,
                        "scanner:game_production_artifact",
                    );
                }
            }
        }
    }
}

fn add_gameplay_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let state = load_gameplay_state(workspace);
    for plan in &state.plans {
        let id = stable_id("gameplay:plan", &plan.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("recipe".to_string(), plan.recipe.id().to_string());
        metadata.insert(
            "status".to_string(),
            format!("{:?}", plan.status).to_lowercase(),
        );
        metadata.insert("map_path".to_string(), plan.map_path.clone());
        metadata.insert("task_ids".to_string(), plan.task_ids.join(", "));
        metadata.insert("roadmap_ids".to_string(), plan.roadmap_ids.join(", "));
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: plan.title.clone(),
            kind: ProjectGraphNodeKind::GameplayPlan,
            path: Some(plan.file_path.clone()),
            summary: compact_inline(&plan.brief, 240),
            source: "scanner:unreal_gameplay".to_string(),
            confidence: 1.0,
            metadata,
            updated_at: plan.updated_at,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::Contains,
            1.0,
            "scanner:unreal_gameplay",
        );
        if let Some(project_node_id) = plan
            .project_node_id
            .as_deref()
            .filter(|node_id| builder.has_node(node_id))
        {
            builder.add_edge(
                &id,
                project_node_id,
                ProjectGraphEdgeKind::RelatedTo,
                1.0,
                "scanner:unreal_gameplay_context",
            );
        }
        for task_id in &plan.task_ids {
            let task_node_id = stable_id("memory:task", task_id);
            if builder.has_node(&task_node_id) {
                builder.add_edge(
                    &id,
                    &task_node_id,
                    ProjectGraphEdgeKind::RelatedTo,
                    0.95,
                    "scanner:unreal_gameplay_task",
                );
            }
        }
        for roadmap_id in &plan.roadmap_ids {
            let roadmap_node_id = stable_id("roadmap", roadmap_id);
            if builder.has_node(&roadmap_node_id) {
                builder.add_edge(
                    &id,
                    &roadmap_node_id,
                    ProjectGraphEdgeKind::RelatedTo,
                    0.95,
                    "scanner:unreal_gameplay_roadmap",
                );
            }
        }
    }

    for run in state.runs.iter().rev().take(100) {
        let id = stable_id("gameplay:run", &run.id);
        let path = run.artifacts.first().cloned();
        let mut metadata = BTreeMap::new();
        metadata.insert("mode".to_string(), run.mode.id().to_string());
        metadata.insert("success".to_string(), run.success.to_string());
        metadata.insert("duration_ms".to_string(), run.duration_ms.to_string());
        metadata.insert("map_path".to_string(), run.map_path.clone());
        metadata.insert("test_filter".to_string(), run.test_filter.clone());
        metadata.insert(
            "artifact_count".to_string(),
            run.artifacts.len().to_string(),
        );
        metadata.insert("issue_count".to_string(), run.issues.len().to_string());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: if run.success {
                format!("Playtest {}: успешно", run.id)
            } else {
                format!("Playtest {}: ошибка", run.id)
            },
            kind: ProjectGraphNodeKind::GameplayRun,
            path,
            summary: compact_inline(&run.output_summary, 240),
            source: "scanner:unreal_gameplay_run".to_string(),
            confidence: 1.0,
            metadata,
            updated_at: run.created_at,
        });
        let parent = run
            .plan_id
            .as_deref()
            .map(|plan_id| stable_id("gameplay:plan", plan_id))
            .filter(|plan_id| builder.has_node(plan_id))
            .unwrap_or_else(|| "project:root".to_string());
        builder.add_edge(
            &parent,
            &id,
            ProjectGraphEdgeKind::Tests,
            1.0,
            "scanner:unreal_gameplay_run",
        );
    }
}

fn add_three_d_asset_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    for job in load_3d_jobs(workspace) {
        let id = stable_id("asset3d", &job.id);
        let output_path = job.output_files.first().cloned();
        let mut metadata = BTreeMap::new();
        metadata.insert("provider".to_string(), job.provider.clone());
        metadata.insert("model".to_string(), job.model.clone());
        metadata.insert(
            "status".to_string(),
            format!("{:?}", job.status).to_lowercase(),
        );
        metadata.insert(
            "stage".to_string(),
            format!("{:?}", job.stage).to_lowercase(),
        );
        metadata.insert("progress".to_string(), job.progress.to_string());
        metadata.insert("target_format".to_string(), job.target_format.clone());
        metadata.insert(
            "target_polycount".to_string(),
            job.target_polycount.to_string(),
        );
        metadata.insert("pbr".to_string(), job.enable_pbr.to_string());
        metadata.insert(
            "license_confirmed".to_string(),
            job.license_confirmed.to_string(),
        );
        if let Some(validation) = &job.validation {
            metadata.insert(
                "import_ready".to_string(),
                validation.import_ready.to_string(),
            );
            metadata.insert(
                "triangles".to_string(),
                validation.triangle_count.to_string(),
            );
            metadata.insert(
                "materials".to_string(),
                validation.material_count.to_string(),
            );
            metadata.insert(
                "animations".to_string(),
                validation.animation_count.to_string(),
            );
        }

        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: if job.prompt.trim().is_empty() {
                job.id.clone()
            } else {
                compact_inline(&job.prompt, 56)
            },
            kind: ProjectGraphNodeKind::ThreeDAsset,
            path: output_path.clone(),
            summary: format!(
                "3D asset job: {} / {} / {}%",
                job.provider,
                format!("{:?}", job.stage).to_lowercase(),
                job.progress
            ),
            source: "scanner:asset_3d_pipeline".to_string(),
            confidence: if job.status == ThreeDJobStatus::Ready {
                1.0
            } else {
                0.85
            },
            metadata,
            updated_at: job.updated_at,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::Generates,
            0.95,
            "scanner:asset_3d_pipeline",
        );

        if let Some(source_image) = &job.source_image {
            let source_id = path_node_id(ProjectGraphNodeKind::Asset, source_image);
            if builder.has_node(&source_id) {
                builder.add_edge(
                    &source_id,
                    &id,
                    ProjectGraphEdgeKind::Generates,
                    0.95,
                    "scanner:asset_3d_pipeline",
                );
            }
        }
        if let Some(path) = output_path {
            let output_id = path_node_id(ProjectGraphNodeKind::Asset, &path);
            if builder.has_node(&output_id) {
                builder.add_edge(
                    &id,
                    &output_id,
                    ProjectGraphEdgeKind::Generates,
                    1.0,
                    "scanner:asset_3d_pipeline",
                );
            }
        }
    }
}

pub fn refresh_project_graph(workspace: &Workspace) -> ProjectGraphState {
    let previous = workspace
        .read_text(PROJECT_GRAPH_PATH, MAX_GRAPH_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectGraphState>(&text).ok());
    merge_incremental_graph(previous, scan_project_graph(workspace))
}

fn add_file_tree_nodes(builder: &mut GraphBuilder, workspace: &Workspace, rows: &[String]) {
    for row in rows {
        if row == "..." {
            continue;
        }
        let is_dir = row.ends_with('/');
        let rel = row.trim_end_matches('/');
        if rel.is_empty() {
            continue;
        }
        let kind = if is_dir {
            ProjectGraphNodeKind::Folder
        } else if is_asset_path(rel) {
            ProjectGraphNodeKind::Asset
        } else {
            ProjectGraphNodeKind::File
        };
        let mut metadata = BTreeMap::new();
        if !is_dir {
            if let Some(extension) = file_extension(rel) {
                metadata.insert("extension".to_string(), extension);
            }
        }
        builder.add_node(ProjectGraphNode {
            id: path_node_id(kind, rel),
            label: file_label(rel),
            kind,
            path: Some(rel.to_string()),
            summary: if is_dir {
                "Каталог проекта".to_string()
            } else if kind == ProjectGraphNodeKind::Asset {
                "Ассет или сгенерированный медиа-файл проекта".to_string()
            } else {
                "Файл проекта".to_string()
            },
            source: "scanner:file_tree".to_string(),
            confidence: 0.95,
            metadata,
            updated_at: builder.now,
        });
        let parent = parent_path(rel)
            .map(|path| folder_node_id(&path))
            .unwrap_or_else(|| "project:root".to_string());
        if parent != "project:root" && !builder.has_node(&parent) {
            let folder_path = parent.trim_start_matches("folder:").to_string();
            builder.add_node(ProjectGraphNode {
                id: parent.clone(),
                label: file_label(&folder_path),
                kind: ProjectGraphNodeKind::Folder,
                path: Some(folder_path),
                summary: "Каталог проекта".to_string(),
                source: "scanner:file_tree:inferred_parent".to_string(),
                confidence: 0.65,
                metadata: BTreeMap::new(),
                updated_at: builder.now,
            });
        }
        builder.add_edge(
            &parent,
            &path_node_id(kind, rel),
            ProjectGraphEdgeKind::Contains,
            0.95,
            "scanner:file_tree",
        );
        if kind == ProjectGraphNodeKind::Asset {
            builder.add_edge(
                "project:root",
                &path_node_id(kind, rel),
                ProjectGraphEdgeKind::Generates,
                0.55,
                "scanner:asset_path",
            );
        }
    }

    builder.add_node(ProjectGraphNode {
        id: "project:root".to_string(),
        label: workspace.display_name(),
        kind: ProjectGraphNodeKind::Project,
        path: Some(".".to_string()),
        summary: "Корень выбранного проекта".to_string(),
        source: "scanner:workspace".to_string(),
        confidence: 1.0,
        metadata: BTreeMap::new(),
        updated_at: builder.now,
    });
}

fn add_project_profile_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    for profile in detect_project_profiles(workspace) {
        let profile_id = stable_id("module:profile", &profile.kind);
        let mut metadata = BTreeMap::new();
        metadata.insert("kind".to_string(), profile.kind.clone());
        metadata.insert("markers".to_string(), profile.markers.join(", "));
        builder.add_node(ProjectGraphNode {
            id: profile_id.clone(),
            label: profile.kind.clone(),
            kind: ProjectGraphNodeKind::Module,
            path: None,
            summary: format!("Профиль проекта: {}", profile.name),
            source: "scanner:project_profile".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &profile_id,
            ProjectGraphEdgeKind::Contains,
            0.9,
            "scanner:project_profile",
        );
        for command in profile.commands {
            let command_id = stable_id("command", &format!("{}:{}", profile.kind, command.id));
            let mut metadata = BTreeMap::new();
            metadata.insert("command".to_string(), command.command.clone());
            metadata.insert("cwd".to_string(), command.cwd.clone());
            metadata.insert("timeout_secs".to_string(), command.timeout_secs.to_string());
            builder.add_node(ProjectGraphNode {
                id: command_id.clone(),
                label: command.label.clone(),
                kind: ProjectGraphNodeKind::Command,
                path: None,
                summary: command.description.clone(),
                source: "scanner:project_command".to_string(),
                confidence: 0.9,
                metadata,
                updated_at: builder.now,
            });
            builder.add_edge(
                &profile_id,
                &command_id,
                ProjectGraphEdgeKind::Contains,
                0.9,
                "scanner:project_command",
            );
            if is_test_command(&command.id, &command.command) {
                builder.add_edge(
                    &command_id,
                    "project:root",
                    ProjectGraphEdgeKind::Tests,
                    0.8,
                    "scanner:project_command",
                );
            }
        }
    }
}

fn add_dependency_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    if let Ok(cargo) = workspace.read_text("Cargo.toml", MAX_TEXT_SCAN_BYTES) {
        for dependency in cargo_dependencies(&cargo).into_iter().take(80) {
            let id = stable_id("module:dependency:cargo", &dependency);
            let mut metadata = BTreeMap::new();
            metadata.insert("manager".to_string(), "cargo".to_string());
            builder.add_node(ProjectGraphNode {
                id: id.clone(),
                label: dependency,
                kind: ProjectGraphNodeKind::Module,
                path: None,
                summary: "Cargo dependency".to_string(),
                source: "scanner:cargo_toml".to_string(),
                confidence: 0.75,
                metadata,
                updated_at: builder.now,
            });
            builder.add_edge(
                "file:Cargo.toml",
                &id,
                ProjectGraphEdgeKind::DependsOn,
                0.75,
                "scanner:cargo_toml",
            );
        }
    }

    if let Ok(package_json) = workspace.read_text("package.json", MAX_TEXT_SCAN_BYTES) {
        for dependency in npm_dependencies(&package_json).into_iter().take(80) {
            let id = stable_id("module:dependency:npm", &dependency);
            let mut metadata = BTreeMap::new();
            metadata.insert("manager".to_string(), "npm".to_string());
            builder.add_node(ProjectGraphNode {
                id: id.clone(),
                label: dependency,
                kind: ProjectGraphNodeKind::Module,
                path: None,
                summary: "npm dependency".to_string(),
                source: "scanner:package_json".to_string(),
                confidence: 0.8,
                metadata,
                updated_at: builder.now,
            });
            builder.add_edge(
                "file:package.json",
                &id,
                ProjectGraphEdgeKind::DependsOn,
                0.8,
                "scanner:package_json",
            );
        }
    }
}

fn add_rust_module_nodes(builder: &mut GraphBuilder, workspace: &Workspace, rows: &[String]) {
    for rel in rows
        .iter()
        .map(|row| row.trim_end_matches('/'))
        .filter(|rel| rel.ends_with(".rs"))
        .take(220)
    {
        let module = rust_module_name(rel);
        let module_id = stable_id("module:rust", &module);
        let mut metadata = BTreeMap::new();
        metadata.insert("language".to_string(), "rust".to_string());
        builder.add_node(ProjectGraphNode {
            id: module_id.clone(),
            label: module.clone(),
            kind: ProjectGraphNodeKind::Module,
            path: Some(rel.to_string()),
            summary: "Rust module inferred from file path".to_string(),
            source: "scanner:rust_path".to_string(),
            confidence: 0.75,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, rel),
            &module_id,
            ProjectGraphEdgeKind::Documents,
            0.75,
            "scanner:rust_path",
        );

        if let Ok(text) = workspace.read_text(rel, MAX_TEXT_SCAN_BYTES) {
            for imported in rust_imports(&text).into_iter().take(60) {
                let imported_id = stable_id("module:rust", &imported);
                if !builder.has_node(&imported_id) {
                    builder.add_node(ProjectGraphNode {
                        id: imported_id.clone(),
                        label: imported.clone(),
                        kind: ProjectGraphNodeKind::Module,
                        path: None,
                        summary: "Rust module inferred from import".to_string(),
                        source: "scanner:rust_imports".to_string(),
                        confidence: 0.45,
                        metadata: BTreeMap::new(),
                        updated_at: builder.now,
                    });
                }
                builder.add_edge(
                    &module_id,
                    &imported_id,
                    ProjectGraphEdgeKind::Imports,
                    0.55,
                    "scanner:rust_imports",
                );
            }
        }
    }
}

fn add_memory_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let memory = load_memory(workspace);
    for goal in memory.goals {
        let id = stable_id("memory:goal", &goal.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "goal".to_string());
        metadata.insert("status".to_string(), goal.status.clone());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: goal.title,
            kind: ProjectGraphNodeKind::Memory,
            path: None,
            summary: goal.notes,
            source: "scanner:memory".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:memory",
        );
    }
    for task in memory.tasks {
        let id = stable_id("memory:task", &task.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "task".to_string());
        metadata.insert("status".to_string(), task.status.clone());
        metadata.insert("workstream".to_string(), task.workstream.clone());
        metadata.insert("milestone".to_string(), task.milestone.clone());
        metadata.insert("priority".to_string(), task.priority.clone());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: task.title,
            kind: ProjectGraphNodeKind::Memory,
            path: None,
            summary: task.notes,
            source: "scanner:memory".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:memory",
        );
    }
    for decision in memory.decisions {
        let id = stable_id("memory:decision", &decision.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "decision".to_string());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: decision.title,
            kind: ProjectGraphNodeKind::Memory,
            path: None,
            summary: decision.rationale,
            source: "scanner:memory".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:memory",
        );
    }
    for source in memory.sources {
        let id = stable_id("memory:source", &source.id);
        let path = source
            .original_path
            .clone()
            .or_else(|| source.stored_path.clone());
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "source".to_string());
        metadata.insert("kind".to_string(), source.kind.clone());
        metadata.insert(
            "content_chars".to_string(),
            source.content_chars.to_string(),
        );
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: source.title,
            kind: ProjectGraphNodeKind::Memory,
            path: path.clone(),
            summary: if source.summary.trim().is_empty() {
                compact_inline(&source.content, 240)
            } else {
                source.summary
            },
            source: "scanner:memory_source".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::Documents,
            0.85,
            "scanner:memory_source",
        );
        if let Some(path) = path.filter(|value| !value.trim().is_empty()) {
            let file_id = path_node_id(ProjectGraphNodeKind::File, &path);
            builder.add_edge(
                &id,
                &file_id,
                ProjectGraphEdgeKind::Documents,
                0.6,
                "scanner:memory_source",
            );
        }
    }
}

fn add_roadmap_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let roadmap = load_roadmap(workspace);
    for item in roadmap.items {
        let id = stable_id("roadmap", &item.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("status".to_string(), item.status.as_str().to_string());
        metadata.insert("kind".to_string(), item.kind.clone());
        metadata.insert("date_label".to_string(), item.date_label.clone());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: item.title,
            kind: ProjectGraphNodeKind::RoadmapItem,
            path: None,
            summary: item.detail,
            source: "scanner:roadmap".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:roadmap",
        );
        for file in item.links.files {
            let file_id = path_node_id(ProjectGraphNodeKind::File, &file);
            builder.add_edge(
                &id,
                &file_id,
                ProjectGraphEdgeKind::RelatedTo,
                0.65,
                "scanner:roadmap_links",
            );
        }
        for memory_id in item.links.memory_ids {
            let memory_node_id = stable_id("memory", &memory_id);
            builder.add_edge(
                &id,
                &memory_node_id,
                ProjectGraphEdgeKind::Documents,
                0.55,
                "scanner:roadmap_links",
            );
        }
    }
}

fn add_unreal_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let intelligence = scan_unreal_project(workspace);
    if intelligence.descriptors.is_empty() {
        return;
    }

    let mut descriptor_ids = BTreeMap::new();
    for descriptor in &intelligence.descriptors {
        let kind = if descriptor.kind == "plugin" {
            ProjectGraphNodeKind::UnrealPlugin
        } else {
            ProjectGraphNodeKind::UnrealProject
        };
        let id = stable_id(&format!("unreal:{}", descriptor.kind), &descriptor.name);
        descriptor_ids.insert(descriptor.name.clone(), id.clone());
        let mut metadata = BTreeMap::new();
        metadata.insert("descriptor_kind".to_string(), descriptor.kind.clone());
        metadata.insert("modules".to_string(), descriptor.modules.join(", "));
        metadata.insert(
            "scan_fingerprint".to_string(),
            intelligence.fingerprint.clone(),
        );
        if descriptor.kind == "project" {
            metadata.insert("scan_manifest_version".to_string(), "2".to_string());
            if let Ok(manifest) = serde_json::to_string(&intelligence.project_inputs) {
                metadata.insert("scan_manifest".to_string(), manifest);
            }
        }
        metadata.insert(
            "asset_count".to_string(),
            intelligence.assets.len().to_string(),
        );
        if let Some(registry_path) = &intelligence.registry_export_path {
            metadata.insert("asset_registry".to_string(), registry_path.clone());
        }
        if let Some(engine) = &descriptor.engine_association {
            metadata.insert("engine_association".to_string(), engine.clone());
        }
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: descriptor.name.clone(),
            kind,
            path: Some(descriptor.path.clone()),
            summary: if descriptor.kind == "plugin" {
                "Unreal Engine plugin descriptor".to_string()
            } else {
                "Unreal Engine project descriptor".to_string()
            },
            source: "scanner:unreal_descriptor".to_string(),
            confidence: 1.0,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::Contains,
            1.0,
            "scanner:unreal_descriptor",
        );
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, &descriptor.path),
            &id,
            ProjectGraphEdgeKind::Declares,
            1.0,
            "scanner:unreal_descriptor",
        );
    }

    let mut module_ids = BTreeMap::new();
    for module in &intelligence.modules {
        let id = stable_id("unreal:module", &module.name);
        module_ids.insert(module.name.clone(), id.clone());
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "module_type".to_string(),
            module
                .module_type
                .clone()
                .unwrap_or_else(|| "не указан".to_string()),
        );
        metadata.insert(
            "public_dependencies".to_string(),
            module.public_dependencies.join(", "),
        );
        metadata.insert(
            "private_dependencies".to_string(),
            module.private_dependencies.join(", "),
        );
        metadata.insert(
            "dynamic_dependencies".to_string(),
            module.dynamic_dependencies.join(", "),
        );
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: module.name.clone(),
            kind: ProjectGraphNodeKind::UnrealModule,
            path: Some(module.path.clone()),
            summary: "Unreal Build.cs module".to_string(),
            source: "scanner:unreal_build_cs".to_string(),
            confidence: 0.98,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, &module.path),
            &id,
            ProjectGraphEdgeKind::Declares,
            0.98,
            "scanner:unreal_build_cs",
        );
    }

    for descriptor in &intelligence.descriptors {
        let Some(descriptor_id) = descriptor_ids.get(&descriptor.name) else {
            continue;
        };
        for module_name in &descriptor.modules {
            let module_id = module_ids
                .get(module_name)
                .cloned()
                .unwrap_or_else(|| stable_id("unreal:module", module_name));
            if !builder.has_node(&module_id) {
                builder.add_node(ProjectGraphNode {
                    id: module_id.clone(),
                    label: module_name.clone(),
                    kind: ProjectGraphNodeKind::UnrealModule,
                    path: None,
                    summary: "Unreal module declared by descriptor".to_string(),
                    source: "scanner:unreal_descriptor_module".to_string(),
                    confidence: 0.8,
                    metadata: BTreeMap::new(),
                    updated_at: builder.now,
                });
            }
            builder.add_edge(
                descriptor_id,
                &module_id,
                ProjectGraphEdgeKind::Declares,
                0.95,
                "scanner:unreal_descriptor",
            );
        }
    }

    for module in &intelligence.modules {
        let Some(module_id) = module_ids.get(&module.name) else {
            continue;
        };
        for (dependency, visibility) in module
            .public_dependencies
            .iter()
            .map(|value| (value, "public"))
            .chain(
                module
                    .private_dependencies
                    .iter()
                    .map(|value| (value, "private")),
            )
            .chain(
                module
                    .dynamic_dependencies
                    .iter()
                    .map(|value| (value, "dynamic")),
            )
        {
            let dependency_id = module_ids
                .get(dependency)
                .cloned()
                .unwrap_or_else(|| stable_id("unreal:module", dependency));
            if !builder.has_node(&dependency_id) {
                let mut metadata = BTreeMap::new();
                metadata.insert("external".to_string(), "true".to_string());
                builder.add_node(ProjectGraphNode {
                    id: dependency_id.clone(),
                    label: dependency.clone(),
                    kind: ProjectGraphNodeKind::UnrealModule,
                    path: None,
                    summary: "Engine/plugin module dependency".to_string(),
                    source: "scanner:unreal_dependency".to_string(),
                    confidence: 0.7,
                    metadata,
                    updated_at: builder.now,
                });
            }
            let mut source = "scanner:unreal_build_cs".to_string();
            source.push(':');
            source.push_str(visibility);
            builder.add_edge(
                module_id,
                &dependency_id,
                ProjectGraphEdgeKind::DependsOn,
                0.95,
                &source,
            );
        }
    }

    for target in &intelligence.targets {
        let id = stable_id("unreal:target", &target.name);
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "target_type".to_string(),
            target
                .target_type
                .clone()
                .unwrap_or_else(|| "не указан".to_string()),
        );
        metadata.insert("modules".to_string(), target.modules.join(", "));
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: target.name.clone(),
            kind: ProjectGraphNodeKind::UnrealTarget,
            path: Some(target.path.clone()),
            summary: "Unreal Target.cs build target".to_string(),
            source: "scanner:unreal_target_cs".to_string(),
            confidence: 0.98,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, &target.path),
            &id,
            ProjectGraphEdgeKind::Declares,
            0.98,
            "scanner:unreal_target_cs",
        );
        for module in &target.modules {
            let module_id = module_ids
                .get(module)
                .cloned()
                .unwrap_or_else(|| stable_id("unreal:module", module));
            builder.add_edge(
                &id,
                &module_id,
                ProjectGraphEdgeKind::Loads,
                0.95,
                "scanner:unreal_target_cs",
            );
        }
    }

    for config in &intelligence.configs {
        let id = stable_id("unreal:config", &config.path);
        let mut metadata = BTreeMap::new();
        metadata.insert("sections".to_string(), config.sections.join(", "));
        metadata.insert("keys".to_string(), config.keys.join(", "));
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: file_label(&config.path),
            kind: ProjectGraphNodeKind::UnrealConfig,
            path: Some(config.path.clone()),
            summary: "Unreal Config INI".to_string(),
            source: "scanner:unreal_config".to_string(),
            confidence: 0.95,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            &id,
            "project:root",
            ProjectGraphEdgeKind::Configures,
            0.9,
            "scanner:unreal_config",
        );
    }

    for source in &intelligence.source_files {
        let id = stable_id("unreal:source", &source.path);
        let mut metadata = BTreeMap::new();
        metadata.insert("language".to_string(), source.language.clone());
        if let Some(module) = &source.module {
            metadata.insert("module".to_string(), module.clone());
        }
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: file_label(&source.path),
            kind: ProjectGraphNodeKind::UnrealSource,
            path: Some(source.path.clone()),
            summary: "Unreal C++/build source".to_string(),
            source: "scanner:unreal_source".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, &source.path),
            &id,
            ProjectGraphEdgeKind::Documents,
            0.85,
            "scanner:unreal_source",
        );
        if let Some(module) = &source.module {
            let module_id = module_ids
                .get(module)
                .cloned()
                .unwrap_or_else(|| stable_id("unreal:module", module));
            builder.add_edge(
                &module_id,
                &id,
                ProjectGraphEdgeKind::Contains,
                0.9,
                "scanner:unreal_source",
            );
        }
    }

    let mut asset_lookup = BTreeMap::new();
    for asset in &intelligence.assets {
        asset_lookup.insert(asset.object_path.clone(), asset.id.clone());
        asset_lookup.insert(asset.package_name.clone(), asset.id.clone());
        let mut metadata = asset.tags.clone();
        metadata.insert("object_path".to_string(), asset.object_path.clone());
        metadata.insert("package_name".to_string(), asset.package_name.clone());
        metadata.insert("asset_class".to_string(), asset.class_name.clone());
        metadata.insert("asset_kind".to_string(), asset.kind.as_str().to_string());
        metadata.insert("asset_source".to_string(), asset.source.clone());
        builder.add_node(ProjectGraphNode {
            id: asset.id.clone(),
            label: asset.name.clone(),
            kind: graph_kind_for_unreal_asset(asset.kind, &asset.class_name),
            path: asset.file_path.clone(),
            summary: format!("Unreal {}: {}", asset.kind.as_str(), asset.object_path),
            source: format!("scanner:unreal_{}", asset.source),
            confidence: if asset.source == "asset_registry" {
                0.98
            } else {
                0.65
            },
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &asset.id,
            ProjectGraphEdgeKind::Contains,
            0.8,
            "scanner:unreal_content",
        );
    }

    for dependency in &intelligence.asset_dependencies {
        let Some(from) = resolve_unreal_asset_id(&asset_lookup, &dependency.from) else {
            continue;
        };
        let to = resolve_unreal_asset_id(&asset_lookup, &dependency.to).unwrap_or_else(|| {
            let id = stable_id("unreal:asset:external", &dependency.to);
            if !builder.has_node(&id) {
                let mut metadata = BTreeMap::new();
                metadata.insert("object_path".to_string(), dependency.to.clone());
                metadata.insert("external".to_string(), "true".to_string());
                metadata.insert(
                    "external_scope".to_string(),
                    if dependency.to.starts_with("/Game/") {
                        "project"
                    } else {
                        "engine_or_plugin"
                    }
                    .to_string(),
                );
                builder.add_node(ProjectGraphNode {
                    id: id.clone(),
                    label: dependency
                        .to
                        .rsplit(['/', '.'])
                        .next()
                        .unwrap_or(&dependency.to)
                        .to_string(),
                    kind: ProjectGraphNodeKind::UnrealAsset,
                    path: None,
                    summary: "Unreal asset referenced by registry export".to_string(),
                    source: "scanner:unreal_asset_dependency".to_string(),
                    confidence: 0.65,
                    metadata,
                    updated_at: builder.now,
                });
            }
            id
        });
        builder.add_edge(
            &from,
            &to,
            ProjectGraphEdgeKind::References,
            0.95,
            &format!(
                "scanner:unreal_asset_dependency:{}",
                dependency.dependency_type
            ),
        );
    }

    add_unreal_semantic_edges(builder, &intelligence.assets, &asset_lookup);

    let unreal_root = descriptor_ids
        .values()
        .next()
        .cloned()
        .unwrap_or_else(|| "project:root".to_string());
    if let Some(export) = intelligence.registry_export_path {
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, &export),
            &unreal_root,
            ProjectGraphEdgeKind::Documents,
            0.95,
            "scanner:unreal_asset_registry",
        );
    }
}

fn graph_kind_for_unreal_asset(kind: UnrealAssetKind, class_name: &str) -> ProjectGraphNodeKind {
    let class_name = class_name.to_ascii_lowercase();
    if class_name.contains("animblueprint") || class_name.contains("animationblueprint") {
        return ProjectGraphNodeKind::UnrealAnimationBlueprint;
    }
    if class_name.contains("animmontage") || class_name.contains("animationmontage") {
        return ProjectGraphNodeKind::UnrealAnimationMontage;
    }
    if class_name.contains("skeleton") && !class_name.contains("skeletalmesh") {
        return ProjectGraphNodeKind::UnrealSkeleton;
    }
    if class_name.contains("skeletalmesh") {
        return ProjectGraphNodeKind::UnrealSkeletalMesh;
    }
    if class_name.contains("staticmesh") {
        return ProjectGraphNodeKind::UnrealStaticMesh;
    }
    if class_name.contains("controlrig") {
        return ProjectGraphNodeKind::UnrealControlRig;
    }
    if class_name.contains("physicsasset") {
        return ProjectGraphNodeKind::UnrealPhysicsAsset;
    }
    if class_name.contains("sound") || class_name.contains("metasound") {
        return ProjectGraphNodeKind::UnrealSound;
    }
    if class_name.contains("widgetblueprint") || class_name.contains("commonactivatablewidget") {
        return ProjectGraphNodeKind::UnrealWidget;
    }
    if class_name.contains("inputaction") || class_name.contains("inputmappingcontext") {
        return ProjectGraphNodeKind::UnrealInputAsset;
    }
    match kind {
        UnrealAssetKind::Map => ProjectGraphNodeKind::UnrealMap,
        UnrealAssetKind::Blueprint => ProjectGraphNodeKind::UnrealBlueprint,
        UnrealAssetKind::DataAsset => ProjectGraphNodeKind::UnrealDataAsset,
        UnrealAssetKind::Material => ProjectGraphNodeKind::UnrealMaterial,
        UnrealAssetKind::Niagara => ProjectGraphNodeKind::UnrealNiagara,
        UnrealAssetKind::Animation => ProjectGraphNodeKind::UnrealAnimation,
        UnrealAssetKind::Asset => ProjectGraphNodeKind::UnrealAsset,
    }
}

fn add_unreal_semantic_edges(
    builder: &mut GraphBuilder,
    assets: &[crate::unreal_intelligence::UnrealAssetInfo],
    lookup: &BTreeMap<String, String>,
) {
    for asset in assets {
        let source_kind = graph_kind_for_unreal_asset(asset.kind, &asset.class_name);
        for (tag, value) in &asset.tags {
            let tag = tag.to_ascii_lowercase();
            let Some(target) = resolve_unreal_asset_id_flexible(lookup, value) else {
                continue;
            };
            let edge_kind = if tag.contains("skeleton") {
                ProjectGraphEdgeKind::UsesSkeleton
            } else if tag.contains("skeletalmesh") || tag == "previewmesh" {
                if matches!(
                    source_kind,
                    ProjectGraphNodeKind::UnrealAnimation
                        | ProjectGraphNodeKind::UnrealAnimationBlueprint
                        | ProjectGraphNodeKind::UnrealAnimationMontage
                        | ProjectGraphNodeKind::UnrealControlRig
                ) {
                    ProjectGraphEdgeKind::Animates
                } else {
                    ProjectGraphEdgeKind::References
                }
            } else if tag.contains("generatedclass") || tag.contains("parentclass") {
                ProjectGraphEdgeKind::ControlledBy
            } else if tag.contains("input") {
                ProjectGraphEdgeKind::BoundToInput
            } else if tag.contains("physicsasset") {
                ProjectGraphEdgeKind::ControlledBy
            } else {
                continue;
            };
            builder.add_edge(
                &asset.id,
                &target,
                edge_kind,
                0.98,
                &format!("scanner:unreal_asset_registry:tag:{tag}"),
            );
            if edge_kind == ProjectGraphEdgeKind::UsesSkeleton
                && matches!(
                    source_kind,
                    ProjectGraphNodeKind::UnrealAnimation
                        | ProjectGraphNodeKind::UnrealAnimationBlueprint
                        | ProjectGraphNodeKind::UnrealAnimationMontage
                        | ProjectGraphNodeKind::UnrealSkeletalMesh
                        | ProjectGraphNodeKind::UnrealControlRig
                )
            {
                builder.add_edge(
                    &asset.id,
                    &target,
                    ProjectGraphEdgeKind::CompatibleWith,
                    0.98,
                    "scanner:unreal_asset_registry:skeleton_compatibility",
                );
            }
        }
    }
}

fn resolve_unreal_asset_id_flexible(
    lookup: &BTreeMap<String, String>,
    value: &str,
) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches(['\'', '"'])
        .split_once('\'')
        .map(|(_, path)| path.trim_end_matches('\''))
        .unwrap_or(value.trim());
    let candidates = [
        trimmed.to_string(),
        trimmed.trim_end_matches("_C").to_string(),
        trimmed.split(':').next().unwrap_or(trimmed).to_string(),
    ];
    candidates
        .iter()
        .find_map(|candidate| resolve_unreal_asset_id(lookup, candidate))
}

fn resolve_unreal_asset_id(lookup: &BTreeMap<String, String>, value: &str) -> Option<String> {
    lookup.get(value).cloned().or_else(|| {
        let package = value.split('.').next().unwrap_or(value);
        lookup.get(package).cloned()
    })
}

fn merge_incremental_graph(
    previous: Option<ProjectGraphState>,
    mut fresh: ProjectGraphState,
) -> ProjectGraphState {
    let Some(previous) = previous else {
        return fresh;
    };
    let previous_nodes = previous
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    for node in &mut fresh.nodes {
        if let Some(old) = previous_nodes.get(&node.id) {
            if graph_node_content_eq(old, node) {
                node.updated_at = old.updated_at;
            }
        }
    }
    let fresh_ids = fresh
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    fresh.nodes.extend(
        previous
            .nodes
            .iter()
            .filter(|node| node.source.starts_with("ui:") && !fresh_ids.contains(&node.id))
            .cloned(),
    );

    let previous_edges = previous
        .edges
        .iter()
        .map(|edge| (edge.id.clone(), edge))
        .collect::<BTreeMap<_, _>>();
    for edge in &mut fresh.edges {
        if let Some(old) = previous_edges.get(&edge.id) {
            if graph_edge_content_eq(old, edge) {
                edge.updated_at = old.updated_at;
            }
        }
    }
    let mut edge_ids = fresh
        .edges
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<BTreeSet<_>>();
    let node_ids = fresh
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    for edge in previous.edges {
        if edge.source.starts_with("ui:")
            && node_ids.contains(&edge.from)
            && node_ids.contains(&edge.to)
            && edge_ids.insert(edge.id.clone())
        {
            fresh.edges.push(edge);
        }
    }
    fresh.nodes.sort_by(|left, right| left.id.cmp(&right.id));
    fresh.edges.sort_by(|left, right| left.id.cmp(&right.id));
    fresh
}

fn graph_node_content_eq(left: &ProjectGraphNode, right: &ProjectGraphNode) -> bool {
    left.id == right.id
        && left.label == right.label
        && left.kind == right.kind
        && left.path == right.path
        && left.summary == right.summary
        && left.source == right.source
        && (left.confidence - right.confidence).abs() < f32::EPSILON
        && left.metadata == right.metadata
}

fn graph_edge_content_eq(left: &ProjectGraphEdge, right: &ProjectGraphEdge) -> bool {
    left.id == right.id
        && left.from == right.from
        && left.to == right.to
        && left.kind == right.kind
        && left.label == right.label
        && left.source == right.source
        && (left.confidence - right.confidence).abs() < f32::EPSILON
}

struct GraphBuilder {
    title: String,
    now: u64,
    nodes: BTreeMap<String, ProjectGraphNode>,
    edges: BTreeMap<String, ProjectGraphEdge>,
}

impl GraphBuilder {
    fn new(title: String, now: u64) -> Self {
        let mut builder = Self {
            title,
            now,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        };
        builder.add_node(ProjectGraphNode {
            id: "project:root".to_string(),
            label: builder.title.clone(),
            kind: ProjectGraphNodeKind::Project,
            path: Some(".".to_string()),
            summary: "Корень выбранного проекта".to_string(),
            source: "scanner:workspace".to_string(),
            confidence: 1.0,
            metadata: BTreeMap::new(),
            updated_at: now,
        });
        builder
    }

    fn has_node(&self, id: &str) -> bool {
        self.nodes.contains_key(id)
    }

    fn add_node(&mut self, node: ProjectGraphNode) {
        if let Some(existing) = self.nodes.get_mut(&node.id) {
            if node.confidence >= existing.confidence {
                *existing = node;
            }
        } else {
            self.nodes.insert(node.id.clone(), node);
        }
    }

    fn add_edge(
        &mut self,
        from: &str,
        to: &str,
        kind: ProjectGraphEdgeKind,
        confidence: f32,
        source: &str,
    ) {
        if from == to {
            return;
        }
        let id = format!("edge:{}:{}->{}", kind.as_str(), from, to);
        self.edges.entry(id.clone()).or_insert(ProjectGraphEdge {
            id,
            from: from.to_string(),
            to: to.to_string(),
            kind,
            label: kind.label().to_string(),
            source: source.to_string(),
            confidence,
            updated_at: self.now,
        });
    }

    fn finish(self) -> ProjectGraphState {
        ProjectGraphState {
            schema_version: schema_version(),
            title: self.title,
            focus: "Файлы, команды, память и roadmap проекта".to_string(),
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_values().collect(),
            updated_at: self.now,
        }
    }
}

fn bounded_graph(
    mut graph: ProjectGraphState,
    max_nodes: Option<usize>,
    include_edges: bool,
) -> serde_json::Value {
    if let Some(max_nodes) = max_nodes {
        if graph.nodes.len() > max_nodes {
            graph.nodes.truncate(max_nodes);
        }
        let allowed = graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        graph
            .edges
            .retain(|edge| allowed.contains(&edge.from) && allowed.contains(&edge.to));
    }
    if !include_edges {
        graph.edges.clear();
    }
    json!({
        "path": PROJECT_GRAPH_PATH,
        "node_count": graph.nodes.len(),
        "edge_count": graph.edges.len(),
        "graph": graph,
    })
}

fn default_include_edges() -> bool {
    true
}

fn schema_version() -> u32 {
    6
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn path_node_id(kind: ProjectGraphNodeKind, rel: &str) -> String {
    match kind {
        ProjectGraphNodeKind::Folder => folder_node_id(rel),
        _ => format!("file:{}", normalize_rel(rel)),
    }
}

fn folder_node_id(rel: &str) -> String {
    format!("folder:{}", normalize_rel(rel))
}

fn stable_id(prefix: &str, value: &str) -> String {
    let cleaned = value
        .trim()
        .replace('\\', "/")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | ':' | '_' | '-' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{prefix}:{cleaned}")
}

fn normalize_rel(rel: &str) -> String {
    rel.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn parent_path(rel: &str) -> Option<String> {
    let rel = normalize_rel(rel);
    rel.rsplit_once('/').map(|(parent, _)| parent.to_string())
}

fn file_label(rel: &str) -> String {
    normalize_rel(rel)
        .rsplit('/')
        .next()
        .unwrap_or(rel)
        .to_string()
}

fn file_extension(rel: &str) -> Option<String> {
    file_label(rel)
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .filter(|extension| !extension.trim().is_empty())
}

fn is_asset_path(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.starts_with("assets/")
        || lower.starts_with("public/")
        || matches!(
            file_extension(&lower).as_deref(),
            Some("png")
                | Some("jpg")
                | Some("jpeg")
                | Some("webp")
                | Some("gif")
                | Some("svg")
                | Some("wav")
                | Some("mp3")
                | Some("ogg")
                | Some("opus")
                | Some("mp4")
                | Some("webm")
        )
}

fn is_test_command(id: &str, command: &str) -> bool {
    let text = format!("{id} {command}").to_ascii_lowercase();
    ["test", "check", "lint", "fmt", "clippy"]
        .iter()
        .any(|needle| text.contains(needle))
}

fn cargo_dependencies(text: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_dependency_section = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_matches(&['[', ']'][..]).trim();
            in_dependency_section = matches!(
                section,
                "dependencies"
                    | "dev-dependencies"
                    | "build-dependencies"
                    | "target.'cfg(windows)'.dependencies"
            );
            continue;
        }
        if !in_dependency_section || line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim().trim_matches('"');
            if !name.is_empty() {
                deps.push(name.to_string());
            }
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn npm_dependencies(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for key in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(object) = value.get(key).and_then(serde_json::Value::as_object) {
            deps.extend(object.keys().cloned());
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn rust_module_name(rel: &str) -> String {
    let rel = normalize_rel(rel);
    let rel = rel.trim_start_matches("src/");
    let rel = rel.trim_end_matches(".rs");
    match rel {
        "main" | "lib" => "crate".to_string(),
        value if value.ends_with("/mod") => value.trim_end_matches("/mod").replace('/', "::"),
        value => value.replace('/', "::"),
    }
}

fn rust_imports(text: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("use crate::") {
            let name = rest
                .trim_end_matches(';')
                .split(&[':', '{', ' ', ';'][..])
                .filter(|part| !part.is_empty())
                .take(3)
                .collect::<Vec<_>>()
                .join("::");
            if !name.is_empty() {
                imports.push(name);
            }
        } else if let Some(rest) = line.strip_prefix("mod ") {
            let name = rest.trim_end_matches(';').trim();
            if !name.is_empty() {
                imports.push(name.to_string());
            }
        }
    }
    imports.sort();
    imports.dedup();
    imports
}

fn compact_inline(value: &str, max_chars: usize) -> String {
    let one_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max_chars {
        return one_line;
    }
    let mut compacted = one_line
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    compacted.push('…');
    compacted
}

fn empty_label(value: &str) -> String {
    if value.trim().is_empty() {
        "нет".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_files_profiles_dependencies_and_modules() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src/agent")).unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("src/main.rs"),
            "mod agent;\nuse crate::agent::types;\n",
        )
        .unwrap();
        fs::write(temp.path().join("src/agent/types.rs"), "pub struct Demo;\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let graph = scan_project_graph(&workspace);

        assert!(graph.nodes.iter().any(|node| node.id == "project:root"));
        assert!(graph.nodes.iter().any(|node| node.id == "file:Cargo.toml"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "module:dependency:cargo:serde"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.kind == ProjectGraphNodeKind::Command && node.label == "Проверка"));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::Contains));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::Imports));
    }

    #[test]
    fn snapshot_can_save_graph_file() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let result = project_graph_snapshot(
            &workspace,
            ProjectGraphSnapshotArgs {
                save_if_missing: true,
                refresh: false,
                include_edges: false,
                max_nodes: Some(4),
            },
        );

        assert!(result.ok);
        assert!(workspace.read_text(PROJECT_GRAPH_PATH, 1_000_000).is_ok());
        assert!(result.output.contains("\"edge_count\": 0"));
    }

    #[test]
    fn scans_persisted_3d_asset_jobs_as_distinct_nodes() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        workspace
            .write_text(
                crate::asset_3d::THREE_D_JOBS_PATH,
                &serde_json::to_string_pretty(&serde_json::json!([{
                    "id": "3d-test",
                    "provider": "meshy-3d",
                    "model": "latest",
                    "input_kind": "text",
                    "prompt": "low-poly sci-fi crate",
                    "source_image": null,
                    "target_format": "glb",
                    "target_polycount": 12000,
                    "enable_pbr": true,
                    "pose_mode": "",
                    "license_confirmed": true,
                    "provider_task_id": "task-1",
                    "provider_task_kind": "text-preview",
                    "status": "ready",
                    "stage": "ready",
                    "progress": 100,
                    "output_files": ["assets/generated/3d/crate.glb"],
                    "validation": null,
                    "provider_payload": {},
                    "error": null,
                    "created_at": 1,
                    "updated_at": 2
                }]))
                .unwrap(),
            )
            .unwrap();

        let graph = scan_project_graph(&workspace);
        let node = graph
            .nodes
            .iter()
            .find(|node| node.kind == ProjectGraphNodeKind::ThreeDAsset)
            .expect("3D asset node");
        assert_eq!(node.metadata.get("provider").unwrap(), "meshy-3d");
        assert_eq!(node.metadata.get("progress").unwrap(), "100");
        assert!(graph
            .edges
            .iter()
            .any(|edge| { edge.to == node.id && edge.kind == ProjectGraphEdgeKind::Generates }));
    }

    #[test]
    fn links_gameplay_plans_and_playtest_runs() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Gameplay fixture\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = crate::unreal_gameplay::create_gameplay_plan(
            &workspace,
            crate::unreal_gameplay::CreateGameplayPlanArgs {
                recipe: crate::unreal_gameplay::GameplayRecipeKind::Interaction,
                title: Some("Door interaction".to_string()),
                brief: "Open a selected gameplay door".to_string(),
                map_path: Some("/Game/Maps/L_Test".to_string()),
                task_ids: None,
                roadmap_ids: None,
            },
        )
        .unwrap();
        crate::unreal_gameplay::record_playtest_result(
            &workspace,
            &crate::unreal_gameplay::GameplayPlaytestCommand {
                id: "playtest-1".to_string(),
                plan_id: Some(plan.id.clone()),
                mode: crate::unreal_gameplay::UnrealPlaytestMode::MapSmoke,
                map_path: plan.map_path.clone(),
                test_filter: String::new(),
                report_dir: "assets/generated/leetcode/unreal/gameplay/runs/playtest-1".to_string(),
                shell_command: "fixture".to_string(),
                timeout_secs: 30,
                started_at: 1,
            },
            true,
            "map smoke passed",
            250,
        )
        .unwrap();

        let graph = scan_project_graph(&workspace);
        let plan_node = graph
            .nodes
            .iter()
            .find(|node| node.kind == ProjectGraphNodeKind::GameplayPlan)
            .expect("gameplay plan node");
        let run_node = graph
            .nodes
            .iter()
            .find(|node| node.kind == ProjectGraphNodeKind::GameplayRun)
            .expect("gameplay run node");
        assert!(graph.edges.iter().any(|edge| {
            edge.from == plan_node.id
                && edge.to == run_node.id
                && edge.kind == ProjectGraphEdgeKind::Tests
        }));
    }

    #[test]
    fn links_game_production_plan_items_and_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Production fixture\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        crate::game_production::create_game_production_plan(
            &workspace,
            crate::game_production::CreateGameProductionPlanArgs {
                title: "Vertical Slice".to_string(),
                brief: "Build a representative Unreal vertical slice".to_string(),
                genre: "Action".to_string(),
                target_platform: "Windows".to_string(),
                scope: crate::game_production::GameScope::VerticalSlice,
                source_task_ids: Vec::new(),
                roadmap_ids: Vec::new(),
                project_node_id: None,
            },
        )
        .unwrap();

        let graph = scan_project_graph(&workspace);
        let plan_node = graph
            .nodes
            .iter()
            .find(|node| node.kind == ProjectGraphNodeKind::GameProductionPlan)
            .expect("game production plan node");
        let production_items = graph
            .nodes
            .iter()
            .filter(|node| node.kind == ProjectGraphNodeKind::ProductionItem)
            .collect::<Vec<_>>();

        assert!(production_items.len() >= 12);
        assert!(graph.edges.iter().any(|edge| {
            edge.from == plan_node.id
                && edge.kind == ProjectGraphEdgeKind::Contains
                && production_items.iter().any(|node| node.id == edge.to)
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == ProjectGraphEdgeKind::DependsOn
                && production_items.iter().any(|node| node.id == edge.from)
                && production_items.iter().any(|node| node.id == edge.to)
        }));
    }

    #[test]
    fn links_vertical_slice_run_phases_and_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Vertical slice fixture\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = crate::game_production::create_game_production_plan(
            &workspace,
            crate::game_production::CreateGameProductionPlanArgs {
                title: "Slice".to_string(),
                brief: "Representative slice".to_string(),
                genre: "Action".to_string(),
                target_platform: "Windows".to_string(),
                scope: crate::game_production::GameScope::VerticalSlice,
                source_task_ids: Vec::new(),
                roadmap_ids: Vec::new(),
                project_node_id: None,
            },
        )
        .unwrap();
        crate::vertical_slice::start_vertical_slice_run(
            &workspace,
            crate::vertical_slice::StartVerticalSliceRunArgs {
                production_plan_id: Some(plan.id),
                title: None,
            },
        )
        .unwrap();

        let graph = scan_project_graph(&workspace);
        let run_node = graph
            .nodes
            .iter()
            .find(|node| node.kind == ProjectGraphNodeKind::VerticalSliceRun)
            .expect("vertical slice run node");
        let phases = graph
            .nodes
            .iter()
            .filter(|node| node.kind == ProjectGraphNodeKind::VerticalSlicePhase)
            .collect::<Vec<_>>();

        assert_eq!(phases.len(), 7);
        assert!(graph.edges.iter().any(|edge| {
            edge.from == run_node.id
                && edge.kind == ProjectGraphEdgeKind::Contains
                && phases.iter().any(|node| node.id == edge.to)
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == ProjectGraphEdgeKind::DependsOn
                && phases.iter().any(|node| node.id == edge.from)
                && phases.iter().any(|node| node.id == edge.to)
        }));
    }

    #[test]
    fn scans_unreal_project_types_and_asset_dependencies() {
        let workspace = Workspace::new(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/unreal/SampleGame"),
        )
        .unwrap();

        let graph = scan_project_graph(&workspace);

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.kind == ProjectGraphNodeKind::UnrealProject));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealModule && node.label == "SampleGame"
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealBlueprint && node.label == "BP_Player"
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealMap && node.label == "L_Test"
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealSkeleton && node.label == "SKEL_Hero"
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealSkeletalMesh && node.label == "SK_Hero"
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealAnimationBlueprint && node.label == "ABP_Hero"
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.kind == ProjectGraphNodeKind::UnrealStaticMesh && node.label == "SM_Bucket"
        }));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::UsesSkeleton));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::Animates));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::References));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::DependsOn));
    }

    #[test]
    fn incremental_refresh_preserves_selection_manual_edges_and_stable_timestamps() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        fs::write(temp.path().join("notes.md"), "notes\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let mut graph = scan_project_graph(&workspace);
        let readme = graph
            .nodes
            .iter_mut()
            .find(|node| node.id == "file:README.md")
            .unwrap();
        readme.updated_at = 7;
        graph.edges.push(ProjectGraphEdge {
            id: "edge:related_to:file:README.md:file:notes.md".to_string(),
            from: "file:README.md".to_string(),
            to: "file:notes.md".to_string(),
            kind: ProjectGraphEdgeKind::RelatedTo,
            label: "связано с".to_string(),
            source: "ui:project_map".to_string(),
            confidence: 0.8,
            updated_at: 7,
        });
        save_project_graph(&workspace, &graph).unwrap();
        save_project_graph_selection(&workspace, Some("file:README.md")).unwrap();

        let refreshed = refresh_project_graph(&workspace);

        assert_eq!(
            refreshed
                .nodes
                .iter()
                .find(|node| node.id == "file:README.md")
                .unwrap()
                .updated_at,
            7
        );
        assert!(refreshed
            .edges
            .iter()
            .any(|edge| edge.source == "ui:project_map"));
        assert_eq!(
            load_project_graph_selection(&workspace).as_deref(),
            Some("file:README.md")
        );
        assert!(selected_project_node_context_for_prompt(&workspace)
            .unwrap()
            .contains("file:README.md"));
    }
}
