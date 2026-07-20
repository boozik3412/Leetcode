use crate::agent::types::{ToolAction, ToolResult};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;

const GOVERNANCE_PATH: &str = "assets/generated/leetcode/governance.json";

#[derive(Clone, Debug, Serialize)]
pub struct ToolSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub risk: &'static str,
    pub description: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceConfig {
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default)]
    pub disabled_categories: Vec<String>,
    #[serde(default)]
    pub shell_deny_patterns: Vec<String>,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            disabled_tools: Vec::new(),
            disabled_categories: Vec::new(),
            shell_deny_patterns: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct GovernanceDecision {
    pub allowed: bool,
    pub tool: String,
    pub category: String,
    pub risk: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct SetToolEnabledArgs {
    pub tool: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetCategoryEnabledArgs {
    pub category: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct AddShellDenyPatternArgs {
    pub pattern: String,
}

pub fn tool_specs() -> &'static [ToolSpec] {
    const SPECS: &[ToolSpec] = &[
        spec(
            "list_files",
            "files",
            "low",
            "Показать файлы рабочей папки.",
        ),
        spec("read_file", "files", "low", "Прочитать файл рабочей папки."),
        spec(
            "write_file",
            "files",
            "high",
            "Записать файл рабочей папки.",
        ),
        spec(
            "edit_file",
            "files",
            "high",
            "Отредактировать файл рабочей папки.",
        ),
        spec("apply_patch", "files", "high", "Применить git patch."),
        spec("grep", "files", "low", "Искать текст в рабочей папке."),
        spec(
            "project_command",
            "shell",
            "high",
            "Запустить обнаруженную проектную команду.",
        ),
        spec(
            "unreal_snapshot",
            "unreal",
            "low",
            "Обнаружить Unreal-проект, плагины, движок и toolchain.",
        ),
        spec(
            "unreal_command",
            "unreal",
            "high",
            "Запустить фиксированный безопасный профиль Unreal Engine.",
        ),
        spec(
            "game_production_snapshot",
            "game_production",
            "low",
            "Посмотреть активный game production plan, workstreams и gates.",
        ),
        spec(
            "create_game_production_plan",
            "game_production",
            "medium",
            "Создать persistent production plan разработки игры.",
        ),
        spec(
            "update_production_item",
            "game_production",
            "medium",
            "Обновить статус, артефакт и validation production item.",
        ),
        spec(
            "evaluate_production_gate",
            "game_production",
            "low",
            "Проверить готовность milestone по зависимостям и артефактам.",
        ),
        spec(
            "vertical_slice_snapshot",
            "game_production",
            "low",
            "Посмотреть orchestration runs, фазы и рекомендуемые инструменты.",
        ),
        spec(
            "start_vertical_slice_run",
            "game_production",
            "medium",
            "Запустить persistent orchestration вертикального среза.",
        ),
        spec(
            "advance_vertical_slice_phase",
            "game_production",
            "medium",
            "Обновить доказательный статус фазы вертикального среза.",
        ),
        spec(
            "evaluate_vertical_slice_readiness",
            "game_production",
            "low",
            "Проверить готовность вертикального среза по live-состоянию пайплайнов.",
        ),
        spec(
            "gameplay_snapshot",
            "unreal",
            "low",
            "Посмотреть gameplay-планы, применения и playtest-запуски Unreal.",
        ),
        spec(
            "create_gameplay_plan",
            "unreal",
            "medium",
            "Создать связанный с задачами и Project Map gameplay-план Unreal.",
        ),
        spec(
            "apply_gameplay_plan",
            "unreal",
            "high",
            "Применить декларативный manifest к уровню Unreal через Python API.",
        ),
        spec(
            "run_gameplay_playtest",
            "unreal",
            "high",
            "Запустить Automation или map smoke playtest и собрать артефакты.",
        ),
        spec(
            "mcp_snapshot",
            "mcp",
            "low",
            "Показать локальный реестр MCP-серверов и последнее состояние подключений.",
        ),
        spec(
            "mcp_discover",
            "mcp",
            "medium",
            "Подключиться к разрешённому MCP-серверу и прочитать список инструментов.",
        ),
        spec(
            "mcp_call",
            "mcp",
            "high",
            "Вызвать разрешённый инструмент внешнего MCP-сервера с подтверждением.",
        ),
        spec(
            "run_shell",
            "shell",
            "high",
            "Запустить произвольную shell-команду.",
        ),
        spec(
            "terminal_start",
            "terminal",
            "medium",
            "Запустить постоянную shell-сессию.",
        ),
        spec(
            "terminal_write",
            "terminal",
            "high",
            "Отправить ввод в постоянную shell-сессию.",
        ),
        spec(
            "terminal_read",
            "terminal",
            "low",
            "Прочитать вывод постоянной shell-сессии.",
        ),
        spec(
            "terminal_stop",
            "terminal",
            "medium",
            "Остановить постоянную shell-сессию.",
        ),
        spec(
            "terminal_clear",
            "terminal",
            "low",
            "Очистить историю вывода терминала.",
        ),
        spec(
            "game_workflow",
            "planning",
            "medium",
            "Создать документы игрового/app-сценария.",
        ),
        spec(
            "open_project_preview",
            "external",
            "medium",
            "Открыть локальный preview URL.",
        ),
        spec(
            "run_subagent",
            "orchestration",
            "medium",
            "Запустить ограниченного агента-специалиста.",
        ),
        spec(
            "delegate_agent",
            "orchestration",
            "medium",
            "Записать передачу агенту-специалисту.",
        ),
        spec(
            "update_workspace_context",
            "orchestration",
            "medium",
            "Обновить общий контекст.",
        ),
        spec(
            "record_run_summary",
            "orchestration",
            "low",
            "Записать итог запуска.",
        ),
        spec(
            "export_trace",
            "orchestration",
            "low",
            "Экспортировать трассу оркестрации.",
        ),
        spec(
            "create_replay_eval",
            "evals",
            "medium",
            "Создать replay-проверку.",
        ),
        spec(
            "run_replay_eval",
            "evals",
            "medium",
            "Запустить локальную replay-проверку.",
        ),
        spec(
            "eval_snapshot",
            "evals",
            "low",
            "Посмотреть кейсы и результаты проверок.",
        ),
        spec(
            "orchestration_snapshot",
            "orchestration",
            "low",
            "Посмотреть состояние оркестрации.",
        ),
        spec(
            "generate_image_asset",
            "assets",
            "paid",
            "Сгенерировать изображение.",
        ),
        spec(
            "generate_spritesheet_asset",
            "assets",
            "paid",
            "Сгенерировать спрайт-лист.",
        ),
        spec(
            "generate_audio_asset",
            "assets",
            "paid",
            "Сгенерировать аудио.",
        ),
        spec(
            "generate_video_asset",
            "assets",
            "paid",
            "Сгенерировать видео.",
        ),
        spec(
            "asset_3d_snapshot",
            "assets3d",
            "low",
            "Inspect persistent 3D provider jobs and validation results.",
        ),
        spec(
            "submit_3d_asset",
            "assets3d",
            "paid",
            "Submit a text/image-to-3D provider job.",
        ),
        spec(
            "refresh_3d_asset",
            "assets3d",
            "medium",
            "Refresh a provider job and download a completed 3D model.",
        ),
        spec(
            "validate_3d_asset",
            "assets3d",
            "low",
            "Validate geometry, scale, UV/PBR, rig, animation and provenance.",
        ),
        spec(
            "import_3d_asset_unreal",
            "unreal",
            "high",
            "Import or reimport a validated 3D asset through Unreal Python/Interchange.",
        ),
        spec(
            "regenerate_image_asset",
            "assets",
            "paid",
            "Повторить задачу изображения.",
        ),
        spec(
            "vary_image_asset",
            "assets",
            "paid",
            "Создать вариацию изображения.",
        ),
        spec(
            "upscale_asset",
            "assets",
            "medium",
            "Увеличить существующий ассет.",
        ),
        spec("export_asset", "assets", "medium", "Экспортировать ассет."),
        spec(
            "attach_asset",
            "assets",
            "low",
            "Прикрепить контекст ассета.",
        ),
        spec(
            "use_asset_as_app_icon",
            "assets",
            "high",
            "Записать иконку приложения.",
        ),
        spec(
            "open_asset_folder",
            "external",
            "medium",
            "Открыть папку ассета.",
        ),
        spec(
            "asset_library_snapshot",
            "assets",
            "low",
            "Посмотреть библиотеку ассетов.",
        ),
        spec("tag_asset", "assets", "medium", "Обновить теги ассета."),
        spec(
            "favorite_asset",
            "assets",
            "medium",
            "Отметить ассет избранным.",
        ),
        spec(
            "export_asset_pack",
            "assets",
            "medium",
            "Экспортировать набор ассетов.",
        ),
        spec(
            "screenshot",
            "desktop",
            "medium",
            "Сделать скриншот рабочего стола.",
        ),
        spec(
            "active_window",
            "desktop",
            "low",
            "Посмотреть активное окно рабочего стола.",
        ),
        spec(
            "focus_window",
            "desktop",
            "medium",
            "Сфокусировать окно рабочего стола.",
        ),
        spec(
            "desktop_step",
            "desktop",
            "high",
            "Наблюдать и действовать на рабочем столе.",
        ),
        spec(
            "mouse_click",
            "desktop",
            "high",
            "Кликнуть по рабочему столу.",
        ),
        spec(
            "type_text",
            "desktop",
            "high",
            "Ввести текст в активное desktop-приложение.",
        ),
        spec(
            "hotkey",
            "desktop",
            "high",
            "Отправить hotkey рабочего стола.",
        ),
        spec(
            "governance_snapshot",
            "governance",
            "low",
            "Посмотреть правила доступа.",
        ),
        spec(
            "set_tool_enabled",
            "governance",
            "high",
            "Включить или отключить один инструмент.",
        ),
        spec(
            "set_category_enabled",
            "governance",
            "high",
            "Включить или отключить категорию.",
        ),
        spec(
            "add_shell_deny_pattern",
            "governance",
            "high",
            "Добавить shell-запрет.",
        ),
        spec(
            "memory_snapshot",
            "memory",
            "low",
            "Посмотреть память проекта.",
        ),
        spec(
            "upsert_task",
            "memory",
            "medium",
            "Создать или обновить задачу.",
        ),
        spec(
            "update_task_status",
            "memory",
            "medium",
            "Обновить статус задачи.",
        ),
        spec(
            "record_decision",
            "memory",
            "medium",
            "Записать решение проекта.",
        ),
        spec(
            "record_project_goal",
            "memory",
            "medium",
            "Записать цель проекта.",
        ),
        spec(
            "record_memory_source",
            "memory",
            "medium",
            "Сохранить источник или заметку в память проекта.",
        ),
        spec(
            "remove_memory_source",
            "memory",
            "medium",
            "Удалить источник из памяти проекта.",
        ),
        spec(
            "project_graph_snapshot",
            "project_graph",
            "low",
            "Посмотреть машинную карту проекта: узлы, связи, команды, память и roadmap.",
        ),
        spec(
            "project_map_readiness",
            "game_task_builder",
            "low",
            "Проверить готовность и полноту Project Map для игровой задачи.",
        ),
        spec(
            "refresh_project_map_deep",
            "game_task_builder",
            "high",
            "Запустить headless Unreal scan и обновить Project Map.",
        ),
        spec(
            "game_task_catalog_snapshot",
            "game_task_builder",
            "low",
            "Посмотреть проектно-зависимый каталог игровых задач.",
        ),
        spec(
            "resolve_game_task_targets",
            "game_task_builder",
            "low",
            "Найти совместимые цели операции только в актуальной Project Map.",
        ),
        spec(
            "evaluate_game_task_prerequisites",
            "game_task_builder",
            "low",
            "Диагностировать зависимости и предложить варианты подготовки.",
        ),
        spec(
            "prepare_game_task_proposal",
            "game_task_builder",
            "medium",
            "Подготовить структурированный план до подтверждения пользователем.",
        ),
        spec(
            "propose_project_relation",
            "game_task_builder",
            "medium",
            "Предложить новую семантическую связь Project Map для отдельного подтверждения.",
        ),
        spec(
            "game_task_snapshot",
            "game_task_builder",
            "low",
            "Посмотреть текущую сессию конструктора и подтверждённый TaskManifest.",
        ),
        spec(
            "roadmap_snapshot",
            "roadmap",
            "low",
            "Посмотреть живую дорожную карту проекта.",
        ),
        spec(
            "record_milestone",
            "roadmap",
            "medium",
            "Зафиксировать milestone дорожной карты.",
        ),
        spec(
            "update_roadmap_item",
            "roadmap",
            "medium",
            "Обновить пункт дорожной карты.",
        ),
        spec(
            "plan_roadmap_item",
            "roadmap",
            "medium",
            "Запланировать новый пункт дорожной карты.",
        ),
        spec(
            "export_roadmap",
            "roadmap",
            "medium",
            "Экспортировать дорожную карту в markdown.",
        ),
        spec(
            "provider_health_snapshot",
            "providers",
            "low",
            "Посмотреть состояние провайдеров.",
        ),
        spec(
            "environment_snapshot",
            "diagnostics",
            "low",
            "Посмотреть диагностику окружения, путей, proxy и toolchain.",
        ),
        spec(
            "production_validation_snapshot",
            "release",
            "low",
            "Собрать единый отчёт production readiness проекта.",
        ),
        spec(
            "update_project_map_golden",
            "project",
            "medium",
            "Зафиксировать текущую структуру Project Map как эталон.",
        ),
        spec(
            "visual_regression_snapshot",
            "visual",
            "low",
            "Посмотреть эталоны и результаты визуальной регрессии.",
        ),
        spec(
            "record_visual_baseline",
            "visual",
            "medium",
            "Сохранить проверенный снимок интерфейса как визуальный эталон.",
        ),
        spec(
            "compare_visual_snapshot",
            "visual",
            "low",
            "Сравнить снимок интерфейса с сохранённым эталоном.",
        ),
        spec(
            "self_improvement_snapshot",
            "self_improvement",
            "low",
            "Посмотреть гипотезы, проверки и решения экспериментов самоулучшения.",
        ),
        spec(
            "start_self_improvement_experiment",
            "self_improvement",
            "medium",
            "Записать гипотезу, baseline и критерии успеха эксперимента.",
        ),
        spec(
            "decide_self_improvement_experiment",
            "self_improvement",
            "high",
            "Принять или отклонить проверенный эксперимент самоулучшения.",
        ),
        spec(
            "prepare_self_improvement_worktree",
            "self_improvement",
            "high",
            "Создать изолированную Git-ветку и worktree кандидата.",
        ),
        spec(
            "apply_self_improvement_patch",
            "self_improvement",
            "high",
            "Применить patch только внутри worktree эксперимента.",
        ),
        spec(
            "register_self_improvement_benchmark",
            "self_improvement",
            "high",
            "Добавить исполняемый benchmark для baseline/candidate проверки.",
        ),
        spec(
            "run_self_improvement_benchmarks",
            "self_improvement",
            "high",
            "Выполнить benchmarks в основной и кандидатной рабочих копиях.",
        ),
        spec(
            "promote_self_improvement_experiment",
            "self_improvement",
            "critical",
            "Закоммитить и fast-forward продвинуть принятый кандидат.",
        ),
        spec(
            "rollback_self_improvement_experiment",
            "self_improvement",
            "critical",
            "Создать revert-коммит для продвинутого эксперимента.",
        ),
        spec(
            "cleanup_self_improvement_experiment",
            "self_improvement",
            "high",
            "Удалить управляемый worktree и экспериментальную ветку.",
        ),
    ];
    SPECS
}

const fn spec(
    id: &'static str,
    category: &'static str,
    risk: &'static str,
    description: &'static str,
) -> ToolSpec {
    ToolSpec {
        id,
        category,
        risk,
        description,
    }
}

pub fn load_governance(workspace: &Workspace) -> GovernanceConfig {
    workspace
        .read_text(GOVERNANCE_PATH, 500_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_governance(workspace: &Workspace, config: &GovernanceConfig) -> anyhow::Result<()> {
    workspace.write_text(GOVERNANCE_PATH, &serde_json::to_string_pretty(config)?)
}

pub fn evaluate_action(
    workspace: Option<&Workspace>,
    action: &ToolAction,
    args: &serde_json::Value,
) -> GovernanceDecision {
    let tool = action_id(action);
    let spec = spec_for_tool(tool);
    let category = spec.map(|spec| spec.category).unwrap_or("unknown");
    let risk = spec.map(|spec| spec.risk).unwrap_or("unknown");
    let Some(workspace) = workspace else {
        return decision(
            true,
            tool,
            category,
            risk,
            "правила доступа рабочей папки не настроены",
        );
    };
    let config = load_governance(workspace);

    if contains_id(&config.disabled_categories, category) {
        return decision(
            false,
            tool,
            category,
            risk,
            "категория инструмента отключена правилами доступа",
        );
    }
    if contains_id(&config.disabled_tools, tool) {
        return decision(
            false,
            tool,
            category,
            risk,
            "инструмент отключён правилами доступа",
        );
    }

    if matches!(tool, "run_shell" | "terminal_write" | "project_command") {
        let text = shell_text_for(tool, args).to_ascii_lowercase();
        for pattern in &config.shell_deny_patterns {
            let pattern = pattern.trim().to_ascii_lowercase();
            if !pattern.is_empty() && text.contains(&pattern) {
                return decision(false, tool, category, risk, "совпадение с shell-запретом");
            }
        }
    }

    decision(true, tool, category, risk, "разрешено")
}

pub fn governance_snapshot(workspace: &Workspace) -> ToolResult {
    let config = load_governance(workspace);
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "config": config,
            "tools": tool_specs()
        }))
        .unwrap_or_else(|_| "снимок правил доступа".to_string()),
    )
}

pub fn set_tool_enabled(workspace: &Workspace, args: SetToolEnabledArgs) -> ToolResult {
    let tool = args.tool.trim();
    if spec_for_tool(tool).is_none() {
        return ToolResult::error(format!("неизвестный инструмент: {tool}"));
    }
    let mut config = load_governance(workspace);
    set_list_value(&mut config.disabled_tools, tool, !args.enabled);
    match save_governance(workspace, &config) {
        Ok(()) => ToolResult::ok(format!("{tool} включён: {}", yes_no_ru(args.enabled))),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn set_category_enabled(workspace: &Workspace, args: SetCategoryEnabledArgs) -> ToolResult {
    let category = args.category.trim().to_ascii_lowercase();
    if !tool_specs().iter().any(|spec| spec.category == category) {
        return ToolResult::error(format!("неизвестная категория: {category}"));
    }
    let mut config = load_governance(workspace);
    set_list_value(&mut config.disabled_categories, &category, !args.enabled);
    match save_governance(workspace, &config) {
        Ok(()) => ToolResult::ok(format!("{category} включена: {}", yes_no_ru(args.enabled))),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn add_shell_deny_pattern(workspace: &Workspace, args: AddShellDenyPatternArgs) -> ToolResult {
    let pattern = args.pattern.trim();
    if pattern.is_empty() {
        return ToolResult::error("паттерн пустой");
    }
    let mut config = load_governance(workspace);
    if !contains_id(&config.shell_deny_patterns, pattern) {
        config.shell_deny_patterns.push(pattern.to_string());
    }
    match save_governance(workspace, &config) {
        Ok(()) => ToolResult::ok(format!("shell-запрет добавлен: {pattern}")),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn action_id(action: &ToolAction) -> &'static str {
    match action {
        ToolAction::ListFiles => "list_files",
        ToolAction::ReadFile => "read_file",
        ToolAction::WriteFile => "write_file",
        ToolAction::EditFile => "edit_file",
        ToolAction::ApplyPatch => "apply_patch",
        ToolAction::Grep => "grep",
        ToolAction::ProjectCommand => "project_command",
        ToolAction::UnrealSnapshot => "unreal_snapshot",
        ToolAction::UnrealCommand => "unreal_command",
        ToolAction::GameProductionSnapshot => "game_production_snapshot",
        ToolAction::CreateGameProductionPlan => "create_game_production_plan",
        ToolAction::UpdateProductionItem => "update_production_item",
        ToolAction::EvaluateProductionGate => "evaluate_production_gate",
        ToolAction::VerticalSliceSnapshot => "vertical_slice_snapshot",
        ToolAction::StartVerticalSliceRun => "start_vertical_slice_run",
        ToolAction::AdvanceVerticalSlicePhase => "advance_vertical_slice_phase",
        ToolAction::EvaluateVerticalSliceReadiness => "evaluate_vertical_slice_readiness",
        ToolAction::GameplaySnapshot => "gameplay_snapshot",
        ToolAction::CreateGameplayPlan => "create_gameplay_plan",
        ToolAction::ApplyGameplayPlan => "apply_gameplay_plan",
        ToolAction::RunGameplayPlaytest => "run_gameplay_playtest",
        ToolAction::McpSnapshot => "mcp_snapshot",
        ToolAction::McpDiscover => "mcp_discover",
        ToolAction::McpCall => "mcp_call",
        ToolAction::GameWorkflow => "game_workflow",
        ToolAction::OpenProjectPreview => "open_project_preview",
        ToolAction::RunSubagent => "run_subagent",
        ToolAction::DelegateAgent => "delegate_agent",
        ToolAction::UpdateWorkspaceContext => "update_workspace_context",
        ToolAction::RecordRunSummary => "record_run_summary",
        ToolAction::ExportTrace => "export_trace",
        ToolAction::CreateReplayEval => "create_replay_eval",
        ToolAction::OrchestrationSnapshot => "orchestration_snapshot",
        ToolAction::RunShell => "run_shell",
        ToolAction::TerminalStart => "terminal_start",
        ToolAction::TerminalWrite => "terminal_write",
        ToolAction::TerminalRead => "terminal_read",
        ToolAction::TerminalStop => "terminal_stop",
        ToolAction::TerminalClear => "terminal_clear",
        ToolAction::GenerateImageAsset => "generate_image_asset",
        ToolAction::GenerateSpritesheetAsset => "generate_spritesheet_asset",
        ToolAction::GenerateAudioAsset => "generate_audio_asset",
        ToolAction::GenerateVideoAsset => "generate_video_asset",
        ToolAction::Asset3dSnapshot => "asset_3d_snapshot",
        ToolAction::Submit3dAsset => "submit_3d_asset",
        ToolAction::Refresh3dAsset => "refresh_3d_asset",
        ToolAction::Validate3dAsset => "validate_3d_asset",
        ToolAction::Import3dAssetUnreal => "import_3d_asset_unreal",
        ToolAction::RegenerateImageAsset => "regenerate_image_asset",
        ToolAction::VaryImageAsset => "vary_image_asset",
        ToolAction::UpscaleAsset => "upscale_asset",
        ToolAction::ExportAsset => "export_asset",
        ToolAction::AttachAsset => "attach_asset",
        ToolAction::UseAssetAsAppIcon => "use_asset_as_app_icon",
        ToolAction::OpenAssetFolder => "open_asset_folder",
        ToolAction::Screenshot => "screenshot",
        ToolAction::ActiveWindow => "active_window",
        ToolAction::FocusWindow => "focus_window",
        ToolAction::DesktopStep => "desktop_step",
        ToolAction::MouseClick => "mouse_click",
        ToolAction::TypeText => "type_text",
        ToolAction::Hotkey => "hotkey",
        ToolAction::GovernanceSnapshot => "governance_snapshot",
        ToolAction::SetToolEnabled => "set_tool_enabled",
        ToolAction::SetCategoryEnabled => "set_category_enabled",
        ToolAction::AddShellDenyPattern => "add_shell_deny_pattern",
        ToolAction::MemorySnapshot => "memory_snapshot",
        ToolAction::UpsertTask => "upsert_task",
        ToolAction::UpdateTaskStatus => "update_task_status",
        ToolAction::RecordDecision => "record_decision",
        ToolAction::RecordProjectGoal => "record_project_goal",
        ToolAction::RecordMemorySource => "record_memory_source",
        ToolAction::RemoveMemorySource => "remove_memory_source",
        ToolAction::ProjectGraphSnapshot => "project_graph_snapshot",
        ToolAction::ProjectMapReadiness => "project_map_readiness",
        ToolAction::RefreshProjectMapDeep => "refresh_project_map_deep",
        ToolAction::GameTaskCatalogSnapshot => "game_task_catalog_snapshot",
        ToolAction::ResolveGameTaskTargets => "resolve_game_task_targets",
        ToolAction::EvaluateGameTaskPrerequisites => "evaluate_game_task_prerequisites",
        ToolAction::PrepareGameTaskProposal => "prepare_game_task_proposal",
        ToolAction::ProposeProjectRelation => "propose_project_relation",
        ToolAction::GameTaskSnapshot => "game_task_snapshot",
        ToolAction::RoadmapSnapshot => "roadmap_snapshot",
        ToolAction::RecordMilestone => "record_milestone",
        ToolAction::UpdateRoadmapItem => "update_roadmap_item",
        ToolAction::PlanRoadmapItem => "plan_roadmap_item",
        ToolAction::ExportRoadmap => "export_roadmap",
        ToolAction::AssetLibrarySnapshot => "asset_library_snapshot",
        ToolAction::TagAsset => "tag_asset",
        ToolAction::FavoriteAsset => "favorite_asset",
        ToolAction::ExportAssetPack => "export_asset_pack",
        ToolAction::RunReplayEval => "run_replay_eval",
        ToolAction::EvalSnapshot => "eval_snapshot",
        ToolAction::SelfImprovementSnapshot => "self_improvement_snapshot",
        ToolAction::StartSelfImprovementExperiment => "start_self_improvement_experiment",
        ToolAction::DecideSelfImprovementExperiment => "decide_self_improvement_experiment",
        ToolAction::PrepareSelfImprovementWorktree => "prepare_self_improvement_worktree",
        ToolAction::ApplySelfImprovementPatch => "apply_self_improvement_patch",
        ToolAction::RegisterSelfImprovementBenchmark => "register_self_improvement_benchmark",
        ToolAction::RunSelfImprovementBenchmarks => "run_self_improvement_benchmarks",
        ToolAction::PromoteSelfImprovementExperiment => "promote_self_improvement_experiment",
        ToolAction::RollbackSelfImprovementExperiment => "rollback_self_improvement_experiment",
        ToolAction::CleanupSelfImprovementExperiment => "cleanup_self_improvement_experiment",
        ToolAction::ProviderHealthSnapshot => "provider_health_snapshot",
        ToolAction::EnvironmentSnapshot => "environment_snapshot",
        ToolAction::ProductionValidationSnapshot => "production_validation_snapshot",
        ToolAction::UpdateProjectMapGolden => "update_project_map_golden",
        ToolAction::VisualRegressionSnapshot => "visual_regression_snapshot",
        ToolAction::RecordVisualBaseline => "record_visual_baseline",
        ToolAction::CompareVisualSnapshot => "compare_visual_snapshot",
    }
}

pub fn spec_for_tool(tool: &str) -> Option<&'static ToolSpec> {
    tool_specs().iter().find(|spec| spec.id == tool)
}

fn decision(
    allowed: bool,
    tool: &str,
    category: &str,
    risk: &str,
    reason: &str,
) -> GovernanceDecision {
    GovernanceDecision {
        allowed,
        tool: tool.to_string(),
        category: category.to_string(),
        risk: risk.to_string(),
        reason: reason.to_string(),
    }
}

fn contains_id(values: &[String], value: &str) -> bool {
    values.iter().any(|known| known.eq_ignore_ascii_case(value))
}

fn set_list_value(values: &mut Vec<String>, value: &str, present: bool) {
    if present {
        if !contains_id(values, value) {
            values.push(value.to_string());
        }
    } else {
        values.retain(|known| !known.eq_ignore_ascii_case(value));
    }
}

fn yes_no_ru(value: bool) -> &'static str {
    if value {
        "да"
    } else {
        "нет"
    }
}

fn shell_text_for(tool: &str, args: &serde_json::Value) -> String {
    match tool {
        "run_shell" => args
            .get("cmd")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        "terminal_write" => args
            .get("input")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        "project_command" => args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disables_tool_by_id() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        set_tool_enabled(
            &workspace,
            SetToolEnabledArgs {
                tool: "run_shell".to_string(),
                enabled: false,
            },
        );

        let decision = evaluate_action(
            Some(&workspace),
            &ToolAction::RunShell,
            &json!({"cmd": "cargo check"}),
        );

        assert!(!decision.allowed);
    }

    #[test]
    fn self_improvement_tools_have_explicit_risk_class() {
        assert_eq!(
            spec_for_tool("self_improvement_snapshot")
                .expect("snapshot spec")
                .risk,
            "low"
        );
        assert_eq!(
            spec_for_tool("decide_self_improvement_experiment")
                .expect("decision spec")
                .risk,
            "high"
        );
    }

    #[test]
    fn mcp_tools_have_explicit_category_and_risk() {
        let snapshot = spec_for_tool("mcp_snapshot").expect("MCP snapshot spec");
        let call = spec_for_tool("mcp_call").expect("MCP call spec");
        assert_eq!(snapshot.category, "mcp");
        assert_eq!(snapshot.risk, "low");
        assert_eq!(call.category, "mcp");
        assert_eq!(call.risk, "high");
    }

    #[test]
    fn gameplay_tools_have_explicit_unreal_risk_classes() {
        for (tool, risk) in [
            ("gameplay_snapshot", "low"),
            ("create_gameplay_plan", "medium"),
            ("apply_gameplay_plan", "high"),
            ("run_gameplay_playtest", "high"),
        ] {
            let spec = spec_for_tool(tool).expect("gameplay tool spec");
            assert_eq!(spec.category, "unreal");
            assert_eq!(spec.risk, risk);
        }
    }

    #[test]
    fn game_production_tools_have_explicit_risk_classes() {
        for (tool, risk) in [
            ("game_production_snapshot", "low"),
            ("create_game_production_plan", "medium"),
            ("update_production_item", "medium"),
            ("evaluate_production_gate", "low"),
        ] {
            let spec = spec_for_tool(tool).expect("game production tool spec");
            assert_eq!(spec.category, "game_production");
            assert_eq!(spec.risk, risk);
        }
    }

    #[test]
    fn vertical_slice_tools_have_explicit_risk_classes() {
        for (tool, risk) in [
            ("vertical_slice_snapshot", "low"),
            ("start_vertical_slice_run", "medium"),
            ("advance_vertical_slice_phase", "medium"),
            ("evaluate_vertical_slice_readiness", "low"),
        ] {
            let spec = spec_for_tool(tool).expect("vertical slice tool spec");
            assert_eq!(spec.category, "game_production");
            assert_eq!(spec.risk, risk);
        }
    }

    #[test]
    fn production_validation_tools_have_explicit_risk_classes() {
        for (tool, category, risk) in [
            ("production_validation_snapshot", "release", "low"),
            ("update_project_map_golden", "project", "medium"),
            ("visual_regression_snapshot", "visual", "low"),
            ("record_visual_baseline", "visual", "medium"),
            ("compare_visual_snapshot", "visual", "low"),
        ] {
            let spec = spec_for_tool(tool).expect("production validation tool spec");
            assert_eq!(spec.category, category);
            assert_eq!(spec.risk, risk);
        }
    }

    #[test]
    fn game_task_builder_tools_have_explicit_governance() {
        for (tool, risk) in [
            ("project_map_readiness", "low"),
            ("refresh_project_map_deep", "high"),
            ("game_task_catalog_snapshot", "low"),
            ("resolve_game_task_targets", "low"),
            ("evaluate_game_task_prerequisites", "low"),
            ("prepare_game_task_proposal", "medium"),
            ("propose_project_relation", "medium"),
            ("game_task_snapshot", "low"),
        ] {
            let spec = spec_for_tool(tool).expect("game task builder spec");
            assert_eq!(spec.category, "game_task_builder");
            assert_eq!(spec.risk, risk);
        }
    }
}
