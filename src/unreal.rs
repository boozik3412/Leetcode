use crate::agent::types::ToolResult;
use crate::workspace::Workspace;
use anyhow::Context;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub const PYTHON_EDITOR_PLUGIN_ID: &str = "PythonScriptPlugin";
pub const EDITOR_SCRIPTING_PLUGIN_ID: &str = "EditorScriptingUtilities";
pub const UNREAL_MCP_PLUGIN_ID: &str = "ModelContextProtocol";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnrealSetupStatus {
    Ready,
    NeedsProject,
    NeedsEngine,
    NeedsTools,
    NeedsPlugins,
}

impl UnrealSetupStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "готово к анализу",
            Self::NeedsProject => "нужно выбрать Unreal-проект",
            Self::NeedsEngine => "не найден Unreal Engine 5.8",
            Self::NeedsTools => "неполная установка Unreal",
            Self::NeedsPlugins => "нужно включить плагины проекта",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealSetupPluginStatus {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub enabled: bool,
    pub required_for_deep_scan: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealSetupReport {
    pub status: UnrealSetupStatus,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub engine_version: Option<String>,
    pub engine_root: Option<String>,
    pub plugins: Vec<UnrealSetupPluginStatus>,
    pub missing_toolchain: Vec<String>,
    pub mcp_profile_configured: bool,
    pub can_deep_scan: bool,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealPluginEnableResult {
    pub changed_plugins: Vec<String>,
    pub backup_path: String,
    pub report: UnrealSetupReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealSnapshot {
    pub workspace_root: String,
    pub project: Option<UnrealProjectDescriptor>,
    pub local_plugins: Vec<UnrealPluginDescriptor>,
    pub installations: Vec<UnrealInstallation>,
    pub selected_engine: Option<UnrealInstallation>,
    pub toolchain: Vec<UnrealToolchainComponent>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealProjectDescriptor {
    pub path: String,
    pub name: String,
    pub engine_association: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub modules: Vec<UnrealModuleDescriptor>,
    pub plugins: Vec<UnrealPluginReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealModuleDescriptor {
    pub name: String,
    pub module_type: String,
    pub loading_phase: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealPluginReference {
    pub name: String,
    pub enabled: bool,
    pub target_allow_list: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealPluginDescriptor {
    pub path: String,
    pub name: String,
    pub friendly_name: Option<String>,
    pub version_name: Option<String>,
    pub enabled_by_default: Option<bool>,
    pub modules: Vec<UnrealModuleDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealInstallation {
    pub root: String,
    pub version: Option<String>,
    pub branch: Option<String>,
    pub associations: Vec<String>,
    pub source: String,
    pub tools: UnrealToolPaths,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UnrealToolPaths {
    pub editor: Option<String>,
    pub editor_cmd: Option<String>,
    pub build: Option<String>,
    pub uat: Option<String>,
    pub ubt: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealToolchainComponent {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnrealCommandKind {
    GenerateProjectFiles,
    BuildEditor,
    OpenEditor,
    AutomationTests,
    Cook,
    Package,
    Validate,
    BuildPlugin,
}

impl UnrealCommandKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::GenerateProjectFiles => "generate_project_files",
            Self::BuildEditor => "build_editor",
            Self::OpenEditor => "open_editor",
            Self::AutomationTests => "automation_tests",
            Self::Cook => "cook",
            Self::Package => "package",
            Self::Validate => "validate",
            Self::BuildPlugin => "build_plugin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GenerateProjectFiles => "Файлы IDE",
            Self::BuildEditor => "Собрать Editor",
            Self::OpenEditor => "Открыть Editor",
            Self::AutomationTests => "Automation",
            Self::Cook => "Cook",
            Self::Package => "Package",
            Self::Validate => "Проверить контент",
            Self::BuildPlugin => "Собрать плагин",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UnrealCommandArgs {
    pub command: UnrealCommandKind,
    pub target: Option<String>,
    pub platform: Option<String>,
    pub configuration: Option<String>,
    pub test_filter: Option<String>,
    pub output_dir: Option<String>,
}

impl UnrealCommandArgs {
    pub fn simple(command: UnrealCommandKind) -> Self {
        Self {
            command,
            target: None,
            platform: None,
            configuration: None,
            test_filter: None,
            output_dir: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct UnrealCommandSpec {
    pub id: String,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub timeout_secs: u64,
    pub detached: bool,
    pub shell_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealLogIssue {
    pub severity: String,
    pub source: String,
    pub code: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub suggested_action: String,
}

#[derive(Clone, Debug)]
pub struct UnrealProfileCommand {
    pub id: String,
    pub label: String,
    pub command: String,
    pub description: String,
    pub timeout_secs: u64,
}

pub fn unreal_snapshot(workspace: &Workspace) -> UnrealSnapshot {
    let project = find_descriptor(workspace.root(), "uproject")
        .and_then(|path| parse_project_descriptor(&path).ok());
    let local_plugins = discover_local_plugins(workspace.root());
    let installations = discover_installations();
    let selected_engine = select_engine(&installations, project.as_ref());
    let toolchain = detect_toolchain(selected_engine.as_ref());
    let mut diagnostics = Vec::new();

    if project.is_none() && local_plugins.is_empty() {
        diagnostics.push("В рабочей папке не найдено .uproject или .uplugin".to_string());
    }
    if installations.is_empty() {
        diagnostics.push(
            "Установка Unreal Engine не найдена; задайте UE_ENGINE_ROOT или установите движок через Epic Games Launcher"
                .to_string(),
        );
    } else if selected_engine.is_none() {
        diagnostics
            .push("EngineAssociation проекта не сопоставлен с установкой Unreal".to_string());
    }
    if let Some(engine) = &selected_engine {
        for (name, path) in [
            ("UnrealEditor", &engine.tools.editor),
            ("UnrealEditor-Cmd", &engine.tools.editor_cmd),
            ("Build.bat", &engine.tools.build),
            ("RunUAT.bat", &engine.tools.uat),
            ("UnrealBuildTool", &engine.tools.ubt),
        ] {
            if path.is_none() {
                diagnostics.push(format!("В выбранной установке не найден {name}"));
            }
        }
    }
    if !toolchain
        .iter()
        .any(|component| component.id == "visual_studio" && component.available)
    {
        diagnostics.push("Не обнаружен Visual Studio с C++ toolchain".to_string());
    }

    UnrealSnapshot {
        workspace_root: path_string(workspace.root()),
        project,
        local_plugins,
        installations,
        selected_engine,
        toolchain,
        diagnostics,
    }
}

pub fn unreal_setup_report(workspace: &Workspace) -> UnrealSetupReport {
    let snapshot = unreal_snapshot(workspace);
    let project = snapshot.project.as_ref();
    let engine = snapshot.selected_engine.as_ref();
    let engine_is_5_8 = engine
        .and_then(|engine| engine.version.as_deref())
        .is_some_and(|version| version.starts_with("5.8"));
    let plugins = [
        (
            PYTHON_EDITOR_PLUGIN_ID,
            "Python Editor Script",
            true,
            "Нужен для headless Asset Registry scan и автоматизации редактора.",
        ),
        (
            EDITOR_SCRIPTING_PLUGIN_ID,
            "Editor Scripting Utilities",
            true,
            "Даёт безопасные editor-операции над ассетами и уровнями.",
        ),
        (
            UNREAL_MCP_PLUGIN_ID,
            "Unreal MCP",
            false,
            "Подключает интерактивные возможности Unreal через MCP; для базового scan необязателен.",
        ),
    ]
    .into_iter()
    .map(|(id, label, required_for_deep_scan, detail)| UnrealSetupPluginStatus {
        id: id.to_string(),
        label: label.to_string(),
        available: engine.is_some_and(|engine| engine_plugin_available(engine, id)),
        enabled: project.is_some_and(|project| project_plugin_enabled(project, id)),
        required_for_deep_scan,
        detail: detail.to_string(),
    })
    .collect::<Vec<_>>();
    let missing_toolchain = snapshot
        .toolchain
        .iter()
        .filter(|component| !component.available)
        .map(|component| component.label.clone())
        .collect::<Vec<_>>();
    let required_plugins_ready = plugins
        .iter()
        .filter(|plugin| plugin.required_for_deep_scan)
        .all(|plugin| plugin.available && plugin.enabled);
    let mcp_profile_configured =
        crate::mcp::ensure_registry(workspace)
            .ok()
            .is_some_and(|registry| {
                registry
                    .servers
                    .iter()
                    .any(|server| server.id == "unreal-mcp")
            });
    let can_deep_scan = project.is_some()
        && engine_is_5_8
        && missing_toolchain.is_empty()
        && required_plugins_ready;
    let status = if project.is_none() {
        UnrealSetupStatus::NeedsProject
    } else if !engine_is_5_8 {
        UnrealSetupStatus::NeedsEngine
    } else if !missing_toolchain.is_empty() {
        UnrealSetupStatus::NeedsTools
    } else if !required_plugins_ready {
        UnrealSetupStatus::NeedsPlugins
    } else {
        UnrealSetupStatus::Ready
    };
    let summary = match status {
        UnrealSetupStatus::Ready => {
            "Проект и Unreal Engine готовы. Можно построить Project Map одной кнопкой."
        }
        UnrealSetupStatus::NeedsProject => {
            "Выберите папку, в корне которой находится файл .uproject."
        }
        UnrealSetupStatus::NeedsEngine => {
            "Для проекта не найдена совместимая установка Unreal Engine 5.8."
        }
        UnrealSetupStatus::NeedsTools => {
            "Установка обнаружена, но для полного игрового pipeline не хватает компонентов."
        }
        UnrealSetupStatus::NeedsPlugins => {
            "Включите необходимые project-плагины; Leetcode создаст резервную копию .uproject."
        }
    }
    .to_string();

    UnrealSetupReport {
        status,
        project_name: project.map(|project| project.name.clone()),
        project_path: project.map(|project| project.path.clone()),
        engine_version: engine.and_then(|engine| engine.version.clone()),
        engine_root: engine.map(|engine| engine.root.clone()),
        plugins,
        missing_toolchain,
        mcp_profile_configured,
        can_deep_scan,
        summary,
    }
}

pub fn enable_unreal_project_plugins(
    workspace: &Workspace,
    requested_plugins: &[String],
) -> anyhow::Result<UnrealPluginEnableResult> {
    let snapshot = unreal_snapshot(workspace);
    let project = snapshot
        .project
        .ok_or_else(|| anyhow::anyhow!("в выбранной папке не найден .uproject"))?;
    let project_path = PathBuf::from(&project.path)
        .canonicalize()
        .with_context(|| format!("не удалось открыть {}", project.path))?;
    if !project_path.starts_with(workspace.root()) {
        anyhow::bail!(".uproject находится вне выбранной рабочей папки");
    }
    let allowed = [
        PYTHON_EDITOR_PLUGIN_ID,
        EDITOR_SCRIPTING_PLUGIN_ID,
        UNREAL_MCP_PLUGIN_ID,
    ];
    let requested = requested_plugins
        .iter()
        .map(|plugin| plugin.trim())
        .filter(|plugin| !plugin.is_empty())
        .collect::<BTreeSet<_>>();
    if requested.is_empty() {
        anyhow::bail!("не выбраны плагины для включения");
    }
    if let Some(plugin) = requested
        .iter()
        .find(|plugin| !allowed.iter().any(|allowed| *allowed == **plugin))
    {
        anyhow::bail!("плагин не разрешён мастером настройки: {plugin}");
    }

    let raw = fs::read_to_string(&project_path)?;
    let mut value: Value = serde_json::from_str(&raw)?;
    let root = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(".uproject должен содержать JSON-объект"))?;
    let plugin_values = root
        .entry("Plugins".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("поле Plugins в .uproject должно быть массивом"))?;
    let mut changed_plugins = Vec::new();
    for plugin_id in requested {
        if let Some(plugin) = plugin_values.iter_mut().find(|plugin| {
            plugin
                .get("Name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(plugin_id))
        }) {
            let enabled = plugin.get("Enabled").and_then(Value::as_bool) == Some(true);
            if !enabled {
                plugin["Enabled"] = Value::Bool(true);
                changed_plugins.push(plugin_id.to_string());
            }
        } else {
            plugin_values.push(json!({"Name": plugin_id, "Enabled": true}));
            changed_plugins.push(plugin_id.to_string());
        }
    }

    let backup_path = if changed_plugins.is_empty() {
        String::new()
    } else {
        let backup_relative = format!(
            "assets/generated/leetcode/backups/unreal/{}-{}.uproject",
            project.name,
            unix_timestamp()
        );
        workspace.write_text(&backup_relative, &raw)?;
        let rendered = format!("{}\n", serde_json::to_string_pretty(&value)?);
        fs::write(&project_path, rendered)?;
        backup_relative
    };

    Ok(UnrealPluginEnableResult {
        changed_plugins,
        backup_path,
        report: unreal_setup_report(workspace),
    })
}

fn project_plugin_enabled(project: &UnrealProjectDescriptor, plugin_id: &str) -> bool {
    project
        .plugins
        .iter()
        .any(|plugin| plugin.name.eq_ignore_ascii_case(plugin_id) && plugin.enabled)
}

fn engine_plugin_available(engine: &UnrealInstallation, plugin_id: &str) -> bool {
    let file_name = format!("{plugin_id}.uplugin");
    WalkDir::new(Path::new(&engine.root).join("Engine/Plugins"))
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_type().is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&file_name))
        })
}

pub fn unreal_snapshot_tool(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&unreal_snapshot(workspace))
            .unwrap_or_else(|_| "Не удалось сериализовать Unreal snapshot".to_string()),
    )
}

pub fn build_unreal_command(
    workspace: &Workspace,
    request: &UnrealCommandArgs,
) -> anyhow::Result<UnrealCommandSpec> {
    let snapshot = unreal_snapshot(workspace);
    build_unreal_command_with_snapshot(workspace, request, &snapshot)
}

fn build_unreal_command_with_snapshot(
    workspace: &Workspace,
    request: &UnrealCommandArgs,
    snapshot: &UnrealSnapshot,
) -> anyhow::Result<UnrealCommandSpec> {
    let engine = snapshot
        .selected_engine
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("подходящая установка Unreal Engine не найдена"))?;
    let platform = validated_token(request.platform.as_deref().unwrap_or("Win64"), "platform")?;
    let configuration = validated_token(
        request.configuration.as_deref().unwrap_or("Development"),
        "configuration",
    )?;
    let project = snapshot.project.as_ref();
    let plugin = snapshot.local_plugins.first();
    let (program, args, timeout_secs, detached) = match request.command {
        UnrealCommandKind::GenerateProjectFiles => {
            let project = require_project(project)?;
            (
                require_tool(&engine.tools.build, "Build.bat")?,
                vec![
                    "-projectfiles".to_string(),
                    format!("-project={}", project.path),
                    "-game".to_string(),
                    "-rocket".to_string(),
                    "-progress".to_string(),
                ],
                1_800,
                false,
            )
        }
        UnrealCommandKind::BuildEditor => {
            let project = require_project(project)?;
            let target = request
                .target
                .as_deref()
                .map(|value| validated_token(value, "target"))
                .transpose()?
                .unwrap_or_else(|| format!("{}Editor", project.name));
            (
                require_tool(&engine.tools.build, "Build.bat")?,
                vec![
                    target,
                    platform,
                    configuration,
                    format!("-Project={}", project.path),
                    "-WaitMutex".to_string(),
                    "-FromMsBuild".to_string(),
                ],
                1_800,
                false,
            )
        }
        UnrealCommandKind::OpenEditor => {
            let project = require_project(project)?;
            (
                require_tool(&engine.tools.editor, "UnrealEditor")?,
                vec![project.path.clone(), "-log".to_string()],
                30,
                true,
            )
        }
        UnrealCommandKind::AutomationTests => {
            let project = require_project(project)?;
            let filter =
                validated_test_filter(request.test_filter.as_deref().unwrap_or("Project"))?;
            let report = workspace.resolve_for_write("Saved/AutomationReports")?;
            (
                require_tool(&engine.tools.editor_cmd, "UnrealEditor-Cmd")?,
                vec![
                    project.path.clone(),
                    "-Unattended".to_string(),
                    "-NoSplash".to_string(),
                    "-NullRHI".to_string(),
                    format!("-ExecCmds=Automation RunTest {filter};Quit"),
                    "-TestExit=Automation Test Queue Empty".to_string(),
                    format!("-ReportExportPath={}", path_string(&report)),
                    "-log".to_string(),
                    "-UTF8Output".to_string(),
                ],
                1_800,
                false,
            )
        }
        UnrealCommandKind::Cook => {
            let project = require_project(project)?;
            (
                require_tool(&engine.tools.editor_cmd, "UnrealEditor-Cmd")?,
                vec![
                    project.path.clone(),
                    "-run=Cook".to_string(),
                    format!("-TargetPlatform={platform}"),
                    "-Unversioned".to_string(),
                    "-Unattended".to_string(),
                    "-stdout".to_string(),
                    "-UTF8Output".to_string(),
                ],
                1_800,
                false,
            )
        }
        UnrealCommandKind::Package => {
            let project = require_project(project)?;
            let output = safe_output_dir(workspace, request.output_dir.as_deref(), "Packaged")?;
            (
                require_tool(&engine.tools.uat, "RunUAT.bat")?,
                vec![
                    "BuildCookRun".to_string(),
                    format!("-project={}", project.path),
                    "-noP4".to_string(),
                    format!("-platform={platform}"),
                    format!("-clientconfig={configuration}"),
                    "-build".to_string(),
                    "-cook".to_string(),
                    "-stage".to_string(),
                    "-pak".to_string(),
                    "-package".to_string(),
                    "-archive".to_string(),
                    format!("-archivedirectory={}", path_string(&output)),
                    "-utf8output".to_string(),
                ],
                1_800,
                false,
            )
        }
        UnrealCommandKind::Validate => {
            let project = require_project(project)?;
            (
                require_tool(&engine.tools.editor_cmd, "UnrealEditor-Cmd")?,
                vec![
                    project.path.clone(),
                    "-run=DataValidation".to_string(),
                    "-unattended".to_string(),
                    "-nop4".to_string(),
                    "-nullrhi".to_string(),
                    "-log".to_string(),
                    "-UTF8Output".to_string(),
                ],
                1_800,
                false,
            )
        }
        UnrealCommandKind::BuildPlugin => {
            let plugin = plugin.ok_or_else(|| anyhow::anyhow!(".uplugin не найден"))?;
            let output =
                safe_output_dir(workspace, request.output_dir.as_deref(), "Packaged/Plugin")?;
            (
                require_tool(&engine.tools.uat, "RunUAT.bat")?,
                vec![
                    "BuildPlugin".to_string(),
                    format!("-Plugin={}", plugin.path),
                    format!("-Package={}", path_string(&output)),
                    format!("-TargetPlatforms={platform}"),
                    "-Rocket".to_string(),
                    "-utf8output".to_string(),
                ],
                1_800,
                false,
            )
        }
    };
    let shell_command = render_powershell_command(&program, &args, detached);
    Ok(UnrealCommandSpec {
        id: request.command.id().to_string(),
        label: request.command.label().to_string(),
        program,
        args,
        cwd: ".".to_string(),
        timeout_secs,
        detached,
        shell_command,
    })
}

pub(crate) fn profile_commands_from_snapshot(
    workspace: &Workspace,
    snapshot: &UnrealSnapshot,
) -> Vec<UnrealProfileCommand> {
    [
        UnrealCommandKind::Validate,
        UnrealCommandKind::GenerateProjectFiles,
        UnrealCommandKind::BuildEditor,
        UnrealCommandKind::OpenEditor,
        UnrealCommandKind::AutomationTests,
        UnrealCommandKind::Cook,
        UnrealCommandKind::Package,
        UnrealCommandKind::BuildPlugin,
    ]
    .into_iter()
    .filter_map(|kind| {
        build_unreal_command_with_snapshot(workspace, &UnrealCommandArgs::simple(kind), &snapshot)
            .ok()
            .map(|spec| UnrealProfileCommand {
                id: spec.id,
                label: spec.label,
                command: spec.shell_command,
                description: format!("Безопасный Unreal-профиль: {}", kind.label()),
                timeout_secs: spec.timeout_secs,
            })
    })
    .collect()
}

pub fn parse_unreal_log(text: &str) -> Vec<UnrealLogIssue> {
    let compiler = Regex::new(
        r"(?i)^(?P<file>.+?)\((?P<line>\d+)(?:,(?P<column>\d+))?\)\s*:\s*(?P<severity>fatal error|error|warning)\s*(?P<code>[A-Z]+\d+)?\s*:\s*(?P<message>.+)$",
    )
    .expect("compiler regex");
    let unreal = Regex::new(
        r"(?i)^(?:\[[^\]]+\]\s*)?(?P<source>Log[^:]+):\s*(?P<severity>Error|Warning):\s*(?P<message>.+)$",
    )
    .expect("unreal regex");
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();

    for raw in text.lines() {
        let line = raw
            .trim()
            .strip_prefix("[stdout]")
            .or_else(|| raw.trim().strip_prefix("[stderr]"))
            .unwrap_or(raw.trim())
            .trim();
        if line.is_empty() {
            continue;
        }
        let issue = if let Some(captures) = compiler.captures(line) {
            let message = captures["message"].trim().to_string();
            Some(UnrealLogIssue {
                severity: captures["severity"].to_ascii_lowercase(),
                source: "compiler".to_string(),
                code: captures
                    .name("code")
                    .map(|value| value.as_str().to_string()),
                file: captures
                    .name("file")
                    .map(|value| value.as_str().to_string()),
                line: captures
                    .name("line")
                    .and_then(|value| value.as_str().parse().ok()),
                column: captures
                    .name("column")
                    .and_then(|value| value.as_str().parse().ok()),
                suggested_action: suggested_action(&message),
                message,
            })
        } else if let Some(captures) = unreal.captures(line) {
            let message = captures["message"].trim().to_string();
            Some(UnrealLogIssue {
                severity: captures["severity"].to_ascii_lowercase(),
                source: captures["source"].to_string(),
                code: None,
                file: None,
                line: None,
                column: None,
                suggested_action: suggested_action(&message),
                message,
            })
        } else {
            generic_log_issue(line)
        };
        if let Some(issue) = issue {
            let key = format!(
                "{}|{}|{}|{}",
                issue.severity,
                issue.file.as_deref().unwrap_or_default(),
                issue.line.unwrap_or_default(),
                issue.message
            );
            if seen.insert(key) {
                issues.push(issue);
            }
            if issues.len() >= 200 {
                break;
            }
        }
    }
    issues
}

fn generic_log_issue(line: &str) -> Option<UnrealLogIssue> {
    let lower = line.to_ascii_lowercase();
    let (severity, source, message) = if let Some(index) = lower.find("error:") {
        ("error", "AutomationTool", line[index + 6..].trim())
    } else if lower.contains("automation test failed")
        || lower.contains("test completed. result={fail")
    {
        ("error", "Automation", line)
    } else if let Some(index) = lower.find("warning:") {
        ("warning", "AutomationTool", line[index + 8..].trim())
    } else {
        return None;
    };
    Some(UnrealLogIssue {
        severity: severity.to_string(),
        source: source.to_string(),
        code: None,
        file: None,
        line: None,
        column: None,
        message: message.to_string(),
        suggested_action: suggested_action(message),
    })
}

fn suggested_action(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("sdk") || lower.contains("toolchain") {
        "Проверьте Visual Studio workload, Windows SDK и Turnkey VerifySdk".to_string()
    } else if lower.contains("plugin") {
        "Проверьте .uplugin, зависимости плагина и совместимость EngineVersion".to_string()
    } else if lower.contains("module") || lower.contains("could not be compiled") {
        "Пересоберите Editor target и проверьте Build.cs/Target.cs".to_string()
    } else if lower.contains("cook") || lower.contains("package") {
        "Проверьте первую ошибку cook, карты упаковки и настройки целевой платформы".to_string()
    } else if lower.contains("automation") || lower.contains("test") {
        "Откройте Automation Report и повторите конкретный test filter".to_string()
    } else {
        "Исправьте первую первичную ошибку и повторите тот же Unreal-профиль".to_string()
    }
}

fn discover_installations() -> Vec<UnrealInstallation> {
    let mut roots = Vec::<(PathBuf, String, Option<String>)>::new();
    for variable in ["UE_ENGINE_ROOT", "UE_ROOT"] {
        if let Some(value) = env::var_os(variable) {
            roots.push((PathBuf::from(value), format!("env:{variable}"), None));
        }
    }
    roots.extend(registry_engine_roots());
    roots.extend(standard_engine_roots());

    let mut by_root = HashMap::<String, UnrealInstallation>::new();
    for (root, source, association) in roots {
        let root = fs::canonicalize(&root).unwrap_or(root);
        if !root.join("Engine").is_dir() {
            continue;
        }
        let key = root.to_string_lossy().to_ascii_lowercase();
        let entry = by_root
            .entry(key)
            .or_insert_with(|| installation(&root, &source));
        if !entry.source.contains(&source) {
            entry.source.push_str(", ");
            entry.source.push_str(&source);
        }
        if let Some(association) = association {
            if !association.is_empty() && !entry.associations.contains(&association) {
                entry.associations.push(association);
            }
        }
    }
    let mut installations = by_root.into_values().collect::<Vec<_>>();
    installations.sort_by(|left, right| version_key(right).cmp(&version_key(left)));
    installations
}

fn installation(root: &Path, source: &str) -> UnrealInstallation {
    let build_version = fs::read_to_string(root.join("Engine/Build/Build.version"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let version = build_version.as_ref().and_then(|value| {
        Some(format!(
            "{}.{}.{}",
            value.get("MajorVersion")?.as_u64()?,
            value.get("MinorVersion")?.as_u64()?,
            value.get("PatchVersion")?.as_u64()?
        ))
    });
    let branch = build_version
        .as_ref()
        .and_then(|value| value.get("BranchName"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let folder_association = root
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("UE_"))
        .map(ToString::to_string);
    let mut associations = Vec::new();
    if let Some(association) = folder_association {
        associations.push(association);
    }
    if let Some(version) = &version {
        let short = version
            .rsplit_once('.')
            .map(|(value, _)| value)
            .unwrap_or(version);
        if !associations.iter().any(|known| known == short) {
            associations.push(short.to_string());
        }
    }
    UnrealInstallation {
        root: path_string(root),
        version,
        branch,
        associations,
        source: source.to_string(),
        tools: UnrealToolPaths {
            editor: existing_string(root.join("Engine/Binaries/Win64/UnrealEditor.exe")),
            editor_cmd: existing_string(root.join("Engine/Binaries/Win64/UnrealEditor-Cmd.exe")),
            build: existing_string(root.join("Engine/Build/BatchFiles/Build.bat")),
            uat: existing_string(root.join("Engine/Build/BatchFiles/RunUAT.bat")),
            ubt: existing_string(
                root.join("Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.exe"),
            )
            .or_else(|| {
                existing_string(
                    root.join("Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.dll"),
                )
            }),
        },
    }
}

#[cfg(target_os = "windows")]
fn registry_engine_roots() -> Vec<(PathBuf, String, Option<String>)> {
    let mut result = Vec::new();
    for (key, source) in [
        (r"HKLM\SOFTWARE\EpicGames\Unreal Engine", "registry:HKLM"),
        (
            r"HKCU\SOFTWARE\Epic Games\Unreal Engine\Builds",
            "registry:HKCU",
        ),
    ] {
        let Ok(output) = Command::new("reg").args(["query", key, "/s"]).output() else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let mut current_association = None;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("HKEY_") {
                current_association = trimmed.rsplit('\\').next().map(ToString::to_string);
                continue;
            }
            let columns = trimmed.split_whitespace().collect::<Vec<_>>();
            let reg_index = columns.iter().position(|column| column.starts_with("REG_"));
            let Some(reg_index) = reg_index else {
                continue;
            };
            let path = columns[reg_index + 1..].join(" ");
            if path.is_empty() {
                continue;
            }
            result.push((
                PathBuf::from(path),
                source.to_string(),
                current_association.clone(),
            ));
        }
    }
    result
}

#[cfg(not(target_os = "windows"))]
fn registry_engine_roots() -> Vec<(PathBuf, String, Option<String>)> {
    Vec::new()
}

fn standard_engine_roots() -> Vec<(PathBuf, String, Option<String>)> {
    let mut result = Vec::new();
    let parents = if cfg!(target_os = "windows") {
        vec![PathBuf::from(r"C:\Program Files\Epic Games")]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Users/Shared/Epic Games")]
    } else {
        Vec::new()
    };
    for parent in parents {
        let Ok(entries) = fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if let Some(association) = name.strip_prefix("UE_").map(ToString::to_string) {
                result.push((path, "standard-directory".to_string(), Some(association)));
            }
        }
    }
    result
}

fn select_engine(
    installations: &[UnrealInstallation],
    project: Option<&UnrealProjectDescriptor>,
) -> Option<UnrealInstallation> {
    if let Some(association) = project.and_then(|project| project.engine_association.as_deref()) {
        if let Some(engine) = installations.iter().find(|engine| {
            engine
                .associations
                .iter()
                .any(|known| known.eq_ignore_ascii_case(association))
        }) {
            return Some(engine.clone());
        }
    }
    installations.first().cloned()
}

fn detect_toolchain(engine: Option<&UnrealInstallation>) -> Vec<UnrealToolchainComponent> {
    let visual_studio = find_visual_studio();
    let dotnet = find_in_path(if cfg!(target_os = "windows") {
        "dotnet.exe"
    } else {
        "dotnet"
    });
    let compiler = visual_studio
        .as_ref()
        .and_then(|root| find_msvc_compiler(root));
    let sdk = windows_sdk_root();
    let mut components = vec![
        component(
            "visual_studio",
            "Visual Studio C++",
            visual_studio,
            "IDE и MSVC workload для сборки C++",
        ),
        component("msvc", "MSVC compiler", compiler, "cl.exe для UBT"),
        component(
            "dotnet",
            ".NET runtime",
            dotnet,
            "Runtime для UBT/AutomationTool",
        ),
        component("windows_sdk", "Windows SDK", sdk, "SDK для Win64 target"),
    ];
    if let Some(engine) = engine {
        components.extend([
            component(
                "unreal_editor",
                "UnrealEditor",
                engine.tools.editor.as_ref().map(PathBuf::from),
                "Интерактивный редактор",
            ),
            component(
                "unreal_editor_cmd",
                "UnrealEditor-Cmd",
                engine.tools.editor_cmd.as_ref().map(PathBuf::from),
                "Commandlet и automation runtime",
            ),
            component(
                "ubt",
                "UnrealBuildTool",
                engine.tools.ubt.as_ref().map(PathBuf::from),
                "Сборка targets и project files",
            ),
            component(
                "uat",
                "AutomationTool",
                engine.tools.uat.as_ref().map(PathBuf::from),
                "Cook, package и BuildPlugin",
            ),
        ]);
    }
    components
}

fn component(
    id: &str,
    label: &str,
    path: Option<PathBuf>,
    detail: &str,
) -> UnrealToolchainComponent {
    let available = path.as_ref().is_some_and(|path| path.exists());
    UnrealToolchainComponent {
        id: id.to_string(),
        label: label.to_string(),
        available,
        path: path.map(|path| path_string(&path)),
        detail: detail.to_string(),
    }
}

fn find_visual_studio() -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let vswhere =
        PathBuf::from(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn find_msvc_compiler(visual_studio: &Path) -> Option<PathBuf> {
    let root = visual_studio.join("VC/Tools/MSVC");
    let mut versions = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    versions
        .into_iter()
        .map(|version| version.join("bin/Hostx64/x64/cl.exe"))
        .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn windows_sdk_root() -> Option<PathBuf> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
            "/v",
            "KitsRoot10",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        let index = columns.iter().position(|column| *column == "REG_SZ")?;
        let path = PathBuf::from(columns[index + 1..].join(" "));
        path.exists().then_some(path)
    })
}

#[cfg(not(target_os = "windows"))]
fn windows_sdk_root() -> Option<PathBuf> {
    None
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(binary))
        .find(|path| path.is_file())
}

fn parse_project_descriptor(path: &Path) -> anyhow::Result<UnrealProjectDescriptor> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("UnrealProject")
        .to_string();
    Ok(UnrealProjectDescriptor {
        path: path_string(path),
        name,
        engine_association: string_field(&value, "EngineAssociation"),
        category: string_field(&value, "Category"),
        description: string_field(&value, "Description"),
        modules: parse_modules(&value),
        plugins: value
            .get("Plugins")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|plugin| {
                Some(UnrealPluginReference {
                    name: plugin.get("Name")?.as_str()?.to_string(),
                    enabled: plugin
                        .get("Enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    target_allow_list: string_array(
                        plugin
                            .get("TargetAllowList")
                            .or_else(|| plugin.get("WhitelistTargets")),
                    ),
                })
            })
            .collect(),
    })
}

fn discover_local_plugins(root: &Path) -> Vec<UnrealPluginDescriptor> {
    let mut plugins = WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some("Binaries" | "Intermediate" | "Saved" | "Packaged" | ".git" | "target")
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("uplugin"))
        })
        .filter_map(|entry| parse_plugin_descriptor(entry.path()).ok())
        .collect::<Vec<_>>();
    plugins.sort_by(|left, right| left.name.cmp(&right.name));
    plugins
}

fn parse_plugin_descriptor(path: &Path) -> anyhow::Result<UnrealPluginDescriptor> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(UnrealPluginDescriptor {
        path: path_string(path),
        name: path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Plugin")
            .to_string(),
        friendly_name: string_field(&value, "FriendlyName"),
        version_name: string_field(&value, "VersionName"),
        enabled_by_default: value.get("EnabledByDefault").and_then(Value::as_bool),
        modules: parse_modules(&value),
    })
}

fn parse_modules(value: &Value) -> Vec<UnrealModuleDescriptor> {
    value
        .get("Modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|module| {
            Some(UnrealModuleDescriptor {
                name: module.get("Name")?.as_str()?.to_string(),
                module_type: module
                    .get("Type")
                    .and_then(Value::as_str)
                    .unwrap_or("Runtime")
                    .to_string(),
                loading_phase: module
                    .get("LoadingPhase")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            })
        })
        .collect()
}

fn find_descriptor(root: &Path, extension: &str) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn existing_string(path: PathBuf) -> Option<String> {
    path.exists().then(|| path_string(&path))
}

fn path_string(path: &Path) -> String {
    let rendered = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        if let Some(path) = rendered.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{path}");
        }
        if let Some(path) = rendered.strip_prefix(r"\\?\") {
            return path.to_string();
        }
    }
    rendered.to_string()
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn require_project(
    project: Option<&UnrealProjectDescriptor>,
) -> anyhow::Result<&UnrealProjectDescriptor> {
    project.ok_or_else(|| anyhow::anyhow!(".uproject не найден в корне рабочей папки"))
}

fn require_tool(path: &Option<String>, name: &str) -> anyhow::Result<String> {
    path.clone()
        .ok_or_else(|| anyhow::anyhow!("в выбранной установке не найден {name}"))
}

fn validated_token(value: &str, field: &str) -> anyhow::Result<String> {
    let value = value.trim();
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        anyhow::bail!("некорректный {field}: {value}");
    }
    Ok(value.to_string())
}

fn validated_test_filter(value: &str) -> anyhow::Result<String> {
    let value = value.trim();
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character.is_whitespace()
                || matches!(character, '.' | '_' | '-' | ':' | '+')
        })
    {
        anyhow::bail!("некорректный automation test filter");
    }
    Ok(value.to_string())
}

fn safe_output_dir(
    workspace: &Workspace,
    requested: Option<&str>,
    fallback: &str,
) -> anyhow::Result<PathBuf> {
    workspace.resolve_for_write(requested.unwrap_or(fallback))
}

fn render_powershell_command(program: &str, args: &[String], detached: bool) -> String {
    let program = powershell_quote(program);
    let rendered_args = args
        .iter()
        .map(|argument| powershell_quote(argument))
        .collect::<Vec<_>>();
    if detached {
        format!(
            "Start-Process -FilePath {program} -ArgumentList @({})",
            rendered_args.join(", ")
        )
    } else if rendered_args.is_empty() {
        format!("& {program}")
    } else {
        format!("& {program} {}", rendered_args.join(" "))
    }
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn version_key(engine: &UnrealInstallation) -> (u32, u32, u32) {
    let mut parts = engine
        .version
        .as_deref()
        .unwrap_or_default()
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok());
    (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_workspace() -> Workspace {
        Workspace::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unreal/SampleGame"),
        )
        .expect("fixture workspace")
    }

    #[test]
    fn parses_fixture_project_and_plugin() {
        let workspace = fixture_workspace();
        let snapshot = unreal_snapshot(&workspace);
        let project = snapshot.project.expect("project");
        assert_eq!(project.name, "SampleGame");
        assert_eq!(project.engine_association.as_deref(), Some("5.8"));
        assert_eq!(project.modules[0].name, "SampleGame");
        assert_eq!(snapshot.local_plugins[0].name, "SampleTools");
    }

    #[test]
    fn setup_wizard_enables_allowlisted_plugins_and_creates_backup() {
        let temp = tempfile::tempdir().expect("workspace");
        fs::write(
            temp.path().join("WizardGame.uproject"),
            r#"{
  "FileVersion": 3,
  "EngineAssociation": "5.8",
  "Plugins": [
    {"Name": "PythonScriptPlugin", "Enabled": false}
  ]
}"#,
        )
        .expect("project descriptor");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let result = enable_unreal_project_plugins(
            &workspace,
            &[
                PYTHON_EDITOR_PLUGIN_ID.to_string(),
                EDITOR_SCRIPTING_PLUGIN_ID.to_string(),
            ],
        )
        .expect("enable plugins");
        assert_eq!(result.changed_plugins.len(), 2);
        assert!(!result.backup_path.is_empty());
        assert!(workspace.resolve_existing(&result.backup_path).is_ok());

        let snapshot = unreal_snapshot(&workspace);
        let project = snapshot.project.expect("project");
        assert!(project_plugin_enabled(&project, PYTHON_EDITOR_PLUGIN_ID));
        assert!(project_plugin_enabled(&project, EDITOR_SCRIPTING_PLUGIN_ID));
    }

    #[test]
    fn setup_wizard_rejects_arbitrary_plugin_mutation() {
        let temp = tempfile::tempdir().expect("workspace");
        fs::write(
            temp.path().join("WizardGame.uproject"),
            r#"{"FileVersion":3,"EngineAssociation":"5.8"}"#,
        )
        .expect("project descriptor");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let error =
            enable_unreal_project_plugins(&workspace, &["UntrustedThirdPartyPlugin".to_string()])
                .unwrap_err();
        assert!(error.to_string().contains("не разрешён"));
    }

    #[test]
    fn builds_safe_command_with_injected_engine() {
        let workspace = fixture_workspace();
        let temp = tempfile::tempdir().expect("engine");
        for rel in [
            "Engine/Binaries/Win64/UnrealEditor.exe",
            "Engine/Binaries/Win64/UnrealEditor-Cmd.exe",
            "Engine/Build/BatchFiles/Build.bat",
            "Engine/Build/BatchFiles/RunUAT.bat",
            "Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.exe",
        ] {
            let path = temp.path().join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, "fixture").expect("tool");
        }
        fs::create_dir_all(temp.path().join("Engine/Build")).expect("build dir");
        fs::write(
            temp.path().join("Engine/Build/Build.version"),
            r#"{"MajorVersion":5,"MinorVersion":8,"PatchVersion":0}"#,
        )
        .expect("version");
        let mut snapshot = unreal_snapshot(&workspace);
        snapshot.selected_engine = Some(installation(temp.path(), "fixture"));
        let spec = build_unreal_command_with_snapshot(
            &workspace,
            &UnrealCommandArgs::simple(UnrealCommandKind::BuildEditor),
            &snapshot,
        )
        .expect("command");
        assert!(spec.shell_command.contains("Build.bat"));
        assert!(spec.args.iter().any(|arg| arg == "SampleGameEditor"));
        assert!(spec.args.iter().any(|arg| arg.starts_with("-Project=")));
    }

    #[test]
    fn rejects_shell_metacharacters_in_test_filter() {
        let result = validated_test_filter("Project; Remove-Item *");
        assert!(result.is_err());
    }

    #[test]
    fn parses_compiler_unreal_and_automation_errors() {
        let issues = parse_unreal_log(
            r#"
C:\Game\Source\Pawn.cpp(42,7): error C2065: 'Value': undeclared identifier
LogDataValidation: Error: /Game/Maps/TestMap has invalid data
ERROR: AutomationTool failed to cook project
Automation Test Failed (SampleGame.Smoke)
"#,
        );
        assert_eq!(issues.len(), 4);
        assert_eq!(issues[0].file.as_deref(), Some(r"C:\Game\Source\Pawn.cpp"));
        assert_eq!(issues[0].line, Some(42));
        assert_eq!(issues[0].code.as_deref(), Some("C2065"));
        assert_eq!(issues[1].source, "LogDataValidation");
        assert!(issues[2].suggested_action.contains("cook"));
    }

    #[test]
    #[ignore = "requires LEETCODE_UE_VALIDATION_WORKSPACE and a local UE 5.8 installation"]
    fn validates_configured_unreal_5_8_workspace() {
        let root = env::var_os("LEETCODE_UE_VALIDATION_WORKSPACE")
            .map(PathBuf::from)
            .expect("set LEETCODE_UE_VALIDATION_WORKSPACE");
        let workspace = Workspace::new(root).expect("validation workspace");
        let snapshot = unreal_snapshot(&workspace);
        println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).expect("snapshot json")
        );

        assert!(snapshot.project.is_some() || !snapshot.local_plugins.is_empty());
        let engine = snapshot.selected_engine.expect("selected Unreal engine");
        assert!(engine
            .version
            .as_deref()
            .is_some_and(|version| version.starts_with("5.8")));
        assert!(engine.tools.editor.is_some());
        assert!(engine.tools.editor_cmd.is_some());
        assert!(engine.tools.build.is_some());
        assert!(engine.tools.uat.is_some());
        assert!(engine.tools.ubt.is_some());
        assert!(snapshot
            .toolchain
            .iter()
            .any(|component| component.id == "visual_studio" && component.available));
    }
}
