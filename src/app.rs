use crate::agent::models::{
    models_for_provider, provider_name, provider_specs, GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID,
};
use crate::agent::routing::route_labels;
use crate::agent::types::{AppEvent, ChatLine, ChatRole, ToolLogLine};
use crate::agent::{run_user_turn, AgentState};
use crate::agent_history::{
    append_agent_history, load_agent_history_tail, AgentRunConfirmedPlan, AgentRunHistoryContext,
    AgentRunHistoryRecord, AGENT_HISTORY_PATH,
};
use crate::asset_library::{favorite_asset, load_library, FavoriteAssetArgs};
use crate::assets::{
    absolute_output_path, asset_provider_env_var, attach_asset_context, audio_provider_name,
    default_audio_model, default_image_model, default_video_model, export_asset,
    image_provider_env_var, image_provider_name, image_provider_specs, image_request_from_job,
    is_image_path, load_jobs, run_audio_job, run_image_job, run_spritesheet_job, run_video_job,
    upscale_asset, video_provider_name, AssetEvent, AssetJob, AssetKind, AssetStatus,
    AudioAssetRequest, ImageAssetRequest, SpritesheetAssetRequest, VideoAssetRequest,
    GEMINI_IMAGE_PROVIDER_ID, OPENAI_AUDIO_PROVIDER_ID, OPENAI_IMAGE_PROVIDER_ID,
    OPENAI_VIDEO_PROVIDER_ID,
};
use crate::config::{
    append_journal, clear_journal, permission_mode_description, policy_profile_labels,
    read_journal_tail, AppConfig, CommandPaletteMacro, ProjectUiState,
};
use crate::conversation::{
    archive_conversation, compile_context_snapshot_with_budget, create_new_conversation,
    default_chat, delete_conversation, export_context_profile, import_context_profile_file,
    list_context_profiles, load_active_conversation, load_index as load_conversation_index,
    load_state as load_conversation_state, read_context_profile_file, rename_conversation,
    restore_conversation, save_conversation_context_notes, save_conversation_snapshot,
    save_index as save_conversation_index, save_state as save_conversation_state,
    set_conversation_pinned, ContextBudget, ContextProfile, ConversationIndex, LoadedConversation,
};
use crate::diagnostics::{environment_diagnostics, DiagnosticItem, EnvironmentDiagnostics};
use crate::evals::{load_results, run_replay_eval, RunReplayEvalArgs};
use crate::game_workflows::{
    parse_workflow_kind, run_game_workflow, workflow_specs, GameWorkflowRequest,
};
use crate::governance::{load_governance, save_governance, tool_specs};
use crate::http::{proxy_status_label, proxy_system_status_label};
use crate::memory::{
    import_memory_source_file, load_memory, record_decision, record_memory_source,
    record_project_goal, remove_memory_source, update_task_status, upsert_task, ProjectTask,
    RecordDecisionArgs, RecordMemorySourceArgs, RecordProjectGoalArgs, RemoveMemorySourceArgs,
    UpdateTaskStatusArgs, UpsertTaskArgs,
};
use crate::orchestration::{
    agent_role_specs, create_replay_eval, export_trace, load_orchestration_state,
    orchestration_snapshot, parse_agent_role, record_handoff,
};
use crate::project::{detect_project_profiles, ProjectCommand, ProjectProfile};
use crate::provider_health::{
    load_provider_validation_history, provider_health_report, provider_validation_plan,
    record_provider_validation_run, run_provider_live_validation, ProviderValidationRun,
};
use crate::roadmap::{
    load_roadmap, record_milestone, roadmap_markdown_export, RecordMilestoneArgs, RoadmapItem,
    RoadmapStatus, UpdateRoadmapItemArgs,
};
use crate::run_timeline::{RunTimeline, RunTimelineStatus};
use crate::self_modification::{
    is_leetcode_workspace, prepare_self_modification_guard, run_self_modification_validation,
    SelfModificationGuard,
};
use crate::terminal::{
    clear_terminal_output, read_terminal_snapshot, start_terminal_session, stop_terminal_session,
    write_terminal_input,
};
use crate::tools::desktop::capture_screenshot_file;
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::shell::{run_shell, RunShellArgs};
use crate::workspace::Workspace;
use eframe::egui::{self, RichText, TextEdit};
use image::{ColorType, ImageFormat};
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct LeetcodeApp {
    config: AppConfig,
    provider_input: String,
    api_key_input: String,
    model_input: String,
    workspace: Option<Workspace>,
    file_rows: Vec<String>,
    selected_tree_path: Option<String>,
    project_rename_target: Option<PathBuf>,
    project_rename_input: String,
    file_search_input: String,
    file_filter: FileTreeFilter,
    git_changed_files: Vec<String>,
    file_rename_target: Option<String>,
    file_rename_input: String,
    last_file_click_path: Option<String>,
    last_file_click_time: f64,
    dragged_tree_path: Option<String>,
    file_operation_status: String,
    selected_file: Option<String>,
    selected_preview: String,
    original_file_content: String,
    selected_file_editable: bool,
    editor_status: String,
    input: String,
    input_attachments: Vec<InputAttachment>,
    input_attachment_status: String,
    input_paste_shortcut_down: bool,
    command_palette_open: bool,
    command_palette_query: String,
    command_palette_selected: usize,
    command_macro_name_input: String,
    command_macro_edit_target: Option<String>,
    command_macro_edit_name: String,
    command_macro_edit_description: String,
    command_macro_import_input: String,
    command_palette_status: String,
    pending_command_macro_run: Option<PendingCommandMacroRun>,
    chat: Vec<ChatLine>,
    active_conversation_id: Option<String>,
    conversation_index: ConversationIndex,
    conversation_status: String,
    conversation_rename_target: Option<String>,
    conversation_rename_input: String,
    context_inspector_query: String,
    context_notes: Vec<String>,
    context_note_input: String,
    context_note_suggestions: Vec<String>,
    context_profile_preview: Option<ContextProfilePreview>,
    context_health_status: String,
    context_panel_tab: ContextPanelTab,
    tool_log: Vec<ToolLogLine>,
    journal_lines: Vec<String>,
    journal_status: String,
    agent_history: Vec<AgentRunHistoryRecord>,
    agent_history_status: String,
    agent_history_query: String,
    agent_history_status_filter: AgentHistoryStatusFilter,
    agent_history_duration_filter: AgentHistoryDurationFilter,
    agent_history_date_filter: AgentHistoryDateFilter,
    selected_agent_history_id: Option<String>,
    git_summary: String,
    git_action_status: String,
    git_commit_dialog_open: bool,
    git_commit_message_input: String,
    project_profiles: Vec<ProjectProfile>,
    project_events_rx: Option<Receiver<AppEvent>>,
    project_is_running: bool,
    project_cancel: Option<Arc<AtomicBool>>,
    project_status: String,
    project_task_title_input: String,
    project_task_workstream_input: String,
    project_task_milestone_input: String,
    project_task_priority_input: String,
    last_project_command: Option<ProjectCommand>,
    project_runs: Vec<ProjectRunRecord>,
    project_fix_requests: Vec<ProjectFixRequestRecord>,
    active_project_run_id: Option<String>,
    desktop_status: String,
    desktop_last_screenshot: Option<String>,
    desktop_active_window: String,
    terminal_input: String,
    terminal_output: String,
    terminal_status: String,
    terminal_running: bool,
    governance_status: String,
    governance_pattern_input: String,
    memory_goal_input: String,
    memory_task_input: String,
    memory_decision_input: String,
    memory_source_title_input: String,
    memory_source_note_input: String,
    memory_status: String,
    asset_library_filter: String,
    asset_library_status: String,
    eval_status: String,
    provider_health_status: String,
    provider_validation_rx: Option<Receiver<ProviderValidationRun>>,
    provider_validation_running: bool,
    provider_validation_results: Vec<ProviderValidationRun>,
    orchestration_status: String,
    asset_provider_input: String,
    asset_kind_input: String,
    asset_api_key_input: String,
    asset_model_input: String,
    asset_prompt: String,
    asset_aspect_ratio: String,
    asset_image_size: String,
    asset_jobs: Vec<AssetJob>,
    asset_events_rx: Option<Receiver<AssetEvent>>,
    asset_is_running: bool,
    asset_status: String,
    asset_compare_paths: Vec<String>,
    asset_import_target_input: String,
    asset_previews: HashMap<String, egui::TextureHandle>,
    events_rx: Option<Receiver<AppEvent>>,
    is_running: bool,
    agent_started_at: Option<Instant>,
    agent_user_message_index: Option<usize>,
    agent_chat_start_index: Option<usize>,
    agent_live_status: String,
    cancel: Option<Arc<AtomicBool>>,
    run_timeline: Option<RunTimeline>,
    run_timeline_anchor_index: Option<usize>,
    active_run_history: Option<AgentRunHistoryContext>,
    self_modification_guard: Option<SelfModificationGuard>,
    self_modification_status: String,
    agent_state: Arc<Mutex<AgentState>>,
    approvals: ApprovalMap,
    pending_approval: Option<PendingApproval>,
    pending_run_gate: Option<PendingRunGate>,
    workspace_mode: WorkspaceMode,
    right_panel_view: RightPanelView,
    file_panel_collapsed: bool,
    roadmap_filter: RoadmapFilter,
    roadmap_status: String,
    active_center_tab: CenterTab,
    file_tabs: Vec<FilePreviewTab>,
}

#[derive(Clone, Debug)]
struct PendingApproval {
    id: String,
    summary: String,
    detail: String,
}

#[derive(Clone, Debug)]
struct PendingRunGate {
    original_message: String,
    summary: String,
    detail: String,
}

#[derive(Clone, Debug)]
struct ContextProfilePreview {
    path: PathBuf,
    profile: ContextProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextPanelTab {
    Overview,
    Memory,
    Prompt,
    Profiles,
}

impl ContextPanelTab {
    const ALL: [ContextPanelTab; 4] = [
        ContextPanelTab::Overview,
        ContextPanelTab::Memory,
        ContextPanelTab::Prompt,
        ContextPanelTab::Profiles,
    ];

    fn label(self) -> &'static str {
        match self {
            ContextPanelTab::Overview => "Обзор",
            ContextPanelTab::Memory => "Память",
            ContextPanelTab::Prompt => "Prompt",
            ContextPanelTab::Profiles => "Профили",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            ContextPanelTab::Overview => "что попадёт в следующий запуск и где есть риск шума",
            ContextPanelTab::Memory => "закреплённые факты и источники проекта",
            ContextPanelTab::Prompt => "бюджет, retrieval и технический предпросмотр",
            ContextPanelTab::Profiles => "экспорт, импорт и перенос контекста между чатами",
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            ContextPanelTab::Overview => {
                "Краткая сводка контекста: здоровье, объём prompt, активный чат и быстрые закрепления."
            }
            ContextPanelTab::Memory => {
                "Постоянные заметки и источники проекта, которые агент может учитывать в следующих задачах."
            }
            ContextPanelTab::Prompt => {
                "Управление тем, сколько сообщений, найденного контекста и прошлых запусков попадёт в следующий запрос модели."
            }
            ContextPanelTab::Profiles => {
                "Экспорт и импорт набора контекста, чтобы переносить память между чатами или проектами."
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ApprovalQuickAction {
    Approve,
    Revise,
    AnalysisOnly,
    Restrict,
    Deny,
}

impl ApprovalQuickAction {
    fn label(self) -> &'static str {
        match self {
            Self::Approve => "Подтверждаю",
            Self::Revise => "Уточнить",
            Self::AnalysisOnly => "Только анализ",
            Self::Restrict => "Ограничить",
            Self::Deny => "Отклонить",
        }
    }

    fn user_reply(self) -> Option<&'static str> {
        match self {
            Self::Approve => None,
            Self::Revise => Some("Уточни план и предложи более безопасный вариант без запуска инструментов."),
            Self::AnalysisOnly => Some("Только анализ: не запускай инструменты и не меняй файлы, дай вывод и план."),
            Self::Restrict => Some("Продолжай с ограничениями: без записи в файлы и без shell-команд, если это возможно."),
            Self::Deny => None,
        }
    }

    fn approves(self) -> bool {
        matches!(self, Self::Approve)
    }
}

#[derive(Clone, Debug)]
struct InputAttachment {
    path: String,
    name: String,
    kind: InputAttachmentKind,
    bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputAttachmentKind {
    File,
    Image,
    Screenshot,
}

impl InputAttachmentKind {
    fn promote_image(self) -> Self {
        match self {
            Self::File => Self::Image,
            Self::Image | Self::Screenshot => self,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RightPanelView {
    Overview,
    Context,
    Roadmap,
    Release,
    Project,
    Assets,
    Control,
    Logs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceMode {
    Chat,
    Code,
    Assets,
    Project,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayoutPreset {
    ChatFocus,
    CodeFocus,
    RoadmapFocus,
    ReleaseFocus,
    AssetFocus,
}

#[derive(Clone, Debug)]
enum CommandPaletteAction {
    ApplyLayout(LayoutPreset),
    SetWorkspaceMode(WorkspaceMode),
    SetRightPanel(RightPanelView),
    ToggleFilePanel,
    OpenProject,
    RefreshWorkspace,
    NewChat,
    ResetConversation,
    SetPrompt(&'static str),
    StartProjectCommand(ProjectCommand),
    GitStatus,
    GitCommit,
    StopAgent,
    StopProjectCommand,
    RunMacro(String),
}

#[derive(Clone, Debug)]
struct CommandPaletteItem {
    id: String,
    title: String,
    category: &'static str,
    description: String,
    shortcut: Option<&'static str>,
    action: CommandPaletteAction,
    enabled: bool,
}

#[derive(Clone, Debug)]
struct PendingCommandMacroRun {
    name: String,
    command_ids: Vec<String>,
    index: usize,
    executed: usize,
    skipped: usize,
}

impl CommandPaletteItem {
    fn new(
        title: impl Into<String>,
        category: &'static str,
        description: impl Into<String>,
        shortcut: Option<&'static str>,
        action: CommandPaletteAction,
        enabled: bool,
    ) -> Self {
        let id = action.stable_id();
        Self {
            id,
            title: title.into(),
            category,
            description: description.into(),
            shortcut,
            action,
            enabled,
        }
    }

    fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {} {}",
            self.id,
            self.title,
            self.category,
            self.description,
            self.shortcut.unwrap_or_default()
        )
        .to_lowercase();
        query
            .split_whitespace()
            .all(|needle| haystack.contains(needle))
    }
}

impl CommandPaletteAction {
    fn stable_id(&self) -> String {
        match self {
            CommandPaletteAction::ApplyLayout(preset) => format!("layout:{}", preset.id()),
            CommandPaletteAction::SetWorkspaceMode(mode) => format!("mode:{}", mode.id()),
            CommandPaletteAction::SetRightPanel(view) => format!("panel:{}", view.id()),
            CommandPaletteAction::ToggleFilePanel => "view:toggle_file_panel".to_string(),
            CommandPaletteAction::OpenProject => "project:open".to_string(),
            CommandPaletteAction::RefreshWorkspace => "project:refresh".to_string(),
            CommandPaletteAction::NewChat => "agent:new_chat".to_string(),
            CommandPaletteAction::ResetConversation => "agent:reset_conversation".to_string(),
            CommandPaletteAction::SetPrompt(prompt) => {
                format!("prompt:{}", command_palette_hash_part(prompt))
            }
            CommandPaletteAction::StartProjectCommand(command) => {
                format!("project_command:{}", command_palette_id_part(&command.id))
            }
            CommandPaletteAction::GitStatus => "git:status".to_string(),
            CommandPaletteAction::GitCommit => "git:commit".to_string(),
            CommandPaletteAction::StopAgent => "agent:stop".to_string(),
            CommandPaletteAction::StopProjectCommand => "project:stop_command".to_string(),
            CommandPaletteAction::RunMacro(id) => format!("macro:{}", command_palette_id_part(id)),
        }
    }
}

fn command_palette_id_part(value: &str) -> String {
    let slug = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if ch == '-' || ch == '_' || ch == ':' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        format!("command-{}", command_palette_hash_part(value))
    } else {
        compact.chars().take(80).collect()
    }
}

fn command_palette_hash_part(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CenterTab {
    Agent,
    File(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileTreeFilter {
    All,
    Modified,
    Code,
    Assets,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoadmapFilter {
    All,
    Done,
    Now,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoadmapEntryState {
    Done,
    Now,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentHistoryStatusFilter {
    All,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentHistoryDurationFilter {
    All,
    Fast,
    Long,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentHistoryDateFilter {
    All,
    Today,
    Week,
}

impl FileTreeFilter {
    const ALL: [FileTreeFilter; 4] = [
        FileTreeFilter::All,
        FileTreeFilter::Modified,
        FileTreeFilter::Code,
        FileTreeFilter::Assets,
    ];

    fn label(self) -> &'static str {
        match self {
            FileTreeFilter::All => "Все",
            FileTreeFilter::Modified => "Изменённые",
            FileTreeFilter::Code => "Код",
            FileTreeFilter::Assets => "Ассеты",
        }
    }
}

impl RoadmapFilter {
    const ALL: [RoadmapFilter; 4] = [
        RoadmapFilter::All,
        RoadmapFilter::Done,
        RoadmapFilter::Now,
        RoadmapFilter::Next,
    ];

    fn label(self) -> &'static str {
        match self {
            RoadmapFilter::All => "Все",
            RoadmapFilter::Done => "Готово",
            RoadmapFilter::Now => "Сейчас",
            RoadmapFilter::Next => "Далее",
        }
    }
}

impl AgentHistoryStatusFilter {
    const ALL: [AgentHistoryStatusFilter; 4] = [
        AgentHistoryStatusFilter::All,
        AgentHistoryStatusFilter::Succeeded,
        AgentHistoryStatusFilter::Failed,
        AgentHistoryStatusFilter::Cancelled,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "Все",
            Self::Succeeded => "Готово",
            Self::Failed => "Ошибки",
            Self::Cancelled => "Отменено",
        }
    }

    fn matches(self, status: &str) -> bool {
        match self {
            Self::All => true,
            Self::Succeeded => status == "succeeded",
            Self::Failed => status == "failed",
            Self::Cancelled => status == "cancelled",
        }
    }
}

impl AgentHistoryDurationFilter {
    const ALL: [AgentHistoryDurationFilter; 3] = [
        AgentHistoryDurationFilter::All,
        AgentHistoryDurationFilter::Fast,
        AgentHistoryDurationFilter::Long,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "Любая",
            Self::Fast => "< 1 мин",
            Self::Long => ">= 1 мин",
        }
    }

    fn matches(self, duration_ms: u64) -> bool {
        match self {
            Self::All => true,
            Self::Fast => duration_ms < 60_000,
            Self::Long => duration_ms >= 60_000,
        }
    }
}

impl AgentHistoryDateFilter {
    const ALL: [AgentHistoryDateFilter; 3] = [
        AgentHistoryDateFilter::All,
        AgentHistoryDateFilter::Today,
        AgentHistoryDateFilter::Week,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "Всё время",
            Self::Today => "Сегодня",
            Self::Week => "7 дней",
        }
    }

    fn matches(self, started_at: u64) -> bool {
        let now = current_unix_timestamp();
        match self {
            Self::All => true,
            Self::Today => started_at >= unix_day_start(now),
            Self::Week => now.saturating_sub(started_at) <= 7 * 86_400,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenameRowAction {
    Commit,
    Cancel,
}

struct ProjectNavRowResponse {
    row: egui::Response,
    disclosure: egui::Response,
}

impl ProjectNavRowResponse {
    fn on_hover_text(self, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            row: self.row.on_hover_text(text),
            disclosure: self.disclosure,
        }
    }
}

struct FileTreeNavRowResponse {
    row: egui::Response,
    disclosure: Option<egui::Response>,
}

impl FileTreeNavRowResponse {
    fn disclosure_clicked(&self) -> bool {
        self.disclosure
            .as_ref()
            .map(|response| response.clicked())
            .unwrap_or(false)
    }

    fn on_hover_text(self, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            row: self.row.on_hover_text(text),
            disclosure: self.disclosure,
        }
    }
}

#[derive(Clone, Debug)]
struct FilePreviewTab {
    path: String,
    content: String,
    original_content: String,
    editable: bool,
    status: String,
}

#[derive(Clone, Debug)]
struct ProjectRunRecord {
    id: String,
    command: ProjectCommand,
    label: String,
    shell_command: String,
    started_at: u64,
    finished_at: Option<u64>,
    status: ProjectRunStatus,
    exit_code: Option<i32>,
    error_summary: Vec<String>,
    diagnostics: Vec<ProjectDiagnostic>,
    output_tail: String,
}

#[derive(Clone, Debug)]
struct ProjectDiagnostic {
    kind: ProjectDiagnosticKind,
    file: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
    message: String,
    raw: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectDiagnosticKind {
    Error,
    Warning,
    Panic,
    Failed,
}

#[derive(Clone, Debug)]
struct ProjectFixRequestRecord {
    id: String,
    run_id: String,
    run_label: String,
    target: String,
    requested_at: u64,
}

#[derive(Clone, Debug)]
struct ReleaseChecklistItem {
    title: String,
    detail: String,
    ok: bool,
}

#[derive(Clone, Debug)]
struct ReleaseArtifact {
    label: String,
    path: String,
    size_bytes: u64,
    modified_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectRunStatus {
    Running,
    Passed,
    Failed,
    Cancelled,
}

impl RightPanelView {
    fn label(self) -> &'static str {
        match self {
            RightPanelView::Overview => "Сводка",
            RightPanelView::Context => "Контекст",
            RightPanelView::Roadmap => "Roadmap",
            RightPanelView::Release => "Релиз",
            RightPanelView::Project => "Проект",
            RightPanelView::Assets => "Ассеты",
            RightPanelView::Control => "Контроль",
            RightPanelView::Logs => "Логи",
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            RightPanelView::Overview => {
                "Общая сводка состояния агента, проекта и быстрых переходов."
            }
            RightPanelView::Context => {
                "Контекст, память и prompt, которые агент использует при следующем запуске."
            }
            RightPanelView::Roadmap => {
                "Живая дорожная карта проекта: что сделано, что в работе и что запланировано."
            }
            RightPanelView::Release => {
                "Release cockpit: версия, preflight-чеклист, сборки, артефакты и готовность публикации."
            }
            RightPanelView::Project => {
                "Команды проекта, терминал, preview, рабочий стол и диагностика запусков."
            }
            RightPanelView::Assets => {
                "Генерация и библиотека ассетов: изображения, варианты, импорт и экспорт."
            }
            RightPanelView::Control => {
                "Разрешения, провайдеры, проверки, память, окружение и безопасное самоизменение."
            }
            RightPanelView::Logs => {
                "Журнал действий агента: инструменты, git, ошибки, трассировка и история запусков."
            }
        }
    }
}

impl WorkspaceMode {
    const ALL: [WorkspaceMode; 4] = [
        WorkspaceMode::Chat,
        WorkspaceMode::Code,
        WorkspaceMode::Assets,
        WorkspaceMode::Project,
    ];

    fn label(self) -> &'static str {
        match self {
            WorkspaceMode::Chat => "Чат",
            WorkspaceMode::Code => "Код",
            WorkspaceMode::Assets => "Ассеты",
            WorkspaceMode::Project => "Проект",
        }
    }

    fn id(self) -> &'static str {
        match self {
            WorkspaceMode::Chat => "chat",
            WorkspaceMode::Code => "code",
            WorkspaceMode::Assets => "assets",
            WorkspaceMode::Project => "project",
        }
    }

    fn from_id(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "code" => WorkspaceMode::Code,
            "assets" => WorkspaceMode::Assets,
            "project" => WorkspaceMode::Project,
            _ => WorkspaceMode::Chat,
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            WorkspaceMode::Chat => "диалог, разрешения и журнал агента",
            WorkspaceMode::Code => "файлы, правки, терминал и проверки",
            WorkspaceMode::Assets => "генерация, история и библиотека ассетов",
            WorkspaceMode::Project => "команды, preview, сборка и рабочий стол",
        }
    }

    fn default_panel(self) -> RightPanelView {
        match self {
            WorkspaceMode::Chat => RightPanelView::Context,
            WorkspaceMode::Code => RightPanelView::Project,
            WorkspaceMode::Assets => RightPanelView::Control,
            WorkspaceMode::Project => RightPanelView::Release,
        }
    }

    fn panels(self) -> &'static [RightPanelView] {
        match self {
            WorkspaceMode::Chat => &[
                RightPanelView::Overview,
                RightPanelView::Context,
                RightPanelView::Roadmap,
                RightPanelView::Control,
                RightPanelView::Logs,
            ],
            WorkspaceMode::Code => &[
                RightPanelView::Project,
                RightPanelView::Release,
                RightPanelView::Context,
                RightPanelView::Roadmap,
                RightPanelView::Control,
                RightPanelView::Logs,
            ],
            WorkspaceMode::Assets => &[
                RightPanelView::Control,
                RightPanelView::Context,
                RightPanelView::Logs,
            ],
            WorkspaceMode::Project => &[
                RightPanelView::Overview,
                RightPanelView::Release,
                RightPanelView::Context,
                RightPanelView::Roadmap,
                RightPanelView::Control,
                RightPanelView::Logs,
            ],
        }
    }
}

impl RightPanelView {
    fn id(self) -> &'static str {
        match self {
            RightPanelView::Overview => "overview",
            RightPanelView::Context => "context",
            RightPanelView::Roadmap => "roadmap",
            RightPanelView::Release => "release",
            RightPanelView::Project => "project",
            RightPanelView::Assets => "assets",
            RightPanelView::Control => "control",
            RightPanelView::Logs => "logs",
        }
    }

    fn from_id(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "overview" => RightPanelView::Overview,
            "roadmap" => RightPanelView::Roadmap,
            "release" => RightPanelView::Release,
            "project" => RightPanelView::Project,
            "assets" => RightPanelView::Assets,
            "control" => RightPanelView::Control,
            "logs" => RightPanelView::Logs,
            _ => RightPanelView::Context,
        }
    }
}

impl LayoutPreset {
    const ALL: [LayoutPreset; 5] = [
        LayoutPreset::ChatFocus,
        LayoutPreset::CodeFocus,
        LayoutPreset::RoadmapFocus,
        LayoutPreset::ReleaseFocus,
        LayoutPreset::AssetFocus,
    ];

    fn label(self) -> &'static str {
        match self {
            LayoutPreset::ChatFocus => "Фокус на чат",
            LayoutPreset::CodeFocus => "Код и файлы",
            LayoutPreset::RoadmapFocus => "Roadmap",
            LayoutPreset::ReleaseFocus => "Релиз",
            LayoutPreset::AssetFocus => "Ассеты",
        }
    }

    fn id(self) -> &'static str {
        match self {
            LayoutPreset::ChatFocus => "chat_focus",
            LayoutPreset::CodeFocus => "code_focus",
            LayoutPreset::RoadmapFocus => "roadmap_focus",
            LayoutPreset::ReleaseFocus => "release_focus",
            LayoutPreset::AssetFocus => "asset_focus",
        }
    }

    fn description(self) -> &'static str {
        match self {
            LayoutPreset::ChatFocus => "Сворачивает проводник и открывает контекст агента.",
            LayoutPreset::CodeFocus => {
                "Открывает проводник, кодовую область и проектные инструменты."
            }
            LayoutPreset::RoadmapFocus => "Открывает карту развития проекта рядом с чатом.",
            LayoutPreset::ReleaseFocus => "Открывает центр релиза, сборок и preflight.",
            LayoutPreset::AssetFocus => "Переходит в студию ассетов и настройки генерации.",
        }
    }

    fn state(self) -> (WorkspaceMode, RightPanelView, bool) {
        match self {
            LayoutPreset::ChatFocus => (WorkspaceMode::Chat, RightPanelView::Context, true),
            LayoutPreset::CodeFocus => (WorkspaceMode::Code, RightPanelView::Project, false),
            LayoutPreset::RoadmapFocus => (WorkspaceMode::Chat, RightPanelView::Roadmap, false),
            LayoutPreset::ReleaseFocus => (WorkspaceMode::Project, RightPanelView::Release, false),
            LayoutPreset::AssetFocus => (WorkspaceMode::Assets, RightPanelView::Control, false),
        }
    }
}

impl ProjectRunStatus {
    fn label(self) -> &'static str {
        match self {
            ProjectRunStatus::Running => "выполняется",
            ProjectRunStatus::Passed => "успешно",
            ProjectRunStatus::Failed => "ошибка",
            ProjectRunStatus::Cancelled => "остановлено",
        }
    }
}

impl ProjectDiagnosticKind {
    fn label(self) -> &'static str {
        match self {
            ProjectDiagnosticKind::Error => "error",
            ProjectDiagnosticKind::Warning => "warning",
            ProjectDiagnosticKind::Panic => "panic",
            ProjectDiagnosticKind::Failed => "failed",
        }
    }

    fn from_text(text: &str) -> Self {
        let lower = text.to_lowercase();
        if lower.contains("warning") {
            ProjectDiagnosticKind::Warning
        } else if lower.contains("panic") {
            ProjectDiagnosticKind::Panic
        } else if lower.contains("failed") {
            ProjectDiagnosticKind::Failed
        } else {
            ProjectDiagnosticKind::Error
        }
    }
}

impl LeetcodeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_app_theme(&cc.egui_ctx);

        let mut config = AppConfig::load();
        let workspace = config
            .last_workspace
            .clone()
            .and_then(|path| Workspace::new(path).ok());
        if let Some(workspace) = &workspace {
            config.remember_project(workspace.root().to_path_buf());
        }
        let file_rows = workspace
            .as_ref()
            .map(|workspace| workspace.ui_file_rows(600))
            .unwrap_or_default();
        let git_changed_files = workspace
            .as_ref()
            .map(git_changed_files_for_workspace)
            .unwrap_or_default();
        let asset_jobs = workspace.as_ref().map(load_jobs).unwrap_or_default();
        let project_profiles = workspace
            .as_ref()
            .map(detect_project_profiles)
            .unwrap_or_default();
        let provider_validation_results = workspace
            .as_ref()
            .map(|workspace| load_provider_validation_history(workspace).runs)
            .unwrap_or_default();

        let api_key_input = config.api_key.clone();
        let model_input = config.model.clone();
        let provider_input = config.provider.clone();
        let asset_provider_input = OPENAI_IMAGE_PROVIDER_ID.to_string();
        let asset_api_key_input = image_api_key_from_config(&config, &asset_provider_input);
        let asset_model_input = image_model_from_config(&config, &asset_provider_input);
        let journal_lines = read_journal_tail(200);
        let agent_history = workspace
            .as_ref()
            .map(|workspace| load_agent_history_tail(workspace, 80))
            .unwrap_or_default();
        let loaded_conversation = workspace.as_ref().map(load_active_conversation);
        let chat = loaded_conversation
            .as_ref()
            .map(|conversation| conversation.chat.clone())
            .unwrap_or_else(default_chat);
        let active_conversation_id = loaded_conversation
            .as_ref()
            .map(|conversation| conversation.id.clone());
        let conversation_index = loaded_conversation
            .as_ref()
            .map(|conversation| conversation.index.clone())
            .unwrap_or_default();
        let conversation_status = loaded_conversation
            .as_ref()
            .map(|conversation| conversation.status.clone())
            .unwrap_or_default();
        let restored_agent_state = loaded_conversation
            .as_ref()
            .and_then(|conversation| conversation.state.agent_state.clone())
            .unwrap_or_default();
        let approvals = Arc::new(Mutex::new(HashMap::new()));
        let workspace_mode = WorkspaceMode::from_id(&config.layout_workspace_mode);
        let mut right_panel_view = RightPanelView::from_id(&config.layout_right_panel_view);
        if !workspace_mode.panels().contains(&right_panel_view) {
            right_panel_view = workspace_mode.default_panel();
        }
        let file_panel_collapsed = config.layout_file_panel_collapsed;

        Self {
            config,
            provider_input,
            api_key_input,
            model_input,
            workspace,
            file_rows,
            selected_tree_path: None,
            project_rename_target: None,
            project_rename_input: String::new(),
            file_search_input: String::new(),
            file_filter: FileTreeFilter::All,
            git_changed_files,
            file_rename_target: None,
            file_rename_input: String::new(),
            last_file_click_path: None,
            last_file_click_time: 0.0,
            dragged_tree_path: None,
            file_operation_status: String::new(),
            selected_file: None,
            selected_preview: String::new(),
            original_file_content: String::new(),
            selected_file_editable: false,
            editor_status: String::new(),
            input: String::new(),
            input_attachments: Vec::new(),
            input_attachment_status: String::new(),
            input_paste_shortcut_down: false,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_selected: 0,
            command_macro_name_input: String::new(),
            command_macro_edit_target: None,
            command_macro_edit_name: String::new(),
            command_macro_edit_description: String::new(),
            command_macro_import_input: String::new(),
            command_palette_status: String::new(),
            pending_command_macro_run: None,
            chat,
            active_conversation_id,
            conversation_index,
            conversation_status,
            conversation_rename_target: None,
            conversation_rename_input: String::new(),
            context_inspector_query: String::new(),
            context_notes: loaded_conversation
                .as_ref()
                .map(|conversation| conversation.state.context_notes.clone())
                .unwrap_or_default(),
            context_note_input: String::new(),
            context_note_suggestions: Vec::new(),
            context_profile_preview: None,
            context_health_status: String::new(),
            context_panel_tab: ContextPanelTab::Overview,
            tool_log: Vec::new(),
            journal_lines,
            journal_status: String::new(),
            agent_history,
            agent_history_status: String::new(),
            agent_history_query: String::new(),
            agent_history_status_filter: AgentHistoryStatusFilter::All,
            agent_history_duration_filter: AgentHistoryDurationFilter::All,
            agent_history_date_filter: AgentHistoryDateFilter::All,
            selected_agent_history_id: None,
            git_summary: String::new(),
            git_action_status: String::new(),
            git_commit_dialog_open: false,
            git_commit_message_input: String::new(),
            project_profiles,
            project_events_rx: None,
            project_is_running: false,
            project_cancel: None,
            project_status: String::new(),
            project_task_title_input: String::new(),
            project_task_workstream_input: String::new(),
            project_task_milestone_input: String::new(),
            project_task_priority_input: "normal".to_string(),
            last_project_command: None,
            project_runs: Vec::new(),
            project_fix_requests: Vec::new(),
            active_project_run_id: None,
            desktop_status: String::new(),
            desktop_last_screenshot: None,
            desktop_active_window: String::new(),
            terminal_input: String::new(),
            terminal_output: String::new(),
            terminal_status: String::new(),
            terminal_running: false,
            governance_status: String::new(),
            governance_pattern_input: String::new(),
            memory_goal_input: String::new(),
            memory_task_input: String::new(),
            memory_decision_input: String::new(),
            memory_source_title_input: String::new(),
            memory_source_note_input: String::new(),
            memory_status: String::new(),
            asset_library_filter: String::new(),
            asset_library_status: String::new(),
            eval_status: String::new(),
            provider_health_status: String::new(),
            provider_validation_rx: None,
            provider_validation_running: false,
            provider_validation_results,
            orchestration_status: String::new(),
            asset_provider_input,
            asset_kind_input: "image".to_string(),
            asset_api_key_input,
            asset_model_input,
            asset_prompt: String::new(),
            asset_aspect_ratio: "1:1".to_string(),
            asset_image_size: "1K".to_string(),
            asset_jobs,
            asset_events_rx: None,
            asset_is_running: false,
            asset_status: String::new(),
            asset_compare_paths: Vec::new(),
            asset_import_target_input: "assets/images".to_string(),
            asset_previews: HashMap::new(),
            events_rx: None,
            is_running: false,
            agent_started_at: None,
            agent_user_message_index: None,
            agent_chat_start_index: None,
            agent_live_status: String::new(),
            cancel: None,
            run_timeline: None,
            run_timeline_anchor_index: None,
            active_run_history: None,
            self_modification_guard: None,
            self_modification_status: String::new(),
            agent_state: Arc::new(Mutex::new(restored_agent_state)),
            approvals,
            pending_approval: None,
            pending_run_gate: None,
            workspace_mode,
            right_panel_view,
            file_panel_collapsed,
            roadmap_filter: RoadmapFilter::All,
            roadmap_status: String::new(),
            active_center_tab: CenterTab::Agent,
            file_tabs: Vec::new(),
        }
    }

    fn choose_workspace(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };

        let _ = self.open_workspace_path(path);
    }

    fn open_workspace_path(&mut self, path: PathBuf) -> bool {
        match Workspace::new(path.clone()) {
            Ok(workspace) => {
                self.sync_config_from_inputs();
                self.sync_asset_provider_settings();
                let canonical_path = workspace.root().to_path_buf();
                self.config.last_workspace = Some(canonical_path.clone());
                self.config.remember_project(canonical_path);
                self.provider_validation_results =
                    load_provider_validation_history(&workspace).runs;
                self.workspace = Some(workspace);
                self.refresh_agent_history();
                self.refresh_file_rows();
                self.refresh_project_profiles();
                self.asset_jobs = self.workspace.as_ref().map(load_jobs).unwrap_or_default();
                self.asset_compare_paths.clear();
                self.asset_previews.clear();
                self.selected_tree_path = None;
                self.project_rename_target = None;
                self.project_rename_input.clear();
                self.file_rename_target = None;
                self.file_rename_input.clear();
                self.last_file_click_path = None;
                self.last_file_click_time = 0.0;
                self.dragged_tree_path = None;
                self.file_operation_status.clear();
                self.selected_file = None;
                self.selected_preview.clear();
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status.clear();
                self.file_tabs.clear();
                self.active_center_tab = CenterTab::Agent;
                self.refresh_git_summary();
                if let Some(workspace) = &self.workspace {
                    let loaded = load_active_conversation(workspace);
                    self.apply_loaded_conversation(loaded);
                }
                let _ = self.config.save();
                true
            }
            Err(err) => {
                self.chat.push(ChatLine::system(format!(
                    "Не удалось открыть рабочую папку: {err}"
                )));
                false
            }
        }
    }

    fn apply_loaded_conversation(&mut self, loaded: LoadedConversation) {
        let restored_agent_state = loaded.state.agent_state.clone().unwrap_or_default();
        self.active_conversation_id = Some(loaded.id);
        self.conversation_index = loaded.index;
        self.conversation_status = loaded.status;
        self.chat = loaded.chat;
        self.context_notes = loaded.state.context_notes;
        self.conversation_rename_target = None;
        self.conversation_rename_input.clear();
        self.context_note_input.clear();
        self.context_note_suggestions.clear();
        *self.agent_state.lock().expect("agent state poisoned") = restored_agent_state;
        self.agent_started_at = None;
        self.agent_user_message_index = None;
        self.agent_chat_start_index = None;
        self.agent_live_status.clear();
        self.run_timeline = None;
        self.run_timeline_anchor_index = None;
        self.active_run_history = None;
        self.self_modification_guard = None;
        self.self_modification_status.clear();
        self.pending_run_gate = None;
        self.pending_approval = None;
        self.is_running = false;
        self.cancel = None;
    }

    fn persist_current_conversation(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let conversation_id = if let Some(id) = &self.active_conversation_id {
            id.clone()
        } else {
            let loaded = load_active_conversation(&workspace);
            let id = loaded.id.clone();
            self.apply_loaded_conversation(loaded);
            id
        };
        let agent_state = self
            .agent_state
            .lock()
            .expect("agent state poisoned")
            .clone();
        match save_conversation_snapshot(
            &workspace,
            &conversation_id,
            &self.chat,
            Some(agent_state),
        ) {
            Ok(state) => {
                self.conversation_index = load_conversation_index(&workspace);
                self.conversation_status = format!(
                    "чат сохранён · {} сообщений · summary {} символов",
                    self.chat.len(),
                    state.rolling_summary.chars().count()
                );
            }
            Err(err) => {
                self.conversation_status = format!("не удалось сохранить чат: {err}");
                append_journal(format!("conversation\terror\t{err}"));
            }
        }
    }

    fn create_new_chat(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            self.chat = default_chat();
            self.active_conversation_id = None;
            self.conversation_status = "рабочая папка не выбрана".to_string();
            return;
        };

        self.persist_current_conversation();
        match create_new_conversation(&workspace) {
            Ok(loaded) => self.apply_loaded_conversation(loaded),
            Err(err) => {
                self.conversation_status = format!("не удалось создать новый чат: {err}");
            }
        }
    }

    fn switch_conversation(&mut self, conversation_id: String) {
        if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
            return;
        }
        let Some(workspace) = self.workspace.clone() else {
            return;
        };

        self.persist_current_conversation();
        let mut index = load_conversation_index(&workspace);
        index.active_id = Some(conversation_id.clone());
        let _ = save_conversation_index(&workspace, &index);
        let mut state = load_conversation_state(&workspace);
        state.active_conversation_id = Some(conversation_id);
        state.agent_state = None;
        let _ = save_conversation_state(&workspace, &state);
        let loaded = load_active_conversation(&workspace);
        self.apply_loaded_conversation(loaded);
    }

    fn begin_rename_active_conversation(&mut self) {
        let Some(active_id) = self.active_conversation_id.clone() else {
            return;
        };
        self.conversation_rename_target = Some(active_id);
        self.conversation_rename_input = self.active_conversation_title();
    }

    fn save_active_conversation_title(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(active_id) = self.active_conversation_id.clone() else {
            return;
        };
        let title = self.conversation_rename_input.trim().to_string();
        match rename_conversation(&workspace, &active_id, &title) {
            Ok(index) => {
                self.conversation_index = index;
                self.conversation_status = "чат переименован".to_string();
                self.conversation_rename_target = None;
                self.conversation_rename_input.clear();
            }
            Err(err) => {
                self.conversation_status = format!("не удалось переименовать чат: {err}");
            }
        }
    }

    fn toggle_active_conversation_pin(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(active_id) = self.active_conversation_id.clone() else {
            return;
        };
        let pinned = self
            .conversation_index
            .conversations
            .iter()
            .find(|meta| meta.id == active_id)
            .map(|meta| !meta.pinned)
            .unwrap_or(true);
        match set_conversation_pinned(&workspace, &active_id, pinned) {
            Ok(index) => {
                self.conversation_index = index;
                self.conversation_status = if pinned {
                    "чат закреплён".to_string()
                } else {
                    "чат откреплён".to_string()
                };
            }
            Err(err) => {
                self.conversation_status = format!("не удалось изменить закрепление: {err}");
            }
        }
    }

    fn archive_active_conversation(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(active_id) = self.active_conversation_id.clone() else {
            return;
        };
        self.persist_current_conversation();
        match archive_conversation(&workspace, &active_id) {
            Ok(loaded) => self.apply_loaded_conversation(loaded),
            Err(err) => {
                self.conversation_status = format!("не удалось архивировать чат: {err}");
            }
        }
    }

    fn restore_conversation_from_archive(&mut self, conversation_id: String) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        self.persist_current_conversation();
        match restore_conversation(&workspace, &conversation_id) {
            Ok(loaded) => self.apply_loaded_conversation(loaded),
            Err(err) => {
                self.conversation_status = format!("не удалось восстановить чат: {err}");
            }
        }
    }

    fn delete_active_conversation(&mut self) {
        let Some(active_id) = self.active_conversation_id.clone() else {
            return;
        };
        self.delete_conversation_by_id(active_id);
    }

    fn delete_conversation_by_id(&mut self, conversation_id: String) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        match delete_conversation(&workspace, &conversation_id) {
            Ok(loaded) => self.apply_loaded_conversation(loaded),
            Err(err) => {
                self.conversation_status = format!("не удалось удалить чат: {err}");
            }
        }
    }

    fn set_workspace_mode(&mut self, mode: WorkspaceMode) {
        if self.workspace_mode != mode {
            self.workspace_mode = mode;
            self.right_panel_view = mode.default_panel();
            self.persist_layout_state();
        }
    }

    fn set_file_panel_collapsed(&mut self, collapsed: bool) {
        if self.file_panel_collapsed != collapsed {
            self.file_panel_collapsed = collapsed;
            self.persist_layout_state();
        }
    }

    fn apply_layout_preset(&mut self, preset: LayoutPreset) {
        let (workspace_mode, right_panel_view, file_panel_collapsed) = preset.state();
        self.workspace_mode = workspace_mode;
        self.right_panel_view = right_panel_view;
        self.file_panel_collapsed = file_panel_collapsed;
        self.persist_layout_state();
    }

    fn persist_layout_state(&mut self) {
        self.config.layout_workspace_mode = self.workspace_mode.id().to_string();
        self.config.layout_right_panel_view = self.right_panel_view.id().to_string();
        self.config.layout_file_panel_collapsed = self.file_panel_collapsed;
        if let Err(err) = self.config.save() {
            self.journal_status = format!("не удалось сохранить вид интерфейса: {err}");
        }
    }

    fn open_command_palette(&mut self) {
        self.command_palette_open = true;
        self.command_palette_query.clear();
        self.command_palette_selected = 0;
    }

    fn handle_command_palette_shortcuts(&mut self, ctx: &egui::Context) {
        let open_pressed = ctx.input(|input| {
            let command_modifier =
                input.modifiers.ctrl || input.modifiers.command || input.modifiers.mac_cmd;
            command_modifier
                && (input.key_pressed(egui::Key::K)
                    || (input.key_pressed(egui::Key::P) && input.modifiers.shift))
        });
        if open_pressed {
            self.open_command_palette();
            return;
        }

        if !self.command_palette_open {
            return;
        }

        let (escape, up, down, enter) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::Enter),
            )
        });
        if escape {
            self.command_palette_open = false;
            return;
        }

        let count = self.filtered_command_palette_items().len();
        if count == 0 {
            self.command_palette_selected = 0;
            return;
        }
        if down {
            self.command_palette_selected = (self.command_palette_selected + 1).min(count - 1);
        }
        if up {
            self.command_palette_selected = self.command_palette_selected.saturating_sub(1);
        }
        if enter {
            let items = self.filtered_command_palette_items();
            if let Some(item) = items.get(self.command_palette_selected).cloned() {
                if item.enabled {
                    self.execute_command_palette_item(item);
                    self.command_palette_open = false;
                }
            }
        }
    }

    fn command_palette_items(&self) -> Vec<CommandPaletteItem> {
        let workspace_ready = self.workspace.is_some();
        let mut items = vec![
            CommandPaletteItem::new(
                "Фокус на чат",
                "Вид",
                "Свернуть проводник и открыть контекст агента.",
                Some("Ctrl+K"),
                CommandPaletteAction::ApplyLayout(LayoutPreset::ChatFocus),
                true,
            ),
            CommandPaletteItem::new(
                "Код и файлы",
                "Вид",
                "Открыть проводник, вкладку кода и проектные инструменты.",
                None,
                CommandPaletteAction::ApplyLayout(LayoutPreset::CodeFocus),
                true,
            ),
            CommandPaletteItem::new(
                "Roadmap",
                "Вид",
                "Показать живую дорожную карту проекта.",
                None,
                CommandPaletteAction::ApplyLayout(LayoutPreset::RoadmapFocus),
                true,
            ),
            CommandPaletteItem::new(
                "Релиз",
                "Вид",
                "Открыть release cockpit и preflight.",
                None,
                CommandPaletteAction::ApplyLayout(LayoutPreset::ReleaseFocus),
                true,
            ),
            CommandPaletteItem::new(
                "Ассеты",
                "Вид",
                "Перейти в студию ассетов.",
                None,
                CommandPaletteAction::ApplyLayout(LayoutPreset::AssetFocus),
                true,
            ),
            CommandPaletteItem::new(
                if self.file_panel_collapsed {
                    "Показать проводник"
                } else {
                    "Свернуть проводник"
                },
                "Вид",
                "Скрыть или вернуть левую область проектов.",
                None,
                CommandPaletteAction::ToggleFilePanel,
                true,
            ),
            CommandPaletteItem::new(
                "Открыть проект",
                "Проект",
                "Выбрать рабочую папку проекта.",
                None,
                CommandPaletteAction::OpenProject,
                !self.is_running,
            ),
            CommandPaletteItem::new(
                "Обновить проект",
                "Проект",
                "Обновить дерево файлов, Git-сводку и профили команд.",
                None,
                CommandPaletteAction::RefreshWorkspace,
                workspace_ready,
            ),
            CommandPaletteItem::new(
                "Новый чат",
                "Агент",
                "Создать новый диалог в текущем проекте.",
                None,
                CommandPaletteAction::NewChat,
                true,
            ),
            CommandPaletteItem::new(
                "Сбросить диалог",
                "Агент",
                "Очистить текущий диалог и агентное состояние.",
                None,
                CommandPaletteAction::ResetConversation,
                !self.is_running,
            ),
            CommandPaletteItem::new(
                "Остановить агента",
                "Агент",
                "Запросить остановку текущего агентного запуска.",
                None,
                CommandPaletteAction::StopAgent,
                self.is_running,
            ),
            CommandPaletteItem::new(
                "Остановить команду проекта",
                "Проект",
                "Остановить текущий shell-запуск проекта.",
                None,
                CommandPaletteAction::StopProjectCommand,
                self.project_is_running,
            ),
            CommandPaletteItem::new(
                "Git status",
                "Git",
                "Обновить Git-сводку и открыть журнал.",
                None,
                CommandPaletteAction::GitStatus,
                workspace_ready,
            ),
            CommandPaletteItem::new(
                "Git commit",
                "Git",
                "Открыть окно комментария коммита.",
                None,
                CommandPaletteAction::GitCommit,
                workspace_ready,
            ),
            CommandPaletteItem::new(
                "Открыть контекст",
                "Панели",
                "Показать, что агент помнит и что попадёт в prompt.",
                None,
                CommandPaletteAction::SetRightPanel(RightPanelView::Context),
                true,
            ),
            CommandPaletteItem::new(
                "Открыть контроль",
                "Панели",
                "Разрешения, провайдеры, проверки и окружение.",
                None,
                CommandPaletteAction::SetRightPanel(RightPanelView::Control),
                true,
            ),
            CommandPaletteItem::new(
                "Открыть логи",
                "Панели",
                "Журнал действий агента, git и история запусков.",
                None,
                CommandPaletteAction::SetRightPanel(RightPanelView::Logs),
                true,
            ),
            CommandPaletteItem::new(
                "Проверить проект",
                "Prompt",
                "Подставить запрос на безопасную проверку проекта.",
                None,
                CommandPaletteAction::SetPrompt(
                    "Проверь текущий проект: определи тип, запусти безопасные проверки и дай краткий статус.",
                ),
                true,
            ),
            CommandPaletteItem::new(
                "Следующий шаг",
                "Prompt",
                "Подставить запрос на анализ roadmap и backlog.",
                None,
                CommandPaletteAction::SetPrompt(
                    "Посмотри roadmap, backlog и состояние проекта. Предложи следующий самый полезный шаг.",
                ),
                true,
            ),
            CommandPaletteItem::new(
                "Релизный preflight",
                "Prompt",
                "Подставить запрос на проверку перед публикацией.",
                None,
                CommandPaletteAction::SetPrompt(
                    "Проведи релизный preflight: проверь версию, готовность, команды сборки, артефакты и риски перед публикацией.",
                ),
                true,
            ),
            CommandPaletteItem::new(
                "Зафиксировать milestone",
                "Prompt",
                "Подставить запрос на запись этапа в Roadmap.",
                None,
                CommandPaletteAction::SetPrompt(
                    "Подготовь краткий milestone для Roadmap: что сделано, какие файлы затронуты, что осталось дальше.",
                ),
                true,
            ),
            CommandPaletteItem::new(
                "Создать ассет",
                "Prompt",
                "Подставить запрос на подготовку ассета.",
                None,
                CommandPaletteAction::SetPrompt(
                    "Помоги подготовить ассет для текущего проекта: уточни назначение, формат и предложи prompt для генерации.",
                ),
                true,
            ),
        ];

        for mode in WorkspaceMode::ALL {
            items.push(CommandPaletteItem::new(
                format!("Перейти: {}", mode.label()),
                "Навигация",
                mode.subtitle(),
                None,
                CommandPaletteAction::SetWorkspaceMode(mode),
                true,
            ));
        }

        for profile in &self.project_profiles {
            for command in &profile.commands {
                items.push(CommandPaletteItem::new(
                    format!("Запустить: {} · {}", command.label, profile.kind),
                    "Команды проекта",
                    format!("{} · {}", command.description, command.command),
                    None,
                    CommandPaletteAction::StartProjectCommand(command.clone()),
                    workspace_ready && !self.project_is_running,
                ));
            }
        }

        for command_macro in &self.config.command_palette_macros {
            let command_count = command_macro.command_ids.len();
            items.push(CommandPaletteItem::new(
                command_macro.name.clone(),
                "Макросы",
                if command_macro.description.trim().is_empty() {
                    format!("{command_count} команд из избранного")
                } else {
                    command_macro.description.clone()
                },
                None,
                CommandPaletteAction::RunMacro(command_macro.id.clone()),
                command_count > 0,
            ));
        }

        items
    }

    fn filtered_command_palette_items(&self) -> Vec<CommandPaletteItem> {
        let mut items = self
            .command_palette_items()
            .into_iter()
            .filter(|item| item.matches_query(&self.command_palette_query))
            .collect::<Vec<_>>();
        items.sort_by_key(|item| self.command_palette_item_rank(item));
        items
    }

    fn command_palette_item_rank(&self, item: &CommandPaletteItem) -> (u8, usize, String, String) {
        let recent_rank = self
            .config
            .command_palette_recent
            .iter()
            .position(|id| id == &item.id)
            .unwrap_or(usize::MAX);
        let bucket = if self.command_palette_is_favorite(&item.id) {
            0
        } else if item.id.starts_with("macro:") {
            1
        } else if recent_rank != usize::MAX {
            2
        } else {
            3
        };
        (
            bucket,
            recent_rank,
            item.category.to_lowercase(),
            item.title.to_lowercase(),
        )
    }

    fn command_palette_is_favorite(&self, command_id: &str) -> bool {
        self.config
            .command_palette_favorites
            .iter()
            .any(|id| id == command_id)
    }

    fn persist_command_palette_state(&mut self) {
        if let Err(err) = self.config.save() {
            self.command_palette_status = format!("Не удалось сохранить палитру команд: {err}");
        }
    }

    fn record_command_palette_use(&mut self, command_id: &str) {
        let command_id = command_id.trim();
        if command_id.is_empty() {
            return;
        }
        self.config
            .command_palette_recent
            .retain(|existing| existing != command_id);
        self.config
            .command_palette_recent
            .insert(0, command_id.to_string());
        self.config.command_palette_recent.truncate(24);
        self.persist_command_palette_state();
    }

    fn toggle_command_palette_favorite(&mut self, command_id: &str) {
        let command_id = command_id.trim();
        if command_id.is_empty() {
            return;
        }
        if let Some(index) = self
            .config
            .command_palette_favorites
            .iter()
            .position(|id| id == command_id)
        {
            self.config.command_palette_favorites.remove(index);
            self.command_palette_status = "Команда убрана из избранного".to_string();
        } else {
            self.config
                .command_palette_favorites
                .push(command_id.to_string());
            self.config.command_palette_favorites.truncate(80);
            self.command_palette_status = "Команда добавлена в избранное".to_string();
        }
        self.persist_command_palette_state();
    }

    fn find_command_palette_item_by_id(&self, command_id: &str) -> Option<CommandPaletteItem> {
        self.command_palette_items()
            .into_iter()
            .find(|item| item.id == command_id)
    }

    fn create_command_macro_from_favorites(&mut self) {
        let name = self.command_macro_name_input.trim().to_string();
        if name.is_empty() {
            self.command_palette_status = "Введите название макроса".to_string();
            return;
        }

        let command_ids = self
            .config
            .command_palette_favorites
            .iter()
            .filter(|id| !id.starts_with("macro:"))
            .filter(|id| self.find_command_palette_item_by_id(id).is_some())
            .take(12)
            .cloned()
            .collect::<Vec<_>>();
        if command_ids.is_empty() {
            self.command_palette_status =
                "Сначала добавьте в избранное хотя бы одну обычную команду".to_string();
            return;
        }

        let mut id = command_palette_id_part(&name);
        if id.starts_with("command-") {
            id = format!("macro-{}", command_palette_hash_part(&name));
        }
        let base_id = id.clone();
        let mut suffix = 2;
        while self
            .config
            .command_palette_macros
            .iter()
            .any(|command_macro| command_macro.id == id)
        {
            id = format!("{base_id}-{suffix}");
            suffix += 1;
        }

        self.config
            .command_palette_macros
            .push(CommandPaletteMacro {
                id,
                name: name.clone(),
                description: format!("{} команд из избранного", command_ids.len()),
                confirm_each_step: false,
                command_ids,
            });
        self.command_macro_name_input.clear();
        self.command_palette_status = format!("Макрос создан: {name}");
        self.persist_command_palette_state();
    }

    fn delete_command_macro(&mut self, macro_id: &str) {
        let before = self.config.command_palette_macros.len();
        self.config
            .command_palette_macros
            .retain(|command_macro| command_macro.id != macro_id);
        self.config
            .command_palette_favorites
            .retain(|id| id != &format!("macro:{macro_id}"));
        self.config
            .command_palette_recent
            .retain(|id| id != &format!("macro:{macro_id}"));
        if self.config.command_palette_macros.len() != before {
            self.command_palette_status = "Макрос удалён".to_string();
            self.persist_command_palette_state();
        }
    }

    fn begin_edit_command_macro(&mut self, macro_id: &str) {
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter()
            .find(|command_macro| command_macro.id == macro_id)
            .cloned()
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };
        self.command_macro_edit_target = Some(command_macro.id);
        self.command_macro_edit_name = command_macro.name;
        self.command_macro_edit_description = command_macro.description;
        self.command_palette_status = "Макрос открыт для редактирования".to_string();
    }

    fn save_command_macro_edit(&mut self) {
        let Some(macro_id) = self.command_macro_edit_target.clone() else {
            self.command_palette_status = "Макрос для редактирования не выбран".to_string();
            return;
        };
        let name = self.command_macro_edit_name.trim().to_string();
        if name.is_empty() {
            self.command_palette_status = "Название макроса не может быть пустым".to_string();
            return;
        }
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter_mut()
            .find(|command_macro| command_macro.id == macro_id)
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };
        command_macro.name = name;
        command_macro.description = self.command_macro_edit_description.trim().to_string();
        self.command_palette_status = "Макрос сохранён".to_string();
        self.persist_command_palette_state();
    }

    fn cancel_command_macro_edit(&mut self) {
        self.command_macro_edit_target = None;
        self.command_macro_edit_name.clear();
        self.command_macro_edit_description.clear();
    }

    fn move_command_macro_step(&mut self, macro_id: &str, index: usize, delta: isize) {
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter_mut()
            .find(|command_macro| command_macro.id == macro_id)
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };
        if index >= command_macro.command_ids.len() {
            return;
        }
        let new_index = if delta < 0 {
            index.saturating_sub(1)
        } else {
            (index + 1).min(command_macro.command_ids.len().saturating_sub(1))
        };
        if new_index != index {
            command_macro.command_ids.swap(index, new_index);
            self.command_palette_status = "Порядок шагов обновлён".to_string();
            self.persist_command_palette_state();
        }
    }

    fn remove_command_macro_step(&mut self, macro_id: &str, index: usize) {
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter_mut()
            .find(|command_macro| command_macro.id == macro_id)
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };
        if index < command_macro.command_ids.len() {
            command_macro.command_ids.remove(index);
            self.command_palette_status = "Шаг удалён".to_string();
            self.persist_command_palette_state();
        }
    }

    fn toggle_command_macro_confirmation(&mut self, macro_id: &str) {
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter_mut()
            .find(|command_macro| command_macro.id == macro_id)
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };
        command_macro.confirm_each_step = !command_macro.confirm_each_step;
        self.command_palette_status = if command_macro.confirm_each_step {
            "Подтверждение шагов включено".to_string()
        } else {
            "Подтверждение шагов выключено".to_string()
        };
        self.persist_command_palette_state();
    }

    fn export_command_macro_to_clipboard(&mut self, macro_id: &str) {
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter()
            .find(|command_macro| command_macro.id == macro_id)
            .cloned()
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };
        let Ok(json) = serde_json::to_string_pretty(&command_macro) else {
            self.command_palette_status = "Не удалось экспортировать макрос".to_string();
            return;
        };
        self.command_macro_import_input = json.clone();
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(json)) {
            Ok(()) => self.command_palette_status = "Макрос скопирован в буфер".to_string(),
            Err(err) => {
                self.command_palette_status =
                    format!("Буфер недоступен, JSON оставлен в поле импорта: {err}");
            }
        }
    }

    fn import_command_macro_from_input(&mut self) {
        let text = self.command_macro_import_input.trim();
        if text.is_empty() {
            self.command_palette_status = "Вставьте JSON макроса для импорта".to_string();
            return;
        }
        let Ok(mut command_macro) = serde_json::from_str::<CommandPaletteMacro>(text) else {
            self.command_palette_status = "JSON макроса не распознан".to_string();
            return;
        };
        command_macro.name = command_macro.name.trim().to_string();
        command_macro.description = command_macro.description.trim().to_string();
        command_macro.command_ids = command_macro
            .command_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty() && !id.starts_with("macro:"))
            .filter(|id| self.find_command_palette_item_by_id(id).is_some())
            .take(12)
            .collect();
        if command_macro.name.is_empty() || command_macro.command_ids.is_empty() {
            self.command_palette_status =
                "Макрос должен иметь название и хотя бы один существующий шаг".to_string();
            return;
        }
        command_macro.id = self.unique_command_macro_id(&command_macro.id, &command_macro.name);
        let imported_name = command_macro.name.clone();
        self.config.command_palette_macros.push(command_macro);
        self.command_macro_import_input.clear();
        self.command_palette_status = format!("Макрос импортирован: {imported_name}");
        self.persist_command_palette_state();
    }

    fn unique_command_macro_id(&self, id: &str, name: &str) -> String {
        let mut candidate = if id.trim().is_empty() {
            command_palette_id_part(name)
        } else {
            command_palette_id_part(id)
        };
        if candidate.starts_with("command-") {
            candidate = format!("macro-{}", command_palette_hash_part(name));
        }
        let base = candidate.clone();
        let mut suffix = 2;
        while self
            .config
            .command_palette_macros
            .iter()
            .any(|command_macro| command_macro.id == candidate)
        {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        candidate
    }

    fn execute_command_palette_item(&mut self, item: CommandPaletteItem) {
        self.record_command_palette_use(&item.id);
        self.execute_command_palette_action(item.action);
    }

    fn execute_command_palette_macro(&mut self, macro_id: &str) {
        let Some(command_macro) = self
            .config
            .command_palette_macros
            .iter()
            .find(|command_macro| command_macro.id == macro_id)
            .cloned()
        else {
            self.command_palette_status = "Макрос не найден".to_string();
            return;
        };

        if command_macro.confirm_each_step {
            self.pending_command_macro_run = Some(PendingCommandMacroRun {
                name: command_macro.name.clone(),
                command_ids: command_macro.command_ids.clone(),
                index: 0,
                executed: 0,
                skipped: 0,
            });
            self.command_palette_status =
                format!("Макрос ждёт подтверждения: {}", command_macro.name);
            return;
        }

        let mut executed = 0;
        let mut skipped = 0;
        for command_id in &command_macro.command_ids {
            match self.find_command_palette_item_by_id(command_id) {
                Some(item) if item.enabled => {
                    self.execute_command_palette_action(item.action);
                    executed += 1;
                }
                _ => skipped += 1,
            }
        }
        self.command_palette_status = if skipped == 0 {
            format!("Макрос выполнен: {} ({executed})", command_macro.name)
        } else {
            format!(
                "Макрос выполнен: {} ({executed}, пропущено {skipped})",
                command_macro.name
            )
        };
    }

    fn advance_pending_command_macro(&mut self, execute_step: bool) {
        let Some(mut run) = self.pending_command_macro_run.take() else {
            return;
        };
        if run.index >= run.command_ids.len() {
            self.command_palette_status = format!(
                "Макрос завершён: {} (выполнено {}, пропущено {})",
                run.name, run.executed, run.skipped
            );
            return;
        }

        let command_id = run.command_ids[run.index].clone();
        if execute_step {
            match self.find_command_palette_item_by_id(&command_id) {
                Some(item) if item.enabled => {
                    self.execute_command_palette_action(item.action);
                    run.executed += 1;
                }
                _ => run.skipped += 1,
            }
        } else {
            run.skipped += 1;
        }
        run.index += 1;

        if run.index >= run.command_ids.len() {
            self.command_palette_status = format!(
                "Макрос завершён: {} (выполнено {}, пропущено {})",
                run.name, run.executed, run.skipped
            );
        } else {
            self.pending_command_macro_run = Some(run);
        }
    }

    fn cancel_pending_command_macro(&mut self) {
        if let Some(run) = self.pending_command_macro_run.take() {
            self.command_palette_status = format!(
                "Макрос остановлен: {} (выполнено {}, пропущено {})",
                run.name, run.executed, run.skipped
            );
        }
    }

    fn execute_command_palette_action(&mut self, action: CommandPaletteAction) {
        match action {
            CommandPaletteAction::ApplyLayout(preset) => self.apply_layout_preset(preset),
            CommandPaletteAction::SetWorkspaceMode(mode) => self.set_workspace_mode(mode),
            CommandPaletteAction::SetRightPanel(view) => self.open_right_panel_from_command(view),
            CommandPaletteAction::ToggleFilePanel => {
                self.set_file_panel_collapsed(!self.file_panel_collapsed);
            }
            CommandPaletteAction::OpenProject => self.choose_workspace(),
            CommandPaletteAction::RefreshWorkspace => {
                self.refresh_file_rows();
                self.refresh_git_summary();
                self.refresh_project_profiles();
                self.project_status = "проект обновлён из палитры команд".to_string();
            }
            CommandPaletteAction::NewChat => {
                self.create_new_chat();
                self.active_center_tab = CenterTab::Agent;
            }
            CommandPaletteAction::ResetConversation => self.reset_conversation(),
            CommandPaletteAction::SetPrompt(prompt) => {
                self.input = prompt.to_string();
                self.active_center_tab = CenterTab::Agent;
            }
            CommandPaletteAction::StartProjectCommand(command) => {
                self.set_workspace_mode(WorkspaceMode::Project);
                self.right_panel_view = RightPanelView::Project;
                self.persist_layout_state();
                self.start_project_command(command);
            }
            CommandPaletteAction::GitStatus => {
                self.open_right_panel_from_command(RightPanelView::Logs);
                self.show_git_status_from_ui();
            }
            CommandPaletteAction::GitCommit => {
                self.open_right_panel_from_command(RightPanelView::Logs);
                self.git_commit_dialog_open = true;
            }
            CommandPaletteAction::StopAgent => self.stop_run(),
            CommandPaletteAction::StopProjectCommand => self.stop_project_command(),
            CommandPaletteAction::RunMacro(macro_id) => {
                self.execute_command_palette_macro(&macro_id);
            }
        }
    }

    fn open_right_panel_from_command(&mut self, view: RightPanelView) {
        if !self.workspace_mode.panels().contains(&view) {
            self.workspace_mode = match view {
                RightPanelView::Release | RightPanelView::Project => WorkspaceMode::Project,
                RightPanelView::Assets => WorkspaceMode::Assets,
                RightPanelView::Overview
                | RightPanelView::Context
                | RightPanelView::Roadmap
                | RightPanelView::Control
                | RightPanelView::Logs => WorkspaceMode::Chat,
            };
        }
        self.right_panel_view = view;
        self.persist_layout_state();
    }

    fn refresh_file_rows(&mut self) {
        self.file_rows = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.ui_file_rows(600))
            .unwrap_or_default();
    }

    fn project_tree_title(&self) -> String {
        self.workspace
            .as_ref()
            .map(|workspace| workspace.display_name())
            .unwrap_or_else(|| "Новый проект".to_string())
    }

    fn active_conversation_title(&self) -> String {
        let Some(active_id) = self.active_conversation_id.as_deref() else {
            return "Новый чат".to_string();
        };
        self.conversation_index
            .conversations
            .iter()
            .find(|meta| meta.id == active_id)
            .map(|meta| meta.title.clone())
            .unwrap_or_else(|| "Новый чат".to_string())
    }

    fn context_budget(&self) -> ContextBudget {
        ContextBudget {
            recent_message_limit: self.config.context_recent_messages,
            relevant_message_limit: self.config.context_relevant_messages,
            recent_run_limit: self.config.context_recent_runs,
        }
        .bounded()
    }

    fn apply_context_preset(
        &mut self,
        label: &str,
        recent_messages: usize,
        relevant_messages: usize,
        recent_runs: usize,
    ) {
        self.config.context_recent_messages = recent_messages;
        self.config.context_relevant_messages = relevant_messages;
        self.config.context_recent_runs = recent_runs;
        if let Err(err) = self.config.save() {
            self.conversation_status = format!("не удалось сохранить пресет контекста: {err}");
        } else {
            self.conversation_status = format!("пресет контекста: {label}");
        }
    }

    fn save_context_notes(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            return;
        };
        match save_conversation_context_notes(
            &workspace,
            &conversation_id,
            self.context_notes.clone(),
        ) {
            Ok(state) => {
                self.context_notes = state.context_notes;
                self.conversation_status =
                    format!("заметки контекста: {}", self.context_notes.len());
            }
            Err(err) => {
                self.conversation_status = format!("не удалось сохранить заметки контекста: {err}");
            }
        }
    }

    fn add_context_note_from_input(&mut self) {
        let note = self.context_note_input.trim().to_string();
        if note.is_empty() {
            return;
        }
        self.context_notes.push(note);
        self.context_note_input.clear();
        self.save_context_notes();
    }

    fn remove_context_note(&mut self, index: usize) {
        if index < self.context_notes.len() {
            self.context_notes.remove(index);
            self.save_context_notes();
        }
    }

    fn export_active_context_profile(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            self.conversation_status = "нет активного чата для экспорта".to_string();
            return;
        };
        let title = self.active_conversation_title();
        match export_context_profile(
            &workspace,
            &conversation_id,
            &title,
            &self.context_notes,
            self.context_budget(),
        ) {
            Ok(rel_path) => {
                self.conversation_status = format!("профиль контекста экспортирован: {rel_path}");
                self.refresh_file_rows();
            }
            Err(err) => {
                self.conversation_status =
                    format!("не удалось экспортировать профиль контекста: {err}");
            }
        }
    }

    fn import_context_profile_for_active_chat(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            self.conversation_status = "нет активного чата для импорта".to_string();
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Context profile", &["json"])
            .pick_file()
        else {
            return;
        };

        match import_context_profile_file(&workspace, &path, &conversation_id) {
            Ok((state, budget)) => {
                self.context_notes = state.context_notes;
                self.config.context_recent_messages = budget.recent_message_limit;
                self.config.context_relevant_messages = budget.relevant_message_limit;
                self.config.context_recent_runs = budget.recent_run_limit;
                let _ = self.config.save();
                self.context_note_suggestions.clear();
                self.conversation_status =
                    format!("профиль контекста импортирован: {}", path.display());
            }
            Err(err) => {
                self.conversation_status =
                    format!("не удалось импортировать профиль контекста: {err}");
            }
        }
    }

    fn accept_context_note_suggestion(&mut self, index: usize) {
        if index >= self.context_note_suggestions.len() {
            return;
        }
        let note = self.context_note_suggestions.remove(index);
        self.context_notes.push(note);
        self.save_context_notes();
    }

    fn accept_all_context_note_suggestions(&mut self) {
        if self.context_note_suggestions.is_empty() {
            return;
        }
        self.context_notes
            .extend(self.context_note_suggestions.drain(..));
        self.save_context_notes();
    }

    fn suggest_context_notes_after_run(
        &mut self,
        context: Option<&AgentRunHistoryContext>,
        final_response: Option<&str>,
        changed_files: &[String],
        elapsed: Option<Duration>,
    ) {
        let long_enough = elapsed.is_some_and(|duration| duration >= Duration::from_secs(45));
        let broad_change = changed_files.len() >= 2;
        let substantial_response = final_response
            .map(|response| response.chars().count() >= 1_200)
            .unwrap_or(false);
        if !(long_enough || broad_change || substantial_response) {
            return;
        }

        let mut candidates = Vec::new();
        if let Some(context) = context {
            if let Some(plan) = &context.confirmed_plan {
                candidates.push(format!(
                    "Согласованный подход: {}",
                    compact_inline(&plan.summary, 240)
                ));
            } else {
                candidates.push(format!(
                    "Текущая задача проекта: {}",
                    compact_inline(&context.user_request, 240)
                ));
            }
        }
        if !changed_files.is_empty() {
            candidates.push(format!(
                "Важные файлы после последней задачи: {}",
                changed_files
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(line) = final_response.and_then(first_useful_response_line) {
            candidates.push(format!("Итог последней задачи: {line}"));
        }

        let mut suggestions = Vec::new();
        for candidate in candidates {
            let note = compact_inline(candidate.trim(), 500);
            if note.is_empty()
                || self
                    .context_notes
                    .iter()
                    .any(|existing| existing.trim().eq_ignore_ascii_case(note.trim()))
                || suggestions
                    .iter()
                    .any(|existing: &String| existing.trim().eq_ignore_ascii_case(note.trim()))
            {
                continue;
            }
            suggestions.push(note);
            if suggestions.len() >= 3 {
                break;
            }
        }

        if !suggestions.is_empty() {
            self.context_note_suggestions = suggestions;
            self.conversation_status = format!(
                "есть предложения для памяти: {}",
                self.context_note_suggestions.len()
            );
        }
    }

    fn active_project_path(&self) -> Option<PathBuf> {
        self.workspace
            .as_ref()
            .map(|workspace| workspace.root().to_path_buf())
    }

    fn active_project_state(&self) -> Option<&ProjectUiState> {
        let path = self.active_project_path()?;
        self.config.project_state(&path)
    }

    fn active_project_state_mut(&mut self) -> Option<&mut ProjectUiState> {
        let path = self.active_project_path()?;
        self.config.remember_project(path.clone());
        self.config.project_state_mut(&path)
    }

    fn project_is_active(&self, path: &Path) -> bool {
        self.active_project_path()
            .as_deref()
            .is_some_and(|active| project_paths_match(active, path))
    }

    fn active_project_is_expanded(&self) -> bool {
        self.active_project_state()
            .map(|state| state.expanded)
            .unwrap_or(false)
    }

    fn toggle_project_expanded(&mut self, path: &Path) {
        self.config.remember_project(path.to_path_buf());
        if let Some(state) = self.config.project_state_mut(path) {
            state.expanded = !state.expanded;
        }
        let _ = self.config.save();
    }

    fn start_project_rename(&mut self, path: &Path) {
        self.config.remember_project(path.to_path_buf());
        let name = self
            .config
            .project_state(path)
            .map(project_label)
            .unwrap_or_else(|| project_display_name(path));
        self.project_rename_target = Some(path.to_path_buf());
        self.project_rename_input = name;
    }

    fn cancel_project_rename(&mut self) {
        self.project_rename_target = None;
        self.project_rename_input.clear();
    }

    fn commit_project_rename(&mut self) {
        let Some(path) = self.project_rename_target.take() else {
            return;
        };
        let display_name = self.project_rename_input.trim().to_string();
        self.project_rename_input.clear();
        if self.config.set_project_display_name(&path, &display_name) {
            self.file_operation_status = if display_name.is_empty() {
                "Имя проекта сброшено".to_string()
            } else {
                format!("Проект переименован: {display_name}")
            };
            let _ = self.config.save();
        }
    }

    fn toggle_project_pinned(&mut self, path: &Path) {
        if self.config.toggle_project_pinned(path) {
            let pinned = self
                .config
                .project_state(path)
                .map(|project| project.pinned)
                .unwrap_or(false);
            self.file_operation_status = if pinned {
                "Проект закреплён".to_string()
            } else {
                "Проект откреплён".to_string()
            };
            let _ = self.config.save();
        }
    }

    fn remove_project_from_list(&mut self, path: &Path) {
        let was_active = self.project_is_active(path);
        if !self.config.remove_project(path) {
            return;
        }
        self.file_operation_status =
            format!("Проект убран из списка: {}", project_display_name(path));
        self.project_rename_target = None;
        self.project_rename_input.clear();

        if was_active {
            let next_project = self
                .config
                .projects
                .first()
                .map(|project| project.path.clone());
            if let Some(next_project) = next_project {
                let _ = self.open_workspace_path(next_project);
            } else {
                self.clear_workspace_selection();
                let _ = self.config.save();
            }
        } else {
            let _ = self.config.save();
        }
    }

    fn clear_workspace_selection(&mut self) {
        self.config.last_workspace = None;
        self.workspace = None;
        self.file_rows.clear();
        self.selected_tree_path = None;
        self.file_rename_target = None;
        self.file_rename_input.clear();
        self.last_file_click_path = None;
        self.last_file_click_time = 0.0;
        self.dragged_tree_path = None;
        self.selected_file = None;
        self.selected_preview.clear();
        self.original_file_content.clear();
        self.selected_file_editable = false;
        self.editor_status.clear();
        self.file_tabs.clear();
        self.active_center_tab = CenterTab::Agent;
        self.asset_jobs.clear();
        self.asset_compare_paths.clear();
        self.asset_previews.clear();
        self.project_profiles.clear();
        self.project_runs.clear();
        self.project_fix_requests.clear();
        self.git_summary.clear();
        self.git_changed_files.clear();
        self.provider_validation_results.clear();
        self.active_conversation_id = None;
        self.conversation_index = ConversationIndex::default();
        self.conversation_status.clear();
        self.conversation_rename_target = None;
        self.conversation_rename_input.clear();
        self.context_inspector_query.clear();
        self.context_notes.clear();
        self.context_note_input.clear();
        self.chat = default_chat();
        self.agent_state
            .lock()
            .expect("agent state poisoned")
            .reset();
    }

    fn active_dir_is_expanded(&self, path: &str) -> bool {
        let dir = normalize_tree_dir(path);
        if dir.is_empty() {
            return true;
        }
        self.active_project_state()
            .map(|state| state.expanded_dirs.iter().any(|expanded| expanded == &dir))
            .unwrap_or(false)
    }

    fn toggle_active_dir_expanded(&mut self, path: &str) {
        let dir = normalize_tree_dir(path);
        if dir.is_empty() {
            return;
        }
        if let Some(state) = self.active_project_state_mut() {
            if let Some(index) = state
                .expanded_dirs
                .iter()
                .position(|expanded| expanded == &dir)
            {
                state.expanded_dirs.remove(index);
            } else {
                state.expanded_dirs.push(dir);
            }
        }
        let _ = self.config.save();
    }

    fn visible_file_rows(&self) -> Vec<String> {
        if !self.active_project_is_expanded() {
            return Vec::new();
        }
        let narrowed = self.file_tree_is_narrowed();
        self.file_rows
            .iter()
            .filter(|row| self.file_tree_row_matches_filters(row))
            .filter(|row| narrowed || self.file_tree_row_is_visible(row))
            .cloned()
            .collect()
    }

    fn file_tree_row_is_visible(&self, row: &str) -> bool {
        if row == "..." {
            return true;
        }
        file_tree_parent_dirs(row)
            .iter()
            .all(|parent| self.active_dir_is_expanded(parent))
    }

    fn file_tree_is_narrowed(&self) -> bool {
        !self.file_search_input.trim().is_empty() || self.file_filter != FileTreeFilter::All
    }

    fn file_tree_row_matches_filters(&self, row: &str) -> bool {
        self.file_tree_row_matches_search(row) && self.file_tree_row_matches_kind_filter(row)
    }

    fn file_tree_row_matches_search(&self, row: &str) -> bool {
        let query = self.file_search_input.trim().to_ascii_lowercase();
        if query.is_empty() || row == "..." {
            return true;
        }
        let haystack = row.to_ascii_lowercase();
        haystack.contains(&query)
            || row.ends_with('/')
                && self.file_rows.iter().any(|candidate| {
                    candidate.starts_with(row) && candidate.to_ascii_lowercase().contains(&query)
                })
    }

    fn file_tree_row_matches_kind_filter(&self, row: &str) -> bool {
        match self.file_filter {
            FileTreeFilter::All => true,
            FileTreeFilter::Modified => {
                tree_row_matches_changed_files(row, &self.git_changed_files)
            }
            FileTreeFilter::Code => tree_row_matches_any(row, &self.file_rows, is_code_file_path),
            FileTreeFilter::Assets => {
                tree_row_matches_any(row, &self.file_rows, is_asset_file_path)
            }
        }
    }

    fn handle_file_tree_click(
        &mut self,
        path: &str,
        is_dir: bool,
        double_clicked: bool,
        time: f64,
    ) {
        if path == "..." {
            return;
        }
        self.selected_tree_path = Some(path.to_string());

        if double_clicked {
            self.last_file_click_path = None;
            self.last_file_click_time = 0.0;
            if is_dir {
                self.toggle_active_dir_expanded(path);
            } else {
                self.load_file_preview(path);
            }
            return;
        }

        let repeated_delayed_click = self.last_file_click_path.as_deref() == Some(path)
            && (0.35..=1.60).contains(&(time - self.last_file_click_time));
        if repeated_delayed_click {
            self.start_tree_rename(path);
            self.last_file_click_path = None;
            self.last_file_click_time = 0.0;
            return;
        }

        self.last_file_click_path = Some(path.to_string());
        self.last_file_click_time = time;
        if !is_dir {
            self.load_file_preview(path);
        }
    }

    fn start_tree_rename(&mut self, path: &str) {
        if path.is_empty() || path == "..." {
            return;
        }
        self.selected_tree_path = Some(path.to_string());
        self.file_rename_target = Some(path.to_string());
        self.file_rename_input = file_tree_name(path).to_string();
    }

    fn cancel_tree_rename(&mut self) {
        self.file_rename_target = None;
        self.file_rename_input.clear();
    }

    fn commit_tree_rename(&mut self) {
        let Some(path) = self.file_rename_target.take() else {
            return;
        };
        let new_name = self.file_rename_input.trim().to_string();
        self.file_rename_input.clear();
        if new_name.is_empty() {
            self.file_operation_status = "Введите имя файла или папки".to_string();
            self.selected_tree_path = Some(path);
            return;
        }

        let Some(workspace) = &self.workspace else {
            self.file_operation_status = "Проект не выбран".to_string();
            return;
        };

        match workspace.rename_entry(&path, &new_name) {
            Ok(new_path) => {
                self.update_open_paths_after_move(&path, &new_path);
                self.selected_tree_path = Some(new_path.clone());
                self.file_operation_status = format!("Переименовано: {new_path}");
                self.refresh_after_file_operation();
            }
            Err(err) => {
                self.selected_tree_path = Some(path);
                self.file_operation_status = format!("Не удалось переименовать: {err}");
            }
        }
    }

    fn duplicate_tree_path(&mut self, path: &str) {
        let Some(workspace) = &self.workspace else {
            self.file_operation_status = "Проект не выбран".to_string();
            return;
        };

        match workspace.duplicate_entry(path) {
            Ok(new_path) => {
                self.selected_tree_path = Some(new_path.clone());
                self.file_operation_status = format!("Создана копия: {new_path}");
                self.refresh_after_file_operation();
            }
            Err(err) => {
                self.file_operation_status = format!("Не удалось скопировать: {err}");
            }
        }
    }

    fn delete_tree_path(&mut self, path: &str) {
        let Some(workspace) = &self.workspace else {
            self.file_operation_status = "Проект не выбран".to_string();
            return;
        };

        match workspace.delete_entry(path) {
            Ok(()) => {
                self.close_deleted_file_tabs(path);
                self.selected_tree_path = None;
                self.file_operation_status = format!("Удалено: {}", path.trim_end_matches('/'));
                self.refresh_after_file_operation();
            }
            Err(err) => {
                self.file_operation_status = format!("Не удалось удалить: {err}");
            }
        }
    }

    fn move_tree_path(&mut self, source: &str, target_dir: &str) {
        if source == "..." || source.is_empty() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            self.file_operation_status = "Проект не выбран".to_string();
            return;
        };

        match workspace.move_entry_to_dir(source, target_dir) {
            Ok(new_path) => {
                self.update_open_paths_after_move(source, &new_path);
                self.selected_tree_path = Some(new_path.clone());
                self.file_operation_status = format!("Перемещено: {new_path}");
                self.refresh_after_file_operation();
            }
            Err(err) => {
                self.file_operation_status = format!("Не удалось переместить: {err}");
            }
        }
    }

    fn refresh_after_file_operation(&mut self) {
        self.refresh_file_rows();
        self.refresh_project_profiles();
        self.refresh_git_summary();
    }

    fn update_open_paths_after_move(&mut self, old_path: &str, new_path: &str) {
        let old_base = file_tree_base_path(old_path);
        let new_base = file_tree_base_path(new_path);

        for tab in &mut self.file_tabs {
            if let Some(updated) = remap_path_after_base_move(&tab.path, old_base, new_base) {
                tab.path = updated;
            }
        }

        if let CenterTab::File(active) = self.active_center_tab.clone() {
            if let Some(updated) = remap_path_after_base_move(&active, old_base, new_base) {
                self.active_center_tab = CenterTab::File(updated);
            }
        }
        if let Some(selected) = self.selected_file.clone() {
            if let Some(updated) = remap_path_after_base_move(&selected, old_base, new_base) {
                self.selected_file = Some(updated);
            }
        }
        if let Some(selected) = self.selected_tree_path.clone() {
            if let Some(updated) = remap_tree_path_after_base_move(&selected, old_base, new_base) {
                self.selected_tree_path = Some(updated);
            }
        }
        if let Some(state) = self.active_project_state_mut() {
            for dir in &mut state.expanded_dirs {
                if let Some(updated) = remap_tree_path_after_base_move(dir, old_base, new_base) {
                    *dir = updated;
                }
            }
        }
    }

    fn close_deleted_file_tabs(&mut self, deleted_path: &str) {
        let deleted_base = file_tree_base_path(deleted_path);
        self.file_tabs
            .retain(|tab| !path_is_same_or_child(&tab.path, deleted_base));

        if matches!(
            &self.active_center_tab,
            CenterTab::File(active) if path_is_same_or_child(active, deleted_base)
        ) {
            self.active_center_tab = CenterTab::Agent;
            self.selected_file = None;
            self.selected_preview.clear();
            self.original_file_content.clear();
            self.selected_file_editable = false;
            self.editor_status.clear();
        }

        if self
            .selected_tree_path
            .as_deref()
            .is_some_and(|path| path_is_same_or_child(file_tree_base_path(path), deleted_base))
        {
            self.selected_tree_path = None;
        }
        if self
            .selected_file
            .as_deref()
            .is_some_and(|path| path_is_same_or_child(path, deleted_base))
        {
            self.selected_file = None;
        }
        if let Some(state) = self.active_project_state_mut() {
            state
                .expanded_dirs
                .retain(|dir| !path_is_same_or_child(file_tree_base_path(dir), deleted_base));
        }
    }

    fn refresh_project_profiles(&mut self) {
        self.project_profiles = self
            .workspace
            .as_ref()
            .map(detect_project_profiles)
            .unwrap_or_default();
    }

    fn sync_config_from_inputs(&mut self) {
        self.config.set_active_provider_settings(
            &self.provider_input,
            self.model_input.trim().to_string(),
            self.api_key_input.trim().to_string(),
        );
    }

    fn save_settings_from_ui(&mut self) {
        self.sync_config_from_inputs();
        self.sync_asset_provider_settings();
        self.config.normalize_proxy_settings();
        self.journal_status = match self.config.save() {
            Ok(()) => "Настройки сохранены".to_string(),
            Err(err) => format!("Не удалось сохранить настройки: {err}"),
        };
    }

    fn sync_asset_provider_settings(&mut self) {
        let provider_id = match self.asset_kind_input.as_str() {
            "audio" => OPENAI_AUDIO_PROVIDER_ID,
            "video" => OPENAI_VIDEO_PROVIDER_ID,
            _ => self.asset_provider_input.as_str(),
        };
        let default_model = match self.asset_kind_input.as_str() {
            "audio" => default_audio_model(provider_id),
            "video" => default_video_model(provider_id),
            _ => default_image_model(provider_id),
        };
        let model = if self.asset_model_input.trim().is_empty() {
            default_model.to_string()
        } else {
            self.asset_model_input.trim().to_string()
        };
        self.asset_model_input = model.clone();
        self.config.set_provider_settings(
            provider_id,
            model,
            self.asset_api_key_input.trim().to_string(),
        );
    }

    fn sync_asset_provider_settings_for(&mut self, provider_id: &str) {
        let model = if self.asset_model_input.trim().is_empty() {
            default_image_model(provider_id).to_string()
        } else {
            self.asset_model_input.trim().to_string()
        };
        self.config.set_provider_settings(
            provider_id,
            model,
            self.asset_api_key_input.trim().to_string(),
        );
    }

    fn switch_provider_from_ui(&mut self, provider_id: String) {
        self.config.select_provider(&provider_id);
        self.provider_input = self.config.provider.clone();
        self.api_key_input = self.config.api_key.clone();
        self.model_input = self.config.model.clone();
    }

    fn switch_asset_provider_from_ui(&mut self, provider_id: String) {
        self.asset_provider_input = provider_id;
        self.asset_api_key_input =
            image_api_key_from_config(&self.config, &self.asset_provider_input);
        self.asset_model_input = image_model_from_config(&self.config, &self.asset_provider_input);
    }

    fn switch_asset_kind_from_ui(&mut self) {
        match self.asset_kind_input.as_str() {
            "audio" => {
                self.asset_api_key_input =
                    media_api_key_from_config(&self.config, OPENAI_AUDIO_PROVIDER_ID);
                self.asset_model_input = media_model_from_config(
                    &self.config,
                    OPENAI_AUDIO_PROVIDER_ID,
                    default_audio_model(OPENAI_AUDIO_PROVIDER_ID),
                );
            }
            "video" => {
                self.asset_api_key_input =
                    media_api_key_from_config(&self.config, OPENAI_VIDEO_PROVIDER_ID);
                self.asset_model_input = media_model_from_config(
                    &self.config,
                    OPENAI_VIDEO_PROVIDER_ID,
                    default_video_model(OPENAI_VIDEO_PROVIDER_ID),
                );
                if !["1280x720", "720x1280", "1920x1080", "1080x1920"]
                    .contains(&self.asset_image_size.as_str())
                {
                    self.asset_image_size = "1280x720".to_string();
                }
            }
            _ => {
                self.asset_api_key_input =
                    image_api_key_from_config(&self.config, &self.asset_provider_input);
                self.asset_model_input =
                    image_model_from_config(&self.config, &self.asset_provider_input);
                if !["0.5K", "1K", "2K", "4K"].contains(&self.asset_image_size.as_str()) {
                    self.asset_image_size = "1K".to_string();
                }
            }
        }
    }

    fn refresh_git_summary(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.git_summary.clear();
            self.git_changed_files.clear();
            return;
        };

        self.git_changed_files = git_changed_files_for_workspace(workspace);

        let status = Command::new("git")
            .arg("status")
            .arg("--short")
            .current_dir(workspace.root())
            .output();
        let diff = Command::new("git")
            .arg("diff")
            .arg("--stat")
            .current_dir(workspace.root())
            .output();

        let status_text = match status {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    "status: чисто".to_string()
                } else {
                    format!("status:\n{stdout}")
                }
            }
            Ok(output) => format!(
                "status не выполнен:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(err) => format!("status не выполнен: {err}"),
        };

        let diff_text = match diff {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    "diff: нет незакоммиченных изменений".to_string()
                } else {
                    format!("diff:\n{stdout}")
                }
            }
            Ok(output) => format!(
                "diff не выполнен:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(err) => format!("diff не выполнен: {err}"),
        };

        self.git_summary = format!("{status_text}\n\n{diff_text}");
    }

    fn show_git_status_from_ui(&mut self) {
        self.refresh_git_summary();
        self.git_action_status = if self.workspace.is_some() {
            "status обновлён".to_string()
        } else {
            "рабочая папка не выбрана".to_string()
        };
    }

    fn run_git_action_from_ui(&mut self, title: &str, args: &[&str]) {
        let Some(workspace) = &self.workspace else {
            self.git_action_status = "рабочая папка не выбрана".to_string();
            return;
        };

        let result = run_git_command(workspace, args);
        self.git_action_status = result.display.clone();
        self.tool_log.push(ToolLogLine {
            title: title.to_string(),
            content: result.display,
        });
        self.refresh_git_summary();
    }

    fn commit_git_from_ui(&mut self) {
        let message = self.git_commit_message_input.trim().to_string();
        if message.is_empty() {
            self.git_action_status = "введите комментарий коммита".to_string();
            return;
        }

        let Some(workspace) = &self.workspace else {
            self.git_action_status = "рабочая папка не выбрана".to_string();
            return;
        };

        let add = run_git_command(workspace, &["add", "-A"]);
        let commit = if add.success {
            run_git_command(workspace, &["commit", "-m", &message])
        } else {
            GitCommandResult {
                success: false,
                display: "commit не запущен: git add -A завершился ошибкой".to_string(),
            }
        };

        self.git_action_status = format!(
            "git add -A:\n{}\n\ngit commit:\n{}",
            add.display, commit.display
        );
        self.tool_log.push(ToolLogLine {
            title: "git commit".to_string(),
            content: self.git_action_status.clone(),
        });

        if commit.success {
            self.git_commit_message_input.clear();
            self.git_commit_dialog_open = false;
        }

        self.refresh_git_summary();
    }

    fn show_git_commit_dialog(&mut self, ctx: &egui::Context) {
        if !self.git_commit_dialog_open {
            return;
        }

        let mut open = self.git_commit_dialog_open;
        let mut should_commit = false;
        egui::Window::new("Коммит")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Комментарий коммита:");
                ui.add(TextEdit::multiline(&mut self.git_commit_message_input).desired_rows(3));
                ui.horizontal(|ui| {
                    if ui.button("Коммит").clicked() {
                        should_commit = true;
                    }
                    if ui.button("Отмена").clicked() {
                        self.git_commit_dialog_open = false;
                    }
                });
            });
        self.git_commit_dialog_open = open && self.git_commit_dialog_open;
        if should_commit {
            self.commit_git_from_ui();
        }
    }

    fn show_command_palette(&mut self, ctx: &egui::Context) {
        if !self.command_palette_open {
            return;
        }

        let mut open = self.command_palette_open;
        let mut execute_item: Option<CommandPaletteItem> = None;
        let mut toggle_favorite_id: Option<String> = None;
        let mut delete_macro_id: Option<String> = None;
        let mut edit_macro_id: Option<String> = None;
        let mut export_macro_id: Option<String> = None;
        let mut toggle_macro_confirmation_id: Option<String> = None;
        let mut move_macro_step: Option<(String, usize, isize)> = None;
        let mut remove_macro_step: Option<(String, usize)> = None;
        let mut save_macro_edit = false;
        let mut cancel_macro_edit = false;
        let mut import_macro = false;
        let mut create_macro = false;
        let items = self.filtered_command_palette_items();
        let edit_macro = self
            .command_macro_edit_target
            .as_ref()
            .and_then(|macro_id| {
                self.config
                    .command_palette_macros
                    .iter()
                    .find(|command_macro| &command_macro.id == macro_id)
                    .cloned()
            });
        if self.command_palette_selected >= items.len() {
            self.command_palette_selected = items.len().saturating_sub(1);
        }

        egui::Window::new("Палитра команд")
            .id(egui::Id::new("command_palette_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 72.0))
            .default_width(680.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(620.0);
                ui.label(
                    RichText::new("Быстрый переход, команды проекта и готовые запросы.")
                        .weak()
                        .small(),
                );
                ui.add_space(8.0);
                let query_response = ui.add_sized(
                    [safe_available_width(ui, 320.0), 32.0],
                    TextEdit::singleline(&mut self.command_palette_query)
                        .id_salt("command_palette_query")
                        .hint_text("Найти команду: roadmap, test, ассет, вид..."),
                );
                query_response.request_focus();
                if query_response.changed() {
                    self.command_palette_selected = 0;
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    chip(ui, "Ctrl+K");
                    chip(ui, "Ctrl+Shift+P");
                    chip(
                        ui,
                        format!("избранное {}", self.config.command_palette_favorites.len()),
                    );
                    chip(
                        ui,
                        format!("последние {}", self.config.command_palette_recent.len()),
                    );
                    chip(
                        ui,
                        format!("макросы {}", self.config.command_palette_macros.len()),
                    );
                    ui.label(
                        RichText::new("Enter — выполнить, Esc — закрыть")
                            .weak()
                            .small(),
                    );
                });
                if let Some(selected_item) = items.get(self.command_palette_selected) {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        let is_favorite = self.command_palette_is_favorite(&selected_item.id);
                        let favorite_label = if is_favorite {
                            "Убрать из избранного"
                        } else {
                            "В избранное"
                        };
                        if ui.button(favorite_label).clicked() {
                            toggle_favorite_id = Some(selected_item.id.clone());
                        }
                        if let CommandPaletteAction::RunMacro(macro_id) = &selected_item.action {
                            if ui.button("Редактировать").clicked() {
                                edit_macro_id = Some(macro_id.clone());
                            }
                            if ui.button("Экспорт").clicked() {
                                export_macro_id = Some(macro_id.clone());
                            }
                            if ui.button("Удалить макрос").clicked() {
                                delete_macro_id = Some(macro_id.clone());
                            }
                        }
                        ui.label(
                            RichText::new(format!("id: {}", selected_item.id))
                                .weak()
                                .small(),
                        );
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [260.0, 28.0],
                        TextEdit::singleline(&mut self.command_macro_name_input)
                            .hint_text("Название макроса из избранного"),
                    );
                    if ui.button("Создать макрос").clicked() {
                        create_macro = true;
                    }
                });
                if let Some(command_macro) = &edit_macro {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(RichText::new("Редактор макроса").strong());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Название").weak());
                        ui.add_sized(
                            [safe_available_width(ui, 220.0), 28.0],
                            TextEdit::singleline(&mut self.command_macro_edit_name),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Описание").weak());
                        ui.add_sized(
                            [safe_available_width(ui, 220.0), 28.0],
                            TextEdit::singleline(&mut self.command_macro_edit_description),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Сохранить").clicked() {
                            save_macro_edit = true;
                        }
                        if ui.button("Закрыть редактор").clicked() {
                            cancel_macro_edit = true;
                        }
                        let confirm_label = if command_macro.confirm_each_step {
                            "Подтверждение: вкл"
                        } else {
                            "Подтверждение: выкл"
                        };
                        if ui.button(confirm_label).clicked() {
                            toggle_macro_confirmation_id = Some(command_macro.id.clone());
                        }
                    });
                    ui.add_space(4.0);
                    for (step_index, command_id) in command_macro.command_ids.iter().enumerate() {
                        let item = items.iter().find(|item| &item.id == command_id);
                        let title = item
                            .map(|item| item.title.as_str())
                            .unwrap_or(command_id.as_str());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("{}.", step_index + 1)).weak());
                            ui.label(RichText::new(title).strong());
                            ui.label(RichText::new(command_id).weak().small());
                            if ui.small_button("Вверх").clicked() {
                                move_macro_step = Some((command_macro.id.clone(), step_index, -1));
                            }
                            if ui.small_button("Вниз").clicked() {
                                move_macro_step = Some((command_macro.id.clone(), step_index, 1));
                            }
                            if ui.small_button("Удалить шаг").clicked() {
                                remove_macro_step = Some((command_macro.id.clone(), step_index));
                            }
                        });
                    }
                }
                ui.add_space(8.0);
                ui.collapsing("Импорт макроса", |ui| {
                    ui.add(
                        TextEdit::multiline(&mut self.command_macro_import_input)
                            .desired_rows(3)
                            .desired_width(safe_available_width(ui, 260.0))
                            .hint_text("Вставьте JSON макроса"),
                    );
                    if ui.button("Импортировать").clicked() {
                        import_macro = true;
                    }
                });
                if !self.command_palette_status.trim().is_empty() {
                    ui.label(RichText::new(&self.command_palette_status).weak().small());
                }
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                if items.is_empty() {
                    empty_state(
                        ui,
                        "Команда не найдена",
                        "Попробуйте другое слово: проект, roadmap, test, git, ассет, вид.",
                    );
                    return;
                }

                egui::ScrollArea::vertical()
                    .id_salt("command_palette_results")
                    .max_height(420.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, item) in items.iter().enumerate() {
                            let selected = index == self.command_palette_selected;
                            let fill = if selected {
                                egui::Color32::from_rgb(30, 67, 82)
                            } else {
                                panel_bg()
                            };
                            let text_color = if item.enabled {
                                text_color()
                            } else {
                                muted_color()
                            };
                            let row = egui::Frame::none()
                                .fill(fill)
                                .rounding(egui::Rounding::same(6.0))
                                .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                                .show(ui, |ui| {
                                    ui.set_min_width(safe_available_width(ui, 320.0));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(&item.title)
                                                    .strong()
                                                    .color(text_color),
                                            );
                                            ui.label(
                                                RichText::new(&item.description).weak().small(),
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                chip(ui, item.category);
                                                if let Some(shortcut) = item.shortcut {
                                                    chip(ui, shortcut);
                                                }
                                            },
                                        );
                                    });
                                });
                            let row_response = ui
                                .interact(
                                    row.response.rect,
                                    ui.id().with(("command_palette_row", index)),
                                    egui::Sense::click(),
                                )
                                .on_hover_text(&item.description);
                            if row_response.hovered() {
                                self.command_palette_selected = index;
                            }
                            if row_response.clicked() && item.enabled {
                                execute_item = Some(item.clone());
                            }
                            ui.add_space(3.0);
                        }
                    });
            });

        if let Some(macro_id) = edit_macro_id {
            self.begin_edit_command_macro(&macro_id);
        }
        if let Some(macro_id) = export_macro_id {
            self.export_command_macro_to_clipboard(&macro_id);
        }
        if let Some(macro_id) = toggle_macro_confirmation_id {
            self.toggle_command_macro_confirmation(&macro_id);
        }
        if let Some((macro_id, step_index, delta)) = move_macro_step {
            self.move_command_macro_step(&macro_id, step_index, delta);
        }
        if let Some((macro_id, step_index)) = remove_macro_step {
            self.remove_command_macro_step(&macro_id, step_index);
        }
        if save_macro_edit {
            self.save_command_macro_edit();
        }
        if cancel_macro_edit {
            self.cancel_command_macro_edit();
        }
        if import_macro {
            self.import_command_macro_from_input();
        }
        if let Some(command_id) = toggle_favorite_id {
            self.toggle_command_palette_favorite(&command_id);
        }
        if let Some(macro_id) = delete_macro_id {
            self.delete_command_macro(&macro_id);
        }
        if create_macro {
            self.create_command_macro_from_favorites();
        }
        if let Some(item) = execute_item {
            self.execute_command_palette_item(item);
            open = false;
        }
        self.command_palette_open = open;
    }

    fn show_command_macro_confirmation(&mut self, ctx: &egui::Context) {
        let Some(run) = self.pending_command_macro_run.clone() else {
            return;
        };

        let current_command_id = run.command_ids.get(run.index).cloned().unwrap_or_default();
        let current_item = self.find_command_palette_item_by_id(&current_command_id);
        let step_title = current_item
            .as_ref()
            .map(|item| item.title.clone())
            .unwrap_or_else(|| current_command_id.clone());
        let step_description = current_item
            .as_ref()
            .map(|item| item.description.clone())
            .unwrap_or_else(|| "Команда недоступна или больше не существует".to_string());
        let step_enabled = current_item
            .as_ref()
            .map(|item| item.enabled)
            .unwrap_or(false);

        let mut execute_step = false;
        let mut skip_step = false;
        let mut stop_macro = false;
        let mut run_all = false;

        egui::Window::new("Подтверждение макроса")
            .id(egui::Id::new("command_macro_confirmation"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(&run.name).strong());
                ui.label(
                    RichText::new(format!(
                        "Шаг {} из {}",
                        run.index + 1,
                        run.command_ids.len()
                    ))
                    .weak(),
                );
                ui.add_space(8.0);
                ui.label(RichText::new(step_title).strong());
                ui.label(RichText::new(step_description).weak());
                if !step_enabled {
                    ui.label(
                        RichText::new("Этот шаг сейчас недоступен и будет пропущен.")
                            .color(egui::Color32::from_rgb(235, 154, 154)),
                    );
                }
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(step_enabled, egui::Button::new("Выполнить шаг"))
                        .clicked()
                    {
                        execute_step = true;
                    }
                    if ui.button("Пропустить").clicked() {
                        skip_step = true;
                    }
                    if ui.button("Выполнить всё").clicked() {
                        run_all = true;
                    }
                    if ui.button("Остановить").clicked() {
                        stop_macro = true;
                    }
                });
            });

        if execute_step {
            self.advance_pending_command_macro(true);
        }
        if skip_step {
            self.advance_pending_command_macro(false);
        }
        if run_all {
            while self.pending_command_macro_run.is_some() {
                self.advance_pending_command_macro(true);
            }
        }
        if stop_macro {
            self.cancel_pending_command_macro();
        }
    }

    fn refresh_journal(&mut self) {
        self.journal_lines = read_journal_tail(200);
        self.journal_status = format!("показано последних записей: {}", self.journal_lines.len());
        self.refresh_agent_history();
    }

    fn refresh_agent_history(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.agent_history.clear();
            self.agent_history_status = "рабочая папка не выбрана".to_string();
            return;
        };

        self.agent_history = load_agent_history_tail(workspace, 80);
        self.agent_history_status = if self.agent_history.is_empty() {
            format!("история запусков пока пуста: {AGENT_HISTORY_PATH}")
        } else {
            format!(
                "показано запусков: {} · {}",
                self.agent_history.len(),
                AGENT_HISTORY_PATH
            )
        };
    }

    fn clear_journal_from_ui(&mut self) {
        match clear_journal() {
            Ok(()) => {
                self.journal_lines.clear();
                self.journal_status = "журнал очищен".to_string();
            }
            Err(err) => {
                self.journal_status = format!("не удалось очистить журнал: {err}");
            }
        }
    }

    fn start_project_command(&mut self, command: ProjectCommand) {
        if self.project_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.project_status = "рабочая папка не выбрана".to_string();
            return;
        };

        self.save_settings_from_ui();

        let (tx, rx) = mpsc::channel();
        let approvals = self.approvals.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let policy = PolicyConfig::from_config(&self.config);
        let tool_id = format!("project-{}-{}", command.id, uuid::Uuid::new_v4());
        let label = command.label.clone();
        let shell_command = command.command.clone();
        let summary = format!("{}: {}", command.label, command.command);
        self.last_project_command = Some(command.clone());
        let args = RunShellArgs {
            cmd: shell_command.clone(),
            cwd: Some(command.cwd.clone()),
            shell: None,
            timeout_secs: Some(command.timeout_secs),
        };
        self.project_runs.push(ProjectRunRecord {
            id: tool_id.clone(),
            command: command.clone(),
            label: label.clone(),
            shell_command,
            started_at: unix_timestamp(),
            finished_at: None,
            status: ProjectRunStatus::Running,
            exit_code: None,
            error_summary: Vec::new(),
            diagnostics: Vec::new(),
            output_tail: String::new(),
        });
        self.active_project_run_id = Some(tool_id.clone());

        self.project_events_rx = Some(rx);
        self.project_is_running = true;
        self.project_cancel = Some(cancel);
        self.project_status = format!("выполняется: {label}");

        thread::spawn(move || {
            let _ = tx.send(AppEvent::ToolStarted {
                id: tool_id.clone(),
                name: "project_command".to_string(),
                summary,
            });
            let result = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_shell(
                    &workspace,
                    args,
                    tx.clone(),
                    approvals,
                    worker_cancel,
                    policy,
                    tool_id.clone(),
                ));
            let ok = result.ok;
            let output = result.output;
            let _ = tx.send(AppEvent::ToolFinished {
                id: tool_id,
                output: output.clone(),
            });
            if !ok {
                let _ = tx.send(AppEvent::Error(output));
            }
            let _ = tx.send(AppEvent::Done);
        });
    }

    fn stop_project_command(&mut self) {
        if let Some(cancel) = &self.project_cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if self.pending_approval.is_some() {
            self.answer_approval(false);
        }
        self.project_status = "остановка запрошена".to_string();
    }

    fn append_project_run_output(&mut self, id: &str, chunk: &str) {
        if let Some(run) = self.project_runs.iter_mut().find(|run| run.id == id) {
            append_output_tail(&mut run.output_tail, chunk, 12_000);
        }
    }

    fn finish_project_run(&mut self, id: &str, output: &str) {
        let exit_code = parse_project_exit_code(output);
        let mut project_status = None;
        if let Some(run) = self.project_runs.iter_mut().find(|run| run.id == id) {
            append_output_tail(&mut run.output_tail, output, 12_000);
            run.diagnostics = project_diagnostics(&run.output_tail);
            run.error_summary =
                project_error_summary_from_diagnostics(&run.diagnostics, &run.output_tail);
            run.finished_at = Some(unix_timestamp());
            run.exit_code = exit_code;
            run.status = match exit_code {
                Some(0) => ProjectRunStatus::Passed,
                Some(_) => ProjectRunStatus::Failed,
                None if output.to_lowercase().contains("cancel") || output.contains("отмен") => {
                    ProjectRunStatus::Cancelled
                }
                None => ProjectRunStatus::Failed,
            };
            project_status = Some(format!(
                "{}: {}{}",
                run.label,
                run.status.label(),
                run.exit_code
                    .map(|code| format!(" (exit code {code})"))
                    .unwrap_or_default()
            ));
        }
        if let Some(project_status) = project_status {
            self.project_status = project_status;
        }
    }

    fn fail_active_project_run(&mut self, error: &str) {
        let Some(id) = self.active_project_run_id.clone() else {
            return;
        };

        let mut project_status = None;
        if let Some(run) = self.project_runs.iter_mut().find(|run| run.id == id) {
            append_output_tail(&mut run.output_tail, error, 12_000);
            run.diagnostics = project_diagnostics(&run.output_tail);
            run.error_summary =
                project_error_summary_from_diagnostics(&run.diagnostics, &run.output_tail);
            run.finished_at = Some(unix_timestamp());
            if run.status == ProjectRunStatus::Running {
                run.status = if error.to_lowercase().contains("cancel") || error.contains("отмен")
                {
                    ProjectRunStatus::Cancelled
                } else {
                    ProjectRunStatus::Failed
                };
            }
            project_status = Some(format!("{}: {}", run.label, run.status.label()));
        }
        if let Some(project_status) = project_status {
            self.project_status = project_status;
        }
    }

    fn latest_failed_or_last_project_run(&self) -> Option<ProjectRunRecord> {
        self.project_runs
            .iter()
            .rev()
            .find(|run| run.status == ProjectRunStatus::Failed)
            .or_else(|| self.project_runs.last())
            .cloned()
    }

    fn prepare_fix_prompt_from_run(&mut self, run: &ProjectRunRecord) {
        if !run.id.is_empty() {
            self.prepare_structured_fix_prompt_from_run(run);
            return;
        }
        self.input = format!(
            "Проанализируй падение проектной команды и исправь причину минимальными изменениями.\n\nКоманда: {}\nСтатус: {}\nExit code: {}\n\nХвост вывода:\n```text\n{}\n```\n\nСначала кратко назови вероятную причину, затем внеси правки и проверь подходящей командой.",
            run.shell_command,
            run.status.label(),
            run.exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "нет".to_string()),
            compact(&run.output_tail, 5_000)
        );
        self.active_center_tab = CenterTab::Agent;
        self.set_workspace_mode(WorkspaceMode::Chat);
    }

    fn prepare_structured_fix_prompt_from_run(&mut self, run: &ProjectRunRecord) {
        let diagnostics = project_diagnostics_prompt_block(run, 8);
        self.input = format!(
            "Проанализируй падение проектной команды и исправь причину минимальными изменениями.\n\nКоманда: {}\nСтатус: {}\nExit code: {}\n\nСтруктурированная диагностика:\n{}\n\nХвост вывода:\n```text\n{}\n```\n\nСначала кратко назови вероятную причину, затем внеси правки и проверь подходящей командой.",
            run.shell_command,
            run.status.label(),
            run.exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "нет".to_string()),
            diagnostics,
            compact(&run.output_tail, 5_000)
        );
        self.record_project_fix_request(run, "весь запуск".to_string());
        self.active_center_tab = CenterTab::Agent;
        self.set_workspace_mode(WorkspaceMode::Chat);
    }

    fn prepare_fix_prompt_from_diagnostic(
        &mut self,
        run: &ProjectRunRecord,
        diagnostic: &ProjectDiagnostic,
    ) {
        self.input = format!(
            "Исправь конкретную проблему из диагностики проектной команды минимальными изменениями.\n\nКоманда: {}\nСтатус: {}\nExit code: {}\n\nПроблема:\n{}\n\nСырой фрагмент:\n```text\n{}\n```\n\nХвост вывода:\n```text\n{}\n```\n\nСначала открой и изучи связанный файл, затем исправь причину и проверь подходящей командой.",
            run.shell_command,
            run.status.label(),
            run.exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "нет".to_string()),
            diagnostic_prompt_line(diagnostic),
            diagnostic.raw,
            compact(&run.output_tail, 5_000)
        );
        self.record_project_fix_request(run, diagnostic_short_target(diagnostic));
        self.active_center_tab = CenterTab::Agent;
        self.set_workspace_mode(WorkspaceMode::Chat);
    }

    fn record_project_fix_request(&mut self, run: &ProjectRunRecord, target: String) {
        self.project_fix_requests.push(ProjectFixRequestRecord {
            id: format!("fix-{}", uuid::Uuid::new_v4()),
            run_id: run.id.clone(),
            run_label: run.label.clone(),
            target,
            requested_at: unix_timestamp(),
        });
        if self.project_fix_requests.len() > 40 {
            let overflow = self.project_fix_requests.len() - 40;
            self.project_fix_requests.drain(0..overflow);
        }
    }

    fn open_first_project_preview(&mut self) {
        let profiles = self.project_profiles.clone();
        for profile in profiles {
            for hook in profile.previews {
                if let Some(url) = hook.url.as_deref() {
                    self.open_preview_url_from_ui(url);
                    return;
                }
                if let Some(command_id) = hook.command_id.as_deref() {
                    if let Some(command) = profile
                        .commands
                        .iter()
                        .find(|command| command.id == command_id)
                        .cloned()
                    {
                        self.start_project_command(command);
                        return;
                    }
                }
            }
        }

        self.project_status = "preview для проекта не найден".to_string();
    }

    fn create_game_workflow_from_ui(&mut self, workflow_id: &str) {
        let Some(workspace) = &self.workspace else {
            self.project_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let Some(workflow) = parse_workflow_kind(workflow_id) else {
            self.project_status = format!("неизвестный сценарий: {workflow_id}");
            return;
        };
        let title = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.display_name())
            .unwrap_or_else(|| "Игровой сценарий".to_string());
        let brief = if self.input.trim().is_empty() {
            format!("Сценарий создан из Leetcode для проекта {title}.")
        } else {
            self.input.trim().to_string()
        };

        match run_game_workflow(
            workspace,
            GameWorkflowRequest {
                workflow,
                title,
                brief,
            },
        ) {
            Ok(result) => {
                self.project_status = format!("создано: {}", result.path);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.project_status = format!("сценарий не выполнен: {err}"),
        }
    }

    fn open_preview_url_from_ui(&mut self, url: &str) {
        #[cfg(target_os = "windows")]
        let result = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(url)
            .spawn()
            .map(|_| ());
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg(url).spawn().map(|_| ());
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let result = Command::new("xdg-open").arg(url).spawn().map(|_| ());

        self.project_status = match result {
            Ok(()) => format!("открыто: {url}"),
            Err(err) => {
                format!("не удалось открыть предпросмотр: {err}")
            }
        };
    }

    fn open_project_folder(&mut self, path: &Path) {
        #[cfg(target_os = "windows")]
        let result = Command::new("explorer").arg(path).spawn().map(|_| ());
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg(path).spawn().map(|_| ());
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let result = Command::new("xdg-open").arg(path).spawn().map(|_| ());

        self.file_operation_status = match result {
            Ok(()) => format!("Папка открыта: {}", project_display_name(path)),
            Err(err) => format!("Не удалось открыть папку проекта: {err}"),
        };
    }

    fn start_image_asset_job(&mut self) {
        if self.asset_is_running {
            return;
        }

        let prompt = self.asset_prompt.trim().to_string();
        if prompt.is_empty() {
            self.asset_status = "промпт ассета пуст".to_string();
            return;
        }

        self.save_settings_from_ui();

        match self.asset_kind_input.as_str() {
            "spritesheet" => {
                let request = SpritesheetAssetRequest {
                    provider: self.asset_provider_input.clone(),
                    prompt,
                    model: self.asset_model_input.trim().to_string(),
                    aspect_ratio: self.asset_aspect_ratio.clone(),
                    image_size: self.asset_image_size.clone(),
                    columns: 4,
                    rows: 4,
                };
                self.start_spritesheet_asset_request(request);
            }
            "audio" => {
                let request = AudioAssetRequest {
                    provider: OPENAI_AUDIO_PROVIDER_ID.to_string(),
                    prompt,
                    model: if self.asset_model_input.trim().is_empty() {
                        default_audio_model(OPENAI_AUDIO_PROVIDER_ID).to_string()
                    } else {
                        self.asset_model_input.trim().to_string()
                    },
                    voice: "alloy".to_string(),
                    format: "wav".to_string(),
                };
                self.start_audio_asset_request(request);
            }
            "video" => {
                let request = VideoAssetRequest {
                    provider: OPENAI_VIDEO_PROVIDER_ID.to_string(),
                    prompt,
                    model: if self.asset_model_input.trim().is_empty() {
                        default_video_model(OPENAI_VIDEO_PROVIDER_ID).to_string()
                    } else {
                        self.asset_model_input.trim().to_string()
                    },
                    size: self.asset_image_size.clone(),
                    seconds: 8,
                };
                self.start_video_asset_request(request);
            }
            _ => {
                let request = ImageAssetRequest {
                    provider: self.asset_provider_input.clone(),
                    prompt,
                    model: self.asset_model_input.trim().to_string(),
                    aspect_ratio: self.asset_aspect_ratio.clone(),
                    image_size: self.asset_image_size.clone(),
                };
                self.start_image_asset_request(request);
            }
        }
    }

    fn start_image_asset_request(&mut self, request: ImageAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };

        let api_key = image_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Сначала сохраните ключ {} ({}) для генерации изображений",
                image_provider_name(&request.provider),
                image_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_image(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("выполняется: {}", job.id);

        let config = self.config.clone();
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_image_job(workspace, api_key, config, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn start_spritesheet_asset_request(&mut self, request: SpritesheetAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let api_key = image_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Сначала сохраните ключ {} ({}) для генерации спрайт-листов",
                image_provider_name(&request.provider),
                image_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_spritesheet(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("выполняется: {}", job.id);
        let config = self.config.clone();
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_spritesheet_job(
                    workspace, api_key, config, request, job,
                ));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn start_audio_asset_request(&mut self, request: AudioAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let api_key = media_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Сначала сохраните ключ {} ({}) для генерации аудио",
                audio_provider_name(&request.provider),
                asset_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_audio(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("выполняется: {}", job.id);
        let config = self.config.clone();
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_audio_job(workspace, api_key, config, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn start_video_asset_request(&mut self, request: VideoAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let api_key = media_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Сначала сохраните ключ {} ({}) для генерации видео",
                video_provider_name(&request.provider),
                asset_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_video(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("выполняется: {}", job.id);
        let config = self.config.clone();
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_video_job(workspace, api_key, config, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn regenerate_asset_job(&mut self, job: &AssetJob) {
        self.start_image_asset_request(image_request_from_job(job, None));
    }

    fn vary_asset_job(&mut self, job: &AssetJob) {
        let prompt = format!(
            "{}\n\nСоздай отполированную вариацию: сохрани назначение, композицию и пригодность ассета для игры/приложения, но измени визуальные детали достаточно, чтобы получить свежий вариант.",
            job.prompt
        );
        self.start_image_asset_request(image_request_from_job(job, Some(prompt)));
    }

    fn load_asset_job_into_form(&mut self, job: &AssetJob) {
        let request = image_request_from_job(job, None);
        self.asset_provider_input = request.provider;
        self.asset_model_input = request.model;
        self.asset_prompt = request.prompt;
        self.asset_aspect_ratio = request.aspect_ratio;
        self.asset_image_size = request.image_size;
        self.asset_api_key_input =
            image_api_key_from_config(&self.config, &self.asset_provider_input);
        self.asset_status = "промпт ассета загружен".to_string();
    }

    fn open_asset_folder(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let Some(path) = absolute_output_path(workspace, rel_path) else {
            self.asset_status = "файл ассета не найден".to_string();
            return;
        };

        #[cfg(target_os = "windows")]
        let result = Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map(|_| ());
        #[cfg(not(target_os = "windows"))]
        let result = Command::new("open")
            .arg(path.parent().unwrap_or_else(|| workspace.root()))
            .spawn()
            .map(|_| ());

        self.asset_status = match result {
            Ok(()) => "папка ассета открыта".to_string(),
            Err(err) => {
                format!("не удалось открыть папку ассета: {err}")
            }
        };
    }

    fn open_generated_assets_folder(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let folder = match workspace.resolve_for_write("assets/generated/images") {
            Ok(path) => path,
            Err(err) => {
                self.asset_status = format!("не удалось подготовить папку ассетов: {err}");
                return;
            }
        };
        if let Err(err) = fs::create_dir_all(&folder) {
            self.asset_status = format!("не удалось подготовить папку ассетов: {err}");
            return;
        }

        #[cfg(target_os = "windows")]
        let result = Command::new("explorer").arg(&folder).spawn().map(|_| ());
        #[cfg(not(target_os = "windows"))]
        let result = Command::new("open").arg(&folder).spawn().map(|_| ());

        self.asset_status = match result {
            Ok(()) => "папка сгенерированных изображений открыта".to_string(),
            Err(err) => format!("не удалось открыть сгенерированные изображения: {err}"),
        };
    }

    fn use_asset_as_app_icon(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let Some(source) = absolute_output_path(workspace, rel_path) else {
            self.asset_status = "файл ассета не найден".to_string();
            return;
        };
        if !is_image_path(&source) {
            self.asset_status = "ассет не является изображением".to_string();
            return;
        }

        let target = match workspace.resolve_for_write("assets/app-icon.png") {
            Ok(path) => path,
            Err(err) => {
                self.asset_status = format!("не удалось подготовить путь иконки: {err}");
                return;
            }
        };
        if let Some(parent) = target.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                self.asset_status = format!("не удалось создать папку иконки: {err}");
                return;
            }
        }

        let result = fs::read(&source)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| image::load_from_memory(&bytes).map_err(anyhow::Error::from))
            .and_then(|image| {
                image
                    .save_with_format(&target, image::ImageFormat::Png)
                    .map_err(anyhow::Error::from)
            });
        match result {
            Ok(()) => {
                self.asset_status = "сохранено assets/app-icon.png".to_string();
                self.asset_previews.remove("assets/app-icon.png");
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("не удалось сохранить иконку: {err}"),
        }
    }

    fn upscale_asset_output(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        match upscale_asset(workspace, rel_path, 2) {
            Ok(job) => {
                self.asset_status = format!("увеличено: {}", job.id);
                self.upsert_asset_job(job);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("не удалось увеличить ассет: {err}"),
        }
    }

    fn export_asset_output(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        match export_asset(workspace, rel_path, None, None) {
            Ok(job) => {
                self.asset_status = format!("экспортировано: {}", job.id);
                self.upsert_asset_job(job);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("экспорт не выполнен: {err}"),
        }
    }

    fn import_asset_output_to_project(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let target_dir = self.asset_import_target_input.trim().to_string();
        let target_dir = if target_dir.is_empty() {
            default_asset_import_target(&asset_kind_for_rel_path(rel_path)).to_string()
        } else {
            target_dir
        };
        match export_asset(workspace, rel_path, None, Some(&target_dir)) {
            Ok(job) => {
                if let Some(output) = job.output_files.first() {
                    self.asset_status = format!("импортировано в проект: {output}");
                } else {
                    self.asset_status = format!("импортировано в проект: {}", job.id);
                }
                self.upsert_asset_job(job);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("импорт в проект не выполнен: {err}"),
        }
    }

    fn attach_asset_output(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "рабочая папка не выбрана".to_string();
            return;
        };
        match attach_asset_context(workspace, rel_path) {
            Ok(_) => {
                self.asset_status = "контекст ассета прикреплён".to_string();
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("не удалось прикрепить ассет: {err}"),
        }
    }

    fn drain_asset_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.asset_events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            match event {
                AssetEvent::JobUpdated(job) => {
                    self.asset_status = match job.status {
                        AssetStatus::Done => format!("готово: {}", job.id),
                        AssetStatus::Failed => format!(
                            "ошибка {}: {}",
                            job.id,
                            job.error.as_deref().unwrap_or("неизвестная ошибка")
                        ),
                        AssetStatus::Running => format!("выполняется: {}", job.id),
                        AssetStatus::Pending => format!("в очереди: {}", job.id),
                    };
                    self.upsert_asset_job(job);
                    self.refresh_file_rows();
                    self.refresh_git_summary();
                }
                AssetEvent::Done => {
                    self.asset_is_running = false;
                }
            }
        }
    }

    fn upsert_asset_job(&mut self, job: AssetJob) {
        if let Some(existing) = self
            .asset_jobs
            .iter_mut()
            .find(|existing| existing.id == job.id)
        {
            *existing = job;
        } else {
            self.asset_jobs.push(job);
        }
        self.asset_jobs.sort_by_key(|job| job.created_at);
    }

    fn toggle_asset_comparison(&mut self, rel_path: &str) {
        if let Some(index) = self
            .asset_compare_paths
            .iter()
            .position(|known| known == rel_path)
        {
            self.asset_compare_paths.remove(index);
            return;
        }
        if self.asset_compare_paths.len() >= 4 {
            self.asset_compare_paths.remove(0);
        }
        self.asset_compare_paths.push(rel_path.to_string());
        self.asset_import_target_input =
            default_asset_import_target(&asset_kind_for_rel_path(rel_path)).to_string();
    }

    fn asset_is_compared(&self, rel_path: &str) -> bool {
        self.asset_compare_paths
            .iter()
            .any(|known| known == rel_path)
    }

    fn texture_for_asset(
        &mut self,
        ctx: &egui::Context,
        rel_path: &str,
    ) -> Option<&egui::TextureHandle> {
        if self.asset_previews.contains_key(rel_path) {
            return self.asset_previews.get(rel_path);
        }

        let workspace = self.workspace.as_ref()?;
        let path = absolute_output_path(workspace, rel_path)?;
        if !is_image_path(&path) {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        let texture = ctx.load_texture(
            format!("asset-preview-{rel_path}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.asset_previews.insert(rel_path.to_string(), texture);
        self.asset_previews.get(rel_path)
    }

    fn load_file_preview(&mut self, rel: &str) {
        self.set_workspace_mode(WorkspaceMode::Code);
        if let Some(index) = self.file_tabs.iter().position(|tab| tab.path == rel) {
            self.active_center_tab = CenterTab::File(rel.to_string());
            self.sync_selected_from_file_tab(index);
            return;
        }

        let tab = if let Some(workspace) = &self.workspace {
            match workspace.read_text(rel, 2_000_000) {
                Ok(text) => FilePreviewTab {
                    path: rel.to_string(),
                    content: text.clone(),
                    original_content: text,
                    editable: true,
                    status: "загружено".to_string(),
                },
                Err(err) => FilePreviewTab {
                    path: rel.to_string(),
                    content: format!("Не удалось открыть файл для предпросмотра: {err}"),
                    original_content: String::new(),
                    editable: false,
                    status: "нельзя редактировать".to_string(),
                },
            }
        } else {
            FilePreviewTab {
                path: rel.to_string(),
                content: "Рабочая папка не выбрана".to_string(),
                original_content: String::new(),
                editable: false,
                status: "рабочая папка не выбрана".to_string(),
            }
        };

        self.file_tabs.push(tab);
        let index = self.file_tabs.len() - 1;
        self.active_center_tab = CenterTab::File(rel.to_string());
        self.sync_selected_from_file_tab(index);
        let opened_as_tab = true;
        if opened_as_tab {
            return;
        }

        self.selected_file = Some(rel.to_string());
        let Some(workspace) = &self.workspace else {
            self.selected_preview = "Рабочая папка не выбрана".to_string();
            self.original_file_content.clear();
            self.selected_file_editable = false;
            self.editor_status = "рабочая папка не выбрана".to_string();
            return;
        };

        match workspace.read_text(rel, 2_000_000) {
            Ok(text) => {
                self.selected_preview = text.clone();
                self.original_file_content = text;
                self.selected_file_editable = true;
                self.editor_status = "загружено".to_string();
            }
            Err(err) => {
                self.selected_preview =
                    format!("Не удалось открыть файл для редактирования: {err}");
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status = "нельзя редактировать".to_string();
            }
        }
    }

    fn editor_dirty(&self) -> bool {
        if let Some(index) = self.active_file_tab_index() {
            let tab = &self.file_tabs[index];
            return tab.editable && tab.content != tab.original_content;
        }

        self.selected_file_editable && self.selected_preview != self.original_file_content
    }

    fn active_file_tab_index(&self) -> Option<usize> {
        let CenterTab::File(path) = &self.active_center_tab else {
            return None;
        };
        self.file_tabs.iter().position(|tab| &tab.path == path)
    }

    fn sync_selected_from_file_tab(&mut self, index: usize) {
        let Some(tab) = self.file_tabs.get(index) else {
            return;
        };
        self.selected_file = Some(tab.path.clone());
        self.selected_preview = tab.content.clone();
        self.original_file_content = tab.original_content.clone();
        self.selected_file_editable = tab.editable;
        self.editor_status = tab.status.clone();
    }

    fn save_selected_file(&mut self) {
        if let Some(index) = self.active_file_tab_index() {
            let path = self.file_tabs[index].path.clone();
            let content = self.file_tabs[index].content.clone();
            let Some(workspace) = &self.workspace else {
                self.file_tabs[index].status = "рабочая папка не выбрана".to_string();
                self.sync_selected_from_file_tab(index);
                return;
            };

            match workspace.write_text(&path, &content) {
                Ok(()) => {
                    self.file_tabs[index].original_content = content;
                    self.file_tabs[index].status = "сохранено".to_string();
                    self.sync_selected_from_file_tab(index);
                    self.refresh_file_rows();
                    self.refresh_git_summary();
                }
                Err(err) => {
                    self.file_tabs[index].status = format!("не удалось сохранить: {err}");
                    self.sync_selected_from_file_tab(index);
                }
            }
            return;
        }

        let Some(path) = self.selected_file.clone() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            self.editor_status = "рабочая папка не выбрана".to_string();
            return;
        };

        match workspace.write_text(&path, &self.selected_preview) {
            Ok(()) => {
                self.original_file_content = self.selected_preview.clone();
                self.editor_status = "сохранено".to_string();
                self.refresh_file_rows();
            }
            Err(err) => {
                self.editor_status = format!("не удалось сохранить: {err}");
            }
        }
    }

    fn revert_selected_file(&mut self) {
        if let Some(index) = self.active_file_tab_index() {
            if self.file_tabs[index].editable {
                self.file_tabs[index].content = self.file_tabs[index].original_content.clone();
                self.file_tabs[index].status = "изменения отменены".to_string();
                self.sync_selected_from_file_tab(index);
            }
            return;
        }

        if self.selected_file_editable {
            self.selected_preview = self.original_file_content.clone();
            self.editor_status = "изменения отменены".to_string();
        }
    }

    fn reload_selected_file(&mut self) {
        if let Some(index) = self.active_file_tab_index() {
            let path = self.file_tabs[index].path.clone();
            let Some(workspace) = &self.workspace else {
                self.file_tabs[index].status = "рабочая папка не выбрана".to_string();
                self.sync_selected_from_file_tab(index);
                return;
            };

            match workspace.read_text(&path, 2_000_000) {
                Ok(text) => {
                    self.file_tabs[index].content = text.clone();
                    self.file_tabs[index].original_content = text;
                    self.file_tabs[index].editable = true;
                    self.file_tabs[index].status = "перезагружено".to_string();
                }
                Err(err) => {
                    self.file_tabs[index].content = format!("Не удалось перезагрузить файл: {err}");
                    self.file_tabs[index].original_content.clear();
                    self.file_tabs[index].editable = false;
                    self.file_tabs[index].status = "нельзя редактировать".to_string();
                }
            }
            self.sync_selected_from_file_tab(index);
            return;
        }

        let Some(path) = self.selected_file.clone() else {
            return;
        };
        self.load_file_preview(&path);
    }

    fn close_file_tab(&mut self, path: &str) {
        let Some(index) = self.file_tabs.iter().position(|tab| tab.path == path) else {
            return;
        };
        self.file_tabs.remove(index);
        if matches!(&self.active_center_tab, CenterTab::File(active) if active == path) {
            if self.file_tabs.is_empty() {
                self.active_center_tab = CenterTab::Agent;
                self.selected_file = None;
                self.selected_preview.clear();
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status.clear();
            } else {
                let next_index = index.min(self.file_tabs.len() - 1);
                self.active_center_tab = CenterTab::File(self.file_tabs[next_index].path.clone());
                self.sync_selected_from_file_tab(next_index);
            }
        }
    }

    fn handle_input_dropped_files(&mut self, dropped_files: Vec<egui::DroppedFile>) {
        if dropped_files.is_empty() {
            return;
        }

        let mut added = 0usize;
        for dropped in dropped_files {
            let result = if let Some(path) = dropped.path {
                self.attach_external_file(&path, InputAttachmentKind::File)
            } else if let Some(bytes) = dropped.bytes {
                self.attach_dropped_bytes(&dropped.name, bytes.as_ref())
            } else {
                Err("перетащенный объект не содержит путь или байты".to_string())
            };

            match result {
                Ok(attachment) => {
                    self.input_attachments.push(attachment);
                    added += 1;
                }
                Err(err) => {
                    self.input_attachment_status = format!("не удалось добавить файл: {err}");
                }
            }
        }

        if added > 0 {
            self.input_attachment_status = format!("добавлено вложений: {added}");
            self.refresh_file_rows();
        }
    }

    fn choose_input_files(&mut self, requested_kind: InputAttachmentKind) {
        let Some(paths) = rfd::FileDialog::new().pick_files() else {
            return;
        };
        self.attach_input_paths(paths, requested_kind);
    }

    fn choose_input_images(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("Изображения", &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_files()
        else {
            return;
        };
        self.attach_input_paths(paths, InputAttachmentKind::Image);
    }

    fn attach_input_paths(&mut self, paths: Vec<PathBuf>, requested_kind: InputAttachmentKind) {
        let mut added = 0usize;
        for path in paths {
            match self.attach_external_file(&path, requested_kind) {
                Ok(attachment) => {
                    self.input_attachments.push(attachment);
                    added += 1;
                }
                Err(err) => {
                    self.input_attachment_status = format!("не удалось добавить файл: {err}");
                }
            }
        }
        if added > 0 {
            self.input_attachment_status = format!("добавлено вложений: {added}");
            self.refresh_file_rows();
        }
    }

    fn choose_input_folder_context(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let shown = if let Some(workspace) = &self.workspace {
            path.strip_prefix(workspace.root())
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path.display().to_string())
        } else {
            path.display().to_string()
        };
        if !self.input.trim().is_empty() {
            self.input.push_str("\n\n");
        }
        self.input
            .push_str(&format!("Используй папку как контекст: {shown}"));
        self.input_attachment_status = format!("папка добавлена в запрос: {shown}");
    }

    fn attach_external_file(
        &mut self,
        source: &Path,
        requested_kind: InputAttachmentKind,
    ) -> Result<InputAttachment, String> {
        let Some(workspace) = &self.workspace else {
            return Err("сначала выберите рабочую папку".to_string());
        };
        if !source.is_file() {
            return Err(format!("это не файл: {}", source.display()));
        }

        let source = source
            .canonicalize()
            .map_err(|err| format!("не удалось открыть {}: {err}", source.display()))?;
        let kind = if is_supported_image_path(&source) {
            requested_kind.promote_image()
        } else {
            requested_kind
        };

        let rel_path = if let Ok(rel) = source.strip_prefix(workspace.root()) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            let file_name = source
                .file_name()
                .and_then(|name| name.to_str())
                .map(sanitize_attachment_file_name)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "attachment.bin".to_string());
            let rel_path = format!(
                "assets/generated/input_attachments/{}-{file_name}",
                uuid::Uuid::new_v4()
            );
            let target = workspace
                .resolve_for_write(&rel_path)
                .map_err(|err| err.to_string())?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            fs::copy(&source, &target).map_err(|err| err.to_string())?;
            rel_path
        };

        self.record_input_attachment_context(&rel_path);
        let bytes = workspace
            .resolve_existing(&rel_path)
            .ok()
            .and_then(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        Ok(InputAttachment {
            name: source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("attachment")
                .to_string(),
            path: rel_path,
            kind,
            bytes,
        })
    }

    fn attach_dropped_bytes(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> Result<InputAttachment, String> {
        let Some(workspace) = &self.workspace else {
            return Err("сначала выберите рабочую папку".to_string());
        };
        if bytes.is_empty() {
            return Err("пустой файл".to_string());
        }

        let file_name = sanitize_attachment_file_name(if name.trim().is_empty() {
            "attachment.bin"
        } else {
            name
        });
        let rel_path = format!(
            "assets/generated/input_attachments/{}-{file_name}",
            uuid::Uuid::new_v4()
        );
        let target = workspace
            .resolve_for_write(&rel_path)
            .map_err(|err| err.to_string())?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(&target, bytes).map_err(|err| err.to_string())?;
        self.record_input_attachment_context(&rel_path);

        Ok(InputAttachment {
            path: rel_path,
            name: file_name,
            kind: if is_supported_image_path(&target) {
                InputAttachmentKind::Image
            } else {
                InputAttachmentKind::File
            },
            bytes: bytes.len() as u64,
        })
    }

    fn input_paste_shortcut_pressed(&mut self, ui: &egui::Ui) -> bool {
        let egui_pressed = ui.input(|input| {
            let keyboard_paste = input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::V,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.ctrl || modifiers.command || modifiers.mac_cmd
                )
            });
            let paste_event = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_)));
            keyboard_paste || paste_event
        });
        let shortcut_down = platform_ctrl_v_down();
        let platform_pressed = shortcut_down && !self.input_paste_shortcut_down;
        self.input_paste_shortcut_down = shortcut_down;
        egui_pressed || platform_pressed
    }
    fn attach_clipboard_image(&mut self, report_missing: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            self.input_attachment_status = "сначала выберите рабочую папку".to_string();
            return false;
        };

        let rel_path = format!(
            "assets/generated/screenshots/clipboard-{}.png",
            uuid::Uuid::new_v4()
        );
        let target = match workspace.resolve_for_write(&rel_path) {
            Ok(target) => target,
            Err(err) => {
                self.input_attachment_status = format!("не удалось подготовить файл: {err}");
                return false;
            }
        };
        if let Some(parent) = target.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                self.input_attachment_status = format!("не удалось создать папку: {err}");
                return false;
            }
        }
        if let Err(err) = save_clipboard_image_to_file(&target) {
            if report_missing {
                self.input_attachment_status = err;
            }
            return false;
        }

        let bytes = fs::metadata(&target)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        if bytes == 0 {
            self.input_attachment_status = "буфер обмена сохранён как пустой файл".to_string();
            let _ = fs::remove_file(&target);
            return false;
        }

        self.record_input_attachment_context(&rel_path);
        self.input_attachments.push(InputAttachment {
            path: rel_path.clone(),
            name: "clipboard.png".to_string(),
            kind: InputAttachmentKind::Screenshot,
            bytes,
        });
        self.input_attachment_status = format!("скриншот из буфера добавлен: {rel_path}");
        self.refresh_file_rows();
        true
    }

    fn capture_input_screenshot(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.input_attachment_status = "сначала выберите рабочую папку".to_string();
            return;
        };

        match capture_screenshot_file(workspace, "input") {
            Ok(path) => {
                self.record_input_attachment_context(&path);
                let bytes = workspace
                    .resolve_existing(&path)
                    .ok()
                    .and_then(|path| fs::metadata(path).ok())
                    .map(|metadata| metadata.len())
                    .unwrap_or_default();
                self.desktop_last_screenshot = Some(path.clone());
                self.input_attachments.push(InputAttachment {
                    path: path.clone(),
                    name: "screenshot.png".to_string(),
                    kind: InputAttachmentKind::Screenshot,
                    bytes,
                });
                self.input_attachment_status = format!("скриншот добавлен: {path}");
                self.refresh_file_rows();
            }
            Err(err) => {
                self.input_attachment_status = format!("не удалось сделать скриншот: {err}");
            }
        }
    }

    fn record_input_attachment_context(&self, rel_path: &str) {
        if let Some(workspace) = &self.workspace {
            let _ = attach_asset_context(workspace, rel_path);
        }
    }

    fn send_current_input(&mut self) {
        if self.pending_run_gate.is_some() {
            let typed_message = self.input.trim().to_string();
            if typed_message.is_empty() {
                return;
            }
            if is_confirmation_text(&typed_message) {
                self.input.clear();
                self.answer_run_gate_with_action(ApprovalQuickAction::Approve);
            } else {
                self.revise_pending_run_gate_from_input();
            }
            return;
        }
        let typed_message = self.input.trim().to_string();
        if (typed_message.is_empty() && self.input_attachments.is_empty()) || self.is_running {
            return;
        }
        if self.workspace.is_none() {
            self.chat
                .push(ChatLine::system("Сначала выберите папку проекта."));
            return;
        }

        self.save_settings_from_ui();

        let attachments = self.input_attachments.clone();
        let message = format_input_message_with_attachments(&typed_message, &attachments);
        self.input.clear();
        self.input_attachments.clear();
        self.input_attachment_status.clear();
        let user_message_index = self.chat.len();
        self.chat.push(ChatLine::user(message.clone()));
        self.agent_user_message_index = Some(user_message_index);
        self.agent_chat_start_index = Some(self.chat.len());
        self.agent_live_status = "Агент размышляет".to_string();
        self.run_timeline = Some(RunTimeline::new(&message));
        self.run_timeline_anchor_index = Some(user_message_index);
        append_journal(format!("user_input\t{}", compact(&message, 500)));
        self.refresh_journal();
        self.persist_current_conversation();
        if should_require_run_gate(&message) {
            let gate = self.build_pre_run_gate(&message, &attachments);
            self.agent_live_status = "Ждёт подтверждение плана".to_string();
            self.active_center_tab = CenterTab::Agent;
            if let Some(timeline) = &mut self.run_timeline {
                timeline.set_plan_detail(gate.detail.clone());
                timeline.pre_run_gate_requested(gate.summary.clone(), gate.detail.clone());
            }
            self.pending_run_gate = Some(gate);
            self.run_timeline_anchor_index = Some(user_message_index);
            append_journal(format!("run_gate\tpending\t{}", compact(&message, 300)));
            self.refresh_journal();
            self.persist_current_conversation();
            return;
        }

        self.launch_agent_run(message.clone(), message, None);
    }

    fn revise_pending_run_gate_from_input(&mut self) {
        let Some(previous_gate) = self.pending_run_gate.take() else {
            return;
        };
        let typed_message = self.input.trim().to_string();
        if typed_message.is_empty() {
            self.pending_run_gate = Some(previous_gate);
            return;
        }

        let attachments = self.input_attachments.clone();
        let clarification = format_input_message_with_attachments(&typed_message, &attachments);
        self.input.clear();
        self.input_attachments.clear();
        self.input_attachment_status.clear();

        let revised_message = format!(
            "{}\n\nУточнение пользователя:\n{}",
            previous_gate.original_message, clarification
        );
        self.chat.push(ChatLine::user(clarification.clone()));
        let gate = self.build_pre_run_gate(&revised_message, &attachments);
        self.agent_live_status = "План обновлён, ждёт подтверждение".to_string();
        if let Some(timeline) = &mut self.run_timeline {
            timeline.set_plan_detail(gate.detail.clone());
            timeline.pre_run_gate_requested(
                "План обновлён после уточнения пользователя",
                gate.detail.clone(),
            );
        }
        self.pending_run_gate = Some(gate);
        self.run_timeline_anchor_index = Some(self.chat.len().saturating_sub(1));
        append_journal(format!(
            "run_gate\trevised\t{}",
            compact(&revised_message, 300)
        ));
        self.refresh_journal();
        self.persist_current_conversation();
    }

    fn build_pre_run_gate(&self, message: &str, attachments: &[InputAttachment]) -> PendingRunGate {
        let stage = requested_stage_number(message);
        let stage_context = stage.and_then(|number| self.backlog_stage_context(number));
        let objective = if let Some(number) = stage {
            format!("нужно выполнить или доработать этап {number} из бэклога проекта")
        } else {
            format!("нужно выполнить задачу: {}", compact_inline(message, 180))
        };
        let summary = format!("Правильно ли я понимаю: {objective}?");
        let attachment_note = if attachments.is_empty() {
            "Вложений нет.".to_string()
        } else {
            format!("Вложений в запросе: {}.", attachments.len())
        };
        let context = stage_context.unwrap_or_else(|| {
            format!(
                "Ключевой контекст беру из запроса пользователя:\n- {}",
                compact(message, 700)
            )
        });
        let detail = format!(
            "Моё видение задачи:\n{context}\n\nКак буду выполнять после подтверждения:\n1. Сначала сверю текущее состояние проекта, бэклога и затронутых модулей.\n2. Затем внесу минимальные изменения в нужные файлы, без лишних рефакторингов.\n3. После этого соберу проект, запущу доступные проверки и кратко отчитаюсь о результате.\n\n{attachment_note}\n\nЕсли план подходит, нажмите `Подтверждаю`. Если нет — напишите уточнение в поле ввода, и я пересоберу план без запуска."
        );
        PendingRunGate {
            original_message: message.to_string(),
            summary,
            detail,
        }
    }

    fn backlog_stage_context(&self, stage: usize) -> Option<String> {
        let text = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.read_text("BACKLOG.md", 400_000).ok())
            .or_else(|| fs::read_to_string("BACKLOG.md").ok())?;
        let section = extract_backlog_stage_section(&text, stage)?;
        let bullets = section
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("- "))
            .take(8)
            .map(clean_backlog_bullet)
            .collect::<Vec<_>>();
        if bullets.is_empty() {
            Some(format!(
                "Этап {stage} найден в бэклоге, но без явных пунктов. Буду опираться на текст раздела:\n- {}",
                compact_inline(&section, 500)
            ))
        } else {
            Some(format!(
                "Этап {stage} по бэклогу включает:\n{}",
                bullets
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
    }

    fn stop_run(&mut self) {
        self.agent_live_status = "Агент останавливается".to_string();
        if let Some(timeline) = &mut self.run_timeline {
            timeline.cancel_requested();
        }
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if self.pending_run_gate.is_some() {
            self.answer_run_gate_with_action(ApprovalQuickAction::Deny);
        }
        if self.pending_approval.is_some() {
            self.answer_approval(false);
        }
        self.tool_log.push(ToolLogLine {
            title: "стоп".to_string(),
            content: "Остановка запрошена".to_string(),
        });
    }

    fn reset_conversation(&mut self) {
        self.create_new_chat();
        self.agent_state
            .lock()
            .expect("agent state poisoned")
            .reset();
        self.agent_started_at = None;
        self.agent_user_message_index = None;
        self.agent_chat_start_index = None;
        self.agent_live_status.clear();
        self.run_timeline = None;
        self.run_timeline_anchor_index = None;
        self.active_run_history = None;
        self.self_modification_guard = None;
        self.self_modification_status.clear();
        self.pending_run_gate = None;
        self.tool_log.clear();
        self.persist_current_conversation();
    }

    fn mark_latest_agent_message_elapsed(&mut self, elapsed: String) {
        let start_index = self.agent_chat_start_index.unwrap_or(0);
        for (index, line) in self.chat.iter_mut().enumerate().rev() {
            if index < start_index {
                break;
            }
            if matches!(line.role, ChatRole::Assistant) {
                line.elapsed = Some(elapsed);
                return;
            }
        }
    }

    fn latest_agent_response_for_active_run(&self) -> Option<String> {
        let start_index = self.agent_chat_start_index.unwrap_or(0);
        self.chat
            .iter()
            .enumerate()
            .rev()
            .take_while(|(index, _)| *index >= start_index)
            .find_map(|(_, line)| {
                if matches!(line.role, ChatRole::Assistant) && !line.content.trim().is_empty() {
                    Some(line.content.clone())
                } else {
                    None
                }
            })
    }

    fn persist_agent_history(
        &mut self,
        context: Option<AgentRunHistoryContext>,
        final_response: Option<String>,
    ) {
        let Some(context) = context else {
            return;
        };
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(timeline) = &self.run_timeline else {
            return;
        };

        let record = AgentRunHistoryRecord::from_timeline(&context, timeline, final_response);
        match append_agent_history(&workspace, &record) {
            Ok(()) => {
                self.agent_history = load_agent_history_tail(&workspace, 80);
                self.agent_history_status = format!(
                    "сохранён запуск {} · {}",
                    compact_inline(&record.id, 32),
                    AGENT_HISTORY_PATH
                );
                append_journal(format!(
                    "agent_history\tsaved\t{}\t{}",
                    record.id, AGENT_HISTORY_PATH
                ));
            }
            Err(err) => {
                self.agent_history_status = format!("не удалось сохранить историю запуска: {err}");
                append_journal(format!("agent_history\terror\t{err}"));
            }
        }
    }

    fn prepare_self_modification_guard_for_run(&mut self, user_request: &str) -> bool {
        self.self_modification_guard = None;
        let Some(workspace) = self.workspace.clone() else {
            return true;
        };
        if !is_leetcode_workspace(&workspace) {
            self.self_modification_status.clear();
            return true;
        }

        self.refresh_git_summary();
        match prepare_self_modification_guard(
            &workspace,
            user_request,
            self.git_changed_files.clone(),
        ) {
            Ok(Some(guard)) => {
                let snapshot = guard.snapshot.clone();
                self.self_modification_status = format!(
                    "self-mod snapshot: {} · файлов: {}",
                    snapshot.id, snapshot.files_copied
                );
                self.tool_log.push(ToolLogLine {
                    title: "self-mod snapshot".to_string(),
                    content: format!(
                        "{}\nRestore path: {}/files",
                        self.self_modification_status, snapshot.rel_path
                    ),
                });
                if let Some(timeline) = &mut self.run_timeline {
                    timeline.note_with_link(
                        format!("selfmod-snapshot-{}", snapshot.id),
                        "Self-mod snapshot",
                        format!(
                            "Перед изменением Leetcode создан restore snapshot: {} · файлов: {}",
                            snapshot.rel_path, snapshot.files_copied
                        ),
                        snapshot.rel_path.clone(),
                    );
                }
                append_journal(format!(
                    "self_modification\tsnapshot\t{}\t{}",
                    snapshot.id, snapshot.rel_path
                ));
                self.self_modification_guard = Some(guard);
                true
            }
            Ok(None) => {
                self.self_modification_status.clear();
                true
            }
            Err(err) => {
                let message = format!(
                    "Self-modification остановлен: не удалось создать restore snapshot перед изменением Leetcode: {err}"
                );
                self.self_modification_status = message.clone();
                self.chat.push(ChatLine::system(message.clone()));
                self.tool_log.push(ToolLogLine {
                    title: "self-mod snapshot error".to_string(),
                    content: message.clone(),
                });
                if let Some(timeline) = &mut self.run_timeline {
                    timeline.fail(&message);
                    timeline.finish(&self.git_changed_files);
                }
                self.agent_live_status.clear();
                self.is_running = false;
                self.cancel = None;
                false
            }
        }
    }

    fn finish_self_modification_guard(&mut self) {
        let Some(guard) = self.self_modification_guard.take() else {
            return;
        };
        let Some(workspace) = self.workspace.clone() else {
            return;
        };

        self.refresh_git_summary();
        let validation =
            run_self_modification_validation(&workspace, guard, &self.git_changed_files);
        self.self_modification_status = validation.short_status();
        let report = validation.report();
        if let Some(timeline) = &mut self.run_timeline {
            timeline.validation_result(
                format!("selfmod-validation-{}", validation.snapshot.id),
                "Self-mod validation",
                report.clone(),
                validation.success,
            );
        }
        self.tool_log.push(ToolLogLine {
            title: "self-mod validation".to_string(),
            content: report.clone(),
        });
        append_journal(format!(
            "self_modification\tvalidation\t{}\t{}\t{}",
            validation.snapshot.id,
            if validation.success { "ok" } else { "failed" },
            validation.snapshot.rel_path
        ));
        if validation.ran && !validation.success {
            self.chat.push(ChatLine::system(format!(
                "Self-check не прошёл. Snapshot для восстановления: {}/files. Можно попросить агента исправить ошибку по отчёту self-mod validation или восстановить файлы из snapshot.",
                validation.snapshot.rel_path
            )));
        }
        self.refresh_git_summary();
    }

    fn drain_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        let had_events = !events.is_empty();
        let mut chat_changed = false;

        for event in events {
            append_journal(format!("event\t{}", compact(&format!("{event:?}"), 2_000)));
            match event {
                AppEvent::AssistantText(text) => {
                    self.agent_live_status = "Агент пишет ответ".to_string();
                    self.chat.push(ChatLine::assistant(text));
                    chat_changed = true;
                    self.run_timeline_anchor_index = Some(self.chat.len().saturating_sub(1));
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.mark_assistant_text();
                    }
                }
                AppEvent::AssistantDelta(delta) => {
                    self.agent_live_status = "Агент пишет ответ".to_string();
                    if let Some(last) = self.chat.last_mut() {
                        if matches!(last.role, ChatRole::Assistant) {
                            last.content.push_str(&delta);
                        } else {
                            self.chat.push(ChatLine::assistant(delta));
                        }
                    } else {
                        self.chat.push(ChatLine::assistant(delta));
                    }
                    chat_changed = true;
                    self.run_timeline_anchor_index = Some(self.chat.len().saturating_sub(1));
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.mark_assistant_text();
                    }
                }
                AppEvent::ToolStarted { id, name, summary } => {
                    self.agent_live_status = agent_live_status_for_tool(&name, &summary);
                    self.tool_log.push(ToolLogLine {
                        title: format!("{name} {id}"),
                        content: summary.clone(),
                    });
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.tool_started(id, name, summary);
                    }
                }
                AppEvent::ToolOutput { id, chunk } => {
                    if id == "routing" {
                        self.agent_live_status = "Агент выбирает маршрут и модель".to_string();
                    }
                    self.append_project_run_output(&id, &chunk);
                    self.tool_log.push(ToolLogLine {
                        title: format!("вывод {id}"),
                        content: chunk.clone(),
                    });
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.tool_output(&id, &chunk);
                    }
                }
                AppEvent::ToolFinished { id, output } => {
                    self.agent_live_status = "Агент анализирует результат инструмента".to_string();
                    self.update_desktop_state_from_tool_output(&output);
                    self.tool_log.push(ToolLogLine {
                        title: format!("готово {id}"),
                        content: compact(&output, 2_000),
                    });
                    if let Some(workspace) = &self.workspace {
                        self.asset_jobs = load_jobs(workspace);
                    }
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.tool_finished(&id, &output);
                    }
                }
                AppEvent::ApprovalRequested {
                    id,
                    summary,
                    detail,
                } => {
                    self.agent_live_status = "Агент ждёт подтверждение действия".to_string();
                    self.active_center_tab = CenterTab::Agent;
                    self.pending_approval = Some(PendingApproval {
                        id: id.clone(),
                        summary: summary.clone(),
                        detail: detail.clone(),
                    });
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.approval_requested(id, summary, detail);
                    }
                }
                AppEvent::Error(err) => {
                    self.agent_live_status = "Агент получил ошибку".to_string();
                    self.chat.push(ChatLine::system(format!("Ошибка: {err}")));
                    chat_changed = true;
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.fail(&err);
                    }
                }
                AppEvent::Done => {
                    let final_response = self.latest_agent_response_for_active_run();
                    let elapsed = self
                        .agent_started_at
                        .take()
                        .map(|started_at| started_at.elapsed());
                    if let Some(elapsed) = elapsed {
                        self.mark_latest_agent_message_elapsed(format_duration(elapsed));
                    }
                    let history_context = self.active_run_history.take();
                    self.agent_user_message_index = None;
                    self.agent_chat_start_index = None;
                    self.agent_live_status.clear();
                    self.is_running = false;
                    self.cancel = None;
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    self.refresh_git_summary();
                    if self.selected_file.is_some()
                        && self.selected_file_editable
                        && !self.editor_dirty()
                    {
                        self.reload_selected_file();
                    }
                    self.tool_log.push(ToolLogLine {
                        title: "запуск".to_string(),
                        content: "Запуск агента завершён".to_string(),
                    });
                    self.finish_self_modification_guard();
                    let changed_files = self.git_changed_files.clone();
                    self.append_run_timeline_context();
                    if let Some(timeline) = &mut self.run_timeline {
                        timeline.finish(&changed_files);
                    }
                    self.suggest_context_notes_after_run(
                        history_context.as_ref(),
                        final_response.as_deref(),
                        &changed_files,
                        elapsed,
                    );
                    self.persist_agent_history(history_context, final_response);
                    chat_changed = true;
                }
            }
        }

        if had_events {
            self.refresh_journal();
        }
        if chat_changed {
            self.persist_current_conversation();
        }
    }

    fn update_desktop_state_from_tool_output(&mut self, output: &str) {
        let trimmed = output.trim();
        if trimmed.starts_with("assets/generated/screenshots/") && trimmed.ends_with(".png") {
            self.desktop_last_screenshot = Some(trimmed.to_string());
            self.desktop_status = format!("screenshot: {trimmed}");
            return;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            return;
        };

        if let Some(path) = value
            .get("after_screenshot")
            .and_then(serde_json::Value::as_str)
        {
            self.desktop_last_screenshot = Some(path.to_string());
            self.desktop_status = value
                .get("action")
                .and_then(serde_json::Value::as_str)
                .map(|action| format!("desktop step: {action}"))
                .unwrap_or_else(|| "desktop step finished".to_string());
        } else if let Some(path) = value
            .get("before_screenshot")
            .and_then(serde_json::Value::as_str)
        {
            self.desktop_last_screenshot = Some(path.to_string());
        }

        if let Some(window) = value
            .get("after_window")
            .or_else(|| value.get("active_window"))
            .or_else(|| {
                if value.get("title").is_some() && value.get("process_name").is_some() {
                    Some(&value)
                } else {
                    None
                }
            })
        {
            self.desktop_active_window = summarize_window_value(window);
        }
    }

    fn drain_project_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.project_events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        let had_events = !events.is_empty();

        for event in events {
            append_journal(format!(
                "project_event\t{}",
                compact(&format!("{event:?}"), 2_000)
            ));
            match event {
                AppEvent::ToolStarted { id, name, summary } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("{name} {id}"),
                        content: summary,
                    });
                }
                AppEvent::ToolOutput { id, chunk } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("вывод {id}"),
                        content: chunk,
                    });
                }
                AppEvent::ToolFinished { id, output } => {
                    self.finish_project_run(&id, &output);
                    self.tool_log.push(ToolLogLine {
                        title: format!("готово {id}"),
                        content: compact(&output, 2_000),
                    });
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    self.refresh_git_summary();
                }
                AppEvent::ApprovalRequested {
                    id,
                    summary,
                    detail,
                } => {
                    self.active_center_tab = CenterTab::Agent;
                    self.pending_approval = Some(PendingApproval {
                        id,
                        summary,
                        detail,
                    });
                }
                AppEvent::Error(err) => {
                    self.fail_active_project_run(&err);
                    self.project_status = format!("ошибка: {err}");
                    self.tool_log.push(ToolLogLine {
                        title: "ошибка проекта".to_string(),
                        content: err,
                    });
                }
                AppEvent::Done => {
                    self.project_is_running = false;
                    self.project_cancel = None;
                    self.active_project_run_id = None;
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    self.refresh_git_summary();
                }
                AppEvent::AssistantText(_) | AppEvent::AssistantDelta(_) => {}
            }
        }

        if had_events {
            self.refresh_journal();
        }
    }

    fn start_provider_live_validation(&mut self, provider_id: String) {
        if self.provider_validation_running {
            return;
        }

        self.save_settings_from_ui();
        let config = self.config.clone();
        let provider_label = provider_name(&provider_id).to_string();
        let (tx, rx) = mpsc::channel();
        self.provider_validation_rx = Some(rx);
        self.provider_validation_running = true;
        self.provider_health_status = format!("проверка провайдера {provider_label}...");

        thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_provider_live_validation(config, provider_id));
            let _ = tx.send(result);
        });
    }

    fn drain_provider_validation_events(&mut self) {
        let mut results = Vec::new();
        if let Some(rx) = &self.provider_validation_rx {
            while let Ok(result) = rx.try_recv() {
                results.push(result);
            }
        }

        for result in results {
            self.provider_validation_running = false;
            self.provider_health_status = if result.ok {
                format!(
                    "{} {}: live-проверка пройдена за {} мс",
                    result.provider_name, result.model, result.elapsed_ms
                )
            } else {
                format!(
                    "{} {}: live-проверка не прошла",
                    result.provider_name, result.model
                )
            };
            if let Some(workspace) = &self.workspace {
                match record_provider_validation_run(workspace, result.clone()) {
                    Ok(history) => {
                        self.provider_validation_results = history.runs;
                    }
                    Err(err) => {
                        self.provider_health_status =
                            format!("результат проверки получен, но не сохранён: {err}");
                        self.provider_validation_results.push(result);
                    }
                }
            } else {
                self.provider_validation_results.push(result);
            }
            if self.provider_validation_results.len() > 100 {
                let overflow = self.provider_validation_results.len() - 100;
                self.provider_validation_results.drain(0..overflow);
            }
            self.provider_validation_rx = None;
        }
    }

    fn start_asset_smoke_validation(&mut self, provider_id: &str) {
        if self.asset_is_running {
            self.provider_health_status =
                "дождитесь завершения текущей генерации ассета".to_string();
            return;
        }

        self.save_settings_from_ui();
        self.set_workspace_mode(WorkspaceMode::Assets);
        self.right_panel_view = RightPanelView::Control;

        match provider_id {
            OPENAI_AUDIO_PROVIDER_ID => {
                let request = AudioAssetRequest {
                    provider: OPENAI_AUDIO_PROVIDER_ID.to_string(),
                    prompt: "Say exactly: Leetcode provider validation passed.".to_string(),
                    model: media_model_from_config(
                        &self.config,
                        OPENAI_AUDIO_PROVIDER_ID,
                        default_audio_model(OPENAI_AUDIO_PROVIDER_ID),
                    ),
                    voice: "alloy".to_string(),
                    format: "wav".to_string(),
                };
                self.start_audio_asset_request(request);
            }
            OPENAI_VIDEO_PROVIDER_ID => {
                let request = VideoAssetRequest {
                    provider: OPENAI_VIDEO_PROVIDER_ID.to_string(),
                    prompt: "A one second neutral desktop app validation clip with a simple glowing checkmark."
                        .to_string(),
                    model: media_model_from_config(
                        &self.config,
                        OPENAI_VIDEO_PROVIDER_ID,
                        default_video_model(OPENAI_VIDEO_PROVIDER_ID),
                    ),
                    size: "1280x720".to_string(),
                    seconds: 1,
                };
                self.start_video_asset_request(request);
            }
            provider_id => {
                let provider_id = provider_id.to_string();
                self.asset_provider_input = provider_id.clone();
                self.asset_kind_input = "image".to_string();
                self.asset_api_key_input = image_api_key_from_config(&self.config, &provider_id);
                self.asset_model_input = image_model_from_config(&self.config, &provider_id);
                let request = ImageAssetRequest {
                    provider: provider_id,
                    prompt: "Small provider validation image: a clean checkmark icon for a desktop development tool, no text.".to_string(),
                    model: self.asset_model_input.clone(),
                    aspect_ratio: "1:1".to_string(),
                    image_size: "0.5K".to_string(),
                };
                self.start_image_asset_request(request);
            }
        }

        self.provider_health_status =
            "asset smoke запущен вручную; результат появится в Asset Studio".to_string();
    }

    fn answer_approval(&mut self, approved: bool) {
        self.answer_approval_with_action(if approved {
            ApprovalQuickAction::Approve
        } else {
            ApprovalQuickAction::Deny
        });
    }

    fn launch_agent_run(
        &mut self,
        message: String,
        user_request: String,
        confirmed_plan: Option<AgentRunConfirmedPlan>,
    ) {
        if !self.prepare_self_modification_guard_for_run(&user_request) {
            self.persist_current_conversation();
            return;
        }

        let (tx, rx) = mpsc::channel();
        let config = self.config.clone();
        let workspace = self.workspace.clone();
        let state = self.agent_state.clone();
        let approvals = self.approvals.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let context_budget = self.context_budget();
        let context_snapshot = self.workspace.as_ref().and_then(|workspace| {
            self.active_conversation_id
                .as_deref()
                .map(|conversation_id| {
                    compile_context_snapshot_with_budget(
                        workspace,
                        conversation_id,
                        &self.chat,
                        &user_request,
                        context_budget,
                    )
                })
        });

        self.events_rx = Some(rx);
        self.cancel = Some(cancel);
        self.agent_started_at = Some(Instant::now());
        self.active_run_history = self.workspace.as_ref().map(|workspace| {
            AgentRunHistoryContext::new(
                self.config.provider_id().to_string(),
                self.config.model.clone(),
                self.config.task_route.clone(),
                self.config.policy_profile.clone(),
                workspace,
                user_request,
                confirmed_plan,
            )
        });
        self.is_running = true;
        self.tool_log.push(ToolLogLine {
            title: "запуск".to_string(),
            content: "Запуск агента начат".to_string(),
        });

        thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .expect("не удалось запустить tokio runtime")
                .block_on(run_user_turn(
                    message,
                    config,
                    workspace,
                    state,
                    tx.clone(),
                    approvals,
                    worker_cancel,
                    context_snapshot,
                ));

            if let Err(err) = result {
                let _ = tx.send(AppEvent::Error(err.to_string()));
            }
            let _ = tx.send(AppEvent::Done);
        });
    }

    fn answer_run_gate_with_action(&mut self, action: ApprovalQuickAction) {
        let Some(gate) = self.pending_run_gate.take() else {
            return;
        };
        let approved = action.approves();
        let note = if approved {
            "План подтверждён пользователем; запуск продолжается."
        } else {
            match action {
                ApprovalQuickAction::Revise => "Пользователь запросил уточнение плана до запуска.",
                ApprovalQuickAction::AnalysisOnly => {
                    "Пользователь ограничил запуск режимом только анализа."
                }
                ApprovalQuickAction::Restrict => {
                    "Пользователь запросил продолжение с ограничениями до запуска."
                }
                ApprovalQuickAction::Deny => "Пользователь отклонил запуск после плана.",
                ApprovalQuickAction::Approve => "План подтверждён пользователем.",
            }
        };
        if let Some(timeline) = &mut self.run_timeline {
            timeline.pre_run_gate_answered(approved, note);
        }
        self.chat.push(ChatLine::assistant(format!(
            "{}\n\n{}",
            gate.summary, gate.detail
        )));
        let assistant_plan_index = self.chat.len().saturating_sub(1);
        self.run_timeline_anchor_index = Some(self.chat.len().saturating_sub(1));
        append_journal(format!(
            "run_gate\t{}\t{}",
            if approved { "approved" } else { action.label() },
            compact(&gate.summary, 200)
        ));
        self.refresh_journal();
        if approved {
            self.chat.push(ChatLine::user("Подтверждаю"));
            let confirmation_index = self.chat.len().saturating_sub(1);
            self.agent_user_message_index = Some(confirmation_index);
            self.agent_chat_start_index = Some(self.chat.len());
            self.run_timeline_anchor_index = Some(assistant_plan_index);
            self.agent_live_status = "Агент работает по подтверждённому плану".to_string();
            self.launch_agent_run(
                confirmed_run_message(&gate),
                gate.original_message.clone(),
                Some(AgentRunConfirmedPlan::new(gate.summary, gate.detail)),
            );
        } else {
            if let Some(reply) = action.user_reply() {
                self.chat.push(ChatLine::user(reply));
                self.input = reply.to_string();
            }
            self.agent_live_status = "Ожидает уточнение пользователя".to_string();
            self.is_running = false;
        }
        self.persist_current_conversation();
    }

    fn answer_approval_with_action(&mut self, action: ApprovalQuickAction) {
        let Some(prompt) = self.pending_approval.take() else {
            return;
        };

        let approved = action.approves();
        let sender = self
            .approvals
            .lock()
            .expect("approval map poisoned")
            .remove(&prompt.id);
        if let Some(sender) = sender {
            let _ = sender.send(approved);
        }

        let decision = if approved {
            "Согласовано"
        } else {
            action.label()
        };
        let mut message = format!(
            "{} — {decision}\n\n{}",
            prompt.summary,
            compact(&prompt.detail, 1_500)
        );
        if let Some(reply) = action.user_reply() {
            message.push_str("\n\nБыстрый ответ пользователя:\n");
            message.push_str(reply);
        }
        self.chat.push(ChatLine::assistant(message));
        if let Some(reply) = action.user_reply() {
            self.chat.push(ChatLine::user(reply));
        }
        self.run_timeline_anchor_index = Some(self.chat.len().saturating_sub(1));
        self.active_center_tab = CenterTab::Agent;

        if let Some(timeline) = &mut self.run_timeline {
            timeline.approval_answered(&prompt.id, approved);
        }
        self.tool_log.push(ToolLogLine {
            title: "подтверждение".to_string(),
            content: if approved {
                format!("Разрешено: {}", prompt.summary)
            } else {
                format!("{}: {}", action.label(), prompt.summary)
            },
        });
        append_journal(format!(
            "approval\t{}\t{}",
            if approved { "approved" } else { "denied" },
            prompt.summary
        ));
        self.refresh_journal();

        if let Some(reply) = action.user_reply() {
            self.input = reply.to_string();
        }
        self.persist_current_conversation();
    }

    fn append_run_timeline_context(&mut self) {
        let Some(timeline) = &mut self.run_timeline else {
            return;
        };

        for line in self.journal_lines.iter().rev().take(3).rev() {
            timeline.note(
                format!("journal-{}", compact(line, 32)),
                "Журнал",
                compact(line, 240),
            );
        }

        for run in self.project_runs.iter().rev().take(2).rev() {
            timeline.note(
                format!("project-run-{}", run.id),
                format!("Команда проекта: {}", run.label),
                format!(
                    "{} · статус: {}{}",
                    run.shell_command,
                    run.status.label(),
                    run.exit_code
                        .map(|code| format!(" · exit code {code}"))
                        .unwrap_or_default()
                ),
            );
        }

        if let Some(workspace) = &self.workspace {
            let orchestration = load_orchestration_state(workspace);
            if let Some(summary) = orchestration.run_summaries.last() {
                timeline.note_with_link(
                    format!("orchestration-{}", summary.id),
                    "Сводка оркестрации",
                    compact(&summary.summary, 240),
                    "assets/generated/orchestration",
                );
            }

            let evals = load_results(workspace);
            if let Some(run) = evals.runs.last() {
                timeline.note_with_link(
                    format!("eval-{}", run.id),
                    format!("Replay eval: {}", run.name),
                    format!(
                        "{} · проверок: {} · issues: {}",
                        run.status,
                        run.checks.len(),
                        run.issues.len()
                    ),
                    "assets/generated/leetcode/eval_results.json",
                );
            }
        }
    }

    fn show_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar")
            .exact_height(34.0)
            .frame(menu_bar_frame())
            .show(ctx, |ui| {
                ui.add_space(2.0);
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Файл", |ui| {
                        if ui.button("Выбрать проект...").clicked() {
                            self.choose_workspace();
                            ui.close_menu();
                        }
                        if ui.button("Обновить файлы").clicked() {
                            self.refresh_file_rows();
                            ui.close_menu();
                        }
                        if ui.button("Сохранить настройки").clicked() {
                            self.save_settings_from_ui();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Сбросить диалог").clicked() {
                            self.reset_conversation();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Инструменты", |ui| {
                        if ui.button("Рабочая сводка").clicked() {
                            self.right_panel_view = RightPanelView::Overview;
                            ui.close_menu();
                        }
                        if ui.button("Проект").clicked() {
                            self.right_panel_view = RightPanelView::Project;
                            ui.close_menu();
                        }
                        if ui.button("Ассеты").clicked() {
                            self.right_panel_view = RightPanelView::Assets;
                            ui.close_menu();
                        }
                        if ui.button("Контроль").clicked() {
                            self.right_panel_view = RightPanelView::Control;
                            ui.close_menu();
                        }
                        if ui.button("Логи").clicked() {
                            self.right_panel_view = RightPanelView::Logs;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Настройка", |ui| {
                        ui.set_min_width(460.0);
                        ui.label(RichText::new("Сеть и proxy").strong());
                        ui.checkbox(
                            &mut self.config.proxy_use_system,
                            "Использовать системные HTTP_PROXY / HTTPS_PROXY / NO_PROXY",
                        );
                        ui.checkbox(
                            &mut self.config.proxy_enabled,
                            "Использовать ручной proxy для API",
                        );

                        ui.add_space(4.0);
                        egui::Grid::new("proxy_settings_grid")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Тип").weak().small());
                                egui::ComboBox::from_id_salt("proxy_scheme_select")
                                    .selected_text(&self.config.proxy_scheme)
                                    .width(112.0)
                                    .show_ui(ui, |ui| {
                                        for scheme in ["http", "https", "socks5", "socks5h"] {
                                            ui.selectable_value(
                                                &mut self.config.proxy_scheme,
                                                scheme.to_string(),
                                                scheme,
                                            );
                                        }
                                    });
                                ui.end_row();

                                ui.label(RichText::new("Host").weak().small());
                                ui.add(
                                    TextEdit::singleline(&mut self.config.proxy_host)
                                        .hint_text("127.0.0.1"),
                                );
                                ui.end_row();

                                ui.label(RichText::new("Port").weak().small());
                                ui.add(
                                    TextEdit::singleline(&mut self.config.proxy_port)
                                        .hint_text("7890"),
                                );
                                ui.end_row();

                                ui.label(RichText::new("Логин").weak().small());
                                ui.add(TextEdit::singleline(&mut self.config.proxy_username));
                                ui.end_row();

                                ui.label(RichText::new("Пароль").weak().small());
                                ui.add(
                                    TextEdit::singleline(&mut self.config.proxy_password)
                                        .password(true),
                                );
                                ui.end_row();

                                ui.label(RichText::new("Без proxy").weak().small());
                                ui.add(
                                    TextEdit::singleline(&mut self.config.proxy_no_proxy)
                                        .hint_text("localhost,127.0.0.1,.local"),
                                );
                                ui.end_row();
                            });

                        self.config.normalize_proxy_settings();
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(proxy_system_status_label(&self.config))
                                .weak()
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "Ручной proxy: {}",
                                proxy_status_label(&self.config)
                            ))
                            .weak()
                            .small(),
                        );
                        ui.separator();
                        if ui.button("Сохранить настройки").clicked() {
                            self.save_settings_from_ui();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Помощь", |ui| {
                        ui.set_min_width(320.0);
                        ui.label(RichText::new("Leetcode").strong());
                        ui.label("Локальный AI-агент для кода, ассетов и рабочего стола.");
                        ui.separator();
                        ui.label(format!("Версия {}", env!("CARGO_PKG_VERSION")));
                    });
                });
            });
    }

    fn show_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(56.0)
            .frame(top_bar_frame())
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Leetcode")
                            .strong()
                            .size(20.0)
                            .color(accent_color()),
                    );
                    ui.label(RichText::new("AI-рабочий стол").weak().small());
                    ui.add_space(12.0);
                    for mode in WorkspaceMode::ALL {
                        let selected = self.workspace_mode == mode;
                        let response = ui
                            .selectable_label(selected, RichText::new(mode.label()).strong())
                            .on_hover_text(mode.subtitle());
                        if response.clicked() {
                            self.set_workspace_mode(mode);
                        }
                    }
                    ui.menu_button("Вид", |ui| {
                        ui.set_min_width(260.0);
                        ui.label(RichText::new("Быстрые режимы").strong());
                        ui.add_space(4.0);
                        for preset in LayoutPreset::ALL {
                            if ui
                                .button(preset.label())
                                .on_hover_text(preset.description())
                                .clicked()
                            {
                                self.apply_layout_preset(preset);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        let file_panel_label = if self.file_panel_collapsed {
                            "Показать проводник"
                        } else {
                            "Свернуть проводник"
                        };
                        if ui
                            .button(file_panel_label)
                            .on_hover_text("Скрывает или возвращает левую область проектов.")
                            .clicked()
                        {
                            self.set_file_panel_collapsed(!self.file_panel_collapsed);
                            ui.close_menu();
                        }
                    })
                    .response
                    .on_hover_text("Сохранённые виды рабочего места и проводник.");
                    if ui
                        .button("Команды")
                        .on_hover_text("Открыть палитру команд: Ctrl+K или Ctrl+Shift+P.")
                        .clicked()
                    {
                        self.open_command_palette();
                    }
                    ui.separator();

                    let ai_menu_label = format!(
                        "AI · {} · {}",
                        provider_name(&self.provider_input),
                        compact_inline(&self.model_input, 18)
                    );
                    ui.menu_button(RichText::new(ai_menu_label).strong(), |ui| {
                        ui.set_min_width(480.0);
                        ui.label(RichText::new("Модель и провайдер").strong());
                        ui.add_space(6.0);

                        let old_provider = self.provider_input.clone();
                        egui::Grid::new("daily_ai_settings_grid")
                            .num_columns(2)
                            .spacing([10.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Провайдер").weak().small());
                                egui::ComboBox::from_id_salt("provider_select")
                                    .selected_text(provider_name(&self.provider_input))
                                    .width(180.0)
                                    .show_ui(ui, |ui| {
                                        for provider in provider_specs()
                                            .iter()
                                            .filter(|provider| provider.implemented)
                                        {
                                            ui.selectable_value(
                                                &mut self.provider_input,
                                                provider.id.to_string(),
                                                provider.name,
                                            );
                                        }
                                    });
                                ui.end_row();

                                ui.label(RichText::new("Модель").weak().small());
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [210.0, 30.0],
                                        TextEdit::singleline(&mut self.model_input),
                                    );
                                    let model_options = models_for_provider(&self.provider_input)
                                        .collect::<Vec<_>>();
                                    if !model_options.is_empty() {
                                        egui::ComboBox::from_id_salt("model_select")
                                            .selected_text("модели")
                                            .width(92.0)
                                            .show_ui(ui, |ui| {
                                                for model in model_options {
                                                    ui.selectable_value(
                                                        &mut self.model_input,
                                                        model.id.to_string(),
                                                        model.name,
                                                    );
                                                }
                                            });
                                    }
                                });
                                ui.end_row();

                                ui.label(RichText::new("Маршрут").weak().small());
                                egui::ComboBox::from_id_salt("task_route_select")
                                    .selected_text(
                                        route_labels()
                                            .iter()
                                            .find(|(id, _)| *id == self.config.task_route)
                                            .map(|(_, label)| *label)
                                            .unwrap_or("Авто"),
                                    )
                                    .width(180.0)
                                    .show_ui(ui, |ui| {
                                        for (id, label) in route_labels() {
                                            ui.selectable_value(
                                                &mut self.config.task_route,
                                                (*id).to_string(),
                                                *label,
                                            );
                                        }
                                    });
                                ui.end_row();

                                ui.label(RichText::new("API-ключ").weak().small());
                                ui.add_sized(
                                    [310.0, 30.0],
                                    TextEdit::singleline(&mut self.api_key_input).password(true),
                                );
                                ui.end_row();
                            });

                        if self.provider_input != old_provider {
                            self.switch_provider_from_ui(self.provider_input.clone());
                        }

                        ui.add_space(6.0);
                        if ui.button("Сохранить настройки AI").clicked() {
                            self.save_settings_from_ui();
                            ui.close_menu();
                        }
                    })
                    .response
                    .on_hover_text("Провайдер, модель, маршрут задач и API-ключ.");

                    ui.separator();
                    if ui
                        .add_enabled(!self.is_running, egui::Button::new("Открыть проект"))
                        .clicked()
                    {
                        self.choose_workspace();
                    }

                    if let Some(workspace) = &self.workspace {
                        chip(
                            ui,
                            format!("Проект · {}", compact_inline(&workspace.display_name(), 22)),
                        );
                    } else {
                        ui.label(RichText::new("проект не выбран").weak().small());
                    }

                    ui.separator();
                    if self.is_running {
                        if ui.button("Стоп").clicked() {
                            self.stop_run();
                        }
                        ui.spinner();
                    } else if ui
                        .add_enabled(!self.is_running, egui::Button::new("Сброс"))
                        .on_hover_text("Очистить текущий диалог.")
                        .clicked()
                    {
                        self.reset_conversation();
                    }
                });
            });
    }

    fn show_file_panel(&mut self, ctx: &egui::Context) {
        let keyboard_shortcuts_enabled = !self.file_panel_collapsed
            && self.file_rename_target.is_none()
            && !ctx.wants_keyboard_input();
        if keyboard_shortcuts_enabled && ctx.input(|input| input.key_pressed(egui::Key::F2)) {
            if let Some(path) = self.selected_tree_path.clone() {
                self.start_tree_rename(&path);
            }
        }
        if keyboard_shortcuts_enabled && ctx.input(|input| input.key_pressed(egui::Key::Delete)) {
            if let Some(path) = self.selected_tree_path.clone() {
                self.delete_tree_path(&path);
            }
        }

        if self.file_panel_collapsed {
            self.show_collapsed_file_panel(ctx);
            return;
        }

        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(280.0)
            .width_range(180.0..=720.0)
            .frame(side_panel_frame())
            .show(ctx, |ui| {
                ui.add_space(8.0);
                let project_count = self
                    .config
                    .projects
                    .len()
                    .max(usize::from(self.workspace.is_some()));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Рабочие папки").weak().small());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("Проекты").strong().size(18.0));
                                if let Some(workspace) = &self.workspace {
                                    chip(ui, workspace.display_name());
                                }
                            });
                            ui.label(RichText::new(project_count_label(project_count)).weak().small());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("‹")
                                .on_hover_text("Свернуть левую панель проекта")
                                .clicked()
                            {
                                self.set_file_panel_collapsed(true);
                            }
                        });
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let available = safe_available_width(ui, 220.0);
                    let primary_width = (available - 82.0).max(120.0);
                    if ui
                        .add_sized([primary_width, 30.0], egui::Button::new("Новый проект"))
                        .clicked()
                    {
                        self.choose_workspace();
                    }
                    if ui
                        .add_sized([34.0, 30.0], egui::Button::new("↻"))
                        .on_hover_text("Обновить дерево проекта")
                        .clicked()
                    {
                        self.refresh_file_rows();
                        self.refresh_git_summary();
                    }
                    ui.menu_button("...", |ui| {
                        if ui.button("Открыть проект").clicked() {
                            self.choose_workspace();
                            ui.close_menu();
                        }
                        if ui.button("Обновить").clicked() {
                            self.refresh_file_rows();
                            self.refresh_git_summary();
                            ui.close_menu();
                        }
                        if ui.button("Свернуть панель").clicked() {
                            self.set_file_panel_collapsed(true);
                            ui.close_menu();
                        }
                    });
                });
                ui.add_space(8.0);
                ui.add(
                    TextEdit::singleline(&mut self.file_search_input)
                        .hint_text("Поиск")
                        .desired_width(safe_available_width(ui, 160.0)),
                );
                ui.horizontal_wrapped(|ui| {
                    for filter in FileTreeFilter::ALL {
                        let label = if filter == FileTreeFilter::Modified {
                            format!("{} {}", filter.label(), self.git_changed_files.len())
                        } else {
                            filter.label().to_string()
                        };
                        if ui
                            .selectable_label(self.file_filter == filter, RichText::new(label).small())
                            .clicked()
                        {
                            self.file_filter = filter;
                        }
                    }
                });
                ui.add_space(6.0);

                let mut select_project_path: Option<PathBuf> = None;
                let mut toggle_project_path: Option<PathBuf> = None;
                let mut rename_project_path: Option<PathBuf> = None;
                let mut toggle_pin_project_path: Option<PathBuf> = None;
                let mut remove_project_path: Option<PathBuf> = None;
                let mut open_project_folder_path: Option<PathBuf> = None;
                let mut clicked_row: Option<(String, bool, bool, f64)> = None;
                let mut toggle_dir_path: Option<String> = None;
                let mut start_rename_path: Option<String> = None;
                let mut duplicate_path: Option<String> = None;
                let mut delete_path: Option<String> = None;
                let mut move_request: Option<(String, String)> = None;
                let mut rename_action: Option<RenameRowAction> = None;
                let pointer_released = ui.input(|input| input.pointer.any_released());
                let projects = ordered_projects(&self.config.projects);
                let visible_rows = self.visible_file_rows();
                let tree_width = file_tree_content_width(&visible_rows)
                    .max(projects.iter().map(|project| project_label(project).chars().count() as f32 * 9.2 + 88.0).fold(190.0, f32::max));

                if projects.is_empty() {
                    let row = project_nav_row(
                        ui,
                        "Новый проект",
                        "new-project",
                        true,
                        false,
                        false,
                        false,
                    );
                    if row.row.clicked() {
                        self.selected_tree_path = None;
                    }
                    empty_state(
                        ui,
                        "Проект не выбран",
                        "Нажмите «Открыть» или кнопку «Проект» сверху, чтобы добавить рабочую папку.",
                    );
                } else {
                    egui::ScrollArea::both()
                        .id_salt("project_tree_scroll")
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(tree_width.max(safe_available_width(ui, 180.0)));
                            for project in projects {
                                let is_active = self.project_is_active(&project.path);
                                let project_id = path_key(&project.path);
                                let row = project_nav_row(
                                    ui,
                                    &project_label(&project),
                                    &project_id,
                                    is_active,
                                    project.expanded,
                                    self.dragged_tree_path.is_some() && is_active,
                                    project.pinned,
                                )
                                .on_hover_text(project.path.to_string_lossy().to_string());

                                let mut project_context_action: Option<&'static str> = None;
                                row.row.clone().context_menu(|ui| {
                                    ui.set_min_width(180.0);
                                    if ui.button("Переименовать").clicked() {
                                        project_context_action = Some("rename");
                                        ui.close_menu();
                                    }
                                    let pin_label = if project.pinned {
                                        "Открепить"
                                    } else {
                                        "Закрепить"
                                    };
                                    if ui.button(pin_label).clicked() {
                                        project_context_action = Some("pin");
                                        ui.close_menu();
                                    }
                                    if ui.button("Открыть папку").clicked() {
                                        project_context_action = Some("open_folder");
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button("Убрать из списка").clicked() {
                                        project_context_action = Some("remove");
                                        ui.close_menu();
                                    }
                                });
                                match project_context_action {
                                    Some("rename") => rename_project_path = Some(project.path.clone()),
                                    Some("pin") => toggle_pin_project_path = Some(project.path.clone()),
                                    Some("open_folder") => {
                                        open_project_folder_path = Some(project.path.clone())
                                    }
                                    Some("remove") => remove_project_path = Some(project.path.clone()),
                                    _ => {}
                                }

                                if row.disclosure.clicked() {
                                    toggle_project_path = Some(project.path.clone());
                                    if !is_active {
                                        select_project_path = Some(project.path.clone());
                                    }
                                } else if row.row.clicked() {
                                    if is_active {
                                        toggle_project_path = Some(project.path.clone());
                                    } else {
                                        select_project_path = Some(project.path.clone());
                                        if !project.expanded {
                                            toggle_project_path = Some(project.path.clone());
                                        }
                                    }
                                    self.selected_tree_path = None;
                                }

                                if is_active && row.row.hovered() && pointer_released {
                                    if let Some(source) = self.dragged_tree_path.take() {
                                        move_request = Some((source, String::new()));
                                    }
                                }

                                if is_active && self.active_project_is_expanded() {
                                    if self.file_rows.is_empty() {
                                        empty_state(
                                            ui,
                                            "Файлы не найдены",
                                            "Папка выбрана, но дерево проекта пустое или элементы скрыты фильтром.",
                                        );
                                    } else if visible_rows.is_empty() {
                                        empty_state(
                                            ui,
                                            "Ничего не найдено",
                                            "Измените поиск или фильтр дерева проекта.",
                                        );
                                    }

                                    for row_path in &visible_rows {
                                        let row_path = row_path.clone();
                                        let is_more = row_path == "...";
                                        let is_dir = row_path.ends_with('/');
                                        let depth = file_tree_depth(&row_path);
                                        let name = file_tree_name(&row_path);
                                        let selected = self.selected_tree_path.as_deref()
                                            == Some(row_path.as_str());
                                        let dir_expanded =
                                            is_dir && self.active_dir_is_expanded(&row_path);

                                        if self.file_rename_target.as_deref()
                                            == Some(row_path.as_str())
                                        {
                                            if let Some(action) = file_tree_rename_row(
                                                ui,
                                                depth,
                                                is_dir,
                                                &mut self.file_rename_input,
                                            ) {
                                                rename_action = Some(action);
                                            }
                                            continue;
                                        }

                                        let row_response = file_tree_nav_row(
                                            ui,
                                            name,
                                            &row_path,
                                            depth,
                                            is_dir,
                                            selected,
                                            dir_expanded,
                                        )
                                        .on_hover_text(row_path.as_str());

                                        let mut context_action: Option<&'static str> = None;
                                        row_response.row.clone().context_menu(|ui| {
                                            ui.set_min_width(180.0);
                                            if is_dir && !is_more {
                                                let label = if dir_expanded {
                                                    "Свернуть"
                                                } else {
                                                    "Развернуть"
                                                };
                                                if ui.button(label).clicked() {
                                                    context_action = Some("toggle");
                                                    ui.close_menu();
                                                }
                                            }
                                            if !is_dir
                                                && !is_more
                                                && ui.button("Открыть").clicked()
                                            {
                                                context_action = Some("open");
                                                ui.close_menu();
                                            }
                                            if !is_more && ui.button("Переименовать").clicked() {
                                                context_action = Some("rename");
                                                ui.close_menu();
                                            }
                                            if !is_more && ui.button("Копировать").clicked() {
                                                context_action = Some("copy");
                                                ui.close_menu();
                                            }
                                            if !is_more && ui.button("Удалить").clicked() {
                                                context_action = Some("delete");
                                                ui.close_menu();
                                            }
                                        });

                                        match context_action {
                                            Some("toggle") => {
                                                toggle_dir_path = Some(row_path.clone())
                                            }
                                            Some("open") => {
                                                clicked_row = Some((
                                                    row_path.clone(),
                                                    is_dir,
                                                    true,
                                                    ui.input(|input| input.time),
                                                ));
                                            }
                                            Some("rename") => {
                                                start_rename_path = Some(row_path.clone())
                                            }
                                            Some("copy") => duplicate_path = Some(row_path.clone()),
                                            Some("delete") => delete_path = Some(row_path.clone()),
                                            _ => {}
                                        }

                                        if row_response.disclosure_clicked() && is_dir {
                                            toggle_dir_path = Some(row_path.clone());
                                        } else if row_response.row.clicked()
                                            || row_response.row.double_clicked()
                                        {
                                            clicked_row = Some((
                                                row_path.clone(),
                                                is_dir,
                                                row_response.row.double_clicked(),
                                                ui.input(|input| input.time),
                                            ));
                                        }

                                        if row_response.row.drag_started() && !is_more {
                                            self.dragged_tree_path = Some(row_path.clone());
                                        }
                                        if pointer_released
                                            && row_response.row.hovered()
                                            && is_dir
                                        {
                                            if let Some(source) = self.dragged_tree_path.take() {
                                                move_request = Some((source, row_path.clone()));
                                            }
                                        }
                                    }
                                }
                            }
                        });
                }

                if pointer_released && move_request.is_none() {
                    self.dragged_tree_path = None;
                }

                if let Some(path) = select_project_path {
                    let _ = self.open_workspace_path(path);
                }
                if let Some(path) = toggle_project_path {
                    self.toggle_project_expanded(&path);
                }
                if let Some(path) = rename_project_path {
                    self.start_project_rename(&path);
                }
                if let Some(path) = toggle_pin_project_path {
                    self.toggle_project_pinned(&path);
                }
                if let Some(path) = open_project_folder_path {
                    self.open_project_folder(&path);
                }
                if let Some(path) = remove_project_path {
                    self.remove_project_from_list(&path);
                }
                if let Some(path) = toggle_dir_path {
                    self.toggle_active_dir_expanded(&path);
                }
                if let Some((path, is_dir, double_clicked, time)) = clicked_row {
                    self.handle_file_tree_click(&path, is_dir, double_clicked, time);
                }
                if let Some(path) = start_rename_path {
                    self.start_tree_rename(&path);
                }
                if let Some(action) = rename_action {
                    match action {
                        RenameRowAction::Commit => self.commit_tree_rename(),
                        RenameRowAction::Cancel => self.cancel_tree_rename(),
                    }
                }
                if let Some(path) = duplicate_path {
                    self.duplicate_tree_path(&path);
                }
                if let Some(path) = delete_path {
                    self.delete_tree_path(&path);
                }
                if let Some((source, target_dir)) = move_request {
                    self.move_tree_path(&source, &target_dir);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                if let Some(path) = self.selected_tree_path.clone() {
                    ui.label(RichText::new("Выбрано").weak().small());
                    ui.label(RichText::new(&path).text_style(egui::TextStyle::Monospace));
                    ui.horizontal_wrapped(|ui| {
                        let is_dir = path.ends_with('/');
                        if is_dir {
                            let label = if self.active_dir_is_expanded(&path) {
                                "Свернуть"
                            } else {
                                "Развернуть"
                            };
                            if ui.button(label).clicked() {
                                self.toggle_active_dir_expanded(&path);
                            }
                        }
                        if ui.button("Переименовать").clicked() {
                            self.start_tree_rename(&path);
                        }
                        if ui.button("Копировать").clicked() {
                            self.duplicate_tree_path(&path);
                        }
                        if ui.button("Удалить").clicked() {
                            self.delete_tree_path(&path);
                        }
                        if !is_dir && ui.button("Открыть").clicked() {
                            self.load_file_preview(&path);
                        }
                    });
                } else {
                    ui.label(RichText::new("Проект").weak().small());
                    if let Some(project) = self.active_project_state().cloned() {
                        let path = project.path.clone();
                        ui.label(RichText::new(project_label(&project)).strong());
                        ui.label(
                            RichText::new(path.to_string_lossy().to_string())
                                .weak()
                                .small()
                                .text_style(egui::TextStyle::Monospace),
                        );

                        if self
                            .project_rename_target
                            .as_deref()
                            .is_some_and(|target| project_paths_match(target, &path))
                        {
                            let response = ui.add(
                                TextEdit::singleline(&mut self.project_rename_input)
                                    .hint_text("Имя проекта в панели")
                                    .desired_width(safe_available_width(ui, 120.0)),
                            );
                            let commit = response.has_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter));
                            let cancel = response.has_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Escape));
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Сохранить").clicked() || commit {
                                    self.commit_project_rename();
                                }
                                if ui.button("Отмена").clicked() || cancel {
                                    self.cancel_project_rename();
                                }
                            });
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Переименовать").clicked() {
                                    self.start_project_rename(&path);
                                }
                                let pin_label = if project.pinned {
                                    "Открепить"
                                } else {
                                    "Закрепить"
                                };
                                if ui.button(pin_label).clicked() {
                                    self.toggle_project_pinned(&path);
                                }
                                if ui.button("Открыть папку").clicked() {
                                    self.open_project_folder(&path);
                                }
                                if ui.button("Убрать из списка").clicked() {
                                    self.remove_project_from_list(&path);
                                }
                            });
                        }
                    } else {
                        ui.label(
                            RichText::new("Раскройте проект кнопкой слева от названия. Каталоги запомнят своё состояние.")
                                .weak()
                                .small(),
                        );
                    }
                }

                if let Some(source) = &self.dragged_tree_path {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Перетаскивание: {source}"))
                            .weak()
                            .small(),
                    );
                }
                if !self.file_operation_status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.file_operation_status).weak().small());
                }
            });
    }

    fn show_collapsed_file_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("files_collapsed")
            .resizable(false)
            .default_width(52.0)
            .width_range(52.0..=52.0)
            .frame(side_panel_frame())
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui
                        .add_sized([34.0, 30.0], egui::Button::new("›"))
                        .on_hover_text("Развернуть проекты и файловое дерево")
                        .clicked()
                    {
                        self.set_file_panel_collapsed(false);
                    }

                    ui.add_space(6.0);
                    if ui
                        .add_sized([34.0, 30.0], egui::Button::new("+"))
                        .on_hover_text("Открыть или добавить проект")
                        .clicked()
                    {
                        self.choose_workspace();
                    }

                    if ui
                        .add_sized([34.0, 30.0], egui::Button::new("↻"))
                        .on_hover_text("Обновить дерево проекта")
                        .clicked()
                    {
                        self.refresh_file_rows();
                        self.refresh_git_summary();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                ui.vertical_centered(|ui| {
                    if let Some(workspace) = &self.workspace {
                        let display_name = workspace.display_name();
                        let name = compact_inline(&display_name, 4);
                        ui.label(RichText::new(name).strong())
                            .on_hover_text(display_name);
                    } else {
                        ui.label(RichText::new("нет").weak().small());
                    }
                });
            });
    }

    #[allow(dead_code)]
    fn show_file_panel_flat(&mut self, ctx: &egui::Context) {
        let keyboard_shortcuts_enabled =
            self.file_rename_target.is_none() && !ctx.wants_keyboard_input();
        if keyboard_shortcuts_enabled && ctx.input(|input| input.key_pressed(egui::Key::F2)) {
            if let Some(path) = self.selected_tree_path.clone() {
                self.start_tree_rename(&path);
            }
        }
        if keyboard_shortcuts_enabled && ctx.input(|input| input.key_pressed(egui::Key::Delete)) {
            if let Some(path) = self.selected_tree_path.clone() {
                self.delete_tree_path(&path);
            }
        }

        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(300.0)
            .width_range(220.0..=560.0)
            .frame(side_panel_frame())
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Проект").strong().size(18.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Обновить").clicked() {
                            self.refresh_file_rows();
                        }
                        if ui.button("Открыть").clicked() {
                            self.choose_workspace();
                        }
                    });
                });

                let title = self.project_tree_title();
                let file_count = self.file_rows.len();
                ui.label(
                    RichText::new(format!("{title} | {file_count} элементов"))
                        .weak()
                        .small(),
                );
                ui.add_space(6.0);

                let mut clicked_row: Option<(String, bool, bool, f64)> = None;
                let mut start_rename_path: Option<String> = None;
                let mut duplicate_path: Option<String> = None;
                let mut delete_path: Option<String> = None;
                let mut move_request: Option<(String, String)> = None;
                let mut rename_action: Option<RenameRowAction> = None;
                let pointer_released = ui.input(|input| input.pointer.any_released());

                if self.workspace.is_none() {
                    let response = project_root_row(ui, "Новый проект", true, false);
                    if response.clicked() {
                        self.selected_tree_path = None;
                    }
                    empty_state(
                        ui,
                        "Проект не выбран",
                        "Нажмите «Открыть» или кнопку «Проект» сверху, чтобы выбрать рабочую папку.",
                    );
                } else {
                    let root_selected = self.selected_tree_path.is_none();
                    let root_response =
                        project_root_row(ui, &title, root_selected, self.dragged_tree_path.is_some())
                            .on_hover_text("Корень выбранного проекта");
                    if root_response.clicked() {
                        self.selected_tree_path = None;
                    }
                    if root_response.hovered() && pointer_released {
                        if let Some(source) = self.dragged_tree_path.take() {
                            move_request = Some((source, String::new()));
                        }
                    }

                    if self.file_rows.is_empty() {
                        empty_state(
                            ui,
                            "Файлы не найдены",
                            "Папка выбрана, но дерево проекта пустое или элементы скрыты фильтром.",
                        );
                    } else {
                        let tree_width = file_tree_content_width(&self.file_rows)
                            .max(title.chars().count() as f32 * 9.2 + 58.0);
                        egui::ScrollArea::both()
                            .id_salt("file_tree_scroll")
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_min_width(tree_width.max(safe_available_width(ui, 160.0)));
                                for idx in 0..self.file_rows.len() {
                                    let row = self.file_rows[idx].clone();
                                    let is_more = row == "...";
                                    let is_dir = row.ends_with('/');
                                    let depth = file_tree_depth(&row);
                                    let name = file_tree_name(&row);
                                    let selected =
                                        self.selected_tree_path.as_deref() == Some(row.as_str());

                                    if self.file_rename_target.as_deref() == Some(row.as_str()) {
                                        if let Some(action) = file_tree_rename_row(
                                            ui,
                                            depth,
                                            is_dir,
                                            &mut self.file_rename_input,
                                        ) {
                                            rename_action = Some(action);
                                        }
                                        continue;
                                    }

                                    let response =
                                        file_tree_row(ui, name, depth, is_dir, selected)
                                            .on_hover_text(row.as_str());

                                    let mut context_action: Option<&'static str> = None;
                                    response.clone().context_menu(|ui| {
                                        ui.set_min_width(170.0);
                                        if !is_dir && !is_more && ui.button("Открыть").clicked() {
                                            context_action = Some("open");
                                            ui.close_menu();
                                        }
                                        if !is_more && ui.button("Переименовать").clicked() {
                                            context_action = Some("rename");
                                            ui.close_menu();
                                        }
                                        if !is_more && ui.button("Копировать").clicked() {
                                            context_action = Some("copy");
                                            ui.close_menu();
                                        }
                                        if !is_more && ui.button("Удалить").clicked() {
                                            context_action = Some("delete");
                                            ui.close_menu();
                                        }
                                    });
                                    match context_action {
                                        Some("open") => {
                                            clicked_row = Some((
                                                row.clone(),
                                                is_dir,
                                                true,
                                                ui.input(|input| input.time),
                                            ));
                                        }
                                        Some("rename") => start_rename_path = Some(row.clone()),
                                        Some("copy") => duplicate_path = Some(row.clone()),
                                        Some("delete") => delete_path = Some(row.clone()),
                                        _ => {}
                                    }

                                    if response.clicked() || response.double_clicked() {
                                        clicked_row = Some((
                                            row.clone(),
                                            is_dir,
                                            response.double_clicked(),
                                            ui.input(|input| input.time),
                                        ));
                                    }

                                    if response.drag_started() && !is_more {
                                        self.dragged_tree_path = Some(row.clone());
                                    }
                                    if pointer_released && response.hovered() && is_dir {
                                        if let Some(source) = self.dragged_tree_path.take() {
                                            move_request = Some((source, row.clone()));
                                        }
                                    }
                                }
                            });
                    }
                }

                if pointer_released && move_request.is_none() {
                    self.dragged_tree_path = None;
                }

                if let Some((path, is_dir, double_clicked, time)) = clicked_row {
                    self.handle_file_tree_click(&path, is_dir, double_clicked, time);
                }
                if let Some(path) = start_rename_path {
                    self.start_tree_rename(&path);
                }
                if let Some(action) = rename_action {
                    match action {
                        RenameRowAction::Commit => self.commit_tree_rename(),
                        RenameRowAction::Cancel => self.cancel_tree_rename(),
                    }
                }
                if let Some(path) = duplicate_path {
                    self.duplicate_tree_path(&path);
                }
                if let Some(path) = delete_path {
                    self.delete_tree_path(&path);
                }
                if let Some((source, target_dir)) = move_request {
                    self.move_tree_path(&source, &target_dir);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                if let Some(path) = self.selected_tree_path.clone() {
                    ui.label(RichText::new("Выбрано").weak().small());
                    ui.label(RichText::new(&path).text_style(egui::TextStyle::Monospace));
                    ui.horizontal_wrapped(|ui| {
                        let is_dir = path.ends_with('/');
                        if ui.button("Переименовать").clicked() {
                            self.start_tree_rename(&path);
                        }
                        if ui.button("Копировать").clicked() {
                            self.duplicate_tree_path(&path);
                        }
                        if ui.button("Удалить").clicked() {
                            self.delete_tree_path(&path);
                        }
                        if !is_dir && ui.button("Открыть").clicked() {
                            self.load_file_preview(&path);
                        }
                    });
                } else {
                    ui.label(RichText::new("Корень проекта").weak().small());
                    ui.label(
                        RichText::new("Перетащите файл или папку сюда, чтобы переместить в корень.")
                            .weak()
                            .small(),
                    );
                }

                if let Some(source) = &self.dragged_tree_path {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Перетаскивание: {source}"))
                            .weak()
                            .small(),
                    );
                }
                if !self.file_operation_status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.file_operation_status).weak().small());
                }
            });
    }

    #[allow(dead_code, unreachable_code)]
    fn show_file_panel_legacy(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(250.0)
            .width_range(180.0..=300.0)
            .frame(side_panel_frame())
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Файлы").strong().size(18.0));
                    if ui.button("Обновить").clicked() {
                        self.refresh_file_rows();
                    }
                });
                if let Some(workspace) = &self.workspace {
                    ui.label(
                        RichText::new(format!(
                            "{} | {} элементов",
                            workspace.display_name(),
                            self.file_rows.len()
                        ))
                        .weak()
                        .small(),
                    );
                } else {
                    ui.label(RichText::new("проект не выбран").weak().small());
                }
                ui.add_space(6.0);

                if self.workspace.is_none() {
                    empty_state(
                        ui,
                        "Проект не выбран",
                        "Нажмите «Проект» сверху и выберите папку.",
                    );
                } else if self.file_rows.is_empty() {
                    empty_state(
                        ui,
                        "Файлы не найдены",
                        "Папка выбрана, но список файлов пуст. Попробуйте «Обновить».",
                    );
                } else {
                    let tree_width = file_tree_content_width(&self.file_rows);
                    egui::ScrollArea::both()
                        .id_salt("file_tree_scroll")
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(tree_width.max(safe_available_width(ui, 160.0)));
                            for idx in 0..self.file_rows.len() {
                                let row = self.file_rows[idx].clone();
                                let selected = self.selected_file.as_deref() == Some(row.as_str());
                                let is_dir = row.ends_with('/');
                                let is_more = row == "...";
                                let depth = file_tree_depth(&row);
                                let name = file_tree_name(&row);
                                let response = file_tree_row(ui, name, depth, is_dir, selected)
                                    .on_hover_text(row.as_str());
                                if response.clicked() && !is_dir && !is_more {
                                    self.load_file_preview(&row);
                                }
                            }
                        });
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                if let Some(file) = &self.selected_file {
                    ui.label(RichText::new("Открыто").weak().small());
                    ui.label(RichText::new(file).text_style(egui::TextStyle::Monospace));
                } else {
                    ui.label(RichText::new("Выберите файл").weak());
                    ui.label(
                        RichText::new("Файл откроется вкладкой в центральной области.")
                            .weak()
                            .small(),
                    );
                }
                return;

                ui.add_space(10.0);
                if let Some(file) = &self.selected_file {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(file).strong());
                        if self.editor_dirty() {
                            ui.label(RichText::new("изменён").italics());
                        } else if !self.editor_status.is_empty() {
                            ui.label(RichText::new(&self.editor_status).weak());
                        }
                    });
                } else {
                    ui.label(RichText::new("Файл не выбран").weak());
                }

                ui.horizontal(|ui| {
                    let dirty = self.editor_dirty();
                    if ui
                        .add_enabled(
                            self.selected_file_editable && dirty,
                            egui::Button::new("Сохранить"),
                        )
                        .clicked()
                    {
                        self.save_selected_file();
                    }
                    if ui
                        .add_enabled(
                            self.selected_file_editable && dirty,
                            egui::Button::new("Отменить"),
                        )
                        .clicked()
                    {
                        self.revert_selected_file();
                    }
                    if ui
                        .add_enabled(
                            self.selected_file.is_some(),
                            egui::Button::new("Перезагрузить"),
                        )
                        .clicked()
                    {
                        self.reload_selected_file();
                    }
                });

                ui.add_space(6.0);
                if self.selected_file.is_none() {
                    ui.label(RichText::new("Выберите файл").weak());
                    ui.label(
                        RichText::new("Предпросмотр и быстрые правки появятся здесь.")
                            .weak()
                            .small(),
                    );
                    return;
                }

                card_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("file_preview_scroll")
                        .show(ui, |ui| {
                            ui.add(
                                TextEdit::multiline(&mut self.selected_preview)
                                    .desired_width(safe_available_width(ui, 160.0))
                                    .horizontal_align(egui::Align::Min)
                                    .font(egui::TextStyle::Monospace)
                                    .interactive(self.selected_file_editable),
                            );
                        });
                });
            });
    }

    fn show_journal_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Журнал", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Обновить").clicked() {
                    self.refresh_journal();
                }
                if ui.button("Очистить").clicked() {
                    self.clear_journal_from_ui();
                }
            });

            if !self.journal_status.is_empty() {
                ui.label(RichText::new(&self.journal_status).weak());
            }

            let mut journal_text = if self.journal_lines.is_empty() {
                "В журнале пока нет записей".to_string()
            } else {
                self.journal_lines.join("\n")
            };
            ui.add(
                TextEdit::multiline(&mut journal_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(safe_available_width(ui, 160.0))
                    .horizontal_align(egui::Align::Min)
                    .desired_rows(10)
                    .interactive(false),
            );

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("История запусков агента").strong());
                if ui.button("Обновить историю").clicked() {
                    self.refresh_agent_history();
                }
            });
            if !self.agent_history_status.is_empty() {
                ui.label(RichText::new(&self.agent_history_status).weak().small());
            }
            if self.agent_history.is_empty() {
                empty_state(
                    ui,
                    "История запусков пуста",
                    "После завершения задачи агент сохранит структурированную запись JSONL в проект.",
                );
            } else {
                for record in self.agent_history.iter().rev().take(8) {
                    let title = format!(
                        "{} · {} · {}",
                        agent_history_status_label(&record.status),
                        format_history_duration_ms(record.duration_ms),
                        record.model
                    );
                    let detail = format!(
                        "{}\nинструменты: {} · approvals: {} · файлы: {}\n{}",
                        compact_inline(&record.user_request, 180),
                        record.tool_calls.len(),
                        record.approvals.len(),
                        record.changed_files.len(),
                        record.final_response.as_deref().unwrap_or("итоговый ответ не записан")
                    );
                    inline_log_entry(ui, &title, &compact(&detail, 1_200));
                }
            }
        });
    }

    fn show_agent_history_explorer(&mut self, ui: &mut egui::Ui) {
        panel_header(
            ui,
            "История агента",
            "Поиск по запускам, инструментам, файлам, моделям, ошибкам и итогам.",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Обновить").clicked() {
                self.refresh_agent_history();
            }
            if ui
                .add_enabled(!self.agent_history.is_empty(), egui::Button::new("Экспорт"))
                .clicked()
            {
                self.export_agent_history_markdown();
            }
        });
        if !self.agent_history_status.trim().is_empty() {
            full_width_wrapped_label(ui, RichText::new(&self.agent_history_status).weak().small());
        }
        ui.add_space(4.0);
        ui.add(
            TextEdit::singleline(&mut self.agent_history_query)
                .desired_width(safe_available_width(ui, 120.0))
                .hint_text("поиск: модель, провайдер, tool, файл, дата, текст"),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Статус").weak().small());
            for filter in AgentHistoryStatusFilter::ALL {
                if ui
                    .selectable_label(self.agent_history_status_filter == filter, filter.label())
                    .clicked()
                {
                    self.agent_history_status_filter = filter;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Длительность").weak().small());
            for filter in AgentHistoryDurationFilter::ALL {
                if ui
                    .selectable_label(self.agent_history_duration_filter == filter, filter.label())
                    .clicked()
                {
                    self.agent_history_duration_filter = filter;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Период").weak().small());
            for filter in AgentHistoryDateFilter::ALL {
                if ui
                    .selectable_label(self.agent_history_date_filter == filter, filter.label())
                    .clicked()
                {
                    self.agent_history_date_filter = filter;
                }
            }
        });

        let filtered = self.filtered_agent_history();
        let total_duration = filtered
            .iter()
            .map(|record| record.duration_ms)
            .sum::<u64>();
        let failures = filtered
            .iter()
            .filter(|record| record.status == "failed")
            .count();
        let tool_calls = filtered
            .iter()
            .map(|record| record.tool_calls.len())
            .sum::<usize>();
        let succeeded = filtered
            .iter()
            .filter(|record| record.status == "succeeded")
            .count();
        let average_duration = if filtered.is_empty() {
            0
        } else {
            total_duration / filtered.len() as u64
        };
        ui.add_space(6.0);
        ui.columns(3, |columns| {
            roadmap_metric(&mut columns[0], filtered.len(), "запусков");
            roadmap_metric(
                &mut columns[1],
                format_history_duration_ms(total_duration),
                "суммарно",
            );
            roadmap_metric(&mut columns[2], failures, "ошибок");
        });
        ui.columns(3, |columns| {
            roadmap_metric(&mut columns[0], succeeded, "успешно");
            roadmap_metric(
                &mut columns[1],
                format_history_duration_ms(average_duration),
                "среднее",
            );
            roadmap_metric(&mut columns[2], tool_calls, "tools");
        });

        if filtered.is_empty() {
            empty_state(
                ui,
                "История не найдена",
                "Измените фильтры или выполните задачу агентом, чтобы появилась структурированная запись.",
            );
            return;
        }

        egui::CollapsingHeader::new("Надёжность провайдеров")
            .default_open(false)
            .show(ui, |ui| {
                let mut provider_stats: BTreeMap<String, (usize, usize, u64)> = BTreeMap::new();
                for record in &filtered {
                    let key = format!("{} / {}", record.provider, record.model);
                    let entry = provider_stats.entry(key).or_default();
                    entry.0 += 1;
                    if record.status == "succeeded" {
                        entry.1 += 1;
                    }
                    entry.2 += record.duration_ms;
                }
                for (provider, (total, ok, duration)) in provider_stats.iter().take(8) {
                    let ratio = if *total == 0 {
                        0.0
                    } else {
                        *ok as f32 / *total as f32
                    };
                    let avg = if *total == 0 {
                        0
                    } else {
                        duration / *total as u64
                    };
                    ui.label(RichText::new(provider).small().strong());
                    ui.add(
                        egui::ProgressBar::new(ratio)
                            .desired_width(safe_available_width(ui, 80.0))
                            .text(format!(
                                "{} из {} · среднее {}",
                                ok,
                                total,
                                format_history_duration_ms(avg)
                            )),
                    );
                }
            });

        egui::CollapsingHeader::new("Использование инструментов")
            .default_open(false)
            .show(ui, |ui| {
                let mut tool_stats: BTreeMap<String, usize> = BTreeMap::new();
                for record in &filtered {
                    for tool in &record.tool_calls {
                        *tool_stats.entry(tool.name.clone()).or_default() += 1;
                    }
                }
                let max = tool_stats.values().copied().max().unwrap_or(1) as f32;
                for (tool, count) in tool_stats.iter().take(12) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(compact_inline(tool, 24)).small());
                        ui.add(
                            egui::ProgressBar::new(*count as f32 / max)
                                .desired_width(safe_available_width(ui, 80.0).min(180.0))
                                .text(count.to_string()),
                        );
                    });
                }
            });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        let mut select_run_id: Option<String> = None;
        for record in filtered.iter().rev().take(20) {
            let selected = self
                .selected_agent_history_id
                .as_deref()
                .map(|id| id == record.id)
                .unwrap_or(false);
            let title = format!(
                "{} · {} · {} / {}",
                agent_history_status_label(&record.status),
                format_history_duration_ms(record.duration_ms),
                compact_inline(&record.provider, 20),
                compact_inline(&record.model, 24)
            );
            let detail = format!(
                "{}\n{} · tools {} · files {} · {}",
                compact_inline(&record.user_request, 160),
                agent_history_date_label(record.started_at),
                record.tool_calls.len(),
                record.changed_files.len(),
                record.id
            );
            let response = ui.selectable_label(selected, title);
            if response.clicked() {
                select_run_id = Some(record.id.clone());
            }
            full_width_wrapped_label(ui, RichText::new(detail).weak().small());
            ui.add_space(4.0);
        }
        if let Some(id) = select_run_id {
            self.selected_agent_history_id = Some(id);
        }

        let selected = self.selected_agent_history();
        if let Some(record) = selected {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            self.show_agent_history_record(ui, &record);
        }
    }

    fn show_agent_history_record(&mut self, ui: &mut egui::Ui, record: &AgentRunHistoryRecord) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Выбранный запуск").strong());
            chip(ui, agent_history_status_label(&record.status));
            chip(ui, &format_history_duration_ms(record.duration_ms));
        });
        full_width_wrapped_label(
            ui,
            RichText::new(format!(
                "{} · {} · {}",
                record.id,
                agent_history_date_label(record.started_at),
                record.route
            ))
            .weak()
            .small(),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("В eval").clicked() {
                self.create_eval_from_history_record(record);
            }
            if ui.button("В память").clicked() {
                self.save_history_record_to_memory(record);
            }
            if ui.button("В roadmap").clicked() {
                self.attach_history_record_to_roadmap(record);
            }
            if ui.button("Экспорт run").clicked() {
                self.export_agent_history_record_markdown(record);
            }
        });

        egui::CollapsingHeader::new("Запрос и план")
            .default_open(true)
            .show(ui, |ui| {
                full_width_wrapped_label(ui, record.user_request.as_str());
                if let Some(plan) = &record.confirmed_plan {
                    ui.add_space(4.0);
                    full_width_wrapped_label(
                        ui,
                        RichText::new(format!("{}: {}", plan.summary, plan.detail)).weak(),
                    );
                }
            });
        egui::CollapsingHeader::new("Инструменты и согласования")
            .default_open(false)
            .show(ui, |ui| {
                if record.tool_calls.is_empty() {
                    ui.label(RichText::new("Инструментов не было").weak());
                } else {
                    for tool in record.tool_calls.iter().take(40) {
                        let title = format!(
                            "{} · {}{}",
                            tool.name,
                            tool.status,
                            tool.duration_ms
                                .map(format_history_duration_ms)
                                .map(|value| format!(" · {value}"))
                                .unwrap_or_default()
                        );
                        inline_log_entry(ui, &title, &compact(&tool.summary, 700));
                    }
                }
                if !record.approvals.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Согласования").strong().small());
                    for approval in &record.approvals {
                        inline_log_entry(ui, &approval.summary, &compact(&approval.detail, 500));
                    }
                }
            });
        egui::CollapsingHeader::new("Файлы, ошибки и итог")
            .default_open(true)
            .show(ui, |ui| {
                if !record.changed_files.is_empty() {
                    ui.label(RichText::new("Изменённые файлы").strong().small());
                    for file in record.changed_files.iter().take(40) {
                        full_width_wrapped_label(ui, RichText::new(file).monospace().small());
                    }
                }
                if !record.errors.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Ошибки").strong().small());
                    for error in &record.errors {
                        full_width_wrapped_label(ui, RichText::new(error).weak().small());
                    }
                }
                if let Some(report) = &record.final_report {
                    ui.separator();
                    ui.label(RichText::new("Итоговый отчёт").strong().small());
                    full_width_wrapped_label(ui, report.as_str());
                } else if let Some(response) = &record.final_response {
                    ui.separator();
                    ui.label(RichText::new("Ответ агента").strong().small());
                    full_width_wrapped_label(ui, response.as_str());
                }
            });
    }

    fn filtered_agent_history(&self) -> Vec<AgentRunHistoryRecord> {
        let query = self.agent_history_query.trim().to_ascii_lowercase();
        self.agent_history
            .iter()
            .filter(|record| self.agent_history_status_filter.matches(&record.status))
            .filter(|record| {
                self.agent_history_duration_filter
                    .matches(record.duration_ms)
            })
            .filter(|record| self.agent_history_date_filter.matches(record.started_at))
            .filter(|record| {
                if query.is_empty() {
                    return true;
                }
                agent_history_search_blob(record).contains(&query)
            })
            .cloned()
            .collect()
    }

    fn selected_agent_history(&self) -> Option<AgentRunHistoryRecord> {
        let selected_id = self.selected_agent_history_id.as_deref()?;
        self.agent_history
            .iter()
            .find(|record| record.id == selected_id)
            .cloned()
    }

    fn create_eval_from_history_record(&mut self, record: &AgentRunHistoryRecord) {
        let Some(workspace) = self.workspace.clone() else {
            self.agent_history_status = "выберите проект, чтобы создать eval".to_string();
            return;
        };
        let tools = record
            .tool_calls
            .iter()
            .map(|tool| tool.name.clone())
            .filter(|name| !name.trim().is_empty())
            .take(12)
            .collect::<Vec<_>>();
        let criteria = vec![
            "Агент должен сохранить смысл исходного запроса.".to_string(),
            "Агент должен завершить запуск без критической ошибки.".to_string(),
        ];
        match create_replay_eval(
            &workspace,
            format!("Replay {}", compact_inline(&record.user_request, 48)),
            record.user_request.clone(),
            tools,
            criteria,
        ) {
            Ok(eval) => {
                self.orchestration_status = format!("создан eval: {}", eval.id);
                self.agent_history_status = self.orchestration_status.clone();
            }
            Err(err) => {
                self.agent_history_status = format!("не удалось создать eval: {err}");
            }
        }
    }

    fn save_history_record_to_memory(&mut self, record: &AgentRunHistoryRecord) {
        let Some(workspace) = self.workspace.clone() else {
            self.agent_history_status = "выберите проект, чтобы сохранить run в память".to_string();
            return;
        };
        let content = agent_history_record_markdown(record);
        let result = record_memory_source(
            &workspace,
            RecordMemorySourceArgs {
                id: Some(format!("agent-history-{}", record.id)),
                title: format!("Agent run {}", agent_history_date_label(record.started_at)),
                kind: Some("agent_run".to_string()),
                summary: Some(compact_inline(&record.user_request, 400)),
                content: Some(content),
                path: None,
            },
        );
        self.agent_history_status = if result.ok {
            "запуск сохранён в память проекта".to_string()
        } else {
            result.output
        };
    }

    fn attach_history_record_to_roadmap(&mut self, record: &AgentRunHistoryRecord) {
        let Some(workspace) = self.workspace.clone() else {
            self.agent_history_status =
                "выберите проект, чтобы прикрепить run к roadmap".to_string();
            return;
        };
        let validation = if record.errors.is_empty() {
            Some("agent run succeeded".to_string())
        } else {
            Some(format!("{} ошибок", record.errors.len()))
        };
        let result = record_milestone(
            &workspace,
            RecordMilestoneArgs {
                title: format!("Agent run · {}", compact_inline(&record.user_request, 64)),
                detail: record
                    .final_report
                    .clone()
                    .or_else(|| record.final_response.clone())
                    .unwrap_or_else(|| "Запуск агента сохранён в истории.".to_string()),
                item_id: Some(format!("run-{}", record.id)),
                status: Some(if record.status == "succeeded" {
                    "done".to_string()
                } else {
                    "now".to_string()
                }),
                commits: Vec::new(),
                changed_files: record.changed_files.clone(),
                agent_run_id: Some(record.id.clone()),
                validation,
                memory_ids: Vec::new(),
            },
        );
        self.agent_history_status = if result.ok {
            "запуск прикреплён к roadmap".to_string()
        } else {
            result.output
        };
    }

    fn export_agent_history_record_markdown(&mut self, record: &AgentRunHistoryRecord) {
        let Some(workspace) = self.workspace.clone() else {
            self.agent_history_status = "выберите проект, чтобы экспортировать run".to_string();
            return;
        };
        let path = format!(
            "assets/generated/leetcode/agent_history_exports/{}.md",
            record.id
        );
        match workspace.write_text(&path, &agent_history_record_markdown(record)) {
            Ok(()) => {
                self.agent_history_status = format!("run экспортирован: {path}");
            }
            Err(err) => {
                self.agent_history_status = format!("не удалось экспортировать run: {err}");
            }
        }
    }

    fn export_agent_history_markdown(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            self.agent_history_status = "выберите проект, чтобы экспортировать историю".to_string();
            return;
        };
        let filtered = self.filtered_agent_history();
        let mut markdown = String::from("# Agent History\n\n");
        markdown.push_str(&format!("Экспортировано запусков: {}\n\n", filtered.len()));
        for record in filtered.iter().rev() {
            markdown.push_str(&agent_history_record_markdown(record));
            markdown.push_str("\n\n---\n\n");
        }
        let path = "assets/generated/leetcode/agent_history_exports/history.md";
        match workspace.write_text(path, &markdown) {
            Ok(()) => {
                self.agent_history_status = format!("история экспортирована: {path}");
            }
            Err(err) => {
                self.agent_history_status = format!("не удалось экспортировать историю: {err}");
            }
        }
    }

    fn show_governance_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Управление доступом", |ui| {
            let Some(workspace) = self.workspace.clone() else {
                empty_state(
                    ui,
                    "Нужна рабочая папка",
                    "Выберите проект, чтобы настраивать разрешения инструментов.",
                );
                return;
            };
            let mut config = load_governance(&workspace);
            panel_header(
                ui,
                "Политика действий",
                "Правила проекта для инструментов агента, shell-доступа и рискованных операций.",
            );
            ui.horizontal_wrapped(|ui| {
                metric_chip(
                    ui,
                    "заблокировано инструментов",
                    config.disabled_tools.len(),
                );
                metric_chip(
                    ui,
                    "заблокировано категорий",
                    config.disabled_categories.len(),
                );
                metric_chip(ui, "shell-запретов", config.shell_deny_patterns.len());
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.add_sized(
                    [(safe_available_width(ui, 180.0) - 54.0).max(120.0), 22.0],
                    TextEdit::singleline(&mut self.governance_pattern_input)
                        .hint_text("запретить shell-фрагмент"),
                );
                if ui.button("Добавить").clicked() {
                    let pattern = self.governance_pattern_input.trim().to_string();
                    if !pattern.is_empty()
                        && !config
                            .shell_deny_patterns
                            .iter()
                            .any(|known| known.eq_ignore_ascii_case(&pattern))
                    {
                        config.shell_deny_patterns.push(pattern);
                        match save_governance(&workspace, &config) {
                            Ok(()) => {
                                self.governance_status = "shell-запрет сохранён".to_string();
                                self.governance_pattern_input.clear();
                            }
                            Err(err) => {
                                self.governance_status = format!("не удалось сохранить: {err}")
                            }
                        }
                    }
                }
            });

            if config.shell_deny_patterns.is_empty() {
                empty_state(
                    ui,
                    "Shell-запретов пока нет",
                    "Добавьте точные фрагменты команд, которые агенту нельзя запускать.",
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    for pattern in config.shell_deny_patterns.iter().take(8) {
                        chip(ui, pattern);
                    }
                });
            }
            ui.separator();

            let mut categories = tool_specs()
                .iter()
                .map(|spec| spec.category)
                .collect::<Vec<_>>();
            categories.sort_unstable();
            categories.dedup();
            ui.label(RichText::new("Категории").strong());
            ui.horizontal_wrapped(|ui| {
                for category in categories {
                    let mut enabled = !config
                        .disabled_categories
                        .iter()
                        .any(|known| known == category);
                    if ui
                        .checkbox(&mut enabled, category_label(category))
                        .changed()
                    {
                        if enabled {
                            config.disabled_categories.retain(|known| known != category);
                        } else if !config
                            .disabled_categories
                            .iter()
                            .any(|known| known == category)
                        {
                            config.disabled_categories.push(category.to_string());
                        }
                        if let Err(err) = save_governance(&workspace, &config) {
                            self.governance_status = format!("не удалось сохранить: {err}");
                        }
                    }
                }
            });
            ui.add_space(2.0);

            ui.label(RichText::new("Инструменты").strong());
            egui::ScrollArea::vertical()
                .max_height(210.0)
                .show(ui, |ui| {
                    for spec in tool_specs() {
                        let mut enabled =
                            !config.disabled_tools.iter().any(|known| known == spec.id);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .checkbox(&mut enabled, "")
                                .on_hover_text(spec.description)
                                .changed()
                            {
                                if enabled {
                                    config.disabled_tools.retain(|known| known != spec.id);
                                } else if !config
                                    .disabled_tools
                                    .iter()
                                    .any(|known| known == spec.id)
                                {
                                    config.disabled_tools.push(spec.id.to_string());
                                }
                                if let Err(err) = save_governance(&workspace, &config) {
                                    self.governance_status = format!("не удалось сохранить: {err}");
                                }
                            }
                            ui.label(RichText::new(spec.id).text_style(egui::TextStyle::Monospace));
                            chip(ui, category_label(spec.category));
                            chip(ui, risk_label(spec.risk));
                            ui.label(RichText::new(spec.description).weak());
                        });
                    }
                });

            if !self.governance_status.is_empty() {
                ui.label(RichText::new(&self.governance_status).weak());
            }
        });
    }

    fn show_memory_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Память проекта", |ui| {
            let Some(workspace) = self.workspace.clone() else {
                empty_state(
                    ui,
                    "Нужна рабочая папка",
                    "Выберите проект, чтобы сохранять цели, решения и текущие задачи.",
                );
                return;
            };
            let memory = load_memory(&workspace);
            let open_tasks = memory
                .tasks
                .iter()
                .filter(|task| task.status != "done")
                .count();
            panel_header(
                ui,
                "Память проекта",
                "Долгосрочный контекст, который агент использует между запусками.",
            );
            ui.horizontal_wrapped(|ui| {
                metric_chip(ui, "цели", memory.goals.len());
                metric_chip(ui, "открытые задачи", open_tasks);
                metric_chip(ui, "решения", memory.decisions.len());
                metric_chip(ui, "вопросы", memory.open_questions.len());
            });
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    [(safe_available_width(ui, 160.0) - 64.0).max(96.0), 22.0],
                    TextEdit::singleline(&mut self.memory_goal_input).hint_text("цель проекта"),
                );
                if ui.button("Цель").clicked() {
                    let title = self.memory_goal_input.trim().to_string();
                    if !title.is_empty() {
                        let result = record_project_goal(
                            &workspace,
                            RecordProjectGoalArgs {
                                title,
                                notes: None,
                                status: Some("todo".to_string()),
                            },
                        );
                        self.memory_status = result.output;
                        self.memory_goal_input.clear();
                    }
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    [(safe_available_width(ui, 172.0) - 76.0).max(96.0), 22.0],
                    TextEdit::singleline(&mut self.memory_task_input).hint_text("следующая задача"),
                );
                if ui.button("Задача").clicked() {
                    let title = self.memory_task_input.trim().to_string();
                    if !title.is_empty() {
                        let result = upsert_task(
                            &workspace,
                            UpsertTaskArgs {
                                id: None,
                                title,
                                status: Some("todo".to_string()),
                                notes: None,
                                workstream: None,
                                milestone: None,
                                priority: None,
                            },
                        );
                        self.memory_status = result.output;
                        self.memory_task_input.clear();
                    }
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    [(safe_available_width(ui, 188.0) - 92.0).max(96.0), 22.0],
                    TextEdit::singleline(&mut self.memory_decision_input).hint_text("решение"),
                );
                if ui.button("Решение").clicked() {
                    let title = self.memory_decision_input.trim().to_string();
                    if !title.is_empty() {
                        let result = record_decision(
                            &workspace,
                            RecordDecisionArgs {
                                title,
                                rationale: None,
                            },
                        );
                        self.memory_status = result.output;
                        self.memory_decision_input.clear();
                    }
                }
            });

            ui.separator();
            ui.label(RichText::new("Активная работа").strong());
            let mut shown_tasks = 0;
            for task in memory
                .tasks
                .iter()
                .filter(|task| task.status != "done")
                .take(8)
            {
                shown_tasks += 1;
                ui.horizontal_wrapped(|ui| {
                    chip(ui, &task.status);
                    ui.label(&task.title);
                    if !task.notes.trim().is_empty() {
                        ui.label(RichText::new(&task.notes).weak());
                    }
                });
            }
            if shown_tasks == 0 {
                empty_state(
                    ui,
                    "Открытых задач нет",
                    "У агента пока нет сохранённой активной работы.",
                );
            }
            ui.separator();
            ui.label(RichText::new("Источники контекста").strong());
            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    [(safe_available_width(ui, 182.0) - 86.0).max(96.0), 24.0],
                    TextEdit::singleline(&mut self.memory_source_title_input)
                        .hint_text("название источника"),
                );
                if ui.button("Импорт").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        let title = self.memory_source_title_input.trim().to_string();
                        let result = import_memory_source_file(
                            &workspace,
                            &path,
                            if title.is_empty() { None } else { Some(title) },
                            None,
                        );
                        self.memory_status = result.output;
                        self.memory_source_title_input.clear();
                    }
                }
            });
            ui.add(
                TextEdit::multiline(&mut self.memory_source_note_input)
                    .desired_rows(3)
                    .horizontal_align(egui::Align::Min)
                    .hint_text("заметка, выдержка из документа, ссылка или правило проекта"),
            );
            if ui.button("Сохранить заметку в память").clicked() {
                let note = self.memory_source_note_input.trim().to_string();
                if !note.is_empty() {
                    let title = self.memory_source_title_input.trim().to_string();
                    let result = record_memory_source(
                        &workspace,
                        RecordMemorySourceArgs {
                            id: None,
                            title: if title.is_empty() {
                                "Заметка проекта".to_string()
                            } else {
                                title
                            },
                            kind: Some("note".to_string()),
                            summary: None,
                            content: Some(note),
                            path: None,
                        },
                    );
                    self.memory_status = result.output;
                    self.memory_source_title_input.clear();
                    self.memory_source_note_input.clear();
                }
            }

            let mut remove_source_id = None;
            for source in memory.sources.iter().rev().take(8) {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    chip(ui, &source.kind);
                    ui.label(RichText::new(&source.title).strong());
                    if ui.small_button("Удалить").clicked() {
                        remove_source_id = Some(source.id.clone());
                    }
                });
                if !source.summary.trim().is_empty() {
                    ui.label(RichText::new(compact_inline(&source.summary, 180)).weak());
                } else if !source.content.trim().is_empty() {
                    ui.label(RichText::new(compact_inline(&source.content, 180)).weak());
                }
                if let Some(path) = source.stored_path.as_ref().or(source.original_path.as_ref()) {
                    ui.label(RichText::new(compact_inline(path, 180)).weak());
                }
            }
            if memory.sources.is_empty() {
                empty_state(
                    ui,
                    "Источников пока нет",
                    "Импортируйте файл или сохраните заметку, чтобы агент всегда видел важный контекст проекта.",
                );
            }
            if let Some(id) = remove_source_id {
                let result = remove_memory_source(&workspace, RemoveMemorySourceArgs { id });
                self.memory_status = result.output;
            }

            if !self.memory_status.is_empty() {
                ui.label(RichText::new(&self.memory_status).weak());
            }
        });
    }

    fn show_context_control_center(&mut self, ui: &mut egui::Ui) {
        let Some(workspace) = self.workspace.clone() else {
            empty_state(
                ui,
                "Контекст недоступен",
                "Выберите проект, чтобы управлять чатами, памятью, профилями и историей запусков.",
            );
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            empty_state(
                ui,
                "Нет активного чата",
                "Создайте или выберите чат, чтобы собрать контекст для агента.",
            );
            return;
        };

        panel_header(
            ui,
            "Контекст агента",
            "Разделённый центр: обзор, память проекта, prompt и переносимые профили.",
        );

        let memory = load_memory(&workspace);
        let profiles = list_context_profiles(&workspace);
        let query = if self.context_inspector_query.trim().is_empty() {
            self.input.as_str()
        } else {
            self.context_inspector_query.as_str()
        };
        let snapshot = compile_context_snapshot_with_budget(
            &workspace,
            &conversation_id,
            &self.chat,
            query,
            self.context_budget(),
        );
        let prompt_block = snapshot.to_prompt_block();
        let prompt_chars = prompt_block.chars().count();
        let duplicate_notes = duplicate_context_note_count(&self.context_notes);
        let oversized_sources = memory
            .sources
            .iter()
            .filter(|source| source.content_chars > 40_000)
            .count();
        let missing_goals = memory.goals.is_empty();
        let stale_summary = snapshot.rolling_summary.trim().is_empty() && self.chat.len() > 20;
        let health_score = context_health_score(
            prompt_chars,
            duplicate_notes,
            missing_goals,
            stale_summary,
            oversized_sources,
        );

        context_panel_switcher(ui, &mut self.context_panel_tab);
        ui.add_space(8.0);

        match self.context_panel_tab {
            ContextPanelTab::Overview => {
                flat_section(ui, |ui| {
                    context_overview_visual(
                        ui,
                        health_score,
                        prompt_chars,
                        self.chat.len(),
                        self.context_notes.len(),
                        memory.sources.len(),
                        self.agent_history.len(),
                        profiles.len(),
                        snapshot.recent_messages.len(),
                        snapshot.relevant_messages.len(),
                        snapshot.recent_runs.len(),
                    );
                    context_health_strip(
                        ui,
                        duplicate_notes,
                        missing_goals,
                        stale_summary,
                        oversized_sources,
                    );
                    if !self.context_health_status.trim().is_empty() {
                        full_width_wrapped_label(
                            ui,
                            RichText::new(&self.context_health_status).weak().small(),
                        );
                    }
                });

                flat_section(ui, |ui| {
                    ui.label(RichText::new("Активный диалог").strong())
                        .on_hover_text(
                            "Управление текущим чатом проекта: переключение, закрепление, переименование и экспорт профиля.",
                        );
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Чаты и быстрые действия вынесены отдельно от prompt-бюджета.",
                            )
                            .weak()
                            .small(),
                        )
                        .wrap(),
                    );

                    let active_id = self.active_conversation_id.clone();
                    let mut switch_to: Option<String> = None;
                    let mut pin_toggle: Option<String> = None;
                    for meta in self
                        .conversation_index
                        .conversations
                        .iter()
                        .filter(|meta| !meta.archived)
                        .take(5)
                    {
                        ui.horizontal_wrapped(|ui| {
                            let active = active_id.as_deref() == Some(meta.id.as_str());
                            let chat_response =
                                ui.selectable_label(active, compact_inline(&meta.title, 34));
                            if chat_response.clicked() {
                                switch_to = Some(meta.id.clone());
                            }
                            chat_response.on_hover_text(
                                "Переключает активный диалог. Именно его сообщения и заметки попадут в контекст агента.",
                            );
                            status_line(ui, "сообщений", &meta.message_count.to_string());
                            if meta.pinned {
                                chip(ui, "закреплён");
                            }
                            let pin_response = ui.small_button(if meta.pinned {
                                    "Открепить"
                                } else {
                                    "Закрепить"
                            });
                            if pin_response.clicked() {
                                pin_toggle = Some(meta.id.clone());
                            }
                            pin_response.on_hover_text(if meta.pinned {
                                "Убрать чат из закреплённых. Сам чат не удаляется."
                            } else {
                                "Закрепить чат, чтобы он был проще доступен в списке и профилях контекста."
                            });
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        let new_chat_response = ui.button("Новый чат");
                        if new_chat_response.clicked() {
                            self.create_new_chat();
                        }
                        new_chat_response.on_hover_text(
                            "Создаёт новый пустой диалог внутри текущего проекта.",
                        );
                        let rename_response = ui.button("Переименовать");
                        if rename_response.clicked() {
                            self.begin_rename_active_conversation();
                        }
                        rename_response.on_hover_text("Переименовать активный чат.");
                        let export_response = ui.button("Экспорт профиля");
                        if export_response.clicked() {
                            self.export_active_context_profile();
                        }
                        export_response.on_hover_text(
                            "Сохранить текущий контекст чата в профиль: заметки, бюджет и выбранные настройки.",
                        );
                    });
                    if let Some(id) = switch_to {
                        self.switch_conversation(id);
                    }
                    if let Some(id) = pin_toggle {
                        self.toggle_conversation_pin_by_id(id);
                    }
                });

                flat_section(ui, |ui| {
                    ui.label(RichText::new("Быстро закрепить").strong())
                        .on_hover_text(
                            "Быстро превращает важную строку из истории, roadmap или run log в заметку для следующих запусков агента.",
                        );
                    ui.add(egui::Label::new(
                        RichText::new("Добавьте важное из чата, roadmap или последнего запуска в память текущего диалога.")
                            .weak()
                            .small(),
                    )
                    .wrap());
                    let mut pin_note: Option<String> = None;

                    ui.label(RichText::new("Последние сообщения").small().strong());
                    for line in self
                        .chat
                        .iter()
                        .rev()
                        .filter(|line| !line.content.trim().is_empty())
                        .take(3)
                    {
                        ui.horizontal_wrapped(|ui| {
                            chip(ui, chat_role_label(&line.role));
                            ui.label(RichText::new(compact_inline(&line.content, 120)).small());
                            let memory_response = ui.small_button("В память");
                            if memory_response.clicked() {
                                pin_note = Some(format!(
                                    "{}: {}",
                                    chat_role_label(&line.role),
                                    compact_inline(&line.content, 360)
                                ));
                            }
                            memory_response.on_hover_text(
                                "Закрепить это сообщение как заметку текущего чата. Агент увидит её в следующих запросах.",
                            );
                        });
                    }

                    ui.add_space(6.0);
                    ui.label(RichText::new("Roadmap").small().strong());
                    let roadmap = load_roadmap(&workspace);
                    for item in roadmap
                        .items
                        .iter()
                        .filter(|item| {
                            matches!(item.status, RoadmapStatus::Now | RoadmapStatus::Next)
                        })
                        .take(3)
                    {
                        ui.horizontal_wrapped(|ui| {
                            chip(ui, item.status.as_str());
                            ui.label(RichText::new(compact_inline(&item.title, 120)).small());
                            let memory_response = ui.small_button("В память");
                            if memory_response.clicked() {
                                pin_note = Some(format!(
                                    "Roadmap {}: {} - {}",
                                    item.id,
                                    item.title,
                                    compact_inline(&item.detail, 260)
                                ));
                            }
                            memory_response.on_hover_text(
                                "Закрепить выбранный пункт roadmap как рабочий контекст для агента.",
                            );
                        });
                    }

                    ui.add_space(6.0);
                    ui.label(RichText::new("Последние запуски").small().strong());
                    for record in self.agent_history.iter().rev().take(3) {
                        ui.horizontal_wrapped(|ui| {
                            chip(ui, agent_history_status_label(&record.status));
                            ui.label(
                                RichText::new(compact_inline(&record.user_request, 120)).small(),
                            );
                            let memory_response = ui.small_button("В память");
                            if memory_response.clicked() {
                                pin_note = Some(format!(
                                    "Run {}: {}",
                                    record.id,
                                    compact_inline(
                                        record
                                            .final_report
                                            .as_deref()
                                            .or(record.final_response.as_deref())
                                            .unwrap_or(&record.user_request),
                                        360
                                    )
                                ));
                            }
                            memory_response.on_hover_text(
                                "Закрепить итог прошлого запуска, чтобы агент учитывал его в следующих задачах.",
                            );
                        });
                    }
                    if let Some(note) = pin_note {
                        self.pin_context_note(note);
                    }
                });
            }
            ContextPanelTab::Memory => {
                flat_section(ui, |ui| {
                    ui.label(RichText::new("Закреплённые заметки").strong())
                        .on_hover_text(
                            "Ручная память текущего чата. Эти записи добавляются в контекст агента при следующих запросах.",
                        );
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Короткие факты, которые агент должен помнить в этом чате.",
                            )
                            .weak()
                            .small(),
                        )
                        .wrap(),
                    );
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            TextEdit::singleline(&mut self.context_note_input)
                                .desired_width(safe_available_width(ui, 220.0))
                                .hint_text("важный факт для этого чата"),
                        );
                        let enter_pressed = response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        response.on_hover_text(
                            "Короткая заметка для текущего чата: цель, ограничение, решение или важный факт.",
                        );
                        let add_response = ui.button("Добавить");
                        if add_response.clicked() || enter_pressed {
                            self.add_context_note_from_input();
                        }
                        add_response.on_hover_text(
                            "Закрепить введённый факт в памяти текущего диалога.",
                        );
                    });
                    let notes = self.context_notes.clone();
                    let mut remove_index = None;
                    for (index, note) in notes.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("•").weak());
                            full_width_wrapped_label(ui, RichText::new(note).small());
                            let remove_response = ui.small_button("Убрать");
                            if remove_response.clicked() {
                                remove_index = Some(index);
                            }
                            remove_response
                                .on_hover_text("Удалить эту заметку из контекста текущего чата.");
                        });
                    }
                    if let Some(index) = remove_index {
                        self.remove_context_note(index);
                    }
                    if self.context_notes.is_empty() {
                        empty_state(
                            ui,
                            "Заметок нет",
                            "Закрепите цель, ограничение или архитектурное решение, чтобы агент видел это в следующих запросах.",
                        );
                    }
                });

                flat_section(ui, |ui| {
                    ui.label(RichText::new("Источники проекта").strong())
                        .on_hover_text(
                            "Долговременная память проекта: импортированные документы, заметки и материалы, которые агент может использовать как контекст.",
                        );
                    ui.add(egui::Label::new(
                        RichText::new("Документы и сохранённая память, из которых агент может брать контекст.")
                            .weak()
                            .small(),
                    )
                    .wrap());
                    let mut pin_note: Option<String> = None;
                    for source in memory.sources.iter().rev().take(10) {
                        ui.horizontal_wrapped(|ui| {
                            chip(ui, &source.kind);
                            ui.label(RichText::new(&source.title).strong());
                            let note_response = ui.small_button("В заметки");
                            if note_response.clicked() {
                                let body = if source.summary.trim().is_empty() {
                                    compact_inline(&source.content, 320)
                                } else {
                                    compact_inline(&source.summary, 320)
                                };
                                pin_note = Some(format!("{}: {}", source.title, body));
                            }
                            note_response.on_hover_text(
                                "Скопировать краткое содержание источника в закреплённые заметки чата.",
                            );
                        });
                        full_width_wrapped_label(
                            ui,
                            RichText::new(if source.summary.trim().is_empty() {
                                compact_inline(&source.content, 220)
                            } else {
                                compact_inline(&source.summary, 220)
                            })
                            .weak()
                            .small(),
                        );
                        ui.add_space(4.0);
                    }
                    if memory.sources.is_empty() {
                        empty_state(
                            ui,
                            "Источников нет",
                            "Импортируйте файлы или сохраните заметку в памяти проекта.",
                        );
                    }
                    if let Some(note) = pin_note {
                        self.pin_context_note(note);
                    }
                });
            }
            ContextPanelTab::Prompt => {
                flat_section(ui, |ui| {
                    ui.label(RichText::new("Бюджет prompt").strong())
                        .on_hover_text(
                            "Настройки объёма контекста. Чем больше значения, тем больше агент помнит, но тем выше стоимость и риск лишнего шума.",
                        );
                    ui.add(egui::Label::new(
                        RichText::new("Настройте, сколько свежей переписки, retrieval и запусков попадёт в следующий запрос.")
                            .weak()
                            .small(),
                    )
                    .wrap());
                    let mut budget_changed = false;
                    ui.horizontal_wrapped(|ui| {
                        let short_response = ui.button("Короткий");
                        if short_response.clicked() {
                            self.apply_context_preset("короткий", 8, 4, 2);
                        }
                        short_response.on_hover_text(
                            "Минимальный контекст: дешевле и быстрее, но агент видит меньше истории.",
                        );
                        let balance_response = ui.button("Баланс");
                        if balance_response.clicked() {
                            self.apply_context_preset("баланс", 14, 8, 5);
                        }
                        balance_response.on_hover_text(
                            "Средний режим: обычно хватает истории, retrieval и последних запусков без сильного шума.",
                        );
                        let deep_response = ui.button("Глубокий");
                        if deep_response.clicked() {
                            self.apply_context_preset("глубокий", 32, 16, 10);
                        }
                        deep_response.on_hover_text(
                            "Больше контекста для сложных задач. Может быть дороже и иногда добавляет лишний шум.",
                        );
                    });
                    budget_changed |= ui
                        .add(
                            egui::Slider::new(&mut self.config.context_recent_messages, 0..=80)
                                .text("последние сообщения"),
                        )
                        .on_hover_text(
                            "Сколько последних сообщений текущего чата попадёт в следующий prompt.",
                        )
                        .changed();
                    budget_changed |= ui
                        .add(
                            egui::Slider::new(&mut self.config.context_relevant_messages, 0..=40)
                                .text("релевантные"),
                        )
                        .on_hover_text(
                            "Сколько старых сообщений будет найдено по смыслу текущего запроса и добавлено в prompt.",
                        )
                        .changed();
                    budget_changed |= ui
                        .add(
                            egui::Slider::new(&mut self.config.context_recent_runs, 0..=20)
                                .text("запуски"),
                        )
                        .on_hover_text(
                            "Сколько последних сохранённых запусков агента добавить как рабочую память.",
                        )
                        .changed();
                    if budget_changed {
                        let _ = self.config.save();
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Запрос").weak().small());
                        ui.add(
                            TextEdit::singleline(&mut self.context_inspector_query)
                                .desired_width(safe_available_width(ui, 220.0))
                                .hint_text("пусто = поле ввода"),
                        )
                        .on_hover_text(
                            "Текст для предпросмотра retrieval. Если оставить пустым, используется текущий текст из нижнего поля ввода.",
                        );
                    });
                });

                flat_section(ui, |ui| {
                    ui.label(RichText::new("Состав следующего prompt").strong())
                        .on_hover_text(
                            "Диагностика того, из каких источников будет собран следующий запрос к модели.",
                        );
                    ui.horizontal_wrapped(|ui| {
                        status_line(
                            ui,
                            "summary",
                            &snapshot.rolling_summary.chars().count().to_string(),
                        );
                        status_line(ui, "последние", &snapshot.recent_messages.len().to_string());
                        status_line(
                            ui,
                            "релевантные",
                            &snapshot.relevant_messages.len().to_string(),
                        );
                        status_line(ui, "runs", &snapshot.recent_runs.len().to_string());
                    });
                    context_signal_row(
                        ui,
                        "последние сообщения",
                        snapshot.recent_messages.len(),
                        self.config.context_recent_messages.max(1),
                        egui::Color32::from_rgb(96, 191, 143),
                    );
                    context_signal_row(
                        ui,
                        "релевантные",
                        snapshot.relevant_messages.len(),
                        self.config.context_relevant_messages.max(1),
                        accent_color(),
                    );
                    context_signal_row(
                        ui,
                        "запуски",
                        snapshot.recent_runs.len(),
                        self.config.context_recent_runs.max(1),
                        egui::Color32::from_rgb(220, 174, 92),
                    );
                });

                flat_collapsing_section(ui, "Технический prompt preview", false, |ui| {
                    let mut prompt_preview = prompt_block;
                    ui.add(
                        TextEdit::multiline(&mut prompt_preview)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(safe_available_width(ui, 320.0))
                            .desired_rows(12)
                            .interactive(false),
                    );
                });
            }
            ContextPanelTab::Profiles => {
                flat_section(ui, |ui| {
                    ui.label(RichText::new("Профили контекста").strong())
                        .on_hover_text(
                            "Снимки контекста можно экспортировать, проверить и затем применить в другом чате или проекте.",
                        );
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Снимки контекста для переноса между чатами и проектами.",
                            )
                            .weak()
                            .small(),
                        )
                        .wrap(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let export_response = ui.button("Экспорт текущего");
                        if export_response.clicked() {
                            self.export_active_context_profile();
                        }
                        export_response.on_hover_text(
                            "Сохранить текущий набор контекста в файл профиля: чат, заметки и настройки бюджета.",
                        );
                        let preview_response = ui.button("Предпросмотр файла");
                        if preview_response.clicked() {
                            self.pick_context_profile_for_preview();
                        }
                        preview_response.on_hover_text(
                            "Выбрать файл профиля и посмотреть, что будет импортировано, без немедленного применения.",
                        );
                    });
                });

                if let Some(preview) = self.context_profile_preview.clone() {
                    flat_section(ui, |ui| {
                        ui.label(RichText::new("Предпросмотр импорта").strong())
                            .on_hover_text(
                                "Показывает, какие новые заметки и настройки будут добавлены при применении профиля.",
                            );
                        full_width_wrapped_label(
                            ui,
                            RichText::new(format!(
                                "{} · {} · {} заметок",
                                preview.profile.title,
                                agent_history_date_label(preview.profile.exported_at),
                                preview.profile.context_notes.len()
                            ))
                            .weak()
                            .small(),
                        );
                        let new_notes = context_profile_new_notes(
                            &preview.profile.context_notes,
                            &self.context_notes,
                        );
                        let duplicate_count = preview
                            .profile
                            .context_notes
                            .len()
                            .saturating_sub(new_notes.len());
                        ui.horizontal_wrapped(|ui| {
                            status_line(ui, "новых", &new_notes.len().to_string());
                            status_line(ui, "дубликатов", &duplicate_count.to_string());
                            status_line(
                                ui,
                                "бюджет",
                                &context_profile_budget_diff(
                                    self.context_budget(),
                                    preview.profile.budget.bounded(),
                                ),
                            );
                        });
                        for note in new_notes.iter().take(5) {
                            full_width_wrapped_label(
                                ui,
                                RichText::new(format!("+ {note}")).small(),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            let apply_response = ui.button("Применить профиль");
                            if apply_response.clicked() {
                                self.apply_context_profile_preview();
                            }
                            apply_response.on_hover_text(
                                "Добавить новые заметки и настройки выбранного профиля в текущий чат.",
                            );
                            let close_response = ui.button("Закрыть preview");
                            if close_response.clicked() {
                                self.context_profile_preview = None;
                            }
                            close_response.on_hover_text(
                                "Закрыть предпросмотр. Профиль не будет применён.",
                            );
                        });
                    });
                }

                flat_section(ui, |ui| {
                    if profiles.is_empty() {
                        empty_state(
                            ui,
                            "Экспортов пока нет",
                            "Экспортируйте текущий профиль, чтобы переносить контекст между чатами и проектами.",
                        );
                    }
                    for entry in profiles.iter().take(10) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(compact_inline(&entry.profile.title, 32)).strong(),
                            );
                            ui.label(
                                RichText::new(agent_history_date_label(entry.profile.exported_at))
                                    .weak()
                                    .small(),
                            );
                            let preview_response = ui.small_button("Предпросмотр");
                            if preview_response.clicked() {
                                self.context_profile_preview = Some(ContextProfilePreview {
                                    path: entry.abs_path.clone(),
                                    profile: entry.profile.clone(),
                                });
                            }
                            preview_response.on_hover_text(
                                "Открыть сравнение профиля с текущими заметками перед импортом.",
                            );
                        });
                        full_width_wrapped_label(
                            ui,
                            RichText::new(compact_inline(&entry.rel_path, 120))
                                .weak()
                                .small(),
                        );
                    }
                });
            }
        }
    }

    fn pin_context_note(&mut self, note: String) {
        let note = compact_inline(note.trim(), 500);
        if note.is_empty() {
            return;
        }
        if !self.context_notes.iter().any(|existing| {
            normalized_context_note_key(existing) == normalized_context_note_key(&note)
        }) {
            self.context_notes.push(note);
        }
        self.save_context_notes();
    }

    fn toggle_conversation_pin_by_id(&mut self, conversation_id: String) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let pinned = self
            .conversation_index
            .conversations
            .iter()
            .find(|meta| meta.id == conversation_id)
            .map(|meta| !meta.pinned)
            .unwrap_or(true);
        match set_conversation_pinned(&workspace, &conversation_id, pinned) {
            Ok(index) => {
                self.conversation_index = index;
                self.conversation_status = if pinned {
                    "чат закреплён".to_string()
                } else {
                    "чат откреплён".to_string()
                };
            }
            Err(err) => {
                self.conversation_status = format!("не удалось изменить закрепление: {err}");
            }
        }
    }

    fn pick_context_profile_for_preview(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Context profile", &["json"])
            .pick_file()
        else {
            return;
        };
        self.preview_context_profile_path(path);
    }

    fn preview_context_profile_path(&mut self, path: PathBuf) {
        match read_context_profile_file(&path) {
            Ok(profile) => {
                self.context_profile_preview = Some(ContextProfilePreview { path, profile });
                self.conversation_status = "профиль контекста открыт для предпросмотра".to_string();
            }
            Err(err) => {
                self.conversation_status = format!("не удалось открыть профиль контекста: {err}");
            }
        }
    }

    fn apply_context_profile_preview(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            return;
        };
        let Some(preview) = self.context_profile_preview.clone() else {
            return;
        };
        match import_context_profile_file(&workspace, &preview.path, &conversation_id) {
            Ok((state, budget)) => {
                self.context_notes = state.context_notes;
                self.config.context_recent_messages = budget.recent_message_limit;
                self.config.context_relevant_messages = budget.relevant_message_limit;
                self.config.context_recent_runs = budget.recent_run_limit;
                let _ = self.config.save();
                self.context_profile_preview = None;
                self.context_note_suggestions.clear();
                self.conversation_status = "профиль контекста применён".to_string();
            }
            Err(err) => {
                self.conversation_status = format!("не удалось применить профиль контекста: {err}");
            }
        }
    }

    fn show_provider_health_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Провайдеры", |ui| {
            let report = provider_health_report(&self.config);
            let ready_chat = report
                .chat_providers
                .iter()
                .filter(|provider| {
                    provider.implemented && provider.key_present && provider.model_known
                })
                .count();
            let ready_assets = report
                .asset_providers
                .iter()
                .filter(|provider| provider.key_present)
                .count();
            let issue_count = report.issues.len()
                + report
                    .chat_providers
                    .iter()
                    .map(|provider| provider.issues.len())
                    .sum::<usize>()
                + report
                    .asset_providers
                    .iter()
                    .map(|provider| provider.issues.len())
                    .sum::<usize>();
            panel_header(
                ui,
                "Провайдеры",
                "Настроенные AI-бэкенды для кода, текста, изображений, аудио и видео.",
            );
            ui.horizontal_wrapped(|ui| {
                metric_chip(ui, "готово чат-моделей", ready_chat);
                metric_chip(ui, "готово asset-моделей", ready_assets);
                metric_chip(ui, "проблем", issue_count);
            });
            if !report.issues.is_empty() {
                ui.label(RichText::new(report.issues.join(", ")).weak());
            }

            ui.separator();
            ui.label(RichText::new("Чат-модели").strong());
            for provider in &report.chat_providers {
                ui.horizontal_wrapped(|ui| {
                    provider_row(
                        ui,
                        &provider.name,
                        provider.key_present,
                        &provider.selected_model,
                        if provider.issues.is_empty() {
                            "ок".to_string()
                        } else {
                            provider.issues.join(", ")
                        },
                    );
                    if ui
                        .add_enabled(
                            provider.implemented
                                && provider.key_present
                                && provider.model_known
                                && !self.provider_validation_running,
                            egui::Button::new("Live smoke"),
                        )
                        .on_hover_text(
                            "Реальный запрос к модели: короткий текстовый ответ и проверка формы tool-call.",
                        )
                        .clicked()
                    {
                        self.start_provider_live_validation(provider.id.clone());
                    }
                });
            }

            ui.separator();
            ui.label(RichText::new("Asset-модели").strong());
            for provider in &report.asset_providers {
                ui.horizontal_wrapped(|ui| {
                    provider_row(
                        ui,
                        &provider.name,
                        provider.key_present,
                        &provider.selected_model,
                        if provider.issues.is_empty() {
                            provider.env_var.clone()
                        } else {
                            provider.issues.join(", ")
                        },
                    );
                    if ui
                        .add_enabled(
                            provider.key_present
                                && self.workspace.is_some()
                                && !self.asset_is_running,
                            egui::Button::new("Платный smoke"),
                        )
                        .on_hover_text(
                            "Запускает маленькую тестовую генерацию и сохраняет результат в Asset Studio.",
                        )
                        .clicked()
                    {
                        self.start_asset_smoke_validation(&provider.id);
                    }
                });
            }
            ui.separator();
            ui.label(RichText::new("Live validation plan").strong());
            for step in provider_validation_plan(&self.config).iter().take(18) {
                status_line(
                    ui,
                    &format!("{} / {}", step.provider_name, step.check),
                    &step.status,
                );
            }
            if !self.provider_validation_results.is_empty() {
                ui.separator();
                ui.label(RichText::new("Последние live-проверки").strong());
                for result in self.provider_validation_results.iter().rev().take(6) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!(
                            "{} {}",
                            result.provider_name, result.model
                        ))
                        .strong());
                        chip(ui, if result.ok { "успешно" } else { "ошибка" });
                        ui.label(RichText::new(format!("{} мс", result.elapsed_ms)).weak());
                    });
                    for check in &result.checks {
                        status_line(
                            ui,
                            &check.check,
                            if check.ok {
                                "ок"
                            } else {
                                check.detail.as_str()
                            },
                        );
                    }
                    ui.add_space(4.0);
                }
            }
            if !self.provider_health_status.is_empty() {
                ui.label(RichText::new(&self.provider_health_status).weak());
            }
        });
    }

    fn show_asset_library_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Библиотека ассетов", |ui| {
            let Some(workspace) = self.workspace.clone() else {
                empty_state(
                    ui,
                    "Нужна рабочая папка",
                    "Выберите проект, чтобы просматривать сгенерированные ассеты.",
                );
                return;
            };
            let library = load_library(&workspace);
            let favorites = library
                .entries
                .iter()
                .filter(|entry| entry.favorite)
                .count();
            panel_header(
                ui,
                "Библиотека ассетов",
                "Сгенерированные изображения, аудио, видео и переиспользуемые ассеты для игр/приложений.",
            );
            ui.horizontal_wrapped(|ui| {
                metric_chip(ui, "ассеты", library.entries.len());
                metric_chip(ui, "избранное", favorites);
            });
            ui.add_sized(
                [safe_available_width(ui, 120.0), 22.0],
                TextEdit::singleline(&mut self.asset_library_filter).hint_text("фильтр или тег"),
            );

            let filter = self.asset_library_filter.trim().to_ascii_lowercase();
            let matching_entries = library
                .entries
                .iter()
                .filter(|entry| {
                    filter.is_empty()
                        || entry.path.to_ascii_lowercase().contains(&filter)
                        || entry.prompt.to_ascii_lowercase().contains(&filter)
                        || entry
                            .tags
                            .iter()
                            .any(|tag| tag.to_ascii_lowercase().contains(&filter))
                })
                .collect::<Vec<_>>();
            if library.entries.is_empty() {
                empty_state(
                    ui,
                    "Сгенерированных ассетов нет",
                    "Попросите агента создать изображение, звук или видео.",
                );
            } else if matching_entries.is_empty() {
                empty_state(
                    ui,
                    "Ничего не найдено",
                    "Очистите или измените фильтр, чтобы увидеть библиотеку.",
                );
            }

            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for entry in matching_entries.iter().take(16) {
                        ui.horizontal_wrapped(|ui| {
                            let mut favorite = entry.favorite;
                            if ui.checkbox(&mut favorite, "избр.").changed() {
                                let result = favorite_asset(
                                    &workspace,
                                    FavoriteAssetArgs {
                                        path: entry.path.clone(),
                                        favorite,
                                    },
                                );
                                self.asset_library_status = result.output;
                            }
                            ui.label(
                                RichText::new(&entry.path).text_style(egui::TextStyle::Monospace),
                            );
                            chip(ui, asset_kind_label(&entry.kind));
                            if !entry.provider.trim().is_empty() {
                                chip(ui, &entry.provider);
                            }
                            for tag in entry.tags.iter().take(4) {
                                chip(ui, tag);
                            }
                        });
                        if !entry.prompt.trim().is_empty() {
                            ui.label(RichText::new(compact_inline(&entry.prompt, 96)).weak());
                        }
                    }
                });
            if !self.asset_library_status.is_empty() {
                ui.label(RichText::new(&self.asset_library_status).weak());
            }
        });
    }

    fn show_evals_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Проверки", |ui| {
            let Some(workspace) = self.workspace.clone() else {
                empty_state(
                    ui,
                    "Нужна рабочая папка",
                    "Выберите проект, чтобы запускать локальные replay-проверки.",
                );
                return;
            };
            let state = load_orchestration_state(&workspace);
            let results = load_results(&workspace);
            let clean_runs = results
                .runs
                .iter()
                .filter(|run| run.issues.is_empty())
                .count();
            panel_header(
                ui,
                "Проверки",
                "Статические replay-проверки промптов, инструментов и критериев успеха.",
            );
            ui.horizontal_wrapped(|ui| {
                metric_chip(ui, "кейсы", state.evals.len());
                metric_chip(ui, "запуски", results.runs.len());
                metric_chip(ui, "чистые", clean_runs);
            });
            if ui
                .add_enabled(
                    !state.evals.is_empty(),
                    egui::Button::new("Запустить статические проверки"),
                )
                .clicked()
            {
                let result = run_replay_eval(&workspace, RunReplayEvalArgs { eval_id: None });
                self.eval_status = result.output;
            }
            if state.evals.is_empty() {
                empty_state(
                    ui,
                    "Replay-кейсов нет",
                    "Используйте инструменты оркестрации, чтобы записать кейсы для повторяемых проверок.",
                );
            }
            for eval in state.evals.iter().rev().take(6) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&eval.id).text_style(egui::TextStyle::Monospace));
                    ui.label(&eval.name);
                    metric_chip(ui, "инструменты", eval.expected_tools.len());
                });
            }
            if !results.runs.is_empty() {
                ui.separator();
                ui.label(RichText::new("Последние результаты").strong());
            }
            for run in results.runs.iter().rev().take(4) {
                ui.horizontal_wrapped(|ui| {
                    chip(ui, eval_run_status_label(&run.status));
                    ui.label(&run.name);
                    if !run.issues.is_empty() {
                        ui.label(RichText::new(run.issues.join(", ")).weak());
                    }
                });
            }
            if !self.eval_status.is_empty() {
                ui.label(RichText::new(&self.eval_status).weak());
            }
        });
    }

    fn show_environment_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Диагностика", |ui| {
            let diagnostics = environment_diagnostics(&self.config, self.workspace.as_ref());
            panel_header(
                ui,
                "Диагностика окружения",
                "Локальные пути, proxy, toolchain и release-проверки без секретов.",
            );
            status_line(ui, "Версия", &diagnostics.app_version);
            status_line(
                ui,
                "Платформа",
                &format!("{} / {}", diagnostics.os, diagnostics.arch),
            );
            status_line(ui, "Процесс", &diagnostics.executable);
            status_line(ui, "Текущая папка", &diagnostics.current_dir);
            if let Some(workspace) = diagnostics.workspace.as_deref() {
                status_line(ui, "Workspace", workspace);
            }
            if let Some(config_path) = diagnostics.config_path.as_deref() {
                status_line(ui, "Config", config_path);
            }
            if let Some(journal_path) = diagnostics.journal_path.as_deref() {
                status_line(ui, "Journal", journal_path);
            }
            if let Some(crash_dir) = diagnostics.crash_dir.as_deref() {
                status_line(ui, "Crash reports", crash_dir);
            }
            status_line(ui, "Proxy", &diagnostics.proxy);
            status_line(ui, "System proxy", &diagnostics.system_proxy);

            ui.separator();
            ui.label(RichText::new("Инструменты").strong());
            for item in &diagnostics.tools {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&item.name).monospace());
                    chip(ui, item.status.as_str());
                    ui.label(RichText::new(&item.detail).weak());
                });
            }

            ui.separator();
            ui.label(RichText::new("Release policy").strong());
            for item in &diagnostics.release_notes {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&item.name).monospace());
                    chip(ui, item.status.as_str());
                    ui.label(RichText::new(&item.detail).weak());
                });
            }
        });
    }

    fn show_tool_panel(&mut self, ctx: &egui::Context) {
        let allowed_panels = self.workspace_mode.panels();
        if !allowed_panels.contains(&self.right_panel_view) {
            self.right_panel_view = self.workspace_mode.default_panel();
            self.persist_layout_state();
        }

        egui::SidePanel::right("tools")
            .resizable(true)
            .default_width(360.0)
            .width_range(260.0..=760.0)
            .frame(side_panel_frame())
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (panel_title, panel_subtitle) = match self.right_panel_view {
                        RightPanelView::Context => (
                            "Контекст агента",
                            "что агент помнит, что попадёт в prompt и где есть риск шума",
                        ),
                        RightPanelView::Roadmap => (
                            "Roadmap",
                            "история проекта, текущий фокус и следующие этапы",
                        ),
                        RightPanelView::Release => (
                            "Релиз",
                            "версии, сборки, артефакты и preflight перед публикацией",
                        ),
                        RightPanelView::Logs => {
                            ("Журнал", "история запусков, инструменты, git и трассировка")
                        }
                        RightPanelView::Project => {
                            ("Проект", "команды, терминал, preview и рабочий стол")
                        }
                        RightPanelView::Assets => {
                            ("Ассеты", "генерация, библиотека, экспорт и варианты")
                        }
                        RightPanelView::Control => (
                            "Контроль",
                            "доступ, память, провайдеры, проверки и окружение",
                        ),
                        RightPanelView::Overview => {
                            ("Сводка", "состояние агента, проекта и быстрые переходы")
                        }
                    };
                    let panel_tooltip = self.right_panel_view.tooltip();
                    ui.vertical(|ui| {
                        ui.label(RichText::new(panel_title).strong().size(18.0))
                            .on_hover_text(panel_tooltip);
                        ui.label(RichText::new(panel_subtitle).weak().small())
                            .on_hover_text(panel_tooltip);
                    });
                    if self.is_running || self.project_is_running || self.asset_is_running {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let refresh_response = ui.button("Обновить");
                        if refresh_response.clicked() {
                            self.refresh_file_rows();
                            self.refresh_git_summary();
                            self.refresh_project_profiles();
                        }
                        refresh_response.on_hover_text(
                            "Обновляет дерево файлов, Git-сводку и список профилей проекта.",
                        );
                    });
                });
                ui.add_space(6.0);

                let previous_panel_view = self.right_panel_view;
                panel_switcher(ui, &mut self.right_panel_view, allowed_panels);
                if self.right_panel_view != previous_panel_view {
                    self.persist_layout_state();
                }
                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .id_salt("right_workspace_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.right_panel_view {
                        RightPanelView::Overview => self.show_right_overview(ui),
                        RightPanelView::Context => self.show_context_control_center(ui),
                        RightPanelView::Roadmap => self.show_roadmap_panel(ui),
                        RightPanelView::Release => self.show_release_cockpit(ui),
                        RightPanelView::Project => {
                            flat_section(ui, |ui| self.show_project_panel(ui));
                            flat_section(ui, |ui| self.show_terminal_panel(ui));
                            flat_section(ui, |ui| self.show_desktop_panel(ui, ctx));
                        }
                        RightPanelView::Assets => {
                            flat_section(ui, |ui| self.show_asset_panel(ui, ctx));
                            flat_section(ui, |ui| self.show_asset_library_panel(ui));
                        }
                        RightPanelView::Control => {
                            flat_section(ui, |ui| self.show_governance_panel(ui));
                            flat_section(ui, |ui| self.show_memory_panel(ui));
                            flat_section(ui, |ui| self.show_provider_health_panel(ui));
                            flat_section(ui, |ui| self.show_evals_panel(ui));
                            flat_section(ui, |ui| self.show_environment_panel(ui));
                        }
                        RightPanelView::Logs => {
                            flat_section(ui, |ui| self.show_orchestration_panel(ui));
                            flat_section(ui, |ui| self.show_agent_history_explorer(ui));
                            self.show_git_section(ui);
                            self.show_tool_log_section(ui);
                        }
                    });
            });
    }

    fn show_roadmap_panel(&mut self, ui: &mut egui::Ui) {
        let workspace = self.workspace.clone();
        let roadmap = workspace.as_ref().map(load_roadmap);
        let completed_stages = roadmap
            .as_ref()
            .map(|roadmap| {
                roadmap
                    .items
                    .iter()
                    .filter(|item| item.status == RoadmapStatus::Done)
                    .count()
            })
            .unwrap_or(0);
        let active_tasks = roadmap
            .as_ref()
            .map(|roadmap| {
                roadmap
                    .items
                    .iter()
                    .filter(|item| item.status == RoadmapStatus::Now)
                    .count()
            })
            .unwrap_or(0);
        let planned_tasks = roadmap
            .as_ref()
            .map(|roadmap| {
                roadmap
                    .items
                    .iter()
                    .filter(|item| item.status == RoadmapStatus::Next)
                    .count()
            })
            .unwrap_or(0);
        let total_items = roadmap
            .as_ref()
            .map(|roadmap| roadmap.items.len())
            .unwrap_or(0);
        let progress = if total_items == 0 {
            0.0
        } else {
            (completed_stages as f32 / total_items as f32).clamp(0.0, 1.0)
        };
        let current_focus = roadmap
            .as_ref()
            .and_then(|roadmap| {
                roadmap
                    .items
                    .iter()
                    .find(|item| !roadmap.focus.is_empty() && item.id == roadmap.focus)
                    .or_else(|| {
                        roadmap
                            .items
                            .iter()
                            .find(|item| item.status == RoadmapStatus::Now)
                    })
            })
            .map(|item| item.title.clone())
            .unwrap_or_else(|| "Roadmap".to_string());

        ui.horizontal_wrapped(|ui| {
            if ui.button("Зафиксировать этап").clicked() {
                self.input = "Зафиксируй текущий milestone в roadmap проекта через record_milestone: что сделано, какие файлы изменены, какие проверки пройдены, какие риски остались и что делать дальше.".to_string();
                self.active_center_tab = CenterTab::Agent;
            }
            if ui.button("+ Запись").clicked() {
                self.input =
                    "Добавь запись в roadmap проекта по текущему контексту через plan_roadmap_item или update_roadmap_item.".to_string();
                self.active_center_tab = CenterTab::Agent;
            }
            if ui.button("Экспорт").clicked() {
                if let Some(workspace) = &workspace {
                    match roadmap_markdown_export(workspace, None) {
                        Ok(path) => {
                            self.roadmap_status = format!("Экспортировано: {path}");
                        }
                        Err(err) => {
                            self.roadmap_status = format!("Ошибка экспорта: {err}");
                        }
                    }
                } else {
                    self.roadmap_status = "Выберите проект для экспорта roadmap.".to_string();
                }
            }
        });
        if !self.roadmap_status.trim().is_empty() {
            ui.add_space(4.0);
            full_width_wrapped_label(ui, RichText::new(&self.roadmap_status).weak().small());
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        ui.columns(3, |columns| {
            roadmap_metric(&mut columns[0], completed_stages, "этапов закрыто");
            roadmap_metric(&mut columns[1], active_tasks, "в работе");
            roadmap_metric(&mut columns[2], planned_tasks, "запланировано");
        });

        flat_section(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Текущий фокус").strong());
                chip(ui, &compact_inline(&current_focus, 36));
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    roadmap
                        .as_ref()
                        .map(|roadmap| roadmap.title.as_str())
                        .unwrap_or("Project Roadmap"),
                )
                .strong(),
            );
            full_width_wrapped_label(
                ui,
                RichText::new(
                    roadmap
                        .as_ref()
                        .map(|roadmap| roadmap.progress_note.as_str())
                        .filter(|note| !note.trim().is_empty())
                        .unwrap_or("Единая история проекта: прошлое, текущая работа, будущие цели и финальное видение."),
                )
                .weak()
                .small(),
            );
            ui.add_space(6.0);
            ui.add(
                egui::ProgressBar::new(progress)
                    .desired_width(safe_available_width(ui, 120.0))
                    .text(format!(
                        "{:.0}% · {} пунктов",
                        progress * 100.0,
                        total_items
                    )),
            );
        });

        flat_section(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Лента развития").strong());
                chip(ui, "Agent");
            });
            ui.horizontal_wrapped(|ui| {
                for filter in RoadmapFilter::ALL {
                    if ui
                        .selectable_label(self.roadmap_filter == filter, filter.label())
                        .clicked()
                    {
                        self.roadmap_filter = filter;
                    }
                }
            });
            ui.add_space(4.0);
            if let Some(roadmap) = &roadmap {
                let filter = self.roadmap_filter;
                let visible_items = roadmap
                    .items
                    .iter()
                    .filter(|item| {
                        roadmap_entry_visible(filter, roadmap_status_to_entry(item.status))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let mut shown = 0usize;
                for item in visible_items.iter() {
                    self.show_roadmap_item(ui, item, workspace.as_ref());
                    shown += 1;
                    if shown >= 18 {
                        full_width_wrapped_label(
                            ui,
                            RichText::new("Показаны первые 18 пунктов. Используйте фильтр, чтобы сузить ленту.")
                                .weak()
                                .small(),
                        );
                        break;
                    }
                }
                if shown == 0 {
                    full_width_wrapped_label(
                        ui,
                        RichText::new("В выбранном фильтре пока нет пунктов roadmap.")
                            .weak()
                            .small(),
                    );
                }
            } else {
                full_width_wrapped_label(
                    ui,
                    RichText::new("Выберите проект, чтобы открыть живую дорожную карту.")
                        .weak()
                        .small(),
                );
            }
        });

        ui.label(RichText::new("Финальные цели").strong());
        if let Some(roadmap) = roadmap {
            for goal in roadmap.goals.iter().take(6) {
                roadmap_goal(ui, &goal.title);
            }
        } else {
            roadmap_goal(
                ui,
                "Выберите проект, чтобы связать roadmap с памятью проекта.",
            );
        }
    }

    fn show_roadmap_item(
        &mut self,
        ui: &mut egui::Ui,
        item: &RoadmapItem,
        workspace: Option<&Workspace>,
    ) {
        let mut detail = item.detail.clone();
        let links = roadmap_links_summary(item);
        if !links.is_empty() {
            if !detail.trim().is_empty() {
                detail.push_str(" · ");
            }
            detail.push_str(&links);
        }
        let time = if item.date_label.trim().is_empty() {
            match item.status {
                RoadmapStatus::Done => "готово",
                RoadmapStatus::Now => "сейчас",
                RoadmapStatus::Next => "далее",
            }
        } else {
            item.date_label.as_str()
        };
        roadmap_entry_row(
            ui,
            roadmap_status_to_entry(item.status),
            &item.title,
            &detail,
            time,
        );
        ui.horizontal_wrapped(|ui| {
            ui.add_space(18.0);
            ui.label(RichText::new("Статус").weak().small());
            if ui.small_button("Сейчас").clicked() {
                self.set_roadmap_item_status(workspace, &item.id, "now");
            }
            if ui.small_button("Готово").clicked() {
                self.set_roadmap_item_status(workspace, &item.id, "done");
            }
            if ui.small_button("Далее").clicked() {
                self.set_roadmap_item_status(workspace, &item.id, "next");
            }
        });
        ui.add_space(4.0);
    }

    fn set_roadmap_item_status(
        &mut self,
        workspace: Option<&Workspace>,
        item_id: &str,
        status: &str,
    ) {
        let Some(workspace) = workspace else {
            self.roadmap_status = "Выберите проект, чтобы изменить roadmap.".to_string();
            return;
        };
        let result = crate::roadmap::update_roadmap_item(
            workspace,
            UpdateRoadmapItemArgs {
                id: item_id.to_string(),
                title: None,
                detail: None,
                status: Some(status.to_string()),
                focus: Some(status == "now"),
                commits: Vec::new(),
                changed_files: Vec::new(),
                agent_run_id: None,
                validation: None,
                memory_ids: Vec::new(),
            },
        );
        self.roadmap_status = if result.ok {
            format!("Roadmap обновлён: {item_id} -> {status}")
        } else {
            result.output
        };
    }

    fn show_release_cockpit(&mut self, ui: &mut egui::Ui) {
        let workspace = self.workspace.clone();
        let diagnostics = environment_diagnostics(&self.config, workspace.as_ref());
        let version = workspace
            .as_ref()
            .and_then(|workspace| release_version_label(workspace.root()))
            .unwrap_or_else(|| "версия не найдена".to_string());
        let artifacts = workspace
            .as_ref()
            .map(|workspace| release_artifacts(workspace.root()))
            .unwrap_or_default();
        let package_script_present = workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .root()
                    .join("scripts")
                    .join("package-windows.ps1")
                    .is_file()
            })
            .unwrap_or(false);
        let checklist = release_checklist(
            workspace.as_ref(),
            &self.project_profiles,
            &self.git_summary,
            &self.project_runs,
            &diagnostics,
            package_script_present,
            !artifacts.is_empty(),
        );
        let passed = checklist.iter().filter(|item| item.ok).count();
        let readiness = if checklist.is_empty() {
            0.0
        } else {
            (passed as f32 / checklist.len() as f32).clamp(0.0, 1.0)
        };

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    workspace.is_some() && !self.project_is_running,
                    egui::Button::new("Проверка"),
                )
                .on_hover_text(
                    "Запустить check/typecheck/lint, если такая команда найдена в профиле проекта.",
                )
                .clicked()
            {
                self.start_release_command_by_ids(&["check", "typecheck", "lint"], "проверка");
            }
            if ui
                .add_enabled(
                    workspace.is_some() && !self.project_is_running,
                    egui::Button::new("Тесты"),
                )
                .on_hover_text("Запустить тесты проекта перед релизом.")
                .clicked()
            {
                self.start_release_command_by_ids(&["test"], "тесты");
            }
            if ui
                .add_enabled(
                    workspace.is_some() && !self.project_is_running,
                    egui::Button::new("Release build"),
                )
                .on_hover_text("Собрать release-бинарник или production build.")
                .clicked()
            {
                self.start_release_command_by_ids(&["release", "build"], "release build");
            }
            if ui
                .add_enabled(
                    workspace.is_some() && package_script_present && !self.project_is_running,
                    egui::Button::new("Package"),
                )
                .on_hover_text(
                    "Запустить scripts/package-windows.ps1 и создать portable-артефакты.",
                )
                .clicked()
            {
                self.start_release_command_by_ids(&["package"], "упаковка");
            }
            if ui
                .add_enabled(workspace.is_some(), egui::Button::new("Git status"))
                .on_hover_text("Обновить Git-сводку перед публикацией.")
                .clicked()
            {
                self.show_git_status_from_ui();
            }
            if ui
                .add_enabled(workspace.is_some(), egui::Button::new("Открыть dist"))
                .on_hover_text("Открыть папку dist с portable build, zip и SHA256.")
                .clicked()
            {
                self.open_release_dist_folder();
            }
            if ui
                .add_enabled(workspace.is_some(), egui::Button::new("В Roadmap"))
                .on_hover_text(
                    "Зафиксировать текущую версию, готовность, проверки и артефакты как milestone Roadmap.",
                )
                .clicked()
            {
                self.record_release_milestone_from_ui(&version, &checklist, &artifacts, readiness, passed);
            }
        });

        if self.project_is_running {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(RichText::new("идёт команда проекта").weak());
                if ui.button("Стоп").clicked() {
                    self.stop_project_command();
                }
            });
        }

        if !self.project_status.trim().is_empty() {
            full_width_wrapped_label(ui, RichText::new(&self.project_status).weak().small());
        }

        if workspace.is_none() {
            empty_state(
                ui,
                "Проект не выбран",
                "Выберите рабочую папку, чтобы увидеть релизный чеклист, команды и артефакты.",
            );
            return;
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        ui.columns(3, |columns| {
            roadmap_metric(
                &mut columns[0],
                format!("{:.0}%", readiness * 100.0),
                "готовность",
            );
            roadmap_metric(&mut columns[1], passed, "пунктов ok");
            roadmap_metric(&mut columns[2], artifacts.len(), "артефактов");
        });

        flat_section(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Кандидат релиза").strong());
                chip(ui, version);
            });
            ui.add_space(6.0);
            ui.add(
                egui::ProgressBar::new(readiness)
                    .desired_width(safe_available_width(ui, 120.0))
                    .text(format!(
                        "{:.0}% · {} из {} пунктов",
                        readiness * 100.0,
                        passed,
                        checklist.len()
                    )),
            );
            ui.add_space(4.0);
            full_width_wrapped_label(
                ui,
                RichText::new(
                    "Перед публикацией пройдите проверку, тесты, release build, упаковку и убедитесь, что Git чистый.",
                )
                .weak()
                .small(),
            );
        });

        flat_section(ui, |ui| {
            panel_header(
                ui,
                "Preflight-чеклист",
                "Главные условия, которые стоит закрыть перед публикацией.",
            );
            for item in &checklist {
                release_check_row(ui, item);
            }
        });

        flat_section(ui, |ui| {
            panel_header(
                ui,
                "Артефакты",
                "Release-бинарник, portable-папка, zip и SHA256 manifest.",
            );
            if artifacts.is_empty() {
                full_width_wrapped_label(
                    ui,
                    RichText::new(
                        "Артефакты пока не найдены. Запустите Release build или Package.",
                    )
                    .weak()
                    .small(),
                );
            } else {
                for artifact in artifacts.iter().take(10) {
                    release_artifact_row(ui, artifact);
                }
            }
        });

        flat_section(ui, |ui| {
            panel_header(
                ui,
                "Последние релизные запуски",
                "Check, test, build, release и package из истории команд проекта.",
            );
            let runs = self
                .project_runs
                .iter()
                .rev()
                .filter(|run| release_command_kind(&run.command.id).is_some())
                .take(6)
                .cloned()
                .collect::<Vec<_>>();
            if runs.is_empty() {
                full_width_wrapped_label(
                    ui,
                    RichText::new("Релизные команды ещё не запускались.")
                        .weak()
                        .small(),
                );
            }
            for run in runs {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&run.label).strong());
                    chip(ui, run.status.label());
                    if let Some(kind) = release_command_kind(&run.command.id) {
                        chip(ui, kind);
                    }
                    if let Some(code) = run.exit_code {
                        chip(ui, format!("exit {code}"));
                    }
                    if ui
                        .add_enabled(!self.project_is_running, egui::Button::new("Повторить"))
                        .clicked()
                    {
                        self.start_project_command(run.command.clone());
                    }
                    if ui
                        .add_enabled(
                            run.status == ProjectRunStatus::Failed,
                            egui::Button::new("Исправить"),
                        )
                        .clicked()
                    {
                        self.prepare_fix_prompt_from_run(&run);
                    }
                });
                if !run.error_summary.is_empty() {
                    full_width_wrapped_label(
                        ui,
                        RichText::new(compact_inline(&run.error_summary.join("; "), 220))
                            .color(egui::Color32::from_rgb(235, 154, 154))
                            .small(),
                    );
                }
                ui.add_space(4.0);
            }
        });

        flat_section(ui, |ui| {
            panel_header(
                ui,
                "Окружение публикации",
                "Toolchain, proxy, config, журнал и crash reports.",
            );
            ui.horizontal_wrapped(|ui| {
                for item in &diagnostics.tools {
                    metric_chip(ui, &item.name, &item.status);
                }
            });
            ui.add_space(6.0);
            for item in &diagnostics.release_notes {
                release_diagnostic_row(ui, item);
            }
        });
    }

    fn start_release_command_by_ids(&mut self, ids: &[&str], label: &str) {
        if self.project_is_running {
            return;
        }
        if let Some(command) = find_command_by_ids(&self.project_profiles, ids) {
            self.start_project_command(command);
        } else {
            self.project_status = format!("Команда для '{label}' не найдена в профиле проекта.");
        }
    }

    fn open_release_dist_folder(&mut self) {
        let Some(workspace) = self.workspace.clone() else {
            self.project_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let dist = workspace.root().join("dist");
        if !dist.exists() {
            self.project_status =
                "Папка dist пока не создана. Запустите Package или Release build.".to_string();
            return;
        }
        self.open_project_folder(&dist);
        self.project_status = format!("открыто: {}", dist.display());
    }

    fn record_release_milestone_from_ui(
        &mut self,
        version: &str,
        checklist: &[ReleaseChecklistItem],
        artifacts: &[ReleaseArtifact],
        readiness: f32,
        passed: usize,
    ) {
        let Some(workspace) = self.workspace.clone() else {
            self.roadmap_status = "Выберите проект, чтобы зафиксировать релиз.".to_string();
            return;
        };

        self.refresh_git_summary();
        let validation = release_validation_summary(checklist, passed);
        let detail = release_milestone_detail(
            readiness,
            passed,
            checklist,
            artifacts,
            &self.project_runs,
            &self.git_summary,
        );
        let changed_files = artifacts
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>();
        let result = record_milestone(
            &workspace,
            RecordMilestoneArgs {
                title: format!("Release checkpoint: {version}"),
                detail,
                item_id: Some(format!("release-{}", current_unix_timestamp())),
                status: Some("done".to_string()),
                commits: Vec::new(),
                changed_files,
                agent_run_id: None,
                validation: Some(validation),
                memory_ids: Vec::new(),
            },
        );

        if result.ok {
            self.roadmap_status = format!("Релиз зафиксирован в Roadmap: {version}");
            self.project_status = self.roadmap_status.clone();
            self.tool_log.push(ToolLogLine {
                title: "roadmap release".to_string(),
                content: result.output,
            });
            self.refresh_file_rows();
            self.refresh_git_summary();
        } else {
            self.roadmap_status = format!("Не удалось зафиксировать релиз: {}", result.output);
            self.project_status = self.roadmap_status.clone();
        }
    }

    fn show_right_overview(&mut self, ui: &mut egui::Ui) {
        flat_section(ui, |ui| {
            panel_header(
                ui,
                "Состояние",
                "Главные индикаторы без раскрытия всех инструментов.",
            );
            status_line(
                ui,
                "Агент",
                if self.is_running {
                    "работает"
                } else {
                    "ожидает"
                },
            );
            status_line(
                ui,
                "Проект",
                if self.project_is_running {
                    "выполняется команда"
                } else {
                    "ожидает"
                },
            );
            status_line(
                ui,
                "Ассеты",
                if self.asset_is_running {
                    "генерация"
                } else {
                    "ожидают"
                },
            );
            status_line(
                ui,
                "Терминал",
                if self.terminal_running {
                    "запущен"
                } else {
                    "остановлен"
                },
            );

            let policy_label = policy_profile_labels()
                .iter()
                .find(|(id, _)| *id == self.config.policy_profile)
                .map(|(_, label)| *label)
                .unwrap_or("Обычный");
            status_line(ui, "Доступ", policy_label);

            if !self.self_modification_status.trim().is_empty() {
                status_line(ui, "Self-mod", &self.self_modification_status);
            }

            if let Some(prompt) = &self.pending_approval {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("ожидает подтверждения")
                        .strong()
                        .color(accent_color()),
                );
                ui.label(compact(&prompt.summary, 140));
            }
        });

        flat_section(ui, |ui| {
            panel_header(ui, "Проект", "Выбранная рабочая папка и быстрый профиль.");
            if let Some(workspace) = &self.workspace {
                ui.label(RichText::new(workspace.display_name()).strong());
                status_line(ui, "Файлы", &self.file_rows.len().to_string());
                status_line(ui, "Профили", &self.project_profiles.len().to_string());
            } else {
                empty_state(
                    ui,
                    "Проект не выбран",
                    "Нажмите «Проект» сверху и выберите папку.",
                );
            }

            if !self.project_status.is_empty() {
                ui.label(RichText::new(&self.project_status).weak());
            }
        });

        flat_section(ui, |ui| {
            panel_header(
                ui,
                "Быстрые переходы",
                "Открыть нужную группу инструментов.",
            );
            ui.horizontal_wrapped(|ui| {
                for view in [
                    RightPanelView::Context,
                    RightPanelView::Roadmap,
                    RightPanelView::Release,
                    RightPanelView::Project,
                    RightPanelView::Assets,
                    RightPanelView::Control,
                    RightPanelView::Logs,
                ] {
                    if ui.link(view.label()).clicked() {
                        self.right_panel_view = view;
                    }
                }
            });
        });

        if !self.tool_log.is_empty() {
            flat_section(ui, |ui| {
                panel_header(ui, "Последний инструмент", "Краткий хвост выполнения.");
                if let Some(line) = self.tool_log.last() {
                    inline_log_entry(ui, &line.title, &compact(&line.content, 260));
                }
            });
        }
    }

    fn show_git_section(&mut self, ui: &mut egui::Ui) {
        flat_collapsing_section(ui, "Git", true, |ui| {
            ui.horizontal_wrapped(|ui| {
                let has_workspace = self.workspace.is_some();
                if ui
                    .add_enabled(has_workspace, egui::Button::new("Коммит"))
                    .clicked()
                {
                    self.git_commit_dialog_open = true;
                }
                if ui
                    .add_enabled(has_workspace, egui::Button::new("Пуш"))
                    .clicked()
                {
                    self.run_git_action_from_ui("git push", &["push"]);
                }
                if ui
                    .add_enabled(has_workspace, egui::Button::new("Статус"))
                    .clicked()
                {
                    self.show_git_status_from_ui();
                }
                if ui
                    .add_enabled(has_workspace, egui::Button::new("Пулл"))
                    .clicked()
                {
                    self.run_git_action_from_ui("git pull", &["pull"]);
                }
            });
            if !self.git_action_status.trim().is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(&self.git_action_status)
                            .text_style(egui::TextStyle::Monospace)
                            .weak(),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
            }
            ui.add(
                egui::Label::new(
                    RichText::new(if self.git_summary.trim().is_empty() {
                        "git status пока не загружен"
                    } else {
                        &self.git_summary
                    })
                    .text_style(egui::TextStyle::Monospace),
                )
                .wrap(),
            );
        });
    }

    fn show_tool_log_section(&mut self, ui: &mut egui::Ui) {
        flat_collapsing_section(ui, "Журнал", true, |ui| {
            self.show_journal_panel(ui);
            ui.add_space(6.0);
            if self.tool_log.is_empty() {
                empty_state(
                    ui,
                    "Журнал инструментов пуст",
                    "Здесь появятся действия агента, команды и результаты инструментов.",
                );
            } else {
                for line in self.tool_log.iter().rev().take(12) {
                    inline_log_entry(ui, &line.title, &line.content);
                }
            }
        });
    }

    fn show_project_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Проект");
            if self.project_is_running {
                ui.spinner();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Обновить"))
                .clicked()
            {
                self.refresh_project_profiles();
            }
            if ui
                .add_enabled(self.project_is_running, egui::Button::new("Стоп"))
                .clicked()
            {
                self.stop_project_command();
            }
        });

        if !self.project_status.is_empty() {
            let shown = compact_inline(&self.project_status, 120);
            let response = ui.label(RichText::new(&shown).weak());
            if shown != self.project_status {
                response.on_hover_text(&self.project_status);
            }
        }

        if self.workspace.is_none() {
            ui.label(RichText::new("Рабочая папка не выбрана").weak());
            return;
        }

        if self.project_profiles.is_empty() {
            ui.label(RichText::new("Профиль проекта не обнаружен").weak());
            self.show_game_workflow_buttons(ui);
            return;
        }

        let profiles = self.project_profiles.clone();
        for profile in profiles {
            let profile_commands = profile.commands.clone();
            let preview_hooks = profile.previews.clone();
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&profile.kind).strong());
                    ui.label(RichText::new(&profile.name).weak());
                });
                if !profile.markers.is_empty() {
                    ui.label(
                        RichText::new(format!("маркеры: {}", profile.markers.join(", ")))
                            .text_style(egui::TextStyle::Monospace)
                            .weak(),
                    );
                }

                if profile.commands.is_empty() {
                    ui.label(RichText::new("Для этого профиля пока нет быстрых команд").weak());
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for command in profile.commands {
                            let response = ui
                                .add_enabled(
                                    !self.project_is_running,
                                    egui::Button::new(&command.label),
                                )
                                .on_hover_text(format!(
                                    "{}\n{}",
                                    command.description, command.command
                                ));
                            if response.clicked() {
                                self.start_project_command(command);
                            }
                        }
                    });
                }

                if !preview_hooks.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Предпросмотр").weak());
                        for hook in preview_hooks {
                            let response = ui
                                .add_enabled(
                                    !self.project_is_running,
                                    egui::Button::new(&hook.label),
                                )
                                .on_hover_text(&hook.description);
                            if response.clicked() {
                                if let Some(url) = hook.url.as_deref() {
                                    self.open_preview_url_from_ui(url);
                                } else if let Some(command_id) = hook.command_id.as_deref() {
                                    if let Some(command) = profile_commands
                                        .iter()
                                        .find(|command| command.id == command_id)
                                        .cloned()
                                    {
                                        self.start_project_command(command);
                                    }
                                }
                            }
                        }
                    });
                }
            });
            ui.add_space(6.0);
        }

        self.show_game_workflow_buttons(ui);
    }

    fn show_desktop_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.collapsing("Рабочий стол", |ui| {
            if self.desktop_status.is_empty()
                && self.desktop_active_window.is_empty()
                && self.desktop_last_screenshot.is_none()
            {
                ui.label(RichText::new("Действий рабочего стола пока нет").weak());
            }

            if !self.desktop_active_window.is_empty() {
                ui.label(RichText::new(&self.desktop_active_window).weak());
            }
            if !self.desktop_status.is_empty() {
                ui.label(RichText::new(&self.desktop_status).weak());
            }

            if let Some(path) = self.desktop_last_screenshot.clone() {
                if let Some(texture) = self.texture_for_asset(ctx, &path) {
                    let size = texture.size_vec2();
                    let scale = (180.0 / size.x.max(size.y)).min(1.0);
                    ui.image((texture.id(), size * scale));
                }
                ui.label(RichText::new(path).text_style(egui::TextStyle::Monospace));
            }
        });
    }

    fn refresh_terminal_snapshot(&mut self) {
        let snapshot = read_terminal_snapshot(Some(400), None);
        self.terminal_running = snapshot.running;
        self.terminal_status = match (&snapshot.shell, &snapshot.cwd) {
            (Some(shell), Some(cwd)) => {
                format!("{} | {} | {}", snapshot.status, shell, cwd)
            }
            _ => snapshot.status,
        };
        self.terminal_output = snapshot
            .lines
            .iter()
            .map(|line| format!("{:>5} [{}] {}", line.seq, line.stream, line.text))
            .collect::<Vec<_>>()
            .join("\n");
    }

    fn start_terminal_from_ui(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.terminal_status = "рабочая папка не выбрана".to_string();
            return;
        };
        match start_terminal_session(workspace, None, Some("powershell")) {
            Ok(_) => self.refresh_terminal_snapshot(),
            Err(err) => self.terminal_status = format!("не удалось запустить терминал: {err}"),
        }
    }

    fn write_terminal_from_ui(&mut self) {
        let input = self.terminal_input.trim_end().to_string();
        if input.is_empty() {
            return;
        }
        match write_terminal_input(&input, true) {
            Ok(_) => {
                self.terminal_input.clear();
                self.refresh_terminal_snapshot();
            }
            Err(err) => self.terminal_status = format!("не удалось отправить команду: {err}"),
        }
    }

    fn stop_terminal_from_ui(&mut self) {
        match stop_terminal_session() {
            Ok(_) => self.refresh_terminal_snapshot(),
            Err(err) => self.terminal_status = format!("не удалось остановить терминал: {err}"),
        }
    }

    fn clear_terminal_from_ui(&mut self) {
        let _ = clear_terminal_output();
        self.refresh_terminal_snapshot();
    }

    fn show_terminal_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Терминал", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.workspace.is_some() && !self.terminal_running,
                        egui::Button::new("Запустить"),
                    )
                    .clicked()
                {
                    self.start_terminal_from_ui();
                }
                if ui
                    .add_enabled(self.terminal_running, egui::Button::new("Стоп"))
                    .clicked()
                {
                    self.stop_terminal_from_ui();
                }
                if ui.button("Очистить").clicked() {
                    self.clear_terminal_from_ui();
                }
                if ui.button("Обновить").clicked() {
                    self.refresh_terminal_snapshot();
                }
            });
            if !self.terminal_status.is_empty() {
                ui.label(RichText::new(&self.terminal_status).weak());
            }

            let response = ui.add(
                TextEdit::singleline(&mut self.terminal_input)
                    .hint_text("Команда")
                    .desired_width(safe_available_width(ui, 160.0)),
            );
            let enter_pressed =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (enter_pressed || ui.button("Отправить").clicked()) && self.terminal_running
            {
                self.write_terminal_from_ui();
            }

            let mut output = self.terminal_output.clone();
            ui.add(
                TextEdit::multiline(&mut output)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(safe_available_width(ui, 160.0))
                    .horizontal_align(egui::Align::Min)
                    .desired_rows(12)
                    .interactive(false),
            );
        });
    }

    fn show_game_workflow_buttons(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Игровые сценарии", |ui| {
            ui.horizontal_wrapped(|ui| {
                for spec in workflow_specs() {
                    if ui
                        .add_enabled(self.workspace.is_some(), egui::Button::new(spec.label))
                        .on_hover_text(spec.description)
                        .clicked()
                    {
                        self.create_game_workflow_from_ui(spec.id);
                    }
                }
            });
        });
    }

    fn create_agent_handoff_from_ui(&mut self, role_id: &str) {
        let Some(workspace) = self.workspace.clone() else {
            self.orchestration_status = "рабочая папка не выбрана".to_string();
            return;
        };
        let Some(role) = parse_agent_role(role_id) else {
            self.orchestration_status = format!("неизвестная роль агента: {role_id}");
            return;
        };

        let task = if self.input.trim().is_empty() {
            format!(
                "Изучи текущую рабочую папку {} и предложи следующие полезные действия.",
                workspace.display_name()
            )
        } else {
            self.input.trim().to_string()
        };
        let context = format!(
            "Рабочая папка: {}\nВыбранный файл: {}\nТекущий промпт: {}",
            workspace.display_name(),
            self.selected_file.as_deref().unwrap_or("нет"),
            if self.input.trim().is_empty() {
                "нет"
            } else {
                self.input.trim()
            }
        );

        match record_handoff(
            &workspace,
            role,
            "Leetcode UI".to_string(),
            task,
            context,
            "Рекомендация специалиста, риски и следующие действия".to_string(),
        ) {
            Ok(record) => {
                self.orchestration_status = format!("передача записана: {}", record.id);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => {
                self.orchestration_status = format!("не удалось записать передачу: {err}");
            }
        }
    }

    fn export_trace_from_ui(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.orchestration_status = "рабочая папка не выбрана".to_string();
            return;
        };

        match export_trace(workspace) {
            Ok(path) => {
                self.orchestration_status = format!("трасса экспортирована: {path}");
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => {
                self.orchestration_status = format!("не удалось экспортировать трассу: {err}");
            }
        }
    }

    fn add_orchestration_snapshot_to_log(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.orchestration_status = "рабочая папка не выбрана".to_string();
            return;
        };

        let snapshot = orchestration_snapshot(workspace);
        let content = serde_json::to_string_pretty(&snapshot)
            .unwrap_or_else(|_| "не удалось сериализовать снимок оркестрации".to_string());
        self.tool_log.push(ToolLogLine {
            title: "снимок оркестрации".to_string(),
            content,
        });
        self.orchestration_status = "снимок добавлен в журнал инструментов".to_string();
    }

    fn show_orchestration_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Агенты", |ui| {
            if self.workspace.is_none() {
                ui.label(RichText::new("Рабочая папка не выбрана").weak());
                return;
            }

            let state = self.workspace.as_ref().map(load_orchestration_state);
            if let Some(state) = &state {
                ui.label(
                    RichText::new(format!(
                        "передачи: {} | субагенты: {} | итоги: {} | проверки: {}",
                        state.handoffs.len(),
                        state.subagent_runs.len(),
                        state.run_summaries.len(),
                        state.evals.len()
                    ))
                    .weak(),
                );
                if !state.context.summary.trim().is_empty() {
                    ui.label(compact(&state.context.summary, 180));
                }
            }

            ui.horizontal_wrapped(|ui| {
                if ui.button("Снимок").clicked() {
                    self.add_orchestration_snapshot_to_log();
                }
                if ui.button("Экспорт трассы").clicked() {
                    self.export_trace_from_ui();
                }
            });

            if !self.orchestration_status.is_empty() {
                ui.label(RichText::new(&self.orchestration_status).weak());
            }

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for spec in agent_role_specs() {
                    if ui
                        .add_enabled(self.workspace.is_some(), egui::Button::new(spec.label))
                        .on_hover_text(spec.purpose)
                        .clicked()
                    {
                        self.create_agent_handoff_from_ui(spec.id);
                    }
                }
            });
        });
    }

    fn show_asset_import_controls(&mut self, ui: &mut egui::Ui) {
        let kind = self
            .asset_compare_paths
            .last()
            .map(|path| asset_kind_for_rel_path(path))
            .unwrap_or(AssetKind::Image);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Импорт в проект").strong());
            egui::ComboBox::from_id_salt("asset_import_target_select")
                .selected_text(compact_inline(&self.asset_import_target_input, 32))
                .width(190.0)
                .show_ui(ui, |ui| {
                    for target in asset_import_targets(&kind) {
                        ui.selectable_value(
                            &mut self.asset_import_target_input,
                            (*target).to_string(),
                            *target,
                        );
                    }
                });
            ui.add_sized(
                [safe_available_width(ui, 160.0), 24.0],
                TextEdit::singleline(&mut self.asset_import_target_input)
                    .hint_text("каталог внутри проекта"),
            );
        });
    }

    fn show_asset_comparison_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Сравнение вариантов").strong());
            ui.label(RichText::new(format!("{} / 4", self.asset_compare_paths.len())).weak());
            if ui
                .add_enabled(
                    !self.asset_compare_paths.is_empty(),
                    egui::Button::new("Очистить"),
                )
                .clicked()
            {
                self.asset_compare_paths.clear();
            }
        });
        self.show_asset_import_controls(ui);

        if self.asset_compare_paths.is_empty() {
            ui.label(
                RichText::new(
                    "Нажмите «Сравнить» на карточке ассета, чтобы увидеть варианты рядом.",
                )
                .weak(),
            );
            return;
        }

        let selected = self.asset_compare_paths.clone();
        let columns = selected.len().clamp(1, 4);
        let mut remove_path: Option<String> = None;
        let mut import_path: Option<String> = None;
        let mut open_path: Option<String> = None;
        ui.columns(columns, |columns| {
            for (index, rel_path) in selected.iter().enumerate() {
                let column = &mut columns[index];
                let job = self
                    .asset_jobs
                    .iter()
                    .find(|job| job.output_files.iter().any(|output| output == rel_path))
                    .cloned();
                column.vertical(|ui| {
                    if let Some(texture) = self.texture_for_asset(ctx, rel_path) {
                        let size = texture.size_vec2();
                        let scale = (180.0 / size.x.max(size.y)).min(1.0);
                        ui.image((texture.id(), size * scale));
                    } else {
                        ui.label(
                            RichText::new(asset_kind_label(&asset_kind_for_rel_path(rel_path)))
                                .strong(),
                        );
                    }
                    ui.label(RichText::new(compact_inline(rel_path, 58)).monospace());
                    if let Some(job) = &job {
                        ui.horizontal_wrapped(|ui| {
                            chip(ui, asset_status_label(&job.status));
                            chip(ui, image_provider_name(&job.provider));
                            chip(ui, &job.model);
                        });
                        ui.add(
                            egui::Label::new(
                                RichText::new(compact_inline(&job.prompt, 140)).weak(),
                            )
                            .wrap(),
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("В проект").clicked() {
                            import_path = Some(rel_path.clone());
                        }
                        if ui.small_button("Папка").clicked() {
                            open_path = Some(rel_path.clone());
                        }
                        if ui.small_button("Убрать").clicked() {
                            remove_path = Some(rel_path.clone());
                        }
                    });
                });
            }
        });
        if let Some(path) = remove_path {
            self.asset_compare_paths.retain(|known| known != &path);
        }
        if let Some(path) = import_path {
            self.import_asset_output_to_project(&path);
        }
        if let Some(path) = open_path {
            self.open_asset_folder(&path);
        }
    }

    fn show_asset_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.heading("Ассеты");
            if self.asset_is_running {
                ui.spinner();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Открыть папку"))
                .clicked()
            {
                self.open_generated_assets_folder();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Обновить"))
                .clicked()
            {
                if let Some(workspace) = &self.workspace {
                    self.asset_jobs = load_jobs(workspace);
                }
            }
        });

        let old_asset_provider = self.asset_provider_input.clone();
        let old_asset_kind = self.asset_kind_input.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Тип");
            egui::ComboBox::from_id_salt("asset_kind_select")
                .selected_text(match self.asset_kind_input.as_str() {
                    "spritesheet" => "Спрайт-лист",
                    "audio" => "Аудио",
                    "video" => "Видео",
                    _ => "Изображение",
                })
                .width(118.0)
                .show_ui(ui, |ui| {
                    for (id, label) in [
                        ("image", "Изображение"),
                        ("spritesheet", "Спрайт-лист"),
                        ("audio", "Аудио"),
                        ("video", "Видео"),
                    ] {
                        ui.selectable_value(&mut self.asset_kind_input, id.to_string(), label);
                    }
                });
        });
        if self.asset_kind_input != old_asset_kind {
            self.sync_asset_provider_settings_for(&old_asset_provider);
            self.switch_asset_kind_from_ui();
        }
        if matches!(self.asset_kind_input.as_str(), "image" | "spritesheet") {
            ui.horizontal_wrapped(|ui| {
                ui.label("Провайдер изображений");
                egui::ComboBox::from_id_salt("asset_provider_select")
                    .selected_text(image_provider_name(&self.asset_provider_input))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for provider in image_provider_specs() {
                            ui.selectable_value(
                                &mut self.asset_provider_input,
                                provider.id.to_string(),
                                provider.name,
                            );
                        }
                    });
            });
            if self.asset_provider_input != old_asset_provider {
                self.sync_asset_provider_settings_for(&old_asset_provider);
                self.switch_asset_provider_from_ui(self.asset_provider_input.clone());
            }

            if let Some(provider) = image_provider_specs()
                .iter()
                .find(|provider| provider.id == self.asset_provider_input)
            {
                ui.label(
                    RichText::new(format!("{} | {}", provider.notes, provider.env_var)).weak(),
                );
            }
        } else {
            let provider_label = if self.asset_kind_input == "video" {
                video_provider_name(OPENAI_VIDEO_PROVIDER_ID)
            } else {
                audio_provider_name(OPENAI_AUDIO_PROVIDER_ID)
            };
            ui.label(RichText::new(format!("{provider_label} | OPENAI_API_KEY")).weak());
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Модель");
            ui.add_sized(
                [safe_available_width(ui, 120.0), 22.0],
                TextEdit::singleline(&mut self.asset_model_input),
            );
        });

        ui.horizontal_wrapped(|ui| {
            ui.label(
                if matches!(self.asset_kind_input.as_str(), "image" | "spritesheet") {
                    "Ключ изображений"
                } else {
                    "Ключ медиа"
                },
            );
            let key_width = (safe_available_width(ui, 178.0) - 82.0).max(96.0);
            ui.add_sized(
                [key_width, 22.0],
                TextEdit::singleline(&mut self.asset_api_key_input).password(true),
            );
            if ui.button("Сохранить").clicked() {
                self.save_settings_from_ui();
            }
        });

        ui.add(
            TextEdit::multiline(&mut self.asset_prompt)
                .hint_text("Промпт для игрового/app-ассета")
                .desired_width(safe_available_width(ui, 160.0))
                .horizontal_align(egui::Align::Min)
                .desired_rows(3),
        );

        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("asset_aspect_ratio")
                .selected_text(&self.asset_aspect_ratio)
                .width(72.0)
                .show_ui(ui, |ui| {
                    for ratio in ["1:1", "3:2", "2:3", "4:3", "3:4", "16:9", "9:16"] {
                        ui.selectable_value(&mut self.asset_aspect_ratio, ratio.to_string(), ratio);
                    }
                });
            egui::ComboBox::from_id_salt("asset_image_size")
                .selected_text(&self.asset_image_size)
                .width(72.0)
                .show_ui(ui, |ui| {
                    let sizes: &[&str] = if self.asset_kind_input == "video" {
                        &["1280x720", "720x1280", "1920x1080", "1080x1920"]
                    } else {
                        &["0.5K", "1K", "2K", "4K"]
                    };
                    for size in sizes {
                        ui.selectable_value(&mut self.asset_image_size, (*size).to_string(), *size);
                    }
                });
            if ui
                .add_enabled(
                    !self.asset_is_running && self.workspace.is_some(),
                    egui::Button::new(match self.asset_kind_input.as_str() {
                        "spritesheet" => "Сгенерировать спрайт-лист",
                        "audio" => "Сгенерировать аудио",
                        "video" => "Сгенерировать видео",
                        _ => "Сгенерировать изображение",
                    }),
                )
                .clicked()
            {
                self.start_image_asset_job();
            }
        });

        if !self.asset_status.is_empty() {
            ui.label(RichText::new(&self.asset_status).weak());
        }
        self.show_asset_comparison_panel(ui, ctx);

        egui::ScrollArea::vertical()
            .id_salt("asset_jobs_scroll")
            .max_height(260.0)
            .show(ui, |ui| {
                if self.asset_jobs.is_empty() {
                    ui.label(RichText::new("Сгенерированных ассетов пока нет").weak());
                    return;
                }

                let jobs = self
                    .asset_jobs
                    .iter()
                    .rev()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>();
                for job in jobs {
                    self.show_asset_card(ui, ctx, job);
                }
            });
    }

    fn show_asset_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, job: AssetJob) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(asset_status_label(&job.status)).strong());
                ui.label(RichText::new(image_provider_name(&job.provider)).weak());
                ui.label(RichText::new(&job.model).weak());
            });

            ui.label(compact(&job.prompt, 170));
            if let Some(error) = &job.error {
                ui.label(RichText::new(compact(error, 180)).weak());
            }

            let first_output = job.output_files.first().cloned();
            if let Some(output) = &first_output {
                if let Some(texture) = self.texture_for_asset(ctx, output) {
                    let size = texture.size_vec2();
                    let scale = (132.0 / size.x.max(size.y)).min(1.0);
                    ui.image((texture.id(), size * scale));
                }
                ui.label(RichText::new(output).text_style(egui::TextStyle::Monospace));
            } else {
                ui.label(RichText::new("Выходного файла нет").weak());
            }

            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !self.asset_is_running && self.workspace.is_some(),
                        egui::Button::new("Повторить"),
                    )
                    .clicked()
                {
                    self.regenerate_asset_job(&job);
                }
                if ui
                    .add_enabled(
                        !self.asset_is_running && self.workspace.is_some(),
                        egui::Button::new("Вариация"),
                    )
                    .clicked()
                {
                    self.vary_asset_job(&job);
                }
                if let Some(output) = first_output.as_deref() {
                    let compared = self.asset_is_compared(output);
                    if ui
                        .button(if compared {
                            "Убрать из сравнения"
                        } else {
                            "Сравнить"
                        })
                        .clicked()
                    {
                        self.toggle_asset_comparison(output);
                    }
                    if ui.button("В проект").clicked() {
                        self.import_asset_output_to_project(output);
                    }
                    if ui.button("Сделать иконкой").clicked() {
                        self.use_asset_as_app_icon(output);
                    }
                    if ui.button("Увеличить").clicked() {
                        self.upscale_asset_output(output);
                    }
                    if ui.button("Экспорт").clicked() {
                        self.export_asset_output(output);
                    }
                    if ui.button("Прикрепить").clicked() {
                        self.attach_asset_output(output);
                    }
                    if ui.button("Открыть папку").clicked() {
                        self.open_asset_folder(output);
                    }
                }
                if ui.button("Загрузить промпт").clicked() {
                    self.load_asset_job_into_form(&job);
                }
            });
        });
        ui.add_space(6.0);
    }

    fn show_center_tabs(&mut self, ui: &mut egui::Ui) {
        let mut close_path: Option<String> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let selected = self.active_center_tab == CenterTab::Agent;
            let (agent_response, _) = center_tab_button(ui, "agent", "Agent", selected, false);
            if agent_response.clicked() {
                self.active_center_tab = CenterTab::Agent;
            }

            let tabs = self
                .file_tabs
                .iter()
                .map(|tab| {
                    let dirty = tab.editable && tab.content != tab.original_content;
                    (
                        tab.path.clone(),
                        file_tree_name(&tab.path).to_string(),
                        dirty,
                    )
                })
                .collect::<Vec<_>>();
            for (path, name, dirty) in tabs {
                let selected = self.active_center_tab == CenterTab::File(path.clone());
                let title = if dirty { format!("{name} *") } else { name };
                let (tab_response, close_clicked) =
                    center_tab_button(ui, path.as_str(), &title, selected, true);
                if tab_response.clicked() && !close_clicked {
                    self.active_center_tab = CenterTab::File(path.clone());
                    if let Some(index) = self.file_tabs.iter().position(|tab| tab.path == path) {
                        self.sync_selected_from_file_tab(index);
                    }
                }
                if close_clicked {
                    close_path = Some(path);
                }
            }
        });

        if let Some(path) = close_path {
            self.close_file_tab(&path);
        }

        ui.separator();
        ui.add_space(8.0);

        match self.active_center_tab.clone() {
            CenterTab::Agent => self.show_agent_tab(ui),
            CenterTab::File(path) => self.show_file_preview_tab(ui, &path),
        }
    }

    fn show_agent_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Leetcode").strong().size(24.0));
            ui.label(
                RichText::new("Локальный агент для кода, ассетов и рабочего стола")
                    .weak()
                    .small(),
            );
            if self.is_running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("агент работает").weak().small());
                });
            } else if let Some(workspace) = &self.workspace {
                ui.label(
                    RichText::new(format!("Проект: {}", workspace.display_name()))
                        .weak()
                        .small(),
                );
            }
        });
        ui.add_space(10.0);

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Чат").weak().small());
            let current_title = self.active_conversation_title();
            let mut selected_id = self.active_conversation_id.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("conversation_select")
                .selected_text(current_title)
                .width(260.0)
                .show_ui(ui, |ui| {
                    for meta in self
                        .conversation_index
                        .conversations
                        .iter()
                        .filter(|meta| !meta.archived)
                    {
                        let pin = if meta.pinned {
                            "закреплён · "
                        } else {
                            ""
                        };
                        let label = format!("{pin}{} · {}", meta.title, meta.message_count);
                        ui.selectable_value(&mut selected_id, meta.id.clone(), label);
                    }
                });
            if !selected_id.is_empty()
                && self.active_conversation_id.as_deref() != Some(selected_id.as_str())
            {
                self.switch_conversation(selected_id);
            }
            if ui.button("Новый чат").clicked() {
                self.create_new_chat();
            }
            if ui.button("Переименовать").clicked() {
                self.begin_rename_active_conversation();
            }
            let active_pinned = self
                .active_conversation_id
                .as_deref()
                .and_then(|id| {
                    self.conversation_index
                        .conversations
                        .iter()
                        .find(|meta| meta.id == id)
                })
                .map(|meta| meta.pinned)
                .unwrap_or(false);
            if ui
                .button(if active_pinned {
                    "Открепить"
                } else {
                    "Закрепить"
                })
                .clicked()
            {
                self.toggle_active_conversation_pin();
            }
            ui.menu_button("...", |ui| {
                if ui.button("Архивировать чат").clicked() {
                    self.archive_active_conversation();
                    ui.close_menu();
                }
                if ui.button("Удалить чат").clicked() {
                    self.delete_active_conversation();
                    ui.close_menu();
                }
            });
            if !self.conversation_status.trim().is_empty() {
                ui.label(RichText::new(&self.conversation_status).weak().small());
            }
        });
        if self.conversation_rename_target == self.active_conversation_id {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Название").weak().small());
                let response = ui.add(
                    TextEdit::singleline(&mut self.conversation_rename_input)
                        .desired_width(320.0)
                        .hint_text("Название чата"),
                );
                let enter_pressed =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui.button("Сохранить").clicked() || enter_pressed {
                    self.save_active_conversation_title();
                }
                if ui.button("Отмена").clicked() {
                    self.conversation_rename_target = None;
                    self.conversation_rename_input.clear();
                }
            });
        }
        self.show_context_inspector(ui);
        self.show_context_note_suggestions(ui);
        self.show_archived_conversations(ui);
        ui.add_space(10.0);

        let has_dialog_content = self.chat.iter().any(|line| {
            matches!(line.role, ChatRole::User | ChatRole::Assistant)
                && !line.content.trim().is_empty()
        });
        if !has_dialog_content
            && self.pending_run_gate.is_none()
            && self.pending_approval.is_none()
            && !self.is_running
        {
            self.show_agent_daily_home(ui);
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("chat_transcript_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show_viewport(ui, |ui, viewport| {
                // Keep transcript content constrained to the visible chat viewport, not to
                // the scroll area's virtual content width. Otherwise egui may place the row
                // as if it was much wider than the central panel, which makes messages look
                // centered on the screen instead of starting at the chat area's left edge.
                let transcript_width = viewport.width().max(1.0);
                ui.set_min_width(transcript_width);
                ui.set_max_width(transcript_width);
                ui.allocate_ui_with_layout(
                    egui::vec2(transcript_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(transcript_width);
                        ui.set_max_width(transcript_width);
                        ui.add_space(4.0);
                        for (index, line) in self.chat.iter().enumerate() {
                            let live_status = if self.is_running
                                && self.agent_user_message_index == Some(index)
                                && !self.agent_live_status.trim().is_empty()
                            {
                                Some(self.agent_live_status.as_str())
                            } else {
                                None
                            };
                            chat_message(ui, line, live_status);
                            if self.run_timeline_anchor_index == Some(index) {
                                if let Some(timeline) = &self.run_timeline {
                                    run_timeline_card(ui, timeline);
                                }
                            }
                        }
                        if self.pending_run_gate.is_some() {
                            self.show_pending_run_gate_message(ui);
                        }
                        if self.pending_approval.is_some() {
                            self.show_pending_approval_message(ui);
                        }
                    },
                );
            });
    }

    fn show_agent_daily_home(&mut self, ui: &mut egui::Ui) {
        ui.add_space(22.0);
        let width = safe_available_width(ui, 1.0);
        let project_name = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.display_name())
            .unwrap_or_else(|| "проект не выбран".to_string());
        let ai_label = format!(
            "{} · {}",
            provider_name(&self.provider_input),
            compact_inline(&self.model_input, 20)
        );
        let chat_title = self.active_conversation_title();

        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Готов к работе")
                    .strong()
                    .size(22.0)
                    .color(text_color()),
            );
            chip(ui, format!("AI · {ai_label}"));
            chip(
                ui,
                format!("Проект · {}", compact_inline(&project_name, 24)),
            );
            chip(ui, format!("Чат · {}", compact_inline(&chat_title, 24)));
        });
        ui.add_space(6.0);
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Выберите быстрый старт или напишите задачу внизу. Агент сначала покажет своё понимание и начнёт работу после подтверждения.",
                )
                .weak(),
            )
            .wrap(),
        );
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(12.0);
        ui.label(RichText::new("Быстрый старт").strong());
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Проверить проект")
                .on_hover_text("Определить тип проекта и запустить безопасные проверки.")
                .clicked()
            {
                self.input =
                    "Проверь текущий проект: определи тип, запусти безопасные проверки и дай краткий статус.".to_string();
            }
            if ui
                .button("Следующий шаг")
                .on_hover_text("Сверить roadmap, backlog и текущее состояние.")
                .clicked()
            {
                self.right_panel_view = RightPanelView::Roadmap;
                self.input =
                    "Посмотри roadmap, backlog и состояние проекта. Предложи следующий самый полезный шаг.".to_string();
            }
            if ui
                .button("Релизный preflight")
                .on_hover_text("Открыть релизную панель и подготовить проверку перед сборкой.")
                .clicked()
            {
                self.set_workspace_mode(WorkspaceMode::Project);
                self.right_panel_view = RightPanelView::Release;
                self.input =
                    "Проведи релизный preflight: проверь версию, готовность, команды сборки, артефакты и риски перед публикацией.".to_string();
            }
            if ui
                .button("Зафиксировать milestone")
                .on_hover_text("Подготовить запись для Roadmap по текущему этапу.")
                .clicked()
            {
                self.right_panel_view = RightPanelView::Roadmap;
                self.input =
                    "Подготовь краткий milestone для Roadmap: что сделано, какие файлы затронуты, что осталось дальше.".to_string();
            }
            if ui
                .button("Создать ассет")
                .on_hover_text("Перейти в режим ассетов и подготовить prompt.")
                .clicked()
            {
                self.set_workspace_mode(WorkspaceMode::Assets);
                self.input =
                    "Помоги подготовить ассет для текущего проекта: уточни назначение, формат и предложи prompt для генерации.".to_string();
            }
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Запуск остаётся под вашим контролем: проверьте текст и нажмите «Отправить».",
            )
            .weak()
            .small(),
        );
    }

    fn show_context_note_suggestions(&mut self, ui: &mut egui::Ui) {
        if self.context_note_suggestions.is_empty() {
            return;
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Предложения для памяти").strong());
            ui.label(RichText::new("после последней задачи").weak().small());
            if ui.button("Сохранить все").clicked() {
                self.accept_all_context_note_suggestions();
            }
            if ui.button("Скрыть").clicked() {
                self.context_note_suggestions.clear();
            }
        });

        let suggestions = self.context_note_suggestions.clone();
        let mut accept_index = None;
        for (index, suggestion) in suggestions.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("•").weak());
                ui.add(
                    egui::Label::new(RichText::new(suggestion).small())
                        .wrap()
                        .halign(egui::Align::Min),
                );
                if ui.button("Сохранить").clicked() {
                    accept_index = Some(index);
                }
            });
        }

        if let Some(index) = accept_index {
            self.accept_context_note_suggestion(index);
        }
    }

    fn show_context_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            return;
        };

        ui.add_space(6.0);
        egui::CollapsingHeader::new("Инспектор контекста")
            .id_salt("context_inspector")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Показывает компактную память, которую агент получит вместе со следующим запросом.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(6.0);
                let mut budget_changed = false;
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Экспорт профиля").clicked() {
                        self.export_active_context_profile();
                    }
                    if ui.button("Импорт профиля").clicked() {
                        self.import_context_profile_for_active_chat();
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Пресет").weak().small());
                    if ui.button("Короткий").clicked() {
                        self.apply_context_preset("короткий", 8, 4, 2);
                    }
                    if ui.button("Баланс").clicked() {
                        self.apply_context_preset("баланс", 14, 8, 5);
                    }
                    if ui.button("Глубокий").clicked() {
                        self.apply_context_preset("глубокий", 32, 16, 10);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Бюджет").weak().small());
                    budget_changed |= ui
                        .add(
                            egui::Slider::new(&mut self.config.context_recent_messages, 0..=80)
                                .text("последние сообщения"),
                        )
                        .changed();
                    budget_changed |= ui
                        .add(
                            egui::Slider::new(&mut self.config.context_relevant_messages, 0..=40)
                                .text("релевантные"),
                        )
                        .changed();
                    budget_changed |= ui
                        .add(
                            egui::Slider::new(&mut self.config.context_recent_runs, 0..=20)
                                .text("запуски"),
                        )
                        .changed();
                });
                if budget_changed {
                    let _ = self.config.save();
                }
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(RichText::new("Закреплённые заметки").strong().small());
                ui.label(
                    RichText::new(
                        "Эти факты всегда попадают в контекст именно этого чата, независимо от retrieval.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.context_note_input)
                            .desired_width(safe_available_width(ui, 220.0))
                            .hint_text("Например: архитектурное решение, ограничение, цель"),
                    );
                    let enter_pressed = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui.button("Добавить").clicked() || enter_pressed {
                        self.add_context_note_from_input();
                    }
                });
                let notes = self.context_notes.clone();
                for (index, note) in notes.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("•").weak());
                        ui.add(
                            egui::Label::new(RichText::new(note).small())
                                .wrap()
                                .halign(egui::Align::Min),
                        );
                        if ui.button("Убрать").clicked() {
                            self.remove_context_note(index);
                        }
                    });
                }
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Запрос").weak().small());
                    ui.add(
                        TextEdit::singleline(&mut self.context_inspector_query)
                            .desired_width(safe_available_width(ui, 240.0))
                            .hint_text("Пусто = текущий текст в поле ввода"),
                    );
                });
                let query = if self.context_inspector_query.trim().is_empty() {
                    self.input.as_str()
                } else {
                    self.context_inspector_query.as_str()
                };
                let snapshot = compile_context_snapshot_with_budget(
                    &workspace,
                    &conversation_id,
                    &self.chat,
                    query,
                    self.context_budget(),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "summary: {} символов",
                            snapshot.rolling_summary.chars().count()
                        ))
                        .weak()
                        .small(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "последние: {}",
                            snapshot.recent_messages.len()
                        ))
                        .weak()
                        .small(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "релевантные: {}",
                            snapshot.relevant_messages.len()
                        ))
                        .weak()
                        .small(),
                    );
                    ui.label(
                        RichText::new(format!("запуски: {}", snapshot.recent_runs.len()))
                            .weak()
                            .small(),
                    );
                    ui.label(
                        RichText::new(format!("заметки: {}", snapshot.pinned_notes.len()))
                            .weak()
                            .small(),
                    );
                });
                ui.add_space(6.0);
                let mut prompt_block = snapshot.to_prompt_block();
                egui::ScrollArea::vertical()
                    .id_salt("context_inspector_scroll")
                    .max_height(260.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut prompt_block)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(safe_available_width(ui, 320.0))
                                .desired_rows(12)
                                .interactive(false),
                        );
                    });
            });
    }

    fn show_archived_conversations(&mut self, ui: &mut egui::Ui) {
        let archived = self
            .conversation_index
            .conversations
            .iter()
            .filter(|meta| meta.archived)
            .map(|meta| {
                (
                    meta.id.clone(),
                    meta.title.clone(),
                    meta.message_count,
                    meta.updated_at,
                )
            })
            .collect::<Vec<_>>();

        if archived.is_empty() {
            return;
        }

        ui.add_space(6.0);
        egui::CollapsingHeader::new(format!("Архив чатов ({})", archived.len()))
            .id_salt("archived_conversations")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Архивные диалоги не попадают в основной список, но их можно вернуть.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(4.0);
                for (id, title, message_count, updated_at) in archived {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(title).strong().small());
                        ui.label(
                            RichText::new(format!("{message_count} сообщений · {}", updated_at))
                                .weak()
                                .small(),
                        );
                        if ui.button("Восстановить").clicked() {
                            self.restore_conversation_from_archive(id.clone());
                        }
                        if ui.button("Удалить").clicked() {
                            self.delete_conversation_by_id(id.clone());
                        }
                    });
                    ui.separator();
                }
            });
    }

    fn show_pending_run_gate_message(&mut self, ui: &mut egui::Ui) {
        let Some(gate) = self.pending_run_gate.clone() else {
            return;
        };
        let width = safe_available_width(ui, 1.0);
        let content_width = (width - 24.0).max(1.0);

        egui::Frame::none()
            .fill(surface_bg())
            .stroke(egui::Stroke::new(1.0, subtle_accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                ui.set_max_width(content_width);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("План перед запуском")
                            .strong()
                            .small()
                            .color(egui::Color32::from_rgb(236, 214, 151)),
                    );
                });
                ui.add_space(6.0);
                full_width_wrapped_label(ui, RichText::new(&gate.summary).strong());
                ui.add_space(6.0);
                full_width_wrapped_label(ui, RichText::new(&gate.detail).small());
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(ApprovalQuickAction::Approve.label())
                                    .strong()
                                    .color(egui::Color32::from_rgb(5, 12, 16)),
                            )
                            .fill(accent_color()),
                        )
                        .clicked()
                    {
                        self.answer_run_gate_with_action(ApprovalQuickAction::Approve);
                    }
                    if ui.button(ApprovalQuickAction::Deny.label()).clicked() {
                        self.answer_run_gate_with_action(ApprovalQuickAction::Deny);
                    }
                });
                ui.add_space(4.0);
                full_width_wrapped_label(
                    ui,
                    RichText::new(
                        "Если план не подходит, напишите уточнение в поле ввода и отправьте его. Агент пересоберёт план без запуска.",
                    )
                    .weak()
                    .small(),
                );
            });
        ui.add_space(8.0);
    }

    fn show_pending_approval_message(&mut self, ui: &mut egui::Ui) {
        let Some(prompt) = self.pending_approval.clone() else {
            return;
        };
        let width = safe_available_width(ui, 1.0);
        let content_width = (width - 24.0).max(1.0);

        egui::Frame::none()
            .fill(surface_bg())
            .stroke(egui::Stroke::new(1.0, subtle_accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                ui.set_max_width(content_width);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Агент просит согласовать доступ")
                            .strong()
                            .small()
                            .color(egui::Color32::from_rgb(236, 214, 151)),
                    );
                });
                ui.add_space(6.0);
                full_width_wrapped_label(ui, RichText::new(&prompt.summary).strong());
                ui.add_space(6.0);
                ui.label(RichText::new("Обоснование / детали").weak().small());
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(15, 17, 21))
                    .stroke(egui::Stroke::new(1.0, border_color()))
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(format!("approval_detail_scroll_{}", prompt.id))
                            .max_height(220.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let detail_width = (content_width - 16.0).max(1.0);
                                ui.set_max_width(detail_width);
                                full_width_wrapped_label(
                                    ui,
                                    RichText::new(&prompt.detail)
                                        .text_style(egui::TextStyle::Monospace)
                                        .small(),
                                );
                            });
                    });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(ApprovalQuickAction::Approve.label())
                                    .strong()
                                    .color(egui::Color32::from_rgb(5, 12, 16)),
                            )
                            .fill(accent_color()),
                        )
                        .clicked()
                    {
                        self.answer_approval_with_action(ApprovalQuickAction::Approve);
                    }
                    if ui.button(ApprovalQuickAction::Revise.label()).clicked() {
                        self.answer_approval_with_action(ApprovalQuickAction::Revise);
                    }
                    if ui
                        .button(ApprovalQuickAction::AnalysisOnly.label())
                        .clicked()
                    {
                        self.answer_approval_with_action(ApprovalQuickAction::AnalysisOnly);
                    }
                    if ui.button(ApprovalQuickAction::Restrict.label()).clicked() {
                        self.answer_approval_with_action(ApprovalQuickAction::Restrict);
                    }
                    if ui.button(ApprovalQuickAction::Deny.label()).clicked() {
                        self.answer_approval_with_action(ApprovalQuickAction::Deny);
                    }
                });
            });
        ui.add_space(8.0);
    }

    fn show_file_preview_tab(&mut self, ui: &mut egui::Ui, path: &str) {
        let Some(index) = self.file_tabs.iter().position(|tab| tab.path == path) else {
            self.active_center_tab = CenterTab::Agent;
            return;
        };
        self.sync_selected_from_file_tab(index);

        let dirty = self.file_tabs[index].editable
            && self.file_tabs[index].content != self.file_tabs[index].original_content;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&self.file_tabs[index].path).strong());
            if dirty {
                ui.label(RichText::new("изменён").italics().weak());
            } else {
                ui.label(RichText::new(&self.file_tabs[index].status).weak());
            }
        });

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.file_tabs[index].editable && dirty,
                    egui::Button::new("Сохранить"),
                )
                .clicked()
            {
                self.save_selected_file();
            }
            if ui
                .add_enabled(
                    self.file_tabs[index].editable && dirty,
                    egui::Button::new("Отменить"),
                )
                .clicked()
            {
                self.revert_selected_file();
            }
            if ui.button("Перезагрузить").clicked() {
                self.reload_selected_file();
            }
        });

        ui.add_space(8.0);
        let editable = self.file_tabs[index].editable;
        egui::ScrollArea::both()
            .id_salt(format!("file_tab_scroll_{path}"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut self.file_tabs[index].content)
                        .desired_width(safe_available_width(ui, 320.0))
                        .horizontal_align(egui::Align::Min)
                        .font(egui::TextStyle::Monospace)
                        .interactive(editable),
                );
            });
    }

    fn show_main_workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match self.workspace_mode {
            WorkspaceMode::Chat => self.show_center_tabs(ui),
            WorkspaceMode::Code => self.show_code_workspace(ui),
            WorkspaceMode::Assets => self.show_asset_studio_center(ui, ctx),
            WorkspaceMode::Project => self.show_project_command_center(ui),
        }
    }

    fn show_code_workspace(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Код").strong().size(22.0));
            if self.file_tabs.is_empty() {
                ui.label(
                    RichText::new("выберите файл слева или попросите агента внести правку").weak(),
                );
            } else {
                ui.label(
                    RichText::new(format!("открыто вкладок: {}", self.file_tabs.len())).weak(),
                );
            }
        });
        ui.add_space(6.0);
        self.show_center_tabs(ui);
    }

    fn show_project_task_tree(&mut self, ui: &mut egui::Ui) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let memory = load_memory(&workspace);
        let tasks = memory.tasks.clone();
        let open_tasks = tasks.iter().filter(|task| task.status != "done").count();
        let done_tasks = tasks.iter().filter(|task| task.status == "done").count();
        let mut grouped: BTreeMap<String, BTreeMap<String, Vec<ProjectTask>>> = BTreeMap::new();
        for task in tasks {
            let workstream = task_tree_value(&task.workstream, "Разработка");
            let milestone = task_tree_value(&task.milestone, "Текущий этап");
            grouped
                .entry(workstream)
                .or_default()
                .entry(milestone)
                .or_default()
                .push(task);
        }

        ui.separator();
        panel_header(
            ui,
            "Дерево задач",
            "Рабочая карта проекта по направлениям, этапам и приоритетам.",
        );
        ui.horizontal_wrapped(|ui| {
            metric_chip(ui, "направления", grouped.len());
            metric_chip(ui, "открытые", open_tasks);
            metric_chip(ui, "готовые", done_tasks);
        });
        ui.add_space(6.0);
        self.show_project_task_quick_add(ui, &workspace);

        if grouped.is_empty() {
            empty_state(
                ui,
                "Задач пока нет",
                "Добавьте первую задачу вручную или попросите агента разложить работу по направлениям и этапам.",
            );
            return;
        }

        let mut status_update: Option<(String, String)> = None;
        for (workstream, milestones) in grouped.iter_mut() {
            let workstream_count = milestones.values().map(Vec::len).sum::<usize>();
            let title = format!("{workstream} · {workstream_count}");
            egui::CollapsingHeader::new(RichText::new(title).strong())
                .default_open(true)
                .show(ui, |ui| {
                    for (milestone, tasks) in milestones.iter_mut() {
                        tasks.sort_by(|left, right| {
                            task_status_sort_key(&left.status)
                                .cmp(&task_status_sort_key(&right.status))
                                .then(
                                    task_priority_sort_key(&left.priority)
                                        .cmp(&task_priority_sort_key(&right.priority)),
                                )
                                .then(left.title.cmp(&right.title))
                        });
                        let milestone_title = format!("{milestone} · {}", tasks.len());
                        egui::CollapsingHeader::new(RichText::new(milestone_title))
                            .default_open(true)
                            .show(ui, |ui| {
                                for task in tasks.iter() {
                                    ui.horizontal_wrapped(|ui| {
                                        chip(ui, task_status_label(&task.status));
                                        chip(ui, task_priority_label(&task.priority));
                                        ui.label(RichText::new(&task.title).strong());
                                        if !task.notes.trim().is_empty() {
                                            ui.label(
                                                RichText::new(compact_inline(&task.notes, 100))
                                                    .weak(),
                                            );
                                        }
                                        if task.status != "doing"
                                            && ui.small_button("В работу").clicked()
                                        {
                                            status_update =
                                                Some((task.id.clone(), "doing".to_string()));
                                        }
                                        if task.status != "done"
                                            && ui.small_button("Готово").clicked()
                                        {
                                            status_update =
                                                Some((task.id.clone(), "done".to_string()));
                                        }
                                        if task.status != "blocked"
                                            && ui.small_button("Блокер").clicked()
                                        {
                                            status_update =
                                                Some((task.id.clone(), "blocked".to_string()));
                                        }
                                    });
                                }
                            });
                    }
                });
        }
        if let Some((id, status)) = status_update {
            let result = update_task_status(
                &workspace,
                UpdateTaskStatusArgs {
                    id,
                    status,
                    notes: None,
                },
            );
            self.project_status = result.output;
        }
    }

    fn show_project_task_quick_add(&mut self, ui: &mut egui::Ui, workspace: &Workspace) {
        ui.horizontal_wrapped(|ui| {
            ui.add_sized(
                [240.0, 26.0],
                TextEdit::singleline(&mut self.project_task_title_input).hint_text("задача"),
            );
            ui.add_sized(
                [140.0, 26.0],
                TextEdit::singleline(&mut self.project_task_workstream_input)
                    .hint_text("направление"),
            );
            ui.add_sized(
                [140.0, 26.0],
                TextEdit::singleline(&mut self.project_task_milestone_input).hint_text("этап"),
            );
            egui::ComboBox::from_id_salt("project_task_priority")
                .selected_text(task_priority_label(&self.project_task_priority_input))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.project_task_priority_input,
                        "high".to_string(),
                        "Высокий",
                    );
                    ui.selectable_value(
                        &mut self.project_task_priority_input,
                        "normal".to_string(),
                        "Обычный",
                    );
                    ui.selectable_value(
                        &mut self.project_task_priority_input,
                        "low".to_string(),
                        "Низкий",
                    );
                });
            if ui.button("Добавить").clicked() {
                let title = self.project_task_title_input.trim().to_string();
                if !title.is_empty() {
                    let workstream = self.project_task_workstream_input.trim().to_string();
                    let milestone = self.project_task_milestone_input.trim().to_string();
                    let result = upsert_task(
                        workspace,
                        UpsertTaskArgs {
                            id: None,
                            title,
                            status: Some("todo".to_string()),
                            notes: None,
                            workstream: if workstream.is_empty() {
                                None
                            } else {
                                Some(workstream)
                            },
                            milestone: if milestone.is_empty() {
                                None
                            } else {
                                Some(milestone)
                            },
                            priority: Some(self.project_task_priority_input.clone()),
                        },
                    );
                    self.project_status = result.output;
                    self.project_task_title_input.clear();
                }
            }
        });
    }

    fn show_project_command_center(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Project Command Center").strong().size(22.0));
            if self.project_is_running {
                ui.spinner();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Обновить"))
                .clicked()
            {
                self.refresh_project_profiles();
                self.refresh_git_summary();
            }
            if ui
                .add_enabled(self.project_is_running, egui::Button::new("Стоп"))
                .clicked()
            {
                self.stop_project_command();
            }
        });
        if let Some(workspace) = &self.workspace {
            ui.label(RichText::new(format!("Проект: {}", workspace.display_name())).weak());
        } else {
            empty_state(
                ui,
                "Проект не выбран",
                "Нажмите «Проект» сверху и выберите рабочую папку.",
            );
            return;
        }
        if !self.project_status.is_empty() {
            ui.label(RichText::new(&self.project_status).weak());
        }
        let has_preview = self
            .project_profiles
            .iter()
            .any(|profile| !profile.previews.is_empty());
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.last_project_command.is_some() && !self.project_is_running,
                    egui::Button::new("Перезапустить"),
                )
                .clicked()
            {
                if let Some(command) = self.last_project_command.clone() {
                    self.start_project_command(command);
                }
            }
            if ui
                .add_enabled(
                    self.latest_failed_or_last_project_run().is_some(),
                    egui::Button::new("Попросить агента исправить"),
                )
                .clicked()
            {
                if let Some(run) = self.latest_failed_or_last_project_run() {
                    self.prepare_fix_prompt_from_run(&run);
                }
            }
            if ui
                .add_enabled(
                    has_preview && !self.project_is_running,
                    egui::Button::new("Открыть preview"),
                )
                .clicked()
            {
                self.open_first_project_preview();
            }
            if ui.button("Обновить Git").clicked() {
                self.refresh_git_summary();
            }
        });
        ui.add_space(8.0);
        self.show_project_task_tree(ui);
        ui.add_space(10.0);

        if !self.project_profiles.is_empty() {
            let profiles = self.project_profiles.clone();
            for profile in profiles {
                let profile_commands = profile.commands.clone();
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&profile.kind).strong());
                    ui.label(RichText::new(&profile.name).weak());
                    if !profile.markers.is_empty() {
                        ui.label(RichText::new(profile.markers.join(", ")).weak().small());
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for command in profile.commands {
                        let response = ui
                            .add_enabled(
                                !self.project_is_running,
                                egui::Button::new(&command.label),
                            )
                            .on_hover_text(format!("{}\n{}", command.description, command.command));
                        if response.clicked() {
                            self.start_project_command(command);
                        }
                    }
                });
                if !profile.previews.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Preview").weak());
                        for hook in profile.previews {
                            let response = ui
                                .add_enabled(
                                    !self.project_is_running,
                                    egui::Button::new(&hook.label),
                                )
                                .on_hover_text(&hook.description);
                            if response.clicked() {
                                if let Some(url) = hook.url.as_deref() {
                                    self.open_preview_url_from_ui(url);
                                } else if let Some(command_id) = hook.command_id.as_deref() {
                                    if let Some(command) = profile_commands
                                        .iter()
                                        .find(|command| command.id == command_id)
                                        .cloned()
                                    {
                                        self.start_project_command(command);
                                    }
                                }
                            }
                        }
                    });
                }
            }
        } else {
            empty_state(
                ui,
                "Профили не обнаружены",
                "Можно работать через чат и файловое дерево, но быстрые команды пока не распознаны.",
            );
            self.show_game_workflow_buttons(ui);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new("История запусков").strong());
        let runs = self
            .project_runs
            .iter()
            .rev()
            .take(10)
            .cloned()
            .collect::<Vec<_>>();
        if runs.is_empty() {
            ui.label(RichText::new("История команд пока пуста").weak());
        }
        for run in runs {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&run.label).strong());
                chip(ui, run.status.label());
                if let Some(code) = run.exit_code {
                    chip(ui, format!("exit {code}"));
                }
                ui.label(RichText::new(format!("старт: {}", run.started_at)).weak());
                if let Some(finished_at) = run.finished_at {
                    ui.label(RichText::new(format!("финиш: {finished_at}")).weak());
                }
                if ui
                    .add_enabled(!self.project_is_running, egui::Button::new("Перезапустить"))
                    .clicked()
                {
                    self.start_project_command(run.command.clone());
                }
                if ui
                    .add_enabled(
                        run.status == ProjectRunStatus::Failed,
                        egui::Button::new("Исправить"),
                    )
                    .clicked()
                {
                    self.prepare_fix_prompt_from_run(&run);
                }
                if ui
                    .add_enabled(
                        has_preview && !self.project_is_running,
                        egui::Button::new("Preview"),
                    )
                    .clicked()
                {
                    self.open_first_project_preview();
                }
            });
            ui.label(RichText::new(&run.shell_command).weak().monospace());
            if !run.diagnostics.is_empty() {
                ui.label(RichText::new("Диагностика").strong());
                let mut open_diagnostic_file = None;
                let mut fix_diagnostic = None;
                for diagnostic in run.diagnostics.iter().take(8) {
                    ui.horizontal_wrapped(|ui| {
                        chip(ui, diagnostic.kind.label());
                        if let Some(location) = diagnostic_location(diagnostic) {
                            ui.label(RichText::new(location).monospace().strong());
                        }
                        ui.label(
                            RichText::new(compact_inline(&diagnostic.message, 160))
                                .color(egui::Color32::from_rgb(235, 154, 154)),
                        )
                        .on_hover_text(&diagnostic.raw);
                        if let Some(file) = &diagnostic.file {
                            if ui.button("Открыть").clicked() {
                                open_diagnostic_file = Some(file.clone());
                            }
                        }
                        if ui
                            .add_enabled(
                                run.status == ProjectRunStatus::Failed,
                                egui::Button::new("Исправить"),
                            )
                            .clicked()
                        {
                            fix_diagnostic = Some(diagnostic.clone());
                        }
                    });
                }
                if let Some(file) = open_diagnostic_file {
                    self.load_file_preview(&file);
                }
                if let Some(diagnostic) = fix_diagnostic {
                    self.prepare_fix_prompt_from_diagnostic(&run, &diagnostic);
                }
            }
            if !run.error_summary.is_empty() {
                ui.label(RichText::new("Найденные ошибки").strong());
                for item in run.error_summary.iter().take(6) {
                    ui.label(RichText::new(item).color(egui::Color32::from_rgb(235, 154, 154)));
                }
            }
            if run.output_tail.trim().is_empty() {
                ui.label(RichText::new("вывод пока пуст").weak());
            } else {
                let mut output = compact(&run.output_tail, 2_400);
                ui.add(
                    TextEdit::multiline(&mut output)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(safe_available_width(ui, 160.0))
                        .horizontal_align(egui::Align::Min)
                        .desired_rows(5)
                        .interactive(false),
                );
            }
        }

        if !self.project_fix_requests.is_empty() {
            ui.add_space(8.0);
            ui.separator();
            ui.label(RichText::new("История исправлений").strong());
            for request in self.project_fix_requests.iter().rev().take(8) {
                ui.horizontal_wrapped(|ui| {
                    chip(ui, format!("#{}", compact_inline(&request.id, 10)));
                    chip(ui, format!("run {}", compact_inline(&request.run_id, 8)));
                    ui.label(RichText::new(&request.run_label).strong());
                    ui.label(RichText::new(compact_inline(&request.target, 140)).weak());
                    ui.label(RichText::new(format!("{}", request.requested_at)).weak());
                });
            }
        }
    }

    fn show_asset_studio_center(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Asset Studio").strong().size(22.0));
            if self.asset_is_running {
                ui.spinner();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Обновить"))
                .clicked()
            {
                if let Some(workspace) = &self.workspace {
                    self.asset_jobs = load_jobs(workspace);
                }
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Открыть папку"))
                .clicked()
            {
                self.open_generated_assets_folder();
            }
        });
        ui.label(
            RichText::new("Генерация, история, варианты, избранное и экспорт ассетов.").weak(),
        );
        ui.add_space(8.0);
        self.show_asset_panel(ui, ctx);
        ui.separator();
        self.show_asset_library_panel(ui);
    }

    fn show_chat_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(central_frame())
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    self.show_main_workspace(ui, ctx);
                });
            });
    }
    fn show_permission_mode_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Доступ").weak().small());
            for (id, label) in policy_profile_labels() {
                if *id == "custom" {
                    continue;
                }
                let selected = self.config.policy_profile == *id;
                let response = ui
                    .selectable_label(selected, RichText::new(*label).small())
                    .on_hover_text(permission_mode_description(id));
                if response.clicked() && !selected {
                    self.config.set_policy_profile(id);
                    self.save_settings_from_ui();
                }
            }
            ui.label(
                RichText::new(compact_inline(
                    permission_mode_description(&self.config.policy_profile),
                    92,
                ))
                .weak()
                .small(),
            )
            .on_hover_text(permission_mode_description(&self.config.policy_profile));
        });
    }

    fn show_input_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("input_bar")
            .exact_height(150.0)
            .frame(input_bar_frame())
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.add_space(7.0);
                    self.show_permission_mode_controls(ui);
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let attach_width = 42.0;
                        let send_width = 108.0;
                        let input_height = 54.0;
                        let input_width =
                            (safe_available_width(ui, 260.0) - attach_width - send_width - 18.0)
                                .max(80.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(attach_width, input_height),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                ui.menu_button(RichText::new("+").strong().size(21.0), |ui| {
                                    ui.set_min_width(220.0);
                                    if ui.button("Добавить файл").clicked() {
                                        self.choose_input_files(InputAttachmentKind::File);
                                        ui.close_menu();
                                    }
                                    if ui.button("Добавить изображение").clicked()
                                    {
                                        self.choose_input_images();
                                        ui.close_menu();
                                    }
                                    if ui.button("Добавить папку проекта").clicked()
                                    {
                                        self.choose_input_folder_context();
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button("Вставить скриншот из буфера").clicked()
                                    {
                                        self.attach_clipboard_image(true);
                                        ui.close_menu();
                                    }
                                    if ui.button("Сделать снимок экрана").clicked()
                                    {
                                        self.capture_input_screenshot();
                                        ui.close_menu();
                                    }
                                })
                                .response
                                .on_hover_text("Добавить файл, изображение или снимок в запрос");
                            },
                        );
                        let response = ui.add_sized(
                            [input_width, input_height],
                            TextEdit::multiline(&mut self.input)
                                .id_salt("main_prompt_input")
                                .horizontal_align(egui::Align::Min)
                                .hint_text("Что сделать? Ctrl+Enter — отправить; + — вложения")
                                .desired_width(input_width),
                        );

                        let send_clicked = ui
                            .add_sized(
                                [send_width, input_height],
                                egui::Button::new(
                                    RichText::new(if self.is_running {
                                        "Выполняется"
                                    } else {
                                        "Отправить"
                                    })
                                    .strong()
                                    .color(
                                        if self.is_running {
                                            muted_color()
                                        } else {
                                            egui::Color32::from_rgb(5, 12, 16)
                                        },
                                    ),
                                )
                                .fill(if self.is_running {
                                    panel_bg()
                                } else {
                                    accent_color()
                                }),
                            )
                            .clicked()
                            && !self.is_running;
                        let input_has_focus = response.has_focus();
                        let dropped_files = if response.hovered() || input_has_focus {
                            ctx.input(|input| input.raw.dropped_files.clone())
                        } else {
                            Vec::new()
                        };
                        if !dropped_files.is_empty() {
                            self.handle_input_dropped_files(dropped_files);
                        }

                        let paste_image_pressed =
                            input_has_focus && self.input_paste_shortcut_pressed(ui);
                        if paste_image_pressed {
                            self.attach_clipboard_image(false);
                        }

                        let screenshot_pressed = input_has_focus
                            && ui.input(|input| {
                                input.key_pressed(egui::Key::S)
                                    && input.modifiers.ctrl
                                    && input.modifiers.shift
                            });
                        if screenshot_pressed {
                            self.capture_input_screenshot();
                        }

                        let enter_pressed = input_has_focus
                            && ui.input(|input| {
                                input.key_pressed(egui::Key::Enter) && input.modifiers.ctrl
                            });

                        if (send_clicked || enter_pressed) && !self.is_running {
                            self.send_current_input();
                        }
                    });

                    if !self.input_attachments.is_empty() {
                        ui.add_space(5.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Вложения").weak().small());
                            let mut remove_index = None;
                            for (index, attachment) in self.input_attachments.iter().enumerate() {
                                let label = format!(
                                    "{} {} ({})",
                                    input_attachment_kind_label(attachment.kind),
                                    attachment.name,
                                    format_bytes_short(attachment.bytes)
                                );
                                if ui
                                    .small_button(label)
                                    .on_hover_text(&attachment.path)
                                    .clicked()
                                {
                                    remove_index = Some(index);
                                }
                            }
                            if let Some(index) = remove_index {
                                self.input_attachments.remove(index);
                                self.input_attachment_status =
                                    "вложение удалено из сообщения".to_string();
                            }
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        if !self.input_attachment_status.is_empty() {
                            ui.label(RichText::new(&self.input_attachment_status).weak().small());
                        } else {
                            ui.label(
                                RichText::new("Перетащите файлы в поле ввода или используйте +.")
                                    .weak()
                                    .small(),
                            );
                        }
                    });
                });
            });
    }
}

impl eframe::App for LeetcodeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.drain_project_events();
        self.drain_asset_events();
        self.drain_provider_validation_events();
        self.refresh_terminal_snapshot();
        self.handle_command_palette_shortcuts(ctx);
        self.show_menu_bar(ctx);
        self.show_top_bar(ctx);
        self.show_file_panel(ctx);
        self.show_tool_panel(ctx);
        // Bottom input belongs to the central workspace, so it must be created
        // after the side panels reserve their widths. Otherwise it spans the
        // full window and feels like it sits below/under the tool panels.
        self.show_input_bar(ctx);
        self.show_chat_panel(ctx);
        self.show_git_commit_dialog(ctx);
        self.show_command_palette(ctx);
        self.show_command_macro_confirmation(ctx);

        if self.is_running
            || self.project_is_running
            || self.asset_is_running
            || self.provider_validation_running
            || self.terminal_running
            || self.pending_command_macro_run.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn apply_app_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(9.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size = egui::vec2(44.0, 32.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(23.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(15.0, egui::FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = app_bg();
    visuals.window_fill = panel_bg();
    visuals.extreme_bg_color = egui::Color32::from_rgb(8, 10, 13);
    visuals.faint_bg_color = surface_bg();
    visuals.hyperlink_color = accent_color();
    visuals.selection.bg_fill = egui::Color32::from_rgb(34, 120, 152);
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(160, 230, 245));
    visuals.widgets.noninteractive.bg_fill = surface_bg();
    visuals.widgets.noninteractive.weak_bg_fill = surface_alt_bg();
    visuals.widgets.inactive.bg_fill = surface_bg();
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(31, 34, 41);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(39, 45, 55);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(43, 63, 77);
    visuals.widgets.open.bg_fill = surface_alt_bg();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border_color());
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border_color());
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, subtle_accent());
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent_color());
    visuals.window_stroke = egui::Stroke::new(1.0, border_color());
    visuals.override_text_color = Some(text_color());
    style.visuals = visuals;

    ctx.set_style(style);
}

fn app_bg() -> egui::Color32 {
    egui::Color32::from_rgb(12, 14, 18)
}

fn panel_bg() -> egui::Color32 {
    egui::Color32::from_rgb(16, 19, 24)
}

fn surface_bg() -> egui::Color32 {
    egui::Color32::from_rgb(21, 25, 31)
}

fn surface_alt_bg() -> egui::Color32 {
    egui::Color32::from_rgb(27, 32, 39)
}

fn text_color() -> egui::Color32 {
    egui::Color32::from_rgb(226, 231, 238)
}

fn muted_color() -> egui::Color32 {
    egui::Color32::from_rgb(146, 156, 170)
}

fn accent_color() -> egui::Color32 {
    egui::Color32::from_rgb(75, 184, 217)
}

fn subtle_accent() -> egui::Color32 {
    egui::Color32::from_rgb(66, 104, 122)
}

fn border_color() -> egui::Color32 {
    egui::Color32::from_rgb(38, 45, 54)
}

fn top_bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(15, 18, 23))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 36, 44)))
        .inner_margin(egui::Margin::symmetric(12.0, 5.0))
}

fn menu_bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(9, 11, 14))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(26, 31, 38)))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
}

fn side_panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(panel_bg())
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(31, 37, 45)))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
}

fn central_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(app_bg())
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
}

fn input_bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(17, 20, 25))
        .stroke(egui::Stroke::new(1.0, border_color()))
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(12.0, 7.0))
}

fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(surface_bg())
        .stroke(egui::Stroke::new(1.0, border_color()))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
}

fn flat_section(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let width = safe_available_width(ui, 1.0);
    ui.vertical(|ui| {
        ui.set_min_width(width);
        add_contents(ui);
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

fn flat_collapsing_section(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let width = safe_available_width(ui, 1.0);
    ui.vertical(|ui| {
        ui.set_min_width(width);
        egui::CollapsingHeader::new(RichText::new(title).strong())
            .default_open(default_open)
            .show(ui, |ui| {
                ui.add_space(4.0);
                add_contents(ui);
            });
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

fn panel_switcher(ui: &mut egui::Ui, selected: &mut RightPanelView, views: &[RightPanelView]) {
    let available = safe_available_width(ui, 120.0);
    let columns = if views.len() <= 3 || available >= 460.0 {
        views.len().max(1)
    } else if available >= 260.0 {
        2
    } else {
        1
    };
    let gap = 6.0;
    let button_width =
        ((available - gap * (columns.saturating_sub(1) as f32)) / columns.max(1) as f32).max(92.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.spacing_mut().item_spacing.y = 6.0;
        for view in views.iter().copied() {
            let is_selected = *selected == view;
            let text_color = if is_selected {
                text_color()
            } else {
                muted_color()
            };
            let fill = if is_selected {
                egui::Color32::from_rgb(33, 96, 118)
            } else {
                egui::Color32::TRANSPARENT
            };
            let stroke = if is_selected {
                egui::Stroke::new(1.0, egui::Color32::from_rgb(74, 174, 205))
            } else {
                egui::Stroke::new(1.0, egui::Color32::TRANSPARENT)
            };
            let response = ui.add_sized(
                [button_width, 30.0],
                egui::Button::new(
                    RichText::new(view.label())
                        .small()
                        .strong()
                        .color(text_color),
                )
                .fill(fill)
                .stroke(stroke)
                .rounding(egui::Rounding::same(6.0)),
            );
            if response.clicked() {
                *selected = view;
            }
            response.on_hover_text(view.tooltip());
        }
    });
    ui.add_space(8.0);
    ui.separator();
}

fn context_panel_switcher(ui: &mut egui::Ui, selected: &mut ContextPanelTab) {
    let available = safe_available_width(ui, 120.0);
    let columns: usize = if available >= 420.0 { 4 } else { 2 };
    let gap = 6.0;
    let button_width =
        ((available - gap * (columns.saturating_sub(1) as f32)) / columns.max(1) as f32).max(92.0);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.spacing_mut().item_spacing.y = 6.0;
        for tab in ContextPanelTab::ALL {
            let is_selected = *selected == tab;
            let response = ui.add_sized(
                [button_width, 30.0],
                egui::Button::new(RichText::new(tab.label()).small().strong().color(
                    if is_selected {
                        text_color()
                    } else {
                        muted_color()
                    },
                ))
                .fill(if is_selected {
                    egui::Color32::from_rgb(33, 96, 118)
                } else {
                    egui::Color32::TRANSPARENT
                })
                .stroke(if is_selected {
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(74, 174, 205))
                } else {
                    egui::Stroke::new(1.0, egui::Color32::TRANSPARENT)
                })
                .rounding(egui::Rounding::same(6.0)),
            );
            if response.clicked() {
                *selected = tab;
            }
            response.on_hover_text(tab.tooltip());
        }
    });

    ui.add_space(6.0);
    ui.add(egui::Label::new(RichText::new(selected.subtitle()).weak().small()).wrap());
    ui.add_space(8.0);
    ui.separator();
}

fn status_line(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(label).weak().small())
            .on_hover_text(status_line_tooltip(label, value));
        ui.add_space(6.0);
        let shown = compact_inline(value, 72);
        let response = ui.label(RichText::new(&shown).small());
        response.on_hover_text(status_line_tooltip(label, value));
    });
}

fn status_line_tooltip(label: &str, value: &str) -> String {
    match label {
        "сообщений" => format!("Количество сообщений в этом чате: {value}."),
        "summary" => format!("Размер сжатого summary старой переписки: {value} символов."),
        "последние" => {
            format!("Свежие сообщения, выбранные для следующего prompt: {value}.")
        }
        "релевантные" => {
            format!("Старые сообщения, найденные как релевантные: {value}.")
        }
        "runs" => format!("Сохранённые прошлые запуски, выбранные в контекст: {value}."),
        "новых" => {
            format!("Новые заметки из профиля, которых ещё нет в текущем чате: {value}.")
        }
        "дубликатов" => {
            format!("Заметки из профиля, которые уже есть в текущем чате: {value}.")
        }
        "бюджет" => {
            format!("Разница бюджета профиля относительно текущих настроек: {value}.")
        }
        _ => format!("{label}: {value}"),
    }
}

fn roadmap_metric(ui: &mut egui::Ui, value: impl std::fmt::Display, label: &str) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(value.to_string())
                .strong()
                .size(22.0)
                .color(egui::Color32::from_rgb(235, 244, 250)),
        );
        ui.label(RichText::new(label).weak().small());
    });
}

fn roadmap_goal(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(egui::Color32::from_rgb(105, 201, 143), "●");
        ui.add(egui::Label::new(RichText::new(text).small()).wrap());
    });
}

fn release_checklist(
    workspace: Option<&Workspace>,
    profiles: &[ProjectProfile],
    git_summary: &str,
    project_runs: &[ProjectRunRecord],
    diagnostics: &EnvironmentDiagnostics,
    package_script_present: bool,
    artifacts_present: bool,
) -> Vec<ReleaseChecklistItem> {
    let toolchain_ok = diagnostics
        .tools
        .iter()
        .filter(|item| matches!(item.name.as_str(), "cargo" | "rustup" | "git"))
        .all(|item| item.status == "ok");
    let release_policy_ok = diagnostics
        .release_notes
        .iter()
        .all(|item| item.status == "ok");
    let check_run = latest_project_run_for_ids(project_runs, &["check", "typecheck", "lint"]);
    let test_run = latest_project_run_for_ids(project_runs, &["test"]);
    let release_run = latest_project_run_for_ids(project_runs, &["release", "build"]);
    let package_run = latest_project_run_for_ids(project_runs, &["package"]);
    let has_release_command = find_command_by_ids(profiles, &["release", "build"]).is_some();
    let has_test_command = find_command_by_ids(profiles, &["test"]).is_some();
    let has_check_command =
        find_command_by_ids(profiles, &["check", "typecheck", "lint"]).is_some();

    vec![
        ReleaseChecklistItem {
            title: "Проект выбран".to_string(),
            detail: workspace
                .map(|workspace| workspace.root().display().to_string())
                .unwrap_or_else(|| "рабочая папка не выбрана".to_string()),
            ok: workspace.is_some(),
        },
        ReleaseChecklistItem {
            title: "Профиль проекта".to_string(),
            detail: if profiles.is_empty() {
                "профили не распознаны".to_string()
            } else {
                profiles
                    .iter()
                    .map(|profile| format!("{} {}", profile.kind, profile.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            ok: !profiles.is_empty(),
        },
        ReleaseChecklistItem {
            title: "Git чистый".to_string(),
            detail: if git_summary_clean(git_summary) {
                "нет незакоммиченных изменений".to_string()
            } else if git_summary.trim().is_empty() {
                "git status пока не загружен".to_string()
            } else {
                compact_inline(git_summary, 180)
            },
            ok: git_summary_clean(git_summary),
        },
        ReleaseChecklistItem {
            title: "Toolchain доступен".to_string(),
            detail: diagnostics
                .tools
                .iter()
                .map(|item| format!("{}: {}", item.name, item.status))
                .collect::<Vec<_>>()
                .join(", "),
            ok: toolchain_ok,
        },
        ReleaseChecklistItem {
            title: "Проверка пройдена".to_string(),
            detail: release_run_detail(check_run, has_check_command, "check/typecheck/lint"),
            ok: latest_run_passed(check_run),
        },
        ReleaseChecklistItem {
            title: "Тесты пройдены".to_string(),
            detail: release_run_detail(test_run, has_test_command, "test"),
            ok: latest_run_passed(test_run),
        },
        ReleaseChecklistItem {
            title: "Release build".to_string(),
            detail: release_run_detail(release_run, has_release_command, "release/build"),
            ok: latest_run_passed(release_run),
        },
        ReleaseChecklistItem {
            title: "Packaging".to_string(),
            detail: if package_script_present {
                release_run_detail(package_run, true, "package")
            } else {
                "scripts/package-windows.ps1 не найден".to_string()
            },
            ok: package_script_present && latest_run_passed(package_run),
        },
        ReleaseChecklistItem {
            title: "Артефакты найдены".to_string(),
            detail: if artifacts_present {
                "dist/target содержат release-файлы".to_string()
            } else {
                "zip, sha256 или release exe пока не найдены".to_string()
            },
            ok: artifacts_present,
        },
        ReleaseChecklistItem {
            title: "Runtime policy".to_string(),
            detail: diagnostics
                .release_notes
                .iter()
                .map(|item| format!("{}: {}", item.name, item.status))
                .collect::<Vec<_>>()
                .join(", "),
            ok: release_policy_ok,
        },
    ]
}

fn release_check_row(ui: &mut egui::Ui, item: &ReleaseChecklistItem) {
    let color = if item.ok {
        egui::Color32::from_rgb(105, 201, 143)
    } else {
        egui::Color32::from_rgb(216, 178, 95)
    };
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(color, if item.ok { "●" } else { "○" });
        ui.vertical(|ui| {
            ui.label(RichText::new(&item.title).strong().small());
            ui.add(
                egui::Label::new(RichText::new(&item.detail).weak().small())
                    .wrap()
                    .halign(egui::Align::Min),
            );
        });
    });
    ui.add_space(5.0);
}

fn release_artifact_row(ui: &mut egui::Ui, artifact: &ReleaseArtifact) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&artifact.label).strong().small());
        chip(ui, file_size_label(artifact.size_bytes));
        if let Some(modified_at) = artifact.modified_at {
            chip(ui, age_label(modified_at));
        }
    });
    full_width_wrapped_label(
        ui,
        RichText::new(&artifact.path)
            .text_style(egui::TextStyle::Monospace)
            .weak()
            .small(),
    );
    ui.add_space(5.0);
}

fn release_diagnostic_row(ui: &mut egui::Ui, item: &DiagnosticItem) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&item.name).monospace().small());
        chip(ui, &item.status);
        ui.add(egui::Label::new(RichText::new(&item.detail).weak().small()).wrap());
    });
}

fn release_validation_summary(checklist: &[ReleaseChecklistItem], passed: usize) -> String {
    let failed = checklist
        .iter()
        .filter(|item| !item.ok)
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    if failed.is_empty() {
        format!("release preflight: {passed}/{} ok", checklist.len())
    } else {
        format!(
            "release preflight: {passed}/{} ok; требует внимания: {}",
            checklist.len(),
            failed.join(", ")
        )
    }
}

fn release_milestone_detail(
    readiness: f32,
    passed: usize,
    checklist: &[ReleaseChecklistItem],
    artifacts: &[ReleaseArtifact],
    project_runs: &[ProjectRunRecord],
    git_summary: &str,
) -> String {
    let checklist_lines = checklist
        .iter()
        .map(|item| {
            format!(
                "- [{}] {}: {}",
                if item.ok { "x" } else { " " },
                item.title,
                compact_inline(&item.detail, 160)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let artifact_lines = if artifacts.is_empty() {
        "- артефакты не найдены".to_string()
    } else {
        artifacts
            .iter()
            .map(|artifact| {
                format!(
                    "- {}: {} ({})",
                    artifact.label,
                    artifact.path,
                    file_size_label(artifact.size_bytes)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let run_lines = project_runs
        .iter()
        .rev()
        .filter(|run| release_command_kind(&run.command.id).is_some())
        .take(6)
        .map(|run| {
            format!(
                "- {}: {}{}",
                run.label,
                run.status.label(),
                run.exit_code
                    .map(|code| format!(" (exit {code})"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let run_lines = if run_lines.is_empty() {
        "- релизные команды ещё не запускались".to_string()
    } else {
        run_lines.join("\n")
    };
    format!(
        "Готовность релиза: {:.0}% ({passed}/{} пунктов).\n\nЧеклист:\n{checklist_lines}\n\nАртефакты:\n{artifact_lines}\n\nПоследние релизные запуски:\n{run_lines}\n\nGit:\n{}",
        readiness * 100.0,
        checklist.len(),
        compact_inline(git_summary, 500)
    )
}

fn release_artifacts(root: &Path) -> Vec<ReleaseArtifact> {
    let candidates = [
        (
            "portable zip",
            root.join("dist").join("leetcode-portable.zip"),
        ),
        (
            "sha256",
            root.join("dist").join("leetcode-portable.sha256.txt"),
        ),
        (
            "portable exe",
            root.join("dist")
                .join("leetcode-portable")
                .join("leetcode.exe"),
        ),
        (
            "release exe",
            root.join("target").join("release").join("leetcode.exe"),
        ),
    ];
    let mut artifacts = Vec::new();
    for (label, path) in candidates {
        if let Some(artifact) = release_artifact_from_path(root, label, path) {
            artifacts.push(artifact);
        }
    }
    artifacts.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    artifacts
}

fn release_artifact_from_path(
    root: &Path,
    label: impl Into<String>,
    path: PathBuf,
) -> Option<ReleaseArtifact> {
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    Some(ReleaseArtifact {
        label: label.into(),
        path: path
            .strip_prefix(root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| path.display().to_string()),
        size_bytes: metadata.len(),
        modified_at,
    })
}

fn release_version_label(root: &Path) -> Option<String> {
    if let Some((name, version)) = cargo_package_name_version(root) {
        return Some(format!("{name} v{version}"));
    }
    if let Some((name, version)) = package_json_name_version(root) {
        return Some(format!("{name} v{version}"));
    }
    None
}

fn cargo_package_name_version(root: &Path) -> Option<(String, String)> {
    let toml = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    let mut name = None;
    let mut version = None;
    for raw_line in toml.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"').to_string();
            match key.trim() {
                "name" => name = Some(value),
                "version" => version = Some(value),
                _ => {}
            }
        }
    }
    Some((name?, version?))
}

fn package_json_name_version(root: &Path) -> Option<(String, String)> {
    let package = fs::read_to_string(root.join("package.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&package).ok()?;
    let name = value.get("name")?.as_str()?.to_string();
    let version = value.get("version")?.as_str()?.to_string();
    Some((name, version))
}

fn find_command_by_ids(profiles: &[ProjectProfile], ids: &[&str]) -> Option<ProjectCommand> {
    ids.iter().find_map(|wanted| {
        profiles
            .iter()
            .flat_map(|profile| profile.commands.iter())
            .find(|command| command.id == *wanted)
            .cloned()
    })
}

fn latest_project_run_for_ids<'a>(
    runs: &'a [ProjectRunRecord],
    ids: &[&str],
) -> Option<&'a ProjectRunRecord> {
    runs.iter()
        .rev()
        .find(|run| ids.iter().any(|id| run.command.id == *id))
}

fn latest_run_passed(run: Option<&ProjectRunRecord>) -> bool {
    run.map(|run| run.status == ProjectRunStatus::Passed)
        .unwrap_or(false)
}

fn release_run_detail(
    run: Option<&ProjectRunRecord>,
    command_present: bool,
    command_name: &str,
) -> String {
    match run {
        Some(run) => format!(
            "{} · {}{}",
            run.label,
            run.status.label(),
            run.exit_code
                .map(|code| format!(" · exit {code}"))
                .unwrap_or_default()
        ),
        None if command_present => format!("команда {command_name} готова, но ещё не запускалась"),
        None => format!("команда {command_name} не найдена"),
    }
}

fn release_command_kind(command_id: &str) -> Option<&'static str> {
    match command_id {
        "check" | "typecheck" | "lint" => Some("preflight"),
        "test" => Some("test"),
        "build" => Some("build"),
        "release" => Some("release"),
        "package" => Some("package"),
        _ => None,
    }
}

fn git_summary_clean(summary: &str) -> bool {
    summary.contains("status: чисто") && summary.contains("diff: нет незакоммиченных изменений")
}

fn file_size_label(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes as f64 >= KB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{bytes} B")
    }
}

fn age_label(timestamp: u64) -> String {
    let now = current_unix_timestamp();
    let elapsed = Duration::from_secs(now.saturating_sub(timestamp));
    format!("{} назад", format_duration(elapsed))
}

fn roadmap_entry_visible(filter: RoadmapFilter, state: RoadmapEntryState) -> bool {
    match filter {
        RoadmapFilter::All => true,
        RoadmapFilter::Done => state == RoadmapEntryState::Done,
        RoadmapFilter::Now => state == RoadmapEntryState::Now,
        RoadmapFilter::Next => state == RoadmapEntryState::Next,
    }
}

fn roadmap_status_to_entry(status: RoadmapStatus) -> RoadmapEntryState {
    match status {
        RoadmapStatus::Done => RoadmapEntryState::Done,
        RoadmapStatus::Now => RoadmapEntryState::Now,
        RoadmapStatus::Next => RoadmapEntryState::Next,
    }
}

fn roadmap_links_summary(item: &RoadmapItem) -> String {
    let mut parts = Vec::new();
    if !item.links.commits.is_empty() {
        parts.push(format!(
            "commits {}",
            compact_inline(&item.links.commits.join(", "), 44)
        ));
    }
    if !item.links.files.is_empty() {
        parts.push(format!("files {}", item.links.files.len()));
    }
    if !item.links.agent_runs.is_empty() {
        parts.push(format!("runs {}", item.links.agent_runs.len()));
    }
    if !item.links.validations.is_empty() {
        parts.push(format!("checks {}", item.links.validations.len()));
    }
    parts.join(" · ")
}

fn roadmap_entry_row(
    ui: &mut egui::Ui,
    state: RoadmapEntryState,
    title: &str,
    detail: &str,
    time: &str,
) {
    let color = match state {
        RoadmapEntryState::Done => egui::Color32::from_rgb(105, 201, 143),
        RoadmapEntryState::Now => accent_color(),
        RoadmapEntryState::Next => egui::Color32::from_rgb(216, 178, 95),
    };
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(color, "●");
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong().small());
            ui.add(egui::Label::new(RichText::new(detail).weak().small()).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.label(RichText::new(time).weak().small());
        });
    });
    ui.add_space(5.0);
    ui.separator();
    ui.add_space(5.0);
}

fn full_width_wrapped_label(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let width = safe_available_width(ui, 1.0);
    ui.scope(|ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.add(egui::Label::new(text).wrap().halign(egui::Align::Min))
    })
    .inner
}

fn full_width_formatted_label(ui: &mut egui::Ui, mut job: egui::text::LayoutJob) -> egui::Response {
    let width = safe_available_width(ui, 1.0);
    job.wrap.max_width = width;
    job.halign = egui::Align::Min;
    job.justify = false;
    ui.scope(|ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.add(egui::Label::new(job).wrap().halign(egui::Align::Min))
    })
    .inner
}

fn safe_available_width(ui: &egui::Ui, min_width: f32) -> f32 {
    let width = ui.available_width();
    if width.is_finite() && width > 0.0 {
        width.max(min_width).min(8_000.0)
    } else {
        min_width.max(1.0)
    }
}

const CHAT_TEXT_PARAGRAPH_SPACING: f32 = 2.0;
const CHAT_MESSAGE_SPACING: f32 = 5.0;

fn chat_inline_job(text: &str, font_size: f32, strong: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.halign = egui::Align::Min;
    let normal_format = egui::TextFormat {
        font_id: egui::FontId::new(font_size, egui::FontFamily::Proportional),
        color: text_color(),
        ..Default::default()
    };
    let code_format = egui::TextFormat {
        font_id: egui::FontId::new((font_size - 1.0).max(11.0), egui::FontFamily::Monospace),
        color: egui::Color32::from_rgb(221, 226, 234),
        background: egui::Color32::from_rgb(34, 39, 48),
        ..Default::default()
    };
    let mut base_format = normal_format;
    if strong {
        base_format.font_id = egui::FontId::new(font_size, egui::FontFamily::Proportional);
    }

    for (index, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        let format = if index % 2 == 1 {
            code_format.clone()
        } else {
            base_format.clone()
        };
        job.append(part, 0.0, format);
    }
    job
}

fn render_chat_text(ui: &mut egui::Ui, content: &str) {
    let mut in_code_block = false;
    let mut code_block = String::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            if in_code_block {
                render_chat_code_block(ui, &code_block);
                code_block.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_block.push_str(line);
            code_block.push('\n');
            continue;
        }

        if trimmed.is_empty() {
            ui.add_space(CHAT_TEXT_PARAGRAPH_SPACING);
            continue;
        }

        if let Some(text) = trimmed.strip_prefix("### ") {
            full_width_formatted_label(ui, chat_inline_job(text, 17.0, true));
            ui.add_space(CHAT_TEXT_PARAGRAPH_SPACING * 0.5);
        } else if let Some(text) = trimmed.strip_prefix("## ") {
            full_width_formatted_label(ui, chat_inline_job(text, 18.0, true));
            ui.add_space(CHAT_TEXT_PARAGRAPH_SPACING * 0.5);
        } else if let Some(text) = trimmed.strip_prefix("# ") {
            full_width_formatted_label(ui, chat_inline_job(text, 20.0, true));
            ui.add_space(CHAT_TEXT_PARAGRAPH_SPACING * 0.5);
        } else if let Some(text) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            render_chat_bullet(ui, text);
        } else if let Some((number, text)) = split_numbered_list_item(trimmed) {
            render_chat_numbered_item(ui, number, text);
        } else if let Some(text) = trimmed.strip_prefix("> ") {
            ui.horizontal(|ui| {
                ui.label(RichText::new("|").weak());
                full_width_formatted_label(ui, chat_inline_job(text, 14.0, false));
            });
        } else {
            full_width_formatted_label(ui, chat_inline_job(trimmed, 14.0, false));
        }
    }

    if in_code_block && !code_block.is_empty() {
        render_chat_code_block(ui, &code_block);
    }
}

fn render_chat_bullet(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("-").weak());
        full_width_formatted_label(ui, chat_inline_job(text, 14.0, false));
    });
}

fn render_chat_numbered_item(ui: &mut egui::Ui, number: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{number}.")).weak());
        full_width_formatted_label(ui, chat_inline_job(text, 14.0, false));
    });
}

fn render_chat_code_block(ui: &mut egui::Ui, code: &str) {
    let width = safe_available_width(ui, 1.0);
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(15, 17, 21))
        .stroke(egui::Stroke::new(1.0, border_color()))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.set_min_width((width - 18.0).max(1.0));
            ui.set_max_width((width - 18.0).max(1.0));
            full_width_wrapped_label(
                ui,
                RichText::new(code.trim_end())
                    .text_style(egui::TextStyle::Monospace)
                    .small(),
            );
        });
    ui.add_space(CHAT_TEXT_PARAGRAPH_SPACING);
}

fn split_numbered_list_item(line: &str) -> Option<(&str, &str)> {
    let (number, rest) = line.split_once(". ")?;
    if !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()) {
        Some((number, rest))
    } else {
        None
    }
}

fn inline_log_entry(ui: &mut egui::Ui, title: &str, content: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(safe_available_width(ui, 1.0));
        ui.label(RichText::new(title).strong().small());
        ui.add(
            egui::Label::new(
                RichText::new(content)
                    .text_style(egui::TextStyle::Monospace)
                    .small()
                    .color(muted_color()),
            )
            .wrap(),
        );
    });
    ui.add_space(6.0);
}

fn agent_live_status_for_tool(name: &str, summary: &str) -> String {
    let text = format!("{} {}", name, summary).to_lowercase();
    let action_status = [
        ("apply_patch", "Агент применяет patch"),
        ("edit_file", "Агент редактирует файл"),
        ("write_file", "Агент записывает файл"),
        ("read_file", "Агент читает файл"),
        ("list_files", "Агент изучает файлы проекта"),
        ("grep", "Агент ищет по коду"),
        ("project_command", "Агент запускает команду проекта"),
        ("run_shell", "Агент выполняет shell-команду"),
        ("terminal_start", "Агент запускает терминал"),
        ("terminal_write", "Агент пишет в терминал"),
        ("terminal_read", "Агент читает терминал"),
        ("run_subagent", "Агент запускает субагента"),
        ("delegate_agent", "Агент передаёт задачу субагенту"),
        ("generate_image_asset", "Агент генерирует изображение"),
        ("generate_audio_asset", "Агент генерирует звук"),
        ("generate_video_asset", "Агент генерирует видео"),
        ("desktop_step", "Агент управляет рабочим столом"),
        ("screenshot", "Агент делает скриншот"),
        ("record_decision", "Агент обновляет память проекта"),
        ("record_memory_source", "Агент сохраняет контекст в память"),
        ("upsert_task", "Агент обновляет задачи проекта"),
    ];
    for (needle, status) in action_status {
        if text.contains(needle) {
            return status.to_string();
        }
    }
    if name == "act" {
        "Агент использует инструмент".to_string()
    } else {
        format!("Агент выполняет {name}")
    }
}

fn format_duration(duration: Duration) -> String {
    let total_ms = duration.as_millis();
    if total_ms < 1_000 {
        format!("{total_ms} мс")
    } else if total_ms < 60_000 {
        format!("{:.1} с", total_ms as f64 / 1_000.0)
    } else {
        let minutes = total_ms / 60_000;
        let seconds = (total_ms % 60_000) / 1_000;
        format!("{minutes} мин {seconds:02} с")
    }
}

fn format_history_duration_ms(total_ms: u64) -> String {
    if total_ms < 1_000 {
        format!("{total_ms} мс")
    } else if total_ms < 60_000 {
        format!("{:.1} с", total_ms as f64 / 1_000.0)
    } else if total_ms < 3_600_000 {
        let minutes = total_ms / 60_000;
        let seconds = (total_ms % 60_000) / 1_000;
        format!("{minutes} мин {seconds:02} с")
    } else {
        let hours = total_ms / 3_600_000;
        let minutes = (total_ms % 3_600_000) / 60_000;
        format!("{hours} ч {minutes:02} мин")
    }
}

fn agent_history_status_label(status: &str) -> &'static str {
    match status {
        "succeeded" => "готово",
        "failed" => "ошибка",
        "cancelled" => "отменено",
        _ => "запуск",
    }
}

fn agent_history_date_label(timestamp: u64) -> String {
    if timestamp == 0 {
        return "дата неизвестна".to_string();
    }
    let days = timestamp / 86_400;
    let seconds = timestamp % 86_400;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    format!("unix day {days} · {hours:02}:{minutes:02}")
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn unix_day_start(timestamp: u64) -> u64 {
    timestamp - (timestamp % 86_400)
}

fn agent_history_search_blob(record: &AgentRunHistoryRecord) -> String {
    let mut parts = vec![
        record.id.clone(),
        record.status.clone(),
        record.provider.clone(),
        record.model.clone(),
        record.route.clone(),
        record.policy_profile.clone(),
        record.workspace_name.clone(),
        record.workspace_root.clone(),
        record.user_request.clone(),
        agent_history_date_label(record.started_at),
        format_history_duration_ms(record.duration_ms),
    ];
    if let Some(plan) = &record.confirmed_plan {
        parts.push(plan.summary.clone());
        parts.push(plan.detail.clone());
    }
    if let Some(response) = &record.final_response {
        parts.push(response.clone());
    }
    if let Some(report) = &record.final_report {
        parts.push(report.clone());
    }
    parts.extend(record.changed_files.iter().cloned());
    parts.extend(record.errors.iter().cloned());
    for approval in &record.approvals {
        parts.push(approval.id.clone());
        parts.push(approval.summary.clone());
        parts.push(approval.detail.clone());
        parts.push(approval.status.clone());
    }
    for tool in &record.tool_calls {
        parts.push(tool.id.clone());
        parts.push(tool.name.clone());
        parts.push(tool.summary.clone());
        parts.push(tool.status.clone());
        parts.push(tool.output_preview.clone());
    }
    for step in &record.timeline_steps {
        parts.push(step.id.clone());
        parts.push(step.title.clone());
        parts.push(step.detail.clone());
        parts.push(step.status.clone());
        parts.push(step.output_preview.clone());
        if let Some(link) = &step.link {
            parts.push(link.clone());
        }
    }
    parts.join("\n").to_ascii_lowercase()
}

fn agent_history_record_markdown(record: &AgentRunHistoryRecord) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!("# Agent run {}\n\n", record.id));
    markdown.push_str("## Сводка\n\n");
    markdown.push_str(&format!(
        "- Статус: {}\n- Дата: {}\n- Длительность: {}\n- Провайдер: {}\n- Модель: {}\n- Маршрут: {}\n- Доступ: {}\n- Проект: {}\n\n",
        agent_history_status_label(&record.status),
        agent_history_date_label(record.started_at),
        format_history_duration_ms(record.duration_ms),
        record.provider,
        record.model,
        record.route,
        record.policy_profile,
        record.workspace_name
    ));
    markdown.push_str("## Запрос\n\n");
    markdown.push_str(record.user_request.trim());
    markdown.push_str("\n\n");

    if let Some(plan) = &record.confirmed_plan {
        markdown.push_str("## Подтверждённый план\n\n");
        markdown.push_str(&format!(
            "**{}**\n\n{}\n\n",
            plan.summary,
            plan.detail.trim()
        ));
    }

    if !record.changed_files.is_empty() {
        markdown.push_str("## Изменённые файлы\n\n");
        for file in &record.changed_files {
            markdown.push_str(&format!("- `{file}`\n"));
        }
        markdown.push('\n');
    }

    if !record.tool_calls.is_empty() {
        markdown.push_str("## Инструменты\n\n");
        for tool in &record.tool_calls {
            let duration = tool
                .duration_ms
                .map(format_history_duration_ms)
                .unwrap_or_else(|| "без времени".to_string());
            markdown.push_str(&format!(
                "- `{}` · {} · {}\n  {}\n",
                tool.name,
                tool.status,
                duration,
                compact_inline(&tool.summary, 500)
            ));
            if !tool.output_preview.trim().is_empty() {
                markdown.push_str(&format!(
                    "  output: {}\n",
                    compact_inline(&tool.output_preview, 500)
                ));
            }
        }
        markdown.push('\n');
    }

    if !record.approvals.is_empty() {
        markdown.push_str("## Согласования\n\n");
        for approval in &record.approvals {
            markdown.push_str(&format!(
                "- {} · {}\n  {}\n",
                approval.summary,
                approval.status,
                compact_inline(&approval.detail, 500)
            ));
        }
        markdown.push('\n');
    }

    if !record.errors.is_empty() {
        markdown.push_str("## Ошибки\n\n");
        for error in &record.errors {
            markdown.push_str(&format!("- {}\n", compact_inline(error, 800)));
        }
        markdown.push('\n');
    }

    if let Some(report) = &record.final_report {
        markdown.push_str("## Итоговый отчёт\n\n");
        markdown.push_str(report.trim());
        markdown.push_str("\n\n");
    } else if let Some(response) = &record.final_response {
        markdown.push_str("## Итоговый ответ\n\n");
        markdown.push_str(response.trim());
        markdown.push_str("\n\n");
    }

    if !record.timeline_steps.is_empty() {
        markdown.push_str("## Timeline\n\n");
        for step in record.timeline_steps.iter().take(120) {
            let duration = step
                .duration_ms
                .map(format_history_duration_ms)
                .unwrap_or_else(|| "без времени".to_string());
            markdown.push_str(&format!(
                "- {} · {} · {}\n  {}\n",
                step.title,
                step.status,
                duration,
                compact_inline(&step.detail, 500)
            ));
        }
        if record.timeline_steps.len() > 120 {
            markdown.push_str("- ... timeline truncated ...\n");
        }
        markdown.push('\n');
    }

    markdown
}

fn run_timeline_card(ui: &mut egui::Ui, timeline: &RunTimeline) {
    let width = safe_available_width(ui, 1.0).clamp(320.0, 980.0);
    ui.set_min_width(width);
    ui.set_max_width(width);

    let overall_status = timeline_overall_status(timeline);
    egui::CollapsingHeader::new(
        RichText::new(timeline_status_summary(timeline))
            .small()
            .strong()
            .color(timeline_status_color(&overall_status)),
    )
    .id_salt("run_timeline_status")
    .default_open(false)
    .show(ui, |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        full_width_wrapped_label(ui, RichText::new(&timeline.title).weak().small());
        ui.add_space(4.0);
        for (index, step) in timeline.steps.iter().enumerate() {
            let title = format!(
                "{}. {} — {}{}",
                index + 1,
                step.title,
                step.status.label(),
                step.duration_label()
                    .map(|duration| format!(" · {duration}"))
                    .unwrap_or_default()
            );
            egui::CollapsingHeader::new(
                RichText::new(title)
                    .small()
                    .color(timeline_status_color(&step.status)),
            )
            .default_open(matches!(
                step.status,
                RunTimelineStatus::Running
                    | RunTimelineStatus::WaitingApproval
                    | RunTimelineStatus::Failed
            ))
            .show(ui, |ui| {
                if !step.detail.trim().is_empty() {
                    full_width_wrapped_label(ui, step.detail.as_str());
                }
                if !step.output.trim().is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("вывод / примечание").weak().small());
                    full_width_wrapped_label(
                        ui,
                        RichText::new(step.output.as_str()).monospace().small(),
                    );
                }
                if let Some(link) = &step.link {
                    ui.add_space(4.0);
                    full_width_wrapped_label(
                        ui,
                        RichText::new(format!("связанный путь: {link}"))
                            .weak()
                            .small(),
                    );
                }
            });
        }
    });
    ui.add_space(4.0);
}

fn timeline_status_summary(timeline: &RunTimeline) -> String {
    let status = timeline_overall_status(timeline);
    let marker = match &status {
        RunTimelineStatus::Running => "●",
        RunTimelineStatus::WaitingApproval => "◆",
        RunTimelineStatus::Succeeded => "✓",
        RunTimelineStatus::Failed => "×",
        RunTimelineStatus::Cancelled => "–",
    };

    if timeline.finished_at.is_some() {
        return format!(
            "{marker} {} · время: {} · файлы: {} · инструменты: {} · шагов: {}",
            timeline_finished_label(&status),
            timeline.elapsed_label(),
            timeline_file_summary(timeline),
            timeline_tool_summary(timeline),
            timeline.steps.len()
        );
    }

    let step_title = timeline
        .steps
        .iter()
        .rev()
        .find(|step| {
            matches!(
                step.status,
                RunTimelineStatus::Running | RunTimelineStatus::WaitingApproval
            )
        })
        .or_else(|| {
            timeline
                .steps
                .iter()
                .rev()
                .find(|step| step.status == status)
        })
        .or_else(|| timeline.steps.last())
        .map(|step| compact_inline(&step.title, 64))
        .unwrap_or_else(|| "нет шагов".to_string());

    format!(
        "{marker} {} · {} · шагов: {} · время: {}",
        status.label(),
        step_title,
        timeline.steps.len(),
        timeline.elapsed_label()
    )
}

fn timeline_finished_label(status: &RunTimelineStatus) -> &'static str {
    match status {
        RunTimelineStatus::Succeeded => "Задача выполнена",
        RunTimelineStatus::Failed => "Задача завершена с ошибкой",
        RunTimelineStatus::Cancelled => "Задача отменена",
        RunTimelineStatus::WaitingApproval => "Задача ждёт доступа",
        RunTimelineStatus::Running => "Задача выполняется",
    }
}

fn timeline_file_summary(timeline: &RunTimeline) -> String {
    if timeline.changed_files.is_empty() {
        return "нет".to_string();
    }
    compact_list(timeline.changed_files.iter().map(String::as_str), 3, 24)
}

fn timeline_tool_summary(timeline: &RunTimeline) -> String {
    let mut tools = Vec::new();
    for step in &timeline.steps {
        let Some(tool) = step.title.strip_prefix("Инструмент: ") else {
            continue;
        };
        if !tools.iter().any(|known| *known == tool) {
            tools.push(tool);
        }
    }
    if tools.is_empty() {
        "нет".to_string()
    } else {
        compact_list(tools.into_iter(), 4, 22)
    }
}

fn compact_list<'a>(
    items: impl Iterator<Item = &'a str>,
    shown: usize,
    item_chars: usize,
) -> String {
    let items = items.collect::<Vec<_>>();
    let total = items.len();
    let mut parts = items
        .into_iter()
        .take(shown)
        .map(|item| compact_inline(item, item_chars))
        .collect::<Vec<_>>();
    if total > shown {
        parts.push(format!("+{}", total - shown));
    }
    parts.join(", ")
}

fn timeline_overall_status(timeline: &RunTimeline) -> RunTimelineStatus {
    if timeline
        .steps
        .iter()
        .any(|step| step.status == RunTimelineStatus::WaitingApproval)
    {
        RunTimelineStatus::WaitingApproval
    } else if timeline
        .steps
        .iter()
        .any(|step| step.status == RunTimelineStatus::Running)
    {
        RunTimelineStatus::Running
    } else if timeline
        .steps
        .iter()
        .any(|step| step.status == RunTimelineStatus::Failed)
        || timeline.failed
    {
        RunTimelineStatus::Failed
    } else if timeline
        .steps
        .iter()
        .any(|step| step.status == RunTimelineStatus::Cancelled)
    {
        RunTimelineStatus::Cancelled
    } else {
        RunTimelineStatus::Succeeded
    }
}

fn timeline_status_color(status: &RunTimelineStatus) -> egui::Color32 {
    match status {
        RunTimelineStatus::Running => egui::Color32::from_rgb(236, 214, 151),
        RunTimelineStatus::WaitingApproval => egui::Color32::from_rgb(240, 180, 96),
        RunTimelineStatus::Succeeded => egui::Color32::from_rgb(139, 211, 158),
        RunTimelineStatus::Failed => egui::Color32::from_rgb(235, 120, 120),
        RunTimelineStatus::Cancelled => egui::Color32::from_rgb(190, 190, 190),
    }
}

fn should_require_run_gate(message: &str) -> bool {
    let lower = message.to_lowercase();
    if is_confirmation_text(&lower) {
        return false;
    }
    let non_trivial_markers = [
        "реализ",
        "добав",
        "исправ",
        "сделай",
        "создай",
        "проверь",
        "проанализ",
        "спланируй",
        "рефактор",
        "запусти",
        "прогони",
        "обнови",
        "измен",
        "cargo ",
        "test",
        "check",
        "build",
        "shell",
        "файл",
        "код",
        "этап",
        "бэклог",
        "roadmap",
        "дорожн",
    ];
    non_trivial_markers
        .iter()
        .any(|marker| lower.contains(marker))
}

fn is_confirmation_text(message: &str) -> bool {
    matches!(
        message.trim().to_lowercase().as_str(),
        "да" | "ок" | "окей" | "подтверждаю" | "согласен" | "согласовано"
    )
}

fn confirmed_run_message(gate: &PendingRunGate) -> String {
    format!(
        "План задачи уже подтверждён пользователем через интерфейс Leetcode.\nНе спрашивай повторно «правильно ли я понимаю» и не запрашивай повторное подтверждение общего плана. Сразу выполняй подтверждённый план; отдельные подтверждения нужны только для конкретных рискованных инструментов.\n\nИсходная задача пользователя:\n{}\n\nПодтверждённое понимание:\n{}\n\nПодтверждённый план:\n{}",
        gate.original_message, gate.summary, gate.detail
    )
}

fn requested_stage_number(message: &str) -> Option<usize> {
    let lower = message.to_lowercase();
    let words = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();

    for pair in words.windows(2) {
        if matches!(pair[0], "этап" | "stage") {
            if let Ok(number) = pair[1].parse::<usize>() {
                return Some(number);
            }
        }
        if matches!(pair[1], "этап" | "stage") {
            if let Ok(number) = pair[0].parse::<usize>() {
                return Some(number);
            }
        }
    }
    None
}

fn extract_backlog_stage_section(backlog: &str, stage: usize) -> Option<String> {
    let english = format!("## Stage {stage}");
    let russian = format!("## Этап {stage}");
    let mut capture = false;
    let mut lines = Vec::new();

    for line in backlog.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## ") {
            if capture {
                break;
            }
            capture = trimmed.starts_with(&english) || trimmed.starts_with(&russian);
        }
        if capture {
            lines.push(line);
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn clean_backlog_bullet(line: &str) -> String {
    line.trim_start_matches("- ")
        .trim_start_matches("Done:")
        .trim_start_matches("Todo:")
        .trim()
        .to_string()
}

fn compact_number(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f32 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f32 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn context_health_score(
    prompt_chars: usize,
    duplicate_notes: usize,
    missing_goals: bool,
    stale_summary: bool,
    oversized_sources: usize,
) -> usize {
    let mut score = 100usize;
    if prompt_chars > 24_000 {
        score = score.saturating_sub(22);
    } else if prompt_chars > 14_000 {
        score = score.saturating_sub(10);
    }
    score = score.saturating_sub(duplicate_notes.min(5) * 6);
    score = score.saturating_sub(oversized_sources.min(4) * 8);
    if missing_goals {
        score = score.saturating_sub(12);
    }
    if stale_summary {
        score = score.saturating_sub(10);
    }
    score.clamp(0, 100)
}

fn context_overview_visual(
    ui: &mut egui::Ui,
    health_score: usize,
    prompt_chars: usize,
    messages: usize,
    notes: usize,
    sources: usize,
    runs: usize,
    profiles: usize,
    recent_messages: usize,
    relevant_messages: usize,
    recent_runs: usize,
) {
    let width = safe_available_width(ui, 1.0);
    let card_response = egui::Frame::none()
        .fill(egui::Color32::from_rgb(18, 22, 28))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 55, 65)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{health_score}%"))
                            .strong()
                            .size(30.0)
                            .color(context_score_color(health_score)),
                    )
                    .on_hover_text(
                        "Индекс качества контекста. Снижается, если prompt слишком большой, есть дубликаты заметок, устаревший summary или крупные источники памяти.",
                    );
                    ui.label(RichText::new("здоровье контекста").weak().small());
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.set_min_width((width - 120.0).max(1.0));
                    ui.label(RichText::new("Состав следующего запуска").strong())
                        .on_hover_text("Что примерно попадёт в следующий запрос агента к модели.");
                    ui.add(
                        egui::ProgressBar::new((prompt_chars as f32 / 24_000.0).clamp(0.0, 1.0))
                            .desired_width(safe_available_width(ui, 160.0))
                            .fill(context_score_color(health_score))
                            .text(format!("prompt ~{} символов", compact_number(prompt_chars))),
                    )
                    .on_hover_text(format!(
                        "Примерный размер системного контекста для следующего запуска: {prompt_chars} символов. Чем больше prompt, тем дороже и шумнее может быть задача."
                    ));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        context_micro_stat(ui, "чат", messages);
                        context_micro_stat(ui, "заметки", notes);
                        context_micro_stat(ui, "память", sources);
                        context_micro_stat(ui, "runs", runs);
                        context_micro_stat(ui, "профили", profiles);
                    });
                });
            });
        });
    card_response.response.on_hover_text(
        "Общая карточка контекста: насколько чистый контекст и из каких частей будет собран следующий запуск агента.",
    );
    ui.add_space(8.0);
    context_signal_row(
        ui,
        "чат",
        recent_messages,
        messages.max(1),
        egui::Color32::from_rgb(96, 191, 143),
    );
    context_signal_row(
        ui,
        "retrieval",
        relevant_messages,
        messages.max(1),
        accent_color(),
    );
    context_signal_row(
        ui,
        "история запусков",
        recent_runs,
        runs.max(1),
        egui::Color32::from_rgb(220, 174, 92),
    );
}

fn context_micro_stat_tooltip(label: &str, value: usize) -> String {
    match label {
        "чат" => format!("Всего сообщений в текущем диалоге: {value}."),
        "заметки" => format!("Закреплённые заметки текущего чата: {value}."),
        "память" => format!("Источники памяти проекта, доступные агенту: {value}."),
        "runs" => format!("Сохранённые записи запусков агента: {value}."),
        "профили" => format!("Экспортированные профили контекста: {value}."),
        _ => format!("{label}: {value}"),
    }
}

fn context_micro_stat(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.label(
        RichText::new(format!("{label} {value}"))
            .small()
            .color(egui::Color32::from_rgb(197, 209, 219)),
    )
    .on_hover_text(context_micro_stat_tooltip(label, value));
}

fn context_signal_tooltip(label: &str, value: usize, total: usize) -> String {
    match label {
        "чат" | "последние сообщения" => format!(
            "Свежие сообщения, которые попадут в следующий prompt: {value} из лимита {total}."
        ),
        "retrieval" | "релевантные" => format!(
            "Старые сообщения, найденные как релевантные текущему запросу: {value} из лимита {total}."
        ),
        "история запусков" | "запуски" => format!(
            "Недавние сохранённые запуски агента, добавляемые в контекст: {value} из лимита {total}."
        ),
        _ => format!("{label}: {value}/{total}"),
    }
}

fn context_signal_row(
    ui: &mut egui::Ui,
    label: &str,
    value: usize,
    total: usize,
    color: egui::Color32,
) {
    let total = total.max(1);
    let ratio = (value as f32 / total as f32).clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        ui.set_min_width(safe_available_width(ui, 1.0));
        let tooltip = context_signal_tooltip(label, value, total);
        ui.label(RichText::new(label).weak().small())
            .on_hover_text(tooltip.clone());
        ui.add(
            egui::ProgressBar::new(ratio)
                .desired_width((safe_available_width(ui, 120.0) * 0.56).max(80.0))
                .fill(color)
                .text(format!("{value}/{total}")),
        )
        .on_hover_text(tooltip);
    });
}

fn context_health_strip(
    ui: &mut egui::Ui,
    duplicate_notes: usize,
    missing_goals: bool,
    stale_summary: bool,
    oversized_sources: usize,
) {
    ui.horizontal_wrapped(|ui| {
        if duplicate_notes == 0 && !missing_goals && !stale_summary && oversized_sources == 0 {
            soft_badge(ui, "контекст чистый", egui::Color32::from_rgb(91, 178, 126));
            return;
        }
        if duplicate_notes > 0 {
            soft_badge(
                ui,
                format!("дубликаты: {duplicate_notes}"),
                egui::Color32::from_rgb(220, 174, 92),
            );
        }
        if missing_goals {
            soft_badge(
                ui,
                "нет целей проекта",
                egui::Color32::from_rgb(220, 174, 92),
            );
        }
        if stale_summary {
            soft_badge(ui, "summary устарел", egui::Color32::from_rgb(220, 174, 92));
        }
        if oversized_sources > 0 {
            soft_badge(
                ui,
                format!("крупные источники: {oversized_sources}"),
                egui::Color32::from_rgb(220, 112, 112),
            );
        }
    });
}

fn soft_badge(ui: &mut egui::Ui, text: impl Into<String>, color: egui::Color32) {
    let text = text.into();
    let shown = compact_inline(&text, 42);
    let tooltip = health_badge_tooltip(&text);
    let fill = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 28);
    let stroke = egui::Stroke::new(
        1.0,
        egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 110),
    );
    let response = egui::Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(egui::Rounding::same(999.0))
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(&shown).small().color(color));
        })
        .response;
    response.on_hover_text(tooltip);
}

fn health_badge_tooltip(text: &str) -> String {
    if text == "контекст чистый" {
        "Нет заметных проблем: дубликатов нет, цели проекта есть, summary актуален, источники памяти не выглядят слишком большими.".to_string()
    } else if text.starts_with("дубликаты") {
        "В закреплённых заметках есть похожие записи. Дубликаты увеличивают prompt и могут путать агента.".to_string()
    } else if text == "нет целей проекта" {
        "В памяти проекта не указаны финальные цели. Агенту сложнее выбирать правильное направление без них.".to_string()
    } else if text == "summary устарел" {
        "Диалог уже длинный, но сжатое summary ещё не создано. Часть старого контекста может потеряться.".to_string()
    } else if text.starts_with("крупные источники") {
        "Некоторые источники памяти очень большие. Их лучше сжать или разделить, чтобы не засорять prompt.".to_string()
    } else {
        text.to_string()
    }
}

fn context_score_color(score: usize) -> egui::Color32 {
    if score >= 80 {
        egui::Color32::from_rgb(105, 201, 143)
    } else if score >= 55 {
        egui::Color32::from_rgb(220, 174, 92)
    } else {
        egui::Color32::from_rgb(220, 112, 112)
    }
}

fn chat_role_label(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::User => "Вы",
        ChatRole::Assistant => "Агент",
        ChatRole::System => "Система",
    }
}

fn normalized_context_note_key(note: &str) -> String {
    note.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn duplicate_context_note_count(notes: &[String]) -> usize {
    let mut seen = Vec::<String>::new();
    let mut duplicates = 0;
    for note in notes {
        let key = normalized_context_note_key(note);
        if key.is_empty() {
            continue;
        }
        if seen.iter().any(|existing| existing == &key) {
            duplicates += 1;
        } else {
            seen.push(key);
        }
    }
    duplicates
}

fn context_profile_new_notes(profile_notes: &[String], current_notes: &[String]) -> Vec<String> {
    profile_notes
        .iter()
        .filter(|note| {
            let key = normalized_context_note_key(note);
            !key.is_empty()
                && !current_notes
                    .iter()
                    .any(|current| normalized_context_note_key(current) == key)
        })
        .map(|note| compact_inline(note, 500))
        .collect()
}

fn context_profile_budget_diff(current: ContextBudget, incoming: ContextBudget) -> String {
    format!(
        "{}>{} / {}>{} / {}>{}",
        current.recent_message_limit,
        incoming.recent_message_limit,
        current.relevant_message_limit,
        incoming.relevant_message_limit,
        current.recent_run_limit,
        incoming.recent_run_limit
    )
}

fn chat_message(ui: &mut egui::Ui, line: &ChatLine, live_status: Option<&str>) {
    let width = safe_available_width(ui, 1.0);
    let is_user = matches!(line.role, ChatRole::User);
    let message_width = if is_user {
        (width * 0.68).clamp(320.0, 780.0).min(width)
    } else {
        width.min(980.0).max(1.0)
    };

    ui.horizontal(|ui| {
        ui.set_min_width(width);
        if is_user {
            ui.add_space((width - message_width).max(0.0));
        }
        ui.vertical(|ui| {
            ui.set_min_width(message_width);
            ui.set_max_width(message_width);
            chat_message_body(ui, line, live_status);
        });
    });
    ui.add_space(CHAT_MESSAGE_SPACING);
    ui.separator();
    ui.add_space(CHAT_MESSAGE_SPACING);
}

fn chat_message_body(ui: &mut egui::Ui, line: &ChatLine, live_status: Option<&str>) {
    let content_width = safe_available_width(ui, 1.0);
    let (label, label_color) = match line.role {
        ChatRole::User => ("Вы", accent_color()),
        ChatRole::Assistant => ("Агент", egui::Color32::from_rgb(236, 214, 151)),
        ChatRole::System => ("Система", muted_color()),
    };

    ui.vertical(|ui| {
        ui.set_min_width(content_width);
        ui.set_max_width(content_width);
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).strong().small().color(label_color));
            if matches!(line.role, ChatRole::System) {
                ui.label(RichText::new("стартовый контекст").weak().small());
            }
            if let Some(elapsed) = line.elapsed.as_deref() {
                if matches!(line.role, ChatRole::Assistant) {
                    ui.label(RichText::new(format!("· {elapsed}")).weak().small());
                }
            }
        });
        ui.add_space(CHAT_TEXT_PARAGRAPH_SPACING);
        render_chat_text(ui, line.content.as_str());
        if let Some(status) = live_status {
            ui.add_space(CHAT_MESSAGE_SPACING);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(status)
                        .weak()
                        .small()
                        .color(egui::Color32::from_rgb(236, 214, 151)),
                );
            });
        }
    });
}

fn image_api_key_from_config(config: &AppConfig, provider_id: &str) -> String {
    let direct_key = config.api_key_for_provider(provider_id);
    if !direct_key.trim().is_empty() {
        return direct_key;
    }

    match provider_id {
        OPENAI_IMAGE_PROVIDER_ID => config.api_key_for_provider(OPENAI_PROVIDER_ID),
        GEMINI_IMAGE_PROVIDER_ID => config.api_key_for_provider(GEMINI_PROVIDER_ID),
        _ => String::new(),
    }
}

fn media_api_key_from_config(config: &AppConfig, provider_id: &str) -> String {
    let direct_key = config.api_key_for_provider(provider_id);
    if !direct_key.trim().is_empty() {
        return direct_key;
    }

    match provider_id {
        OPENAI_AUDIO_PROVIDER_ID | OPENAI_VIDEO_PROVIDER_ID => {
            config.api_key_for_provider(OPENAI_PROVIDER_ID)
        }
        _ => String::new(),
    }
}

fn image_model_from_config(config: &AppConfig, provider_id: &str) -> String {
    config
        .providers
        .get(provider_id)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_image_model(provider_id).to_string())
}

fn media_model_from_config(config: &AppConfig, provider_id: &str, default_model: &str) -> String {
    config
        .providers
        .get(provider_id)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_model.to_string())
}

fn summarize_window_value(value: &serde_json::Value) -> String {
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Без названия");
    let process = value
        .get("process_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("неизвестно");
    let pid = value
        .get("process_id")
        .and_then(serde_json::Value::as_u64)
        .map(|pid| pid.to_string())
        .unwrap_or_default();

    if pid.is_empty() {
        format!("активно: {title} ({process})")
    } else {
        format!("активно: {title} ({process}, pid {pid})")
    }
}

fn center_tab_button(
    ui: &mut egui::Ui,
    id_salt: &str,
    label: &str,
    selected: bool,
    closeable: bool,
) -> (egui::Response, bool) {
    let base_width = label.chars().count() as f32 * 8.4 + if closeable { 48.0 } else { 30.0 };
    let width = if closeable {
        base_width.clamp(112.0, 220.0)
    } else {
        base_width.clamp(86.0, 120.0)
    };
    let height = 34.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let rounding = egui::Rounding {
        nw: 6.0,
        ne: 6.0,
        sw: 0.0,
        se: 0.0,
    };
    let fill = if selected {
        surface_bg()
    } else if response.hovered() {
        surface_alt_bg()
    } else {
        panel_bg()
    };
    let stroke = if selected {
        egui::Stroke::new(1.0, egui::Color32::from_rgb(69, 82, 98))
    } else {
        egui::Stroke::new(1.0, border_color())
    };

    ui.painter().rect_filled(rect, rounding, fill);
    ui.painter().rect_stroke(rect, rounding, stroke);
    if selected {
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 1.0, rect.top() + 1.0),
                egui::pos2(rect.right() - 1.0, rect.top() + 1.0),
            ],
            egui::Stroke::new(2.0, accent_color()),
        );
    }

    let close_reserved = if closeable { 30.0 } else { 0.0 };
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 12.0, rect.top()),
        egui::pos2(rect.right() - close_reserved - 8.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(text_rect).text(
        egui::pos2(text_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
        if selected {
            text_color()
        } else {
            muted_color()
        },
    );

    let mut close_clicked = false;
    if closeable {
        let close_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, rect.center().y),
            egui::vec2(20.0, 20.0),
        );
        let close_response = ui
            .interact(
                close_rect,
                ui.make_persistent_id(("center_tab_close", id_salt)),
                egui::Sense::click(),
            )
            .on_hover_text("Закрыть вкладку");
        if close_response.hovered() {
            ui.painter()
                .rect_filled(close_rect, egui::Rounding::same(4.0), surface_alt_bg());
        }
        let x_color = if close_response.hovered() {
            text_color()
        } else {
            muted_color()
        };
        ui.painter().line_segment(
            [
                egui::pos2(close_rect.left() + 6.0, close_rect.top() + 6.0),
                egui::pos2(close_rect.right() - 6.0, close_rect.bottom() - 6.0),
            ],
            egui::Stroke::new(1.4, x_color),
        );
        ui.painter().line_segment(
            [
                egui::pos2(close_rect.right() - 6.0, close_rect.top() + 6.0),
                egui::pos2(close_rect.left() + 6.0, close_rect.bottom() - 6.0),
            ],
            egui::Stroke::new(1.4, x_color),
        );
        close_clicked = close_response.clicked();
    }

    (response, close_clicked)
}

fn project_nav_row(
    ui: &mut egui::Ui,
    name: &str,
    id_salt: &str,
    selected: bool,
    expanded: bool,
    drop_target: bool,
    pinned: bool,
) -> ProjectNavRowResponse {
    let width = safe_available_width(ui, 160.0);
    let height = 31.0;
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let fill = if selected {
        egui::Color32::from_rgb(27, 30, 36)
    } else if drop_target && row.hovered() {
        egui::Color32::from_rgb(28, 42, 48)
    } else if row.hovered() {
        egui::Color32::from_rgb(22, 25, 31)
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(
            rect.shrink2(egui::vec2(2.0, 1.0)),
            egui::Rounding::same(4.0),
            fill,
        );
    }
    if selected {
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 2.0, rect.top() + 6.0),
                egui::pos2(rect.left() + 2.0, rect.bottom() - 6.0),
            ],
            egui::Stroke::new(2.0, accent_color()),
        );
    }

    let disclosure_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 5.0, rect.center().y - 10.0),
        egui::vec2(20.0, 20.0),
    );
    let disclosure = ui.interact(
        disclosure_rect,
        ui.make_persistent_id(("project_disclosure", id_salt)),
        egui::Sense::click(),
    );
    draw_disclosure_icon(
        ui,
        disclosure_rect,
        ui.make_persistent_id(("project_disclosure_anim", id_salt)),
        expanded,
        if selected {
            accent_color()
        } else {
            muted_color()
        },
    );

    draw_project_icon(
        ui,
        egui::pos2(rect.left() + 33.0, rect.center().y),
        if selected {
            accent_color()
        } else {
            muted_color()
        },
    );

    ui.painter().text(
        egui::pos2(rect.left() + 52.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::new(15.5, egui::FontFamily::Proportional),
        text_color(),
    );

    if pinned {
        let pin_center = egui::pos2(rect.right() - 12.0, rect.center().y);
        ui.painter().circle_filled(pin_center, 3.0, subtle_accent());
    }

    ProjectNavRowResponse { row, disclosure }
}

fn file_tree_nav_row(
    ui: &mut egui::Ui,
    name: &str,
    id_salt: &str,
    depth: usize,
    is_dir: bool,
    selected: bool,
    expanded: bool,
) -> FileTreeNavRowResponse {
    let width = safe_available_width(ui, 80.0);
    let height = 27.0;
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let row_rect = rect.shrink2(egui::vec2(2.0, 1.0));
    let fill = if selected {
        egui::Color32::from_rgb(27, 30, 36)
    } else if row.hovered() {
        egui::Color32::from_rgb(28, 33, 40)
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(row_rect, egui::Rounding::same(4.0), fill);
    }

    let indent = 8.0 + depth as f32 * 17.0;
    let icon_x = rect.left() + indent;
    let kind_icon_center = if is_dir {
        egui::pos2(icon_x + 22.0, rect.center().y)
    } else {
        egui::pos2(icon_x + 8.0, rect.center().y)
    };
    let text_x = if is_dir { icon_x + 38.0 } else { icon_x + 24.0 };
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top()),
        egui::pos2(rect.right() - 6.0, rect.bottom()),
    );
    let text_color = if is_dir {
        egui::Color32::from_rgb(222, 228, 236)
    } else {
        text_color()
    };

    let disclosure = if is_dir {
        let disclosure_rect = egui::Rect::from_min_size(
            egui::pos2(icon_x - 4.0, rect.center().y - 10.0),
            egui::vec2(20.0, 20.0),
        );
        let disclosure = ui.interact(
            disclosure_rect,
            ui.make_persistent_id(("dir_disclosure", id_salt)),
            egui::Sense::click(),
        );
        draw_disclosure_icon(
            ui,
            disclosure_rect,
            ui.make_persistent_id(("dir_disclosure_anim", id_salt)),
            expanded,
            if selected {
                accent_color()
            } else {
                muted_color()
            },
        );
        Some(disclosure)
    } else {
        None
    };

    draw_tree_item_icon(ui, id_salt, is_dir, kind_icon_center, selected);

    ui.painter().with_clip_rect(text_rect).text(
        egui::pos2(text_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::new(15.5, egui::FontFamily::Proportional),
        text_color,
    );

    FileTreeNavRowResponse { row, disclosure }
}

fn draw_disclosure_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    id: egui::Id,
    expanded: bool,
    color: egui::Color32,
) {
    let progress = ui.ctx().animate_bool(id, expanded);
    let angle = progress * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let center = rect.center();
    let rotate =
        |x: f32, y: f32| egui::pos2(center.x + x * cos - y * sin, center.y + x * sin + y * cos);
    let points = [rotate(-3.5, -5.0), rotate(3.5, 0.0), rotate(-3.5, 5.0)];
    ui.painter()
        .line_segment([points[0], points[1]], egui::Stroke::new(1.5, color));
    ui.painter()
        .line_segment([points[1], points[2]], egui::Stroke::new(1.5, color));
}

fn draw_project_icon(ui: &egui::Ui, center: egui::Pos2, color: egui::Color32) {
    let rect = egui::Rect::from_center_size(center, egui::vec2(12.0, 13.0));
    ui.painter().rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        egui::Stroke::new(1.2, color),
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.left() + 3.0, rect.top() + 3.5),
            egui::pos2(rect.right() - 3.0, rect.top() + 3.5),
        ],
        egui::Stroke::new(1.0, color),
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.left() + 3.0, rect.bottom() - 3.5),
            egui::pos2(rect.right() - 3.0, rect.bottom() - 3.5),
        ],
        egui::Stroke::new(1.0, color),
    );
}

fn draw_tree_item_icon(
    ui: &egui::Ui,
    path: &str,
    is_dir: bool,
    center: egui::Pos2,
    selected: bool,
) {
    let color = file_kind_color(path, is_dir, selected);
    if is_dir {
        let body = egui::Rect::from_center_size(
            egui::pos2(center.x + 0.5, center.y + 1.5),
            egui::vec2(14.0, 9.0),
        );
        let tab = egui::Rect::from_min_size(
            egui::pos2(body.left() + 1.0, body.top() - 3.0),
            egui::vec2(6.0, 4.0),
        );
        ui.painter().rect_stroke(
            body,
            egui::Rounding::same(2.0),
            egui::Stroke::new(1.2, color),
        );
        ui.painter().line_segment(
            [tab.left_top(), egui::pos2(tab.right(), tab.top())],
            egui::Stroke::new(1.2, color),
        );
        ui.painter().line_segment(
            [tab.left_top(), tab.left_bottom()],
            egui::Stroke::new(1.2, color),
        );
        ui.painter().line_segment(
            [tab.left_bottom(), egui::pos2(tab.right(), body.top())],
            egui::Stroke::new(1.2, color),
        );
        return;
    }

    let rect = egui::Rect::from_center_size(center, egui::vec2(11.0, 14.0));
    ui.painter().rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        egui::Stroke::new(1.1, color),
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.right() - 4.0, rect.top()),
            egui::pos2(rect.right(), rect.top() + 4.0),
        ],
        egui::Stroke::new(1.1, color),
    );
    ui.painter()
        .circle_filled(egui::pos2(rect.center().x, rect.bottom() - 3.0), 1.3, color);
}

fn file_kind_color(path: &str, is_dir: bool, selected: bool) -> egui::Color32 {
    if selected {
        return accent_color();
    }
    if is_dir {
        return egui::Color32::from_rgb(144, 170, 190);
    }
    match file_extension(path).as_deref() {
        Some(
            "rs" | "toml" | "lock" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "cs" | "cpp" | "c"
            | "h" | "hpp" | "gd" | "shader" | "wgsl" | "glsl",
        ) => egui::Color32::from_rgb(102, 184, 224),
        Some("json" | "yml" | "yaml" | "xml" | "ini" | "env") => {
            egui::Color32::from_rgb(210, 172, 96)
        }
        Some("md" | "txt" | "rst") => egui::Color32::from_rgb(174, 150, 220),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "ico") => {
            egui::Color32::from_rgb(93, 197, 161)
        }
        Some("wav" | "mp3" | "ogg" | "flac" | "mp4" | "mov" | "webm") => {
            egui::Color32::from_rgb(218, 126, 153)
        }
        Some("fbx" | "obj" | "gltf" | "glb" | "blend") => egui::Color32::from_rgb(211, 144, 92),
        _ => muted_color(),
    }
}

fn project_root_row(
    ui: &mut egui::Ui,
    name: &str,
    selected: bool,
    drop_target: bool,
) -> egui::Response {
    let width = safe_available_width(ui, 160.0);
    let height = 31.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let fill = if selected {
        egui::Color32::from_rgb(28, 96, 120)
    } else if drop_target && response.hovered() {
        egui::Color32::from_rgb(31, 73, 88)
    } else if response.hovered() {
        egui::Color32::from_rgb(24, 29, 35)
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(
            rect.shrink2(egui::vec2(2.0, 1.0)),
            egui::Rounding::same(4.0),
            fill,
        );
    }

    let text_color = if selected {
        egui::Color32::WHITE
    } else {
        text_color()
    };
    ui.painter().text(
        egui::pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "▾",
        egui::FontId::new(13.5, egui::FontFamily::Proportional),
        muted_color(),
    );
    ui.painter().text(
        egui::pos2(rect.left() + 28.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::new(16.5, egui::FontFamily::Proportional),
        text_color,
    );
    response
}

fn file_tree_row(
    ui: &mut egui::Ui,
    name: &str,
    depth: usize,
    is_dir: bool,
    selected: bool,
) -> egui::Response {
    let width = safe_available_width(ui, 80.0);
    let height = 27.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let row_rect = rect.shrink2(egui::vec2(2.0, 1.0));
    let fill = if selected {
        egui::Color32::from_rgb(35, 127, 158)
    } else if response.hovered() {
        egui::Color32::from_rgb(28, 33, 40)
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(row_rect, egui::Rounding::same(4.0), fill);
    }

    let indent = 8.0 + depth as f32 * 17.0;
    let icon_x = rect.left() + indent;
    let text_x = icon_x + 18.0;
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top()),
        egui::pos2(rect.right() - 6.0, rect.bottom()),
    );
    let text_color = if selected {
        egui::Color32::WHITE
    } else if is_dir {
        egui::Color32::from_rgb(222, 228, 236)
    } else {
        text_color()
    };

    if is_dir {
        ui.painter().text(
            egui::pos2(icon_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "▾",
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
            if selected {
                egui::Color32::WHITE
            } else {
                muted_color()
            },
        );
    }

    ui.painter().with_clip_rect(text_rect).text(
        egui::pos2(text_rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::new(15.5, egui::FontFamily::Proportional),
        text_color,
    );

    response
}

fn file_tree_rename_row(
    ui: &mut egui::Ui,
    depth: usize,
    is_dir: bool,
    input: &mut String,
) -> Option<RenameRowAction> {
    let width = safe_available_width(ui, 120.0);
    let height = 29.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let indent = 8.0 + depth as f32 * 17.0;
    let icon_x = rect.left() + indent;
    let text_x = icon_x + 18.0;
    if is_dir {
        ui.painter().text(
            egui::pos2(icon_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "▾",
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
            muted_color(),
        );
    }

    let edit_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top() + 2.0),
        egui::pos2(rect.right() - 6.0, rect.bottom() - 2.0),
    );
    let id = ui.make_persistent_id(("file_tree_rename", depth, is_dir));
    let response = ui.put(
        edit_rect,
        TextEdit::singleline(input)
            .id(id)
            .font(egui::TextStyle::Body)
            .desired_width(edit_rect.width().max(1.0)),
    );
    response.request_focus();

    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        return Some(RenameRowAction::Cancel);
    }
    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        return Some(RenameRowAction::Commit);
    }
    if response.lost_focus() && !ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        return Some(RenameRowAction::Commit);
    }
    None
}

fn file_tree_content_width(rows: &[String]) -> f32 {
    rows.iter()
        .map(|row| {
            let depth = file_tree_depth(row) as f32;
            let name = file_tree_name(row);
            let text_width = name.chars().count() as f32 * 8.8;
            58.0 + depth * 17.0 + text_width
        })
        .fold(180.0, f32::max)
        .min(2400.0)
}

fn file_tree_depth(path: &str) -> usize {
    path.trim_end_matches('/').matches('/').count()
}

fn file_tree_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed == "..." {
        return trimmed;
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

fn project_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn project_label(project: &ProjectUiState) -> String {
    if project.display_name.trim().is_empty() {
        project_display_name(&project.path)
    } else {
        project.display_name.trim().to_string()
    }
}

fn ordered_projects(projects: &[ProjectUiState]) -> Vec<ProjectUiState> {
    let mut indexed = projects.iter().cloned().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        right
            .pinned
            .cmp(&left.pinned)
            .then(left_index.cmp(right_index))
    });
    indexed
        .into_iter()
        .map(|(_, project)| project)
        .collect::<Vec<_>>()
}

fn project_count_label(count: usize) -> String {
    let suffix = match count % 100 {
        11..=14 => "проектов",
        _ => match count % 10 {
            1 => "проект",
            2..=4 => "проекта",
            _ => "проектов",
        },
    };
    format!("{count} {suffix}")
}

struct GitCommandResult {
    success: bool,
    display: String,
}

fn run_git_command(workspace: &Workspace, args: &[&str]) -> GitCommandResult {
    let command_line = format!("git {}", args.join(" "));
    match Command::new("git")
        .args(args)
        .current_dir(workspace.root())
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let mut parts = Vec::new();
            if !stdout.is_empty() {
                parts.push(stdout);
            }
            if !stderr.is_empty() {
                parts.push(stderr);
            }
            let body = if parts.is_empty() {
                if output.status.success() {
                    "готово".to_string()
                } else {
                    "команда завершилась без вывода".to_string()
                }
            } else {
                parts.join("\n")
            };
            GitCommandResult {
                success: output.status.success(),
                display: format!("{command_line}\n{body}"),
            }
        }
        Err(err) => GitCommandResult {
            success: false,
            display: format!("{command_line}\nне удалось запустить: {err}"),
        },
    }
}

fn git_changed_files_for_workspace(workspace: &Workspace) -> Vec<String> {
    let Ok(output) = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(workspace.root())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_git_status_path)
        .collect()
}

fn parse_git_status_path(line: &str) -> Option<String> {
    if line.len() < 4 {
        return None;
    }
    let path = if let Some((_, target)) = line[3..].split_once(" -> ") {
        target
    } else {
        &line[3..]
    };
    let path = path.trim().trim_matches('"').replace('\\', "/");
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn tree_row_matches_changed_files(row: &str, changed_files: &[String]) -> bool {
    if row == "..." {
        return true;
    }
    if row.ends_with('/') {
        let dir = row.trim_end_matches('/');
        changed_files
            .iter()
            .any(|changed| path_is_same_or_child(changed, dir))
    } else {
        changed_files.iter().any(|changed| changed == row)
    }
}

fn tree_row_matches_any(row: &str, rows: &[String], predicate: fn(&str) -> bool) -> bool {
    if row == "..." {
        return true;
    }
    if row.ends_with('/') {
        rows.iter()
            .any(|candidate| candidate.starts_with(row) && predicate(candidate))
    } else {
        predicate(row)
    }
}

fn is_code_file_path(path: &str) -> bool {
    matches!(
        file_extension(path).as_deref(),
        Some(
            "rs" | "toml"
                | "lock"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "json"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "swift"
                | "cs"
                | "cpp"
                | "c"
                | "h"
                | "hpp"
                | "gd"
                | "shader"
                | "wgsl"
                | "glsl"
                | "html"
                | "css"
                | "scss"
                | "md"
                | "yml"
                | "yaml"
        )
    )
}

fn is_asset_file_path(path: &str) -> bool {
    path.starts_with("assets/")
        || matches!(
            file_extension(path).as_deref(),
            Some(
                "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "svg"
                    | "ico"
                    | "wav"
                    | "mp3"
                    | "ogg"
                    | "flac"
                    | "mp4"
                    | "mov"
                    | "webm"
                    | "fbx"
                    | "obj"
                    | "gltf"
                    | "glb"
                    | "blend"
            )
        )
}

fn file_extension(path: &str) -> Option<String> {
    Path::new(path.trim_end_matches('/'))
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn project_paths_match(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
}

fn path_key(path: &Path) -> String {
    strip_windows_extended_prefix_for_ui(&path.to_string_lossy().replace('\\', "/"))
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn strip_windows_extended_prefix_for_ui(path: &str) -> String {
    path.strip_prefix("//?/")
        .or_else(|| path.strip_prefix("\\\\?\\"))
        .unwrap_or(path)
        .to_string()
}

fn normalize_tree_dir(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_matches('/').trim();
    if path.is_empty() || path == "." || path.contains("..") {
        String::new()
    } else {
        format!("{path}/")
    }
}

fn file_tree_parent_dirs(path: &str) -> Vec<String> {
    if path == "..." {
        return Vec::new();
    }
    let trimmed = path.trim_end_matches('/');
    let parts = trimmed
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        return Vec::new();
    }

    let mut parents = Vec::new();
    for index in 1..parts.len() {
        parents.push(format!("{}/", parts[..index].join("/")));
    }
    parents
}

fn file_tree_base_path(path: &str) -> &str {
    path.trim_end_matches('/')
}

fn path_is_same_or_child(path: &str, base: &str) -> bool {
    path == base
        || path
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn remap_path_after_base_move(path: &str, old_base: &str, new_base: &str) -> Option<String> {
    if path == old_base {
        return Some(new_base.to_string());
    }
    let old_prefix = format!("{old_base}/");
    path.strip_prefix(&old_prefix)
        .map(|rest| format!("{new_base}/{rest}"))
}

fn remap_tree_path_after_base_move(path: &str, old_base: &str, new_base: &str) -> Option<String> {
    let was_dir = path.ends_with('/');
    remap_path_after_base_move(file_tree_base_path(path), old_base, new_base).map(|updated| {
        if was_dir {
            format!("{updated}/")
        } else {
            updated
        }
    })
}

fn task_tree_value(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn task_status_sort_key(status: &str) -> u8 {
    match status {
        "doing" => 0,
        "blocked" => 1,
        "todo" => 2,
        "done" => 3,
        _ => 4,
    }
}

fn task_priority_sort_key(priority: &str) -> u8 {
    match priority {
        "high" => 0,
        "normal" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn task_status_label(status: &str) -> &'static str {
    match status {
        "doing" => "в работе",
        "done" => "готово",
        "blocked" => "блокер",
        "todo" => "к плану",
        _ => "задача",
    }
}

fn task_priority_label(priority: &str) -> &'static str {
    match priority {
        "high" => "Высокий",
        "low" => "Низкий",
        _ => "Обычный",
    }
}

fn panel_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).strong());
    ui.add(egui::Label::new(RichText::new(subtitle).weak().small()).wrap());
}

fn metric_chip(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display) {
    let value = value.to_string();
    let shown_label = compact_inline(label, 24);
    let shown_value = compact_inline(&value, 12);
    let response = egui::Frame::none()
        .fill(surface_alt_bg())
        .stroke(egui::Stroke::new(1.0, border_color()))
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            // Counters live mostly in the narrow right panel. A two-line fixed
            // mini-card is more stable there than a long "label: value" chip:
            // values align visually, labels do not push neighbours away, and
            // wrapped rows remain predictable while resizing the panel.
            ui.set_min_width(96.0);
            ui.set_max_width(96.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&shown_value)
                        .strong()
                        .size(17.0)
                        .color(accent_color())
                        .text_style(egui::TextStyle::Monospace),
                );
                ui.label(
                    RichText::new(&shown_label)
                        .weak()
                        .small()
                        .color(muted_color()),
                );
            });
        })
        .response;
    if shown_label != label || shown_value != value {
        response.on_hover_text(format!("{label}: {value}"));
    }
}

fn chip(ui: &mut egui::Ui, text: impl Into<String>) {
    let text = text.into();
    let shown = compact_inline(&text, 44);
    let response = egui::Frame::none()
        .fill(surface_alt_bg())
        .stroke(egui::Stroke::new(1.0, border_color()))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(7.0, 3.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(&shown)
                    .small()
                    .color(egui::Color32::from_rgb(202, 211, 222))
                    .text_style(egui::TextStyle::Monospace),
            )
        })
        .inner;
    if shown != text {
        response.on_hover_text(text);
    }
}

fn empty_state(ui: &mut egui::Ui, title: &str, detail: &str) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(18, 21, 26))
        .stroke(egui::Stroke::new(1.0, border_color()))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong().color(muted_color()));
            ui.label(RichText::new(detail).weak().small());
        });
}

fn provider_row(ui: &mut egui::Ui, name: &str, key_present: bool, model: &str, status: String) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(compact_inline(name, 38)).strong())
            .on_hover_text(name);
        chip(ui, format!("ключ: {}", yes_no(key_present)));
        chip(ui, model);
        let shown = compact_inline(&status, 70);
        let response = ui.label(RichText::new(&shown).weak());
        if shown != status {
            response.on_hover_text(status);
        }
    });
}

fn asset_kind_label(kind: &AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "изображение",
        AssetKind::Spritesheet => "спрайт-лист",
        AssetKind::Audio => "аудио",
        AssetKind::Video => "видео",
    }
}

fn asset_kind_for_rel_path(path: &str) -> AssetKind {
    match file_extension(path).as_deref() {
        Some("wav" | "mp3" | "ogg" | "flac" | "opus") => AssetKind::Audio,
        Some("mp4" | "mov" | "webm") => AssetKind::Video,
        Some("json") if path.to_ascii_lowercase().contains("spritesheet") => AssetKind::Spritesheet,
        _ => AssetKind::Image,
    }
}

fn asset_import_targets(kind: &AssetKind) -> &'static [&'static str] {
    match kind {
        AssetKind::Image => &[
            "assets/images",
            "assets/icons",
            "assets/textures",
            "public/assets/images",
            "src/assets/images",
            "Assets/Art",
        ],
        AssetKind::Spritesheet => &[
            "assets/spritesheets",
            "assets/animations",
            "public/assets/spritesheets",
            "src/assets/spritesheets",
            "Assets/Sprites",
        ],
        AssetKind::Audio => &[
            "assets/audio",
            "assets/sfx",
            "public/assets/audio",
            "src/assets/audio",
            "Assets/Audio",
        ],
        AssetKind::Video => &[
            "assets/video",
            "public/assets/video",
            "src/assets/video",
            "Assets/Video",
        ],
    }
}

fn default_asset_import_target(kind: &AssetKind) -> &'static str {
    asset_import_targets(kind)
        .first()
        .copied()
        .unwrap_or("assets")
}

fn asset_status_label(status: &AssetStatus) -> &'static str {
    match status {
        AssetStatus::Pending => "в очереди",
        AssetStatus::Running => "выполняется",
        AssetStatus::Done => "готово",
        AssetStatus::Failed => "ошибка",
    }
}

fn eval_run_status_label(status: &str) -> &'static str {
    match status {
        "passed_static_checks" => "статические проверки пройдены",
        "needs_review" => "нужна проверка",
        _ => "статус неизвестен",
    }
}

fn category_label(category: &str) -> &'static str {
    match category {
        "files" => "файлы",
        "shell" => "shell",
        "terminal" => "терминал",
        "planning" => "планирование",
        "external" => "внешнее",
        "orchestration" => "оркестрация",
        "evals" => "проверки",
        "assets" => "ассеты",
        "desktop" => "рабочий стол",
        "governance" => "доступ",
        "memory" => "память",
        "providers" => "провайдеры",
        _ => "другое",
    }
}

fn risk_label(risk: &str) -> &'static str {
    match risk {
        "low" => "низкий риск",
        "medium" => "средний риск",
        "high" => "высокий риск",
        "paid" => "платный API",
        _ => "риск",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "да"
    } else {
        "нет"
    }
}

fn format_input_message_with_attachments(text: &str, attachments: &[InputAttachment]) -> String {
    let mut message = text.trim().to_string();
    if attachments.is_empty() {
        return message;
    }

    if !message.is_empty() {
        message.push_str("\n\n");
    }
    message.push_str("Вложения к сообщению (пути относительны рабочей папке):\n");
    for attachment in attachments {
        message.push_str(&format!(
            "- {} `{}` — {} ({}).\n",
            input_attachment_kind_label(attachment.kind),
            attachment.path,
            attachment.name,
            format_bytes_short(attachment.bytes)
        ));
    }
    message.push_str(
        "Используй эти пути как контекст; для текстовых файлов можно читать содержимое через read_file, для изображений учитывай, что это приложенные скриншоты/картинки."
    );
    message
}

fn input_attachment_kind_label(kind: InputAttachmentKind) -> &'static str {
    match kind {
        InputAttachmentKind::File => "файл",
        InputAttachmentKind::Image => "изображение",
        InputAttachmentKind::Screenshot => "скриншот",
    }
}

fn format_bytes_short(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{bytes} B")
    }
}

fn sanitize_attachment_file_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if sanitized.is_empty() {
        "attachment.bin".to_string()
    } else {
        sanitized.chars().take(96).collect()
    }
}

fn is_supported_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

#[cfg(target_os = "windows")]
fn platform_ctrl_v_down() -> bool {
    const VK_CONTROL: i32 = 0x11;
    const VK_LCONTROL: i32 = 0xA2;
    const VK_RCONTROL: i32 = 0xA3;
    const VK_V: i32 = 0x56;

    fn key_down(v_key: i32) -> bool {
        unsafe { (GetAsyncKeyState(v_key) as u16 & 0x8000) != 0 }
    }

    (key_down(VK_CONTROL) || key_down(VK_LCONTROL) || key_down(VK_RCONTROL)) && key_down(VK_V)
}

#[cfg(not(target_os = "windows"))]
fn platform_ctrl_v_down() -> bool {
    false
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(v_key: i32) -> i16;
}
fn save_clipboard_image_to_file(target: &Path) -> Result<(), String> {
    match save_clipboard_image_with_arboard(target) {
        Ok(()) => Ok(()),
        Err(arboard_err) => {
            #[cfg(target_os = "windows")]
            {
                save_clipboard_image_with_powershell(target).map_err(|powershell_err| {
                    format!(
                        "в буфере нет доступного изображения: arboard: {arboard_err}; Windows Clipboard: {powershell_err}"
                    )
                })
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(format!("в буфере нет изображения: {arboard_err}"))
            }
        }
    }?;

    let bytes = fs::metadata(target)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    if bytes == 0 {
        return Err("изображение из буфера сохранилось как пустой файл".to_string());
    }
    Ok(())
}

fn save_clipboard_image_with_arboard(target: &Path) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("буфер обмена недоступен: {err}"))?;
    let image = clipboard
        .get_image()
        .map_err(|err| format!("изображение не найдено: {err}"))?;
    image::save_buffer_with_format(
        target,
        image.bytes.as_ref(),
        image.width as u32,
        image.height as u32,
        ColorType::Rgba8,
        ImageFormat::Png,
    )
    .map_err(|err| format!("не удалось сохранить изображение: {err}"))
}

#[cfg(target_os = "windows")]
fn save_clipboard_image_with_powershell(target: &Path) -> Result<(), String> {
    let escaped_path = target.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$path = '{escaped_path}'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
if (-not [System.Windows.Forms.Clipboard]::ContainsImage()) {{
    Write-Error 'clipboard does not contain an image'
    exit 2
}}
$image = [System.Windows.Forms.Clipboard]::GetImage()
$image.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
$image.Dispose()
"#
    );
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|err| format!("не удалось запустить powershell: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        Err(if detail.is_empty() {
            format!("powershell завершился с кодом {}", output.status)
        } else {
            detail
        })
    }
}

fn compact(text: &str, max_chars: usize) -> String {
    let mut compacted = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        compacted.push_str("\n... обрезано ...");
    }
    compacted
}

fn compact_inline(text: &str, max_chars: usize) -> String {
    let mut compacted = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        compacted.push_str("...");
    }
    compacted
}

fn first_useful_response_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("```")
                && !line.starts_with('#')
                && line.chars().count() >= 24
        })
        .next()
        .map(|line| compact_inline(line, 240))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn append_output_tail(target: &mut String, chunk: &str, max_chars: usize) {
    if !target.is_empty() && !target.ends_with('\n') {
        target.push('\n');
    }
    target.push_str(chunk);
    if !target.ends_with('\n') {
        target.push('\n');
    }

    let char_count = target.chars().count();
    if char_count > max_chars {
        let keep_from = char_count.saturating_sub(max_chars);
        *target = target.chars().skip(keep_from).collect::<String>();
        target.insert_str(0, "... ");
    }
}

fn parse_project_exit_code(output: &str) -> Option<i32> {
    for line in output.lines() {
        let lower = line.to_lowercase();
        if !(lower.contains("exit code") || lower.contains("код выхода")) {
            continue;
        }
        let Some((_, tail)) = line.rsplit_once(':') else {
            continue;
        };
        if let Ok(code) = tail.trim().parse::<i32>() {
            return Some(code);
        }
    }

    None
}

fn project_diagnostics(output: &str) -> Vec<ProjectDiagnostic> {
    let lines = output.lines().collect::<Vec<_>>();
    let rust_location = Regex::new(r"^\s*-->\s+(.+):(\d+):(\d+)").expect("valid rust regex");
    let ts_parenthesized = Regex::new(r"^(.+)\((\d+),(\d+)\):\s*(error|warning)\b[^:]*:\s*(.+)$")
        .expect("valid ts parenthesized regex");
    let ts_colon = Regex::new(r"^(.+):(\d+):(\d+)\s+-\s+(error|warning)\b[^:]*:\s*(.+)$")
        .expect("valid ts colon regex");
    let python_file =
        Regex::new(r#"^\s*File "(.+)", line (\d+)"#).expect("valid python file regex");

    let mut diagnostics = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(captures) = rust_location.captures(trimmed) {
            let message = previous_error_line(&lines, index)
                .unwrap_or_else(|| "Rust compiler diagnostic".to_string());
            push_unique_diagnostic(
                &mut diagnostics,
                ProjectDiagnostic {
                    kind: ProjectDiagnosticKind::from_text(&message),
                    file: Some(normalize_diagnostic_path(&captures[1])),
                    line: captures[2].parse().ok(),
                    column: captures[3].parse().ok(),
                    message,
                    raw: diagnostic_raw_window(&lines, index.saturating_sub(1), index + 2),
                },
            );
            continue;
        }

        if let Some(captures) = ts_parenthesized.captures(trimmed) {
            push_unique_diagnostic(
                &mut diagnostics,
                ProjectDiagnostic {
                    kind: ProjectDiagnosticKind::from_text(&captures[4]),
                    file: Some(normalize_diagnostic_path(&captures[1])),
                    line: captures[2].parse().ok(),
                    column: captures[3].parse().ok(),
                    message: captures[5].trim().to_string(),
                    raw: trimmed.to_string(),
                },
            );
            continue;
        }

        if let Some(captures) = ts_colon.captures(trimmed) {
            push_unique_diagnostic(
                &mut diagnostics,
                ProjectDiagnostic {
                    kind: ProjectDiagnosticKind::from_text(&captures[4]),
                    file: Some(normalize_diagnostic_path(&captures[1])),
                    line: captures[2].parse().ok(),
                    column: captures[3].parse().ok(),
                    message: captures[5].trim().to_string(),
                    raw: trimmed.to_string(),
                },
            );
            continue;
        }

        if let Some(captures) = python_file.captures(trimmed) {
            let message = next_error_line(&lines, index)
                .unwrap_or_else(|| "Python runtime diagnostic".to_string());
            push_unique_diagnostic(
                &mut diagnostics,
                ProjectDiagnostic {
                    kind: ProjectDiagnosticKind::from_text(&message),
                    file: Some(normalize_diagnostic_path(&captures[1])),
                    line: captures[2].parse().ok(),
                    column: None,
                    message,
                    raw: diagnostic_raw_window(&lines, index, index + 4),
                },
            );
            continue;
        }

        let lower = trimmed.to_lowercase();
        let is_project_error = lower.contains("error")
            || lower.contains("failed")
            || lower.contains("panic")
            || lower.contains("ошибка")
            || lower.contains("не удалось");
        if is_project_error {
            if lines
                .get(index + 1)
                .is_some_and(|next| rust_location.is_match(next.trim()))
            {
                continue;
            }
            push_unique_diagnostic(
                &mut diagnostics,
                ProjectDiagnostic {
                    kind: ProjectDiagnosticKind::from_text(trimmed),
                    file: None,
                    line: None,
                    column: None,
                    message: trimmed.to_string(),
                    raw: trimmed.to_string(),
                },
            );
        }

        if diagnostics.len() >= 24 {
            break;
        }
    }
    diagnostics
}

fn project_error_summary_from_diagnostics(
    diagnostics: &[ProjectDiagnostic],
    output: &str,
) -> Vec<String> {
    if diagnostics.is_empty() {
        project_error_summary(output)
    } else {
        diagnostics
            .iter()
            .take(12)
            .map(diagnostic_prompt_line)
            .collect()
    }
}

fn project_diagnostics_prompt_block(run: &ProjectRunRecord, limit: usize) -> String {
    if run.diagnostics.is_empty() {
        return if run.error_summary.is_empty() {
            "- структурированная диагностика не найдена".to_string()
        } else {
            run.error_summary
                .iter()
                .take(limit)
                .map(|line| format!("- {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
    }

    run.diagnostics
        .iter()
        .take(limit)
        .map(|diagnostic| format!("- {}", diagnostic_prompt_line(diagnostic)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn diagnostic_prompt_line(diagnostic: &ProjectDiagnostic) -> String {
    match diagnostic_location(diagnostic) {
        Some(location) => format!(
            "{} {}: {}",
            diagnostic.kind.label(),
            location,
            diagnostic.message
        ),
        None => format!("{}: {}", diagnostic.kind.label(), diagnostic.message),
    }
}

fn diagnostic_short_target(diagnostic: &ProjectDiagnostic) -> String {
    match diagnostic_location(diagnostic) {
        Some(location) => format!("{} {}", location, compact_inline(&diagnostic.message, 90)),
        None => compact_inline(&diagnostic.message, 120),
    }
}

fn diagnostic_location(diagnostic: &ProjectDiagnostic) -> Option<String> {
    let file = diagnostic.file.as_ref()?;
    Some(match (diagnostic.line, diagnostic.column) {
        (Some(line), Some(column)) => format!("{file}:{line}:{column}"),
        (Some(line), None) => format!("{file}:{line}"),
        _ => file.clone(),
    })
}

fn push_unique_diagnostic(diagnostics: &mut Vec<ProjectDiagnostic>, diagnostic: ProjectDiagnostic) {
    let key = format!(
        "{}|{:?}|{:?}|{:?}|{}",
        diagnostic.file.as_deref().unwrap_or(""),
        diagnostic.line,
        diagnostic.column,
        diagnostic.kind,
        diagnostic.message
    );
    let exists = diagnostics.iter().any(|existing| {
        format!(
            "{}|{:?}|{:?}|{:?}|{}",
            existing.file.as_deref().unwrap_or(""),
            existing.line,
            existing.column,
            existing.kind,
            existing.message
        ) == key
    });
    if !exists {
        diagnostics.push(diagnostic);
    }
}

fn previous_error_line(lines: &[&str], index: usize) -> Option<String> {
    lines[..index].iter().rev().find_map(|line| {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("error")
            || lower.starts_with("warning")
            || lower.contains("panic")
            || lower.contains("failed")
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn next_error_line(lines: &[&str], index: usize) -> Option<String> {
    lines[index.saturating_add(1)..].iter().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.contains("Error")
            || trimmed.contains("Exception")
            || trimmed.contains("SyntaxError")
            || trimmed.contains("Traceback")
            || trimmed.contains("panic")
            || trimmed.contains("failed")
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn diagnostic_raw_window(lines: &[&str], start: usize, end: usize) -> String {
    lines
        .iter()
        .skip(start)
        .take(end.saturating_sub(start).max(1))
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_diagnostic_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_string()
}

fn project_error_summary(output: &str) -> Vec<String> {
    let mut summary = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        let is_error = lower.contains("error")
            || lower.contains("failed")
            || lower.contains("panic")
            || lower.contains("ошибка")
            || lower.contains("не удалось");
        if is_error && !summary.iter().any(|existing| existing == trimmed) {
            summary.push(trimmed.to_string());
        }
        if summary.len() >= 12 {
            break;
        }
    }
    summary
}

#[cfg(test)]
mod project_diagnostic_tests {
    use super::*;

    #[test]
    fn parses_rust_compiler_location() {
        let output = r#"
error[E0425]: cannot find value `foo` in this scope
 --> src/main.rs:12:9
  |
12 |     foo();
  |     ^^^ not found in this scope
"#;

        let diagnostics = project_diagnostics(output);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(diagnostics[0].line, Some(12));
        assert_eq!(diagnostics[0].column, Some(9));
        assert_eq!(diagnostics[0].kind, ProjectDiagnosticKind::Error);
    }

    #[test]
    fn parses_typescript_parenthesized_location() {
        let output = "src/App.tsx(8,15): error TS2304: Cannot find name 'Widget'.";

        let diagnostics = project_diagnostics(output);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].file.as_deref(), Some("src/App.tsx"));
        assert_eq!(diagnostics[0].line, Some(8));
        assert_eq!(diagnostics[0].column, Some(15));
        assert!(diagnostics[0].message.contains("Cannot find name"));
    }

    #[test]
    fn parses_python_traceback_file_line() {
        let output = r#"
Traceback (most recent call last):
  File "scripts/build.py", line 22, in <module>
    main()
SyntaxError: invalid syntax
"#;

        let diagnostics = project_diagnostics(output);

        assert!(diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.file.as_deref() == Some("scripts/build.py")
                    && diagnostic.line == Some(22)
            ));
    }

    #[test]
    fn detects_requested_backlog_stage_number() {
        assert_eq!(
            requested_stage_number("Реализуй, пожалуйста, этап 18 бэклога"),
            Some(18)
        );
        assert_eq!(
            requested_stage_number("Нужно выполнить 20 stage roadmap"),
            Some(20)
        );
    }

    #[test]
    fn extracts_russian_backlog_stage_section() {
        let backlog = r#"
## Этап 17 - Упаковка

- Done: Installer.

## Этап 18 - Управляемая автономность

- Done: Timeline.
- Todo: Confirmation gate.

## Этап 19 - Roadmap

- Todo: Living roadmap.
"#;

        let section = extract_backlog_stage_section(backlog, 18).expect("stage exists");

        assert!(section.contains("## Этап 18"));
        assert!(section.contains("Confirmation gate"));
        assert!(!section.contains("Living roadmap"));
    }

    #[test]
    fn command_palette_item_matches_title_category_and_description() {
        let item = CommandPaletteItem::new(
            "Релизный preflight",
            "Prompt",
            "Проверить версию, сборку и артефакты перед публикацией.",
            Some("Ctrl+K"),
            CommandPaletteAction::ApplyLayout(LayoutPreset::ReleaseFocus),
            true,
        );

        assert!(item.matches_query("релиз"));
        assert!(item.matches_query("prompt"));
        assert!(item.matches_query("артефакты публикацией"));
        assert!(!item.matches_query("gemini"));
    }
}
