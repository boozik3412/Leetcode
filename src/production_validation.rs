use crate::agent::models::provider_specs;
use crate::agent::types::ToolResult;
use crate::config::AppConfig;
use crate::mcp::{ensure_registry, registry_snapshot};
use crate::project_graph::{load_project_graph, ProjectGraphState};
use crate::provider_health::load_provider_validation_history;
use crate::unreal::unreal_snapshot;
use crate::updater::{validate_manifest, UpdateManifest};
use crate::visual_regression::load_visual_state;
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PRODUCTION_REPORT_PATH: &str =
    "assets/generated/leetcode/production-validation/latest.json";
pub const PROJECT_MAP_GOLDEN_PATH: &str =
    "assets/generated/leetcode/production-validation/project-map.golden.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionCheckStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductionCheck {
    pub id: String,
    pub category: String,
    pub label: String,
    pub status: ProductionCheckStatus,
    pub detail: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductionValidationReport {
    pub schema_version: u32,
    pub created_at: u64,
    pub app_version: String,
    pub ready: bool,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub skipped: usize,
    pub checks: Vec<ProductionCheck>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectMapGolden {
    pub schema_version: u32,
    pub graph_schema_version: u32,
    pub node_count: usize,
    pub edge_count: usize,
    pub node_kinds: BTreeMap<String, usize>,
    pub edge_kinds: BTreeMap<String, usize>,
    pub stable_sha256: String,
    pub created_at: u64,
}

pub fn production_validation_snapshot(
    workspace: &Workspace,
    config: &AppConfig,
) -> ProductionValidationReport {
    let mut checks = Vec::new();
    checks.extend(source_release_checks(workspace.root()));
    checks.push(update_manifest_check(
        workspace.root(),
        "latest.json",
        "host-updater",
        "Leetcode",
    ));
    checks.push(update_manifest_check(
        workspace.root(),
        "client-latest.json",
        "client-updater",
        "Leetcode Client",
    ));
    checks.push(provider_contract_check(workspace, config));
    checks.push(mcp_contract_check(workspace));
    checks.push(unreal_contract_check(workspace));
    checks.push(project_map_golden_check(workspace));
    checks.push(visual_regression_check(workspace));
    checks.push(release_artifact_check(workspace.root()));
    report_from_checks(checks)
}

pub fn production_validation_tool(workspace: &Workspace, config: &AppConfig) -> ToolResult {
    let report = production_validation_snapshot(workspace, config);
    let _ = save_production_report(workspace, &report);
    ToolResult::ok(
        serde_json::to_string_pretty(&report)
            .unwrap_or_else(|_| "production validation report".to_string()),
    )
}

pub fn save_production_report(
    workspace: &Workspace,
    report: &ProductionValidationReport,
) -> anyhow::Result<()> {
    workspace.write_text(
        PRODUCTION_REPORT_PATH,
        &serde_json::to_string_pretty(report)?,
    )
}

pub fn load_production_report(workspace: &Workspace) -> Option<ProductionValidationReport> {
    workspace
        .read_text(PRODUCTION_REPORT_PATH, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

pub fn update_project_map_golden(workspace: &Workspace) -> anyhow::Result<ProjectMapGolden> {
    let golden = project_map_golden(&load_project_graph(workspace));
    workspace.write_text(
        PROJECT_MAP_GOLDEN_PATH,
        &serde_json::to_string_pretty(&golden)?,
    )?;
    Ok(golden)
}

pub fn load_project_map_golden(workspace: &Workspace) -> Option<ProjectMapGolden> {
    workspace
        .read_text(PROJECT_MAP_GOLDEN_PATH, 1_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn report_from_checks(checks: Vec<ProductionCheck>) -> ProductionValidationReport {
    let passed = checks
        .iter()
        .filter(|check| check.status == ProductionCheckStatus::Passed)
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.status == ProductionCheckStatus::Warning)
        .count();
    let failed = checks
        .iter()
        .filter(|check| check.status == ProductionCheckStatus::Failed)
        .count();
    let skipped = checks
        .iter()
        .filter(|check| check.status == ProductionCheckStatus::Skipped)
        .count();
    ProductionValidationReport {
        schema_version: 1,
        created_at: unix_timestamp(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ready: failed == 0 && warnings == 0,
        passed,
        warnings,
        failed,
        skipped,
        checks,
    }
}

fn source_release_checks(root: &Path) -> Vec<ProductionCheck> {
    let required = [
        ("host-package", "scripts/package-windows.ps1"),
        ("host-installer", "scripts/install-leetcode.ps1"),
        ("host-uninstaller", "scripts/uninstall-leetcode.ps1"),
        ("client-package", "scripts/package-client-windows.ps1"),
        ("client-installer", "scripts/install-leetcode-client.ps1"),
        (
            "client-uninstaller",
            "scripts/uninstall-leetcode-client.ps1",
        ),
        ("production-preflight", "scripts/production-preflight.ps1"),
        (
            "unreal-production",
            "scripts/validate-unreal-production.ps1",
        ),
    ];
    required
        .into_iter()
        .map(|(id, relative)| {
            let exists = root.join(relative).is_file();
            ProductionCheck {
                id: id.to_string(),
                category: "release-source".to_string(),
                label: relative.to_string(),
                status: if exists {
                    ProductionCheckStatus::Passed
                } else {
                    ProductionCheckStatus::Failed
                },
                detail: if exists {
                    "файл присутствует".to_string()
                } else {
                    "обязательный файл не найден".to_string()
                },
                artifacts: exists.then(|| relative.to_string()).into_iter().collect(),
            }
        })
        .collect()
}

fn update_manifest_check(
    root: &Path,
    file_name: &str,
    id: &str,
    expected_app: &str,
) -> ProductionCheck {
    let path = root.join("dist").join(file_name);
    if !path.is_file() {
        return ProductionCheck {
            id: id.to_string(),
            category: "updater".to_string(),
            label: file_name.to_string(),
            status: ProductionCheckStatus::Warning,
            detail: "manifest появится после release packaging".to_string(),
            artifacts: Vec::new(),
        };
    }
    let result = fs::read_to_string(&path)
        .map_err(anyhow::Error::from)
        .and_then(|text| serde_json::from_str::<UpdateManifest>(&text).map_err(Into::into))
        .and_then(|manifest| {
            if expected_app == "Leetcode" {
                validate_manifest(&manifest)?;
            } else {
                validate_companion_manifest(&manifest, expected_app)?;
            }
            validate_local_manifest_package(&path, &manifest)
        });
    ProductionCheck {
        id: id.to_string(),
        category: "updater".to_string(),
        label: file_name.to_string(),
        status: if result.is_ok() {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Failed
        },
        detail: result
            .map(|_| "manifest и SHA256 локального пакета корректны".to_string())
            .unwrap_or_else(|error| error.to_string()),
        artifacts: vec![relative_path(root, &path)],
    }
}

fn validate_companion_manifest(
    manifest: &UpdateManifest,
    expected_app: &str,
) -> anyhow::Result<()> {
    if manifest.app.trim() != expected_app {
        anyhow::bail!("manifest относится не к {expected_app}: {}", manifest.app);
    }
    if manifest.platform.trim() != "windows-x64" {
        anyhow::bail!("неподдерживаемая платформа: {}", manifest.platform);
    }
    if manifest.version.trim().is_empty() || manifest.package.trim().is_empty() {
        anyhow::bail!("manifest не содержит version/package");
    }
    let hash = manifest.sha256.trim();
    if hash.len() != 64 || !hash.chars().all(|character| character.is_ascii_hexdigit()) {
        anyhow::bail!("manifest содержит некорректный SHA256");
    }
    Ok(())
}

fn validate_local_manifest_package(path: &Path, manifest: &UpdateManifest) -> anyhow::Result<()> {
    if manifest.package.starts_with("http://") || manifest.package.starts_with("https://") {
        return Ok(());
    }
    let package = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&manifest.package);
    let bytes = fs::read(&package)?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(manifest.sha256.trim()) {
        anyhow::bail!(
            "SHA256 пакета {} не совпадает с manifest",
            package.display()
        );
    }
    if let Some(expected) = manifest.size_bytes {
        if expected != bytes.len() as u64 {
            anyhow::bail!(
                "размер пакета {} не совпадает с manifest",
                package.display()
            );
        }
    }
    Ok(())
}

fn provider_contract_check(workspace: &Workspace, config: &AppConfig) -> ProductionCheck {
    let configured = provider_specs()
        .iter()
        .filter(|provider| provider.implemented)
        .filter(|provider| !config.api_key_for_provider(provider.id).trim().is_empty())
        .map(|provider| provider.id)
        .collect::<Vec<_>>();
    if configured.is_empty() {
        return skipped_check(
            "provider-contracts",
            "providers",
            "Live-контракты провайдеров",
            "нет настроенных chat-провайдеров",
        );
    }
    let history = load_provider_validation_history(workspace);
    let missing = configured
        .iter()
        .filter(|provider_id| {
            !history
                .runs
                .iter()
                .rev()
                .any(|run| run.provider_id.as_str() == **provider_id && run.ok)
        })
        .copied()
        .collect::<Vec<_>>();
    ProductionCheck {
        id: "provider-contracts".to_string(),
        category: "providers".to_string(),
        label: "Live-контракты провайдеров".to_string(),
        status: if missing.is_empty() {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Warning
        },
        detail: if missing.is_empty() {
            format!("проверено провайдеров: {}", configured.len())
        } else {
            format!("нет успешной live-проверки: {}", missing.join(", "))
        },
        artifacts: vec!["assets/generated/leetcode/provider_validation_results.json".to_string()],
    }
}

fn mcp_contract_check(workspace: &Workspace) -> ProductionCheck {
    let registry = match ensure_registry(workspace) {
        Ok(registry) => registry,
        Err(error) => {
            return failed_check(
                "mcp-contracts",
                "mcp",
                "Live-контракты MCP",
                error.to_string(),
            )
        }
    };
    let enabled = registry
        .servers
        .iter()
        .filter(|server| server.enabled)
        .count();
    if enabled == 0 {
        return skipped_check(
            "mcp-contracts",
            "mcp",
            "Live-контракты MCP",
            "нет включённых MCP-серверов",
        );
    }
    let snapshot = registry_snapshot(workspace);
    let connected = snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .servers
                .iter()
                .filter(|server| server.config.enabled && server.status.connected)
                .count()
        })
        .unwrap_or_default();
    ProductionCheck {
        id: "mcp-contracts".to_string(),
        category: "mcp".to_string(),
        label: "Live-контракты MCP".to_string(),
        status: if connected == enabled {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Warning
        },
        detail: format!("подключено {connected} из {enabled}; полный контракт запускается opt-in"),
        artifacts: vec!["assets/generated/leetcode/mcp/servers.json".to_string()],
    }
}

fn unreal_contract_check(workspace: &Workspace) -> ProductionCheck {
    let snapshot = unreal_snapshot(workspace);
    if snapshot.project.is_none() && snapshot.local_plugins.is_empty() {
        return skipped_check(
            "unreal-production",
            "unreal",
            "Headless Unreal",
            "workspace не является Unreal-проектом",
        );
    }
    let engine_ok = snapshot.selected_engine.as_ref().is_some_and(|engine| {
        engine
            .version
            .as_deref()
            .is_some_and(|version| version.starts_with("5.8"))
            && engine.tools.editor_cmd.is_some()
            && engine.tools.uat.is_some()
    });
    ProductionCheck {
        id: "unreal-production".to_string(),
        category: "unreal".to_string(),
        label: "Headless Unreal".to_string(),
        status: if engine_ok {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Failed
        },
        detail: if engine_ok {
            "UE 5.8, UnrealEditor-Cmd и UAT доступны; live smoke запускается opt-in".to_string()
        } else {
            snapshot.diagnostics.join("; ")
        },
        artifacts: vec!["assets/generated/leetcode/unreal/production-validation.json".to_string()],
    }
}

fn project_map_golden_check(workspace: &Workspace) -> ProductionCheck {
    let Some(expected) = load_project_map_golden(workspace) else {
        return ProductionCheck {
            id: "project-map-golden".to_string(),
            category: "project-map".to_string(),
            label: "Golden Project Map".to_string(),
            status: ProductionCheckStatus::Warning,
            detail: "эталон ещё не зафиксирован".to_string(),
            artifacts: Vec::new(),
        };
    };
    let actual = project_map_golden(&load_project_graph(workspace));
    let matches = expected.stable_sha256 == actual.stable_sha256;
    ProductionCheck {
        id: "project-map-golden".to_string(),
        category: "project-map".to_string(),
        label: "Golden Project Map".to_string(),
        status: if matches {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Warning
        },
        detail: if matches {
            format!("{} узлов, {} связей", actual.node_count, actual.edge_count)
        } else {
            format!(
                "карта изменилась: было {}/{}, стало {}/{}; обновите эталон после ревью",
                expected.node_count, expected.edge_count, actual.node_count, actual.edge_count
            )
        },
        artifacts: vec![PROJECT_MAP_GOLDEN_PATH.to_string()],
    }
}

fn visual_regression_check(workspace: &Workspace) -> ProductionCheck {
    let state = load_visual_state(workspace);
    if state.baselines.is_empty() {
        return ProductionCheck {
            id: "visual-regression".to_string(),
            category: "ui".to_string(),
            label: "Screenshot regression".to_string(),
            status: ProductionCheckStatus::Warning,
            detail: "нет визуальных эталонов desktop/remote UI".to_string(),
            artifacts: Vec::new(),
        };
    }
    let failing = state
        .baselines
        .iter()
        .filter(|baseline| {
            !state
                .comparisons
                .iter()
                .rev()
                .find(|comparison| comparison.scenario == baseline.scenario)
                .is_some_and(|comparison| comparison.passed)
        })
        .count();
    ProductionCheck {
        id: "visual-regression".to_string(),
        category: "ui".to_string(),
        label: "Screenshot regression".to_string(),
        status: if failing == 0 {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Warning
        },
        detail: format!(
            "эталонов: {}, без успешного последнего сравнения: {failing}",
            state.baselines.len()
        ),
        artifacts: vec!["assets/generated/leetcode/visual-regression/state.json".to_string()],
    }
}

fn release_artifact_check(root: &Path) -> ProductionCheck {
    let candidates = [
        root.join("dist/leetcode-portable.zip"),
        root.join("dist/leetcode-client-portable.zip"),
        root.join("target/release/leetcode.exe"),
        root.join("target/release/leetcode-client.exe"),
        root.join("target/release/leetcode-relay.exe"),
    ];
    let artifacts = candidates
        .iter()
        .filter(|path| path.is_file())
        .map(|path| relative_path(root, path))
        .collect::<Vec<_>>();
    ProductionCheck {
        id: "release-artifacts".to_string(),
        category: "release".to_string(),
        label: "Release artifacts".to_string(),
        status: if artifacts.len() >= 5 {
            ProductionCheckStatus::Passed
        } else {
            ProductionCheckStatus::Warning
        },
        detail: format!("найдено обязательных артефактов: {}/5", artifacts.len()),
        artifacts,
    }
}

fn project_map_golden(graph: &ProjectGraphState) -> ProjectMapGolden {
    let mut node_kinds = BTreeMap::new();
    let mut edge_kinds = BTreeMap::new();
    let mut stable_rows = Vec::new();
    for node in &graph.nodes {
        let kind = enum_key(&node.kind);
        *node_kinds.entry(kind.clone()).or_insert(0) += 1;
        stable_rows.push(format!("node|{}|{kind}", node.id));
    }
    for edge in &graph.edges {
        let kind = enum_key(&edge.kind);
        *edge_kinds.entry(kind.clone()).or_insert(0) += 1;
        stable_rows.push(format!("edge|{}|{}|{}|{kind}", edge.id, edge.from, edge.to));
    }
    stable_rows.sort();
    ProjectMapGolden {
        schema_version: 1,
        graph_schema_version: graph.schema_version,
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
        node_kinds,
        edge_kinds,
        stable_sha256: format!("{:x}", Sha256::digest(stable_rows.join("\n").as_bytes())),
        created_at: unix_timestamp(),
    }
}

fn enum_key<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn failed_check(id: &str, category: &str, label: &str, detail: String) -> ProductionCheck {
    ProductionCheck {
        id: id.to_string(),
        category: category.to_string(),
        label: label.to_string(),
        status: ProductionCheckStatus::Failed,
        detail,
        artifacts: Vec::new(),
    }
}

fn skipped_check(id: &str, category: &str, label: &str, detail: &str) -> ProductionCheck {
    ProductionCheck {
        id: id.to_string(),
        category: category.to_string(),
        label: label.to_string(),
        status: ProductionCheckStatus::Skipped,
        detail: detail.to_string(),
        artifacts: Vec::new(),
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
    use crate::project_graph::{
        ProjectGraphEdge, ProjectGraphEdgeKind, ProjectGraphNode, ProjectGraphNodeKind,
    };

    #[test]
    fn golden_project_map_detects_structural_changes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let graph = ProjectGraphState {
            nodes: vec![ProjectGraphNode {
                id: "project:demo".to_string(),
                label: "Demo".to_string(),
                kind: ProjectGraphNodeKind::Project,
                path: None,
                summary: String::new(),
                source: "test".to_string(),
                confidence: 1.0,
                metadata: BTreeMap::new(),
                updated_at: 1,
            }],
            edges: Vec::new(),
            ..ProjectGraphState::default()
        };
        crate::project_graph::save_project_graph(&workspace, &graph).expect("graph");
        let first = update_project_map_golden(&workspace).expect("golden");
        assert_eq!(first.node_count, 1);
        assert_eq!(
            project_map_golden_check(&workspace).status,
            ProductionCheckStatus::Passed
        );

        let mut changed = graph;
        changed.edges.push(ProjectGraphEdge {
            id: "edge:test".to_string(),
            from: "project:demo".to_string(),
            to: "project:demo".to_string(),
            kind: ProjectGraphEdgeKind::RelatedTo,
            label: "related".to_string(),
            source: "test".to_string(),
            confidence: 1.0,
            updated_at: 2,
        });
        crate::project_graph::save_project_graph(&workspace, &changed).expect("changed graph");
        assert_eq!(
            project_map_golden_check(&workspace).status,
            ProductionCheckStatus::Warning
        );
    }

    #[test]
    fn production_report_fails_on_failed_check_and_warns_strictly() {
        let report = report_from_checks(vec![
            ProductionCheck {
                id: "ok".to_string(),
                category: "test".to_string(),
                label: "ok".to_string(),
                status: ProductionCheckStatus::Passed,
                detail: "ok".to_string(),
                artifacts: Vec::new(),
            },
            ProductionCheck {
                id: "warn".to_string(),
                category: "test".to_string(),
                label: "warn".to_string(),
                status: ProductionCheckStatus::Warning,
                detail: "review".to_string(),
                artifacts: Vec::new(),
            },
        ]);
        assert!(!report.ready);
        assert_eq!(report.passed, 1);
        assert_eq!(report.warnings, 1);
    }
}
