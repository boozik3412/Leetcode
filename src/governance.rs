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
        ToolAction::ProviderHealthSnapshot => "provider_health_snapshot",
        ToolAction::EnvironmentSnapshot => "environment_snapshot",
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
}
