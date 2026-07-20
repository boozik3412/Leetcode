use crate::agent::types::ToolResult;
use crate::asset_3d::{load_3d_jobs, ThreeDJobStatus};
use crate::game_production::{
    evaluate_production_gate, load_game_production_state, EvaluateProductionGateArgs, GameScope,
    ProductionMilestone,
};
use crate::mcp::registry_snapshot;
use crate::unreal::unreal_snapshot;
use crate::unreal_gameplay::{load_gameplay_state, GameplayPlanStatus};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERTICAL_SLICE_STATE_PATH: &str = "assets/generated/leetcode/vertical-slice/state.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalSliceRunStatus {
    Active,
    Blocked,
    Completed,
}

impl VerticalSliceRunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "в работе",
            Self::Blocked => "требует внимания",
            Self::Completed => "готов",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalSlicePhaseKind {
    Preflight,
    GameplayFoundation,
    VisualAssets,
    LevelIntegration,
    Experience,
    Playtest,
    ProductionGate,
}

impl VerticalSlicePhaseKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::GameplayFoundation => "gameplay_foundation",
            Self::VisualAssets => "visual_assets",
            Self::LevelIntegration => "level_integration",
            Self::Experience => "experience",
            Self::Playtest => "playtest",
            Self::ProductionGate => "production_gate",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Preflight => "Unreal preflight",
            Self::GameplayFoundation => "Gameplay foundation",
            Self::VisualAssets => "3D и visual assets",
            Self::LevelIntegration => "Интеграция уровня",
            Self::Experience => "UI, feedback и audio",
            Self::Playtest => "Playtest",
            Self::ProductionGate => "Production gate",
        }
    }

    pub fn recommended_tools(self) -> &'static [&'static str] {
        match self {
            Self::Preflight => &["unreal_snapshot", "mcp_snapshot", "mcp_discover"],
            Self::GameplayFoundation => &[
                "gameplay_snapshot",
                "create_gameplay_plan",
                "apply_gameplay_plan",
            ],
            Self::VisualAssets => &[
                "asset_3d_snapshot",
                "submit_3d_asset",
                "validate_3d_asset",
                "import_3d_asset_unreal",
            ],
            Self::LevelIntegration => &["mcp_call", "apply_gameplay_plan", "unreal_command"],
            Self::Experience => &["generate_image_asset", "generate_audio_asset", "mcp_call"],
            Self::Playtest => &["run_gameplay_playtest"],
            Self::ProductionGate => &["evaluate_production_gate", "production_validation_snapshot"],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalSlicePhaseStatus {
    Planned,
    Ready,
    InProgress,
    Blocked,
    Completed,
}

impl VerticalSlicePhaseStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planned => "ожидает",
            Self::Ready => "готово к работе",
            Self::InProgress => "в работе",
            Self::Blocked => "заблокировано",
            Self::Completed => "готово",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerticalSlicePhase {
    pub kind: VerticalSlicePhaseKind,
    pub title: String,
    pub description: String,
    pub status: VerticalSlicePhaseStatus,
    #[serde(default)]
    pub depends_on: Vec<VerticalSlicePhaseKind>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub notes: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerticalSliceEvent {
    pub phase: VerticalSlicePhaseKind,
    pub status: VerticalSlicePhaseStatus,
    pub detail: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerticalSliceRun {
    pub id: String,
    pub title: String,
    pub production_plan_id: String,
    pub status: VerticalSliceRunStatus,
    #[serde(default)]
    pub phases: Vec<VerticalSlicePhase>,
    #[serde(default)]
    pub events: Vec<VerticalSliceEvent>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerticalSliceState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub active_run_id: Option<String>,
    #[serde(default)]
    pub runs: Vec<VerticalSliceRun>,
}

impl Default for VerticalSliceState {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            active_run_id: None,
            runs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct StartVerticalSliceRunArgs {
    #[serde(default)]
    pub production_plan_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AdvanceVerticalSlicePhaseArgs {
    pub run_id: String,
    pub phase: VerticalSlicePhaseKind,
    pub status: VerticalSlicePhaseStatus,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub artifact: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EvaluateVerticalSliceReadinessArgs {
    pub run_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerticalSliceReadinessCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerticalSliceReadinessReport {
    pub run_id: String,
    pub ready: bool,
    pub completed_phases: usize,
    pub total_phases: usize,
    pub progress_percent: u8,
    pub next_phase: Option<VerticalSlicePhaseKind>,
    pub ready_phases: Vec<VerticalSlicePhaseKind>,
    pub recommended_tools: Vec<String>,
    pub blockers: Vec<String>,
    pub checks: Vec<VerticalSliceReadinessCheck>,
}

pub fn load_vertical_slice_state(workspace: &Workspace) -> VerticalSliceState {
    workspace
        .read_text(VERTICAL_SLICE_STATE_PATH, 4_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_vertical_slice_state(
    workspace: &Workspace,
    state: &VerticalSliceState,
) -> anyhow::Result<()> {
    workspace.write_text(
        VERTICAL_SLICE_STATE_PATH,
        &serde_json::to_string_pretty(state)?,
    )
}

pub fn vertical_slice_snapshot(workspace: &Workspace) -> ToolResult {
    let state = load_vertical_slice_state(workspace);
    let reports = state
        .runs
        .iter()
        .map(|run| evaluate_run_readiness(workspace, run))
        .collect::<Vec<_>>();
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({ "state": state, "readiness": reports }))
            .unwrap_or_else(|_| "vertical slice snapshot".to_string()),
    )
}

pub fn start_vertical_slice_run(
    workspace: &Workspace,
    args: StartVerticalSliceRunArgs,
) -> anyhow::Result<VerticalSliceRun> {
    let production = load_game_production_state(workspace);
    let plan = args
        .production_plan_id
        .as_deref()
        .and_then(|id| production.plans.iter().find(|plan| plan.id == id))
        .or_else(|| {
            production
                .active_plan_id
                .as_deref()
                .and_then(|id| production.plans.iter().find(|plan| plan.id == id))
        })
        .or_else(|| production.plans.last())
        .ok_or_else(|| anyhow::anyhow!("сначала создайте game production plan"))?;
    if plan.scope == GameScope::Prototype {
        anyhow::bail!(
            "production plan имеет scope prototype; для orchestrator нужен vertical_slice или full_game"
        );
    }

    let mut state = load_vertical_slice_state(workspace);
    if let Some(existing) = state.runs.iter().find(|run| {
        run.production_plan_id == plan.id && run.status != VerticalSliceRunStatus::Completed
    }) {
        state.active_run_id = Some(existing.id.clone());
        save_vertical_slice_state(workspace, &state)?;
        return Ok(existing.clone());
    }

    let now = unix_timestamp();
    let id = format!("vertical-slice-{}-{}", now, slug(&plan.title));
    let run = VerticalSliceRun {
        id: id.clone(),
        title: args
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("{} · Vertical Slice", plan.title)),
        production_plan_id: plan.id.clone(),
        status: VerticalSliceRunStatus::Active,
        phases: phase_template(now),
        events: vec![VerticalSliceEvent {
            phase: VerticalSlicePhaseKind::Preflight,
            status: VerticalSlicePhaseStatus::Ready,
            detail: "Vertical Slice orchestration создана".to_string(),
            created_at: now,
        }],
        created_at: now,
        updated_at: now,
    };
    state.runs.push(run.clone());
    state.active_run_id = Some(id);
    save_vertical_slice_state(workspace, &state)?;
    Ok(run)
}

pub fn advance_vertical_slice_phase(
    workspace: &Workspace,
    args: AdvanceVerticalSlicePhaseArgs,
) -> anyhow::Result<VerticalSliceRun> {
    if matches!(
        args.status,
        VerticalSlicePhaseStatus::Planned | VerticalSlicePhaseStatus::Ready
    ) {
        anyhow::bail!("planned/ready управляются orchestrator автоматически");
    }
    let mut state = load_vertical_slice_state(workspace);
    let run = state
        .runs
        .iter_mut()
        .find(|run| run.id == args.run_id)
        .ok_or_else(|| anyhow::anyhow!("vertical slice run не найден: {}", args.run_id))?;
    if run.status == VerticalSliceRunStatus::Completed {
        anyhow::bail!("vertical slice run уже завершён");
    }
    let dependencies_complete = phase_dependencies_complete(run, args.phase);
    if matches!(
        args.status,
        VerticalSlicePhaseStatus::InProgress | VerticalSlicePhaseStatus::Completed
    ) && !dependencies_complete
    {
        anyhow::bail!("зависимости фазы {} ещё не завершены", args.phase.label());
    }

    let now = unix_timestamp();
    let phase = run
        .phases
        .iter_mut()
        .find(|phase| phase.kind == args.phase)
        .ok_or_else(|| anyhow::anyhow!("фаза не найдена: {}", args.phase.id()))?;
    if let Some(evidence) = args.evidence.map(|value| value.trim().to_string()) {
        if !evidence.is_empty() && !phase.evidence.contains(&evidence) {
            phase.evidence.push(evidence);
        }
    }
    if let Some(artifact) = args.artifact.map(|value| value.trim().to_string()) {
        if !artifact.is_empty() && !phase.artifacts.contains(&artifact) {
            workspace.resolve_existing(&artifact)?;
            phase.artifacts.push(artifact);
        }
    }
    if let Some(notes) = args.notes {
        phase.notes = notes.trim().to_string();
    }
    phase.status = args.status;
    phase.updated_at = now;

    if args.status == VerticalSlicePhaseStatus::Completed {
        validate_phase_completion(workspace, run, args.phase)?;
    }
    run.events.push(VerticalSliceEvent {
        phase: args.phase,
        status: args.status,
        detail: phase_event_detail(run, args.phase),
        created_at: now,
    });
    unlock_ready_phases(run, now);
    update_run_status(run, now);
    let updated = run.clone();
    save_vertical_slice_state(workspace, &state)?;
    Ok(updated)
}

pub fn evaluate_vertical_slice_readiness(
    workspace: &Workspace,
    args: EvaluateVerticalSliceReadinessArgs,
) -> anyhow::Result<VerticalSliceReadinessReport> {
    let state = load_vertical_slice_state(workspace);
    let run = state
        .runs
        .iter()
        .find(|run| run.id == args.run_id)
        .ok_or_else(|| anyhow::anyhow!("vertical slice run не найден: {}", args.run_id))?;
    Ok(evaluate_run_readiness(workspace, run))
}

pub fn vertical_slice_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Vertical Slice Orchestrator: рабочая папка не выбрана.".to_string();
    };
    let state = load_vertical_slice_state(workspace);
    let Some(run) = state
        .active_run_id
        .as_deref()
        .and_then(|id| state.runs.iter().find(|run| run.id == id))
        .or_else(|| state.runs.last())
    else {
        return "Vertical Slice Orchestrator: активный run ещё не создан.".to_string();
    };
    let report = evaluate_run_readiness(workspace, run);
    let next = report
        .next_phase
        .map(|phase| phase.label())
        .unwrap_or("нет");
    format!(
        "Vertical Slice Orchestrator: {} · {}% · статус {} · следующая фаза {} · инструменты {}.",
        run.title,
        report.progress_percent,
        run.status.label(),
        next,
        if report.recommended_tools.is_empty() {
            "нет".to_string()
        } else {
            report.recommended_tools.join(", ")
        }
    )
}

fn phase_template(now: u64) -> Vec<VerticalSlicePhase> {
    vec![
        phase(
            VerticalSlicePhaseKind::Preflight,
            "Unreal preflight",
            "Проверить .uproject, UE 5.8, toolchain и Unreal MCP.",
            VerticalSlicePhaseStatus::Ready,
            &[],
            now,
        ),
        phase(
            VerticalSlicePhaseKind::GameplayFoundation,
            "Gameplay foundation",
            "Создать и применить репрезентативный gameplay plan.",
            VerticalSlicePhaseStatus::Planned,
            &[VerticalSlicePhaseKind::Preflight],
            now,
        ),
        phase(
            VerticalSlicePhaseKind::VisualAssets,
            "3D и visual assets",
            "Создать, проверить и импортировать hero assets и visual target.",
            VerticalSlicePhaseStatus::Planned,
            &[VerticalSlicePhaseKind::Preflight],
            now,
        ),
        phase(
            VerticalSlicePhaseKind::LevelIntegration,
            "Интеграция уровня",
            "Свести gameplay и проверенные ассеты на репрезентативной карте.",
            VerticalSlicePhaseStatus::Planned,
            &[
                VerticalSlicePhaseKind::GameplayFoundation,
                VerticalSlicePhaseKind::VisualAssets,
            ],
            now,
        ),
        phase(
            VerticalSlicePhaseKind::Experience,
            "UI, feedback и audio",
            "Добавить HUD, feedback, VFX, SFX и пользовательский flow.",
            VerticalSlicePhaseStatus::Planned,
            &[VerticalSlicePhaseKind::LevelIntegration],
            now,
        ),
        phase(
            VerticalSlicePhaseKind::Playtest,
            "Playtest",
            "Пройти Automation/map smoke и сохранить screenshot/video/log artifacts.",
            VerticalSlicePhaseStatus::Planned,
            &[VerticalSlicePhaseKind::Experience],
            now,
        ),
        phase(
            VerticalSlicePhaseKind::ProductionGate,
            "Production gate",
            "Проверить Vertical Slice milestone и зафиксировать решение о переходе дальше.",
            VerticalSlicePhaseStatus::Planned,
            &[VerticalSlicePhaseKind::Playtest],
            now,
        ),
    ]
}

fn phase(
    kind: VerticalSlicePhaseKind,
    title: &str,
    description: &str,
    status: VerticalSlicePhaseStatus,
    depends_on: &[VerticalSlicePhaseKind],
    now: u64,
) -> VerticalSlicePhase {
    VerticalSlicePhase {
        kind,
        title: title.to_string(),
        description: description.to_string(),
        status,
        depends_on: depends_on.to_vec(),
        evidence: Vec::new(),
        artifacts: Vec::new(),
        notes: String::new(),
        updated_at: now,
    }
}

fn phase_dependencies_complete(run: &VerticalSliceRun, kind: VerticalSlicePhaseKind) -> bool {
    let Some(phase) = run.phases.iter().find(|phase| phase.kind == kind) else {
        return false;
    };
    phase.depends_on.iter().all(|dependency| {
        run.phases.iter().any(|phase| {
            phase.kind == *dependency && phase.status == VerticalSlicePhaseStatus::Completed
        })
    })
}

fn validate_phase_completion(
    workspace: &Workspace,
    run: &VerticalSliceRun,
    kind: VerticalSlicePhaseKind,
) -> anyhow::Result<()> {
    let phase = run
        .phases
        .iter()
        .find(|phase| phase.kind == kind)
        .ok_or_else(|| anyhow::anyhow!("фаза не найдена"))?;
    if phase.evidence.is_empty() && phase.artifacts.is_empty() {
        anyhow::bail!("для завершения фазы нужны evidence или существующий artifact");
    }
    match kind {
        VerticalSlicePhaseKind::Preflight => {
            let snapshot = unreal_snapshot(workspace);
            if snapshot.project.is_none() {
                anyhow::bail!("Unreal preflight: .uproject не найден");
            }
            if snapshot.selected_engine.is_none() {
                anyhow::bail!("Unreal preflight: установка движка не сопоставлена с проектом");
            }
            if registry_snapshot(workspace)?.servers.is_empty() {
                anyhow::bail!("Unreal preflight: MCP registry пуст");
            }
        }
        VerticalSlicePhaseKind::GameplayFoundation => {
            let ready = load_gameplay_state(workspace).plans.iter().any(|plan| {
                matches!(
                    plan.status,
                    GameplayPlanStatus::Applied | GameplayPlanStatus::Validated
                )
            });
            if !ready {
                anyhow::bail!("нет применённого gameplay plan");
            }
        }
        VerticalSlicePhaseKind::VisualAssets => {
            let validated_job = load_3d_jobs(workspace).iter().any(|job| {
                job.status == ThreeDJobStatus::Ready
                    && job
                        .validation
                        .as_ref()
                        .map(|report| report.import_ready)
                        .unwrap_or(false)
            });
            if !validated_job && phase.artifacts.is_empty() {
                anyhow::bail!("нет import-ready 3D job или существующего visual artifact");
            }
        }
        VerticalSlicePhaseKind::LevelIntegration | VerticalSlicePhaseKind::Experience => {
            if phase.artifacts.is_empty() {
                anyhow::bail!("для интеграционной фазы нужен существующий artifact");
            }
        }
        VerticalSlicePhaseKind::Playtest => {
            if !load_gameplay_state(workspace)
                .runs
                .iter()
                .any(|playtest| playtest.success)
            {
                anyhow::bail!("нет успешного gameplay playtest");
            }
        }
        VerticalSlicePhaseKind::ProductionGate => {
            let report = evaluate_production_gate(
                workspace,
                EvaluateProductionGateArgs {
                    plan_id: run.production_plan_id.clone(),
                    milestone: Some(ProductionMilestone::VerticalSlice),
                },
            )?;
            if !report.passed {
                anyhow::bail!(
                    "Vertical Slice gate не пройден: {}",
                    report.blockers.join("; ")
                );
            }
        }
    }
    Ok(())
}

fn unlock_ready_phases(run: &mut VerticalSliceRun, now: u64) {
    let completed = run
        .phases
        .iter()
        .filter(|phase| phase.status == VerticalSlicePhaseStatus::Completed)
        .map(|phase| phase.kind)
        .collect::<Vec<_>>();
    for phase in &mut run.phases {
        if phase.status == VerticalSlicePhaseStatus::Planned
            && phase
                .depends_on
                .iter()
                .all(|dependency| completed.contains(dependency))
        {
            phase.status = VerticalSlicePhaseStatus::Ready;
            phase.updated_at = now;
        }
    }
}

fn update_run_status(run: &mut VerticalSliceRun, now: u64) {
    run.status = if run
        .phases
        .iter()
        .all(|phase| phase.status == VerticalSlicePhaseStatus::Completed)
    {
        VerticalSliceRunStatus::Completed
    } else if run
        .phases
        .iter()
        .any(|phase| phase.status == VerticalSlicePhaseStatus::Blocked)
    {
        VerticalSliceRunStatus::Blocked
    } else {
        VerticalSliceRunStatus::Active
    };
    run.updated_at = now;
}

fn phase_event_detail(run: &VerticalSliceRun, kind: VerticalSlicePhaseKind) -> String {
    run.phases
        .iter()
        .find(|phase| phase.kind == kind)
        .map(|phase| {
            let detail = phase
                .evidence
                .last()
                .or_else(|| phase.artifacts.last())
                .cloned()
                .unwrap_or_else(|| phase.description.clone());
            format!("{}: {detail}", phase.title)
        })
        .unwrap_or_else(|| kind.label().to_string())
}

fn evaluate_run_readiness(
    workspace: &Workspace,
    run: &VerticalSliceRun,
) -> VerticalSliceReadinessReport {
    let completed_phases = run
        .phases
        .iter()
        .filter(|phase| phase.status == VerticalSlicePhaseStatus::Completed)
        .count();
    let total_phases = run.phases.len();
    let ready_phases = run
        .phases
        .iter()
        .filter(|phase| {
            matches!(
                phase.status,
                VerticalSlicePhaseStatus::Ready | VerticalSlicePhaseStatus::InProgress
            )
        })
        .map(|phase| phase.kind)
        .collect::<Vec<_>>();
    let next_phase = ready_phases.first().copied().or_else(|| {
        run.phases
            .iter()
            .find(|phase| phase.status == VerticalSlicePhaseStatus::Blocked)
            .map(|phase| phase.kind)
    });
    let mut blockers = run
        .phases
        .iter()
        .filter(|phase| phase.status == VerticalSlicePhaseStatus::Blocked)
        .map(|phase| format!("{}: {}", phase.title, phase.notes))
        .collect::<Vec<_>>();
    let production_exists = load_game_production_state(workspace)
        .plans
        .iter()
        .any(|plan| plan.id == run.production_plan_id);
    let unreal = unreal_snapshot(workspace);
    let playtest_ok = load_gameplay_state(workspace)
        .runs
        .iter()
        .any(|playtest| playtest.success);
    let production_gate = evaluate_production_gate(
        workspace,
        EvaluateProductionGateArgs {
            plan_id: run.production_plan_id.clone(),
            milestone: Some(ProductionMilestone::VerticalSlice),
        },
    )
    .ok();
    let gate_ok = production_gate
        .as_ref()
        .map(|report| report.passed)
        .unwrap_or(false);
    let all_phases = completed_phases == total_phases && total_phases > 0;
    let checks = vec![
        VerticalSliceReadinessCheck {
            id: "production-plan".to_string(),
            passed: production_exists,
            detail: if production_exists {
                "production plan найден".to_string()
            } else {
                "production plan отсутствует".to_string()
            },
        },
        VerticalSliceReadinessCheck {
            id: "unreal-project".to_string(),
            passed: unreal.project.is_some() && unreal.selected_engine.is_some(),
            detail: format!(
                "uproject: {}, engine: {}",
                unreal.project.is_some(),
                unreal.selected_engine.is_some()
            ),
        },
        VerticalSliceReadinessCheck {
            id: "successful-playtest".to_string(),
            passed: playtest_ok,
            detail: if playtest_ok {
                "успешный playtest найден".to_string()
            } else {
                "успешного playtest ещё нет".to_string()
            },
        },
        VerticalSliceReadinessCheck {
            id: "production-gate".to_string(),
            passed: gate_ok,
            detail: production_gate
                .map(|report| {
                    if report.passed {
                        "Vertical Slice gate пройден".to_string()
                    } else {
                        report.blockers.join("; ")
                    }
                })
                .unwrap_or_else(|| "production gate недоступен".to_string()),
        },
        VerticalSliceReadinessCheck {
            id: "orchestration-phases".to_string(),
            passed: all_phases,
            detail: format!("завершено {completed_phases} из {total_phases} фаз"),
        },
    ];
    blockers.extend(
        checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| check.detail.clone()),
    );
    blockers.sort();
    blockers.dedup();
    VerticalSliceReadinessReport {
        run_id: run.id.clone(),
        ready: checks.iter().all(|check| check.passed),
        completed_phases,
        total_phases,
        progress_percent: if total_phases == 0 {
            0
        } else {
            ((completed_phases * 100) / total_phases) as u8
        },
        next_phase,
        ready_phases: ready_phases.clone(),
        recommended_tools: ready_phases
            .iter()
            .flat_map(|phase| phase.recommended_tools().iter().copied())
            .fold(Vec::<String>::new(), |mut tools, tool| {
                if !tools.iter().any(|existing| existing == tool) {
                    tools.push(tool.to_string());
                }
                tools
            }),
        blockers,
        checks,
    }
}

fn slug(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let value = value
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if value.is_empty() {
        "game".to_string()
    } else {
        value
    }
}

fn schema_version() -> u32 {
    1
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_production::{create_game_production_plan, CreateGameProductionPlanArgs};

    fn production_plan(workspace: &Workspace, scope: GameScope) -> String {
        create_game_production_plan(
            workspace,
            CreateGameProductionPlanArgs {
                title: "Test Game".to_string(),
                brief: "A representative Unreal vertical slice".to_string(),
                genre: "Action".to_string(),
                target_platform: "Windows".to_string(),
                scope,
                source_task_ids: Vec::new(),
                roadmap_ids: Vec::new(),
                project_node_id: None,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn requires_vertical_slice_or_full_game_scope() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        production_plan(&workspace, GameScope::Prototype);

        let result = start_vertical_slice_run(
            &workspace,
            StartVerticalSliceRunArgs {
                production_plan_id: None,
                title: None,
            },
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scope prototype"));
    }

    #[test]
    fn creates_parallel_gameplay_and_asset_branches() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan_id = production_plan(&workspace, GameScope::VerticalSlice);

        let run = start_vertical_slice_run(
            &workspace,
            StartVerticalSliceRunArgs {
                production_plan_id: Some(plan_id),
                title: None,
            },
        )
        .unwrap();

        assert_eq!(run.phases.len(), 7);
        assert_eq!(run.phases[0].status, VerticalSlicePhaseStatus::Ready);
        assert_eq!(
            run.phases[1].depends_on,
            vec![VerticalSlicePhaseKind::Preflight]
        );
        assert_eq!(
            run.phases[2].depends_on,
            vec![VerticalSlicePhaseKind::Preflight]
        );
        assert!(run.phases[3]
            .depends_on
            .contains(&VerticalSlicePhaseKind::GameplayFoundation));
        assert!(run.phases[3]
            .depends_on
            .contains(&VerticalSlicePhaseKind::VisualAssets));
    }

    #[test]
    fn completed_preflight_unlocks_only_parallel_foundation_branches() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan_id = production_plan(&workspace, GameScope::VerticalSlice);
        let mut run = start_vertical_slice_run(
            &workspace,
            StartVerticalSliceRunArgs {
                production_plan_id: Some(plan_id),
                title: None,
            },
        )
        .unwrap();

        run.phases[0].status = VerticalSlicePhaseStatus::Completed;
        unlock_ready_phases(&mut run, unix_timestamp());
        let report = evaluate_run_readiness(&workspace, &run);

        assert_eq!(
            report.ready_phases,
            vec![
                VerticalSlicePhaseKind::GameplayFoundation,
                VerticalSlicePhaseKind::VisualAssets,
            ]
        );
        assert_eq!(run.phases[1].status, VerticalSlicePhaseStatus::Ready);
        assert_eq!(run.phases[2].status, VerticalSlicePhaseStatus::Ready);
        assert_eq!(run.phases[3].status, VerticalSlicePhaseStatus::Planned);
    }

    #[test]
    fn rejects_manual_ready_state_and_missing_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan_id = production_plan(&workspace, GameScope::VerticalSlice);
        let run = start_vertical_slice_run(
            &workspace,
            StartVerticalSliceRunArgs {
                production_plan_id: Some(plan_id),
                title: None,
            },
        )
        .unwrap();

        let manual_ready = advance_vertical_slice_phase(
            &workspace,
            AdvanceVerticalSlicePhaseArgs {
                run_id: run.id.clone(),
                phase: VerticalSlicePhaseKind::Preflight,
                status: VerticalSlicePhaseStatus::Ready,
                evidence: None,
                artifact: None,
                notes: None,
            },
        );
        assert!(manual_ready.is_err());

        let skipped_dependency = advance_vertical_slice_phase(
            &workspace,
            AdvanceVerticalSlicePhaseArgs {
                run_id: run.id,
                phase: VerticalSlicePhaseKind::LevelIntegration,
                status: VerticalSlicePhaseStatus::InProgress,
                evidence: Some("attempt".to_string()),
                artifact: None,
                notes: None,
            },
        );
        assert!(skipped_dependency.is_err());
    }

    #[test]
    fn snapshot_recommends_preflight_tools() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan_id = production_plan(&workspace, GameScope::FullGame);
        let run = start_vertical_slice_run(
            &workspace,
            StartVerticalSliceRunArgs {
                production_plan_id: Some(plan_id),
                title: None,
            },
        )
        .unwrap();

        let report = evaluate_vertical_slice_readiness(
            &workspace,
            EvaluateVerticalSliceReadinessArgs { run_id: run.id },
        )
        .unwrap();

        assert_eq!(report.next_phase, Some(VerticalSlicePhaseKind::Preflight));
        assert!(report
            .recommended_tools
            .contains(&"unreal_snapshot".to_string()));
        assert!(!report.ready);
    }
}
