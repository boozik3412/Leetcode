use crate::unreal::unreal_snapshot;
use crate::workspace::Workspace;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

pub const GENERATED_ASSET_REGISTRY_PATH: &str =
    "assets/generated/leetcode/unreal/asset_registry.json";
const MAX_TEXT_BYTES: u64 = 2_000_000;
const MAX_ASSET_REGISTRY_BYTES: u64 = 96_000_000;
const MAX_SOURCE_FILES: usize = 4_000;
const MAX_CONTENT_ASSETS: usize = 8_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UnrealProjectIntelligence {
    pub descriptors: Vec<UnrealDescriptorInfo>,
    pub modules: Vec<UnrealBuildModule>,
    pub targets: Vec<UnrealTargetInfo>,
    pub configs: Vec<UnrealConfigInfo>,
    pub source_files: Vec<UnrealSourceInfo>,
    pub assets: Vec<UnrealAssetInfo>,
    pub asset_dependencies: Vec<UnrealAssetDependency>,
    pub registry_export_path: Option<String>,
    pub fingerprint: String,
    #[serde(default)]
    pub project_inputs: Vec<UnrealProjectInput>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnrealProjectInput {
    pub path: String,
    pub size: u64,
    pub modified_ns: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealDescriptorInfo {
    pub kind: String,
    pub name: String,
    pub path: String,
    pub engine_association: Option<String>,
    pub modules: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealBuildModule {
    pub name: String,
    pub path: String,
    pub module_type: Option<String>,
    pub public_dependencies: Vec<String>,
    pub private_dependencies: Vec<String>,
    pub dynamic_dependencies: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealTargetInfo {
    pub name: String,
    pub path: String,
    pub target_type: Option<String>,
    pub modules: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealConfigInfo {
    pub path: String,
    pub sections: Vec<String>,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealSourceInfo {
    pub path: String,
    pub module: Option<String>,
    pub language: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnrealAssetKind {
    Map,
    Blueprint,
    DataAsset,
    Material,
    Niagara,
    Animation,
    Asset,
}

impl UnrealAssetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Blueprint => "blueprint",
            Self::DataAsset => "data_asset",
            Self::Material => "material",
            Self::Niagara => "niagara",
            Self::Animation => "animation",
            Self::Asset => "asset",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealAssetInfo {
    pub id: String,
    pub name: String,
    pub object_path: String,
    pub package_name: String,
    pub class_name: String,
    pub kind: UnrealAssetKind,
    pub file_path: Option<String>,
    pub tags: BTreeMap<String, String>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealAssetDependency {
    pub from: String,
    pub to: String,
    pub dependency_type: String,
}

pub fn scan_unreal_project(workspace: &Workspace) -> UnrealProjectIntelligence {
    let snapshot = unreal_snapshot(workspace);
    let root = workspace.root();
    let mut descriptors = Vec::new();
    let mut declared_module_types = BTreeMap::new();

    if let Some(project) = snapshot.project {
        for module in &project.modules {
            declared_module_types.insert(module.name.clone(), module.module_type.clone());
        }
        descriptors.push(UnrealDescriptorInfo {
            kind: "project".to_string(),
            name: project.name,
            path: relative_path(root, Path::new(&project.path)),
            engine_association: project.engine_association,
            modules: project
                .modules
                .into_iter()
                .map(|module| module.name)
                .collect(),
        });
    }
    for plugin in snapshot.local_plugins {
        for module in &plugin.modules {
            declared_module_types.insert(module.name.clone(), module.module_type.clone());
        }
        descriptors.push(UnrealDescriptorInfo {
            kind: "plugin".to_string(),
            name: plugin.name,
            path: relative_path(root, Path::new(&plugin.path)),
            engine_association: None,
            modules: plugin
                .modules
                .into_iter()
                .map(|module| module.name)
                .collect(),
        });
    }
    descriptors.sort_by(|left, right| left.path.cmp(&right.path));

    let mut modules = Vec::new();
    let mut targets = Vec::new();
    let mut configs = Vec::new();
    let mut source_files = Vec::new();
    let mut project_inputs = Vec::new();

    for entry in WalkDir::new(root)
        .max_depth(14)
        .into_iter()
        .filter_entry(is_project_entry)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        let rel = relative_path(root, path);
        let lower = rel.to_ascii_lowercase();
        if let Ok(metadata) = entry.metadata() {
            if is_project_map_input(&lower) {
                project_inputs.push(project_input_signature(rel.clone(), &metadata));
            }
        }
        if lower.ends_with(".build.cs") {
            if let Some(text) = read_bounded(path) {
                let name = build_file_stem(path, ".Build.cs");
                modules.push(UnrealBuildModule {
                    module_type: declared_module_types.get(&name).cloned(),
                    public_dependencies: dependency_values(&text, &["PublicDependencyModuleNames"]),
                    private_dependencies: dependency_values(
                        &text,
                        &["PrivateDependencyModuleNames"],
                    ),
                    dynamic_dependencies: dependency_values(
                        &text,
                        &["DynamicallyLoadedModuleNames"],
                    ),
                    name,
                    path: rel,
                });
            }
        } else if lower.ends_with(".target.cs") {
            if let Some(text) = read_bounded(path) {
                targets.push(UnrealTargetInfo {
                    name: build_file_stem(path, ".Target.cs"),
                    path: rel,
                    target_type: capture_first(&text, r"TargetType\.([A-Za-z0-9_]+)"),
                    modules: dependency_values(&text, &["ExtraModuleNames"]),
                });
            }
        } else if lower.ends_with(".ini") && is_config_path(&lower) {
            if let Some(text) = read_bounded(path) {
                let (sections, keys) = parse_ini_outline(&text);
                configs.push(UnrealConfigInfo {
                    path: rel,
                    sections,
                    keys,
                });
            }
        } else if is_source_path(&lower) && source_files.len() < MAX_SOURCE_FILES {
            if let Some(language) = source_language(&lower) {
                source_files.push(UnrealSourceInfo {
                    module: module_from_source_path(&rel),
                    path: rel,
                    language: language.to_string(),
                });
            }
        }
    }

    modules.sort_by(|left, right| left.name.cmp(&right.name));
    targets.sort_by(|left, right| left.name.cmp(&right.name));
    configs.sort_by(|left, right| left.path.cmp(&right.path));
    source_files.sort_by(|left, right| left.path.cmp(&right.path));

    let (mut assets, mut asset_dependencies, registry_export_path, mut diagnostics) =
        load_asset_registry_export(workspace);
    let registry_keys = assets
        .iter()
        .flat_map(|asset| [asset.object_path.clone(), asset.package_name.clone()])
        .collect::<BTreeSet<_>>();
    for asset in scan_content_files(root) {
        if !registry_keys.contains(&asset.object_path)
            && !registry_keys.contains(&asset.package_name)
        {
            assets.push(asset);
        }
    }
    assets.sort_by(|left, right| left.object_path.cmp(&right.object_path));
    assets.dedup_by(|left, right| left.id == right.id);
    asset_dependencies.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then(left.to.cmp(&right.to))
            .then(left.dependency_type.cmp(&right.dependency_type))
    });
    asset_dependencies.dedup_by(|left, right| {
        left.from == right.from
            && left.to == right.to
            && left.dependency_type == right.dependency_type
    });

    if descriptors.is_empty() {
        diagnostics.push("Не найдено .uproject/.uplugin: Unreal-граф не активирован".to_string());
    }
    if registry_export_path.is_none() {
        diagnostics.push(format!(
            "Asset Registry JSON не найден; импортированы только имена .uasset/.umap. Сохраните экспорт в {GENERATED_ASSET_REGISTRY_PATH} для точных классов и зависимостей"
        ));
    }
    if let Some(registry_path) = &registry_export_path {
        if !project_inputs
            .iter()
            .any(|input| input.path.eq_ignore_ascii_case(registry_path))
        {
            if let Ok(metadata) = fs::metadata(root.join(registry_path)) {
                project_inputs.push(project_input_signature(registry_path.clone(), &metadata));
            }
        }
    }
    project_inputs.sort_by(|left, right| left.path.cmp(&right.path));
    project_inputs.dedup_by(|left, right| left.path.eq_ignore_ascii_case(&right.path));
    let mut hasher = Sha256::new();
    for input in &project_inputs {
        hasher.update(input.path.as_bytes());
        hasher.update(b":");
        hasher.update(input.size.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(input.modified_ns.to_string().as_bytes());
        hasher.update(b"\n");
    }

    UnrealProjectIntelligence {
        descriptors,
        modules,
        targets,
        configs,
        source_files,
        assets,
        asset_dependencies,
        registry_export_path,
        fingerprint: format!("{:x}", hasher.finalize()),
        project_inputs,
        diagnostics,
    }
}

fn project_input_signature(path: String, metadata: &fs::Metadata) -> UnrealProjectInput {
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or_default();
    UnrealProjectInput {
        path,
        size: metadata.len(),
        modified_ns,
    }
}

fn is_project_map_input(lower: &str) -> bool {
    if lower == GENERATED_ASSET_REGISTRY_PATH.to_ascii_lowercase() {
        return true;
    }
    if lower.starts_with("assets/generated/leetcode/") {
        return false;
    }
    lower.ends_with(".uproject")
        || lower.ends_with(".uplugin")
        || lower.ends_with(".uprojectdirs")
        || lower.starts_with("source/")
        || lower.contains("/source/")
        || lower.starts_with("content/")
        || lower.contains("/content/")
        || lower.starts_with("config/")
        || lower.contains("/config/")
        || lower.starts_with("build/")
        || lower.contains("/build/")
}

fn load_asset_registry_export(
    workspace: &Workspace,
) -> (
    Vec<UnrealAssetInfo>,
    Vec<UnrealAssetDependency>,
    Option<String>,
    Vec<String>,
) {
    let candidates = asset_registry_candidates(workspace.root());
    for path in candidates {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_ASSET_REGISTRY_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        match serde_json::from_str::<Value>(&text).map(|value| parse_asset_registry_value(&value)) {
            Ok((assets, dependencies)) if !assets.is_empty() => {
                return (
                    assets,
                    dependencies,
                    Some(relative_path(workspace.root(), &path)),
                    Vec::new(),
                );
            }
            Ok(_) => continue,
            Err(error) => {
                return (
                    Vec::new(),
                    Vec::new(),
                    Some(relative_path(workspace.root(), &path)),
                    vec![format!("Некорректный Asset Registry JSON: {error}")],
                );
            }
        }
    }
    (Vec::new(), Vec::new(), None, Vec::new())
}

fn parse_asset_registry_value(value: &Value) -> (Vec<UnrealAssetInfo>, Vec<UnrealAssetDependency>) {
    let assets_value = value
        .as_array()
        .or_else(|| object_value_ci(value, &["assets", "asset_data"]).and_then(Value::as_array));
    let mut assets = Vec::new();
    let mut dependencies = Vec::new();
    for item in assets_value.into_iter().flatten() {
        let Some(asset) = parse_asset(item) else {
            continue;
        };
        if let Some(items) =
            object_value_ci(item, &["dependencies", "depends_on"]).and_then(Value::as_array)
        {
            dependencies.extend(items.iter().filter_map(|dependency| {
                dependency_target(dependency).map(|(target, dependency_type)| {
                    UnrealAssetDependency {
                        from: asset.object_path.clone(),
                        to: target,
                        dependency_type,
                    }
                })
            }));
        }
        assets.push(asset);
    }

    if let Some(top_level) = object_value_ci(value, &["dependencies", "asset_dependencies"]) {
        if let Some(items) = top_level.as_array() {
            dependencies.extend(items.iter().filter_map(parse_dependency_record));
        } else if let Some(object) = top_level.as_object() {
            for (from, values) in object {
                if let Some(items) = values.as_array() {
                    dependencies.extend(items.iter().filter_map(|dependency| {
                        dependency_target(dependency).map(|(to, dependency_type)| {
                            UnrealAssetDependency {
                                from: from.clone(),
                                to,
                                dependency_type,
                            }
                        })
                    }));
                }
            }
        }
    }
    (assets, dependencies)
}

fn parse_asset(value: &Value) -> Option<UnrealAssetInfo> {
    let object_path = string_value_ci(
        value,
        &[
            "object_path",
            "objectpath",
            "object_path_name",
            "objectpathname",
        ],
    )
    .filter(|path| !path.trim().is_empty())
    .or_else(|| {
        let package = string_value_ci(value, &["package_name", "packagename"])?;
        let name = string_value_ci(value, &["asset_name", "assetname", "name"])
            .or_else(|| package.rsplit('/').next().map(ToString::to_string))?;
        Some(format!("{package}.{name}"))
    })?;
    let package_name =
        string_value_ci(value, &["package_name", "packagename"]).unwrap_or_else(|| {
            object_path
                .split('.')
                .next()
                .unwrap_or(&object_path)
                .to_string()
        });
    let name = string_value_ci(value, &["asset_name", "assetname", "name"]).unwrap_or_else(|| {
        object_path
            .rsplit(['/', '.'])
            .next()
            .unwrap_or("Asset")
            .to_string()
    });
    let class_name = string_value_ci(
        value,
        &[
            "asset_class",
            "assetclass",
            "asset_class_path",
            "assetclasspath",
            "class",
        ],
    )
    .unwrap_or_else(|| "Unknown".to_string());
    let tags = object_value_ci(value, &["tags", "tags_and_values", "tagsandvalues"])
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .take(80)
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let kind = classify_asset(&class_name, &object_path, None);
    Some(UnrealAssetInfo {
        id: asset_id(&object_path),
        name,
        object_path,
        package_name,
        class_name,
        kind,
        file_path: string_value_ci(value, &["file_path", "filepath"]),
        tags,
        source: "asset_registry".to_string(),
    })
}

fn parse_dependency_record(value: &Value) -> Option<UnrealAssetDependency> {
    let from = string_value_ci(value, &["from", "source", "package"])?;
    let to = string_value_ci(value, &["to", "target", "dependency"])?;
    let dependency_type = string_value_ci(value, &["type", "category", "dependency_type"])
        .unwrap_or_else(|| "package".to_string());
    Some(UnrealAssetDependency {
        from,
        to,
        dependency_type,
    })
}

fn dependency_target(value: &Value) -> Option<(String, String)> {
    if let Some(target) = value.as_str() {
        return Some((target.to_string(), "package".to_string()));
    }
    let target = string_value_ci(value, &["to", "target", "package", "dependency"])?;
    let dependency_type = string_value_ci(value, &["type", "category", "dependency_type"])
        .unwrap_or_else(|| "package".to_string());
    Some((target, dependency_type))
}

fn scan_content_files(root: &Path) -> Vec<UnrealAssetInfo> {
    let mut assets = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(18)
        .into_iter()
        .filter_entry(is_content_entry)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if assets.len() >= MAX_CONTENT_ASSETS {
            break;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(extension.to_ascii_lowercase().as_str(), "uasset" | "umap") {
            continue;
        }
        let rel = relative_path(root, entry.path());
        let Some((mount, package_rel)) = unreal_mount_and_package(&rel) else {
            continue;
        };
        let stem = entry
            .path()
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Asset");
        let package_name = format!(
            "/{mount}/{}",
            package_rel.trim_end_matches(&format!(".{extension}"))
        );
        let object_path = format!("{package_name}.{stem}");
        let class_name = if extension.eq_ignore_ascii_case("umap") {
            "World"
        } else {
            "Unknown"
        };
        assets.push(UnrealAssetInfo {
            id: asset_id(&object_path),
            name: stem.to_string(),
            kind: classify_asset(class_name, &object_path, Some(extension)),
            object_path,
            package_name,
            class_name: class_name.to_string(),
            file_path: Some(rel),
            tags: BTreeMap::new(),
            source: "content_scan".to_string(),
        });
    }
    assets
}

fn classify_asset(class_name: &str, object_path: &str, extension: Option<&str>) -> UnrealAssetKind {
    let text = format!("{class_name} {object_path}").to_ascii_lowercase();
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("umap"))
        || text.contains("world")
        || text.contains("/maps/")
    {
        UnrealAssetKind::Map
    } else if text.contains("blueprint") || text.contains("/blueprints/") {
        UnrealAssetKind::Blueprint
    } else if text.contains("dataasset") || text.contains("data_asset") {
        UnrealAssetKind::DataAsset
    } else if text.contains("material") || text.contains("/materials/") {
        UnrealAssetKind::Material
    } else if text.contains("niagara") {
        UnrealAssetKind::Niagara
    } else if ["anim", "skeleton", "montage", "blendspace"]
        .iter()
        .any(|needle| text.contains(needle))
    {
        UnrealAssetKind::Animation
    } else {
        UnrealAssetKind::Asset
    }
}

fn asset_registry_candidates(root: &Path) -> Vec<PathBuf> {
    let known = [
        GENERATED_ASSET_REGISTRY_PATH,
        "Saved/Leetcode/AssetRegistry.json",
        "Saved/AssetRegistryExport.json",
        "Saved/AssetRegistry/AssetRegistry.json",
    ];
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    for path in known.iter().map(|path| root.join(path)) {
        if path.is_file() && seen.insert(path.clone()) {
            paths.push(path);
        }
    }
    for base in [
        root.join("Saved"),
        root.join("assets/generated/leetcode/unreal"),
    ] {
        if !base.is_dir() {
            continue;
        }
        for entry in WalkDir::new(base)
            .max_depth(5)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".json")
                && (name.contains("assetregistry") || name.contains("asset_registry"))
            {
                let path = entry.into_path();
                if seen.insert(path.clone()) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

fn dependency_values(text: &str, fields: &[&str]) -> Vec<String> {
    let quoted = Regex::new(r#"[\"']([A-Za-z0-9_./-]+)[\"']"#).expect("quoted regex");
    let mut values = Vec::new();
    for field in fields {
        let pattern = format!(
            r"(?s){}\s*\.\s*Add(?:Range)?\s*\((.*?)\)\s*;",
            regex::escape(field)
        );
        let Ok(call) = Regex::new(&pattern) else {
            continue;
        };
        for capture in call.captures_iter(text) {
            let body = capture
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            values.extend(
                quoted
                    .captures_iter(body)
                    .filter_map(|value| value.get(1).map(|item| item.as_str().to_string())),
            );
        }
    }
    values.sort();
    values.dedup();
    values
}

fn parse_ini_outline(text: &str) -> (Vec<String>, Vec<String>) {
    let mut sections = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut current_section = String::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line.trim_matches(&['[', ']'][..]).to_string();
            sections.insert(current_section.clone());
        } else if let Some((key, _)) = line.split_once('=') {
            let key = key.trim().trim_start_matches(['+', '-', '!', '.']);
            if !key.is_empty() {
                keys.insert(if current_section.is_empty() {
                    key.to_string()
                } else {
                    format!("{current_section}.{key}")
                });
            }
        }
    }
    (
        sections.into_iter().take(80).collect(),
        keys.into_iter().take(160).collect(),
    )
}

fn object_value_ci<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    object.iter().find_map(|(key, value)| {
        keys.iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
            .then_some(value)
    })
}

fn string_value_ci(value: &Value, keys: &[&str]) -> Option<String> {
    object_value_ci(value, keys).and_then(|value| {
        value.as_str().map(ToString::to_string).or_else(|| {
            value
                .get("AssetName")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
    })
}

fn capture_first(text: &str, pattern: &str) -> Option<String> {
    Regex::new(pattern)
        .ok()?
        .captures(text)?
        .get(1)
        .map(|value| value.as_str().to_string())
}

fn build_file_stem(path: &Path, suffix: &str) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_suffix(suffix))
        .unwrap_or("UnrealModule")
        .to_string()
}

fn read_bounded(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    (metadata.len() <= MAX_TEXT_BYTES)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

fn is_project_entry(entry: &DirEntry) -> bool {
    !matches!(
        entry.file_name().to_str(),
        Some(
            ".git"
                | "Binaries"
                | "DerivedDataCache"
                | "Intermediate"
                | "Saved"
                | "target"
                | "node_modules"
        )
    )
}

fn is_content_entry(entry: &DirEntry) -> bool {
    !matches!(
        entry.file_name().to_str(),
        Some(
            ".git"
                | "Binaries"
                | "DerivedDataCache"
                | "Intermediate"
                | "Saved"
                | "target"
                | "node_modules"
        )
    )
}

fn is_config_path(lower: &str) -> bool {
    lower.starts_with("config/") || lower.contains("/config/")
}

fn is_source_path(lower: &str) -> bool {
    lower.starts_with("source/") || lower.contains("/source/")
}

fn source_language(lower: &str) -> Option<&'static str> {
    if lower.ends_with(".cpp") || lower.ends_with(".cc") || lower.ends_with(".c") {
        Some("cpp")
    } else if lower.ends_with(".h") || lower.ends_with(".hpp") {
        Some("cpp_header")
    } else if lower.ends_with(".cs") {
        Some("csharp")
    } else {
        None
    }
}

fn module_from_source_path(rel: &str) -> Option<String> {
    let parts = rel.split('/').collect::<Vec<_>>();
    let index = parts.iter().rposition(|part| *part == "Source")?;
    parts.get(index + 1).map(|value| (*value).to_string())
}

fn unreal_mount_and_package(rel: &str) -> Option<(String, String)> {
    let parts = rel.split('/').collect::<Vec<_>>();
    if parts.first().is_some_and(|value| *value == "Content") {
        return Some(("Game".to_string(), parts[1..].join("/")));
    }
    let plugins = parts.iter().position(|part| *part == "Plugins")?;
    let content = parts.iter().position(|part| *part == "Content")?;
    if content <= plugins + 1 {
        return None;
    }
    Some((
        parts[plugins + 1].to_string(),
        parts[content + 1..].join("/"),
    ))
}

fn asset_id(object_path: &str) -> String {
    format!(
        "unreal:asset:{}",
        object_path
            .trim()
            .replace('\\', "/")
            .replace([' ', '#'], "-")
    )
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_workspace() -> Workspace {
        Workspace::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unreal/SampleGame"),
        )
        .expect("fixture workspace")
    }

    #[test]
    fn scans_unreal_modules_targets_config_source_and_assets() {
        let snapshot = scan_unreal_project(&fixture_workspace());

        assert!(snapshot
            .descriptors
            .iter()
            .any(|item| item.name == "SampleGame"));
        assert!(snapshot.modules.iter().any(|module| {
            module.name == "SampleGame"
                && module.public_dependencies.contains(&"Engine".to_string())
        }));
        assert!(snapshot.targets.iter().any(|target| {
            target.name == "SampleGameEditor" && target.target_type.as_deref() == Some("Editor")
        }));
        assert!(snapshot
            .configs
            .iter()
            .any(|config| config.path == "Config/DefaultEngine.ini"));
        assert!(snapshot
            .source_files
            .iter()
            .any(|source| source.path.ends_with("SampleActor.cpp")));
        assert!(snapshot
            .assets
            .iter()
            .any(|asset| asset.kind == UnrealAssetKind::Map));
        assert!(snapshot
            .assets
            .iter()
            .any(|asset| asset.kind == UnrealAssetKind::Blueprint));
        assert!(snapshot.asset_dependencies.iter().any(|dependency| {
            dependency.from.contains("BP_Player") && dependency.to.contains("DA_PlayerTuning")
        }));
        assert_eq!(
            snapshot.registry_export_path.as_deref(),
            Some(GENERATED_ASSET_REGISTRY_PATH)
        );
    }

    #[test]
    fn parses_flexible_asset_registry_keys() {
        let value = serde_json::json!({
            "Assets": [{
                "ObjectPath": "/Game/VFX/NS_Dash.NS_Dash",
                "PackageName": "/Game/VFX/NS_Dash",
                "AssetName": "NS_Dash",
                "AssetClass": "NiagaraSystem",
                "Dependencies": [{"Target": "/Game/Materials/M_Dash", "Type": "hard"}]
            }]
        });
        let (assets, dependencies) = parse_asset_registry_value(&value);
        assert_eq!(assets[0].kind, UnrealAssetKind::Niagara);
        assert_eq!(dependencies[0].dependency_type, "hard");
    }

    #[test]
    fn reconstructs_missing_object_path_from_package_and_asset_name() {
        let value = serde_json::json!({
            "object_path": "",
            "package_name": "/Game/Characters/Hero/BP_Hero",
            "asset_name": "BP_Hero",
            "asset_class": "Blueprint"
        });

        let asset = parse_asset(&value).expect("asset should be reconstructed");

        assert_eq!(asset.object_path, "/Game/Characters/Hero/BP_Hero.BP_Hero");
        assert_eq!(asset.kind, UnrealAssetKind::Blueprint);
    }

    #[test]
    fn asset_registry_limit_accepts_realistic_unreal_exports() {
        assert!(MAX_ASSET_REGISTRY_BYTES >= 90_000_000);
        assert!(MAX_ASSET_REGISTRY_BYTES > MAX_TEXT_BYTES);
    }

    #[test]
    fn leetcode_generated_files_do_not_invalidate_unreal_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("Source/SampleGame")).unwrap();
        fs::write(
            temp.path().join("SampleGame.uproject"),
            r#"{"FileVersion":3,"EngineAssociation":"5.8","Modules":[]}"#,
        )
        .unwrap();
        fs::write(
            temp.path().join("Source/SampleGame/SampleActor.cpp"),
            "void SampleActor() {}",
        )
        .unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let before = scan_unreal_project(&workspace);

        workspace
            .write_text(
                "assets/generated/leetcode/game-task-builder/state.json",
                r#"{"last_scan":"changed"}"#,
            )
            .unwrap();
        workspace
            .write_text(
                "assets/generated/leetcode/project_graph.json",
                r#"{"nodes":[],"edges":[]}"#,
            )
            .unwrap();
        let after = scan_unreal_project(&workspace);

        assert_eq!(before.fingerprint, after.fingerprint);
        assert!(after
            .project_inputs
            .iter()
            .all(|input| !input.path.starts_with("assets/generated/leetcode/")));
    }

    #[test]
    fn unreal_source_change_updates_project_input_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("Source/SampleGame")).unwrap();
        fs::write(
            temp.path().join("SampleGame.uproject"),
            r#"{"FileVersion":3,"EngineAssociation":"5.8","Modules":[]}"#,
        )
        .unwrap();
        let source = temp.path().join("Source/SampleGame/SampleActor.cpp");
        fs::write(&source, "void SampleActor() {}").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let before = scan_unreal_project(&workspace);

        fs::write(&source, "void SampleActor() { int changed = 1; }").unwrap();
        let after = scan_unreal_project(&workspace);

        assert_ne!(before.fingerprint, after.fingerprint);
        assert!(after
            .project_inputs
            .iter()
            .any(|input| input.path.ends_with("SampleActor.cpp")));
    }
}
