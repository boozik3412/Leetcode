use crate::agent::types::ToolResult;
use crate::evals::load_results;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SELF_IMPROVEMENT_STATE_PATH: &str =
    "assets/generated/leetcode/self_improvement/experiments.json";
const MAX_EXPERIMENTS: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfImprovementState {
    #[serde(default)]
    pub experiments: Vec<SelfImprovementExperiment>,
    #[serde(default = "default_benchmarks")]
    pub benchmarks: Vec<BenchmarkDefinition>,
}

impl Default for SelfImprovementState {
    fn default() -> Self {
        Self {
            experiments: Vec::new(),
            benchmarks: default_benchmarks(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfImprovementExperiment {
    pub id: String,
    pub title: String,
    pub hypothesis: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    pub status: ExperimentStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub baseline_git_head: Option<String>,
    pub baseline_evals: EvalMetrics,
    pub current_evals: Option<EvalMetrics>,
    pub snapshot_id: Option<String>,
    pub snapshot_path: Option<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub validation_steps: Vec<ExperimentValidationStep>,
    pub validation_success: Option<bool>,
    pub decision: Option<ExperimentDecision>,
    pub decision_reason: Option<String>,
    pub worktree: Option<ExperimentWorktree>,
    #[serde(default)]
    pub benchmark_comparisons: Vec<BenchmarkComparison>,
    pub benchmark_success: Option<bool>,
    pub promoted_commit: Option<String>,
    pub rolled_back_commit: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Running,
    WorktreeReady,
    Benchmarking,
    NoChanges,
    Validated,
    Failed,
    Accepted,
    Rejected,
    Promoted,
    RolledBack,
}

impl ExperimentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "в работе",
            Self::WorktreeReady => "worktree готов",
            Self::Benchmarking => "проверяется",
            Self::NoChanges => "без изменений",
            Self::Validated => "проверен",
            Self::Failed => "проверка не пройдена",
            Self::Accepted => "принят",
            Self::Rejected => "отклонён",
            Self::Promoted => "продвинут",
            Self::RolledBack => "откачен",
        }
    }
}

pub fn is_active_status(status: ExperimentStatus) -> bool {
    matches!(
        status,
        ExperimentStatus::Running
            | ExperimentStatus::WorktreeReady
            | ExperimentStatus::Benchmarking
            | ExperimentStatus::Validated
            | ExperimentStatus::Failed
            | ExperimentStatus::Accepted
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentDecision {
    Accept,
    Reject,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EvalMetrics {
    pub runs: usize,
    pub clean_runs: usize,
    pub issues: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExperimentValidationStep {
    pub name: String,
    pub command: String,
    pub success: bool,
    pub duration_ms: u128,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExperimentWorktree {
    pub root: String,
    pub branch: String,
    pub baseline_commit: String,
    pub created_at: u64,
    pub cleaned_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkDefinition {
    pub id: String,
    pub name: String,
    pub command: String,
    pub timeout_secs: u64,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkCommandResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub benchmark_id: String,
    pub name: String,
    pub required: bool,
    pub baseline: BenchmarkCommandResult,
    pub candidate: BenchmarkCommandResult,
    pub outcome: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StartSelfImprovementExperimentArgs {
    pub title: String,
    pub hypothesis: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DecideSelfImprovementExperimentArgs {
    pub experiment_id: String,
    pub decision: ExperimentDecision,
    #[serde(default)]
    pub rationale: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PrepareSelfImprovementWorktreeArgs {
    pub experiment_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApplySelfImprovementPatchArgs {
    pub experiment_id: String,
    pub patch: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RunSelfImprovementBenchmarksArgs {
    pub experiment_id: String,
    #[serde(default)]
    pub benchmark_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RegisterSelfImprovementBenchmarkArgs {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default = "default_benchmark_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PromoteSelfImprovementExperimentArgs {
    pub experiment_id: String,
    pub commit_message: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RollbackSelfImprovementExperimentArgs {
    pub experiment_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CleanupSelfImprovementExperimentArgs {
    pub experiment_id: String,
}

pub fn load_state(workspace: &Workspace) -> SelfImprovementState {
    workspace
        .read_text(SELF_IMPROVEMENT_STATE_PATH, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn active_experiment(workspace: &Workspace) -> Option<(String, ExperimentStatus)> {
    load_state(workspace)
        .experiments
        .into_iter()
        .rev()
        .find(|experiment| is_active_status(experiment.status))
        .map(|experiment| (experiment.id, experiment.status))
}

pub fn isolated_experiment_status(
    workspace: &Workspace,
    experiment_id: &str,
) -> Option<ExperimentStatus> {
    load_state(workspace)
        .experiments
        .into_iter()
        .find(|experiment| experiment.id == experiment_id && experiment.worktree.is_some())
        .map(|experiment| experiment.status)
}

pub fn self_improvement_snapshot(workspace: &Workspace) -> ToolResult {
    let state = load_state(workspace);
    let recent = state.experiments.iter().rev().take(12).collect::<Vec<_>>();
    let active = state
        .experiments
        .iter()
        .filter(|experiment| is_active_status(experiment.status))
        .count();
    let validated = state
        .experiments
        .iter()
        .filter(|experiment| experiment.status == ExperimentStatus::Validated)
        .count();

    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "experiment_count": state.experiments.len(),
            "active": active,
            "awaiting_decision": validated,
            "benchmarks": state.benchmarks,
            "recent": recent,
        }))
        .unwrap_or_else(|_| "снимок экспериментов самоулучшения".to_string()),
    )
}

pub fn start_self_improvement_experiment(
    workspace: &Workspace,
    args: StartSelfImprovementExperimentArgs,
) -> ToolResult {
    let title = compact(&args.title, 160);
    let hypothesis = compact(&args.hypothesis, 1_000);
    if title.is_empty() || hypothesis.is_empty() {
        return ToolResult::error("название и гипотеза эксперимента обязательны");
    }
    let criteria = normalized_criteria(args.success_criteria);
    match begin_experiment(workspace, title, hypothesis, criteria, None, None) {
        Ok(experiment) => ToolResult::ok(
            serde_json::to_string_pretty(&experiment)
                .unwrap_or_else(|_| format!("эксперимент {} создан", experiment.id)),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn begin_guarded_experiment(
    workspace: &Workspace,
    request: &str,
    snapshot_id: &str,
    snapshot_path: &str,
) -> anyhow::Result<String> {
    let title = compact(request.lines().next().unwrap_or(request), 160);
    let hypothesis = format!(
        "Изменение Leetcode по запросу «{}» улучшит поведение без регрессий сборки и тестов.",
        compact(request, 500)
    );
    let experiment = begin_experiment(
        workspace,
        if title.is_empty() {
            "Самоизменение Leetcode".to_string()
        } else {
            title
        },
        hypothesis,
        default_success_criteria(),
        Some(snapshot_id.to_string()),
        Some(snapshot_path.to_string()),
    )?;
    Ok(experiment.id)
}

pub fn record_validation(
    workspace: &Workspace,
    experiment_id: &str,
    changed_files: &[String],
    ran: bool,
    success: bool,
    steps: Vec<ExperimentValidationStep>,
) -> anyhow::Result<()> {
    let mut state = load_state(workspace);
    let experiment = state
        .experiments
        .iter_mut()
        .find(|experiment| experiment.id == experiment_id)
        .ok_or_else(|| anyhow::anyhow!("эксперимент не найден: {experiment_id}"))?;
    experiment.changed_files = changed_files.to_vec();
    experiment.validation_steps = steps;
    experiment.validation_success = if ran { Some(success) } else { None };
    experiment.current_evals = Some(eval_metrics(workspace));
    experiment.status = if !ran {
        ExperimentStatus::NoChanges
    } else if success {
        ExperimentStatus::Validated
    } else {
        ExperimentStatus::Failed
    };
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)
}

pub fn decide_self_improvement_experiment(
    workspace: &Workspace,
    args: DecideSelfImprovementExperimentArgs,
) -> ToolResult {
    let mut state = load_state(workspace);
    let Some(experiment) = state
        .experiments
        .iter_mut()
        .find(|experiment| experiment.id == args.experiment_id.trim())
    else {
        return ToolResult::error(format!(
            "эксперимент не найден: {}",
            args.experiment_id.trim()
        ));
    };

    if args.decision == ExperimentDecision::Accept
        && (experiment.status != ExperimentStatus::Validated
            || experiment.validation_success != Some(true))
    {
        return ToolResult::error(
            "принять можно только эксперимент, успешно прошедший автоматическую валидацию",
        );
    }

    experiment.decision = Some(args.decision);
    experiment.decision_reason = non_empty(compact(&args.rationale, 1_000));
    experiment.status = match args.decision {
        ExperimentDecision::Accept => ExperimentStatus::Accepted,
        ExperimentDecision::Reject => ExperimentStatus::Rejected,
    };
    experiment.updated_at = unix_timestamp();
    let rendered = serde_json::to_string_pretty(experiment)
        .unwrap_or_else(|_| format!("решение по эксперименту {} сохранено", experiment.id));
    match save_state(workspace, &state) {
        Ok(()) => ToolResult::ok(rendered),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn prepare_self_improvement_worktree(
    workspace: &Workspace,
    args: PrepareSelfImprovementWorktreeArgs,
) -> ToolResult {
    match prepare_worktree(workspace, args.experiment_id.trim()) {
        Ok(worktree) => ToolResult::ok(
            serde_json::to_string_pretty(&worktree)
                .unwrap_or_else(|_| format!("worktree создан: {}", worktree.root)),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn worktree_workspace(workspace: &Workspace, experiment_id: &str) -> anyhow::Result<Workspace> {
    let state = load_state(workspace);
    let experiment = find_experiment(&state, experiment_id)?;
    let worktree = experiment
        .worktree
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("worktree эксперимента ещё не создан"))?;
    if worktree.cleaned_at.is_some() {
        anyhow::bail!("worktree эксперимента уже очищен");
    }
    let configured = PathBuf::from(&worktree.root);
    ensure_managed_worktree_path(workspace, &configured)?;
    Workspace::new(configured)
}

pub fn record_worktree_changes(
    workspace: &Workspace,
    experiment_id: &str,
) -> anyhow::Result<Vec<String>> {
    let candidate = worktree_workspace(workspace, experiment_id)?;
    let changed_files = git_changed_files(candidate.root())?;
    let mut state = load_state(workspace);
    let experiment = find_experiment_mut(&mut state, experiment_id)?;
    experiment.changed_files = changed_files.clone();
    experiment.status = ExperimentStatus::WorktreeReady;
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)?;
    Ok(changed_files)
}

pub fn register_self_improvement_benchmark(
    workspace: &Workspace,
    args: RegisterSelfImprovementBenchmarkArgs,
) -> ToolResult {
    let id = args.id.trim().to_ascii_lowercase();
    if !valid_identifier(&id) {
        return ToolResult::error(
            "benchmark id должен содержать только a-z, 0-9, точку, дефис или подчёркивание",
        );
    }
    let name = compact(&args.name, 120);
    let command = args.command.trim().to_string();
    if name.is_empty() || command.is_empty() {
        return ToolResult::error("название и команда benchmark обязательны");
    }
    let definition = BenchmarkDefinition {
        id: id.clone(),
        name,
        command,
        timeout_secs: args.timeout_secs.clamp(1, 3_600),
        required: args.required,
    };
    let mut state = load_state(workspace);
    if let Some(existing) = state
        .benchmarks
        .iter_mut()
        .find(|benchmark| benchmark.id == id)
    {
        *existing = definition.clone();
    } else {
        state.benchmarks.push(definition.clone());
    }
    match save_state(workspace, &state) {
        Ok(()) => ToolResult::ok(
            serde_json::to_string_pretty(&definition)
                .unwrap_or_else(|_| format!("benchmark {id} сохранён")),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn run_self_improvement_benchmarks(
    workspace: &Workspace,
    args: RunSelfImprovementBenchmarksArgs,
) -> ToolResult {
    match run_benchmarks(workspace, args) {
        Ok(comparisons) => ToolResult::ok(
            serde_json::to_string_pretty(&json!({ "comparisons": comparisons }))
                .unwrap_or_else(|_| "benchmarks завершены".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn promote_self_improvement_experiment(
    workspace: &Workspace,
    args: PromoteSelfImprovementExperimentArgs,
) -> ToolResult {
    match promote_experiment(workspace, args) {
        Ok(commit) => ToolResult::ok(format!("эксперимент продвинут: {commit}")),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn rollback_self_improvement_experiment(
    workspace: &Workspace,
    args: RollbackSelfImprovementExperimentArgs,
) -> ToolResult {
    match rollback_experiment(workspace, args.experiment_id.trim()) {
        Ok(commit) => ToolResult::ok(format!("эксперимент откачен revert-коммитом: {commit}")),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn cleanup_self_improvement_experiment(
    workspace: &Workspace,
    args: CleanupSelfImprovementExperimentArgs,
) -> ToolResult {
    match cleanup_worktree(workspace, args.experiment_id.trim()) {
        Ok(()) => ToolResult::ok("worktree эксперимента очищен"),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

fn prepare_worktree(
    workspace: &Workspace,
    experiment_id: &str,
) -> anyhow::Result<ExperimentWorktree> {
    ensure_git_repository(workspace)?;
    ensure_clean_workspace(workspace.root())?;
    let baseline_commit = git_output(workspace.root(), &["rev-parse", "HEAD"])?;
    let mut state = load_state(workspace);
    let experiment = find_experiment_mut(&mut state, experiment_id)?;
    if let Some(worktree) = &experiment.worktree {
        if worktree.cleaned_at.is_none() && Path::new(&worktree.root).exists() {
            return Ok(worktree.clone());
        }
    }
    if let Some(expected) = experiment.baseline_git_head.as_deref() {
        if !baseline_commit.starts_with(expected) {
            anyhow::bail!(
                "HEAD изменился после создания эксперимента: baseline {expected}, сейчас {baseline_commit}"
            );
        }
    }

    let root = managed_worktree_root(workspace).join(safe_component(experiment_id));
    if root.exists() {
        anyhow::bail!("путь worktree уже существует: {}", root.display());
    }
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let suffix = experiment_id
        .rsplit('-')
        .next()
        .unwrap_or("candidate")
        .chars()
        .take(12)
        .collect::<String>();
    let branch = format!("leetcode/experiment/{suffix}");
    let root_text = root.to_string_lossy().to_string();
    git_output(
        workspace.root(),
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            &root_text,
            &baseline_commit,
        ],
    )?;
    let worktree = ExperimentWorktree {
        root: root_text,
        branch,
        baseline_commit,
        created_at: unix_timestamp(),
        cleaned_at: None,
    };
    experiment.worktree = Some(worktree.clone());
    experiment.status = ExperimentStatus::WorktreeReady;
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)?;
    Ok(worktree)
}

fn run_benchmarks(
    workspace: &Workspace,
    args: RunSelfImprovementBenchmarksArgs,
) -> anyhow::Result<Vec<BenchmarkComparison>> {
    ensure_clean_workspace(workspace.root())?;
    let candidate = worktree_workspace(workspace, &args.experiment_id)?;
    let mut state = load_state(workspace);
    let selected = select_benchmarks(&state.benchmarks, &args.benchmark_ids)?;
    if selected.is_empty() {
        anyhow::bail!("benchmarks не настроены");
    }
    {
        let experiment = find_experiment_mut(&mut state, &args.experiment_id)?;
        experiment.status = ExperimentStatus::Benchmarking;
        experiment.updated_at = unix_timestamp();
    }
    save_state(workspace, &state)?;

    let mut comparisons = Vec::new();
    for benchmark in selected {
        let baseline = run_benchmark_command(
            workspace.root(),
            workspace.root(),
            &benchmark.command,
            benchmark.timeout_secs,
        );
        let candidate_result = run_benchmark_command(
            candidate.root(),
            workspace.root(),
            &benchmark.command,
            benchmark.timeout_secs,
        );
        let outcome = benchmark_outcome(&baseline, &candidate_result);
        comparisons.push(BenchmarkComparison {
            benchmark_id: benchmark.id,
            name: benchmark.name,
            required: benchmark.required,
            baseline,
            candidate: candidate_result,
            outcome,
        });
    }

    let required_success = comparisons
        .iter()
        .filter(|comparison| comparison.required)
        .all(|comparison| comparison.candidate.success);
    let changed_files = git_changed_files(candidate.root())?;
    let success = required_success && !changed_files.is_empty();
    let mut state = load_state(workspace);
    let experiment = find_experiment_mut(&mut state, &args.experiment_id)?;
    experiment.changed_files = changed_files;
    experiment.benchmark_comparisons = comparisons.clone();
    experiment.benchmark_success = Some(success);
    experiment.validation_success = Some(success);
    experiment.status = if success {
        ExperimentStatus::Validated
    } else {
        ExperimentStatus::Failed
    };
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)?;
    Ok(comparisons)
}

fn promote_experiment(
    workspace: &Workspace,
    args: PromoteSelfImprovementExperimentArgs,
) -> anyhow::Result<String> {
    ensure_clean_workspace(workspace.root())?;
    let candidate = worktree_workspace(workspace, &args.experiment_id)?;
    let mut state = load_state(workspace);
    let experiment = find_experiment_mut(&mut state, &args.experiment_id)?;
    if experiment.status != ExperimentStatus::Accepted
        || experiment.decision != Some(ExperimentDecision::Accept)
        || experiment.benchmark_success != Some(true)
    {
        anyhow::bail!(
            "promotion разрешён только для принятого эксперимента с успешными benchmarks"
        );
    }
    let worktree = experiment
        .worktree
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("worktree не найден"))?;
    let current_head = git_output(workspace.root(), &["rev-parse", "HEAD"])?;
    if current_head != worktree.baseline_commit {
        anyhow::bail!(
            "основная ветка ушла вперёд после baseline; пересоздайте эксперимент на актуальном HEAD"
        );
    }
    if git_changed_files(candidate.root())?.is_empty() {
        anyhow::bail!("candidate не содержит изменений");
    }

    git_output(candidate.root(), &["add", "-A"])?;
    let message = args
        .commit_message
        .as_deref()
        .map(|message| compact(message, 180))
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| format!("experiment: {}", compact(&experiment.title, 120)));
    git_output(
        candidate.root(),
        &[
            "-c",
            "user.name=Leetcode",
            "-c",
            "user.email=leetcode@local",
            "commit",
            "-m",
            &message,
        ],
    )?;
    let commit = git_output(candidate.root(), &["rev-parse", "HEAD"])?;
    git_output(workspace.root(), &["merge", "--ff-only", &commit])?;
    experiment.promoted_commit = Some(commit.clone());
    experiment.status = ExperimentStatus::Promoted;
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)?;
    Ok(commit)
}

fn rollback_experiment(workspace: &Workspace, experiment_id: &str) -> anyhow::Result<String> {
    ensure_clean_workspace(workspace.root())?;
    let mut state = load_state(workspace);
    let experiment = find_experiment_mut(&mut state, experiment_id)?;
    if experiment.status != ExperimentStatus::Promoted {
        anyhow::bail!("откат доступен только для продвинутого эксперимента");
    }
    let promoted_commit = experiment
        .promoted_commit
        .clone()
        .ok_or_else(|| anyhow::anyhow!("promoted commit не записан"))?;
    git_output(workspace.root(), &["revert", "--no-edit", &promoted_commit])?;
    let rollback_commit = git_output(workspace.root(), &["rev-parse", "HEAD"])?;
    experiment.rolled_back_commit = Some(rollback_commit.clone());
    experiment.status = ExperimentStatus::RolledBack;
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)?;
    Ok(rollback_commit)
}

fn cleanup_worktree(workspace: &Workspace, experiment_id: &str) -> anyhow::Result<()> {
    let mut state = load_state(workspace);
    let experiment = find_experiment_mut(&mut state, experiment_id)?;
    let status = experiment.status;
    if !matches!(
        status,
        ExperimentStatus::Rejected
            | ExperimentStatus::Failed
            | ExperimentStatus::NoChanges
            | ExperimentStatus::Promoted
            | ExperimentStatus::RolledBack
    ) {
        anyhow::bail!("сначала завершите, примите или отклоните эксперимент");
    }
    let worktree = experiment
        .worktree
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("worktree не найден"))?;
    if worktree.cleaned_at.is_some() {
        return Ok(());
    }
    let root = PathBuf::from(&worktree.root);
    ensure_managed_worktree_path(workspace, &root)?;
    let root_text = root.to_string_lossy().to_string();
    git_output(
        workspace.root(),
        &["worktree", "remove", "--force", &root_text],
    )?;
    let delete_flag = if matches!(
        status,
        ExperimentStatus::Promoted | ExperimentStatus::RolledBack
    ) {
        "-d"
    } else {
        "-D"
    };
    git_output(workspace.root(), &["branch", delete_flag, &worktree.branch])?;
    worktree.cleaned_at = Some(unix_timestamp());
    experiment.updated_at = unix_timestamp();
    save_state(workspace, &state)
}

fn begin_experiment(
    workspace: &Workspace,
    title: String,
    hypothesis: String,
    success_criteria: Vec<String>,
    snapshot_id: Option<String>,
    snapshot_path: Option<String>,
) -> anyhow::Result<SelfImprovementExperiment> {
    let mut state = load_state(workspace);
    if let Some(active) = state
        .experiments
        .iter()
        .rev()
        .find(|experiment| is_active_status(experiment.status))
    {
        anyhow::bail!(
            "сначала завершите активный эксперимент {} ({})",
            active.id,
            active.status.label()
        );
    }
    let now = unix_timestamp();
    let experiment = SelfImprovementExperiment {
        id: format!("experiment-{now}-{}", uuid::Uuid::new_v4().simple()),
        title,
        hypothesis,
        success_criteria,
        status: ExperimentStatus::Running,
        created_at: now,
        updated_at: now,
        baseline_git_head: git_head(workspace),
        baseline_evals: eval_metrics(workspace),
        current_evals: None,
        snapshot_id,
        snapshot_path,
        changed_files: Vec::new(),
        validation_steps: Vec::new(),
        validation_success: None,
        decision: None,
        decision_reason: None,
        worktree: None,
        benchmark_comparisons: Vec::new(),
        benchmark_success: None,
        promoted_commit: None,
        rolled_back_commit: None,
    };
    state.experiments.push(experiment.clone());
    if state.experiments.len() > MAX_EXPERIMENTS {
        let remove = state.experiments.len() - MAX_EXPERIMENTS;
        state.experiments.drain(0..remove);
    }
    save_state(workspace, &state)?;
    Ok(experiment)
}

fn save_state(workspace: &Workspace, state: &SelfImprovementState) -> anyhow::Result<()> {
    workspace.write_text(
        SELF_IMPROVEMENT_STATE_PATH,
        &serde_json::to_string_pretty(state)?,
    )
}

fn eval_metrics(workspace: &Workspace) -> EvalMetrics {
    let results = load_results(workspace);
    EvalMetrics {
        runs: results.runs.len(),
        clean_runs: results
            .runs
            .iter()
            .filter(|run| run.issues.is_empty())
            .count(),
        issues: results.runs.iter().map(|run| run.issues.len()).sum(),
    }
}

fn find_experiment<'a>(
    state: &'a SelfImprovementState,
    experiment_id: &str,
) -> anyhow::Result<&'a SelfImprovementExperiment> {
    state
        .experiments
        .iter()
        .find(|experiment| experiment.id == experiment_id.trim())
        .ok_or_else(|| anyhow::anyhow!("эксперимент не найден: {}", experiment_id.trim()))
}

fn find_experiment_mut<'a>(
    state: &'a mut SelfImprovementState,
    experiment_id: &str,
) -> anyhow::Result<&'a mut SelfImprovementExperiment> {
    state
        .experiments
        .iter_mut()
        .find(|experiment| experiment.id == experiment_id.trim())
        .ok_or_else(|| anyhow::anyhow!("эксперимент не найден: {}", experiment_id.trim()))
}

fn select_benchmarks(
    benchmarks: &[BenchmarkDefinition],
    selected_ids: &[String],
) -> anyhow::Result<Vec<BenchmarkDefinition>> {
    if selected_ids.is_empty() {
        return Ok(benchmarks.to_vec());
    }
    let mut selected = Vec::new();
    for id in selected_ids {
        let id = id.trim();
        let benchmark = benchmarks
            .iter()
            .find(|benchmark| benchmark.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| anyhow::anyhow!("benchmark не найден: {id}"))?;
        selected.push(benchmark.clone());
    }
    Ok(selected)
}

fn default_benchmarks() -> Vec<BenchmarkDefinition> {
    vec![
        BenchmarkDefinition {
            id: "format".to_string(),
            name: "Rust formatting".to_string(),
            command: "cargo fmt -- --check".to_string(),
            timeout_secs: 180,
            required: true,
        },
        BenchmarkDefinition {
            id: "check".to_string(),
            name: "Rust compile check".to_string(),
            command: "cargo check".to_string(),
            timeout_secs: 900,
            required: true,
        },
        BenchmarkDefinition {
            id: "test".to_string(),
            name: "Rust test suite".to_string(),
            command: "cargo test".to_string(),
            timeout_secs: 1_800,
            required: true,
        },
    ]
}

fn default_benchmark_timeout() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

fn run_benchmark_command(
    cwd: &Path,
    toolchain_root: &Path,
    command: &str,
    timeout_secs: u64,
) -> BenchmarkCommandResult {
    let started = Instant::now();
    let mut process = if cfg!(windows) {
        let mut process = Command::new("cmd");
        process.args(["/D", "/S", "/C", command]);
        process
    } else {
        let mut process = Command::new("sh");
        process.args(["-lc", command]);
        process
    };
    process
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_local_toolchain_env(toolchain_root, &mut process);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(err) => {
            return BenchmarkCommandResult {
                success: false,
                exit_code: None,
                duration_ms: started.elapsed().as_millis(),
                stdout: String::new(),
                stderr: err.to_string(),
            };
        }
    };
    let stdout_reader = child.stdout.take().map(|mut stdout| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            bytes
        })
    });
    let stderr_reader = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            bytes
        })
    });
    let timeout = Duration::from_secs(timeout_secs.clamp(1, 3_600));
    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), false),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                break (child.wait().ok(), true);
            }
            Err(_) => break (child.wait().ok(), false),
        }
    };
    let stdout = stdout_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let stderr = stderr_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let stderr = compact_output(&String::from_utf8_lossy(&stderr), 8_000);
    BenchmarkCommandResult {
        success: status.as_ref().is_some_and(|status| status.success()) && !timed_out,
        exit_code: status.and_then(|status| status.code()),
        duration_ms: started.elapsed().as_millis(),
        stdout: compact_output(&String::from_utf8_lossy(&stdout), 8_000),
        stderr: if timed_out {
            format!("timeout после {} с\n{stderr}", timeout.as_secs())
        } else {
            stderr
        },
    }
}

fn benchmark_outcome(
    baseline: &BenchmarkCommandResult,
    candidate: &BenchmarkCommandResult,
) -> String {
    match (baseline.success, candidate.success) {
        (true, true) => {
            let slower = candidate.duration_ms.saturating_sub(baseline.duration_ms);
            if slower > 500 && candidate.duration_ms > baseline.duration_ms.saturating_mul(3) / 2 {
                "passed_with_timing_regression".to_string()
            } else {
                "passed".to_string()
            }
        }
        (false, true) => "improved".to_string(),
        (true, false) => "regressed".to_string(),
        (false, false) => "still_failing".to_string(),
    }
}

fn ensure_git_repository(workspace: &Workspace) -> anyhow::Result<()> {
    git_output(workspace.root(), &["rev-parse", "--is-inside-work-tree"])
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("self-improvement worktree требует Git-репозиторий"))
}

fn ensure_clean_workspace(root: &Path) -> anyhow::Result<()> {
    let status = git_output(root, &["status", "--porcelain=v1"])?;
    if !status.trim().is_empty() {
        anyhow::bail!(
            "основная рабочая копия должна быть чистой перед изолированным экспериментом"
        );
    }
    Ok(())
}

fn git_changed_files(root: &Path) -> anyhow::Result<Vec<String>> {
    let output = git_output(root, &["status", "--porcelain=v1"])?;
    Ok(output
        .lines()
        .filter_map(|line| line.get(3..))
        .map(|path| {
            path.rsplit(" -> ")
                .next()
                .unwrap_or(path)
                .trim_matches('"')
                .replace('\\', "/")
        })
        .filter(|path| !path.is_empty())
        .collect())
}

fn git_output(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} завершился с ошибкой {}: {}",
            args.join(" "),
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string()),
            compact_output(&String::from_utf8_lossy(&output.stderr), 4_000)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn managed_worktree_root(workspace: &Workspace) -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    let mut hasher = Sha256::new();
    hasher.update(workspace.root().to_string_lossy().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    base.join("Leetcode")
        .join("self-improvement")
        .join("worktrees")
        .join(&digest[..16])
}

fn ensure_managed_worktree_path(workspace: &Workspace, path: &Path) -> anyhow::Result<()> {
    let root = managed_worktree_root(workspace);
    if !path.starts_with(&root) {
        anyhow::bail!("worktree находится вне управляемого каталога Leetcode");
    }
    Ok(())
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect()
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

fn apply_local_toolchain_env(root: &Path, command: &mut Command) {
    let rustup_home = root.join(".rustup");
    if rustup_home.exists() {
        command.env("RUSTUP_HOME", &rustup_home);
    }
    let cargo_home = root.join(".cargo");
    if cargo_home.exists() {
        command.env("CARGO_HOME", &cargo_home);
        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![cargo_home.join("bin")];
        paths.extend(std::env::split_paths(&old_path));
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
}

fn compact_output(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}

fn git_head(workspace: &Workspace) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace.root())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    non_empty(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn default_success_criteria() -> Vec<String> {
    vec![
        "Изменены только файлы, относящиеся к подтверждённой задаче".to_string(),
        "cargo fmt выполняется успешно".to_string(),
        "cargo check выполняется успешно".to_string(),
        "cargo test выполняется успешно".to_string(),
        "Результат явно принят или отклонён после проверки".to_string(),
    ]
}

fn normalized_criteria(criteria: Vec<String>) -> Vec<String> {
    let criteria = criteria
        .into_iter()
        .map(|criterion| compact(&criterion, 300))
        .filter(|criterion| !criterion.is_empty())
        .take(20)
        .collect::<Vec<_>>();
    if criteria.is_empty() {
        default_success_criteria()
    } else {
        criteria
    }
}

fn compact(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        format!(
            "{}...",
            normalized.chars().take(max_chars).collect::<String>()
        )
    }
}

fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
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
    use std::fs;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("src")).expect("src");
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"leetcode\"\nversion = \"0.1.0\"\n",
        )
        .expect("cargo");
        fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").expect("main");
        fs::write(
            temp.path().join(".gitignore"),
            "/assets/generated/leetcode/\n",
        )
        .expect("gitignore");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        (temp, workspace)
    }

    fn git_workspace() -> (tempfile::TempDir, Workspace) {
        let (temp, workspace) = workspace();
        git_output(workspace.root(), &["init"]).expect("git init");
        git_output(
            workspace.root(),
            &["config", "user.email", "leetcode-tests@local"],
        )
        .expect("git email");
        git_output(workspace.root(), &["config", "user.name", "Leetcode Tests"]).expect("git name");
        git_output(workspace.root(), &["add", "-A"]).expect("git add");
        git_output(workspace.root(), &["commit", "-m", "baseline"]).expect("git commit");
        (temp, workspace)
    }

    #[test]
    fn records_validation_and_accepts_only_successful_experiment() {
        let (_temp, workspace) = workspace();
        let experiment = begin_experiment(
            &workspace,
            "Улучшить тесты".to_string(),
            "Новая проверка предотвратит регрессию".to_string(),
            vec!["тест проходит".to_string()],
            Some("snapshot-1".to_string()),
            Some("snapshots/1".to_string()),
        )
        .expect("experiment");

        let premature = decide_self_improvement_experiment(
            &workspace,
            DecideSelfImprovementExperimentArgs {
                experiment_id: experiment.id.clone(),
                decision: ExperimentDecision::Accept,
                rationale: String::new(),
            },
        );
        assert!(!premature.ok);

        record_validation(
            &workspace,
            &experiment.id,
            &["src/main.rs".to_string()],
            true,
            true,
            vec![ExperimentValidationStep {
                name: "cargo test".to_string(),
                command: "cargo test".to_string(),
                success: true,
                duration_ms: 10,
                exit_code: Some(0),
            }],
        )
        .expect("validation");

        let accepted = decide_self_improvement_experiment(
            &workspace,
            DecideSelfImprovementExperimentArgs {
                experiment_id: experiment.id.clone(),
                decision: ExperimentDecision::Accept,
                rationale: "Проверки зелёные".to_string(),
            },
        );
        assert!(accepted.ok);
        let state = load_state(&workspace);
        assert_eq!(state.experiments[0].status, ExperimentStatus::Accepted);
    }

    #[test]
    fn guarded_experiment_has_default_quality_gate() {
        let (_temp, workspace) = workspace();
        let id = begin_guarded_experiment(
            &workspace,
            "реализуй безопасное самоулучшение",
            "snapshot-2",
            "snapshots/2",
        )
        .expect("guarded experiment");
        let state = load_state(&workspace);
        let experiment = state
            .experiments
            .iter()
            .find(|experiment| experiment.id == id)
            .expect("stored experiment");
        assert_eq!(experiment.status, ExperimentStatus::Running);
        assert!(experiment.success_criteria.len() >= 4);
    }

    #[test]
    fn rejects_parallel_active_experiments() {
        let (_temp, workspace) = workspace();
        let first = begin_experiment(
            &workspace,
            "Первый эксперимент".to_string(),
            "Проверяет последовательное выполнение".to_string(),
            vec!["готово".to_string()],
            None,
            None,
        )
        .expect("first experiment");

        assert_eq!(
            active_experiment(&workspace),
            Some((first.id.clone(), ExperimentStatus::Running))
        );
        let second = start_self_improvement_experiment(
            &workspace,
            StartSelfImprovementExperimentArgs {
                title: "Второй эксперимент".to_string(),
                hypothesis: "Не должен начаться параллельно".to_string(),
                success_criteria: Vec::new(),
            },
        );
        assert!(!second.ok);
        assert_eq!(load_state(&workspace).experiments.len(), 1);
    }

    #[test]
    fn isolated_experiment_promotes_and_rolls_back_with_git_history() {
        let (_temp, workspace) = git_workspace();
        let experiment = begin_experiment(
            &workspace,
            "Изолированное улучшение".to_string(),
            "Candidate изменит код без касания main до promotion".to_string(),
            vec!["smoke benchmark проходит".to_string()],
            None,
            None,
        )
        .expect("experiment");
        let worktree = prepare_worktree(&workspace, &experiment.id).expect("worktree");
        let candidate = Workspace::new(PathBuf::from(&worktree.root)).expect("candidate");
        candidate
            .write_text("src/main.rs", "fn main() { println!(\"changed\"); }\n")
            .expect("candidate change");
        record_worktree_changes(&workspace, &experiment.id).expect("changed files");

        let smoke_command = if cfg!(windows) {
            "findstr /C:\"changed\" src\\main.rs"
        } else {
            "grep -q changed src/main.rs"
        };
        let registered = register_self_improvement_benchmark(
            &workspace,
            RegisterSelfImprovementBenchmarkArgs {
                id: "candidate-smoke".to_string(),
                name: "Candidate smoke".to_string(),
                command: smoke_command.to_string(),
                timeout_secs: 30,
                required: true,
            },
        );
        assert!(registered.ok);
        let comparisons = run_benchmarks(
            &workspace,
            RunSelfImprovementBenchmarksArgs {
                experiment_id: experiment.id.clone(),
                benchmark_ids: vec!["candidate-smoke".to_string()],
            },
        )
        .expect("benchmarks");
        assert_eq!(comparisons[0].outcome, "improved");

        let accepted = decide_self_improvement_experiment(
            &workspace,
            DecideSelfImprovementExperimentArgs {
                experiment_id: experiment.id.clone(),
                decision: ExperimentDecision::Accept,
                rationale: "candidate smoke зелёный".to_string(),
            },
        );
        assert!(accepted.ok);
        let promoted = promote_experiment(
            &workspace,
            PromoteSelfImprovementExperimentArgs {
                experiment_id: experiment.id.clone(),
                commit_message: Some("test: promote isolated candidate".to_string()),
            },
        )
        .expect("promote");
        assert!(!promoted.is_empty());
        assert!(workspace
            .read_text("src/main.rs", 10_000)
            .expect("promoted file")
            .contains("changed"));

        rollback_experiment(&workspace, &experiment.id).expect("rollback");
        assert!(!workspace
            .read_text("src/main.rs", 10_000)
            .expect("rolled back file")
            .contains("changed"));
        cleanup_worktree(&workspace, &experiment.id).expect("cleanup");
        assert!(!Path::new(&worktree.root).exists());
    }
}
