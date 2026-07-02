use crate::agent::types::ToolResult;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

const MEMORY_PATH: &str = "assets/generated/leetcode/memory.json";

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

#[derive(Debug, Deserialize)]
pub struct UpsertTaskArgs {
    pub id: Option<String>,
    pub title: String,
    pub status: Option<String>,
    pub notes: Option<String>,
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

pub fn load_memory(workspace: &Workspace) -> ProjectMemory {
    workspace
        .read_text(MEMORY_PATH, 1_000_000)
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
        .map(|task| format!("- {} [{}]", task.title, task.status))
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

    format!(
        "Память проекта:\nЦели:\n{}\nОткрытые задачи:\n{}\nПоследние решения:\n{}",
        empty_label(&goals),
        empty_label(&tasks),
        empty_label(&decisions)
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
    if let Some(task) = memory.tasks.iter_mut().find(|task| task.id == id) {
        task.title = title.to_string();
        task.status = status;
        task.notes = args.notes.unwrap_or_default();
        task.updated_at = now;
    } else {
        memory.tasks.push(ProjectTask {
            id: id.clone(),
            title: title.to_string(),
            status,
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

fn empty_label(value: &str) -> &str {
    if value.trim().is_empty() {
        "- нет"
    } else {
        value
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
            },
        );

        assert!(result.ok);
        assert_eq!(load_memory(&workspace).tasks.len(), 1);
    }
}
