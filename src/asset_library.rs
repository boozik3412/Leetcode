use crate::agent::types::ToolResult;
use crate::assets::{load_jobs, AssetKind, AssetStatus};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const LIBRARY_PATH: &str = "assets/generated/leetcode/asset_library.json";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AssetLibrary {
    #[serde(default)]
    pub entries: Vec<AssetLibraryEntry>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetLibraryEntry {
    pub path: String,
    pub kind: AssetKind,
    pub source_job_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub license: String,
    pub updated_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct TagAssetArgs {
    pub path: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FavoriteAssetArgs {
    pub path: String,
    pub favorite: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExportAssetPackArgs {
    pub target_dir: Option<String>,
    pub tag: Option<String>,
    pub favorites_only: Option<bool>,
}

pub fn load_library(workspace: &Workspace) -> AssetLibrary {
    let saved = workspace
        .read_text(LIBRARY_PATH, 1_000_000)
        .ok()
        .and_then(|text| serde_json::from_str::<AssetLibrary>(&text).ok())
        .unwrap_or_default();
    merge_jobs(workspace, saved)
}

pub fn save_library(workspace: &Workspace, library: &AssetLibrary) -> anyhow::Result<()> {
    workspace.write_text(LIBRARY_PATH, &serde_json::to_string_pretty(library)?)
}

pub fn asset_library_snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&load_library(workspace))
            .unwrap_or_else(|_| "библиотека ассетов".to_string()),
    )
}

pub fn tag_asset(workspace: &Workspace, args: TagAssetArgs) -> ToolResult {
    let path = normalize_path(&args.path);
    if path.is_empty() {
        return ToolResult::error("путь ассета пустой");
    }
    let mut library = load_library(workspace);
    let Some(entry) = library.entries.iter_mut().find(|entry| entry.path == path) else {
        return ToolResult::error(format!("ассет не найден в библиотеке: {path}"));
    };
    entry.tags = normalize_tags(args.tags);
    if let Some(notes) = args.notes {
        entry.notes = notes;
    }
    entry.updated_at = unix_timestamp();
    library.updated_at = entry.updated_at;
    match save_library(workspace, &library) {
        Ok(()) => ToolResult::ok(format!("теги обновлены: {path}")),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn favorite_asset(workspace: &Workspace, args: FavoriteAssetArgs) -> ToolResult {
    let path = normalize_path(&args.path);
    let mut library = load_library(workspace);
    let Some(entry) = library.entries.iter_mut().find(|entry| entry.path == path) else {
        return ToolResult::error(format!("ассет не найден в библиотеке: {path}"));
    };
    entry.favorite = args.favorite;
    entry.updated_at = unix_timestamp();
    library.updated_at = entry.updated_at;
    match save_library(workspace, &library) {
        Ok(()) => ToolResult::ok(format!("{path} избранное: {}", yes_no_ru(args.favorite))),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn export_asset_pack(workspace: &Workspace, args: ExportAssetPackArgs) -> ToolResult {
    let library = load_library(workspace);
    let tag = args.tag.map(|tag| tag.trim().to_ascii_lowercase());
    let favorites_only = args.favorites_only.unwrap_or(false);
    let target_dir = args
        .target_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("assets/generated/exports/asset-pack");
    let target_root = match workspace.resolve_for_write(target_dir) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if let Err(err) = fs::create_dir_all(&target_root) {
        return ToolResult::error(err.to_string());
    }

    let mut copied = Vec::new();
    for entry in library.entries.iter().filter(|entry| {
        (!favorites_only || entry.favorite)
            && tag
                .as_deref()
                .map(|tag| {
                    entry
                        .tags
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(tag))
                })
                .unwrap_or(true)
    }) {
        let Ok(source) = workspace.resolve_existing(&entry.path) else {
            continue;
        };
        let Some(name) = source.file_name() else {
            continue;
        };
        let target = target_root.join(name);
        if fs::copy(&source, &target).is_ok() {
            copied.push(entry.path.clone());
        }
    }

    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "target_dir": target_dir,
            "copied": copied
        }))
        .unwrap_or_else(|_| "пак ассетов экспортирован".to_string()),
    )
}

fn yes_no_ru(value: bool) -> &'static str {
    if value {
        "да"
    } else {
        "нет"
    }
}

fn merge_jobs(workspace: &Workspace, mut library: AssetLibrary) -> AssetLibrary {
    let now = unix_timestamp();
    for job in load_jobs(workspace)
        .into_iter()
        .filter(|job| job.status == AssetStatus::Done)
    {
        for path in job.output_files {
            if library.entries.iter().any(|entry| entry.path == path) {
                continue;
            }
            let license = job
                .metadata
                .get("license")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("provider terms")
                .to_string();
            library.entries.push(AssetLibraryEntry {
                path,
                kind: job.kind.clone(),
                source_job_id: Some(job.id.clone()),
                provider: job.provider.clone(),
                model: job.model.clone(),
                prompt: job.prompt.clone(),
                tags: default_tags(&job.kind),
                favorite: false,
                notes: String::new(),
                license,
                updated_at: now,
            });
        }
    }
    library.entries.sort_by(|a, b| a.path.cmp(&b.path));
    library.updated_at = now;
    library
}

fn default_tags(kind: &AssetKind) -> Vec<String> {
    match kind {
        AssetKind::Image => vec!["image".to_string()],
        AssetKind::Spritesheet => vec!["spritesheet".to_string()],
        AssetKind::Audio => vec!["audio".to_string()],
        AssetKind::Video => vec!["video".to_string()],
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_ascii_lowercase().replace(' ', "-");
        if !tag.is_empty() && !normalized.iter().any(|known| known == &tag) {
            normalized.push(tag);
        }
    }
    normalized
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
