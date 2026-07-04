use crate::run_timeline::{RunTimeline, RunTimelineStatus, RunTimelineStep};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const AGENT_HISTORY_PATH: &str = "assets/generated/leetcode/agent_history.jsonl";

#[derive(Clone, Debug)]
pub struct AgentRunHistoryContext {
    pub id: String,
    pub started_at: u64,
    pub provider: String,
    pub model: String,
    pub route: String,
    pub policy_profile: String,
    pub workspace_name: String,
    pub workspace_root: String,
    pub user_request: String,
    pub confirmed_plan: Option<AgentRunConfirmedPlan>,
}

impl AgentRunHistoryContext {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        route: impl Into<String>,
        policy_profile: impl Into<String>,
        workspace: &Workspace,
        user_request: impl Into<String>,
        confirmed_plan: Option<AgentRunConfirmedPlan>,
    ) -> Self {
        let started_at = unix_timestamp();
        Self {
            id: format!("run-{started_at}-{}", uuid::Uuid::new_v4().simple()),
            started_at,
            provider: provider.into(),
            model: model.into(),
            route: route.into(),
            policy_profile: policy_profile.into(),
            workspace_name: workspace.display_name(),
            workspace_root: workspace.root().to_string_lossy().to_string(),
            user_request: truncate_chars(&user_request.into(), 6_000),
            confirmed_plan,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunHistoryRecord {
    pub schema_version: u32,
    pub id: String,
    pub started_at: u64,
    pub finished_at: u64,
    pub duration_ms: u64,
    pub status: String,
    pub provider: String,
    pub model: String,
    pub route: String,
    pub policy_profile: String,
    pub workspace_name: String,
    pub workspace_root: String,
    pub user_request: String,
    pub confirmed_plan: Option<AgentRunConfirmedPlan>,
    pub final_response: Option<String>,
    pub final_report: Option<String>,
    pub changed_files: Vec<String>,
    pub errors: Vec<String>,
    pub approvals: Vec<AgentRunApproval>,
    pub tool_calls: Vec<AgentRunToolCall>,
    pub timeline_steps: Vec<AgentRunTimelineStep>,
}

impl AgentRunHistoryRecord {
    pub fn from_timeline(
        context: &AgentRunHistoryContext,
        timeline: &RunTimeline,
        final_response: Option<String>,
    ) -> Self {
        let finished_at = unix_timestamp();
        let duration_ms = timeline_duration_ms(timeline);
        let timeline_steps = timeline
            .steps
            .iter()
            .take(300)
            .map(AgentRunTimelineStep::from_step)
            .collect::<Vec<_>>();
        let errors = timeline
            .steps
            .iter()
            .filter(|step| matches!(step.status, RunTimelineStatus::Failed))
            .map(|step| {
                let detail = if step.output.trim().is_empty() {
                    &step.detail
                } else {
                    &step.output
                };
                truncate_chars(detail, 2_000)
            })
            .collect::<Vec<_>>();
        let approvals = timeline
            .steps
            .iter()
            .filter(|step| is_approval_step(step))
            .map(AgentRunApproval::from_step)
            .collect::<Vec<_>>();
        let tool_calls = timeline
            .steps
            .iter()
            .filter(|step| is_tool_step(step))
            .map(AgentRunToolCall::from_step)
            .collect::<Vec<_>>();

        Self {
            schema_version: 1,
            id: context.id.clone(),
            started_at: context.started_at,
            finished_at,
            duration_ms,
            status: timeline_status(timeline),
            provider: context.provider.clone(),
            model: context.model.clone(),
            route: context.route.clone(),
            policy_profile: context.policy_profile.clone(),
            workspace_name: context.workspace_name.clone(),
            workspace_root: context.workspace_root.clone(),
            user_request: context.user_request.clone(),
            confirmed_plan: context.confirmed_plan.clone(),
            final_response: final_response.map(|text| truncate_chars(&text, 8_000)),
            final_report: timeline
                .final_report
                .as_ref()
                .map(|text| truncate_chars(text, 6_000)),
            changed_files: timeline
                .changed_files
                .iter()
                .take(300)
                .map(|path| truncate_chars(path, 500))
                .collect(),
            errors,
            approvals,
            tool_calls,
            timeline_steps,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunConfirmedPlan {
    pub summary: String,
    pub detail: String,
}

impl AgentRunConfirmedPlan {
    pub fn new(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            summary: truncate_chars(&summary.into(), 2_000),
            detail: truncate_chars(&detail.into(), 6_000),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunToolCall {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub status: String,
    pub duration_ms: Option<u64>,
    pub output_preview: String,
}

impl AgentRunToolCall {
    fn from_step(step: &RunTimelineStep) -> Self {
        Self {
            id: step.id.clone(),
            name: step_title_name(step),
            summary: truncate_chars(&step.detail, 3_000),
            status: status_code(&step.status).to_string(),
            duration_ms: step_duration_ms(step),
            output_preview: truncate_chars(&step.output, 4_000),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunApproval {
    pub id: String,
    pub summary: String,
    pub detail: String,
    pub status: String,
}

impl AgentRunApproval {
    fn from_step(step: &RunTimelineStep) -> Self {
        Self {
            id: step.id.clone(),
            summary: truncate_chars(&step.title, 1_000),
            detail: truncate_chars(&step.detail, 4_000),
            status: status_code(&step.status).to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunTimelineStep {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub duration_ms: Option<u64>,
    pub output_preview: String,
    pub link: Option<String>,
}

impl AgentRunTimelineStep {
    fn from_step(step: &RunTimelineStep) -> Self {
        Self {
            id: step.id.clone(),
            title: truncate_chars(&step.title, 1_000),
            detail: truncate_chars(&step.detail, 3_000),
            status: status_code(&step.status).to_string(),
            duration_ms: step_duration_ms(step),
            output_preview: truncate_chars(&step.output, 4_000),
            link: step.link.clone(),
        }
    }
}

pub fn append_agent_history(
    workspace: &Workspace,
    record: &AgentRunHistoryRecord,
) -> anyhow::Result<()> {
    let path = workspace.resolve_for_write(AGENT_HISTORY_PATH)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, record)?;
    writeln!(file)?;
    Ok(())
}

pub fn load_agent_history_tail(workspace: &Workspace, limit: usize) -> Vec<AgentRunHistoryRecord> {
    let Ok(text) = workspace.read_text(AGENT_HISTORY_PATH, 5_000_000) else {
        return Vec::new();
    };

    let mut records = text
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<AgentRunHistoryRecord>(line).ok())
        .take(limit)
        .collect::<Vec<_>>();
    records.reverse();
    records
}

fn timeline_status(timeline: &RunTimeline) -> String {
    if timeline.failed
        || timeline
            .steps
            .iter()
            .any(|step| matches!(step.status, RunTimelineStatus::Failed))
    {
        "failed".to_string()
    } else if timeline
        .steps
        .iter()
        .any(|step| matches!(step.status, RunTimelineStatus::Cancelled))
    {
        "cancelled".to_string()
    } else {
        "succeeded".to_string()
    }
}

fn status_code(status: &RunTimelineStatus) -> &'static str {
    match status {
        RunTimelineStatus::Running => "running",
        RunTimelineStatus::WaitingApproval => "waiting_approval",
        RunTimelineStatus::Succeeded => "succeeded",
        RunTimelineStatus::Failed => "failed",
        RunTimelineStatus::Cancelled => "cancelled",
    }
}

fn timeline_duration_ms(timeline: &RunTimeline) -> u64 {
    let finished_at = timeline.finished_at.unwrap_or_else(Instant::now);
    duration_ms(
        finished_at
            .saturating_duration_since(timeline.started_at)
            .as_millis(),
    )
}

fn step_duration_ms(step: &RunTimelineStep) -> Option<u64> {
    step.finished_after
        .or_else(|| step.started_at.map(|started| started.elapsed()))
        .map(|duration| duration_ms(duration.as_millis()))
}

fn duration_ms(value: u128) -> u64 {
    value.min(u64::MAX as u128) as u64
}

fn is_approval_step(step: &RunTimelineStep) -> bool {
    let title = step.title.to_lowercase();
    title.contains("согласование") || title.contains("approval")
}

fn is_tool_step(step: &RunTimelineStep) -> bool {
    if is_approval_step(step) || is_meta_step(step) {
        return false;
    }
    true
}

fn is_meta_step(step: &RunTimelineStep) -> bool {
    step.id == "planning"
        || step.id.starts_with("journal-")
        || step.id.starts_with("project-run-")
        || step.id.starts_with("orchestration-")
        || step.id.starts_with("eval-")
}

fn step_title_name(step: &RunTimelineStep) -> String {
    let after_colon = step
        .title
        .split_once(':')
        .map(|(_, tail)| tail.trim())
        .filter(|tail| !tail.is_empty());
    truncate_chars(after_colon.unwrap_or(&step.title), 200)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        truncated.push_str("\n... truncated ...");
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_reads_history_tail() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let context = AgentRunHistoryContext::new(
            "openai",
            "gpt-5.5",
            "auto",
            "work",
            &workspace,
            "Проверь проект",
            Some(AgentRunConfirmedPlan::new("Подтверждаю", "План")),
        );
        let mut timeline = RunTimeline::new("Проверь проект");
        timeline.tool_started(
            "tool-1".to_string(),
            "act".to_string(),
            r#"act({"action":"list_files","args":{}})"#.to_string(),
        );
        timeline.tool_finished("tool-1", "ok");
        timeline.finish(&["src/app.rs".to_string()]);

        let record =
            AgentRunHistoryRecord::from_timeline(&context, &timeline, Some("Готово".to_string()));
        append_agent_history(&workspace, &record).expect("append");

        let history = load_agent_history_tail(&workspace, 10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, record.id);
        assert_eq!(history[0].status, "succeeded");
        assert_eq!(history[0].tool_calls.len(), 1);
        assert_eq!(history[0].changed_files, vec!["src/app.rs"]);
    }
}
