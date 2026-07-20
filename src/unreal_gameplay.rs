use crate::project_graph::load_project_graph_selection;
use crate::unreal::{parse_unreal_log, unreal_snapshot, UnrealLogIssue};
use crate::workspace::Workspace;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub const GAMEPLAY_STATE_PATH: &str = "assets/generated/leetcode/unreal/gameplay/state.json";
pub const GAMEPLAY_MANIFEST_DIR: &str = "assets/generated/leetcode/unreal/gameplay/manifests";
pub const GAMEPLAY_RUN_DIR: &str = "assets/generated/leetcode/unreal/gameplay/runs";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameplayRecipeKind {
    LevelBootstrap,
    ThirdPersonLoop,
    Interaction,
    PickupAndInventory,
    Checkpoint,
    EnemyEncounter,
    PcgEnvironment,
    NiagaraFeedback,
    EnhancedInput,
    GameHud,
}

impl GameplayRecipeKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::LevelBootstrap => "level_bootstrap",
            Self::ThirdPersonLoop => "third_person_loop",
            Self::Interaction => "interaction",
            Self::PickupAndInventory => "pickup_and_inventory",
            Self::Checkpoint => "checkpoint",
            Self::EnemyEncounter => "enemy_encounter",
            Self::PcgEnvironment => "pcg_environment",
            Self::NiagaraFeedback => "niagara_feedback",
            Self::EnhancedInput => "enhanced_input",
            Self::GameHud => "game_hud",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LevelBootstrap => "Базовый уровень",
            Self::ThirdPersonLoop => "Third-person loop",
            Self::Interaction => "Взаимодействие",
            Self::PickupAndInventory => "Предметы и инвентарь",
            Self::Checkpoint => "Checkpoint",
            Self::EnemyEncounter => "Столкновение с противником",
            Self::PcgEnvironment => "PCG-окружение",
            Self::NiagaraFeedback => "Niagara feedback",
            Self::EnhancedInput => "Enhanced Input",
            Self::GameHud => "Игровой HUD",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameplayPlanStatus {
    Planned,
    Applied,
    Validated,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameplayPlan {
    pub id: String,
    pub title: String,
    pub brief: String,
    pub recipe: GameplayRecipeKind,
    pub map_path: String,
    pub status: GameplayPlanStatus,
    pub systems: Vec<String>,
    pub implementation_steps: Vec<String>,
    pub validation_steps: Vec<String>,
    pub file_path: String,
    pub project_node_id: Option<String>,
    #[serde(default)]
    pub task_ids: Vec<String>,
    #[serde(default)]
    pub roadmap_ids: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameplayRun {
    pub id: String,
    pub plan_id: Option<String>,
    pub mode: UnrealPlaytestMode,
    pub map_path: String,
    pub test_filter: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub issues: Vec<UnrealLogIssue>,
    pub output_summary: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct GameplayState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub plans: Vec<GameplayPlan>,
    #[serde(default)]
    pub runs: Vec<GameplayRun>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateGameplayPlanArgs {
    pub recipe: GameplayRecipeKind,
    pub title: Option<String>,
    pub brief: String,
    pub map_path: Option<String>,
    pub task_ids: Option<Vec<String>>,
    pub roadmap_ids: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameplayOperationKind {
    LoadLevel,
    CreateLevel,
    SpawnActor,
    AddActorComponent,
    DeleteActor,
    SetActorTransform,
    SetActorProperty,
    CreateDataAsset,
    SaveLevel,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameplayOperation {
    pub operation: GameplayOperationKind,
    pub actor_label: Option<String>,
    pub component_name: Option<String>,
    pub class_path: Option<String>,
    pub asset_path: Option<String>,
    pub package_path: Option<String>,
    pub property: Option<String>,
    pub value: Option<Value>,
    pub location: Option<[f64; 3]>,
    pub rotation: Option<[f64; 3]>,
    pub scale: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApplyGameplayPlanArgs {
    pub plan_id: Option<String>,
    pub map_path: String,
    pub create_map: Option<bool>,
    pub operations: Vec<GameplayOperation>,
    pub save_level: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameplayManifest {
    pub schema_version: u32,
    pub id: String,
    pub plan_id: Option<String>,
    pub map_path: String,
    pub create_map: bool,
    pub operations: Vec<GameplayOperation>,
    pub save_level: bool,
    pub result_path: String,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub struct GameplayApplyCommand {
    pub manifest_path: String,
    pub result_path: String,
    pub shell_command: String,
    pub timeout_secs: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnrealPlaytestMode {
    Automation,
    MapSmoke,
    MovieRender,
}

impl UnrealPlaytestMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Automation => "automation",
            Self::MapSmoke => "map_smoke",
            Self::MovieRender => "movie_render",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RunGameplayPlaytestArgs {
    pub plan_id: Option<String>,
    pub mode: UnrealPlaytestMode,
    pub map_path: Option<String>,
    pub test_filter: Option<String>,
    pub level_sequence: Option<String>,
    pub movie_pipeline_config: Option<String>,
    pub capture_screenshot: Option<bool>,
    pub timeout_secs: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct GameplayPlaytestCommand {
    pub id: String,
    pub plan_id: Option<String>,
    pub mode: UnrealPlaytestMode,
    pub map_path: String,
    pub test_filter: String,
    pub report_dir: String,
    pub shell_command: String,
    pub timeout_secs: u64,
    pub started_at: u64,
}

pub fn load_gameplay_state(workspace: &Workspace) -> GameplayState {
    workspace
        .read_text(GAMEPLAY_STATE_PATH, 8_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_gameplay_state(workspace: &Workspace, state: &GameplayState) -> Result<()> {
    workspace.write_text(GAMEPLAY_STATE_PATH, &serde_json::to_string_pretty(state)?)
}

pub fn gameplay_snapshot(workspace: &Workspace) -> Value {
    let state = load_gameplay_state(workspace);
    serde_json::json!({
        "schema_version": schema_version(),
        "selected_project_node": load_project_graph_selection(workspace),
        "plans": state.plans,
        "runs": state.runs.iter().rev().take(20).collect::<Vec<_>>(),
        "recipes": gameplay_recipe_specs(),
    })
}

pub fn create_gameplay_plan(
    workspace: &Workspace,
    args: CreateGameplayPlanArgs,
) -> Result<GameplayPlan> {
    let map_path =
        normalize_content_path(args.map_path.as_deref().unwrap_or("/Game/Maps/L_Gameplay"))?;
    let brief = args.brief.trim().to_string();
    if brief.is_empty() {
        anyhow::bail!("Gameplay brief is required");
    }
    let id = format!("gameplay-{}", compact_id());
    let title = args
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| args.recipe.label().to_string());
    let (systems, implementation_steps, validation_steps) = recipe_content(args.recipe);
    let file_path = format!("docs/gameplay/{}-{id}.md", slug(&title));
    let now = unix_timestamp();
    let plan = GameplayPlan {
        id,
        title,
        brief,
        recipe: args.recipe,
        map_path,
        status: GameplayPlanStatus::Planned,
        systems,
        implementation_steps,
        validation_steps,
        file_path: file_path.clone(),
        project_node_id: load_project_graph_selection(workspace),
        task_ids: normalize_ids(args.task_ids.unwrap_or_default()),
        roadmap_ids: normalize_ids(args.roadmap_ids.unwrap_or_default()),
        created_at: now,
        updated_at: now,
    };
    workspace.write_text(&file_path, &render_plan_markdown(&plan))?;
    let mut state = load_gameplay_state(workspace);
    state.plans.push(plan.clone());
    save_gameplay_state(workspace, &state)?;
    Ok(plan)
}

pub fn build_gameplay_apply_command(
    workspace: &Workspace,
    args: ApplyGameplayPlanArgs,
) -> Result<GameplayApplyCommand> {
    let snapshot = unreal_snapshot(workspace);
    let project = snapshot.project.as_ref().context("No .uproject found")?;
    let editor_cmd = snapshot
        .selected_engine
        .as_ref()
        .and_then(|engine| engine.tools.editor_cmd.as_deref())
        .context("UnrealEditor-Cmd was not found")?;
    let script = workspace.resolve_existing("scripts/unreal/apply_gameplay_manifest.py")?;
    let map_path = normalize_content_path(&args.map_path)?;
    validate_operations(&args.operations)?;
    let id = uuid::Uuid::new_v4().to_string();
    let manifest_rel = format!("{GAMEPLAY_MANIFEST_DIR}/{id}.json");
    let result_rel = format!("{GAMEPLAY_MANIFEST_DIR}/{id}.result.json");
    let manifest_path = workspace.resolve_for_write(&manifest_rel)?;
    let result_path = workspace.resolve_for_write(&result_rel)?;
    let manifest = GameplayManifest {
        schema_version: 1,
        id,
        plan_id: args.plan_id,
        map_path,
        create_map: args.create_map.unwrap_or(false),
        operations: args.operations,
        save_level: args.save_level.unwrap_or(true),
        result_path: path_string(&result_path),
        created_at: unix_timestamp(),
    };
    workspace.write_text(&manifest_rel, &serde_json::to_string_pretty(&manifest)?)?;
    let shell_command = format!(
        "$env:LEETCODE_GAMEPLAY_MANIFEST={}; & {} {} -Unattended -NoSplash -NoP4 -ExecutePythonScript={} -log -UTF8Output",
        powershell_quote(&path_string(&manifest_path)),
        powershell_quote(editor_cmd),
        powershell_quote(&project.path),
        powershell_quote(&path_string(&script)),
    );
    Ok(GameplayApplyCommand {
        manifest_path: manifest_rel,
        result_path: result_rel,
        shell_command,
        timeout_secs: 1_800,
    })
}

pub fn build_gameplay_playtest_command(
    workspace: &Workspace,
    args: RunGameplayPlaytestArgs,
) -> Result<GameplayPlaytestCommand> {
    let snapshot = unreal_snapshot(workspace);
    let project = snapshot.project.as_ref().context("No .uproject found")?;
    let editor_cmd = snapshot
        .selected_engine
        .as_ref()
        .and_then(|engine| engine.tools.editor_cmd.as_deref())
        .context("UnrealEditor-Cmd was not found")?;
    let map_path =
        normalize_content_path(args.map_path.as_deref().unwrap_or("/Game/Maps/L_Gameplay"))?;
    let test_filter = if args.mode == UnrealPlaytestMode::Automation {
        normalize_test_filter(args.test_filter.as_deref().unwrap_or("Project.Gameplay"))?
    } else {
        String::new()
    };
    let id = format!("playtest-{}", compact_id());
    let report_dir = format!("{GAMEPLAY_RUN_DIR}/{id}/automation");
    let report_abs = workspace.resolve_for_write(&report_dir)?;
    let capture = args.capture_screenshot.unwrap_or(true);
    let mut command_args = vec![
        project.path.clone(),
        "-Unattended".to_string(),
        "-NoSplash".to_string(),
        "-NoP4".to_string(),
        "-UTF8Output".to_string(),
        "-log".to_string(),
    ];
    match args.mode {
        UnrealPlaytestMode::Automation => {
            if capture {
                command_args.extend([
                    "-RenderOffscreen".to_string(),
                    "-ResX=1280".to_string(),
                    "-ResY=720".to_string(),
                ]);
            } else {
                command_args.push("-NullRHI".to_string());
            }
            command_args.extend([
                format!("-ExecCmds=Automation RunTests {test_filter};Quit"),
                "-TestExit=Automation Test Queue Empty".to_string(),
                format!("-ReportExportPath={}", path_string(&report_abs)),
            ]);
        }
        UnrealPlaytestMode::MapSmoke => {
            command_args.extend([
                map_path.clone(),
                "-game".to_string(),
                "-RenderOffscreen".to_string(),
                "-ResX=1280".to_string(),
                "-ResY=720".to_string(),
                if capture {
                    "-ExecCmds=Shot".to_string()
                } else {
                    "-ExecCmds=stat fps".to_string()
                },
                "-benchmark".to_string(),
                "-seconds=10".to_string(),
            ]);
        }
        UnrealPlaytestMode::MovieRender => {
            let level_sequence = normalize_content_path(
                args.level_sequence
                    .as_deref()
                    .context("movie_render requires level_sequence")?,
            )?;
            let pipeline_config = normalize_content_path(
                args.movie_pipeline_config
                    .as_deref()
                    .context("movie_render requires movie_pipeline_config")?,
            )?;
            command_args.extend([
                map_path.clone(),
                "-game".to_string(),
                format!("-LevelSequence={level_sequence}"),
                format!("-MoviePipelineConfig={pipeline_config}"),
                "-RenderOffscreen".to_string(),
                "-ResX=1280".to_string(),
                "-ResY=720".to_string(),
                "-NoTextureStreaming".to_string(),
            ]);
        }
    }
    Ok(GameplayPlaytestCommand {
        id,
        plan_id: args.plan_id,
        mode: args.mode,
        map_path,
        test_filter: if args.mode == UnrealPlaytestMode::MovieRender {
            format!(
                "Movie Render Queue: {}",
                args.level_sequence.as_deref().unwrap_or_default()
            )
        } else {
            test_filter
        },
        report_dir,
        shell_command: render_powershell_command(editor_cmd, &command_args),
        timeout_secs: args.timeout_secs.unwrap_or(1_800).clamp(30, 1_800),
        started_at: unix_timestamp(),
    })
}

pub fn record_apply_result(
    workspace: &Workspace,
    plan_id: Option<&str>,
    success: bool,
) -> Result<()> {
    let Some(plan_id) = plan_id else {
        return Ok(());
    };
    let mut state = load_gameplay_state(workspace);
    if let Some(plan) = state.plans.iter_mut().find(|plan| plan.id == plan_id) {
        plan.status = if success {
            GameplayPlanStatus::Applied
        } else {
            GameplayPlanStatus::Failed
        };
        plan.updated_at = unix_timestamp();
    }
    save_gameplay_state(workspace, &state)
}

pub fn record_playtest_result(
    workspace: &Workspace,
    command: &GameplayPlaytestCommand,
    success: bool,
    output: &str,
    duration_ms: u64,
) -> Result<GameplayRun> {
    let run = GameplayRun {
        id: command.id.clone(),
        plan_id: command.plan_id.clone(),
        mode: command.mode,
        map_path: command.map_path.clone(),
        test_filter: command.test_filter.clone(),
        success,
        duration_ms,
        artifacts: collect_playtest_artifacts(workspace, command.started_at, &command.report_dir),
        issues: parse_unreal_log(output),
        output_summary: compact(output, 4_000),
        created_at: unix_timestamp(),
    };
    let mut state = load_gameplay_state(workspace);
    if let Some(plan_id) = &run.plan_id {
        if let Some(plan) = state.plans.iter_mut().find(|plan| &plan.id == plan_id) {
            plan.status = if success {
                GameplayPlanStatus::Validated
            } else {
                GameplayPlanStatus::Failed
            };
            plan.updated_at = unix_timestamp();
        }
    }
    state.runs.push(run.clone());
    if state.runs.len() > 200 {
        state.runs.drain(0..state.runs.len() - 200);
    }
    save_gameplay_state(workspace, &state)?;
    let run_path = format!("{GAMEPLAY_RUN_DIR}/{}.json", run.id);
    workspace.write_text(&run_path, &serde_json::to_string_pretty(&run)?)?;
    Ok(run)
}

fn gameplay_recipe_specs() -> Vec<Value> {
    [
        GameplayRecipeKind::LevelBootstrap,
        GameplayRecipeKind::ThirdPersonLoop,
        GameplayRecipeKind::Interaction,
        GameplayRecipeKind::PickupAndInventory,
        GameplayRecipeKind::Checkpoint,
        GameplayRecipeKind::EnemyEncounter,
        GameplayRecipeKind::PcgEnvironment,
        GameplayRecipeKind::NiagaraFeedback,
        GameplayRecipeKind::EnhancedInput,
        GameplayRecipeKind::GameHud,
    ]
    .into_iter()
    .map(|kind| serde_json::json!({"id": kind.id(), "label": kind.label()}))
    .collect()
}

fn recipe_content(kind: GameplayRecipeKind) -> (Vec<String>, Vec<String>, Vec<String>) {
    let common_validation = vec![
        "Data Validation без ошибок".to_string(),
        "Automation test для основного happy path".to_string(),
        "Map smoke с логом и screenshot artifact".to_string(),
    ];
    let (systems, steps) = match kind {
        GameplayRecipeKind::LevelBootstrap => (
            vec![
                "Map + World Settings",
                "GameMode/Pawn/Controller",
                "Lighting + PlayerStart",
            ],
            vec![
                "Создать карту",
                "Настроить World Settings",
                "Разместить PlayerStart и базовый свет",
            ],
        ),
        GameplayRecipeKind::ThirdPersonLoop => (
            vec!["Character", "PlayerController", "Camera", "Enhanced Input"],
            vec![
                "Определить input actions",
                "Настроить movement/camera",
                "Добавить reset и debug HUD",
            ],
        ),
        GameplayRecipeKind::Interaction => (
            vec!["Interaction interface", "Trace component", "Prompt UI"],
            vec![
                "Создать контракт взаимодействия",
                "Добавить trace и focus state",
                "Покрыть interact/no-target тестом",
            ],
        ),
        GameplayRecipeKind::PickupAndInventory => (
            vec![
                "Item Data Asset",
                "Pickup Actor",
                "Inventory Component",
                "HUD",
            ],
            vec![
                "Описать item schema",
                "Добавить pickup/inventory",
                "Проверить duplicate/full inventory",
            ],
        ),
        GameplayRecipeKind::Checkpoint => (
            vec!["Checkpoint Actor", "SaveGame", "Respawn flow"],
            vec![
                "Определить checkpoint state",
                "Сохранить минимальные данные",
                "Проверить смерть и respawn",
            ],
        ),
        GameplayRecipeKind::EnemyEncounter => (
            vec![
                "AIController",
                "Behavior Tree",
                "Blackboard",
                "Encounter trigger",
            ],
            vec![
                "Настроить perception",
                "Собрать state flow",
                "Проверить spawn/combat/cleanup",
            ],
        ),
        GameplayRecipeKind::PcgEnvironment => (
            vec!["PCG Graph", "Data Layers", "World Partition"],
            vec![
                "Определить seed и bounds",
                "Собрать deterministic graph",
                "Проверить regenerate и performance budget",
            ],
        ),
        GameplayRecipeKind::NiagaraFeedback => (
            vec!["Niagara System", "Gameplay event", "Audio/Camera feedback"],
            vec![
                "Определить event payload",
                "Привязать Niagara spawn",
                "Проверить pooling и scalability",
            ],
        ),
        GameplayRecipeKind::EnhancedInput => (
            vec!["Input Actions", "Mapping Context", "Player Controller"],
            vec![
                "Описать action/value types",
                "Настроить context priorities",
                "Проверить keyboard/gamepad conflicts",
            ],
        ),
        GameplayRecipeKind::GameHud => (
            vec!["UMG Widgets", "HUD/ViewModel", "Input mode"],
            vec![
                "Определить UI state",
                "Собрать widgets без polling",
                "Проверить DPI, focus и gamepad navigation",
            ],
        ),
    };
    (
        systems.into_iter().map(str::to_string).collect(),
        steps.into_iter().map(str::to_string).collect(),
        common_validation,
    )
}

fn render_plan_markdown(plan: &GameplayPlan) -> String {
    let list = |values: &[String]| {
        values
            .iter()
            .map(|value| format!("- {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "# {}\n\n## Бриф\n{}\n\n## Recipe\n`{:?}`\n\n## Карта\n`{}`\n\n## Системы\n{}\n\n## Реализация\n{}\n\n## Проверка\n{}\n\n## Связи\n- Project Map node: {}\n- Tasks: {}\n- Roadmap: {}\n",
        plan.title,
        plan.brief,
        plan.recipe,
        plan.map_path,
        list(&plan.systems),
        list(&plan.implementation_steps),
        list(&plan.validation_steps),
        plan.project_node_id.as_deref().unwrap_or("нет"),
        if plan.task_ids.is_empty() { "нет".to_string() } else { plan.task_ids.join(", ") },
        if plan.roadmap_ids.is_empty() { "нет".to_string() } else { plan.roadmap_ids.join(", ") },
    )
}

fn validate_operations(operations: &[GameplayOperation]) -> Result<()> {
    if operations.len() > 128 {
        anyhow::bail!("Gameplay manifest is limited to 128 operations");
    }
    for operation in operations {
        if let Some(label) = &operation.actor_label {
            validate_label(label)?;
        }
        if let Some(component_name) = &operation.component_name {
            validate_label(component_name)?;
        }
        for path in [
            operation.class_path.as_deref(),
            operation.asset_path.as_deref(),
            operation.package_path.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            normalize_object_path(path)?;
        }
        if operation.operation == GameplayOperationKind::SetActorProperty {
            let property = operation
                .property
                .as_deref()
                .context("property is required")?;
            if !matches!(
                property,
                "actor_label" | "hidden" | "can_be_damaged" | "tags" | "folder_path"
            ) {
                anyhow::bail!("Property is not in the safe gameplay allowlist: {property}");
            }
            validate_safe_value(operation.value.as_ref().context("value is required")?)?;
        }
        match operation.operation {
            GameplayOperationKind::SpawnActor => {
                if operation.class_path.is_none() && operation.asset_path.is_none() {
                    anyhow::bail!("spawn_actor requires class_path or asset_path");
                }
            }
            GameplayOperationKind::AddActorComponent => {
                operation
                    .actor_label
                    .as_deref()
                    .context("add_actor_component requires actor_label")?;
                operation
                    .class_path
                    .as_deref()
                    .context("add_actor_component requires class_path")?;
                operation
                    .component_name
                    .as_deref()
                    .context("add_actor_component requires component_name")?;
            }
            GameplayOperationKind::DeleteActor
            | GameplayOperationKind::SetActorTransform
            | GameplayOperationKind::SetActorProperty => {
                operation
                    .actor_label
                    .as_deref()
                    .context("actor_label is required for actor mutation")?;
            }
            GameplayOperationKind::CreateDataAsset => {
                operation
                    .class_path
                    .as_deref()
                    .context("create_data_asset requires class_path")?;
                operation
                    .package_path
                    .as_deref()
                    .context("create_data_asset requires package_path")?;
            }
            GameplayOperationKind::LoadLevel
            | GameplayOperationKind::CreateLevel
            | GameplayOperationKind::SaveLevel => {}
        }
    }
    Ok(())
}

fn validate_safe_value(value: &Value) -> Result<()> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
        Value::Array(values) if values.len() <= 64 => {
            for value in values {
                validate_safe_value(value)?;
            }
            Ok(())
        }
        _ => anyhow::bail!("Only scalar values and short scalar arrays are allowed"),
    }
}

fn normalize_content_path(value: &str) -> Result<String> {
    let value = value.trim().replace('\\', "/");
    if !value.starts_with("/Game/")
        || value.contains("..")
        || value.contains([';', '"', '\'', '\n', '\r'])
    {
        anyhow::bail!("Unsafe Unreal content path: {value}");
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn normalize_object_path(value: &str) -> Result<String> {
    let value = value.trim().replace('\\', "/");
    if !(value.starts_with("/Game/") || value.starts_with("/Script/"))
        || value.contains("..")
        || value.contains([';', '"', '\'', '\n', '\r'])
    {
        anyhow::bail!("Unsafe Unreal object path: {value}");
    }
    Ok(value)
}

fn normalize_test_filter(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 160
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | ' '))
    {
        anyhow::bail!("Unsafe Unreal automation filter");
    }
    Ok(value.to_string())
}

fn validate_label(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 128 || value.contains([';', '"', '\'', '\n', '\r'])
    {
        anyhow::bail!("Unsafe or empty actor label");
    }
    Ok(())
}

fn collect_playtest_artifacts(
    workspace: &Workspace,
    started_at: u64,
    report_dir: &str,
) -> Vec<String> {
    let mut artifacts = Vec::new();
    for root in [
        "Saved/AutomationReports",
        "Saved/Screenshots",
        "Saved/MovieRenders",
        "Saved/VideoCaptures",
        GAMEPLAY_RUN_DIR,
        report_dir,
    ] {
        let Ok(path) = workspace.resolve_for_write(root) else {
            continue;
        };
        if !path.exists() {
            continue;
        }
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            let modified = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            if modified + 2 < started_at {
                continue;
            }
            if let Ok(relative) = entry.path().strip_prefix(workspace.root()) {
                artifacts.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    artifacts.sort();
    artifacts.dedup();
    artifacts.truncate(200);
    artifacts
}

fn normalize_ids(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(64);
    values
}

fn render_powershell_command(program: &str, args: &[String]) -> String {
    let mut parts = vec![format!("& {}", powershell_quote(program))];
    parts.extend(args.iter().map(|arg| powershell_quote(arg)));
    parts.join(" ")
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn compact(value: &str, max_chars: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let mut output = value
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        output.push('…');
        output
    }
}

fn slug(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while output.contains("--") {
        output = output.replace("--", "-");
    }
    let output = output.trim_matches('-');
    if output.is_empty() {
        "gameplay".to_string()
    } else {
        output.chars().take(48).collect()
    }
}

fn compact_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

fn schema_version() -> u32 {
    1
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_linked_gameplay_plan() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = create_gameplay_plan(
            &workspace,
            CreateGameplayPlanArgs {
                recipe: GameplayRecipeKind::Interaction,
                title: Some("Interaction slice".to_string()),
                brief: "Player highlights and activates a door".to_string(),
                map_path: Some("/Game/Maps/L_Test".to_string()),
                task_ids: Some(vec!["task-1".to_string()]),
                roadmap_ids: Some(vec!["stage-41".to_string()]),
            },
        )
        .unwrap();
        assert!(workspace.read_text(&plan.file_path, 1_000_000).is_ok());
        assert_eq!(load_gameplay_state(&workspace).plans.len(), 1);
    }

    #[test]
    fn rejects_unsafe_paths_properties_and_filters() {
        assert!(normalize_content_path("/Game/Maps/L_Test").is_ok());
        assert!(normalize_content_path("/Game/../Secret").is_err());
        assert!(normalize_test_filter("Project.Gameplay Interaction").is_ok());
        assert!(normalize_test_filter("Project;Quit").is_err());
        let operation = GameplayOperation {
            operation: GameplayOperationKind::SetActorProperty,
            actor_label: Some("Door".to_string()),
            component_name: None,
            class_path: None,
            asset_path: None,
            package_path: None,
            property: Some("python_command".to_string()),
            value: Some(Value::String("bad".to_string())),
            location: None,
            rotation: None,
            scale: None,
        };
        assert!(validate_operations(&[operation]).is_err());

        let missing_component_name = GameplayOperation {
            operation: GameplayOperationKind::AddActorComponent,
            actor_label: Some("Door".to_string()),
            component_name: None,
            class_path: Some("/Script/Engine.BoxComponent".to_string()),
            asset_path: None,
            package_path: None,
            property: None,
            value: None,
            location: None,
            rotation: None,
            scale: None,
        };
        assert!(validate_operations(&[missing_component_name]).is_err());
        assert!(gameplay_recipe_specs()
            .iter()
            .any(|recipe| recipe["id"] == "pickup_and_inventory"));
    }

    #[test]
    fn state_roundtrips_playtest_result() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let command = GameplayPlaytestCommand {
            id: "run-1".to_string(),
            plan_id: None,
            mode: UnrealPlaytestMode::Automation,
            map_path: "/Game/Maps/L_Test".to_string(),
            test_filter: "Project.Gameplay".to_string(),
            report_dir: "Saved/AutomationReports".to_string(),
            shell_command: "test".to_string(),
            timeout_secs: 30,
            started_at: unix_timestamp(),
        };
        let run =
            record_playtest_result(&workspace, &command, true, "all tests passed", 42).unwrap();
        assert!(run.success);
        assert_eq!(load_gameplay_state(&workspace).runs.len(), 1);
    }
}
