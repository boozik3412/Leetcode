use crate::agent::types::ToolResult;
use crate::governance::spec_for_tool;
use crate::orchestration::{load_orchestration_state, ReplayEvalCase};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

const EVAL_RESULTS_PATH: &str = "assets/generated/leetcode/eval_results.json";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EvalResults {
    #[serde(default)]
    pub runs: Vec<EvalRunResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvalRunResult {
    pub id: String,
    pub eval_id: String,
    pub name: String,
    pub status: String,
    pub checks: Vec<String>,
    pub issues: Vec<String>,
    pub created_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct RunReplayEvalArgs {
    pub eval_id: Option<String>,
}

pub fn eval_snapshot(workspace: &Workspace) -> ToolResult {
    let state = load_orchestration_state(workspace);
    let results = load_results(workspace);
    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "eval_count": state.evals.len(),
            "recent_evals": state.evals.iter().rev().take(10).collect::<Vec<_>>(),
            "recent_results": results.runs.iter().rev().take(10).collect::<Vec<_>>()
        }))
        .unwrap_or_else(|_| "eval snapshot".to_string()),
    )
}

pub fn run_replay_eval(workspace: &Workspace, args: RunReplayEvalArgs) -> ToolResult {
    let state = load_orchestration_state(workspace);
    let selected = if let Some(eval_id) = args.eval_id.as_deref() {
        state
            .evals
            .iter()
            .filter(|eval| eval.id == eval_id)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        state.evals.clone()
    };

    if selected.is_empty() {
        return ToolResult::error("replay-проверки не найдены");
    }

    let mut results = load_results(workspace);
    let mut new_runs = Vec::new();
    for eval in selected {
        let run = evaluate_case(&eval);
        results.runs.push(run.clone());
        new_runs.push(run);
    }

    match save_results(workspace, &results) {
        Ok(()) => ToolResult::ok(
            serde_json::to_string_pretty(&json!({ "runs": new_runs }))
                .unwrap_or_else(|_| "проверка завершена".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn load_results(workspace: &Workspace) -> EvalResults {
    workspace
        .read_text(EVAL_RESULTS_PATH, 1_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_results(workspace: &Workspace, results: &EvalResults) -> anyhow::Result<()> {
    workspace.write_text(EVAL_RESULTS_PATH, &serde_json::to_string_pretty(results)?)
}

fn evaluate_case(eval: &ReplayEvalCase) -> EvalRunResult {
    let mut checks = Vec::new();
    let mut issues = Vec::new();

    if eval.prompt.trim().is_empty() {
        issues.push("промпт пуст".to_string());
    } else {
        checks.push("промпт есть".to_string());
    }

    if eval.expected_tools.is_empty() {
        issues.push("expected_tools пуст".to_string());
    } else {
        for tool in &eval.expected_tools {
            if spec_for_tool(tool).is_some() {
                checks.push(format!("ожидаемый инструмент известен: {tool}"));
            } else {
                issues.push(format!("неизвестный ожидаемый инструмент: {tool}"));
            }
        }
    }

    if eval.success_criteria.is_empty() {
        issues.push("success_criteria пуст".to_string());
    } else {
        checks.push(format!("критериев успеха: {}", eval.success_criteria.len()));
    }

    EvalRunResult {
        id: format!("eval-run-{}", uuid::Uuid::new_v4()),
        eval_id: eval.id.clone(),
        name: eval.name.clone(),
        status: if issues.is_empty() {
            "passed_static_checks".to_string()
        } else {
            "needs_review".to_string()
        },
        checks,
        issues,
        created_at: unix_timestamp(),
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
