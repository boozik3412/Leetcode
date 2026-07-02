use crate::assets::load_jobs;
use crate::project::{describe_project_commands, detect_project_profiles};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

const ORCHESTRATION_PATH: &str = "assets/generated/orchestration/orchestration.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    CodeAgent,
    GameDesigner,
    ArtDirector,
    AudioAgent,
    QaAgent,
    BuildAgent,
}

#[derive(Clone, Copy, Debug)]
pub struct AgentRoleSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub purpose: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub id: String,
    pub from: String,
    pub to: AgentRole,
    pub task: String,
    pub context: String,
    pub expected_output: String,
    pub recommended_tools: Vec<String>,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SharedWorkspaceContext {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub decisions: Vec<String>,
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
pub struct RunSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub completed: Vec<String>,
    pub next_steps: Vec<String>,
    pub risks: Vec<String>,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub struct SubagentRunDraft {
    pub role: AgentRole,
    pub task: String,
    pub context: String,
    pub status: String,
    pub output: String,
    pub tool_calls: Vec<String>,
    pub denied_tools: Vec<String>,
    pub rounds: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubagentRun {
    pub id: String,
    pub role: AgentRole,
    pub task: String,
    pub context: String,
    pub status: String,
    pub output: String,
    pub tool_calls: Vec<String>,
    pub denied_tools: Vec<String>,
    pub rounds: usize,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayEvalCase {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub expected_tools: Vec<String>,
    pub success_criteria: Vec<String>,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OrchestrationState {
    #[serde(default)]
    pub context: SharedWorkspaceContext,
    #[serde(default)]
    pub handoffs: Vec<HandoffRecord>,
    #[serde(default)]
    pub subagent_runs: Vec<SubagentRun>,
    #[serde(default)]
    pub run_summaries: Vec<RunSummary>,
    #[serde(default)]
    pub evals: Vec<ReplayEvalCase>,
}

pub fn agent_role_specs() -> &'static [AgentRoleSpec] {
    &[
        AgentRoleSpec {
            id: "code_agent",
            label: "Код-агент",
            purpose: "Реализует код, рефакторинг, тесты и проектные команды.",
        },
        AgentRoleSpec {
            id: "game_designer",
            label: "Гейм-дизайнер",
            purpose: "Превращает механики, циклы, уровни и цели игрока в реализуемые планы.",
        },
        AgentRoleSpec {
            id: "art_director",
            label: "Арт-директор",
            purpose: "Ведёт визуальный стиль, спрайты, иконки, UI и целостность ассетов.",
        },
        AgentRoleSpec {
            id: "audio_agent",
            label: "Аудио-агент",
            purpose:
                "Планирует и генерирует UI/game-звуки, музыкальные акценты и голосовые промпты.",
        },
        AgentRoleSpec {
            id: "qa_agent",
            label: "QA-агент",
            purpose: "Создаёт тест-планы, playtest-чеклисты, баг-репорты и проходы валидации.",
        },
        AgentRoleSpec {
            id: "build_agent",
            label: "Build-агент",
            purpose: "Запускает сборки, упаковку, release-проверки, preview и deployment-шаги.",
        },
    ]
}

pub fn parse_agent_role(value: &str) -> Option<AgentRole> {
    match normalize(value).as_str() {
        "code-agent" | "code" | "engineering" => Some(AgentRole::CodeAgent),
        "game-designer" | "designer" | "game-design" => Some(AgentRole::GameDesigner),
        "art-director" | "art" | "visual" => Some(AgentRole::ArtDirector),
        "audio-agent" | "audio" | "sound" => Some(AgentRole::AudioAgent),
        "qa-agent" | "qa" | "tester" => Some(AgentRole::QaAgent),
        "build-agent" | "build" | "release" => Some(AgentRole::BuildAgent),
        _ => None,
    }
}

pub fn load_orchestration_state(workspace: &Workspace) -> OrchestrationState {
    workspace
        .read_text(ORCHESTRATION_PATH, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str::<OrchestrationState>(&text).ok())
        .unwrap_or_default()
}

pub fn save_orchestration_state(
    workspace: &Workspace,
    state: &OrchestrationState,
) -> anyhow::Result<()> {
    workspace.write_text(ORCHESTRATION_PATH, &serde_json::to_string_pretty(state)?)?;
    Ok(())
}

pub fn update_shared_context(
    workspace: &Workspace,
    mut patch: SharedWorkspaceContext,
) -> anyhow::Result<SharedWorkspaceContext> {
    let mut state = load_orchestration_state(workspace);
    if !patch.summary.trim().is_empty() {
        state.context.summary = patch.summary;
    }
    append_unique(&mut state.context.decisions, &mut patch.decisions);
    append_unique(&mut state.context.open_questions, &mut patch.open_questions);
    append_unique(
        &mut state.context.important_files,
        &mut patch.important_files,
    );
    append_unique(
        &mut state.context.important_assets,
        &mut patch.important_assets,
    );
    state.context.updated_at = unix_timestamp();
    save_orchestration_state(workspace, &state)?;
    Ok(state.context)
}

pub fn record_handoff(
    workspace: &Workspace,
    role: AgentRole,
    from: String,
    task: String,
    context: String,
    expected_output: String,
) -> anyhow::Result<HandoffRecord> {
    let mut state = load_orchestration_state(workspace);
    let record = HandoffRecord {
        id: format!("handoff-{}", uuid::Uuid::new_v4()),
        from,
        to: role,
        task,
        context,
        expected_output,
        recommended_tools: recommended_tools_for_role(role),
        created_at: unix_timestamp(),
    };
    state.handoffs.push(record.clone());
    save_orchestration_state(workspace, &state)?;
    Ok(record)
}

pub fn record_run_summary(
    workspace: &Workspace,
    title: String,
    summary: String,
    completed: Vec<String>,
    next_steps: Vec<String>,
    risks: Vec<String>,
) -> anyhow::Result<RunSummary> {
    let mut state = load_orchestration_state(workspace);
    let summary = RunSummary {
        id: format!("summary-{}", uuid::Uuid::new_v4()),
        title,
        summary,
        completed,
        next_steps,
        risks,
        created_at: unix_timestamp(),
    };
    state.run_summaries.push(summary.clone());
    save_orchestration_state(workspace, &state)?;
    Ok(summary)
}

pub fn record_subagent_run(
    workspace: &Workspace,
    draft: SubagentRunDraft,
) -> anyhow::Result<SubagentRun> {
    let mut state = load_orchestration_state(workspace);
    let run = SubagentRun {
        id: format!("subagent-{}", uuid::Uuid::new_v4()),
        role: draft.role,
        task: draft.task,
        context: draft.context,
        status: draft.status,
        output: draft.output,
        tool_calls: draft.tool_calls,
        denied_tools: draft.denied_tools,
        rounds: draft.rounds,
        created_at: unix_timestamp(),
    };
    state.subagent_runs.push(run.clone());
    save_orchestration_state(workspace, &state)?;
    Ok(run)
}

pub fn create_replay_eval(
    workspace: &Workspace,
    name: String,
    prompt: String,
    expected_tools: Vec<String>,
    success_criteria: Vec<String>,
) -> anyhow::Result<ReplayEvalCase> {
    let mut state = load_orchestration_state(workspace);
    let eval = ReplayEvalCase {
        id: format!("eval-{}", uuid::Uuid::new_v4()),
        name,
        prompt,
        expected_tools,
        success_criteria,
        created_at: unix_timestamp(),
    };
    let eval_path = format!("assets/generated/orchestration/evals/{}.json", eval.id);
    workspace.write_text(&eval_path, &serde_json::to_string_pretty(&eval)?)?;
    state.evals.push(eval.clone());
    save_orchestration_state(workspace, &state)?;
    Ok(eval)
}

pub fn export_trace(workspace: &Workspace) -> anyhow::Result<String> {
    let state = load_orchestration_state(workspace);
    let profiles = detect_project_profiles(workspace);
    let asset_jobs = load_jobs(workspace);
    let trace = json!({
        "exported_at": unix_timestamp(),
        "workspace": workspace.display_name(),
        "project_profiles": profiles.iter().map(|profile| {
            json!({
                "kind": profile.kind,
                "name": profile.name,
                "markers": profile.markers,
                "commands": profile.commands.iter().map(|command| command.id.clone()).collect::<Vec<_>>(),
                "previews": profile.previews.iter().map(|preview| preview.id.clone()).collect::<Vec<_>>()
            })
        }).collect::<Vec<_>>(),
        "project_commands": describe_project_commands(&profiles),
        "asset_jobs": asset_jobs.iter().rev().take(25).collect::<Vec<_>>(),
        "orchestration": state,
        "architecture_decision": {
            "current": "Rust-owned orchestration",
            "reason": "Keeps the desktop app self-contained while preserving handoff, trace, and eval files that can later be replayed by an OpenAI Agents SDK sidecar.",
            "future_sidecar": "Add an Agents SDK sidecar when richer hosted tracing, sessions, or independent specialist execution becomes necessary."
        }
    });
    let path = format!(
        "assets/generated/orchestration/traces/trace-{}.json",
        unix_timestamp()
    );
    workspace.write_text(&path, &serde_json::to_string_pretty(&trace)?)?;
    Ok(path)
}

pub fn orchestration_snapshot(workspace: &Workspace) -> Value {
    let state = load_orchestration_state(workspace);
    json!({
        "roles": agent_role_specs().iter().map(|spec| {
            json!({
                "id": spec.id,
                "label": spec.label,
                "purpose": spec.purpose
            })
        }).collect::<Vec<_>>(),
        "context": state.context,
        "handoff_count": state.handoffs.len(),
        "subagent_run_count": state.subagent_runs.len(),
        "run_summary_count": state.run_summaries.len(),
        "eval_count": state.evals.len(),
        "recent_handoffs": state.handoffs.iter().rev().take(5).collect::<Vec<_>>(),
        "recent_subagent_runs": state.subagent_runs.iter().rev().take(5).collect::<Vec<_>>(),
        "recent_summaries": state.run_summaries.iter().rev().take(5).collect::<Vec<_>>(),
    })
}

pub fn recommended_tools_for_role(role: AgentRole) -> Vec<String> {
    match role {
        AgentRole::CodeAgent => vec!["read_file", "apply_patch", "project_command"],
        AgentRole::GameDesigner => vec!["game_workflow", "project_command"],
        AgentRole::ArtDirector => vec![
            "generate_image_asset",
            "generate_spritesheet_asset",
            "attach_asset",
        ],
        AgentRole::AudioAgent => vec!["generate_audio_asset", "export_asset"],
        AgentRole::QaAgent => vec!["game_workflow", "project_command", "open_project_preview"],
        AgentRole::BuildAgent => vec!["project_command", "open_project_preview", "export_trace"],
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn append_unique(target: &mut Vec<String>, source: &mut Vec<String>) {
    for value in source.drain(..) {
        if !value.trim().is_empty() && !target.iter().any(|known| known == &value) {
            target.push(value);
        }
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '_'], "-")
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
    fn parses_agent_roles() {
        assert_eq!(parse_agent_role("code"), Some(AgentRole::CodeAgent));
        assert_eq!(
            parse_agent_role("art_director"),
            Some(AgentRole::ArtDirector)
        );
        assert_eq!(parse_agent_role("qa"), Some(AgentRole::QaAgent));
    }

    #[test]
    fn records_handoff_and_trace() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let handoff = record_handoff(
            &workspace,
            AgentRole::QaAgent,
            "Leetcode".to_string(),
            "Test movement".to_string(),
            "Prototype is playable".to_string(),
            "Checklist".to_string(),
        )
        .unwrap();
        let trace_path = export_trace(&workspace).unwrap();

        assert!(handoff.id.starts_with("handoff-"));
        assert!(trace_path.starts_with("assets/generated/orchestration/traces/"));
        assert!(workspace
            .read_text(&trace_path, 200_000)
            .unwrap()
            .contains("qa_agent"));
    }
}
