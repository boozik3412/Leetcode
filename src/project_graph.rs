use crate::agent::types::ToolResult;
use crate::memory::load_memory;
use crate::project::detect_project_profiles;
use crate::roadmap::load_roadmap;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROJECT_GRAPH_PATH: &str = "assets/generated/leetcode/project_graph.json";
const MAX_GRAPH_FILE_BYTES: usize = 5_000_000;
const MAX_TEXT_SCAN_BYTES: usize = 220_000;
const MAX_FILE_ROWS: usize = 1_200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGraphState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub nodes: Vec<ProjectGraphNode>,
    #[serde(default)]
    pub edges: Vec<ProjectGraphEdge>,
    #[serde(default)]
    pub updated_at: u64,
}

impl Default for ProjectGraphState {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            title: "Project Graph".to_string(),
            focus: "Автоматически собранная карта проекта".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            updated_at: unix_timestamp(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGraphNode {
    pub id: String,
    pub label: String,
    pub kind: ProjectGraphNodeKind,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectGraphNodeKind {
    Project,
    Folder,
    File,
    Module,
    Symbol,
    Command,
    Asset,
    Memory,
    RoadmapItem,
}

impl ProjectGraphNodeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Folder => "folder",
            Self::File => "file",
            Self::Module => "module",
            Self::Symbol => "symbol",
            Self::Command => "command",
            Self::Asset => "asset",
            Self::Memory => "memory",
            Self::RoadmapItem => "roadmap_item",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectGraphEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: ProjectGraphEdgeKind,
    pub label: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectGraphEdgeKind {
    Contains,
    Imports,
    DependsOn,
    Calls,
    Generates,
    Tests,
    Documents,
    RelatedTo,
}

impl ProjectGraphEdgeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::DependsOn => "depends_on",
            Self::Calls => "calls",
            Self::Generates => "generates",
            Self::Tests => "tests",
            Self::Documents => "documents",
            Self::RelatedTo => "related_to",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Contains => "содержит",
            Self::Imports => "импортирует",
            Self::DependsOn => "зависит от",
            Self::Calls => "вызывает",
            Self::Generates => "генерирует",
            Self::Tests => "проверяет",
            Self::Documents => "документирует",
            Self::RelatedTo => "связано с",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectGraphSnapshotArgs {
    #[serde(default)]
    pub save_if_missing: bool,
    #[serde(default)]
    pub refresh: bool,
    #[serde(default = "default_include_edges")]
    pub include_edges: bool,
    #[serde(default)]
    pub max_nodes: Option<usize>,
}

pub fn load_project_graph(workspace: &Workspace) -> ProjectGraphState {
    workspace
        .read_text(PROJECT_GRAPH_PATH, MAX_GRAPH_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectGraphState>(&text).ok())
        .unwrap_or_else(|| scan_project_graph(workspace))
}

pub fn save_project_graph(workspace: &Workspace, graph: &ProjectGraphState) -> anyhow::Result<()> {
    workspace.write_text(PROJECT_GRAPH_PATH, &serde_json::to_string_pretty(graph)?)
}

pub fn project_graph_summary_for_prompt(workspace: Option<&Workspace>) -> String {
    let Some(workspace) = workspace else {
        return "Карта проекта: рабочая папка не выбрана.".to_string();
    };
    let Some(graph) = workspace
        .read_text(PROJECT_GRAPH_PATH, MAX_GRAPH_FILE_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectGraphState>(&text).ok())
    else {
        return "Карта проекта: снимок ещё не сохранён. Используй project_graph_snapshot с refresh=true, когда нужно построить архитектурную карту файлов, команд, памяти и roadmap.".to_string();
    };
    let mut counts = BTreeMap::<ProjectGraphNodeKind, usize>::new();
    for node in &graph.nodes {
        *counts.entry(node.kind).or_default() += 1;
    }
    let counts = counts
        .iter()
        .map(|(kind, count)| format!("{}: {}", kind.as_str(), count))
        .collect::<Vec<_>>()
        .join(", ");
    let commands = graph
        .nodes
        .iter()
        .filter(|node| node.kind == ProjectGraphNodeKind::Command)
        .take(6)
        .map(|node| node.label.clone())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Карта проекта: {} узлов, {} связей{}.\nИспользуй project_graph_snapshot, когда нужно понять архитектуру, зависимости, проектные команды, связь файлов с roadmap или памятью.",
        graph.nodes.len(),
        graph.edges.len(),
        if commands.is_empty() {
            format!("; типы: {}", empty_label(&counts))
        } else {
            format!("; команды: {commands}; типы: {}", empty_label(&counts))
        }
    )
}

pub fn project_graph_snapshot(workspace: &Workspace, args: ProjectGraphSnapshotArgs) -> ToolResult {
    let graph_exists = workspace.read_text(PROJECT_GRAPH_PATH, 1).is_ok();
    let graph = if args.refresh {
        let graph = scan_project_graph(workspace);
        if let Err(err) = save_project_graph(workspace, &graph) {
            return ToolResult::error(err.to_string());
        }
        graph
    } else {
        let graph = load_project_graph(workspace);
        if args.save_if_missing && !graph_exists {
            if let Err(err) = save_project_graph(workspace, &graph) {
                return ToolResult::error(err.to_string());
            }
        }
        graph
    };
    let graph = bounded_graph(graph, args.max_nodes, args.include_edges);
    ToolResult::ok(
        serde_json::to_string_pretty(&graph).unwrap_or_else(|_| "снимок карты проекта".to_string()),
    )
}

pub fn scan_project_graph(workspace: &Workspace) -> ProjectGraphState {
    let now = unix_timestamp();
    let mut builder = GraphBuilder::new(workspace.display_name(), now);
    let rows = workspace.ui_file_rows(MAX_FILE_ROWS);

    add_file_tree_nodes(&mut builder, workspace, &rows);
    add_project_profile_nodes(&mut builder, workspace);
    add_dependency_nodes(&mut builder, workspace);
    add_rust_module_nodes(&mut builder, workspace, &rows);
    add_memory_nodes(&mut builder, workspace);
    add_roadmap_nodes(&mut builder, workspace);

    builder.finish()
}

fn add_file_tree_nodes(builder: &mut GraphBuilder, workspace: &Workspace, rows: &[String]) {
    for row in rows {
        if row == "..." {
            continue;
        }
        let is_dir = row.ends_with('/');
        let rel = row.trim_end_matches('/');
        if rel.is_empty() {
            continue;
        }
        let kind = if is_dir {
            ProjectGraphNodeKind::Folder
        } else if is_asset_path(rel) {
            ProjectGraphNodeKind::Asset
        } else {
            ProjectGraphNodeKind::File
        };
        let mut metadata = BTreeMap::new();
        if !is_dir {
            if let Some(extension) = file_extension(rel) {
                metadata.insert("extension".to_string(), extension);
            }
        }
        builder.add_node(ProjectGraphNode {
            id: path_node_id(kind, rel),
            label: file_label(rel),
            kind,
            path: Some(rel.to_string()),
            summary: if is_dir {
                "Каталог проекта".to_string()
            } else if kind == ProjectGraphNodeKind::Asset {
                "Ассет или сгенерированный медиа-файл проекта".to_string()
            } else {
                "Файл проекта".to_string()
            },
            source: "scanner:file_tree".to_string(),
            confidence: 0.95,
            metadata,
            updated_at: builder.now,
        });
        let parent = parent_path(rel)
            .map(|path| folder_node_id(&path))
            .unwrap_or_else(|| "project:root".to_string());
        if parent != "project:root" && !builder.has_node(&parent) {
            let folder_path = parent.trim_start_matches("folder:").to_string();
            builder.add_node(ProjectGraphNode {
                id: parent.clone(),
                label: file_label(&folder_path),
                kind: ProjectGraphNodeKind::Folder,
                path: Some(folder_path),
                summary: "Каталог проекта".to_string(),
                source: "scanner:file_tree:inferred_parent".to_string(),
                confidence: 0.65,
                metadata: BTreeMap::new(),
                updated_at: builder.now,
            });
        }
        builder.add_edge(
            &parent,
            &path_node_id(kind, rel),
            ProjectGraphEdgeKind::Contains,
            0.95,
            "scanner:file_tree",
        );
        if kind == ProjectGraphNodeKind::Asset {
            builder.add_edge(
                "project:root",
                &path_node_id(kind, rel),
                ProjectGraphEdgeKind::Generates,
                0.55,
                "scanner:asset_path",
            );
        }
    }

    builder.add_node(ProjectGraphNode {
        id: "project:root".to_string(),
        label: workspace.display_name(),
        kind: ProjectGraphNodeKind::Project,
        path: Some(".".to_string()),
        summary: "Корень выбранного проекта".to_string(),
        source: "scanner:workspace".to_string(),
        confidence: 1.0,
        metadata: BTreeMap::new(),
        updated_at: builder.now,
    });
}

fn add_project_profile_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    for profile in detect_project_profiles(workspace) {
        let profile_id = stable_id("module:profile", &profile.kind);
        let mut metadata = BTreeMap::new();
        metadata.insert("kind".to_string(), profile.kind.clone());
        metadata.insert("markers".to_string(), profile.markers.join(", "));
        builder.add_node(ProjectGraphNode {
            id: profile_id.clone(),
            label: profile.kind.clone(),
            kind: ProjectGraphNodeKind::Module,
            path: None,
            summary: format!("Профиль проекта: {}", profile.name),
            source: "scanner:project_profile".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &profile_id,
            ProjectGraphEdgeKind::Contains,
            0.9,
            "scanner:project_profile",
        );
        for command in profile.commands {
            let command_id = stable_id("command", &format!("{}:{}", profile.kind, command.id));
            let mut metadata = BTreeMap::new();
            metadata.insert("command".to_string(), command.command.clone());
            metadata.insert("cwd".to_string(), command.cwd.clone());
            metadata.insert("timeout_secs".to_string(), command.timeout_secs.to_string());
            builder.add_node(ProjectGraphNode {
                id: command_id.clone(),
                label: command.label.clone(),
                kind: ProjectGraphNodeKind::Command,
                path: None,
                summary: command.description.clone(),
                source: "scanner:project_command".to_string(),
                confidence: 0.9,
                metadata,
                updated_at: builder.now,
            });
            builder.add_edge(
                &profile_id,
                &command_id,
                ProjectGraphEdgeKind::Contains,
                0.9,
                "scanner:project_command",
            );
            if is_test_command(&command.id, &command.command) {
                builder.add_edge(
                    &command_id,
                    "project:root",
                    ProjectGraphEdgeKind::Tests,
                    0.8,
                    "scanner:project_command",
                );
            }
        }
    }
}

fn add_dependency_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    if let Ok(cargo) = workspace.read_text("Cargo.toml", MAX_TEXT_SCAN_BYTES) {
        for dependency in cargo_dependencies(&cargo).into_iter().take(80) {
            let id = stable_id("module:dependency:cargo", &dependency);
            let mut metadata = BTreeMap::new();
            metadata.insert("manager".to_string(), "cargo".to_string());
            builder.add_node(ProjectGraphNode {
                id: id.clone(),
                label: dependency,
                kind: ProjectGraphNodeKind::Module,
                path: None,
                summary: "Cargo dependency".to_string(),
                source: "scanner:cargo_toml".to_string(),
                confidence: 0.75,
                metadata,
                updated_at: builder.now,
            });
            builder.add_edge(
                "file:Cargo.toml",
                &id,
                ProjectGraphEdgeKind::DependsOn,
                0.75,
                "scanner:cargo_toml",
            );
        }
    }

    if let Ok(package_json) = workspace.read_text("package.json", MAX_TEXT_SCAN_BYTES) {
        for dependency in npm_dependencies(&package_json).into_iter().take(80) {
            let id = stable_id("module:dependency:npm", &dependency);
            let mut metadata = BTreeMap::new();
            metadata.insert("manager".to_string(), "npm".to_string());
            builder.add_node(ProjectGraphNode {
                id: id.clone(),
                label: dependency,
                kind: ProjectGraphNodeKind::Module,
                path: None,
                summary: "npm dependency".to_string(),
                source: "scanner:package_json".to_string(),
                confidence: 0.8,
                metadata,
                updated_at: builder.now,
            });
            builder.add_edge(
                "file:package.json",
                &id,
                ProjectGraphEdgeKind::DependsOn,
                0.8,
                "scanner:package_json",
            );
        }
    }
}

fn add_rust_module_nodes(builder: &mut GraphBuilder, workspace: &Workspace, rows: &[String]) {
    for rel in rows
        .iter()
        .map(|row| row.trim_end_matches('/'))
        .filter(|rel| rel.ends_with(".rs"))
        .take(220)
    {
        let module = rust_module_name(rel);
        let module_id = stable_id("module:rust", &module);
        let mut metadata = BTreeMap::new();
        metadata.insert("language".to_string(), "rust".to_string());
        builder.add_node(ProjectGraphNode {
            id: module_id.clone(),
            label: module.clone(),
            kind: ProjectGraphNodeKind::Module,
            path: Some(rel.to_string()),
            summary: "Rust module inferred from file path".to_string(),
            source: "scanner:rust_path".to_string(),
            confidence: 0.75,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            &path_node_id(ProjectGraphNodeKind::File, rel),
            &module_id,
            ProjectGraphEdgeKind::Documents,
            0.75,
            "scanner:rust_path",
        );

        if let Ok(text) = workspace.read_text(rel, MAX_TEXT_SCAN_BYTES) {
            for imported in rust_imports(&text).into_iter().take(60) {
                let imported_id = stable_id("module:rust", &imported);
                if !builder.has_node(&imported_id) {
                    builder.add_node(ProjectGraphNode {
                        id: imported_id.clone(),
                        label: imported.clone(),
                        kind: ProjectGraphNodeKind::Module,
                        path: None,
                        summary: "Rust module inferred from import".to_string(),
                        source: "scanner:rust_imports".to_string(),
                        confidence: 0.45,
                        metadata: BTreeMap::new(),
                        updated_at: builder.now,
                    });
                }
                builder.add_edge(
                    &module_id,
                    &imported_id,
                    ProjectGraphEdgeKind::Imports,
                    0.55,
                    "scanner:rust_imports",
                );
            }
        }
    }
}

fn add_memory_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let memory = load_memory(workspace);
    for goal in memory.goals {
        let id = stable_id("memory:goal", &goal.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "goal".to_string());
        metadata.insert("status".to_string(), goal.status.clone());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: goal.title,
            kind: ProjectGraphNodeKind::Memory,
            path: None,
            summary: goal.notes,
            source: "scanner:memory".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:memory",
        );
    }
    for task in memory.tasks {
        let id = stable_id("memory:task", &task.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "task".to_string());
        metadata.insert("status".to_string(), task.status.clone());
        metadata.insert("workstream".to_string(), task.workstream.clone());
        metadata.insert("milestone".to_string(), task.milestone.clone());
        metadata.insert("priority".to_string(), task.priority.clone());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: task.title,
            kind: ProjectGraphNodeKind::Memory,
            path: None,
            summary: task.notes,
            source: "scanner:memory".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:memory",
        );
    }
    for decision in memory.decisions {
        let id = stable_id("memory:decision", &decision.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "decision".to_string());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: decision.title,
            kind: ProjectGraphNodeKind::Memory,
            path: None,
            summary: decision.rationale,
            source: "scanner:memory".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:memory",
        );
    }
    for source in memory.sources {
        let id = stable_id("memory:source", &source.id);
        let path = source
            .original_path
            .clone()
            .or_else(|| source.stored_path.clone());
        let mut metadata = BTreeMap::new();
        metadata.insert("type".to_string(), "source".to_string());
        metadata.insert("kind".to_string(), source.kind.clone());
        metadata.insert(
            "content_chars".to_string(),
            source.content_chars.to_string(),
        );
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: source.title,
            kind: ProjectGraphNodeKind::Memory,
            path: path.clone(),
            summary: if source.summary.trim().is_empty() {
                compact_inline(&source.content, 240)
            } else {
                source.summary
            },
            source: "scanner:memory_source".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::Documents,
            0.85,
            "scanner:memory_source",
        );
        if let Some(path) = path.filter(|value| !value.trim().is_empty()) {
            let file_id = path_node_id(ProjectGraphNodeKind::File, &path);
            builder.add_edge(
                &id,
                &file_id,
                ProjectGraphEdgeKind::Documents,
                0.6,
                "scanner:memory_source",
            );
        }
    }
}

fn add_roadmap_nodes(builder: &mut GraphBuilder, workspace: &Workspace) {
    let roadmap = load_roadmap(workspace);
    for item in roadmap.items {
        let id = stable_id("roadmap", &item.id);
        let mut metadata = BTreeMap::new();
        metadata.insert("status".to_string(), item.status.as_str().to_string());
        metadata.insert("kind".to_string(), item.kind.clone());
        metadata.insert("date_label".to_string(), item.date_label.clone());
        builder.add_node(ProjectGraphNode {
            id: id.clone(),
            label: item.title,
            kind: ProjectGraphNodeKind::RoadmapItem,
            path: None,
            summary: item.detail,
            source: "scanner:roadmap".to_string(),
            confidence: 0.9,
            metadata,
            updated_at: builder.now,
        });
        builder.add_edge(
            "project:root",
            &id,
            ProjectGraphEdgeKind::RelatedTo,
            0.9,
            "scanner:roadmap",
        );
        for file in item.links.files {
            let file_id = path_node_id(ProjectGraphNodeKind::File, &file);
            builder.add_edge(
                &id,
                &file_id,
                ProjectGraphEdgeKind::RelatedTo,
                0.65,
                "scanner:roadmap_links",
            );
        }
        for memory_id in item.links.memory_ids {
            let memory_node_id = stable_id("memory", &memory_id);
            builder.add_edge(
                &id,
                &memory_node_id,
                ProjectGraphEdgeKind::Documents,
                0.55,
                "scanner:roadmap_links",
            );
        }
    }
}

struct GraphBuilder {
    title: String,
    now: u64,
    nodes: BTreeMap<String, ProjectGraphNode>,
    edges: BTreeMap<String, ProjectGraphEdge>,
}

impl GraphBuilder {
    fn new(title: String, now: u64) -> Self {
        let mut builder = Self {
            title,
            now,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        };
        builder.add_node(ProjectGraphNode {
            id: "project:root".to_string(),
            label: builder.title.clone(),
            kind: ProjectGraphNodeKind::Project,
            path: Some(".".to_string()),
            summary: "Корень выбранного проекта".to_string(),
            source: "scanner:workspace".to_string(),
            confidence: 1.0,
            metadata: BTreeMap::new(),
            updated_at: now,
        });
        builder
    }

    fn has_node(&self, id: &str) -> bool {
        self.nodes.contains_key(id)
    }

    fn add_node(&mut self, node: ProjectGraphNode) {
        if let Some(existing) = self.nodes.get_mut(&node.id) {
            if node.confidence >= existing.confidence {
                *existing = node;
            }
        } else {
            self.nodes.insert(node.id.clone(), node);
        }
    }

    fn add_edge(
        &mut self,
        from: &str,
        to: &str,
        kind: ProjectGraphEdgeKind,
        confidence: f32,
        source: &str,
    ) {
        if from == to {
            return;
        }
        let id = format!("edge:{}:{}->{}", kind.as_str(), from, to);
        self.edges.entry(id.clone()).or_insert(ProjectGraphEdge {
            id,
            from: from.to_string(),
            to: to.to_string(),
            kind,
            label: kind.label().to_string(),
            source: source.to_string(),
            confidence,
            updated_at: self.now,
        });
    }

    fn finish(self) -> ProjectGraphState {
        ProjectGraphState {
            schema_version: schema_version(),
            title: self.title,
            focus: "Файлы, команды, память и roadmap проекта".to_string(),
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_values().collect(),
            updated_at: self.now,
        }
    }
}

fn bounded_graph(
    mut graph: ProjectGraphState,
    max_nodes: Option<usize>,
    include_edges: bool,
) -> serde_json::Value {
    if let Some(max_nodes) = max_nodes {
        if graph.nodes.len() > max_nodes {
            graph.nodes.truncate(max_nodes);
        }
        let allowed = graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        graph
            .edges
            .retain(|edge| allowed.contains(&edge.from) && allowed.contains(&edge.to));
    }
    if !include_edges {
        graph.edges.clear();
    }
    json!({
        "path": PROJECT_GRAPH_PATH,
        "node_count": graph.nodes.len(),
        "edge_count": graph.edges.len(),
        "graph": graph,
    })
}

fn default_include_edges() -> bool {
    true
}

fn schema_version() -> u32 {
    1
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn path_node_id(kind: ProjectGraphNodeKind, rel: &str) -> String {
    match kind {
        ProjectGraphNodeKind::Folder => folder_node_id(rel),
        _ => format!("file:{}", normalize_rel(rel)),
    }
}

fn folder_node_id(rel: &str) -> String {
    format!("folder:{}", normalize_rel(rel))
}

fn stable_id(prefix: &str, value: &str) -> String {
    let cleaned = value
        .trim()
        .replace('\\', "/")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | ':' | '_' | '-' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{prefix}:{cleaned}")
}

fn normalize_rel(rel: &str) -> String {
    rel.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn parent_path(rel: &str) -> Option<String> {
    let rel = normalize_rel(rel);
    rel.rsplit_once('/').map(|(parent, _)| parent.to_string())
}

fn file_label(rel: &str) -> String {
    normalize_rel(rel)
        .rsplit('/')
        .next()
        .unwrap_or(rel)
        .to_string()
}

fn file_extension(rel: &str) -> Option<String> {
    file_label(rel)
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .filter(|extension| !extension.trim().is_empty())
}

fn is_asset_path(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.starts_with("assets/")
        || lower.starts_with("public/")
        || matches!(
            file_extension(&lower).as_deref(),
            Some("png")
                | Some("jpg")
                | Some("jpeg")
                | Some("webp")
                | Some("gif")
                | Some("svg")
                | Some("wav")
                | Some("mp3")
                | Some("ogg")
                | Some("opus")
                | Some("mp4")
                | Some("webm")
        )
}

fn is_test_command(id: &str, command: &str) -> bool {
    let text = format!("{id} {command}").to_ascii_lowercase();
    ["test", "check", "lint", "fmt", "clippy"]
        .iter()
        .any(|needle| text.contains(needle))
}

fn cargo_dependencies(text: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_dependency_section = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_matches(&['[', ']'][..]).trim();
            in_dependency_section = matches!(
                section,
                "dependencies"
                    | "dev-dependencies"
                    | "build-dependencies"
                    | "target.'cfg(windows)'.dependencies"
            );
            continue;
        }
        if !in_dependency_section || line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim().trim_matches('"');
            if !name.is_empty() {
                deps.push(name.to_string());
            }
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn npm_dependencies(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for key in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(object) = value.get(key).and_then(serde_json::Value::as_object) {
            deps.extend(object.keys().cloned());
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn rust_module_name(rel: &str) -> String {
    let rel = normalize_rel(rel);
    let rel = rel.trim_start_matches("src/");
    let rel = rel.trim_end_matches(".rs");
    match rel {
        "main" | "lib" => "crate".to_string(),
        value if value.ends_with("/mod") => value.trim_end_matches("/mod").replace('/', "::"),
        value => value.replace('/', "::"),
    }
}

fn rust_imports(text: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("use crate::") {
            let name = rest
                .trim_end_matches(';')
                .split(&[':', '{', ' ', ';'][..])
                .filter(|part| !part.is_empty())
                .take(3)
                .collect::<Vec<_>>()
                .join("::");
            if !name.is_empty() {
                imports.push(name);
            }
        } else if let Some(rest) = line.strip_prefix("mod ") {
            let name = rest.trim_end_matches(';').trim();
            if !name.is_empty() {
                imports.push(name.to_string());
            }
        }
    }
    imports.sort();
    imports.dedup();
    imports
}

fn compact_inline(value: &str, max_chars: usize) -> String {
    let one_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max_chars {
        return one_line;
    }
    let mut compacted = one_line
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    compacted.push('…');
    compacted
}

fn empty_label(value: &str) -> String {
    if value.trim().is_empty() {
        "нет".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_files_profiles_dependencies_and_modules() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src/agent")).unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("src/main.rs"),
            "mod agent;\nuse crate::agent::types;\n",
        )
        .unwrap();
        fs::write(temp.path().join("src/agent/types.rs"), "pub struct Demo;\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let graph = scan_project_graph(&workspace);

        assert!(graph.nodes.iter().any(|node| node.id == "project:root"));
        assert!(graph.nodes.iter().any(|node| node.id == "file:Cargo.toml"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "module:dependency:cargo:serde"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.kind == ProjectGraphNodeKind::Command && node.label == "Проверка"));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::Contains));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == ProjectGraphEdgeKind::Imports));
    }

    #[test]
    fn snapshot_can_save_graph_file() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        let result = project_graph_snapshot(
            &workspace,
            ProjectGraphSnapshotArgs {
                save_if_missing: true,
                refresh: false,
                include_edges: false,
                max_nodes: Some(4),
            },
        );

        assert!(result.ok);
        assert!(workspace.read_text(PROJECT_GRAPH_PATH, 1_000_000).is_ok());
        assert!(result.output.contains("\"edge_count\": 0"));
    }
}
