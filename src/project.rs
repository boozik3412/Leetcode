use crate::workspace::Workspace;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectProfile {
    pub kind: String,
    pub name: String,
    pub markers: Vec<String>,
    pub commands: Vec<ProjectCommand>,
    pub previews: Vec<ProjectPreviewHook>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectCommand {
    pub id: String,
    pub label: String,
    pub command: String,
    pub cwd: String,
    pub description: String,
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPreviewHook {
    pub id: String,
    pub label: String,
    pub url: Option<String>,
    pub command_id: Option<String>,
    pub description: String,
}

impl ProjectPreviewHook {
    fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        url: Option<String>,
        command_id: Option<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            url,
            command_id,
            description: description.into(),
        }
    }
}

impl ProjectCommand {
    fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        command: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            command: command.into(),
            cwd: ".".to_string(),
            description: description.into(),
            timeout_secs: 1_800,
        }
    }
}

pub fn detect_project_profiles(workspace: &Workspace) -> Vec<ProjectProfile> {
    let root = workspace.root();
    let mut profiles = Vec::new();

    if root.join("Cargo.toml").is_file() {
        profiles.push(detect_rust_profile(root));
    }
    if root.join("package.json").is_file() {
        profiles.push(detect_node_profile(root));
    }
    if root.join("pyproject.toml").is_file() || root.join("requirements.txt").is_file() {
        profiles.push(detect_python_profile(root));
    }
    if root.join("project.godot").is_file() {
        profiles.push(ProjectProfile {
            kind: "Godot".to_string(),
            name: workspace.display_name(),
            markers: vec!["project.godot".to_string()],
            commands: vec![
                ProjectCommand::new("run", "Запуск", "godot --path .", "Запустить Godot-проект"),
                ProjectCommand::new(
                    "editor",
                    "Редактор",
                    "godot --path . --editor",
                    "Открыть редактор Godot",
                ),
            ],
            previews: vec![ProjectPreviewHook::new(
                "godot-editor",
                "Редактор Godot",
                None,
                Some("editor".to_string()),
                "Открыть редактор Godot для локального предпросмотра/плейтеста",
            )],
        });
    }
    if root
        .join("ProjectSettings")
        .join("ProjectVersion.txt")
        .is_file()
        && root.join("Assets").is_dir()
    {
        profiles.push(ProjectProfile {
            kind: "Unity".to_string(),
            name: workspace.display_name(),
            markers: vec![
                "ProjectSettings/ProjectVersion.txt".to_string(),
                "Assets/".to_string(),
            ],
            commands: Vec::new(),
            previews: Vec::new(),
        });
    }
    if let Some(uproject) = first_file_with_extension(root, "uproject") {
        let uproject_name = uproject
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project.uproject")
            .to_string();
        profiles.push(ProjectProfile {
            kind: "Unreal".to_string(),
            name: uproject_name.clone(),
            markers: vec![uproject_name.clone()],
            commands: vec![ProjectCommand::new(
                "editor",
                "Редактор",
                format!("UnrealEditor \"{uproject_name}\""),
                "Открыть Unreal-проект в редакторе",
            )],
            previews: vec![ProjectPreviewHook::new(
                "unreal-editor",
                "Редактор Unreal",
                None,
                Some("editor".to_string()),
                "Открыть редактор Unreal для локального предпросмотра/плейтеста",
            )],
        });
    }

    profiles
}

pub fn find_project_command(
    profiles: &[ProjectProfile],
    command: &str,
    profile_filter: Option<&str>,
) -> Option<ProjectCommand> {
    let wanted = normalize(command);
    let profile_filter = profile_filter.map(normalize);

    profiles
        .iter()
        .filter(|profile| {
            profile_filter
                .as_ref()
                .map(|filter| {
                    normalize(&profile.kind) == *filter || normalize(&profile.name) == *filter
                })
                .unwrap_or(true)
        })
        .flat_map(|profile| profile.commands.iter())
        .find(|candidate| {
            normalize(&candidate.id) == wanted
                || normalize(&candidate.label) == wanted
                || normalize(&candidate.command) == wanted
        })
        .cloned()
}

pub fn find_project_preview(
    profiles: &[ProjectProfile],
    preview: &str,
    profile_filter: Option<&str>,
) -> Option<ProjectPreviewHook> {
    let wanted = normalize(preview);
    let profile_filter = profile_filter.map(normalize);

    profiles
        .iter()
        .filter(|profile| {
            profile_filter
                .as_ref()
                .map(|filter| {
                    normalize(&profile.kind) == *filter || normalize(&profile.name) == *filter
                })
                .unwrap_or(true)
        })
        .flat_map(|profile| profile.previews.iter())
        .find(|candidate| {
            normalize(&candidate.id) == wanted || normalize(&candidate.label) == wanted
        })
        .cloned()
}

pub fn describe_project_commands(profiles: &[ProjectProfile]) -> String {
    if profiles.is_empty() {
        return "Профили проекта не обнаружены".to_string();
    }

    let mut lines = Vec::new();
    for profile in profiles {
        if profile.commands.is_empty() {
            lines.push(format!(
                "{} {}: быстрые команды не зарегистрированы",
                profile.kind, profile.name
            ));
            continue;
        }
        let commands = profile
            .commands
            .iter()
            .map(|command| format!("{} => {}", command.id, command.command))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("{} {}: {commands}", profile.kind, profile.name));
    }
    lines.join("\n")
}

fn detect_rust_profile(root: &Path) -> ProjectProfile {
    let name = fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|toml| package_name_from_cargo_toml(&toml))
        .unwrap_or_else(|| "Rust-проект".to_string());
    let mut commands = vec![
        ProjectCommand::new(
            "check",
            "Проверка",
            "cargo check",
            "Проверить компиляцию Rust",
        ),
        ProjectCommand::new("test", "Тесты", "cargo test", "Запустить Rust-тесты"),
        ProjectCommand::new("run", "Запуск", "cargo run", "Запустить Rust-приложение"),
        ProjectCommand::new("build", "Сборка", "cargo build", "Собрать debug-бинарник"),
        ProjectCommand::new(
            "release",
            "Релиз",
            "cargo build --release",
            "Собрать release-бинарник",
        ),
    ];
    if root.join("Trunk.toml").is_file() || root.join("index.html").is_file() {
        commands.push(ProjectCommand::new(
            "serve",
            "Сервер",
            "trunk serve",
            "Запустить Rust/WASM-приложение через Trunk",
        ));
    }

    ProjectProfile {
        kind: "Rust".to_string(),
        name,
        markers: vec!["Cargo.toml".to_string()],
        commands,
        previews: rust_preview_hooks(root),
    }
}

fn detect_node_profile(root: &Path) -> ProjectProfile {
    let package = fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let has_vite = package
        .as_ref()
        .map(|package| has_package_dep(package, "vite"))
        .unwrap_or(false)
        || has_any_marker(
            root,
            &[
                "vite.config.js",
                "vite.config.ts",
                "vite.config.mjs",
                "vite.config.mts",
            ],
        );
    let has_react = package
        .as_ref()
        .map(|package| has_package_dep(package, "react"))
        .unwrap_or(false);
    let has_next = package
        .as_ref()
        .map(|package| has_package_dep(package, "next"))
        .unwrap_or(false);
    let kind = match (has_react, has_vite) {
        (true, true) => "React/Vite",
        (false, true) => "Vite",
        (true, false) => "React",
        (false, false) => "Node",
    };
    let name = package
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Node-проект")
        .to_string();
    let scripts = package
        .as_ref()
        .and_then(|value| value.get("scripts"))
        .and_then(Value::as_object);
    let runner = node_runner(root);
    let mut commands = Vec::new();

    if let Some(scripts) = scripts {
        for id in [
            "dev",
            "start",
            "build",
            "test",
            "lint",
            "preview",
            "format",
            "typecheck",
        ] {
            if scripts.contains_key(id) {
                push_unique_command(
                    &mut commands,
                    ProjectCommand::new(
                        id,
                        script_label(id),
                        format!("{runner} run {id}"),
                        format!("Запустить script из package.json: '{id}'"),
                    ),
                );
            }
        }

        for id in scripts.keys().take(8) {
            if !commands.iter().any(|command| command.id == *id) {
                push_unique_command(
                    &mut commands,
                    ProjectCommand::new(
                        id,
                        script_label(id),
                        format!("{runner} run {id}"),
                        format!("Запустить script из package.json: '{id}'"),
                    ),
                );
            }
        }
    }

    let commands_for_preview = commands.clone();
    ProjectProfile {
        kind: kind.to_string(),
        name,
        markers: node_markers(root),
        commands,
        previews: node_preview_hooks(
            root,
            scripts,
            has_vite,
            has_react,
            has_next,
            &commands_for_preview,
        ),
    }
}

fn detect_python_profile(root: &Path) -> ProjectProfile {
    let mut markers = Vec::new();
    if root.join("pyproject.toml").is_file() {
        markers.push("pyproject.toml".to_string());
    }
    if root.join("requirements.txt").is_file() {
        markers.push("requirements.txt".to_string());
    }

    let mut commands = vec![ProjectCommand::new(
        "test",
        "Тесты",
        "python -m pytest",
        "Запустить Python-тесты через pytest",
    )];
    if root.join("main.py").is_file() {
        commands.push(ProjectCommand::new(
            "run",
            "Запуск",
            "python main.py",
            "Запустить main.py",
        ));
    }
    if root.join("requirements.txt").is_file() {
        commands.push(ProjectCommand::new(
            "install",
            "Установить",
            "python -m pip install -r requirements.txt",
            "Установить Python-зависимости",
        ));
    }

    ProjectProfile {
        kind: "Python".to_string(),
        name: package_name_from_pyproject(root).unwrap_or_else(|| "Python-проект".to_string()),
        markers,
        commands,
        previews: Vec::new(),
    }
}

fn package_name_from_cargo_toml(toml: &str) -> Option<String> {
    let mut in_package = false;
    for raw_line in toml.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some(name) = line.strip_prefix("name") {
                let Some((_, value)) = name.split_once('=') else {
                    continue;
                };
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn package_name_from_pyproject(root: &Path) -> Option<String> {
    fs::read_to_string(root.join("pyproject.toml"))
        .ok()
        .and_then(|toml| {
            toml.lines().find_map(|line| {
                let line = line.trim();
                line.strip_prefix("name")
                    .and_then(|name| name.split_once('=').map(|(_, value)| value))
                    .map(str::trim)
                    .map(|value| value.trim_matches('"').to_string())
                    .filter(|value| !value.is_empty())
            })
        })
}

fn node_runner(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if root.join("yarn.lock").is_file() {
        "yarn"
    } else if root.join("bun.lockb").is_file() || root.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    }
}

fn node_markers(root: &Path) -> Vec<String> {
    let mut markers = vec!["package.json".to_string()];
    for marker in [
        "vite.config.js",
        "vite.config.ts",
        "vite.config.mjs",
        "vite.config.mts",
    ] {
        if root.join(marker).is_file() {
            markers.push(marker.to_string());
        }
    }
    markers
}

fn rust_preview_hooks(root: &Path) -> Vec<ProjectPreviewHook> {
    if root.join("Trunk.toml").is_file() || root.join("index.html").is_file() {
        vec![ProjectPreviewHook::new(
            "trunk-local",
            "Trunk 8080",
            Some("http://localhost:8080".to_string()),
            Some("serve".to_string()),
            "Открыть предпросмотр Rust/WASM-приложения через Trunk",
        )]
    } else {
        Vec::new()
    }
}

fn node_preview_hooks(
    _root: &Path,
    scripts: Option<&serde_json::Map<String, Value>>,
    has_vite: bool,
    _has_react: bool,
    has_next: bool,
    commands: &[ProjectCommand],
) -> Vec<ProjectPreviewHook> {
    let mut hooks = Vec::new();
    if has_vite || has_script(scripts, "dev") {
        let url = if has_next {
            "http://localhost:3000"
        } else {
            "http://localhost:5173"
        };
        hooks.push(ProjectPreviewHook::new(
            "dev-server",
            "URL разработки",
            Some(url.to_string()),
            command_id_if_present(commands, "dev"),
            "Открыть URL локального сервера разработки",
        ));
    }
    if has_script(scripts, "preview") {
        hooks.push(ProjectPreviewHook::new(
            "preview-server",
            "URL предпросмотра",
            Some("http://localhost:4173".to_string()),
            command_id_if_present(commands, "preview"),
            "Открыть URL локального предпросмотра production-сборки",
        ));
    }
    if has_script(scripts, "start") && !hooks.iter().any(|hook| hook.id == "dev-server") {
        hooks.push(ProjectPreviewHook::new(
            "start-server",
            "URL запуска",
            Some("http://localhost:3000".to_string()),
            command_id_if_present(commands, "start"),
            "Открыть стандартный URL локального сервера приложения",
        ));
    }
    hooks
}

fn has_script(scripts: Option<&serde_json::Map<String, Value>>, id: &str) -> bool {
    scripts
        .map(|scripts| scripts.contains_key(id))
        .unwrap_or(false)
}

fn command_id_if_present(commands: &[ProjectCommand], id: &str) -> Option<String> {
    commands
        .iter()
        .any(|command| command.id == id)
        .then(|| id.to_string())
}

fn has_package_dep(package: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|section| {
            package
                .get(section)
                .and_then(Value::as_object)
                .map(|deps| deps.contains_key(name))
                .unwrap_or(false)
        })
}

fn has_any_marker(root: &Path, markers: &[&str]) -> bool {
    markers.iter().any(|marker| root.join(marker).is_file())
}

fn first_file_with_extension(root: &Path, extension: &str) -> Option<std::path::PathBuf> {
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case(extension))
                .unwrap_or(false)
        })
}

fn push_unique_command(commands: &mut Vec<ProjectCommand>, command: ProjectCommand) {
    if !commands.iter().any(|known| known.id == command.id) {
        commands.push(command);
    }
}

fn script_label(id: &str) -> String {
    match id {
        "dev" => "Dev".to_string(),
        "start" => "Старт".to_string(),
        "build" => "Сборка".to_string(),
        "test" => "Тесты".to_string(),
        "lint" => "Линт".to_string(),
        "preview" => "Предпросмотр".to_string(),
        "format" => "Формат".to_string(),
        "typecheck" => "Типы".to_string(),
        _ => title_case(id),
    }
}

fn title_case(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Запуск".to_string(),
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '_'], "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_rust_profile_and_commands() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo-game\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let profiles = detect_project_profiles(&workspace);

        assert_eq!(profiles[0].kind, "Rust");
        assert_eq!(profiles[0].name, "demo-game");
        assert!(find_project_command(&profiles, "check", None).is_some());
    }

    #[test]
    fn detects_node_scripts_in_preferred_order() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("package.json"),
            r#"{"name":"demo-web","scripts":{"build":"vite build","dev":"vite","lint":"eslint .","custom":"node tool.js"}}"#,
        )
        .unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let profiles = detect_project_profiles(&workspace);
        let commands = profiles[0]
            .commands
            .iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(profiles[0].kind, "Node");
        assert_eq!(profiles[0].name, "demo-web");
        assert_eq!(commands[..3], ["dev", "build", "lint"]);
        assert_eq!(
            find_project_command(&profiles, "dev", Some("node"))
                .unwrap()
                .command,
            "npm run dev"
        );
    }

    #[test]
    fn detects_react_vite_profile() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("package.json"),
            r#"{"name":"demo-game-ui","scripts":{"dev":"vite"},"dependencies":{"react":"latest"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let profiles = detect_project_profiles(&workspace);

        assert_eq!(profiles[0].kind, "React/Vite");
        assert!(find_project_command(&profiles, "dev", Some("react/vite")).is_some());
        assert!(find_project_preview(&profiles, "dev-server", Some("react/vite")).is_some());
    }

    #[test]
    fn detects_trunk_preview_hook() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"wasm-game\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(temp.path().join("Trunk.toml"), "[build]\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let profiles = detect_project_profiles(&workspace);

        assert!(find_project_command(&profiles, "serve", Some("rust")).is_some());
        assert!(find_project_preview(&profiles, "trunk-local", Some("rust")).is_some());
    }
}
