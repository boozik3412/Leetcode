use super::types::{SemanticIndexState, SemanticTagAssignment};
use crate::workspace::Workspace;
use anyhow::Context;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const INDEX_FILE: &str = "semantic_index.json";
const AUDIT_FILE: &str = "semantic_audit.jsonl";

pub fn project_key(workspace: &Workspace) -> String {
    let canonical = workspace.root().to_string_lossy().to_ascii_lowercase();
    let digest = Sha256::digest(canonical.as_bytes());
    let suffix = format!("{digest:x}");
    let name = workspace
        .display_name()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}-{}", name, &suffix[..12])
}

pub fn semantic_data_dir(workspace: &Workspace) -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    base.join("leetcode")
        .join("projects")
        .join(project_key(workspace))
        .join("semantics")
}

pub fn load_semantic_index(workspace: &Workspace) -> SemanticIndexState {
    let mut state = fs::read_to_string(semantic_data_dir(workspace).join(INDEX_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<SemanticIndexState>(&text).ok())
        .unwrap_or_default();
    state.project_key = project_key(workspace);
    state
}

pub fn save_semantic_index(
    workspace: &Workspace,
    state: &SemanticIndexState,
) -> anyhow::Result<()> {
    let directory = semantic_data_dir(workspace);
    fs::create_dir_all(&directory)
        .with_context(|| format!("Не удалось создать {}", directory.display()))?;
    let content = serde_json::to_string_pretty(state)?;
    fs::write(directory.join(INDEX_FILE), content)?;
    Ok(())
}

pub fn append_semantic_audit(
    workspace: &Workspace,
    action: &str,
    node_id: Option<&str>,
    tag_ids: &[String],
    detail: &str,
) -> anyhow::Result<()> {
    let directory = semantic_data_dir(workspace);
    fs::create_dir_all(&directory)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(AUDIT_FILE))?;
    let record = json!({
        "timestamp": unix_timestamp(),
        "action": action,
        "node_id": node_id,
        "tag_ids": tag_ids,
        "detail": detail,
    });
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(())
}

pub fn export_semantic_index(workspace: &Workspace) -> anyhow::Result<String> {
    let state = load_semantic_index(workspace);
    let path = ".leetcode/semantic-labels.json";
    workspace.write_text(path, &serde_json::to_string_pretty(&state)?)?;
    append_semantic_audit(workspace, "export", None, &[], path)?;
    Ok(path.to_string())
}

pub fn upsert_assignment(
    assignments: &mut Vec<SemanticTagAssignment>,
    assignment: SemanticTagAssignment,
) {
    if let Some(existing) = assignments
        .iter_mut()
        .find(|existing| existing.tag_id == assignment.tag_id)
    {
        if existing.locked && !assignment.confirmed_by_user {
            return;
        }
        *existing = assignment;
    } else {
        assignments.push(assignment);
    }
    assignments.sort_by(|left, right| left.tag_id.cmp(&right.tag_id));
}

pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
