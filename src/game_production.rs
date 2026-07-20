use crate::agent::types::ToolResult;
use crate::production_validation::load_production_report;
use crate::project_graph::load_project_graph_selection;
use crate::unreal_gameplay::load_gameplay_state;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub const GAME_PRODUCTION_STATE_PATH: &str = "assets/generated/leetcode/game-production/state.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameScope {
    Prototype,
    VerticalSlice,
    FullGame,
}

impl GameScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Prototype => "Прототип",
            Self::VerticalSlice => "Вертикальный срез",
            Self::FullGame => "Полная игра",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionMilestone {
    Prototype,
    VerticalSlice,
    Alpha,
    Beta,
    Release,
}

impl ProductionMilestone {
    pub fn id(self) -> &'static str {
        match self {
            Self::Prototype => "prototype",
            Self::VerticalSlice => "vertical_slice",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Release => "release",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Prototype => "Прототип",
            Self::VerticalSlice => "Вертикальный срез",
            Self::Alpha => "Alpha",
            Self::Beta => "Beta",
            Self::Release => "Release",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionWorkstream {
    CreativeDirection,
    Engineering,
    Gameplay,
    LevelDesign,
    ThreeDArt,
    TwoDArt,
    Animation,
    Audio,
    UiUx,
    Integration,
    Quality,
    Release,
}

impl ProductionWorkstream {
    pub fn label(self) -> &'static str {
        match self {
            Self::CreativeDirection => "Геймдизайн",
            Self::Engineering => "Инженерия",
            Self::Gameplay => "Gameplay",
            Self::LevelDesign => "Уровни",
            Self::ThreeDArt => "3D",
            Self::TwoDArt => "2D",
            Self::Animation => "Анимация",
            Self::Audio => "Аудио",
            Self::UiUx => "UI/UX",
            Self::Integration => "Интеграция",
            Self::Quality => "QA",
            Self::Release => "Релиз",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionItemStatus {
    Planned,
    Ready,
    InProgress,
    Blocked,
    Done,
}

impl ProductionItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planned => "запланировано",
            Self::Ready => "готово к работе",
            Self::InProgress => "в работе",
            Self::Blocked => "заблокировано",
            Self::Done => "выполнено",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionPlanStatus {
    Planned,
    Active,
    Blocked,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductionItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub workstream: ProductionWorkstream,
    pub milestone: ProductionMilestone,
    pub status: ProductionItemStatus,
    pub priority: u8,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub validation: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GameProductionPlan {
    pub id: String,
    pub title: String,
    pub brief: String,
    pub genre: String,
    pub target_platform: String,
    pub engine: String,
    pub scope: GameScope,
    pub status: ProductionPlanStatus,
    pub current_milestone: ProductionMilestone,
    #[serde(default)]
    pub source_task_ids: Vec<String>,
    #[serde(default)]
    pub roadmap_ids: Vec<String>,
    #[serde(default)]
    pub project_node_id: Option<String>,
    #[serde(default)]
    pub items: Vec<ProductionItem>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GameProductionState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub active_plan_id: Option<String>,
    #[serde(default)]
    pub plans: Vec<GameProductionPlan>,
}

impl Default for GameProductionState {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            active_plan_id: None,
            plans: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateGameProductionPlanArgs {
    pub title: String,
    pub brief: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub target_platform: String,
    pub scope: GameScope,
    #[serde(default)]
    pub source_task_ids: Vec<String>,
    #[serde(default)]
    pub roadmap_ids: Vec<String>,
    #[serde(default)]
    pub project_node_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateProductionItemArgs {
    pub plan_id: String,
    pub item_id: String,
    pub status: ProductionItemStatus,
    #[serde(default)]
    pub artifact: Option<String>,
    #[serde(default)]
    pub validation: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EvaluateProductionGateArgs {
    pub plan_id: String,
    #[serde(default)]
    pub milestone: Option<ProductionMilestone>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProductionGateCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProductionGateReport {
    pub plan_id: String,
    pub milestone: ProductionMilestone,
    pub passed: bool,
    pub completed_items: usize,
    pub total_items: usize,
    pub blockers: Vec<String>,
    pub checks: Vec<ProductionGateCheck>,
}

pub fn load_game_production_state(workspace: &Workspace) -> GameProductionState {
    workspace
        .read_text(GAME_PRODUCTION_STATE_PATH, 4_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_game_production_state(
    workspace: &Workspace,
    state: &GameProductionState,
) -> anyhow::Result<()> {
    workspace.write_text(
        GAME_PRODUCTION_STATE_PATH,
        &serde_json::to_string_pretty(state)?,
    )
}

pub fn game_production_snapshot(workspace: &Workspace) -> ToolResult {
    let state = load_game_production_state(workspace);
    let gates = state
        .plans
        .iter()
        .map(|plan| evaluate_plan_gate(workspace, plan, plan.current_milestone))
        .collect::<Vec<_>>();
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({ "state": state, "gates": gates }))
            .unwrap_or_else(|_| "game production snapshot".to_string()),
    )
}

pub fn create_game_production_plan(
    workspace: &Workspace,
    args: CreateGameProductionPlanArgs,
) -> anyhow::Result<GameProductionPlan> {
    let title = args.title.trim();
    let brief = args.brief.trim();
    if title.is_empty() || brief.is_empty() {
        anyhow::bail!("title и brief обязательны");
    }
    let now = unix_timestamp();
    let id = format!("production-{}-{}", now, slug(title));
    let items = production_template(&id, args.scope, now);
    let plan = GameProductionPlan {
        id: id.clone(),
        title: title.to_string(),
        brief: brief.to_string(),
        genre: empty_as(&args.genre, "не указан"),
        target_platform: empty_as(&args.target_platform, "Windows PC"),
        engine: "Unreal Engine 5.8".to_string(),
        scope: args.scope,
        status: ProductionPlanStatus::Active,
        current_milestone: ProductionMilestone::Prototype,
        source_task_ids: clean_ids(args.source_task_ids),
        roadmap_ids: clean_ids(args.roadmap_ids),
        project_node_id: args
            .project_node_id
            .filter(|value| !value.trim().is_empty())
            .or_else(|| load_project_graph_selection(workspace)),
        items,
        created_at: now,
        updated_at: now,
    };
    let mut state = load_game_production_state(workspace);
    state.plans.push(plan.clone());
    state.active_plan_id = Some(id);
    save_game_production_state(workspace, &state)?;
    Ok(plan)
}

pub fn update_production_item(
    workspace: &Workspace,
    args: UpdateProductionItemArgs,
) -> anyhow::Result<GameProductionPlan> {
    let mut state = load_game_production_state(workspace);
    let plan = state
        .plans
        .iter_mut()
        .find(|plan| plan.id == args.plan_id)
        .ok_or_else(|| anyhow::anyhow!("production plan не найден: {}", args.plan_id))?;
    let now = unix_timestamp();
    let item = plan
        .items
        .iter_mut()
        .find(|item| item.id == args.item_id)
        .ok_or_else(|| anyhow::anyhow!("production item не найден: {}", args.item_id))?;
    item.status = args.status;
    if let Some(artifact) = args.artifact.map(|value| value.trim().to_string()) {
        if !artifact.is_empty() && !item.artifacts.contains(&artifact) {
            workspace.resolve_existing(&artifact)?;
            item.artifacts.push(artifact);
        }
    }
    if let Some(validation) = args.validation {
        item.validation = validation.trim().to_string();
    }
    if item.status == ProductionItemStatus::Done
        && item.validation.is_empty()
        && item.artifacts.is_empty()
    {
        anyhow::bail!("production item нельзя завершить без validation или существующего artifact");
    }
    item.updated_at = now;
    unlock_ready_items(plan);
    update_plan_progress(workspace, plan, now);
    let updated = plan.clone();
    save_game_production_state(workspace, &state)?;
    Ok(updated)
}

pub fn evaluate_production_gate(
    workspace: &Workspace,
    args: EvaluateProductionGateArgs,
) -> anyhow::Result<ProductionGateReport> {
    let state = load_game_production_state(workspace);
    let plan = state
        .plans
        .iter()
        .find(|plan| plan.id == args.plan_id)
        .ok_or_else(|| anyhow::anyhow!("production plan не найден: {}", args.plan_id))?;
    Ok(evaluate_plan_gate(
        workspace,
        plan,
        args.milestone.unwrap_or(plan.current_milestone),
    ))
}

pub fn game_production_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Game Production Director: рабочая папка не выбрана.".to_string();
    };
    let state = load_game_production_state(workspace);
    let Some(plan) = state
        .active_plan_id
        .as_deref()
        .and_then(|id| state.plans.iter().find(|plan| plan.id == id))
        .or_else(|| state.plans.last())
    else {
        return "Game Production Director: production plan ещё не создан.".to_string();
    };
    let ready = plan
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.status,
                ProductionItemStatus::Ready | ProductionItemStatus::InProgress
            )
        })
        .take(6)
        .map(|item| format!("{} [{}]", item.title, item.workstream.label()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Game Production Director: {} · {} · {} · текущий этап {}. Готово к работе: {}.",
        plan.title,
        plan.scope.label(),
        plan.engine,
        plan.current_milestone.label(),
        empty_as(&ready, "нет")
    )
}

fn evaluate_plan_gate(
    workspace: &Workspace,
    plan: &GameProductionPlan,
    milestone: ProductionMilestone,
) -> ProductionGateReport {
    let items = plan
        .items
        .iter()
        .filter(|item| item.milestone == milestone)
        .collect::<Vec<_>>();
    let completed_items = items
        .iter()
        .filter(|item| item.status == ProductionItemStatus::Done)
        .count();
    let mut blockers = items
        .iter()
        .filter(|item| item.status != ProductionItemStatus::Done)
        .map(|item| format!("{}: {}", item.title, item.status.label()))
        .collect::<Vec<_>>();
    let mut checks = vec![ProductionGateCheck {
        id: "milestone-items".to_string(),
        passed: completed_items == items.len() && !items.is_empty(),
        detail: format!("выполнено {completed_items} из {}", items.len()),
    }];
    if milestone >= ProductionMilestone::VerticalSlice {
        let playtest_ok = load_gameplay_state(workspace)
            .runs
            .iter()
            .rev()
            .any(|run| run.success);
        checks.push(ProductionGateCheck {
            id: "successful-playtest".to_string(),
            passed: playtest_ok,
            detail: if playtest_ok {
                "найден успешный gameplay playtest".to_string()
            } else {
                "нет успешного gameplay playtest".to_string()
            },
        });
        if !playtest_ok {
            blockers.push("нужен успешный gameplay playtest".to_string());
        }
    }
    if milestone == ProductionMilestone::Release {
        let production_ready = load_production_report(workspace)
            .map(|report| report.ready)
            .unwrap_or(false);
        checks.push(ProductionGateCheck {
            id: "production-validation".to_string(),
            passed: production_ready,
            detail: if production_ready {
                "production validation готов".to_string()
            } else {
                "production validation не готов".to_string()
            },
        });
        if !production_ready {
            blockers.push("нужен зелёный production validation report".to_string());
        }
    }
    ProductionGateReport {
        plan_id: plan.id.clone(),
        milestone,
        passed: checks.iter().all(|check| check.passed),
        completed_items,
        total_items: items.len(),
        blockers,
        checks,
    }
}

fn production_template(plan_id: &str, scope: GameScope, now: u64) -> Vec<ProductionItem> {
    let mut items = Vec::new();
    let vision = add_item(
        &mut items,
        plan_id,
        "vision",
        "Игровое видение",
        "Зафиксировать core fantasy, pillars, аудиторию и критерии успеха.",
        ProductionWorkstream::CreativeDirection,
        ProductionMilestone::Prototype,
        1,
        &[],
        now,
    );
    let foundation = add_item(
        &mut items,
        plan_id,
        "technical-foundation",
        "Технический фундамент UE 5.8",
        "Подготовить проект, модули, плагины, input, базовые data contracts и проверки.",
        ProductionWorkstream::Engineering,
        ProductionMilestone::Prototype,
        1,
        &[&vision],
        now,
    );
    let core_loop = add_item(
        &mut items,
        plan_id,
        "core-loop",
        "Играбельный core loop",
        "Собрать минимальный игровой цикл и измеримую fail/success петлю.",
        ProductionWorkstream::Gameplay,
        ProductionMilestone::Prototype,
        1,
        &[&foundation],
        now,
    );
    let prototype_map = add_item(
        &mut items,
        plan_id,
        "prototype-map",
        "Прототип уровня",
        "Создать blockout-карту для проверки core loop и масштаба.",
        ProductionWorkstream::LevelDesign,
        ProductionMilestone::Prototype,
        2,
        &[&vision, &foundation],
        now,
    );
    let prototype_test = add_item(
        &mut items,
        plan_id,
        "prototype-playtest",
        "Проверка прототипа",
        "Запустить smoke/playtest, собрать проблемы и решение о продолжении.",
        ProductionWorkstream::Quality,
        ProductionMilestone::Prototype,
        1,
        &[&core_loop, &prototype_map],
        now,
    );

    if scope != GameScope::Prototype {
        let visual = add_item(
            &mut items,
            plan_id,
            "visual-target",
            "Визуальный target",
            "Утвердить art direction, lighting, material и performance budget.",
            ProductionWorkstream::TwoDArt,
            ProductionMilestone::VerticalSlice,
            1,
            &[&prototype_test],
            now,
        );
        let assets = add_item(
            &mut items,
            plan_id,
            "hero-assets",
            "Hero 3D-ассеты",
            "Создать, валидировать и импортировать ключевые mesh/material/rig assets.",
            ProductionWorkstream::ThreeDArt,
            ProductionMilestone::VerticalSlice,
            1,
            &[&visual],
            now,
        );
        let animation = add_item(
            &mut items,
            plan_id,
            "animation-pass",
            "Анимационный проход",
            "Подключить skeleton, animation blueprint и ключевые игровые состояния.",
            ProductionWorkstream::Animation,
            ProductionMilestone::VerticalSlice,
            2,
            &[&assets],
            now,
        );
        let level = add_item(
            &mut items,
            plan_id,
            "slice-level",
            "Уровень вертикального среза",
            "Довести одну репрезентативную карту до игрового и визуального target.",
            ProductionWorkstream::LevelDesign,
            ProductionMilestone::VerticalSlice,
            1,
            &[&prototype_test, &visual],
            now,
        );
        let ui = add_item(
            &mut items,
            plan_id,
            "ui-flow",
            "Игровой UI/UX",
            "Собрать HUD, feedback, меню и доступный пользовательский flow.",
            ProductionWorkstream::UiUx,
            ProductionMilestone::VerticalSlice,
            2,
            &[&prototype_test],
            now,
        );
        let audio = add_item(
            &mut items,
            plan_id,
            "audio-pass",
            "Аудиопроход",
            "Добавить UI/gameplay SFX, ambience и критичный feedback.",
            ProductionWorkstream::Audio,
            ProductionMilestone::VerticalSlice,
            2,
            &[&prototype_test],
            now,
        );
        let integration = add_item(
            &mut items,
            plan_id,
            "slice-integration",
            "Интеграция вертикального среза",
            "Свести gameplay, level, assets, animation, UI и audio в один build.",
            ProductionWorkstream::Integration,
            ProductionMilestone::VerticalSlice,
            1,
            &[&assets, &animation, &level, &ui, &audio],
            now,
        );
        let slice_test = add_item(
            &mut items,
            plan_id,
            "slice-playtest",
            "Playtest вертикального среза",
            "Пройти automation/map smoke и зафиксировать качество среза.",
            ProductionWorkstream::Quality,
            ProductionMilestone::VerticalSlice,
            1,
            &[&integration],
            now,
        );

        if scope == GameScope::FullGame {
            let alpha_content = add_item(
                &mut items,
                plan_id,
                "alpha-content",
                "Alpha: контент и системы",
                "Сделать feature/content complete согласно утверждённому scope.",
                ProductionWorkstream::Integration,
                ProductionMilestone::Alpha,
                1,
                &[&slice_test],
                now,
            );
            let alpha_test = add_item(
                &mut items,
                plan_id,
                "alpha-quality",
                "Alpha: системное тестирование",
                "Закрыть критические gameplay, save/load и progression дефекты.",
                ProductionWorkstream::Quality,
                ProductionMilestone::Alpha,
                1,
                &[&alpha_content],
                now,
            );
            let beta_polish = add_item(
                &mut items,
                plan_id,
                "beta-polish",
                "Beta: polish и оптимизация",
                "Стабилизировать UX, performance, memory и platform behavior.",
                ProductionWorkstream::Engineering,
                ProductionMilestone::Beta,
                1,
                &[&alpha_test],
                now,
            );
            let beta_test = add_item(
                &mut items,
                plan_id,
                "beta-quality",
                "Beta: regression",
                "Пройти полный regression и подтвердить release candidate.",
                ProductionWorkstream::Quality,
                ProductionMilestone::Beta,
                1,
                &[&beta_polish],
                now,
            );
            let release_package = add_item(
                &mut items,
                plan_id,
                "release-package",
                "Release package",
                "Собрать, подписать и проверить publishable build и manifests.",
                ProductionWorkstream::Release,
                ProductionMilestone::Release,
                1,
                &[&beta_test],
                now,
            );
            add_item(
                &mut items,
                plan_id,
                "release-validation",
                "Production validation",
                "Закрыть production report, installer/updater и release checklist.",
                ProductionWorkstream::Release,
                ProductionMilestone::Release,
                1,
                &[&release_package],
                now,
            );
        }
    }
    items
}

#[allow(clippy::too_many_arguments)]
fn add_item(
    items: &mut Vec<ProductionItem>,
    plan_id: &str,
    suffix: &str,
    title: &str,
    description: &str,
    workstream: ProductionWorkstream,
    milestone: ProductionMilestone,
    priority: u8,
    depends_on: &[&str],
    now: u64,
) -> String {
    let id = format!("{plan_id}:{suffix}");
    items.push(ProductionItem {
        id: id.clone(),
        title: title.to_string(),
        description: description.to_string(),
        workstream,
        milestone,
        status: if depends_on.is_empty() {
            ProductionItemStatus::Ready
        } else {
            ProductionItemStatus::Planned
        },
        priority,
        depends_on: depends_on.iter().map(|id| (*id).to_string()).collect(),
        artifacts: Vec::new(),
        validation: String::new(),
        updated_at: now,
    });
    id
}

fn unlock_ready_items(plan: &mut GameProductionPlan) {
    let done = plan
        .items
        .iter()
        .filter(|item| item.status == ProductionItemStatus::Done)
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    for item in &mut plan.items {
        if item.status == ProductionItemStatus::Planned
            && item.depends_on.iter().all(|id| done.contains(id))
        {
            item.status = ProductionItemStatus::Ready;
        }
    }
}

fn update_plan_progress(workspace: &Workspace, plan: &mut GameProductionPlan, now: u64) {
    plan.status = if plan
        .items
        .iter()
        .any(|item| item.status == ProductionItemStatus::Blocked)
    {
        ProductionPlanStatus::Blocked
    } else {
        ProductionPlanStatus::Active
    };

    let current_milestone = plan.current_milestone;
    let current_items = plan
        .items
        .iter()
        .filter(|item| item.milestone == current_milestone)
        .collect::<Vec<_>>();
    let milestone_items_done = !current_items.is_empty()
        && current_items
            .iter()
            .all(|item| item.status == ProductionItemStatus::Done);
    if milestone_items_done {
        let gate = evaluate_plan_gate(workspace, plan, current_milestone);
        if gate.passed {
            if let Some(next) = plan
                .items
                .iter()
                .filter(|item| item.status != ProductionItemStatus::Done)
                .map(|item| item.milestone)
                .min()
            {
                plan.current_milestone = next;
                if plan.status != ProductionPlanStatus::Blocked {
                    plan.status = ProductionPlanStatus::Active;
                }
            } else {
                plan.status = ProductionPlanStatus::Completed;
            }
        } else {
            plan.status = ProductionPlanStatus::Blocked;
        }
    }
    plan.updated_at = now;
}

fn clean_ids(ids: Vec<String>) -> Vec<String> {
    let mut cleaned = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if !id.is_empty() && !cleaned.contains(&id) {
            cleaned.push(id);
        }
    }
    cleaned
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    empty_as(&compact, "game")
}

fn empty_as(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
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

    fn create_plan(workspace: &Workspace, scope: GameScope) -> GameProductionPlan {
        create_game_production_plan(
            workspace,
            CreateGameProductionPlanArgs {
                title: "Test Game".to_string(),
                brief: "A focused Unreal game".to_string(),
                genre: "Action".to_string(),
                target_platform: "Windows".to_string(),
                scope,
                source_task_ids: vec!["task-1".to_string()],
                roadmap_ids: vec!["stage-43".to_string()],
                project_node_id: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn creates_dependency_driven_vertical_slice() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = create_plan(&workspace, GameScope::VerticalSlice);

        assert_eq!(plan.engine, "Unreal Engine 5.8");
        assert!(plan.items.len() >= 12);
        assert_eq!(plan.items[0].status, ProductionItemStatus::Ready);
        assert!(plan
            .items
            .iter()
            .any(|item| item.workstream == ProductionWorkstream::ThreeDArt));
    }

    #[test]
    fn completing_dependency_unlocks_next_item() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = create_plan(&workspace, GameScope::Prototype);
        let vision = plan.items[0].id.clone();
        let foundation = plan.items[1].id.clone();

        let updated = update_production_item(
            &workspace,
            UpdateProductionItemArgs {
                plan_id: plan.id,
                item_id: vision,
                status: ProductionItemStatus::Done,
                artifact: None,
                validation: Some("vision reviewed".to_string()),
            },
        )
        .unwrap();

        assert_eq!(
            updated
                .items
                .iter()
                .find(|item| item.id == foundation)
                .unwrap()
                .status,
            ProductionItemStatus::Ready
        );
    }

    #[test]
    fn prototype_gate_reports_incomplete_items() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = create_plan(&workspace, GameScope::Prototype);

        let gate = evaluate_production_gate(
            &workspace,
            EvaluateProductionGateArgs {
                plan_id: plan.id,
                milestone: Some(ProductionMilestone::Prototype),
            },
        )
        .unwrap();

        assert!(!gate.passed);
        assert_eq!(gate.total_items, 5);
        assert!(!gate.blockers.is_empty());
    }

    #[test]
    fn cannot_complete_item_without_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let plan = create_plan(&workspace, GameScope::Prototype);

        let result = update_production_item(
            &workspace,
            UpdateProductionItemArgs {
                plan_id: plan.id,
                item_id: plan.items[0].id.clone(),
                status: ProductionItemStatus::Done,
                artifact: None,
                validation: None,
            },
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("validation"));
    }

    #[test]
    fn vertical_slice_does_not_finish_without_successful_playtest_gate() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let mut plan = create_plan(&workspace, GameScope::VerticalSlice);

        loop {
            let Some(item_id) = plan
                .items
                .iter()
                .find(|item| item.status == ProductionItemStatus::Ready)
                .map(|item| item.id.clone())
            else {
                break;
            };
            plan = update_production_item(
                &workspace,
                UpdateProductionItemArgs {
                    plan_id: plan.id.clone(),
                    item_id,
                    status: ProductionItemStatus::Done,
                    artifact: None,
                    validation: Some("fixture validation".to_string()),
                },
            )
            .unwrap();
        }

        assert!(plan
            .items
            .iter()
            .all(|item| item.status == ProductionItemStatus::Done));
        assert_eq!(plan.current_milestone, ProductionMilestone::VerticalSlice);
        assert_eq!(plan.status, ProductionPlanStatus::Blocked);
    }
}
