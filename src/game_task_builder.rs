use crate::agent::types::{ToolAction, ToolResult};
use crate::config::append_journal;
use crate::project_graph::{
    load_project_graph, project_graph_fingerprint, refresh_project_graph, save_project_graph,
    ProjectGraphEdge, ProjectGraphEdgeKind, ProjectGraphNode, ProjectGraphNodeKind,
    ProjectGraphState, PROJECT_GRAPH_PATH,
};
use crate::project_semantics::{
    ensure_semantic_index, rank_semantic_candidates, semantic_catalog_map, SemanticCandidateScore,
};
use crate::unreal::unreal_snapshot;
use crate::unreal_intelligence::{
    scan_unreal_project, UnrealProjectInput, GENERATED_ASSET_REGISTRY_PATH,
};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const GAME_TASK_BUILDER_STATE_PATH: &str =
    "assets/generated/leetcode/game-task-builder/state.json";
pub const GAME_TASK_DEEP_SCAN_SCRIPT_PATH: &str =
    "assets/generated/leetcode/unreal/export_asset_registry.py";

const MAX_STATE_BYTES: usize = 4_000_000;
const MAX_REPORTED_PROJECT_CHANGES: usize = 80;
const MAX_RECENT_TASK_TARGETS: usize = 64;
const MAX_RECENT_TARGET_SUGGESTIONS: usize = 6;
const CATALOG_SCHEMA_VERSION: u32 = 1;
const STATE_SCHEMA_VERSION: u32 = 1;
const DEEP_SCAN_SCRIPT: &str = include_str!("../scripts/unreal/export_asset_registry.py");
const OPERATION_ACTIONS: [(&str, &str); 10] = [
    ("create", "Создать"),
    ("modify", "Изменить"),
    ("repair", "Исправить"),
    ("extend", "Расширить"),
    ("configure", "Настроить"),
    ("integrate", "Интегрировать"),
    ("connect", "Связать с проектом"),
    ("optimize", "Оптимизировать"),
    ("validate", "Проверить"),
    ("document", "Задокументировать"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameTaskDomain {
    Gameplay,
    CharactersAnimation,
    WorldLevelDesign,
    VisualAssetsVfx,
    NarrativeQuests,
    UiUxAccessibility,
    AudioMusicVoice,
    EngineeringQualityRelease,
}

impl GameTaskDomain {
    pub const ALL: [Self; 8] = [
        Self::Gameplay,
        Self::CharactersAnimation,
        Self::WorldLevelDesign,
        Self::VisualAssetsVfx,
        Self::NarrativeQuests,
        Self::UiUxAccessibility,
        Self::AudioMusicVoice,
        Self::EngineeringQualityRelease,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Gameplay => "gameplay",
            Self::CharactersAnimation => "characters_animation",
            Self::WorldLevelDesign => "world_level_design",
            Self::VisualAssetsVfx => "visual_assets_vfx",
            Self::NarrativeQuests => "narrative_quests",
            Self::UiUxAccessibility => "ui_ux_accessibility",
            Self::AudioMusicVoice => "audio_music_voice",
            Self::EngineeringQualityRelease => "engineering_quality_release",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gameplay => "Геймплей и игровые системы",
            Self::CharactersAnimation => "Персонажи, AI и анимация",
            Self::WorldLevelDesign => "Мир и дизайн уровней",
            Self::VisualAssetsVfx => "Графика, ассеты и VFX",
            Self::NarrativeQuests => "Сюжет, нарратив и задания",
            Self::UiUxAccessibility => "Интерфейс, UX и доступность",
            Self::AudioMusicVoice => "Звук, музыка и озвучка",
            Self::EngineeringQualityRelease => "Инженерия, качество и выпуск",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|domain| domain.id() == value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionCapability {
    BuiltIn,
    Mcp,
    ExternalProvider,
    GuidedManual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetContractKind {
    Project,
    Gameplay,
    Character,
    CharacterAnimation,
    Ai,
    Map,
    StaticMesh,
    Material,
    Niagara,
    Cinematic,
    Narrative,
    Ui,
    Audio,
    Engineering,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetContract {
    pub kind: TargetContractKind,
    pub allowed_node_kinds: Vec<ProjectGraphNodeKind>,
    pub requires_exact_target: bool,
    pub requires_skeleton: bool,
    pub allow_group: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskOperation {
    pub id: String,
    pub action_id: String,
    pub label: String,
    pub direction_id: String,
    pub domain_id: String,
    pub target: TargetContract,
    pub expected_artifacts: Vec<String>,
    pub recommended_tools: Vec<String>,
    pub validation: Vec<String>,
    pub risk: String,
    pub capability: ExecutionCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDirection {
    pub id: String,
    pub label: String,
    pub operations: Vec<TaskOperation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDomainDefinition {
    pub id: String,
    pub label: String,
    pub directions: Vec<TaskDirection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameTaskCatalog {
    pub schema_version: u32,
    pub domains: Vec<TaskDomainDefinition>,
    pub custom_option_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMapReadinessStatus {
    Uninitialized,
    Scanning,
    Ready,
    Degraded,
    Stale,
    Failed,
}

impl ProjectMapReadinessStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Uninitialized => "не инициализирована",
            Self::Scanning => "сканирование",
            Self::Ready => "готова",
            Self::Degraded => "неполная",
            Self::Stale => "есть изменения",
            Self::Failed => "ошибка",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMapCoverageArea {
    pub id: String,
    pub label: String,
    pub score: u8,
    pub max_score: u8,
    pub ready: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectMapChanges {
    pub baseline_available: bool,
    pub added_count: usize,
    pub modified_count: usize,
    pub removed_count: usize,
    pub added_paths: Vec<String>,
    pub modified_paths: Vec<String>,
    pub removed_paths: Vec<String>,
}

impl ProjectMapChanges {
    pub fn total(&self) -> usize {
        self.added_count + self.modified_count + self.removed_count
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMapHealth {
    pub coverage_percent: u8,
    pub node_count: usize,
    pub edge_count: usize,
    pub semantic_edge_count: usize,
    pub unresolved_nodes: usize,
    pub external_dependency_nodes: usize,
    pub ambiguous_labels: usize,
    pub dependency_cycles: usize,
    pub stale_nodes: usize,
    pub linked_nodes: usize,
    pub code_nodes: usize,
    pub unreal_asset_nodes: usize,
    pub planning_nodes: usize,
    pub asset_registry_integrated: bool,
    pub coverage_areas: Vec<ProjectMapCoverageArea>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMapReadinessReport {
    pub status: ProjectMapReadinessStatus,
    pub graph_fingerprint: String,
    pub deep_scan_completed: bool,
    pub unreal_project: bool,
    pub engine_available: bool,
    pub mcp_server_count: usize,
    pub registry_path: Option<String>,
    pub health: ProjectMapHealth,
    #[serde(default)]
    pub changes: ProjectMapChanges,
    pub remediation: Vec<RemediationOption>,
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskFeasibility {
    Ready,
    NeedsSetup,
    Ambiguous,
    StaleContext,
    ExternalToolRequired,
}

impl TaskFeasibility {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "готово",
            Self::NeedsSetup => "требуется подготовка",
            Self::Ambiguous => "нужно выбрать цель",
            Self::StaleContext => "карта устарела",
            Self::ExternalToolRequired => "нужен внешний инструмент",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskTargetBinding {
    pub node_id: String,
    pub label: String,
    pub node_kind: ProjectGraphNodeKind,
    pub object_path: Option<String>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetResolutionReport {
    pub operation_id: String,
    pub feasibility: TaskFeasibility,
    #[serde(default)]
    pub recommended_candidates: Vec<SemanticTargetSuggestion>,
    #[serde(default)]
    pub recent_candidates: Vec<RecentTargetSuggestion>,
    #[serde(default)]
    pub related_candidates: Vec<SemanticTargetSuggestion>,
    pub candidates: Vec<TaskTargetBinding>,
    pub excluded: Vec<TaskTargetBinding>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticTargetSuggestion {
    pub target: TaskTargetBinding,
    pub semantic_score: f32,
    #[serde(default)]
    pub tag_labels: Vec<String>,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub relation_labels: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentTargetSuggestion {
    pub target: TaskTargetBinding,
    pub operation_label: String,
    pub context_label: String,
    pub last_completed_at: u64,
    pub completed_task_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemediationOption {
    pub id: String,
    pub title: String,
    pub description: String,
    pub expected_artifacts: Vec<String>,
    pub tools: Vec<String>,
    pub complexity: String,
    pub estimated_time: String,
    pub risk: String,
    pub requires_approval: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrerequisiteIssue {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub affected_node_ids: Vec<String>,
    pub remediation: Vec<RemediationOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrerequisiteReport {
    pub operation_id: String,
    pub feasibility: TaskFeasibility,
    pub issues: Vec<PrerequisiteIssue>,
    pub target_bindings: Vec<TaskTargetBinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalSuggestion {
    pub id: String,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskProposal {
    pub understood_task: String,
    pub exact_targets: Vec<TaskTargetBinding>,
    pub excluded_targets: Vec<TaskTargetBinding>,
    pub affected_dependencies: Vec<TaskTargetBinding>,
    pub steps: Vec<String>,
    pub preparation: Vec<String>,
    pub expected_artifacts: Vec<String>,
    pub validation: Vec<String>,
    pub risks: Vec<String>,
    pub improvements: Vec<ProposalSuggestion>,
    pub efficiency: Vec<ProposalSuggestion>,
    pub feasibility: TaskFeasibility,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameTaskBuilderSession {
    pub id: String,
    pub domain_id: String,
    pub direction_id: String,
    pub operation_id: String,
    #[serde(default)]
    pub custom_request: String,
    #[serde(default)]
    pub target_node_ids: Vec<String>,
    #[serde(default)]
    pub remediation_ids: Vec<String>,
    pub proposal: Option<TaskProposal>,
    pub graph_fingerprint: String,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskManifest {
    pub id: String,
    pub session_id: String,
    pub operation_id: String,
    pub understood_task: String,
    pub target_node_ids: Vec<String>,
    pub allowed_node_ids: Vec<String>,
    pub object_paths: Vec<String>,
    pub selected_improvement_ids: Vec<String>,
    pub selected_efficiency_ids: Vec<String>,
    pub graph_fingerprint: String,
    pub confirmed_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentTaskTarget {
    pub target: TaskTargetBinding,
    pub domain_id: String,
    pub direction_id: String,
    pub operation_id: String,
    pub operation_label: String,
    pub last_completed_at: u64,
    pub completed_task_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomCatalogOption {
    pub id: String,
    pub parent_id: String,
    pub label: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectRelationProposal {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub kind: ProjectGraphEdgeKind,
    pub label: String,
    pub reason: String,
    pub status: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeepScanRecord {
    pub status: ProjectMapReadinessStatus,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub duration_ms: Option<u64>,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameTaskBuilderState {
    #[serde(default = "state_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub sessions: Vec<GameTaskBuilderSession>,
    #[serde(default)]
    pub manifests: Vec<TaskManifest>,
    #[serde(default)]
    pub recent_targets: Vec<RecentTaskTarget>,
    #[serde(default)]
    pub custom_options: Vec<CustomCatalogOption>,
    #[serde(default)]
    pub relation_proposals: Vec<ProjectRelationProposal>,
    pub active_session_id: Option<String>,
    pub active_manifest_id: Option<String>,
    pub last_scan: Option<DeepScanRecord>,
}

impl Default for GameTaskBuilderState {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            sessions: Vec::new(),
            manifests: Vec::new(),
            recent_targets: Vec::new(),
            custom_options: Vec::new(),
            relation_proposals: Vec::new(),
            active_session_id: None,
            active_manifest_id: None,
            last_scan: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectMapReadinessArgs {
    #[serde(default)]
    pub refresh_if_stale: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RefreshProjectMapDeepArgs {
    #[serde(default = "default_true")]
    pub run_unreal_scan: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GameTaskCatalogSnapshotArgs {
    pub domain_id: Option<String>,
    pub direction_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResolveGameTaskTargetsArgs {
    pub operation_id: String,
    pub query: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EvaluateGameTaskPrerequisitesArgs {
    pub operation_id: String,
    #[serde(default)]
    pub target_node_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PrepareGameTaskProposalArgs {
    pub operation_id: String,
    #[serde(default)]
    pub target_node_ids: Vec<String>,
    #[serde(default)]
    pub remediation_ids: Vec<String>,
    pub custom_request: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProposeProjectRelationArgs {
    pub from_node_id: String,
    pub to_node_id: String,
    pub kind: String,
    pub reason: String,
}

static GAME_TASK_CATALOG: OnceLock<GameTaskCatalog> = OnceLock::new();

pub fn game_task_catalog_ref() -> &'static GameTaskCatalog {
    GAME_TASK_CATALOG.get_or_init(build_game_task_catalog)
}

pub fn game_task_catalog() -> GameTaskCatalog {
    game_task_catalog_ref().clone()
}

fn build_game_task_catalog() -> GameTaskCatalog {
    GameTaskCatalog {
        schema_version: CATALOG_SCHEMA_VERSION,
        domains: GameTaskDomain::ALL
            .into_iter()
            .map(build_domain_definition)
            .collect(),
        custom_option_label: "Свой вариант".to_string(),
    }
}

pub fn game_task_catalog_snapshot(args: &GameTaskCatalogSnapshotArgs) -> ToolResult {
    let mut catalog = game_task_catalog();
    if let Some(domain_id) = args
        .domain_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if GameTaskDomain::from_id(domain_id).is_none() {
            return ToolResult::error(format!("неизвестная сфера каталога: {domain_id}"));
        }
        catalog.domains.retain(|domain| domain.id == domain_id);
    }
    if let Some(direction_id) = args
        .direction_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        for domain in &mut catalog.domains {
            domain
                .directions
                .retain(|direction| direction.id == direction_id);
        }
        catalog
            .domains
            .retain(|domain| !domain.directions.is_empty());
    }
    ToolResult::ok(
        serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| "game task catalog".to_string()),
    )
}

fn build_domain_definition(domain: GameTaskDomain) -> TaskDomainDefinition {
    let directions = direction_seeds(domain)
        .into_iter()
        .map(|(id, label, target)| {
            let operations = OPERATION_ACTIONS
                .iter()
                .map(|(action_id, action_label)| {
                    build_operation(domain, id, label, target, action_id, action_label)
                })
                .collect();
            TaskDirection {
                id: format!("{}.{}", domain.id(), id),
                label: label.to_string(),
                operations,
            }
        })
        .collect();
    TaskDomainDefinition {
        id: domain.id().to_string(),
        label: domain.label().to_string(),
        directions,
    }
}

fn build_operation(
    domain: GameTaskDomain,
    direction_id: &str,
    direction_label: &str,
    target_kind: TargetContractKind,
    action_id: &str,
    action_label: &str,
) -> TaskOperation {
    let target = target_contract(target_kind);
    let capability = match domain {
        GameTaskDomain::CharactersAnimation
        | GameTaskDomain::WorldLevelDesign
        | GameTaskDomain::VisualAssetsVfx => ExecutionCapability::Mcp,
        GameTaskDomain::AudioMusicVoice => ExecutionCapability::ExternalProvider,
        GameTaskDomain::EngineeringQualityRelease => ExecutionCapability::BuiltIn,
        _ => ExecutionCapability::BuiltIn,
    };
    TaskOperation {
        id: format!("{}.{}.{}", domain.id(), direction_id, action_id),
        action_id: action_id.to_string(),
        label: format!("{action_label}: {direction_label}"),
        direction_id: format!("{}.{}", domain.id(), direction_id),
        domain_id: domain.id().to_string(),
        target,
        expected_artifacts: vec![format!("Результат направления «{direction_label}»")],
        recommended_tools: recommended_tools(domain, target_kind),
        validation: validation_steps(domain, target_kind),
        risk: if matches!(action_id, "validate" | "document") {
            "low".to_string()
        } else {
            "medium".to_string()
        },
        capability,
    }
}

fn direction_seeds(
    domain: GameTaskDomain,
) -> Vec<(&'static str, &'static str, TargetContractKind)> {
    use TargetContractKind as T;
    match domain {
        GameTaskDomain::Gameplay => vec![
            ("core_loop", "Основной игровой цикл", T::Gameplay),
            ("combat", "Боевая система", T::Character),
            ("movement", "Передвижение", T::Character),
            ("interaction", "Взаимодействие с миром", T::Gameplay),
            ("inventory", "Инвентарь и предметы", T::Gameplay),
            ("crafting", "Крафт и строительство", T::Gameplay),
            ("progression", "Характеристики и развитие", T::Gameplay),
            ("objectives", "Задания и цели", T::Narrative),
            ("economy", "Экономика и награды", T::Gameplay),
            ("social", "Социальные системы", T::Narrative),
        ],
        GameTaskDomain::CharactersAnimation => vec![
            ("character_setup", "Создание персонажа", T::Character),
            ("locomotion", "Локомоция", T::CharacterAnimation),
            ("combat_animation", "Боевая анимация", T::CharacterAnimation),
            (
                "context_animation",
                "Контекстная анимация",
                T::CharacterAnimation,
            ),
            (
                "animation_repair",
                "Исправление анимации",
                T::CharacterAnimation,
            ),
            (
                "control_rig",
                "Control Rig и ретаргетинг",
                T::CharacterAnimation,
            ),
            ("facial", "Лицо и диалог", T::CharacterAnimation),
            ("ai_perception", "Восприятие и решения AI", T::Ai),
            ("npc_behavior", "Поведение NPC", T::Ai),
            ("character_physics", "Физика персонажа", T::Character),
        ],
        GameTaskDomain::WorldLevelDesign => vec![
            ("layout", "Планировка уровня", T::Map),
            ("landscape", "Ландшафт", T::Map),
            ("world_partition", "World Partition", T::Map),
            ("pcg", "Процедурная генерация", T::Map),
            ("biomes", "Окружение и биомы", T::Map),
            ("level_flow", "Игровой поток уровня", T::Map),
            ("set_dressing", "Наполнение сцены", T::Map),
            ("lighting", "Освещение и атмосфера", T::Map),
            ("navigation", "Навигационное пространство", T::Map),
            ("level_validation", "Проверка уровня", T::Map),
        ],
        GameTaskDomain::VisualAssetsVfx => vec![
            ("static_mesh", "Статические 3D-модели", T::StaticMesh),
            ("character_art", "Персонажи и существа", T::Character),
            ("props", "Предметы, оружие и транспорт", T::StaticMesh),
            ("materials", "Материалы и шейдеры", T::Material),
            ("textures", "Текстуры и декали", T::Material),
            ("niagara", "Niagara VFX", T::Niagara),
            ("cameras", "Камеры", T::Cinematic),
            ("cinematics", "Синематики", T::Cinematic),
            ("rendering", "Рендеринг и постобработка", T::Project),
            ("asset_management", "Управление ассетами", T::Project),
        ],
        GameTaskDomain::NarrativeQuests => vec![
            ("lore", "Мир и лор", T::Narrative),
            ("story_structure", "Сюжетная структура", T::Narrative),
            ("dialogue", "Диалоговая система", T::Narrative),
            ("characters", "Нарративные персонажи", T::Narrative),
            ("quests", "Задания и ветвление", T::Narrative),
            ("factions", "Фракции и отношения", T::Narrative),
            ("choices", "Выборы и последствия", T::Narrative),
            ("environmental", "Нарратив окружения", T::Map),
            ("localization", "Локализация текста", T::Narrative),
            ("narrative_validation", "Проверка целостности", T::Narrative),
        ],
        GameTaskDomain::UiUxAccessibility => vec![
            ("hud", "HUD", T::Ui),
            ("menus", "Меню и навигация", T::Ui),
            ("inventory_ui", "Инвентарь и экипировка", T::Ui),
            ("map_ui", "Карта и журнал", T::Ui),
            ("onboarding", "Обучение и подсказки", T::Ui),
            ("input_prompts", "Ввод и подсказки управления", T::Ui),
            ("accessibility", "Доступность", T::Ui),
            ("responsive", "Адаптивная компоновка", T::Ui),
            ("feedback", "Обратная связь интерфейса", T::Ui),
            ("ux_testing", "UX-тестирование", T::Ui),
        ],
        GameTaskDomain::AudioMusicVoice => vec![
            ("sfx", "Звуковые эффекты", T::Audio),
            ("ambience", "Окружение и атмосфера", T::Audio),
            ("music", "Музыка", T::Audio),
            ("adaptive_music", "Адаптивная музыка", T::Audio),
            ("voice", "Озвучка", T::Audio),
            ("spatial", "Пространственный звук", T::Audio),
            ("metasounds", "MetaSounds", T::Audio),
            ("mix", "Микс и Submix", T::Audio),
            ("audio_ui", "Звук интерфейса", T::Audio),
            ("audio_validation", "Audio Insights и проверка", T::Audio),
        ],
        GameTaskDomain::EngineeringQualityRelease => vec![
            ("architecture", "Архитектура кода", T::Engineering),
            ("data", "Данные и конфигурация", T::Engineering),
            ("multiplayer", "Multiplayer и сеть", T::Engineering),
            ("online", "Онлайн-сервисы", T::Engineering),
            ("performance", "Производительность", T::Project),
            ("testing", "Тестирование", T::Project),
            ("diagnostics", "Ошибки и диагностика", T::Project),
            ("automation", "Автоматизация редактора", T::Engineering),
            ("builds", "Сборка и платформы", T::Project),
            ("release", "CI, релиз и LiveOps", T::Project),
        ],
    }
}

fn target_contract(kind: TargetContractKind) -> TargetContract {
    use ProjectGraphNodeKind as N;
    let (allowed_node_kinds, requires_skeleton, allow_group) = match kind {
        TargetContractKind::Project => (vec![N::Project, N::UnrealProject], false, false),
        TargetContractKind::Gameplay => (
            vec![
                N::UnrealProject,
                N::UnrealBlueprint,
                N::UnrealDataAsset,
                N::GameplayPlan,
            ],
            false,
            true,
        ),
        TargetContractKind::Character => (
            vec![N::UnrealBlueprint, N::UnrealSkeletalMesh, N::UnrealSkeleton],
            false,
            true,
        ),
        TargetContractKind::CharacterAnimation => (
            vec![
                N::UnrealBlueprint,
                N::UnrealSkeletalMesh,
                N::UnrealSkeleton,
                N::UnrealAnimation,
                N::UnrealAnimationBlueprint,
                N::UnrealAnimationMontage,
                N::UnrealControlRig,
            ],
            true,
            true,
        ),
        TargetContractKind::Ai => (
            vec![
                N::UnrealBlueprint,
                N::UnrealDataAsset,
                N::UnrealSkeletalMesh,
            ],
            false,
            true,
        ),
        TargetContractKind::Map => (vec![N::UnrealMap], false, true),
        TargetContractKind::StaticMesh => (vec![N::UnrealStaticMesh, N::ThreeDAsset], false, true),
        TargetContractKind::Material => (vec![N::UnrealMaterial, N::UnrealAsset], false, true),
        TargetContractKind::Niagara => (vec![N::UnrealNiagara], false, true),
        TargetContractKind::Cinematic => (
            vec![N::UnrealMap, N::UnrealBlueprint, N::UnrealAsset],
            false,
            false,
        ),
        TargetContractKind::Narrative => (
            vec![
                N::UnrealDataAsset,
                N::UnrealBlueprint,
                N::RoadmapItem,
                N::Memory,
            ],
            false,
            true,
        ),
        TargetContractKind::Ui => (
            vec![N::UnrealWidget, N::UnrealBlueprint, N::UnrealInputAsset],
            false,
            true,
        ),
        TargetContractKind::Audio => (vec![N::UnrealSound, N::UnrealAsset], false, true),
        TargetContractKind::Engineering => (
            vec![
                N::Project,
                N::File,
                N::Module,
                N::UnrealModule,
                N::UnrealSource,
                N::UnrealPlugin,
            ],
            false,
            true,
        ),
    };
    TargetContract {
        kind,
        allowed_node_kinds,
        requires_exact_target: true,
        requires_skeleton,
        allow_group,
    }
}

fn recommended_tools(domain: GameTaskDomain, target: TargetContractKind) -> Vec<String> {
    let mut tools = vec![
        "project_graph_snapshot".to_string(),
        "game_task_snapshot".to_string(),
    ];
    match target {
        TargetContractKind::CharacterAnimation
        | TargetContractKind::Map
        | TargetContractKind::Niagara => {
            tools.extend(["mcp_snapshot".to_string(), "mcp_call".to_string()]);
        }
        TargetContractKind::StaticMesh => tools.push("asset_3d_snapshot".to_string()),
        TargetContractKind::Audio => tools.push("generate_audio_asset".to_string()),
        _ => {}
    }
    if domain == GameTaskDomain::EngineeringQualityRelease {
        tools.push("project_command".to_string());
    }
    tools
}

fn validation_steps(domain: GameTaskDomain, target: TargetContractKind) -> Vec<String> {
    let mut steps = vec!["Проверить точную привязку цели по Project Map".to_string()];
    if matches!(
        target,
        TargetContractKind::CharacterAnimation | TargetContractKind::Character
    ) {
        steps.push("Проверить Skeleton, Animation Blueprint и поведение в PIE".to_string());
    } else if target == TargetContractKind::Map {
        steps.push("Запустить map smoke/playtest и проверить артефакты".to_string());
    } else if domain == GameTaskDomain::EngineeringQualityRelease {
        steps.push("Запустить check/test/build для затронутого проекта".to_string());
    } else {
        steps.push("Проверить созданный или изменённый артефакт".to_string());
    }
    steps
}

pub fn load_game_task_builder_state(workspace: &Workspace) -> GameTaskBuilderState {
    workspace
        .read_text(GAME_TASK_BUILDER_STATE_PATH, MAX_STATE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_game_task_builder_state(
    workspace: &Workspace,
    state: &GameTaskBuilderState,
) -> anyhow::Result<()> {
    workspace.write_text(
        GAME_TASK_BUILDER_STATE_PATH,
        &serde_json::to_string_pretty(state)?,
    )
}

pub fn project_map_readiness(
    workspace: &Workspace,
    refresh_if_stale: bool,
) -> ProjectMapReadinessReport {
    let graph_exists = workspace.resolve_existing(PROJECT_GRAPH_PATH).is_ok();
    if !graph_exists {
        return ProjectMapReadinessReport {
            status: ProjectMapReadinessStatus::Uninitialized,
            graph_fingerprint: String::new(),
            deep_scan_completed: false,
            unreal_project: false,
            engine_available: false,
            mcp_server_count: mcp_server_count(workspace),
            registry_path: None,
            health: empty_health("Project Map ещё не построена"),
            changes: ProjectMapChanges::default(),
            remediation: map_bootstrap_remediation(),
            updated_at: unix_timestamp(),
        };
    }
    let mut graph = load_project_graph(workspace);
    let intelligence = scan_unreal_project(workspace);
    let unreal_project = !intelligence.descriptors.is_empty();
    let project_node = graph
        .nodes
        .iter()
        .find(|node| node.kind == ProjectGraphNodeKind::UnrealProject);
    let stored_manifest = project_node
        .and_then(|node| node.metadata.get("scan_manifest"))
        .and_then(|manifest| serde_json::from_str::<Vec<UnrealProjectInput>>(manifest).ok());
    let mut changes =
        compare_project_inputs(stored_manifest.as_deref(), &intelligence.project_inputs);
    let stale = unreal_project && changes.baseline_available && !changes.is_empty();
    if stale && refresh_if_stale {
        graph = refresh_project_graph(workspace);
        let _ = save_project_graph(workspace, &graph);
        changes = ProjectMapChanges {
            baseline_available: true,
            ..ProjectMapChanges::default()
        };
    }
    let registry_path = intelligence.registry_export_path.clone();
    let deep_scan_completed = !unreal_project || registry_path.is_some();
    let registry_integrated = !unreal_project || graph_has_integrated_asset_registry(&graph);
    let snapshot = unreal_snapshot(workspace);
    let engine_available = !unreal_project || snapshot.selected_engine.is_some();
    let status = if stale && !refresh_if_stale {
        ProjectMapReadinessStatus::Stale
    } else if unreal_project && (!deep_scan_completed || !registry_integrated || !engine_available)
    {
        ProjectMapReadinessStatus::Degraded
    } else {
        ProjectMapReadinessStatus::Ready
    };
    ProjectMapReadinessReport {
        status,
        graph_fingerprint: project_graph_fingerprint(&graph),
        deep_scan_completed,
        unreal_project,
        engine_available,
        mcp_server_count: mcp_server_count(workspace),
        registry_path,
        health: graph_health(&graph, registry_integrated),
        changes: changes.clone(),
        remediation: match status {
            ProjectMapReadinessStatus::Ready => Vec::new(),
            ProjectMapReadinessStatus::Stale => project_changes_remediation(&changes),
            _ => map_bootstrap_remediation(),
        },
        updated_at: graph.updated_at,
    }
}

pub fn refresh_project_map_deep(
    workspace: &Workspace,
    args: RefreshProjectMapDeepArgs,
) -> anyhow::Result<ProjectMapReadinessReport> {
    append_journal("game_task_builder\tproject_map_scan_started");
    let started = Instant::now();
    let started_at = unix_timestamp();
    let mut state = load_game_task_builder_state(workspace);
    state.last_scan = Some(DeepScanRecord {
        status: ProjectMapReadinessStatus::Scanning,
        started_at,
        finished_at: None,
        duration_ms: None,
        detail: "Выполняется быстрый scan Project Map".to_string(),
    });
    save_game_task_builder_state(workspace, &state)?;

    let graph = refresh_project_graph(workspace);
    save_project_graph(workspace, &graph)?;
    let snapshot = unreal_snapshot(workspace);
    let mut detail = "Быстрый Project Map scan завершён".to_string();
    let mut deep_scan_failed = false;
    if args.run_unreal_scan {
        if let (Some(project), Some(engine)) = (snapshot.project, snapshot.selected_engine) {
            if let Some(editor_cmd) = engine.tools.editor_cmd {
                workspace.write_text(GAME_TASK_DEEP_SCAN_SCRIPT_PATH, DEEP_SCAN_SCRIPT)?;
                let script = workspace.resolve_existing(GAME_TASK_DEEP_SCAN_SCRIPT_PATH)?;
                let script_argument = unreal_cli_path(&script);
                let registry_before = asset_registry_signature(workspace);
                let output = Command::new(editor_cmd)
                    .arg(project.path)
                    .arg("-Unattended")
                    .arg("-NoSplash")
                    .arg("-NullRHI")
                    .arg(format!("-ExecutePythonScript={script_argument}"))
                    .arg("-log")
                    .arg("-stdout")
                    .arg("-FullStdOutLogOutput")
                    .arg("-UTF8Output")
                    .output();
                let registry_after = asset_registry_signature(workspace);
                let registry_updated =
                    registry_after.is_some() && registry_after != registry_before;
                match output {
                    Ok(output) if output.status.success() && registry_updated => {
                        detail = format!(
                            "Глубокий Unreal Asset Registry scan завершён; snapshot сохранён в {GENERATED_ASSET_REGISTRY_PATH}"
                        );
                    }
                    Ok(output) => {
                        deep_scan_failed = true;
                        detail = if output.status.success() {
                            format!(
                                "Unreal завершил процесс, но не создал {GENERATED_ASSET_REGISTRY_PATH}. {}",
                                unreal_scan_output_detail(&output)
                            )
                        } else {
                            format!(
                                "Unreal scan завершился с кодом {:?}: {}",
                                output.status.code(),
                                unreal_scan_output_detail(&output)
                            )
                        };
                    }
                    Err(error) => {
                        deep_scan_failed = true;
                        detail = format!("Не удалось запустить UnrealEditor-Cmd: {error}");
                    }
                }
                let graph = refresh_project_graph(workspace);
                save_project_graph(workspace, &graph)?;
            } else {
                detail = "UnrealEditor-Cmd не найден; доступен импорт Asset Registry или MCP"
                    .to_string();
            }
        }
    }
    let mut report = project_map_readiness(workspace, false);
    if deep_scan_failed {
        report.status = ProjectMapReadinessStatus::Failed;
        report.remediation = map_bootstrap_remediation();
    }
    state = load_game_task_builder_state(workspace);
    state.last_scan = Some(DeepScanRecord {
        status: report.status,
        started_at,
        finished_at: Some(unix_timestamp()),
        duration_ms: Some(started.elapsed().as_millis() as u64),
        detail,
    });
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\tproject_map_scan_finished\tstatus={}\tnodes={}\tedges={}",
        report.status.label(),
        report.health.node_count,
        report.health.edge_count
    ));
    Ok(report)
}

pub fn find_operation(operation_id: &str) -> Option<TaskOperation> {
    game_task_catalog_ref()
        .domains
        .iter()
        .flat_map(|domain| domain.directions.iter())
        .flat_map(|direction| direction.operations.iter())
        .find(|operation| operation.id == operation_id)
        .cloned()
}

pub fn resolve_game_task_targets(
    workspace: &Workspace,
    args: &ResolveGameTaskTargetsArgs,
) -> anyhow::Result<TargetResolutionReport> {
    let operation = find_operation(&args.operation_id)
        .ok_or_else(|| anyhow::anyhow!("операция каталога не найдена: {}", args.operation_id))?;
    let readiness = project_map_readiness(workspace, false);
    if matches!(
        readiness.status,
        ProjectMapReadinessStatus::Uninitialized | ProjectMapReadinessStatus::Stale
    ) {
        return Ok(TargetResolutionReport {
            operation_id: operation.id,
            feasibility: TaskFeasibility::StaleContext,
            recommended_candidates: Vec::new(),
            recent_candidates: Vec::new(),
            related_candidates: Vec::new(),
            candidates: Vec::new(),
            excluded: Vec::new(),
            message:
                "Сначала обновите Project Map, чтобы не привязывать задачу к устаревшему объекту"
                    .to_string(),
        });
    }
    let graph = load_project_graph(workspace);
    let semantic_index = ensure_semantic_index(workspace, &graph)?;
    let semantic_catalog = semantic_catalog_map();
    let semantic_annotations = semantic_index
        .nodes
        .iter()
        .map(|annotation| (annotation.node_id.as_str(), annotation))
        .collect::<BTreeMap<_, _>>();
    let query = args
        .query
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut candidates = graph
        .nodes
        .iter()
        .filter(|node| operation.target.allowed_node_kinds.contains(&node.kind))
        .filter(|node| {
            query.is_empty()
                || node.label.to_ascii_lowercase().contains(&query)
                || node
                    .metadata
                    .values()
                    .any(|value| value.to_ascii_lowercase().contains(&query))
                || semantic_annotations
                    .get(node.id.as_str())
                    .is_some_and(|annotation| {
                        annotation.assignments.iter().any(|assignment| {
                            assignment.tag_id.to_ascii_lowercase().contains(&query)
                                || semantic_catalog.get(&assignment.tag_id).is_some_and(
                                    |definition| {
                                        definition.label.to_ascii_lowercase().contains(&query)
                                    },
                                )
                        })
                    })
        })
        .map(|node| {
            target_binding(
                node,
                target_score(node, &query),
                "Тип узла совместим с операцией",
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .confidence
            .total_cmp(&left.confidence)
            .then(left.label.cmp(&right.label))
    });
    let candidate_count = candidates.len();
    let state = load_game_task_builder_state(workspace);
    let mut recent_records = state
        .recent_targets
        .iter()
        .filter(|recent| {
            candidates
                .iter()
                .any(|candidate| candidate.node_id == recent.target.node_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    recent_records.sort_by(|left, right| {
        recent_context_priority(right, &operation)
            .cmp(&recent_context_priority(left, &operation))
            .then(right.last_completed_at.cmp(&left.last_completed_at))
            .then(right.completed_task_count.cmp(&left.completed_task_count))
            .then(left.target.label.cmp(&right.target.label))
    });
    let mut recent_candidates = Vec::new();
    for recent in recent_records
        .into_iter()
        .take(MAX_RECENT_TARGET_SUGGESTIONS)
    {
        let Some(candidate_index) = candidates
            .iter()
            .position(|candidate| candidate.node_id == recent.target.node_id)
        else {
            continue;
        };
        let mut target = candidates.remove(candidate_index);
        target.reason = format!(
            "Недавняя цель из успешно завершённой задачи. {}",
            target.reason
        );
        let context_label = recent_context_label(&recent, &operation).to_string();
        recent_candidates.push(RecentTargetSuggestion {
            target,
            operation_label: recent.operation_label,
            context_label,
            last_completed_at: recent.last_completed_at,
            completed_task_count: recent.completed_task_count,
        });
    }

    let semantic_candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.node_id.clone())
        .collect::<BTreeSet<_>>();
    let semantic_scores =
        rank_semantic_candidates(&semantic_index, &graph, &operation, &semantic_candidate_ids);
    let mut recommended_candidates = Vec::new();
    let mut related_candidates = Vec::new();
    let mut remaining_candidates = Vec::new();
    for mut candidate in candidates {
        let Some(score) = semantic_scores.get(&candidate.node_id) else {
            remaining_candidates.push(candidate);
            continue;
        };
        if score.direct_match && score.score >= 0.24 {
            candidate.reason = format!(
                "Семантически подходит к задаче. {}",
                score.reasons.join("; ")
            );
            recommended_candidates.push(semantic_suggestion(candidate, score));
        } else if score.relation_match && score.score >= 0.12 {
            candidate.reason = format!(
                "Связан с объектами выбранной подсистемы. {}",
                score.reasons.join("; ")
            );
            related_candidates.push(semantic_suggestion(candidate, score));
        } else {
            remaining_candidates.push(candidate);
        }
    }
    recommended_candidates.sort_by(semantic_suggestion_order);
    related_candidates.sort_by(semantic_suggestion_order);
    if recommended_candidates.len() > 8 {
        remaining_candidates.extend(
            recommended_candidates
                .drain(8..)
                .map(|suggestion| suggestion.target),
        );
    }
    if related_candidates.len() > 8 {
        remaining_candidates.extend(
            related_candidates
                .drain(8..)
                .map(|suggestion| suggestion.target),
        );
    }
    let recommended_ids = recommended_candidates
        .iter()
        .map(|suggestion| suggestion.target.node_id.as_str())
        .collect::<BTreeSet<_>>();
    let related_ids = related_candidates
        .iter()
        .map(|suggestion| suggestion.target.node_id.as_str())
        .collect::<BTreeSet<_>>();
    remaining_candidates.retain(|candidate| {
        !recommended_ids.contains(candidate.node_id.as_str())
            && !related_ids.contains(candidate.node_id.as_str())
    });
    let limit = args.limit.unwrap_or(30).clamp(1, 100);
    let reserved =
        recent_candidates.len() + recommended_candidates.len() + related_candidates.len();
    remaining_candidates.truncate(limit.saturating_sub(reserved));

    let excluded = graph
        .nodes
        .iter()
        .filter(|node| {
            operation.target.kind == TargetContractKind::CharacterAnimation
                && node.kind == ProjectGraphNodeKind::UnrealStaticMesh
        })
        .take(12)
        .map(|node| {
            target_binding(
                node,
                0.0,
                "Static Mesh не может быть целью персонажной анимации",
            )
        })
        .collect::<Vec<_>>();
    let feasibility = match candidate_count {
        0 => TaskFeasibility::NeedsSetup,
        1 => TaskFeasibility::Ready,
        _ => TaskFeasibility::Ambiguous,
    };
    Ok(TargetResolutionReport {
        operation_id: operation.id,
        feasibility,
        message: match feasibility {
            TaskFeasibility::Ready => "Найдена одна точная совместимая цель".to_string(),
            TaskFeasibility::Ambiguous => {
                "Найдено несколько целей: выберите одну или явно сформируйте группу".to_string()
            }
            _ => "Совместимая цель не найдена; выберите вариант подготовки".to_string(),
        },
        recommended_candidates,
        recent_candidates,
        related_candidates,
        candidates: remaining_candidates,
        excluded,
    })
}

pub fn evaluate_game_task_prerequisites(
    workspace: &Workspace,
    args: &EvaluateGameTaskPrerequisitesArgs,
) -> anyhow::Result<PrerequisiteReport> {
    let operation = find_operation(&args.operation_id)
        .ok_or_else(|| anyhow::anyhow!("операция каталога не найдена: {}", args.operation_id))?;
    let readiness = project_map_readiness(workspace, false);
    let graph = load_project_graph(workspace);
    let bindings = args
        .target_node_ids
        .iter()
        .filter_map(|id| graph.nodes.iter().find(|node| &node.id == id))
        .map(|node| target_binding(node, 1.0, "Цель выбрана пользователем"))
        .collect::<Vec<_>>();
    let mut issues = Vec::new();
    if readiness.status != ProjectMapReadinessStatus::Ready {
        issues.push(PrerequisiteIssue {
            id: "project_map_not_ready".to_string(),
            title: "Project Map требует обновления".to_string(),
            detail: format!("Текущее состояние карты: {}", readiness.status.label()),
            affected_node_ids: args.target_node_ids.clone(),
            remediation: readiness.remediation,
        });
    }
    if bindings.len() != args.target_node_ids.len() || bindings.is_empty() {
        issues.push(PrerequisiteIssue {
            id: "exact_target_missing".to_string(),
            title: "Точная цель не выбрана".to_string(),
            detail:
                "Выберите существующий узел Project Map; агент не будет угадывать объект по имени"
                    .to_string(),
            affected_node_ids: args.target_node_ids.clone(),
            remediation: vec![remediation(
                "select_target",
                "Выбрать объект Project Map",
                "Показать только совместимые цели и их связи",
                vec!["project_graph_snapshot"],
                "низкая",
                "до минуты",
                "low",
                false,
            )],
        });
    }
    for binding in &bindings {
        if !operation
            .target
            .allowed_node_kinds
            .contains(&binding.node_kind)
        {
            issues.push(PrerequisiteIssue {
                id: format!("incompatible_target:{}", binding.node_id),
                title: "Несовместимый тип цели".to_string(),
                detail: format!(
                    "{} имеет тип {}, который не поддерживает выбранную операцию",
                    binding.label,
                    binding.node_kind.as_str()
                ),
                affected_node_ids: vec![binding.node_id.clone()],
                remediation: vec![remediation(
                    "change_target",
                    "Выбрать другой объект",
                    "Вернуться к списку совместимых целей",
                    vec!["project_graph_snapshot"],
                    "низкая",
                    "до минуты",
                    "low",
                    false,
                )],
            });
        }
        if operation.target.requires_skeleton
            && !has_related_kind(
                &graph,
                &binding.node_id,
                ProjectGraphNodeKind::UnrealSkeleton,
                3,
            )
            && binding.node_kind != ProjectGraphNodeKind::UnrealSkeleton
        {
            issues.push(missing_skeleton_issue(&binding.node_id));
        }
    }
    let feasibility = if issues
        .iter()
        .any(|issue| issue.id == "project_map_not_ready")
    {
        TaskFeasibility::StaleContext
    } else if !issues.is_empty() {
        TaskFeasibility::NeedsSetup
    } else if operation.capability != ExecutionCapability::BuiltIn {
        TaskFeasibility::ExternalToolRequired
    } else {
        TaskFeasibility::Ready
    };
    Ok(PrerequisiteReport {
        operation_id: operation.id,
        feasibility,
        issues,
        target_bindings: bindings,
    })
}

pub fn prepare_game_task_proposal(
    workspace: &Workspace,
    args: PrepareGameTaskProposalArgs,
) -> anyhow::Result<GameTaskBuilderSession> {
    let operation = find_operation(&args.operation_id)
        .ok_or_else(|| anyhow::anyhow!("операция каталога не найдена: {}", args.operation_id))?;
    let prerequisite = evaluate_game_task_prerequisites(
        workspace,
        &EvaluateGameTaskPrerequisitesArgs {
            operation_id: operation.id.clone(),
            target_node_ids: args.target_node_ids.clone(),
        },
    )?;
    let graph = load_project_graph(workspace);
    let target_ids = prerequisite
        .target_bindings
        .iter()
        .map(|binding| binding.node_id.clone())
        .collect::<BTreeSet<_>>();
    let dependencies = graph
        .edges
        .iter()
        .filter(|edge| target_ids.contains(&edge.from) || target_ids.contains(&edge.to))
        .filter_map(|edge| {
            let id = if target_ids.contains(&edge.from) {
                &edge.to
            } else {
                &edge.from
            };
            graph.nodes.iter().find(|node| &node.id == id)
        })
        .take(24)
        .map(|node| target_binding(node, node.confidence, "Связанный узел Project Map"))
        .collect::<Vec<_>>();
    let excluded = graph
        .nodes
        .iter()
        .filter(|node| {
            operation.target.kind == TargetContractKind::CharacterAnimation
                && node.kind == ProjectGraphNodeKind::UnrealStaticMesh
        })
        .take(8)
        .map(|node| target_binding(node, 0.0, "Исключён как несовместимый Static Mesh"))
        .collect::<Vec<_>>();
    let custom = args.custom_request.unwrap_or_default();
    let understood_task = format!(
        "{} для {}{}",
        operation.label,
        prerequisite
            .target_bindings
            .iter()
            .map(|target| target.label.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        if custom.trim().is_empty() {
            String::new()
        } else {
            format!(". Уточнение: {}", custom.trim())
        }
    );
    let preparation = prerequisite
        .issues
        .iter()
        .flat_map(|issue| {
            issue
                .remediation
                .iter()
                .filter(|option| args.remediation_ids.contains(&option.id))
                .map(|option| format!("{}: {}", option.title, option.description))
        })
        .collect::<Vec<_>>();
    let proposal = TaskProposal {
        understood_task,
        exact_targets: prerequisite.target_bindings,
        excluded_targets: excluded,
        affected_dependencies: dependencies,
        steps: proposal_steps(&operation),
        preparation,
        expected_artifacts: operation.expected_artifacts.clone(),
        validation: operation.validation.clone(),
        risks: vec![
            "Изменять только подтверждённые target node IDs и object paths".to_string(),
            format!("Risk class операции: {}", operation.risk),
        ],
        improvements: improvement_suggestions(&operation),
        efficiency: efficiency_suggestions(&operation),
        feasibility: prerequisite.feasibility,
    };
    let now = unix_timestamp();
    let direction_id = operation.direction_id.clone();
    let domain_id = operation.domain_id.clone();
    let mut state = load_game_task_builder_state(workspace);
    let session = GameTaskBuilderSession {
        id: format!("task-session-{}", Uuid::new_v4()),
        domain_id,
        direction_id,
        operation_id: operation.id,
        custom_request: custom,
        target_node_ids: args.target_node_ids,
        remediation_ids: args.remediation_ids,
        proposal: Some(proposal),
        graph_fingerprint: project_graph_fingerprint(&graph),
        status: "awaiting_confirmation".to_string(),
        created_at: now,
        updated_at: now,
    };
    state.active_session_id = Some(session.id.clone());
    state.sessions.push(session.clone());
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\tproposal_prepared\tsession={}\toperation={}\ttargets={}",
        session.id,
        session.operation_id,
        session.target_node_ids.join(",")
    ));
    Ok(session)
}

pub fn confirm_game_task_proposal(
    workspace: &Workspace,
    session_id: &str,
    improvement_ids: Vec<String>,
    efficiency_ids: Vec<String>,
) -> anyhow::Result<TaskManifest> {
    let readiness = project_map_readiness(workspace, false);
    if readiness.status != ProjectMapReadinessStatus::Ready {
        anyhow::bail!(
            "Project Map имеет состояние {}; изменяющая игровая задача не может быть подтверждена до обновления карты",
            readiness.status.label()
        );
    }
    let graph = load_project_graph(workspace);
    let fingerprint = project_graph_fingerprint(&graph);
    let mut state = load_game_task_builder_state(workspace);
    let session = state
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
        .ok_or_else(|| anyhow::anyhow!("сессия конструктора не найдена"))?;
    if session.graph_fingerprint != fingerprint {
        anyhow::bail!("Project Map изменилась; обновите привязку целей перед подтверждением");
    }
    let proposal = session
        .proposal
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("предложение задачи ещё не подготовлено"))?;
    if proposal.exact_targets.is_empty() {
        anyhow::bail!("точная цель Project Map не выбрана");
    }
    let target_ids = proposal
        .exact_targets
        .iter()
        .map(|target| target.node_id.clone())
        .collect::<Vec<_>>();
    let mut allowed = target_ids.iter().cloned().collect::<BTreeSet<_>>();
    for edge in &graph.edges {
        if target_ids.contains(&edge.from) {
            allowed.insert(edge.to.clone());
        }
        if target_ids.contains(&edge.to) {
            allowed.insert(edge.from.clone());
        }
    }
    let manifest = TaskManifest {
        id: format!("task-manifest-{}", Uuid::new_v4()),
        session_id: session.id.clone(),
        operation_id: session.operation_id.clone(),
        understood_task: proposal.understood_task.clone(),
        target_node_ids: target_ids,
        allowed_node_ids: allowed.into_iter().collect(),
        object_paths: proposal
            .exact_targets
            .iter()
            .filter_map(|target| target.object_path.clone())
            .collect(),
        selected_improvement_ids: improvement_ids,
        selected_efficiency_ids: efficiency_ids,
        graph_fingerprint: fingerprint,
        confirmed_at: unix_timestamp(),
    };
    session.status = "confirmed".to_string();
    session.updated_at = unix_timestamp();
    state.active_manifest_id = Some(manifest.id.clone());
    state.manifests.push(manifest.clone());
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\tmanifest_confirmed\tmanifest={}\toperation={}\ttargets={}",
        manifest.id,
        manifest.operation_id,
        manifest.target_node_ids.join(",")
    ));
    Ok(manifest)
}

pub fn propose_project_relation(
    workspace: &Workspace,
    args: ProposeProjectRelationArgs,
) -> anyhow::Result<ProjectRelationProposal> {
    let graph = load_project_graph(workspace);
    if !graph.nodes.iter().any(|node| node.id == args.from_node_id)
        || !graph.nodes.iter().any(|node| node.id == args.to_node_id)
    {
        anyhow::bail!("оба узла relation proposal должны существовать в Project Map");
    }
    let kind = parse_semantic_edge_kind(&args.kind)
        .ok_or_else(|| anyhow::anyhow!("неподдерживаемый тип связи: {}", args.kind))?;
    let proposal = ProjectRelationProposal {
        id: format!("relation-proposal-{}", Uuid::new_v4()),
        from_node_id: args.from_node_id,
        to_node_id: args.to_node_id,
        kind,
        label: kind.label().to_string(),
        reason: args.reason,
        status: "pending".to_string(),
        created_at: unix_timestamp(),
    };
    let mut state = load_game_task_builder_state(workspace);
    state.relation_proposals.push(proposal.clone());
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\trelation_proposed\tproposal={}\tkind={}\tfrom={}\tto={}",
        proposal.id,
        proposal.kind.as_str(),
        proposal.from_node_id,
        proposal.to_node_id
    ));
    Ok(proposal)
}

pub fn answer_project_relation_proposal(
    workspace: &Workspace,
    proposal_id: &str,
    approved: bool,
) -> anyhow::Result<()> {
    let mut state = load_game_task_builder_state(workspace);
    let proposal = state
        .relation_proposals
        .iter_mut()
        .find(|proposal| proposal.id == proposal_id)
        .ok_or_else(|| anyhow::anyhow!("relation proposal не найден"))?;
    proposal.status = if approved { "approved" } else { "rejected" }.to_string();
    if approved {
        let mut graph = load_project_graph(workspace);
        let id = format!(
            "edge:{}:{}->{}",
            proposal.kind.as_str(),
            proposal.from_node_id,
            proposal.to_node_id
        );
        graph.edges.retain(|edge| edge.id != id);
        graph.edges.push(ProjectGraphEdge {
            id,
            from: proposal.from_node_id.clone(),
            to: proposal.to_node_id.clone(),
            kind: proposal.kind,
            label: proposal.kind.label().to_string(),
            source: "ui:game_task_builder".to_string(),
            confidence: 1.0,
            updated_at: unix_timestamp(),
        });
        graph.updated_at = unix_timestamp();
        save_project_graph(workspace, &graph)?;
    }
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\trelation_decided\tproposal={}\tapproved={}",
        proposal_id, approved
    ));
    Ok(())
}

pub fn save_custom_catalog_option(
    workspace: &Workspace,
    parent_id: &str,
    label: &str,
) -> anyhow::Result<CustomCatalogOption> {
    let label = label.trim();
    if label.is_empty() {
        anyhow::bail!("пользовательский вариант не может быть пустым");
    }
    let mut state = load_game_task_builder_state(workspace);
    if let Some(existing) = state
        .custom_options
        .iter()
        .find(|option| option.parent_id == parent_id && option.label.eq_ignore_ascii_case(label))
        .cloned()
    {
        return Ok(existing);
    }
    let option = CustomCatalogOption {
        id: format!("custom-{}", Uuid::new_v4()),
        parent_id: parent_id.to_string(),
        label: label.to_string(),
        created_at: unix_timestamp(),
    };
    state.custom_options.push(option.clone());
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\tcustom_option_saved\toption={}\tparent={}",
        option.id, option.parent_id
    ));
    Ok(option)
}

pub fn game_task_snapshot(workspace: &Workspace) -> ToolResult {
    let state = load_game_task_builder_state(workspace);
    let readiness = project_map_readiness(workspace, false);
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "readiness": readiness,
            "active_session": state.active_session_id.as_deref().and_then(|id| state.sessions.iter().find(|session| session.id == id)),
            "active_manifest": state.active_manifest_id.as_deref().and_then(|id| state.manifests.iter().find(|manifest| manifest.id == id)),
            "pending_relation_proposals": state.relation_proposals.iter().filter(|proposal| proposal.status == "pending").collect::<Vec<_>>(),
            "custom_options": state.custom_options,
            "recent_targets": state.recent_targets,
        }))
        .unwrap_or_else(|_| "game task snapshot".to_string()),
    )
}

pub fn active_task_manifest_context_value(workspace: &Workspace) -> Option<Value> {
    let state = load_game_task_builder_state(workspace);
    let manifest_id = state.active_manifest_id.as_deref()?;
    let manifest = state
        .manifests
        .iter()
        .find(|manifest| manifest.id == manifest_id)?;
    Some(json!({
        "manifest_id": manifest.id,
        "operation_id": manifest.operation_id,
        "graph_fingerprint": manifest.graph_fingerprint,
        "target_node_ids": manifest.target_node_ids,
        "allowed_node_ids": manifest.allowed_node_ids,
        "object_paths": manifest.object_paths,
    }))
}

pub fn finish_active_task_manifest(workspace: &Workspace, status: &str) -> anyhow::Result<()> {
    let mut state = load_game_task_builder_state(workspace);
    let Some(manifest_id) = state.active_manifest_id.take() else {
        return Ok(());
    };
    let manifest = state
        .manifests
        .iter()
        .find(|manifest| manifest.id == manifest_id)
        .cloned();
    let session = manifest.as_ref().and_then(|manifest| {
        state
            .sessions
            .iter()
            .find(|session| session.id == manifest.session_id)
            .cloned()
    });
    if let Some(session_id) = manifest
        .as_ref()
        .map(|manifest| manifest.session_id.clone())
    {
        if let Some(session) = state
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = status.to_string();
            session.updated_at = unix_timestamp();
        }
    }
    if status == "completed" {
        if let Some(manifest) = manifest.as_ref() {
            record_recent_task_targets(workspace, &mut state, manifest, session.as_ref());
        }
    }
    save_game_task_builder_state(workspace, &state)?;
    append_journal(format!(
        "game_task_builder\tmanifest_finished\tmanifest={}\tstatus={}",
        manifest_id, status
    ));
    Ok(())
}

pub fn validate_tool_action_against_active_manifest(
    workspace: &Workspace,
    action: &ToolAction,
    args: &Value,
) -> anyhow::Result<()> {
    if !is_manifest_guarded_action(action) {
        return Ok(());
    }
    let state = load_game_task_builder_state(workspace);
    let Some(manifest_id) = state.active_manifest_id.as_deref() else {
        return Ok(());
    };
    let manifest = state
        .manifests
        .iter()
        .find(|manifest| manifest.id == manifest_id)
        .ok_or_else(|| anyhow::anyhow!("активный TaskManifest не найден"))?;
    let graph = load_project_graph(workspace);
    if project_graph_fingerprint(&graph) != manifest.graph_fingerprint {
        anyhow::bail!(
            "Project Map изменилась после подтверждения задачи; обновите target binding и подтвердите план повторно"
        );
    }
    for target in &manifest.target_node_ids {
        if !graph.nodes.iter().any(|node| &node.id == target) {
            anyhow::bail!("подтверждённый target node исчез из Project Map: {target}");
        }
    }
    let explicit_references = collect_project_references(args);
    for reference in explicit_references {
        let matching_node = graph.nodes.iter().find(|node| {
            node.id == reference
                || node.path.as_deref().is_some_and(|path| {
                    path == reference || (path.len() > 3 && reference.contains(path))
                })
                || node.metadata.get("object_path").is_some_and(|path| {
                    path == &reference || (path.len() > 3 && reference.contains(path))
                })
                || node.metadata.get("package_name").is_some_and(|path| {
                    path == &reference || (path.len() > 3 && reference.contains(path))
                })
        });
        if let Some(node) = matching_node {
            if !manifest.allowed_node_ids.contains(&node.id) {
                anyhow::bail!(
                    "операция обращается к узлу вне подтверждённого TaskManifest: {} ({})",
                    node.label,
                    node.id
                );
            }
        }
    }
    Ok(())
}

pub fn game_task_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Конструктор игровых задач: рабочая папка не выбрана.".to_string();
    };
    let state = load_game_task_builder_state(workspace);
    let readiness = project_map_readiness(workspace, false);
    let active = state
        .active_manifest_id
        .as_deref()
        .and_then(|id| state.manifests.iter().find(|manifest| manifest.id == id));
    match active {
        Some(manifest) => format!(
            "Конструктор игровых задач: Project Map {}, manifest {} подтверждён; задача: {}; точные цели: {}. Не меняй игровые объекты вне target/allowed node IDs и не перепривязывай цель молча.",
            readiness.status.label(),
            manifest.id,
            manifest.understood_task,
            manifest.target_node_ids.join(", ")
        ),
        None => format!(
            "Конструктор игровых задач: Project Map {}, активного TaskManifest нет. Для игровой изменяющей задачи используй project_map_readiness и воронку каталога до точной привязки объекта.",
            readiness.status.label()
        ),
    }
}

fn compare_project_inputs(
    baseline: Option<&[UnrealProjectInput]>,
    current: &[UnrealProjectInput],
) -> ProjectMapChanges {
    let Some(baseline) = baseline else {
        return ProjectMapChanges::default();
    };
    let baseline_by_path = baseline
        .iter()
        .map(|input| (input.path.to_ascii_lowercase(), input))
        .collect::<BTreeMap<_, _>>();
    let current_by_path = current
        .iter()
        .map(|input| (input.path.to_ascii_lowercase(), input))
        .collect::<BTreeMap<_, _>>();

    let mut added_paths = Vec::new();
    let mut modified_paths = Vec::new();
    let mut removed_paths = Vec::new();
    let mut added_count = 0;
    let mut modified_count = 0;
    let mut removed_count = 0;

    for (key, input) in &current_by_path {
        match baseline_by_path.get(key) {
            None => {
                added_count += 1;
                if added_paths.len() < MAX_REPORTED_PROJECT_CHANGES {
                    added_paths.push(input.path.clone());
                }
            }
            Some(previous)
                if previous.size != input.size || previous.modified_ns != input.modified_ns =>
            {
                modified_count += 1;
                if modified_paths.len() < MAX_REPORTED_PROJECT_CHANGES {
                    modified_paths.push(input.path.clone());
                }
            }
            Some(_) => {}
        }
    }
    for (key, input) in &baseline_by_path {
        if !current_by_path.contains_key(key) {
            removed_count += 1;
            if removed_paths.len() < MAX_REPORTED_PROJECT_CHANGES {
                removed_paths.push(input.path.clone());
            }
        }
    }

    ProjectMapChanges {
        baseline_available: true,
        added_count,
        modified_count,
        removed_count,
        added_paths,
        modified_paths,
        removed_paths,
    }
}

fn graph_health(graph: &ProjectGraphState, asset_registry_integrated: bool) -> ProjectMapHealth {
    let semantic = graph
        .edges
        .iter()
        .filter(|edge| is_semantic_edge(edge.kind))
        .count();
    let external_nodes = graph
        .nodes
        .iter()
        .filter(|node| {
            node.metadata
                .get("external")
                .is_some_and(|value| value == "true")
        })
        .collect::<Vec<_>>();
    let unresolved = external_nodes
        .iter()
        .filter(|node| {
            node.metadata
                .get("external_scope")
                .is_some_and(|scope| scope == "project")
                || node
                    .metadata
                    .get("object_path")
                    .is_some_and(|path| path.starts_with("/Game/"))
        })
        .count();
    let external_dependency_nodes = external_nodes.len().saturating_sub(unresolved);
    let mut labels = BTreeMap::<String, usize>::new();
    for node in &graph.nodes {
        *labels.entry(node.label.to_ascii_lowercase()).or_default() += 1;
    }
    let ambiguous = labels.values().filter(|count| **count > 1).count();
    let cycles = dependency_cycle_count(graph);
    let stale_nodes = graph
        .nodes
        .iter()
        .filter(|node| {
            node.metadata
                .get("stale")
                .is_some_and(|value| value == "true")
        })
        .count();
    let linked_node_ids = graph
        .edges
        .iter()
        .flat_map(|edge| [edge.from.as_str(), edge.to.as_str()])
        .collect::<BTreeSet<_>>();
    let linked_nodes = linked_node_ids.len();
    let code_nodes = graph
        .nodes
        .iter()
        .filter(|node| is_code_node(node.kind))
        .count();
    let unreal_asset_nodes = graph
        .nodes
        .iter()
        .filter(|node| is_unreal_asset_node(node.kind))
        .count();
    let planning_nodes = graph
        .nodes
        .iter()
        .filter(|node| is_planning_node(node.kind))
        .count();
    let mut diagnostics = Vec::new();
    if !asset_registry_integrated {
        diagnostics.push("Asset Registry ещё не встроен в Project Map".to_string());
    }
    if unresolved > 0 {
        diagnostics.push(format!("Неразрешённых внешних ссылок: {unresolved}"));
    }
    if ambiguous > 0 {
        diagnostics.push(format!("Неоднозначных подписей узлов: {ambiguous}"));
    }
    if cycles > 0 {
        diagnostics.push(format!("Циклов зависимостей: {cycles}"));
    }
    if stale_nodes > 0 {
        diagnostics.push(format!("Устаревших узлов: {stale_nodes}"));
    }
    let structure_score = if graph.nodes.is_empty() { 0 } else { 40 };
    let registry_score = if asset_registry_integrated { 35 } else { 0 };
    let semantic_score = if asset_registry_integrated { 15 } else { 0 };
    // Readiness measures whether the source was analysed, not whether the project is clean.
    // Missing /Game targets stay visible as diagnostics but do not turn a completed scan into
    // an endless 90/100 loop.
    let reference_score = if asset_registry_integrated || unresolved == 0 {
        10
    } else {
        0
    };
    let coverage_areas = vec![
        ProjectMapCoverageArea {
            id: "project_structure".to_string(),
            label: "Структура проекта".to_string(),
            score: structure_score,
            max_score: 40,
            ready: structure_score == 40,
            detail: if graph.nodes.is_empty() {
                "Файлы, каталоги и дескриптор проекта ещё не просканированы".to_string()
            } else {
                format!(
                    "Найдено {} узлов; {} из них участвуют хотя бы в одной связи",
                    graph.nodes.len(),
                    linked_nodes
                )
            },
        },
        ProjectMapCoverageArea {
            id: "asset_registry".to_string(),
            label: "Данные Unreal Asset Registry".to_string(),
            score: registry_score,
            max_score: 35,
            ready: asset_registry_integrated,
            detail: if asset_registry_integrated {
                format!(
                    "Получены точные классы, object paths и зависимости для {unreal_asset_nodes} Unreal-ассетов"
                )
            } else {
                "Asset Registry ещё не встроен в карту: классы ассетов, object paths и package dependencies пока определены только приблизительно".to_string()
            },
        },
        ProjectMapCoverageArea {
            id: "semantic_relations".to_string(),
            label: "Игровые семантические связи".to_string(),
            score: semantic_score,
            max_score: 15,
            ready: asset_registry_integrated,
            detail: if semantic > 0 {
                format!(
                    "Построено {semantic} связей типа Skeleton, анимация, компоненты, ввод и владение"
                )
            } else if asset_registry_integrated {
                "Семантический проход завершён; поддерживаемых связей Skeleton/Animation Blueprint/Input/Components в текущих данных не найдено".to_string()
            } else {
                "Семантический проход запустится автоматически после импорта Asset Registry"
                    .to_string()
            },
        },
        ProjectMapCoverageArea {
            id: "external_references".to_string(),
            label: "Разрешение внешних ссылок".to_string(),
            score: reference_score,
            max_score: 10,
            ready: asset_registry_integrated || unresolved == 0,
            detail: if unresolved == 0 {
                "Все найденные ссылки указывают на известные узлы карты".to_string()
            } else if asset_registry_integrated {
                format!(
                    "Проверка завершена; {unresolved} ссылок внутри /Game не имеют цели в текущем Asset Registry и показаны как диагностика проекта"
                )
            } else {
                format!(
                    "{unresolved} ссылок указывают на пакеты или ассеты, которых нет в текущем snapshot; это не обязательно повреждённые файлы"
                )
            },
        },
    ];
    let coverage = structure_score + registry_score + semantic_score + reference_score;
    ProjectMapHealth {
        coverage_percent: coverage.min(100),
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
        semantic_edge_count: semantic,
        unresolved_nodes: unresolved,
        external_dependency_nodes,
        ambiguous_labels: ambiguous,
        dependency_cycles: cycles,
        stale_nodes,
        linked_nodes,
        code_nodes,
        unreal_asset_nodes,
        planning_nodes,
        asset_registry_integrated,
        coverage_areas,
        diagnostics,
    }
}

fn empty_health(detail: &str) -> ProjectMapHealth {
    ProjectMapHealth {
        coverage_percent: 0,
        node_count: 0,
        edge_count: 0,
        semantic_edge_count: 0,
        unresolved_nodes: 0,
        external_dependency_nodes: 0,
        ambiguous_labels: 0,
        dependency_cycles: 0,
        stale_nodes: 0,
        linked_nodes: 0,
        code_nodes: 0,
        unreal_asset_nodes: 0,
        planning_nodes: 0,
        asset_registry_integrated: false,
        coverage_areas: vec![ProjectMapCoverageArea {
            id: "project_structure".to_string(),
            label: "Структура проекта".to_string(),
            score: 0,
            max_score: 40,
            ready: false,
            detail: detail.to_string(),
        }],
        diagnostics: vec![detail.to_string()],
    }
}

fn graph_has_integrated_asset_registry(graph: &ProjectGraphState) -> bool {
    graph.nodes.iter().any(|node| {
        node.metadata
            .get("asset_source")
            .is_some_and(|source| source == "asset_registry")
            || node
                .metadata
                .get("asset_registry")
                .is_some_and(|path| !path.trim().is_empty())
    })
}

fn is_code_node(kind: ProjectGraphNodeKind) -> bool {
    matches!(
        kind,
        ProjectGraphNodeKind::File
            | ProjectGraphNodeKind::Module
            | ProjectGraphNodeKind::Symbol
            | ProjectGraphNodeKind::UnrealModule
            | ProjectGraphNodeKind::UnrealTarget
            | ProjectGraphNodeKind::UnrealConfig
            | ProjectGraphNodeKind::UnrealSource
    )
}

fn is_unreal_asset_node(kind: ProjectGraphNodeKind) -> bool {
    matches!(
        kind,
        ProjectGraphNodeKind::UnrealMap
            | ProjectGraphNodeKind::UnrealBlueprint
            | ProjectGraphNodeKind::UnrealDataAsset
            | ProjectGraphNodeKind::UnrealMaterial
            | ProjectGraphNodeKind::UnrealNiagara
            | ProjectGraphNodeKind::UnrealAnimation
            | ProjectGraphNodeKind::UnrealSkeleton
            | ProjectGraphNodeKind::UnrealSkeletalMesh
            | ProjectGraphNodeKind::UnrealStaticMesh
            | ProjectGraphNodeKind::UnrealAnimationBlueprint
            | ProjectGraphNodeKind::UnrealAnimationMontage
            | ProjectGraphNodeKind::UnrealControlRig
            | ProjectGraphNodeKind::UnrealPhysicsAsset
            | ProjectGraphNodeKind::UnrealSound
            | ProjectGraphNodeKind::UnrealWidget
            | ProjectGraphNodeKind::UnrealInputAsset
            | ProjectGraphNodeKind::UnrealAsset
    )
}

fn is_planning_node(kind: ProjectGraphNodeKind) -> bool {
    matches!(
        kind,
        ProjectGraphNodeKind::Memory
            | ProjectGraphNodeKind::RoadmapItem
            | ProjectGraphNodeKind::GameplayPlan
            | ProjectGraphNodeKind::GameplayRun
            | ProjectGraphNodeKind::GameProductionPlan
            | ProjectGraphNodeKind::ProductionItem
            | ProjectGraphNodeKind::VerticalSliceRun
            | ProjectGraphNodeKind::VerticalSlicePhase
    )
}

fn project_changes_remediation(changes: &ProjectMapChanges) -> Vec<RemediationOption> {
    vec![remediation(
        "sync_project_changes",
        "Синхронизировать изменения",
        &format!(
            "Обновить Project Map по {} изменениям: +{} новых, ~{} изменённых, −{} удалённых",
            changes.total(),
            changes.added_count,
            changes.modified_count,
            changes.removed_count
        ),
        vec!["refresh_project_map_deep"],
        "низкая",
        "от нескольких секунд",
        "low",
        false,
    )]
}

fn map_bootstrap_remediation() -> Vec<RemediationOption> {
    vec![
        remediation(
            "run_deep_scan",
            "Построить глубокую карту",
            "Запустить быстрый scan и Unreal Asset Registry export",
            vec!["refresh_project_map_deep"],
            "средняя",
            "1–10 минут",
            "medium",
            true,
        ),
        remediation(
            "configure_engine",
            "Настроить Unreal Engine",
            "Выбрать совместимую установку Unreal Engine 5.8",
            vec!["unreal_snapshot"],
            "низкая",
            "1–3 минуты",
            "low",
            false,
        ),
        remediation(
            "connect_mcp",
            "Подключить Unreal MCP",
            "Добавить MCP server и обнаружить его инструменты",
            vec!["mcp_discover"],
            "средняя",
            "2–10 минут",
            "medium",
            true,
        ),
        remediation(
            "import_registry",
            "Импортировать Asset Registry",
            "Выбрать существующий JSON export из Unreal Editor",
            vec!["project_graph_snapshot"],
            "низкая",
            "до минуты",
            "low",
            false,
        ),
    ]
}

fn missing_skeleton_issue(node_id: &str) -> PrerequisiteIssue {
    PrerequisiteIssue {
        id: format!("missing_skeleton:{node_id}"),
        title: "Skeleton не найден".to_string(),
        detail: "Для персонажной анимации нужно связать Skeletal Mesh/Animation Blueprint с точным Skeleton".to_string(),
        affected_node_ids: vec![node_id.to_string()],
        remediation: vec![
            remediation("create_skeleton", "Создать Skeleton", "Построить Skeleton из выбранного Skeletal Mesh и проверить иерархию костей", vec!["mcp_call"], "средняя", "5–20 минут", "medium", true),
            remediation("select_skeleton", "Выбрать совместимый Skeleton", "Показать Skeleton с совпадающими bone chains", vec!["project_graph_snapshot"], "низкая", "1–3 минуты", "low", false),
            remediation("import_rigged_model", "Импортировать модель с ригом", "Импортировать FBX/GLB и создать Skeletal Mesh/Skeleton", vec!["import_3d_asset_unreal"], "высокая", "10–40 минут", "high", true),
            remediation("use_mannequin", "Использовать временный Mannequin", "Создать временную привязку и добавить последующий retarget", vec!["mcp_call"], "средняя", "5–15 минут", "medium", true),
            remediation("change_target", "Изменить объект", "Вернуться к совместимым целям Project Map", vec!["resolve_game_task_targets"], "низкая", "до минуты", "low", false),
            remediation("custom_resolution", "Свой вариант", "Описать способ подготовки вручную", vec![], "не определена", "не определено", "medium", true),
        ],
    }
}

fn remediation(
    id: &str,
    title: &str,
    description: &str,
    tools: Vec<&str>,
    complexity: &str,
    estimated_time: &str,
    risk: &str,
    requires_approval: bool,
) -> RemediationOption {
    RemediationOption {
        id: id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        expected_artifacts: Vec::new(),
        tools: tools.into_iter().map(ToString::to_string).collect(),
        complexity: complexity.to_string(),
        estimated_time: estimated_time.to_string(),
        risk: risk.to_string(),
        requires_approval,
    }
}

fn target_binding(node: &ProjectGraphNode, confidence: f32, reason: &str) -> TaskTargetBinding {
    TaskTargetBinding {
        node_id: node.id.clone(),
        label: node.label.clone(),
        node_kind: node.kind,
        object_path: node
            .metadata
            .get("object_path")
            .cloned()
            .or_else(|| node.path.clone()),
        confidence,
        reason: reason.to_string(),
    }
}

fn semantic_suggestion(
    target: TaskTargetBinding,
    score: &SemanticCandidateScore,
) -> SemanticTargetSuggestion {
    SemanticTargetSuggestion {
        target,
        semantic_score: score.score,
        tag_labels: score.tag_labels.clone(),
        reasons: score.reasons.clone(),
        relation_labels: score.relation_labels.clone(),
    }
}

fn semantic_suggestion_order(
    left: &SemanticTargetSuggestion,
    right: &SemanticTargetSuggestion,
) -> std::cmp::Ordering {
    right
        .semantic_score
        .total_cmp(&left.semantic_score)
        .then(right.target.confidence.total_cmp(&left.target.confidence))
        .then(left.target.label.cmp(&right.target.label))
}

fn target_score(node: &ProjectGraphNode, query: &str) -> f32 {
    let mut score = node.confidence.max(0.4);
    if !query.is_empty() && node.label.to_ascii_lowercase() == query {
        score += 0.2;
    }
    if node.source.contains("asset_registry") {
        score += 0.1;
    }
    score.min(1.0)
}

fn recent_context_priority(recent: &RecentTaskTarget, operation: &TaskOperation) -> u8 {
    if recent.operation_id == operation.id {
        3
    } else if recent.direction_id == operation.direction_id {
        2
    } else if recent.domain_id == operation.domain_id {
        1
    } else {
        0
    }
}

fn recent_context_label(recent: &RecentTaskTarget, operation: &TaskOperation) -> &'static str {
    match recent_context_priority(recent, operation) {
        3 => "та же операция",
        2 => "то же направление",
        1 => "та же сфера",
        _ => "другая задача проекта",
    }
}

fn operation_records_recent_targets(operation_id: &str) -> bool {
    !matches!(
        operation_id.rsplit('.').next(),
        Some("validate" | "document")
    )
}

fn record_recent_task_targets(
    workspace: &Workspace,
    state: &mut GameTaskBuilderState,
    manifest: &TaskManifest,
    session: Option<&GameTaskBuilderSession>,
) {
    if !operation_records_recent_targets(&manifest.operation_id) {
        return;
    }
    let graph = load_project_graph(workspace);
    let operation = find_operation(&manifest.operation_id);
    let domain_id = session
        .map(|session| session.domain_id.clone())
        .or_else(|| {
            operation
                .as_ref()
                .map(|operation| operation.domain_id.clone())
        })
        .unwrap_or_default();
    let direction_id = session
        .map(|session| session.direction_id.clone())
        .or_else(|| {
            operation
                .as_ref()
                .map(|operation| operation.direction_id.clone())
        })
        .unwrap_or_default();
    let operation_label = operation
        .as_ref()
        .map(|operation| operation.label.clone())
        .unwrap_or_else(|| manifest.operation_id.clone());
    let proposal_targets = session
        .and_then(|session| session.proposal.as_ref())
        .map(|proposal| proposal.exact_targets.as_slice())
        .unwrap_or_default();
    let now = unix_timestamp();

    for node_id in &manifest.target_node_ids {
        let target = graph
            .nodes
            .iter()
            .find(|node| &node.id == node_id)
            .map(|node| target_binding(node, 1.0, "Цель успешно завершённой задачи конструктора"))
            .or_else(|| {
                proposal_targets
                    .iter()
                    .find(|target| &target.node_id == node_id)
                    .cloned()
            });
        let Some(target) = target else {
            continue;
        };
        let completed_task_count = state
            .recent_targets
            .iter()
            .find(|recent| recent.target.node_id == *node_id)
            .map(|recent| recent.completed_task_count.saturating_add(1))
            .unwrap_or(1);
        state
            .recent_targets
            .retain(|recent| recent.target.node_id != *node_id);
        state.recent_targets.push(RecentTaskTarget {
            target,
            domain_id: domain_id.clone(),
            direction_id: direction_id.clone(),
            operation_id: manifest.operation_id.clone(),
            operation_label: operation_label.clone(),
            last_completed_at: now,
            completed_task_count,
        });
    }
    state.recent_targets.sort_by(|left, right| {
        right
            .last_completed_at
            .cmp(&left.last_completed_at)
            .then(right.completed_task_count.cmp(&left.completed_task_count))
            .then(left.target.label.cmp(&right.target.label))
    });
    state.recent_targets.truncate(MAX_RECENT_TASK_TARGETS);
}

fn has_related_kind(
    graph: &ProjectGraphState,
    start: &str,
    desired: ProjectGraphNodeKind,
    max_depth: usize,
) -> bool {
    let mut visited = BTreeSet::new();
    let mut frontier = vec![(start.to_string(), 0usize)];
    while let Some((id, depth)) = frontier.pop() {
        if !visited.insert(id.clone()) || depth > max_depth {
            continue;
        }
        if graph
            .nodes
            .iter()
            .any(|node| node.id == id && node.kind == desired)
        {
            return true;
        }
        for edge in &graph.edges {
            if edge.from == id {
                frontier.push((edge.to.clone(), depth + 1));
            } else if edge.to == id {
                frontier.push((edge.from.clone(), depth + 1));
            }
        }
    }
    false
}

fn proposal_steps(operation: &TaskOperation) -> Vec<String> {
    vec![
        "Повторно проверить Project Map и точную target binding".to_string(),
        format!("Подготовить операцию «{}»", operation.label),
        format!(
            "Выполнить через: {}",
            operation.recommended_tools.join(", ")
        ),
        "Проверить затронутые зависимости и артефакты".to_string(),
        format!("Валидация: {}", operation.validation.join("; ")),
        "Обновить Project Map и сохранить результат в истории запуска".to_string(),
    ]
}

fn improvement_suggestions(operation: &TaskOperation) -> Vec<ProposalSuggestion> {
    vec![
        ProposalSuggestion {
            id: "add_regression_test".to_string(),
            title: "Добавить regression-проверку".to_string(),
            detail: operation.validation.join("; "),
            selected: false,
        },
        ProposalSuggestion {
            id: "make_data_driven".to_string(),
            title: "Вынести параметры в Data Asset".to_string(),
            detail: "Упростит последующую настройку без изменения кода".to_string(),
            selected: false,
        },
        ProposalSuggestion {
            id: "document_relations".to_string(),
            title: "Зафиксировать новые связи Project Map".to_string(),
            detail: "Сохранить подтверждённые зависимости и provenance".to_string(),
            selected: false,
        },
    ]
}

fn efficiency_suggestions(operation: &TaskOperation) -> Vec<ProposalSuggestion> {
    vec![
        ProposalSuggestion {
            id: "reuse_compatible_assets".to_string(),
            title: "Переиспользовать совместимые ассеты".to_string(),
            detail: "Сначала проверить существующие target-compatible узлы".to_string(),
            selected: false,
        },
        ProposalSuggestion {
            id: "bounded_subagents".to_string(),
            title: "Распараллелить независимые проверки".to_string(),
            detail: format!(
                "Главный агент сохраняет TaskManifest; субагенты используют {}",
                operation.recommended_tools.join(", ")
            ),
            selected: false,
        },
    ]
}

fn parse_semantic_edge_kind(value: &str) -> Option<ProjectGraphEdgeKind> {
    use ProjectGraphEdgeKind as E;
    Some(match value {
        "uses_skeleton" => E::UsesSkeleton,
        "animates" => E::Animates,
        "controlled_by" => E::ControlledBy,
        "has_component" => E::HasComponent,
        "compatible_with" => E::CompatibleWith,
        "spawned_by" => E::SpawnedBy,
        "owned_by" => E::OwnedBy,
        "bound_to_input" => E::BoundToInput,
        "produces" => E::Produces,
        "consumes" => E::Consumes,
        "related_to" => E::RelatedTo,
        _ => return None,
    })
}

fn is_semantic_edge(kind: ProjectGraphEdgeKind) -> bool {
    matches!(
        kind,
        ProjectGraphEdgeKind::UsesSkeleton
            | ProjectGraphEdgeKind::Animates
            | ProjectGraphEdgeKind::ControlledBy
            | ProjectGraphEdgeKind::HasComponent
            | ProjectGraphEdgeKind::CompatibleWith
            | ProjectGraphEdgeKind::SpawnedBy
            | ProjectGraphEdgeKind::OwnedBy
            | ProjectGraphEdgeKind::BoundToInput
            | ProjectGraphEdgeKind::Produces
            | ProjectGraphEdgeKind::Consumes
    )
}

fn dependency_cycle_count(graph: &ProjectGraphState) -> usize {
    let adjacency = graph
        .edges
        .iter()
        .filter(|edge| edge.kind == ProjectGraphEdgeKind::DependsOn)
        .fold(BTreeMap::<String, Vec<String>>::new(), |mut map, edge| {
            map.entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            map
        });
    let mut cycles = BTreeSet::new();
    for start in adjacency.keys() {
        let mut stack = vec![(start.clone(), Vec::<String>::new())];
        while let Some((node, mut path)) = stack.pop() {
            if path.contains(&node) {
                path.push(node);
                path.sort();
                cycles.insert(path.join("|"));
                continue;
            }
            if path.len() > 24 {
                continue;
            }
            path.push(node.clone());
            if let Some(next) = adjacency.get(&node) {
                for child in next {
                    stack.push((child.clone(), path.clone()));
                }
            }
        }
    }
    cycles.len()
}

fn collect_project_references(value: &Value) -> Vec<String> {
    let mut references = Vec::new();
    match value {
        Value::String(value) => {
            if !value.trim().is_empty() {
                references.push(value.clone());
            }
        }
        Value::Array(values) => {
            for value in values {
                references.extend(collect_project_references(value));
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                references.extend(collect_project_references(value));
            }
        }
        _ => {}
    }
    references
}

fn is_manifest_guarded_action(action: &ToolAction) -> bool {
    matches!(
        action,
        ToolAction::WriteFile
            | ToolAction::EditFile
            | ToolAction::ApplyPatch
            | ToolAction::ProjectCommand
            | ToolAction::RunShell
            | ToolAction::TerminalStart
            | ToolAction::TerminalWrite
            | ToolAction::UnrealCommand
            | ToolAction::ApplyGameplayPlan
            | ToolAction::McpCall
            | ToolAction::Import3dAssetUnreal
            | ToolAction::DesktopStep
            | ToolAction::MouseClick
            | ToolAction::TypeText
            | ToolAction::Hotkey
            | ToolAction::GenerateImageAsset
            | ToolAction::GenerateSpritesheetAsset
            | ToolAction::GenerateAudioAsset
            | ToolAction::GenerateVideoAsset
            | ToolAction::RegenerateImageAsset
            | ToolAction::VaryImageAsset
            | ToolAction::UpscaleAsset
            | ToolAction::ExportAsset
            | ToolAction::UseAssetAsAppIcon
    )
}

fn mcp_server_count(workspace: &Workspace) -> usize {
    crate::mcp::registry_snapshot(workspace)
        .map(|snapshot| snapshot.servers.len())
        .unwrap_or_default()
}

fn compact(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn unreal_cli_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!("//{}", rest.replace('\\', "/"))
    } else {
        raw.strip_prefix(r"\\?\").unwrap_or(&raw).replace('\\', "/")
    }
}

fn asset_registry_signature(workspace: &Workspace) -> Option<(SystemTime, u64)> {
    let path = workspace
        .resolve_existing(GENERATED_ASSET_REGISTRY_PATH)
        .ok()?;
    let metadata = path.metadata().ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

fn unreal_scan_output_detail(output: &Output) -> String {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let important = combined
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("logpython")
                || lower.contains("executepythonscript")
                || lower.contains("error:")
                || lower.contains("fatal")
        })
        .take(12)
        .collect::<Vec<_>>()
        .join(" | ");
    if important.trim().is_empty() {
        "Подробная причина не попала в stdout; откройте Saved/Logs проекта.".to_string()
    } else {
        compact(&important, 1_200)
    }
}

fn default_true() -> bool {
    true
}

fn state_schema_version() -> u32 {
    STATE_SCHEMA_VERSION
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

    fn write_graph(
        workspace: &Workspace,
        nodes: Vec<ProjectGraphNode>,
        edges: Vec<ProjectGraphEdge>,
    ) {
        let mut graph = ProjectGraphState::default();
        graph.nodes = nodes;
        graph.edges = edges;
        save_project_graph(workspace, &graph).unwrap();
    }

    fn node(id: &str, label: &str, kind: ProjectGraphNodeKind) -> ProjectGraphNode {
        ProjectGraphNode {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            path: None,
            summary: String::new(),
            source: "test".to_string(),
            confidence: 1.0,
            metadata: BTreeMap::new(),
            updated_at: unix_timestamp(),
        }
    }

    #[test]
    fn catalog_has_eight_by_ten_by_ten_operations() {
        let catalog = game_task_catalog();
        assert_eq!(catalog.domains.len(), 8);
        for domain in &catalog.domains {
            assert_eq!(domain.directions.len(), 10, "{}", domain.id);
            for direction in &domain.directions {
                assert_eq!(direction.operations.len(), 10, "{}", direction.id);
            }
        }
        assert_eq!(
            catalog
                .domains
                .iter()
                .flat_map(|domain| &domain.directions)
                .flat_map(|direction| &direction.operations)
                .count(),
            800
        );
        assert_eq!(catalog.custom_option_label, "Свой вариант");
    }

    #[test]
    fn unreal_cli_path_removes_windows_verbatim_prefix() {
        assert_eq!(
            unreal_cli_path(Path::new(r"\\?\C:\Work\Game\script.py")),
            "C:/Work/Game/script.py"
        );
        assert_eq!(
            unreal_cli_path(Path::new(r"C:\Work\Game\script.py")),
            "C:/Work/Game/script.py"
        );
        assert_eq!(
            unreal_cli_path(Path::new(r"\\?\UNC\server\share\script.py")),
            "//server/share/script.py"
        );
    }

    #[test]
    fn project_map_health_explains_weighted_readiness() {
        let mut graph = ProjectGraphState::default();
        let mut external = node(
            "external:plugin",
            "MissingPluginAsset",
            ProjectGraphNodeKind::UnrealAsset,
        );
        external
            .metadata
            .insert("external".to_string(), "true".to_string());
        external.metadata.insert(
            "object_path".to_string(),
            "/Game/Missing/MissingPluginAsset".to_string(),
        );
        graph.nodes = vec![
            node("project:root", "Game", ProjectGraphNodeKind::Project),
            external,
        ];

        let health = graph_health(&graph, false);

        assert_eq!(health.coverage_percent, 40);
        assert_eq!(health.unresolved_nodes, 1);
        assert_eq!(health.coverage_areas.len(), 4);
        assert!(health
            .coverage_areas
            .iter()
            .any(|area| area.id == "project_structure" && area.ready));
        assert!(health
            .coverage_areas
            .iter()
            .any(|area| area.id == "asset_registry" && !area.ready));
    }

    #[test]
    fn project_input_diff_reports_added_modified_and_removed_paths() {
        let baseline = vec![
            UnrealProjectInput {
                path: "Source/Game/Hero.cpp".to_string(),
                size: 10,
                modified_ns: 1,
            },
            UnrealProjectInput {
                path: "Content/Old.uasset".to_string(),
                size: 20,
                modified_ns: 1,
            },
        ];
        let current = vec![
            UnrealProjectInput {
                path: "Source/Game/Hero.cpp".to_string(),
                size: 12,
                modified_ns: 2,
            },
            UnrealProjectInput {
                path: "Content/New.uasset".to_string(),
                size: 30,
                modified_ns: 3,
            },
        ];

        let changes = compare_project_inputs(Some(&baseline), &current);

        assert_eq!(changes.added_count, 1);
        assert_eq!(changes.modified_count, 1);
        assert_eq!(changes.removed_count, 1);
        assert_eq!(changes.total(), 3);
        assert_eq!(changes.added_paths, vec!["Content/New.uasset"]);
        assert_eq!(changes.modified_paths, vec!["Source/Game/Hero.cpp"]);
        assert_eq!(changes.removed_paths, vec!["Content/Old.uasset"]);
    }

    #[test]
    fn readiness_ignores_leetcode_state_but_reports_source_changes() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        workspace
            .write_text(
                "SampleGame.uproject",
                r#"{"FileVersion":3,"EngineAssociation":"5.8","Modules":[]}"#,
            )
            .unwrap();
        workspace
            .write_text("Source/SampleGame/Hero.cpp", "void Hero() {}")
            .unwrap();
        let graph = refresh_project_graph(&workspace);
        save_project_graph(&workspace, &graph).unwrap();

        workspace
            .write_text(
                "assets/generated/leetcode/game-task-builder/state.json",
                r#"{"last_scan":"changed"}"#,
            )
            .unwrap();
        let unchanged = project_map_readiness(&workspace, false);
        assert_ne!(unchanged.status, ProjectMapReadinessStatus::Stale);
        assert!(unchanged.changes.is_empty());

        workspace
            .write_text(
                "Source/SampleGame/Hero.cpp",
                "void Hero() { int changed = 1; }",
            )
            .unwrap();
        let changed = project_map_readiness(&workspace, false);
        assert_eq!(changed.status, ProjectMapReadinessStatus::Stale);
        assert_eq!(changed.changes.modified_count, 1);
        assert!(changed
            .changes
            .modified_paths
            .iter()
            .any(|path| path.ends_with("Hero.cpp")));
    }

    #[test]
    fn engine_and_plugin_dependencies_do_not_reduce_readiness() {
        let mut graph = ProjectGraphState::default();
        let mut engine_dependency = node(
            "external:enhanced-input",
            "EnhancedInput",
            ProjectGraphNodeKind::UnrealModule,
        );
        engine_dependency
            .metadata
            .insert("external".to_string(), "true".to_string());
        engine_dependency
            .metadata
            .insert("external_scope".to_string(), "engine_or_plugin".to_string());
        graph.nodes = vec![
            node("project:root", "Game", ProjectGraphNodeKind::Project),
            engine_dependency,
        ];

        let health = graph_health(&graph, false);

        assert_eq!(health.unresolved_nodes, 0);
        assert_eq!(health.external_dependency_nodes, 1);
        assert_eq!(health.coverage_percent, 50);
    }

    #[test]
    fn integrated_asset_registry_completes_data_readiness() {
        let mut graph = ProjectGraphState::default();
        let mut asset = node(
            "unreal:asset:/Game/Hero/BP_Hero.BP_Hero",
            "BP_Hero",
            ProjectGraphNodeKind::UnrealBlueprint,
        );
        asset
            .metadata
            .insert("asset_source".to_string(), "asset_registry".to_string());
        graph.nodes = vec![
            node("project:root", "Game", ProjectGraphNodeKind::Project),
            asset,
        ];

        assert!(graph_has_integrated_asset_registry(&graph));
        let health = graph_health(&graph, true);
        assert_eq!(health.coverage_percent, 100);
        assert!(health.asset_registry_integrated);
        assert!(health.coverage_areas.iter().all(|area| area.ready));
    }

    #[test]
    #[ignore = "requires LEETCODE_UE_VALIDATION_WORKSPACE with an existing Asset Registry export"]
    fn live_asset_registry_rebuild_reaches_complete_data_readiness() {
        let root = std::env::var_os("LEETCODE_UE_VALIDATION_WORKSPACE")
            .map(std::path::PathBuf::from)
            .expect("set LEETCODE_UE_VALIDATION_WORKSPACE");
        let workspace = Workspace::new(root).expect("live Unreal workspace");

        let graph = refresh_project_graph(&workspace);
        save_project_graph(&workspace, &graph).expect("save rebuilt Project Map");
        let report = project_map_readiness(&workspace, false);

        assert!(report.deep_scan_completed);
        assert!(report.health.asset_registry_integrated);
        assert_eq!(report.health.coverage_percent, 100);
    }

    #[test]
    fn character_animation_excludes_static_mesh_bucket() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        write_graph(
            &workspace,
            vec![
                node(
                    "character",
                    "BP_Hero",
                    ProjectGraphNodeKind::UnrealBlueprint,
                ),
                node(
                    "bucket",
                    "SM_Bucket",
                    ProjectGraphNodeKind::UnrealStaticMesh,
                ),
            ],
            Vec::new(),
        );
        let report = resolve_game_task_targets(
            &workspace,
            &ResolveGameTaskTargetsArgs {
                operation_id: "characters_animation.locomotion.create".to_string(),
                query: None,
                limit: None,
            },
        )
        .unwrap();
        assert!(report
            .candidates
            .iter()
            .any(|target| target.node_id == "character"));
        assert!(!report
            .candidates
            .iter()
            .any(|target| target.node_id == "bucket"));
        assert!(report
            .excluded
            .iter()
            .any(|target| target.node_id == "bucket"));
    }

    #[test]
    fn missing_skeleton_offers_guided_choices() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        write_graph(
            &workspace,
            vec![node(
                "character",
                "BP_Hero",
                ProjectGraphNodeKind::UnrealBlueprint,
            )],
            Vec::new(),
        );
        let report = evaluate_game_task_prerequisites(
            &workspace,
            &EvaluateGameTaskPrerequisitesArgs {
                operation_id: "characters_animation.locomotion.create".to_string(),
                target_node_ids: vec!["character".to_string()],
            },
        )
        .unwrap();
        let issue = report
            .issues
            .iter()
            .find(|issue| issue.id.starts_with("missing_skeleton"))
            .unwrap();
        assert!(issue
            .remediation
            .iter()
            .any(|option| option.id == "create_skeleton"));
        assert!(issue
            .remediation
            .iter()
            .any(|option| option.id == "custom_resolution"));
    }

    #[test]
    fn confirmed_manifest_rejects_stale_graph() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        write_graph(
            &workspace,
            vec![node("project:root", "Game", ProjectGraphNodeKind::Project)],
            Vec::new(),
        );
        let graph = load_project_graph(&workspace);
        let mut state = GameTaskBuilderState::default();
        let manifest = TaskManifest {
            id: "manifest".to_string(),
            session_id: "session".to_string(),
            operation_id: "engineering_quality_release.testing.validate".to_string(),
            understood_task: "test".to_string(),
            target_node_ids: vec!["project:root".to_string()],
            allowed_node_ids: vec!["project:root".to_string()],
            object_paths: Vec::new(),
            selected_improvement_ids: Vec::new(),
            selected_efficiency_ids: Vec::new(),
            graph_fingerprint: project_graph_fingerprint(&graph),
            confirmed_at: unix_timestamp(),
        };
        state.active_manifest_id = Some(manifest.id.clone());
        state.manifests.push(manifest);
        save_game_task_builder_state(&workspace, &state).unwrap();
        let mut changed = graph;
        changed.nodes.push(node(
            "bucket",
            "SM_Bucket",
            ProjectGraphNodeKind::UnrealStaticMesh,
        ));
        save_project_graph(&workspace, &changed).unwrap();
        assert!(validate_tool_action_against_active_manifest(
            &workspace,
            &ToolAction::WriteFile,
            &json!({"path":"Source/Game.cpp"})
        )
        .is_err());
    }

    #[test]
    fn multiple_characters_require_explicit_target_choice() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        write_graph(
            &workspace,
            vec![
                node("hero", "BP_Hero", ProjectGraphNodeKind::UnrealBlueprint),
                node("npc", "BP_Guard", ProjectGraphNodeKind::UnrealBlueprint),
            ],
            Vec::new(),
        );
        let report = resolve_game_task_targets(
            &workspace,
            &ResolveGameTaskTargetsArgs {
                operation_id: "characters_animation.character_setup.modify".to_string(),
                query: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(report.feasibility, TaskFeasibility::Ambiguous);
        assert_eq!(report.candidates.len(), 2);
    }

    #[test]
    fn unselected_improvements_do_not_enter_manifest_and_session_persists() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        write_graph(
            &workspace,
            vec![node("project:root", "Game", ProjectGraphNodeKind::Project)],
            Vec::new(),
        );
        let session = prepare_game_task_proposal(
            &workspace,
            PrepareGameTaskProposalArgs {
                operation_id: "engineering_quality_release.testing.validate".to_string(),
                target_node_ids: vec!["project:root".to_string()],
                remediation_ids: Vec::new(),
                custom_request: None,
            },
        )
        .unwrap();
        assert!(!session.proposal.as_ref().unwrap().improvements.is_empty());
        let restored = load_game_task_builder_state(&workspace);
        assert_eq!(
            restored.active_session_id.as_deref(),
            Some(session.id.as_str())
        );
        let manifest =
            confirm_game_task_proposal(&workspace, &session.id, Vec::new(), Vec::new()).unwrap();
        assert!(manifest.selected_improvement_ids.is_empty());
        assert!(manifest.selected_efficiency_ids.is_empty());
    }

    #[test]
    fn manifest_blocks_explicit_object_outside_confirmed_scope() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let mut hero = node("hero", "BP_Hero", ProjectGraphNodeKind::UnrealBlueprint);
        hero.metadata
            .insert("object_path".to_string(), "/Game/Hero.BP_Hero".to_string());
        let mut bucket = node(
            "bucket",
            "SM_Bucket",
            ProjectGraphNodeKind::UnrealStaticMesh,
        );
        bucket.metadata.insert(
            "object_path".to_string(),
            "/Game/Props/SM_Bucket.SM_Bucket".to_string(),
        );
        write_graph(&workspace, vec![hero, bucket], Vec::new());
        let graph = load_project_graph(&workspace);
        let manifest = TaskManifest {
            id: "manifest".to_string(),
            session_id: "session".to_string(),
            operation_id: "characters_animation.locomotion.modify".to_string(),
            understood_task: "Изменить Hero".to_string(),
            target_node_ids: vec!["hero".to_string()],
            allowed_node_ids: vec!["hero".to_string()],
            object_paths: vec!["/Game/Hero.BP_Hero".to_string()],
            selected_improvement_ids: Vec::new(),
            selected_efficiency_ids: Vec::new(),
            graph_fingerprint: project_graph_fingerprint(&graph),
            confirmed_at: unix_timestamp(),
        };
        let mut state = GameTaskBuilderState::default();
        state.active_manifest_id = Some(manifest.id.clone());
        state.manifests.push(manifest);
        save_game_task_builder_state(&workspace, &state).unwrap();
        let error = validate_tool_action_against_active_manifest(
            &workspace,
            &ToolAction::McpCall,
            &json!({"arguments":{"object_path":"/Game/Props/SM_Bucket.SM_Bucket"}}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("вне подтверждённого"));
    }

    #[test]
    fn completed_manifest_promotes_recent_target_and_deduplicates_history() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let mut player = node(
            "player",
            "BP_Character_Player",
            ProjectGraphNodeKind::UnrealBlueprint,
        );
        player.metadata.insert(
            "object_path".to_string(),
            "/Game/Characters/BP_Character_Player.BP_Character_Player".to_string(),
        );
        let hud = node("hud", "WBP_HUD", ProjectGraphNodeKind::UnrealWidget);
        write_graph(&workspace, vec![player, hud], Vec::new());
        let graph = load_project_graph(&workspace);

        for index in 1..=2 {
            let manifest = TaskManifest {
                id: format!("manifest-{index}"),
                session_id: format!("session-{index}"),
                operation_id: "ui_ux_accessibility.hud.modify".to_string(),
                understood_task: "Изменить HUD персонажа".to_string(),
                target_node_ids: vec!["player".to_string()],
                allowed_node_ids: vec!["player".to_string()],
                object_paths: vec![
                    "/Game/Characters/BP_Character_Player.BP_Character_Player".to_string()
                ],
                selected_improvement_ids: Vec::new(),
                selected_efficiency_ids: Vec::new(),
                graph_fingerprint: project_graph_fingerprint(&graph),
                confirmed_at: unix_timestamp(),
            };
            let mut state = load_game_task_builder_state(&workspace);
            state.active_manifest_id = Some(manifest.id.clone());
            state.manifests.push(manifest);
            save_game_task_builder_state(&workspace, &state).unwrap();
            finish_active_task_manifest(&workspace, "completed").unwrap();
        }

        let restored = load_game_task_builder_state(&workspace);
        assert_eq!(restored.recent_targets.len(), 1);
        assert_eq!(restored.recent_targets[0].target.node_id, "player");
        assert_eq!(restored.recent_targets[0].completed_task_count, 2);

        let report = resolve_game_task_targets(
            &workspace,
            &ResolveGameTaskTargetsArgs {
                operation_id: "ui_ux_accessibility.hud.modify".to_string(),
                query: None,
                limit: Some(40),
            },
        )
        .unwrap();
        assert_eq!(report.recent_candidates.len(), 1);
        assert_eq!(report.recent_candidates[0].target.node_id, "player");
        assert_eq!(report.recent_candidates[0].context_label, "та же операция");
        assert!(!report
            .candidates
            .iter()
            .any(|candidate| candidate.node_id == "player"));
    }

    #[test]
    fn failed_manifest_does_not_enter_recent_targets() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        write_graph(
            &workspace,
            vec![node(
                "player",
                "BP_Character_Player",
                ProjectGraphNodeKind::UnrealBlueprint,
            )],
            Vec::new(),
        );
        let graph = load_project_graph(&workspace);
        let manifest = TaskManifest {
            id: "failed-manifest".to_string(),
            session_id: "failed-session".to_string(),
            operation_id: "ui_ux_accessibility.hud.modify".to_string(),
            understood_task: "Изменить HUD персонажа".to_string(),
            target_node_ids: vec!["player".to_string()],
            allowed_node_ids: vec!["player".to_string()],
            object_paths: Vec::new(),
            selected_improvement_ids: Vec::new(),
            selected_efficiency_ids: Vec::new(),
            graph_fingerprint: project_graph_fingerprint(&graph),
            confirmed_at: unix_timestamp(),
        };
        let mut state = GameTaskBuilderState::default();
        state.active_manifest_id = Some(manifest.id.clone());
        state.manifests.push(manifest);
        save_game_task_builder_state(&workspace, &state).unwrap();

        finish_active_task_manifest(&workspace, "failed").unwrap();

        assert!(load_game_task_builder_state(&workspace)
            .recent_targets
            .is_empty());
    }

    #[test]
    fn read_only_operations_do_not_claim_recent_modifications() {
        assert!(operation_records_recent_targets(
            "ui_ux_accessibility.hud.modify"
        ));
        assert!(!operation_records_recent_targets(
            "ui_ux_accessibility.hud.validate"
        ));
        assert!(!operation_records_recent_targets(
            "ui_ux_accessibility.hud.document"
        ));
    }
}
