use crate::workspace::Workspace;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub const SELF_MODIFICATION_DIR: &str = "assets/generated/leetcode/self_modification";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfModificationSnapshot {
    pub id: String,
    pub created_at: u64,
    pub reason: String,
    pub workspace_root: String,
    pub git_head: Option<String>,
    pub rel_path: String,
    pub files_copied: usize,
}

#[derive(Clone, Debug)]
pub struct SelfModificationGuard {
    pub snapshot: SelfModificationSnapshot,
    pub baseline_changed_files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SelfModificationValidation {
    pub snapshot: SelfModificationSnapshot,
    pub changed_files: Vec<String>,
    pub ran: bool,
    pub success: bool,
    pub steps: Vec<SelfModificationValidationStep>,
}

#[derive(Clone, Debug)]
pub struct SelfModificationValidationStep {
    pub name: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
}

pub fn prepare_self_modification_guard(
    workspace: &Workspace,
    user_request: &str,
    baseline_changed_files: Vec<String>,
) -> anyhow::Result<Option<SelfModificationGuard>> {
    if !should_guard_run(workspace, user_request) {
        return Ok(None);
    }

    let snapshot = create_restore_snapshot(workspace, user_request)?;
    Ok(Some(SelfModificationGuard {
        snapshot,
        baseline_changed_files,
    }))
}

pub fn should_guard_run(workspace: &Workspace, user_request: &str) -> bool {
    is_leetcode_workspace(workspace) && looks_like_self_modification(user_request)
}

pub fn is_leetcode_workspace(workspace: &Workspace) -> bool {
    let cargo_toml = workspace.root().join("Cargo.toml");
    let main_rs = workspace.root().join("src").join("main.rs");
    let Ok(cargo_text) = fs::read_to_string(cargo_toml) else {
        return false;
    };

    main_rs.exists() && cargo_text.contains("name = \"leetcode\"")
}

pub fn run_self_modification_validation(
    workspace: &Workspace,
    guard: SelfModificationGuard,
    current_changed_files: &[String],
) -> SelfModificationValidation {
    let changed_files =
        changed_files_since_snapshot(&guard.baseline_changed_files, current_changed_files);
    if changed_files.is_empty() {
        return SelfModificationValidation {
            snapshot: guard.snapshot,
            changed_files,
            ran: false,
            success: true,
            steps: Vec::new(),
        };
    }

    let mut steps = Vec::new();
    for (name, args) in [
        ("cargo fmt", vec!["fmt"]),
        ("cargo check", vec!["check"]),
        ("cargo test", vec!["test"]),
    ] {
        let step = run_cargo_step(workspace.root(), name, &args);
        let success = step.success;
        steps.push(step);
        if !success {
            break;
        }
    }

    let success = steps.iter().all(|step| step.success);
    SelfModificationValidation {
        snapshot: guard.snapshot,
        changed_files,
        ran: true,
        success,
        steps,
    }
}

#[allow(dead_code)]
pub fn restore_snapshot(
    workspace: &Workspace,
    snapshot: &SelfModificationSnapshot,
) -> anyhow::Result<usize> {
    let source_root = workspace
        .root()
        .join(&snapshot.rel_path)
        .join("files")
        .canonicalize()
        .with_context(|| format!("snapshot not found: {}", snapshot.rel_path))?;
    let mut restored = 0;
    for entry in WalkDir::new(&source_root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(&source_root)?;
        let target = workspace.root().join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), target)?;
        restored += 1;
    }
    Ok(restored)
}

impl SelfModificationValidation {
    pub fn short_status(&self) -> String {
        if !self.ran {
            return format!(
                "snapshot {} создан, новых self-changes нет",
                self.snapshot.id
            );
        }
        let status = if self.success {
            "прошла"
        } else {
            "не прошла"
        };
        format!(
            "self-check {}: {} шагов, snapshot {}",
            status,
            self.steps.len(),
            self.snapshot.id
        )
    }

    pub fn report(&self) -> String {
        let changed = if self.changed_files.is_empty() {
            "нет новых изменённых файлов".to_string()
        } else {
            self.changed_files
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };
        let steps = if self.steps.is_empty() {
            "проверки не запускались".to_string()
        } else {
            self.steps
                .iter()
                .map(|step| {
                    let code = step
                        .exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "signal".to_string());
                    format!(
                        "- {} (`{}`): {} за {} мс (exit {code})",
                        step.name,
                        step.command,
                        if step.success { "ok" } else { "failed" },
                        step.duration_ms
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let last_failure = self
            .steps
            .iter()
            .find(|step| !step.success)
            .map(|step| {
                format!(
                    "\n\nПоследняя ошибка:\nstdout:\n{}\nstderr:\n{}",
                    compact(&step.stdout, 2_000),
                    compact(&step.stderr, 2_000)
                )
            })
            .unwrap_or_default();

        let rollback = format!(
            "Restore snapshot: {}/files. Если проверка сломалась, безопасный следующий шаг: попросить агента исправить ошибку по этому отчёту или восстановить файлы из snapshot.",
            self.snapshot.rel_path
        );

        format!(
            "Snapshot: {}\nИзменения: {changed}\n\nПроверки:\n{steps}\n\n{rollback}{last_failure}",
            self.snapshot.rel_path
        )
    }
}

fn create_restore_snapshot(
    workspace: &Workspace,
    reason: &str,
) -> anyhow::Result<SelfModificationSnapshot> {
    let created_at = unix_timestamp();
    let id = format!("selfmod-{created_at}-{}", uuid::Uuid::new_v4().simple());
    let rel_path = format!("{SELF_MODIFICATION_DIR}/snapshots/{id}");
    let snapshot_root = workspace.resolve_for_write(&rel_path)?;
    let files_root = snapshot_root.join("files");
    fs::create_dir_all(&files_root)?;

    let mut files_copied = 0;
    for entry in WalkDir::new(workspace.root())
        .into_iter()
        .filter_entry(|entry| !is_snapshot_excluded(workspace.root(), entry.path()))
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(workspace.root())?;
        let target = files_root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), target)?;
        files_copied += 1;
    }

    let snapshot = SelfModificationSnapshot {
        id,
        created_at,
        reason: compact(reason, 1_000),
        workspace_root: workspace.root().to_string_lossy().to_string(),
        git_head: git_head(workspace.root()),
        rel_path,
        files_copied,
    };
    fs::write(
        snapshot_root.join("snapshot.json"),
        serde_json::to_string_pretty(&snapshot)?,
    )?;
    Ok(snapshot)
}

fn run_cargo_step(root: &Path, name: &str, args: &[&str]) -> SelfModificationValidationStep {
    let started = Instant::now();
    let mut command = Command::new(cargo_exe(root));
    command.args(args).current_dir(root);
    apply_local_toolchain_env(root, &mut command);
    let output = command.output();
    let duration_ms = started.elapsed().as_millis();

    match output {
        Ok(output) => SelfModificationValidationStep {
            name: name.to_string(),
            command: format!("cargo {}", args.join(" ")),
            exit_code: output.status.code(),
            success: output.status.success(),
            duration_ms,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(err) => SelfModificationValidationStep {
            name: name.to_string(),
            command: format!("cargo {}", args.join(" ")),
            exit_code: None,
            success: false,
            duration_ms,
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

fn apply_local_toolchain_env(root: &Path, command: &mut Command) {
    let rustup_home = root.join(".rustup");
    if rustup_home.exists() {
        command.env("RUSTUP_HOME", &rustup_home);
    }
    let cargo_home = root.join(".cargo");
    if cargo_home.exists() {
        command.env("CARGO_HOME", &cargo_home);
        let cargo_bin = cargo_home.join("bin");
        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![cargo_bin];
        paths.extend(std::env::split_paths(&old_path));
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
}

fn cargo_exe(root: &Path) -> PathBuf {
    let local =
        root.join(".cargo")
            .join("bin")
            .join(if cfg!(windows) { "cargo.exe" } else { "cargo" });
    if local.exists() {
        local
    } else {
        PathBuf::from("cargo")
    }
}

fn changed_files_since_snapshot(
    baseline_changed_files: &[String],
    current_changed_files: &[String],
) -> Vec<String> {
    let baseline = baseline_changed_files
        .iter()
        .map(|path| normalize_path(path))
        .collect::<HashSet<_>>();
    current_changed_files
        .iter()
        .map(|path| normalize_path(path))
        .filter(|path| !baseline.contains(path))
        .collect()
}

fn looks_like_self_modification(user_request: &str) -> bool {
    let lower = user_request.to_lowercase();
    let markers = [
        "реализ",
        "добав",
        "исправ",
        "измени",
        "обнов",
        "сделай",
        "этап",
        "бэклог",
        "код",
        "агент",
        "leetcode",
        "stage",
        "backlog",
        "implement",
        "add",
        "fix",
        "change",
        "update",
    ];
    markers.iter().any(|marker| lower.contains(marker))
}

fn is_snapshot_excluded(root: &Path, path: &Path) -> bool {
    if path == root {
        return false;
    }
    let Ok(rel) = path.strip_prefix(root) else {
        return true;
    };
    let rel = normalize_path(&rel.to_string_lossy());
    let first = rel.split('/').next().unwrap_or_default();
    matches!(
        first,
        ".git" | "target" | ".cargo" | ".rustup" | "dist" | "node_modules" | ".next" | "build"
    ) || rel == "assets/generated"
        || rel.starts_with("assets/generated/")
}

fn git_head(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head.is_empty() {
        None
    } else {
        Some(head)
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
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
    fn detects_leetcode_workspace_and_creates_snapshot() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"leetcode\"\n",
        )
        .expect("cargo");
        fs::create_dir_all(temp.path().join("src")).expect("src");
        fs::write(temp.path().join("src").join("main.rs"), "fn main() {}\n").expect("main");
        fs::create_dir_all(temp.path().join("target")).expect("target");
        fs::write(temp.path().join("target").join("skip.txt"), "skip").expect("skip");
        fs::create_dir_all(temp.path().join("assets").join("generated")).expect("generated");
        fs::write(
            temp.path()
                .join("assets")
                .join("generated")
                .join("skip.txt"),
            "skip",
        )
        .expect("generated file");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");

        assert!(should_guard_run(&workspace, "реализуй этап 21"));
        let guard = prepare_self_modification_guard(&workspace, "реализуй этап 21", Vec::new())
            .expect("guard")
            .expect("some");

        let snapshot_root = workspace.root().join(&guard.snapshot.rel_path);
        assert!(snapshot_root.join("snapshot.json").exists());
        assert!(snapshot_root
            .join("files")
            .join("src")
            .join("main.rs")
            .exists());
        assert!(!snapshot_root.join("files").join("target").exists());
        assert!(!snapshot_root
            .join("files")
            .join("assets")
            .join("generated")
            .exists());
    }

    #[test]
    fn validation_skips_when_no_new_changes() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"leetcode\"\n",
        )
        .expect("cargo");
        fs::create_dir_all(temp.path().join("src")).expect("src");
        fs::write(temp.path().join("src").join("main.rs"), "fn main() {}\n").expect("main");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let guard = prepare_self_modification_guard(
            &workspace,
            "добавь безопасное самоизменение",
            vec!["src/main.rs".to_string()],
        )
        .expect("guard")
        .expect("some");

        let validation =
            run_self_modification_validation(&workspace, guard, &["src/main.rs".to_string()]);

        assert!(!validation.ran);
        assert!(validation.success);
    }
}
