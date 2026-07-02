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
                ProjectCommand::new("run", "Run", "godot --path .", "Run the Godot project"),
                ProjectCommand::new(
                    "editor",
                    "Editor",
                    "godot --path . --editor",
                    "Open the Godot editor",
                ),
            ],
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
                "Editor",
                format!("UnrealEditor \"{uproject_name}\""),
                "Open the Unreal project in the editor",
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

pub fn describe_project_commands(profiles: &[ProjectProfile]) -> String {
    if profiles.is_empty() {
        return "No project profiles detected".to_string();
    }

    let mut lines = Vec::new();
    for profile in profiles {
        if profile.commands.is_empty() {
            lines.push(format!(
                "{} {}: no quick commands registered",
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
        .unwrap_or_else(|| "Rust project".to_string());

    ProjectProfile {
        kind: "Rust".to_string(),
        name,
        markers: vec!["Cargo.toml".to_string()],
        commands: vec![
            ProjectCommand::new("check", "Check", "cargo check", "Check Rust compilation"),
            ProjectCommand::new("test", "Test", "cargo test", "Run Rust tests"),
            ProjectCommand::new("run", "Run", "cargo run", "Run the Rust app"),
            ProjectCommand::new("build", "Build", "cargo build", "Build debug binary"),
            ProjectCommand::new(
                "release",
                "Release",
                "cargo build --release",
                "Build release binary",
            ),
        ],
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
        .unwrap_or("Node project")
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
                        title_case(id),
                        format!("{runner} run {id}"),
                        format!("Run package.json script '{id}'"),
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
                        title_case(id),
                        format!("{runner} run {id}"),
                        format!("Run package.json script '{id}'"),
                    ),
                );
            }
        }
    }

    ProjectProfile {
        kind: kind.to_string(),
        name,
        markers: node_markers(root),
        commands,
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
        "Test",
        "python -m pytest",
        "Run Python tests with pytest",
    )];
    if root.join("main.py").is_file() {
        commands.push(ProjectCommand::new(
            "run",
            "Run",
            "python main.py",
            "Run main.py",
        ));
    }
    if root.join("requirements.txt").is_file() {
        commands.push(ProjectCommand::new(
            "install",
            "Install",
            "python -m pip install -r requirements.txt",
            "Install Python dependencies",
        ));
    }

    ProjectProfile {
        kind: "Python".to_string(),
        name: package_name_from_pyproject(root).unwrap_or_else(|| "Python project".to_string()),
        markers,
        commands,
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

fn title_case(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Run".to_string(),
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
    }
}
