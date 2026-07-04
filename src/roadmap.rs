use crate::agent::types::ToolResult;
use crate::memory::load_memory;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub const ROADMAP_PATH: &str = "assets/generated/leetcode/roadmap.json";
const ROADMAP_EXPORT_PATH: &str = "assets/generated/leetcode/roadmap.md";
const MAX_ROADMAP_FILE_BYTES: usize = 2_000_000;
const MAX_BACKLOG_BYTES: usize = 800_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoadmapState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub progress_note: String,
    #[serde(default)]
    pub items: Vec<RoadmapItem>,
    #[serde(default)]
    pub goals: Vec<RoadmapGoal>,
    #[serde(default)]
    pub updated_at: u64,
}

impl Default for RoadmapState {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            title: "Project Roadmap".to_string(),
            focus: String::new(),
            progress_note: String::new(),
            items: Vec::new(),
            goals: default_goals(),
            updated_at: unix_timestamp(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoadmapItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub status: RoadmapStatus,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub date_label: String,
    #[serde(default)]
    pub links: RoadmapLinks,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RoadmapGoal {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RoadmapLinks {
    #[serde(default)]
    pub commits: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub agent_runs: Vec<String>,
    #[serde(default)]
    pub memory_ids: Vec<String>,
    #[serde(default)]
    pub validations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoadmapStatus {
    Done,
    #[default]
    Now,
    Next,
}

impl RoadmapStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Now => "now",
            Self::Next => "next",
        }
    }

    pub fn from_label(value: &str) -> Self {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "done" | "complete" | "completed" | "closed" | "готово" | "закрыто" => {
                Self::Done
            }
            "next" | "todo" | "planned" | "planned_next" | "далее" | "план" => Self::Next,
            _ => Self::Now,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RoadmapSnapshotArgs {
    #[serde(default)]
    pub save_if_missing: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecordMilestoneArgs {
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub commits: Vec<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub agent_run_id: Option<String>,
    #[serde(default)]
    pub validation: Option<String>,
    #[serde(default)]
    pub memory_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateRoadmapItemArgs {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub focus: Option<bool>,
    #[serde(default)]
    pub commits: Vec<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub agent_run_id: Option<String>,
    #[serde(default)]
    pub validation: Option<String>,
    #[serde(default)]
    pub memory_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlanRoadmapItemArgs {
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExportRoadmapArgs {
    #[serde(default)]
    pub path: Option<String>,
}

pub fn load_roadmap(workspace: &Workspace) -> RoadmapState {
    workspace
        .read_text(ROADMAP_PATH, MAX_ROADMAP_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str::<RoadmapState>(&text).ok())
        .map(normalize_roadmap)
        .unwrap_or_else(|| seed_roadmap(workspace))
}

pub fn save_roadmap(workspace: &Workspace, state: &RoadmapState) -> anyhow::Result<()> {
    workspace.write_text(ROADMAP_PATH, &serde_json::to_string_pretty(state)?)
}

pub fn roadmap_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Roadmap: рабочая папка не выбрана.".to_string();
    };
    let roadmap = load_roadmap(workspace);
    let focus = if roadmap.focus.trim().is_empty() {
        "не задан".to_string()
    } else {
        roadmap.focus.clone()
    };
    let current = roadmap
        .items
        .iter()
        .filter(|item| item.status == RoadmapStatus::Now)
        .take(4)
        .map(|item| format!("- {}: {}", item.id, item.title))
        .collect::<Vec<_>>()
        .join("\n");
    let next = roadmap
        .items
        .iter()
        .filter(|item| item.status == RoadmapStatus::Next)
        .take(5)
        .map(|item| format!("- {}: {}", item.id, item.title))
        .collect::<Vec<_>>()
        .join("\n");
    let done = roadmap
        .items
        .iter()
        .filter(|item| item.status == RoadmapStatus::Done)
        .rev()
        .take(3)
        .map(|item| format!("- {}: {}", item.id, item.title))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Roadmap проекта:\nФокус: {focus}\nСейчас:\n{}\nДалее:\n{}\nНедавно закрыто:\n{}\nИспользуй roadmap_snapshot, record_milestone, update_roadmap_item, plan_roadmap_item и export_roadmap, когда пользователь просит зафиксировать этап, изменить дорожную карту или спланировать дальнейшую работу.",
        empty_label(&current),
        empty_label(&next),
        empty_label(&done)
    )
}

pub fn roadmap_snapshot(workspace: &Workspace, args: RoadmapSnapshotArgs) -> ToolResult {
    let roadmap = load_roadmap(workspace);
    if args.save_if_missing && workspace.read_text(ROADMAP_PATH, 1).is_err() {
        if let Err(err) = save_roadmap(workspace, &roadmap) {
            return ToolResult::error(err.to_string());
        }
    }
    ToolResult::ok(
        serde_json::to_string_pretty(&roadmap).unwrap_or_else(|_| "roadmap snapshot".to_string()),
    )
}

pub fn record_milestone(workspace: &Workspace, args: RecordMilestoneArgs) -> ToolResult {
    let title = args.title.trim();
    if title.is_empty() {
        return ToolResult::error("название milestone пустое");
    }

    let mut roadmap = load_roadmap(workspace);
    let now = unix_timestamp();
    let id = args
        .item_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("milestone-{now}"));
    let status = args
        .status
        .as_deref()
        .map(RoadmapStatus::from_label)
        .unwrap_or(RoadmapStatus::Done);
    let mut links = RoadmapLinks::default();
    links.commits = merge_lists(
        args.commits,
        current_git_head(workspace).into_iter().collect(),
    );
    links.files = merge_lists(args.changed_files, current_git_changed_files(workspace));
    if let Some(run_id) = args.agent_run_id.filter(|value| !value.trim().is_empty()) {
        links.agent_runs.push(run_id);
    }
    if let Some(validation) = args.validation.filter(|value| !value.trim().is_empty()) {
        links.validations.push(validation);
    }
    links.memory_ids = dedup(args.memory_ids);

    upsert_item(
        &mut roadmap,
        RoadmapItem {
            id: id.clone(),
            title: title.to_string(),
            detail: args.detail,
            status,
            kind: "milestone".to_string(),
            source: "agent".to_string(),
            date_label: today_label(),
            links,
            updated_at: now,
        },
    );
    if status == RoadmapStatus::Now {
        roadmap.focus = id.clone();
    }
    roadmap.updated_at = now;
    if let Err(err) = save_roadmap(workspace, &roadmap) {
        return ToolResult::error(err.to_string());
    }
    ToolResult::ok(
        serde_json::to_string_pretty(
            &json!({ "roadmap_item_id": id, "status": status.as_str(), "path": ROADMAP_PATH }),
        )
        .unwrap_or_else(|_| "roadmap milestone recorded".to_string()),
    )
}

pub fn update_roadmap_item(workspace: &Workspace, args: UpdateRoadmapItemArgs) -> ToolResult {
    let mut roadmap = load_roadmap(workspace);
    let now = unix_timestamp();
    let Some(item) = roadmap.items.iter_mut().find(|item| item.id == args.id) else {
        return ToolResult::error(format!("roadmap item не найден: {}", args.id));
    };

    if let Some(title) = args.title.filter(|value| !value.trim().is_empty()) {
        item.title = title;
    }
    if let Some(detail) = args.detail {
        item.detail = detail;
    }
    if let Some(status) = args.status {
        item.status = RoadmapStatus::from_label(&status);
    }
    item.links.commits = merge_lists(item.links.commits.clone(), args.commits);
    item.links.files = merge_lists(item.links.files.clone(), args.changed_files);
    if let Some(run_id) = args.agent_run_id.filter(|value| !value.trim().is_empty()) {
        push_unique(&mut item.links.agent_runs, run_id);
    }
    if let Some(validation) = args.validation.filter(|value| !value.trim().is_empty()) {
        push_unique(&mut item.links.validations, validation);
    }
    item.links.memory_ids = merge_lists(item.links.memory_ids.clone(), args.memory_ids);
    item.updated_at = now;
    if args.focus.unwrap_or(false) {
        roadmap.focus = item.id.clone();
        item.status = RoadmapStatus::Now;
    }
    roadmap.updated_at = now;

    if let Err(err) = save_roadmap(workspace, &roadmap) {
        return ToolResult::error(err.to_string());
    }
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({ "roadmap_item_id": args.id, "path": ROADMAP_PATH }))
            .unwrap_or_else(|_| "roadmap item updated".to_string()),
    )
}

pub fn plan_roadmap_item(workspace: &Workspace, args: PlanRoadmapItemArgs) -> ToolResult {
    let title = args.title.trim();
    if title.is_empty() {
        return ToolResult::error("название roadmap item пустое");
    }
    let mut roadmap = load_roadmap(workspace);
    let now = unix_timestamp();
    let status = args
        .status
        .as_deref()
        .map(RoadmapStatus::from_label)
        .unwrap_or(RoadmapStatus::Next);
    let id = args
        .id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("planned-{now}"));
    upsert_item(
        &mut roadmap,
        RoadmapItem {
            id: id.clone(),
            title: title.to_string(),
            detail: args.detail,
            status,
            kind: "planned".to_string(),
            source: "agent".to_string(),
            date_label: if status == RoadmapStatus::Next {
                "далее".to_string()
            } else {
                today_label()
            },
            links: RoadmapLinks::default(),
            updated_at: now,
        },
    );
    if status == RoadmapStatus::Now {
        roadmap.focus = id.clone();
    }
    roadmap.updated_at = now;
    if let Err(err) = save_roadmap(workspace, &roadmap) {
        return ToolResult::error(err.to_string());
    }
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({ "roadmap_item_id": id, "path": ROADMAP_PATH }))
            .unwrap_or_else(|_| "roadmap item planned".to_string()),
    )
}

pub fn export_roadmap(workspace: &Workspace, args: ExportRoadmapArgs) -> ToolResult {
    let roadmap = load_roadmap(workspace);
    let path = args
        .path
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| ROADMAP_EXPORT_PATH.to_string());
    let markdown = render_roadmap_markdown(&roadmap);
    match workspace.write_text(&path, &markdown) {
        Ok(()) => ToolResult::ok(
            serde_json::to_string_pretty(&json!({ "path": path }))
                .unwrap_or_else(|_| "roadmap exported".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn roadmap_markdown_export(
    workspace: &Workspace,
    path: Option<String>,
) -> anyhow::Result<String> {
    let path = path.unwrap_or_else(|| ROADMAP_EXPORT_PATH.to_string());
    let roadmap = load_roadmap(workspace);
    workspace.write_text(&path, &render_roadmap_markdown(&roadmap))?;
    Ok(path)
}

fn normalize_roadmap(mut roadmap: RoadmapState) -> RoadmapState {
    if roadmap.schema_version == 0 {
        roadmap.schema_version = schema_version();
    }
    if roadmap.title.trim().is_empty() {
        roadmap.title = "Project Roadmap".to_string();
    }
    if roadmap.goals.is_empty() {
        roadmap.goals = default_goals();
    }
    if roadmap.updated_at == 0 {
        roadmap.updated_at = unix_timestamp();
    }
    roadmap
}

fn seed_roadmap(workspace: &Workspace) -> RoadmapState {
    let mut roadmap = RoadmapState::default();
    roadmap.title = format!("{} Roadmap", workspace.display_name());
    roadmap.items = parse_backlog_items(workspace);
    roadmap.goals = merge_memory_goals(workspace, default_goals());
    roadmap.focus = roadmap
        .items
        .iter()
        .find(|item| item.status == RoadmapStatus::Now)
        .map(|item| item.id.clone())
        .unwrap_or_default();
    roadmap.progress_note =
        "Живая история проекта: завершённые этапы, текущий фокус, будущие задачи и финальные цели."
            .to_string();
    roadmap.updated_at = unix_timestamp();
    roadmap
}

fn parse_backlog_items(workspace: &Workspace) -> Vec<RoadmapItem> {
    let Ok(text) = workspace.read_text("BACKLOG.md", MAX_BACKLOG_BYTES) else {
        return default_items();
    };
    let mut stages = Vec::<ParsedStage>::new();
    let mut current: Option<ParsedStage> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some((id, title)) = parse_stage_header(trimmed) {
            if let Some(stage) = current.take() {
                stages.push(stage);
            }
            current = Some(ParsedStage {
                id,
                title,
                detail: String::new(),
                has_todo: false,
                has_done: false,
            });
        } else if let Some(stage) = current.as_mut() {
            if stage.detail.is_empty()
                && (trimmed.starts_with("Цель:")
                    || trimmed.starts_with("Goal:")
                    || trimmed.starts_with("Цель"))
            {
                stage.detail = trimmed.to_string();
            }
            if trimmed.starts_with("- Todo:") {
                stage.has_todo = true;
            }
            if trimmed.starts_with("- Done:") {
                stage.has_done = true;
            }
        }
    }
    if let Some(stage) = current.take() {
        stages.push(stage);
    }

    if stages.is_empty() {
        return default_items();
    }

    let first_open_index = stages.iter().position(|stage| stage.has_todo);
    stages
        .into_iter()
        .enumerate()
        .map(|(index, stage)| {
            let status = if !stage.has_todo && stage.has_done {
                RoadmapStatus::Done
            } else if Some(index) == first_open_index {
                RoadmapStatus::Now
            } else {
                RoadmapStatus::Next
            };
            RoadmapItem {
                id: stage.id,
                title: stage.title,
                detail: stage.detail,
                status,
                kind: "stage".to_string(),
                source: "BACKLOG.md".to_string(),
                date_label: if status == RoadmapStatus::Next {
                    "далее".to_string()
                } else {
                    String::new()
                },
                links: RoadmapLinks::default(),
                updated_at: unix_timestamp(),
            }
        })
        .collect()
}

fn parse_stage_header(line: &str) -> Option<(String, String)> {
    let header = line.strip_prefix("## ")?;
    if let Some(rest) = header.strip_prefix("Stage ") {
        let (number, title) = split_stage_number(rest)?;
        return Some((
            format!("stage-{number}"),
            format!("Stage {number} - {title}"),
        ));
    }
    if let Some(rest) = header.strip_prefix("Этап ") {
        let (number, title) = split_stage_number(rest)?;
        return Some((
            format!("stage-{number}"),
            format!("Stage {number} - {title}"),
        ));
    }
    None
}

fn split_stage_number(rest: &str) -> Option<(String, String)> {
    let mut parts = rest.splitn(2, ['-', '–', '—']);
    let number = parts.next()?.trim();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let title = parts.next().unwrap_or("").trim();
    Some((number.to_string(), title.to_string()))
}

fn merge_memory_goals(workspace: &Workspace, mut goals: Vec<RoadmapGoal>) -> Vec<RoadmapGoal> {
    let memory = load_memory(workspace);
    for goal in memory.goals.iter().rev().take(8) {
        if goals.iter().any(|existing| existing.title == goal.title) {
            continue;
        }
        goals.push(RoadmapGoal {
            id: format!("memory-{}", goal.id),
            title: goal.title.clone(),
            status: goal.status.clone(),
            notes: goal.notes.clone(),
            updated_at: goal.updated_at,
        });
    }
    goals
}

fn default_items() -> Vec<RoadmapItem> {
    let now = unix_timestamp();
    vec![
        RoadmapItem {
            id: "stage-22".to_string(),
            title: "Stage 22 - Живая дорожная карта проекта".to_string(),
            detail: "Структурированное состояние roadmap, связи с историей агента и управляемые milestone.".to_string(),
            status: RoadmapStatus::Now,
            kind: "stage".to_string(),
            source: "default".to_string(),
            date_label: "сейчас".to_string(),
            links: RoadmapLinks::default(),
            updated_at: now,
        },
        RoadmapItem {
            id: "stage-23".to_string(),
            title: "Stage 23 - Проводник истории агента".to_string(),
            detail: "Поиск, фильтры, отчёты и действия над сохранёнными agent runs.".to_string(),
            status: RoadmapStatus::Next,
            kind: "stage".to_string(),
            source: "default".to_string(),
            date_label: "далее".to_string(),
            links: RoadmapLinks::default(),
            updated_at: now,
        },
    ]
}

fn default_goals() -> Vec<RoadmapGoal> {
    let now = unix_timestamp();
    [
        "Локальный AI-агент для разработки игр и приложений.",
        "Мультипровайдерные модели: код, текст, изображения, звук и видео.",
        "Самоулучшающийся агент с безопасной валидацией и понятной историей проекта.",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, title)| RoadmapGoal {
        id: format!("goal-{}", index + 1),
        title: title.to_string(),
        status: "active".to_string(),
        notes: String::new(),
        updated_at: now,
    })
    .collect()
}

fn upsert_item(roadmap: &mut RoadmapState, item: RoadmapItem) {
    if let Some(existing) = roadmap
        .items
        .iter_mut()
        .find(|existing| existing.id == item.id)
    {
        *existing = item;
    } else {
        roadmap.items.push(item);
    }
}

fn current_git_head(workspace: &Workspace) -> Option<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--short")
        .arg("HEAD")
        .current_dir(workspace.root())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn current_git_changed_files(workspace: &Workspace) -> Vec<String> {
    let output = Command::new("git")
        .arg("status")
        .arg("--short")
        .current_dir(workspace.root())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() < 4 {
                return None;
            }
            Some(trimmed[3..].trim().replace('\\', "/"))
        })
        .collect::<Vec<_>>()
}

fn render_roadmap_markdown(roadmap: &RoadmapState) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", roadmap.title));
    if !roadmap.focus.trim().is_empty() {
        out.push_str(&format!("Current focus: `{}`\n\n", roadmap.focus));
    }
    if !roadmap.progress_note.trim().is_empty() {
        out.push_str(&format!("{}\n\n", roadmap.progress_note));
    }

    for (status, heading) in [
        (RoadmapStatus::Now, "Now"),
        (RoadmapStatus::Next, "Next"),
        (RoadmapStatus::Done, "Done"),
    ] {
        out.push_str(&format!("## {heading}\n\n"));
        let rows = roadmap
            .items
            .iter()
            .filter(|item| item.status == status)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            out.push_str("- none\n\n");
            continue;
        }
        for item in rows {
            out.push_str(&format!("- **{}** (`{}`)", item.title, item.id));
            if !item.detail.trim().is_empty() {
                out.push_str(&format!(" — {}", item.detail.trim()));
            }
            let links = render_links(&item.links);
            if !links.is_empty() {
                out.push_str(&format!(" _{}_ ", links));
            }
            out.push('\n');
        }
        out.push('\n');
    }

    out.push_str("## Final Goals\n\n");
    for goal in &roadmap.goals {
        out.push_str(&format!("- [{}] {}", goal.status, goal.title));
        if !goal.notes.trim().is_empty() {
            out.push_str(&format!(" — {}", goal.notes.trim()));
        }
        out.push('\n');
    }
    out
}

fn render_links(links: &RoadmapLinks) -> String {
    let mut parts = Vec::new();
    if !links.commits.is_empty() {
        parts.push(format!("commits: {}", links.commits.join(", ")));
    }
    if !links.files.is_empty() {
        parts.push(format!("files: {}", links.files.join(", ")));
    }
    if !links.agent_runs.is_empty() {
        parts.push(format!("runs: {}", links.agent_runs.join(", ")));
    }
    if !links.validations.is_empty() {
        parts.push(format!("validation: {}", links.validations.join(", ")));
    }
    parts.join("; ")
}

fn merge_lists(mut first: Vec<String>, second: Vec<String>) -> Vec<String> {
    for value in second {
        push_unique(&mut first, value);
    }
    dedup(first)
}

fn dedup(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        push_unique(&mut out, value);
    }
    out
}

fn push_unique(values: &mut Vec<String>, value: String) {
    let value = value.trim().to_string();
    if !value.is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn empty_label(text: &str) -> String {
    if text.trim().is_empty() {
        "- нет".to_string()
    } else {
        text.to_string()
    }
}

fn today_label() -> String {
    let output = Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--format=%cs")
        .output()
        .ok()
        .and_then(|output| {
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .filter(|value| !value.is_empty());
    output.unwrap_or_else(|| "сейчас".to_string())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn schema_version() -> u32 {
    1
}

#[derive(Clone, Debug)]
struct ParsedStage {
    id: String,
    title: String,
    detail: String,
    has_todo: bool,
    has_done: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(dir.path().to_path_buf()).expect("workspace");
        (dir, workspace)
    }

    #[test]
    fn seeds_roadmap_from_backlog() {
        let (_dir, workspace) = temp_workspace();
        workspace
            .write_text(
                "BACKLOG.md",
                r#"# Backlog

## Этап 1 - Foundation

- Done: ready.

## Этап 2 - Roadmap

Цель: сделать roadmap живым.

- Todo: structured state.
"#,
            )
            .expect("write backlog");

        let roadmap = load_roadmap(&workspace);

        assert_eq!(roadmap.items.len(), 2);
        assert_eq!(roadmap.items[0].status, RoadmapStatus::Done);
        assert_eq!(roadmap.items[1].status, RoadmapStatus::Now);
        assert_eq!(roadmap.focus, "stage-2");
    }

    #[test]
    fn records_and_exports_milestone() {
        let (_dir, workspace) = temp_workspace();
        let result = record_milestone(
            &workspace,
            RecordMilestoneArgs {
                title: "Stage 22 MVP".to_string(),
                detail: "Structured roadmap".to_string(),
                item_id: Some("stage-22".to_string()),
                status: Some("done".to_string()),
                commits: vec!["abc123".to_string()],
                changed_files: vec!["src/roadmap.rs".to_string()],
                agent_run_id: Some("run-1".to_string()),
                validation: Some("cargo test".to_string()),
                memory_ids: vec!["goal-1".to_string()],
            },
        );

        assert!(result.ok);
        let roadmap = load_roadmap(&workspace);
        let item = roadmap
            .items
            .iter()
            .find(|item| item.id == "stage-22")
            .expect("item");
        assert_eq!(item.status, RoadmapStatus::Done);
        assert!(item.links.files.contains(&"src/roadmap.rs".to_string()));

        let path = roadmap_markdown_export(&workspace, None).expect("export");
        let exported = workspace.read_text(&path, 100_000).expect("read export");
        assert!(exported.contains("Stage 22 MVP"));
    }

    #[test]
    fn save_if_missing_persists_seed() {
        let (dir, workspace) = temp_workspace();
        let result = roadmap_snapshot(
            &workspace,
            RoadmapSnapshotArgs {
                save_if_missing: true,
            },
        );

        assert!(result.ok);
        assert!(dir
            .path()
            .join("assets/generated/leetcode/roadmap.json")
            .exists());
        let text = fs::read_to_string(dir.path().join("assets/generated/leetcode/roadmap.json"))
            .expect("read roadmap");
        assert!(text.contains("Roadmap"));
    }
}
