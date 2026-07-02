use crate::agent::types::ToolResult;
use crate::tools::policy::{request_approval, ApprovalMap, PolicyConfig};
use crate::workspace::{is_ignored_entry, Workspace};
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use walkdir::WalkDir;

use crate::agent::types::AppEvent;

#[derive(Debug, Deserialize)]
pub struct ListFilesArgs {
    pub path: Option<String>,
    pub depth: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    pub path: String,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileArgs {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct EditFileArgs {
    pub path: String,
    pub old: String,
    pub new: String,
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyPatchArgs {
    pub patch: String,
}

#[derive(Debug, Deserialize)]
pub struct GrepArgs {
    pub pattern: String,
    pub path: Option<String>,
    pub limit: Option<usize>,
}

pub fn list_files(workspace: &Workspace, args: ListFilesArgs) -> ToolResult {
    let path = args.path.as_deref().unwrap_or(".");
    let depth = args.depth.unwrap_or(4).min(12);
    let limit = args.limit.unwrap_or(500).min(2_000);
    let root = match workspace.resolve_existing(path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };

    let mut rows = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(depth)
        .into_iter()
        .filter_entry(|entry| !is_ignored_entry(entry))
        .filter_map(Result::ok)
    {
        if rows.len() >= limit {
            rows.push("... обрезано ...".to_string());
            break;
        }
        if entry.path() == workspace.root() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(workspace.root()) else {
            continue;
        };
        let suffix = if entry.file_type().is_dir() { "/" } else { "" };
        rows.push(format!(
            "{}{}",
            rel.to_string_lossy().replace('\\', "/"),
            suffix
        ));
    }

    ToolResult::ok(if rows.is_empty() {
        "нет".to_string()
    } else {
        rows.join("\n")
    })
}

pub fn read_file(workspace: &Workspace, args: ReadFileArgs) -> ToolResult {
    let path = match workspace.resolve_existing(&args.path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if path.is_dir() {
        return ToolResult::error("read_file ожидает файл, получена папка");
    }

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => return ToolResult::error(err.to_string()),
    };

    let offset = args.offset.unwrap_or(0);
    let limit = args.limit.unwrap_or(240).min(1_000);
    let lines: Vec<_> = text.lines().collect();
    let selected = lines.iter().skip(offset).take(limit);
    let rendered = selected
        .enumerate()
        .map(|(idx, line)| format!("{:4}| {}", offset + idx + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    ToolResult::ok(if rendered.is_empty() {
        "(пусто)".to_string()
    } else {
        rendered
    })
}

pub fn write_file(
    workspace: &Workspace,
    args: WriteFileArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if policy.require_write_approval
        && !request_approval(
            events,
            approvals,
            format!("Записать файл {}", args.path),
            preview_text(&args.content),
        )
    {
        return ToolResult::error("write_file отклонён пользователем");
    }

    let path = match workspace.resolve_for_write(&args.path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };

    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return ToolResult::error(err.to_string());
        }
    }

    match fs::write(path, args.content) {
        Ok(()) => ToolResult::ok("ок"),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn edit_file(
    workspace: &Workspace,
    args: EditFileArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let path = match workspace.resolve_existing(&args.path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if path.is_dir() {
        return ToolResult::error("edit_file ожидает файл, получена папка");
    }

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => return ToolResult::error(err.to_string()),
    };

    if !text.contains(&args.old) {
        return ToolResult::error("исходная строка не найдена");
    }
    let count = text.matches(&args.old).count();
    if !args.all.unwrap_or(false) && count > 1 {
        return ToolResult::error(format!(
            "исходная строка встречается {count} раз; задайте all=true или сделайте её уникальной"
        ));
    }

    if policy.require_write_approval
        && !request_approval(
            events,
            approvals,
            format!("Изменить файл {}", args.path),
            format!("Заменить:\n{}\n\nНа:\n{}", args.old, args.new),
        )
    {
        return ToolResult::error("edit_file отклонён пользователем");
    }

    let replaced = if args.all.unwrap_or(false) {
        text.replace(&args.old, &args.new)
    } else {
        text.replacen(&args.old, &args.new, 1)
    };

    match fs::write(path, replaced) {
        Ok(()) => ToolResult::ok("ок"),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn apply_patch(
    workspace: &Workspace,
    args: ApplyPatchArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.patch.trim().is_empty() {
        return ToolResult::error("patch пустой");
    }

    if patch_looks_outside_workspace(&args.patch) {
        return ToolResult::error("patch ссылается на пути вне рабочей папки");
    }

    let check = run_git_apply(workspace, &args.patch, true);
    if !check.status_success {
        return ToolResult::error(format!(
            "git apply --check не выполнен\nstdout:\n{}\nstderr:\n{}",
            check.stdout, check.stderr
        ));
    }

    let affected = patch_affected_files(&args.patch);
    let impact = if affected.is_empty() {
        "Затронутые файлы: неизвестно".to_string()
    } else {
        format!("Затронутые файлы:\n{}", affected.join("\n"))
    };

    if policy.require_write_approval
        && !request_approval(
            events,
            approvals,
            "Применить patch",
            format!("{impact}\n\nPatch:\n{}", preview_text(&args.patch)),
        )
    {
        return ToolResult::error("apply_patch отклонён пользователем");
    }

    let applied = run_git_apply(workspace, &args.patch, false);
    let stat = git_diff_stat(workspace);
    let rendered = format!(
        "stdout:\n{}\nstderr:\n{}\n\n{}",
        applied.stdout, applied.stderr, stat
    );

    if applied.status_success {
        ToolResult::ok(if rendered.trim().is_empty() {
            "ok".to_string()
        } else {
            rendered
        })
    } else {
        ToolResult::error(format!(
            "git apply завершился с ошибочным статусом {}\n{}",
            applied.status, rendered
        ))
    }
}

pub fn grep(workspace: &Workspace, args: GrepArgs) -> ToolResult {
    let regex = match Regex::new(&args.pattern) {
        Ok(regex) => regex,
        Err(err) => return ToolResult::error(err.to_string()),
    };

    let path = args.path.as_deref().unwrap_or(".");
    let root = match workspace.resolve_existing(path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    let limit = args.limit.unwrap_or(100).min(1_000);
    let mut hits = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_ignored_entry(entry))
        .filter_map(Result::ok)
    {
        if hits.len() >= limit {
            hits.push("... обрезано ...".to_string());
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(rel) = entry.path().strip_prefix(workspace.root()) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        for (idx, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                hits.push(format!("{rel}:{}:{}", idx + 1, line));
                if hits.len() >= limit {
                    break;
                }
            }
        }
    }

    ToolResult::ok(if hits.is_empty() {
        "нет".to_string()
    } else {
        hits.join("\n")
    })
}

fn preview_text(text: &str) -> String {
    let mut preview = text.chars().take(2_000).collect::<String>();
    if text.chars().count() > 2_000 {
        preview.push_str("\n... обрезано ...");
    }
    preview
}

fn patch_looks_outside_workspace(patch: &str) -> bool {
    patch.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("+++ /")
            || trimmed.starts_with("--- /")
            || trimmed.starts_with("+++ ../")
            || trimmed.starts_with("--- ../")
            || trimmed.starts_with("rename to ../")
            || trimmed.starts_with("rename from ../")
    })
}

struct GitApplyOutput {
    status: String,
    status_success: bool,
    stdout: String,
    stderr: String,
}

fn run_git_apply(workspace: &Workspace, patch: &str, check_only: bool) -> GitApplyOutput {
    let mut command = Command::new("git");
    command
        .arg("apply")
        .arg("--whitespace=nowarn")
        .current_dir(workspace.root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if check_only {
        command.arg("--check");
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return GitApplyOutput {
                status: "spawn не выполнен".to_string(),
                status_success: false,
                stdout: String::new(),
                stderr: err.to_string(),
            }
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(err) = stdin.write_all(patch.as_bytes()) {
            return GitApplyOutput {
                status: "stdin не выполнен".to_string(),
                status_success: false,
                stdout: String::new(),
                stderr: err.to_string(),
            };
        }
    }

    match child.wait_with_output() {
        Ok(output) => GitApplyOutput {
            status: output.status.to_string(),
            status_success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(err) => GitApplyOutput {
            status: "wait не выполнен".to_string(),
            status_success: false,
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

fn git_diff_stat(workspace: &Workspace) -> String {
    match Command::new("git")
        .arg("diff")
        .arg("--stat")
        .current_dir(workspace.root())
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                "git diff --stat: нет изменений".to_string()
            } else {
                format!("git diff --stat:\n{stdout}")
            }
        }
        Ok(output) => format!(
            "git diff --stat не выполнен:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(err) => format!("git diff --stat ошибка: {err}"),
    }
}

fn patch_affected_files(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            files.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("--- a/") {
            let candidate = rest.to_string();
            if !files.iter().any(|file| file == &candidate) {
                files.push(candidate);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_patch_paths_outside_workspace() {
        assert!(patch_looks_outside_workspace("--- ../x\n+++ ../x"));
        assert!(patch_looks_outside_workspace("--- /tmp/x\n+++ /tmp/x"));
    }

    #[test]
    fn extracts_affected_files_from_unified_diff() {
        let patch = "--- a/src/main.rs\n+++ b/src/main.rs\n@@\n-old\n+new\n";
        assert_eq!(patch_affected_files(patch), vec!["src/main.rs"]);
    }
}
