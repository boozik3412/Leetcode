use crate::agent::types::ToolResult;
use crate::workspace::Workspace;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MEMORY_PATH: &str = "assets/generated/leetcode/memory.json";
const MEMORY_SOURCE_DIR: &str = "assets/generated/leetcode/memory_sources";
const MAX_MEMORY_FILE_BYTES: usize = 4_000_000;
const MAX_SOURCE_FILE_BYTES: usize = 500_000;
const MAX_STORED_SOURCE_CHARS: usize = 80_000;
const MAX_PROMPT_SOURCE_CHARS: usize = 700;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProjectMemory {
    #[serde(default)]
    pub goals: Vec<ProjectGoal>,
    #[serde(default)]
    pub tasks: Vec<ProjectTask>,
    #[serde(default)]
    pub decisions: Vec<ProjectDecision>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub important_files: Vec<String>,
    #[serde(default)]
    pub important_assets: Vec<String>,
    #[serde(default)]
    pub sources: Vec<MemorySource>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGoal {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub status: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectTask {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub workstream: String,
    #[serde(default)]
    pub milestone: String,
    #[serde(default)]
    pub priority: String,
    pub notes: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectDecision {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemorySource {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub summary: String,
    pub content: String,
    pub content_chars: usize,
    pub stored_path: Option<String>,
    pub original_path: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpsertTaskArgs {
    pub id: Option<String>,
    pub title: String,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub workstream: Option<String>,
    pub milestone: Option<String>,
    pub priority: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskStatusArgs {
    pub id: String,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordDecisionArgs {
    pub title: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordProjectGoalArgs {
    pub title: String,
    pub notes: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordMemorySourceArgs {
    pub id: Option<String>,
    pub title: String,
    pub kind: Option<String>,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveMemorySourceArgs {
    pub id: String,
}

pub fn load_memory(workspace: &Workspace) -> ProjectMemory {
    workspace
        .read_text(MEMORY_PATH, MAX_MEMORY_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_memory(workspace: &Workspace, memory: &ProjectMemory) -> anyhow::Result<()> {
    workspace.write_text(MEMORY_PATH, &serde_json::to_string_pretty(memory)?)
}

pub fn memory_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Память проекта: рабочая папка не выбрана.".to_string();
    };
    let memory = load_memory(workspace);
    if memory.goals.is_empty()
        && memory.tasks.is_empty()
        && memory.decisions.is_empty()
        && memory.open_questions.is_empty()
        && memory.sources.is_empty()
    {
        return "Память проекта: пусто.".to_string();
    }

    let goals = memory
        .goals
        .iter()
        .rev()
        .take(3)
        .map(|goal| format!("- [{}] {}", goal.status, goal.title))
        .collect::<Vec<_>>()
        .join("\n");
    let tasks = memory
        .tasks
        .iter()
        .filter(|task| task.status != "done")
        .take(6)
        .map(|task| {
            format!(
                "- {} [{}; {}; {}; {}]",
                task.title,
                task.status,
                task_field_or(&task.workstream, "Разработка"),
                task_field_or(&task.milestone, "Текущий этап"),
                task_field_or(&task.priority, "normal"),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let decisions = memory
        .decisions
        .iter()
        .rev()
        .take(4)
        .map(|decision| format!("- {}", decision.title))
        .collect::<Vec<_>>()
        .join("\n");
    let sources = memory
        .sources
        .iter()
        .rev()
        .take(5)
        .map(|source| {
            let summary = if source.summary.trim().is_empty() {
                compact_inline(&source.content, MAX_PROMPT_SOURCE_CHARS)
            } else {
                compact_inline(&source.summary, MAX_PROMPT_SOURCE_CHARS)
            };
            format!("- {} [{}]: {}", source.title, source.kind, summary)
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Память проекта:\nЦели:\n{}\nОткрытые задачи:\n{}\nПоследние решения:\n{}\nИсточники контекста:\n{}",
        empty_label(&goals),
        empty_label(&tasks),
        empty_label(&decisions),
        empty_label(&sources)
    )
}

pub fn memory_snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&load_memory(workspace))
            .unwrap_or_else(|_| "память проекта".to_string()),
    )
}

pub fn upsert_task(workspace: &Workspace, args: UpsertTaskArgs) -> ToolResult {
    let title = args.title.trim();
    if title.is_empty() {
        return ToolResult::error("название задачи пустое");
    }
    let mut memory = load_memory(workspace);
    let now = unix_timestamp();
    let id = args
        .id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));
    let status = normalize_status(args.status.as_deref().unwrap_or("todo"));
    let workstream = args
        .workstream
        .as_deref()
        .map(|value| normalize_task_group(value, "Разработка"));
    let milestone = args
        .milestone
        .as_deref()
        .map(|value| normalize_task_group(value, "Текущий этап"));
    let priority = args.priority.as_deref().map(normalize_priority);
    if let Some(task) = memory.tasks.iter_mut().find(|task| task.id == id) {
        task.title = title.to_string();
        task.status = status;
        task.workstream =
            workstream.unwrap_or_else(|| task_field_or(&task.workstream, "Разработка"));
        task.milestone =
            milestone.unwrap_or_else(|| task_field_or(&task.milestone, "Текущий этап"));
        task.priority = priority.unwrap_or_else(|| task_field_or(&task.priority, "normal"));
        task.notes = args.notes.unwrap_or_default();
        task.updated_at = now;
    } else {
        memory.tasks.push(ProjectTask {
            id: id.clone(),
            title: title.to_string(),
            status,
            workstream: workstream.unwrap_or_else(|| "Разработка".to_string()),
            milestone: milestone.unwrap_or_else(|| "Текущий этап".to_string()),
            priority: priority.unwrap_or_else(|| "normal".to_string()),
            notes: args.notes.unwrap_or_default(),
            updated_at: now,
        });
    }
    memory.updated_at = now;
    save_result(workspace, memory, json!({ "task_id": id }))
}

pub fn update_task_status(workspace: &Workspace, args: UpdateTaskStatusArgs) -> ToolResult {
    let mut memory = load_memory(workspace);
    let now = unix_timestamp();
    let status = normalize_status(&args.status);
    let Some(task) = memory.tasks.iter_mut().find(|task| task.id == args.id) else {
        return ToolResult::error(format!("задача не найдена: {}", args.id));
    };
    task.status = status;
    if let Some(notes) = args.notes {
        task.notes = notes;
    }
    task.updated_at = now;
    memory.updated_at = now;
    save_result(workspace, memory, json!({ "task_id": args.id }))
}

pub fn record_decision(workspace: &Workspace, args: RecordDecisionArgs) -> ToolResult {
    let title = args.title.trim();
    if title.is_empty() {
        return ToolResult::error("название решения пустое");
    }
    let mut memory = load_memory(workspace);
    let now = unix_timestamp();
    let id = format!("decision-{}", uuid::Uuid::new_v4());
    memory.decisions.push(ProjectDecision {
        id: id.clone(),
        title: title.to_string(),
        rationale: args.rationale.unwrap_or_default(),
        updated_at: now,
    });
    memory.updated_at = now;
    save_result(workspace, memory, json!({ "decision_id": id }))
}

pub fn record_project_goal(workspace: &Workspace, args: RecordProjectGoalArgs) -> ToolResult {
    let title = args.title.trim();
    if title.is_empty() {
        return ToolResult::error("название цели пустое");
    }
    let mut memory = load_memory(workspace);
    let now = unix_timestamp();
    let id = format!("goal-{}", uuid::Uuid::new_v4());
    memory.goals.push(ProjectGoal {
        id: id.clone(),
        title: title.to_string(),
        notes: args.notes.unwrap_or_default(),
        status: normalize_status(args.status.as_deref().unwrap_or("todo")),
        updated_at: now,
    });
    memory.updated_at = now;
    save_result(workspace, memory, json!({ "goal_id": id }))
}

pub fn record_memory_source(workspace: &Workspace, args: RecordMemorySourceArgs) -> ToolResult {
    match record_memory_source_inner(workspace, args) {
        Ok(payload) => ToolResult::ok(
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "источник памяти обновлён".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn import_memory_source_file(
    workspace: &Workspace,
    source_path: &Path,
    title: Option<String>,
    summary: Option<String>,
) -> ToolResult {
    match import_memory_source_file_inner(workspace, source_path, title, summary) {
        Ok(payload) => ToolResult::ok(
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "источник памяти импортирован".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn remove_memory_source(workspace: &Workspace, args: RemoveMemorySourceArgs) -> ToolResult {
    let mut memory = load_memory(workspace);
    let before = memory.sources.len();
    memory.sources.retain(|source| source.id != args.id);
    if memory.sources.len() == before {
        return ToolResult::error(format!("источник не найден: {}", args.id));
    }
    memory.updated_at = unix_timestamp();
    save_result(workspace, memory, json!({ "removed_source_id": args.id }))
}

fn record_memory_source_inner(
    workspace: &Workspace,
    args: RecordMemorySourceArgs,
) -> anyhow::Result<serde_json::Value> {
    let title = args.title.trim();
    if title.is_empty() {
        anyhow::bail!("название источника пустое");
    }

    let path = args.path.map(|path| path.trim().to_string());
    let content = match (args.content, path.as_deref()) {
        (Some(content), _) if !content.trim().is_empty() => content,
        (_, Some(path)) if !path.is_empty() => workspace.read_text(path, MAX_SOURCE_FILE_BYTES)?,
        _ => anyhow::bail!("для источника нужен content или path"),
    };

    let now = unix_timestamp();
    let id = args
        .id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("source-{}", uuid::Uuid::new_v4()));
    let source = build_source(
        id.clone(),
        title.to_string(),
        args.kind.unwrap_or_else(|| "note".to_string()),
        args.summary.unwrap_or_default(),
        content,
        path.clone(),
        path,
        now,
    );

    let mut memory = load_memory(workspace);
    if let Some(existing) = memory.sources.iter_mut().find(|source| source.id == id) {
        *existing = source;
    } else {
        memory.sources.push(source);
    }
    memory.updated_at = now;
    save_memory(workspace, &memory)?;
    Ok(json!({ "source_id": id }))
}

fn import_memory_source_file_inner(
    workspace: &Workspace,
    source_path: &Path,
    title: Option<String>,
    summary: Option<String>,
) -> anyhow::Result<serde_json::Value> {
    if !source_path.is_file() {
        anyhow::bail!("источник не файл: {}", source_path.display());
    }
    let bytes = fs::read(source_path)
        .with_context(|| format!("не удалось прочитать {}", source_path.display()))?;
    if bytes.len() > MAX_SOURCE_FILE_BYTES {
        anyhow::bail!(
            "файл слишком большой: {} bytes, лимит {} bytes",
            bytes.len(),
            MAX_SOURCE_FILE_BYTES
        );
    }
    let content = String::from_utf8(bytes)
        .with_context(|| format!("файл не похож на UTF-8 текст: {}", source_path.display()))?;
    let now = unix_timestamp();
    let id = format!("source-{}", uuid::Uuid::new_v4());
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source.txt");
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.trim().is_empty())
        .map(|extension| format!(".{}", extension))
        .unwrap_or_else(|| ".txt".to_string());
    let stored_rel = format!(
        "{}/{}-{}{}",
        MEMORY_SOURCE_DIR,
        slugify(file_name),
        id.trim_start_matches("source-")
            .chars()
            .take(8)
            .collect::<String>(),
        extension
    );
    let stored_path = workspace.resolve_for_write(&stored_rel)?;
    if let Some(parent) = stored_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_path, &stored_path).with_context(|| {
        format!(
            "не удалось скопировать источник в {}",
            stored_path.display()
        )
    })?;

    let fallback_title = source_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name)
        .to_string();
    let source = build_source(
        id.clone(),
        title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(fallback_title),
        "file".to_string(),
        summary.unwrap_or_default(),
        content,
        Some(stored_rel.clone()),
        Some(source_path.display().to_string()),
        now,
    );

    let mut memory = load_memory(workspace);
    memory.sources.push(source);
    memory.updated_at = now;
    save_memory(workspace, &memory)?;
    Ok(json!({ "source_id": id, "stored_path": stored_rel }))
}

fn build_source(
    id: String,
    title: String,
    kind: String,
    summary: String,
    content: String,
    stored_path: Option<String>,
    original_path: Option<String>,
    now: u64,
) -> MemorySource {
    let content_chars = content.chars().count();
    MemorySource {
        id,
        title: title.trim().to_string(),
        kind: normalize_source_kind(&kind),
        summary: summary.trim().to_string(),
        content: compact(&content, MAX_STORED_SOURCE_CHARS),
        content_chars,
        stored_path,
        original_path,
        created_at: now,
        updated_at: now,
    }
}

fn save_result(
    workspace: &Workspace,
    memory: ProjectMemory,
    payload: serde_json::Value,
) -> ToolResult {
    match save_memory(workspace, &memory) {
        Ok(()) => ToolResult::ok(
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "память обновлена".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

fn normalize_status(status: &str) -> String {
    match status
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "doing" | "in_progress" | "active" => "doing".to_string(),
        "blocked" => "blocked".to_string(),
        "done" | "complete" | "completed" => "done".to_string(),
        _ => "todo".to_string(),
    }
}

fn normalize_task_group(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn normalize_priority(priority: &str) -> String {
    match priority.trim().to_ascii_lowercase().as_str() {
        "high" | "critical" | "urgent" | "p0" | "p1" | "высокий" | "критичный" | "срочно" => {
            "high".to_string()
        }
        "low" | "p3" | "p4" | "низкий" => "low".to_string(),
        "normal" | "medium" | "p2" | "обычный" | "средний" | "" => {
            "normal".to_string()
        }
        other => other.chars().take(32).collect(),
    }
}

fn task_field_or(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_source_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "file" | "document" | "doc" => "file".to_string(),
        "link" | "url" => "link".to_string(),
        "spec" | "requirements" => "spec".to_string(),
        "note" | "text" | "fact" | "" => "note".to_string(),
        other => other.chars().take(32).collect(),
    }
}

fn empty_label(value: &str) -> &str {
    if value.trim().is_empty() {
        "- нет"
    } else {
        value
    }
}

fn compact(text: &str, max_chars: usize) -> String {
    let mut compacted = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        compacted.push_str("\n... trimmed ...");
    }
    compacted
}

fn compact_inline(text: &str, max_chars: usize) -> String {
    compact(text, max_chars)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn slugify(text: &str) -> String {
    let mut slug = text
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '.') {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug = slug.trim_matches('-').chars().take(48).collect();
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_tasks() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let result = upsert_task(
            &workspace,
            UpsertTaskArgs {
                id: None,
                title: "Build launcher".to_string(),
                status: Some("doing".to_string()),
                notes: None,
                workstream: Some("Desktop".to_string()),
                milestone: Some("MVP".to_string()),
                priority: Some("high".to_string()),
            },
        );

        assert!(result.ok);
        let memory = load_memory(&workspace);
        assert_eq!(memory.tasks.len(), 1);
        assert_eq!(memory.tasks[0].workstream, "Desktop");
        assert_eq!(memory.tasks[0].milestone, "MVP");
        assert_eq!(memory.tasks[0].priority, "high");
    }

    #[test]
    fn records_memory_source_from_content() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let result = record_memory_source(
            &workspace,
            RecordMemorySourceArgs {
                id: None,
                title: "Combat rules".to_string(),
                kind: Some("spec".to_string()),
                summary: Some("Core combat constraints".to_string()),
                content: Some("Damage is deterministic for the first prototype.".to_string()),
                path: None,
            },
        );

        assert!(result.ok);
        let memory = load_memory(&workspace);
        assert_eq!(memory.sources.len(), 1);
        assert_eq!(memory.sources[0].kind, "spec");
        assert!(memory_summary_for_prompt(Some(&workspace)).contains("Combat rules"));
    }

    #[test]
    fn imports_memory_source_file() {
        let temp = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let source_path = external.path().join("brief.md");
        fs::write(&source_path, "# Brief\nUse a cozy art style.").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let result = import_memory_source_file(&workspace, &source_path, None, None);

        assert!(result.ok);
        let memory = load_memory(&workspace);
        assert_eq!(memory.sources.len(), 1);
        let stored_path = memory.sources[0].stored_path.as_ref().unwrap();
        assert!(temp.path().join(stored_path).is_file());
    }
}
