mod catalog;
mod inference;
mod ranking;
mod store;
mod types;

pub use catalog::{semantic_catalog, semantic_catalog_map};
pub use inference::analyze_project_semantics;
pub use ranking::rank_semantic_candidates;
pub use store::{
    append_semantic_audit, export_semantic_index, load_semantic_index, save_semantic_index,
    semantic_data_dir, unix_timestamp, upsert_assignment,
};
pub use types::*;

use crate::agent::types::ToolResult;
use crate::game_task_builder::find_operation;
use crate::project_graph::{load_project_graph, project_graph_fingerprint, ProjectGraphState};
use crate::workspace::Workspace;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn ensure_semantic_index(
    workspace: &Workspace,
    graph: &ProjectGraphState,
) -> anyhow::Result<SemanticIndexState> {
    let state = load_semantic_index(workspace);
    if state.graph_fingerprint == project_graph_fingerprint(graph) && !state.nodes.is_empty() {
        return Ok(state);
    }
    analyze_project_semantics(workspace, graph, state, false).map(|(state, _)| state)
}

pub fn semantic_catalog_snapshot(args: SemanticCatalogSnapshotArgs) -> ToolResult {
    let definitions = semantic_catalog()
        .into_iter()
        .filter(|definition| args.group.is_none_or(|group| definition.group == group))
        .collect::<Vec<_>>();
    json_result(json!({
        "schema_version": 1,
        "count": definitions.len(),
        "definitions": definitions,
    }))
}

pub fn analyze_semantics(workspace: &Workspace, args: AnalyzeProjectSemanticsArgs) -> ToolResult {
    let graph = load_project_graph(workspace);
    let previous = load_semantic_index(workspace);
    match analyze_project_semantics(workspace, &graph, previous, args.force) {
        Ok((_, report)) => json_result(json!({
            "status": "ready",
            "report": report,
            "storage": semantic_data_dir(workspace),
            "note": "Метки хранятся вне Unreal-проекта и не изменяют ассеты",
        })),
        Err(error) => ToolResult::error(error.to_string()),
    }
}

pub fn semantic_node_snapshot(workspace: &Workspace, args: SemanticNodeSnapshotArgs) -> ToolResult {
    let graph = load_project_graph(workspace);
    let index = match ensure_semantic_index(workspace, &graph) {
        Ok(index) => index,
        Err(error) => return ToolResult::error(error.to_string()),
    };
    let Some(node) = graph.nodes.iter().find(|node| node.id == args.node_id) else {
        return ToolResult::error(format!("Узел Project Map не найден: {}", args.node_id));
    };
    let annotation = index.nodes.iter().find(|item| item.node_id == args.node_id);
    let proposals = index
        .proposals
        .iter()
        .filter(|proposal| proposal.node_id == args.node_id)
        .collect::<Vec<_>>();
    let relations = graph
        .edges
        .iter()
        .filter(|edge| edge.from == args.node_id || edge.to == args.node_id)
        .take(100)
        .collect::<Vec<_>>();
    json_result(json!({
        "node": node,
        "annotation": annotation,
        "proposals": proposals,
        "relations": relations,
    }))
}

pub fn propose_semantic_labels(
    workspace: &Workspace,
    args: ProposeSemanticLabelsArgs,
) -> ToolResult {
    let graph = load_project_graph(workspace);
    if !graph.nodes.iter().any(|node| node.id == args.node_id) {
        return ToolResult::error(format!("Узел Project Map не найден: {}", args.node_id));
    }
    let catalog = semantic_catalog_map();
    let unknown = args
        .tag_ids
        .iter()
        .filter(|tag_id| !catalog.contains_key(*tag_id))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return ToolResult::error(format!("Неизвестные метки: {}", unknown.join(", ")));
    }
    let mut state = match ensure_semantic_index(workspace, &graph) {
        Ok(state) => state,
        Err(error) => return ToolResult::error(error.to_string()),
    };
    let now = unix_timestamp();
    let confidence = args.confidence.clamp(0.0, 1.0);
    let mut created = Vec::new();
    for tag_id in &args.tag_ids {
        if state.nodes.iter().any(|annotation| {
            annotation.node_id == args.node_id
                && annotation
                    .assignments
                    .iter()
                    .any(|assignment| assignment.tag_id == *tag_id)
        }) {
            continue;
        }
        let proposal = SemanticProposal {
            id: format!("semantic-proposal-{}", Uuid::new_v4()),
            node_id: args.node_id.clone(),
            tag_id: tag_id.clone(),
            confidence,
            reason: args.reason.clone(),
            source: SemanticEvidenceSource::AiProposal,
            status: "pending".to_string(),
            created_at: now,
            updated_at: now,
        };
        created.push(proposal.clone());
        state.proposals.push(proposal);
    }
    state.updated_at = now;
    if let Err(error) = save_semantic_index(workspace, &state) {
        return ToolResult::error(error.to_string());
    }
    let _ = append_semantic_audit(
        workspace,
        "propose_labels",
        Some(&args.node_id),
        &args.tag_ids,
        &args.reason,
    );
    json_result(json!({
        "created": created,
        "requires_user_confirmation": true,
    }))
}

pub fn decide_semantic_proposals(
    workspace: &Workspace,
    args: DecideSemanticProposalsArgs,
) -> ToolResult {
    let mut state = load_semantic_index(workspace);
    let now = unix_timestamp();
    let selected = args.proposal_ids.iter().cloned().collect::<BTreeSet<_>>();
    let graph_nodes = load_project_graph(workspace)
        .nodes
        .into_iter()
        .map(|node| {
            (
                node.id,
                (
                    node.label,
                    node.metadata.get("object_path").cloned().or(node.path),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut decisions = Vec::new();
    let proposals = state.proposals.clone();
    for proposal in proposals
        .iter()
        .filter(|proposal| selected.contains(&proposal.id))
    {
        if args.accept {
            let label = graph_nodes
                .get(&proposal.node_id)
                .cloned()
                .unwrap_or_else(|| (proposal.node_id.clone(), None));
            let annotation = state
                .nodes
                .iter_mut()
                .find(|annotation| annotation.node_id == proposal.node_id);
            let annotation = match annotation {
                Some(annotation) => annotation,
                None => {
                    state.nodes.push(SemanticNodeAnnotation {
                        node_id: proposal.node_id.clone(),
                        object_path: label.1,
                        label: label.0,
                        assignments: Vec::new(),
                        updated_at: now,
                    });
                    state.nodes.last_mut().expect("annotation was inserted")
                }
            };
            upsert_assignment(
                &mut annotation.assignments,
                SemanticTagAssignment {
                    tag_id: proposal.tag_id.clone(),
                    confidence: 1.0,
                    source: SemanticEvidenceSource::UserConfirmed,
                    evidence: vec![SemanticEvidence {
                        source: proposal.source,
                        detail: proposal.reason.clone(),
                        confidence: proposal.confidence,
                    }],
                    confirmed_by_user: true,
                    locked: true,
                    updated_at: now,
                },
            );
            annotation.updated_at = now;
        }
        if let Some(stored) = state
            .proposals
            .iter_mut()
            .find(|stored| stored.id == proposal.id)
        {
            stored.status = if args.accept { "accepted" } else { "rejected" }.to_string();
            stored.updated_at = now;
        }
        decisions.push(json!({
            "proposal_id": proposal.id,
            "node_id": proposal.node_id,
            "tag_id": proposal.tag_id,
            "decision": if args.accept { "accepted" } else { "rejected" },
        }));
    }
    state.updated_at = now;
    if let Err(error) = save_semantic_index(workspace, &state) {
        return ToolResult::error(error.to_string());
    }
    let _ = append_semantic_audit(
        workspace,
        if args.accept {
            "accept_proposals"
        } else {
            "reject_proposals"
        },
        None,
        &[],
        &format!("{} решений", decisions.len()),
    );
    json_result(json!({ "decisions": decisions }))
}

pub fn update_semantic_labels(workspace: &Workspace, args: UpdateSemanticLabelsArgs) -> ToolResult {
    let graph = load_project_graph(workspace);
    let Some(node) = graph.nodes.iter().find(|node| node.id == args.node_id) else {
        return ToolResult::error(format!("Узел Project Map не найден: {}", args.node_id));
    };
    let catalog = semantic_catalog_map();
    let unknown = args
        .add_tag_ids
        .iter()
        .filter(|tag_id| !catalog.contains_key(*tag_id))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return ToolResult::error(format!("Неизвестные метки: {}", unknown.join(", ")));
    }
    let mut state = match ensure_semantic_index(workspace, &graph) {
        Ok(state) => state,
        Err(error) => return ToolResult::error(error.to_string()),
    };
    let now = unix_timestamp();
    for tag_id in &args.remove_tag_ids {
        if let Some(proposal) = state
            .proposals
            .iter_mut()
            .find(|proposal| proposal.node_id == args.node_id && proposal.tag_id == *tag_id)
        {
            proposal.status = "rejected".to_string();
            proposal.updated_at = now;
        } else {
            state.proposals.push(SemanticProposal {
                id: format!("semantic-suppression-{}", Uuid::new_v4()),
                node_id: args.node_id.clone(),
                tag_id: tag_id.clone(),
                confidence: 1.0,
                reason: "метка исключена пользователем".to_string(),
                source: SemanticEvidenceSource::UserConfirmed,
                status: "rejected".to_string(),
                created_at: now,
                updated_at: now,
            });
        }
    }
    for tag_id in &args.add_tag_ids {
        state
            .proposals
            .retain(|proposal| !(proposal.node_id == args.node_id && proposal.tag_id == *tag_id));
    }
    let annotation = match state
        .nodes
        .iter_mut()
        .find(|annotation| annotation.node_id == args.node_id)
    {
        Some(annotation) => annotation,
        None => {
            state.nodes.push(SemanticNodeAnnotation {
                node_id: node.id.clone(),
                object_path: node
                    .metadata
                    .get("object_path")
                    .cloned()
                    .or_else(|| node.path.clone()),
                label: node.label.clone(),
                assignments: Vec::new(),
                updated_at: now,
            });
            state.nodes.last_mut().expect("annotation was inserted")
        }
    };
    annotation
        .assignments
        .retain(|assignment| !args.remove_tag_ids.contains(&assignment.tag_id));
    for tag_id in &args.add_tag_ids {
        upsert_assignment(
            &mut annotation.assignments,
            SemanticTagAssignment {
                tag_id: tag_id.clone(),
                confidence: 1.0,
                source: SemanticEvidenceSource::UserConfirmed,
                evidence: vec![SemanticEvidence {
                    source: SemanticEvidenceSource::UserConfirmed,
                    detail: "метка добавлена пользователем".to_string(),
                    confidence: 1.0,
                }],
                confirmed_by_user: true,
                locked: true,
                updated_at: now,
            },
        );
    }
    annotation.updated_at = now;
    state.updated_at = now;
    if let Err(error) = save_semantic_index(workspace, &state) {
        return ToolResult::error(error.to_string());
    }
    let mut changed = args.add_tag_ids.clone();
    changed.extend(args.remove_tag_ids.clone());
    let _ = append_semantic_audit(
        workspace,
        "update_labels",
        Some(&args.node_id),
        &changed,
        "ручное изменение меток",
    );
    semantic_node_snapshot(
        workspace,
        SemanticNodeSnapshotArgs {
            node_id: args.node_id,
        },
    )
}

pub fn resolve_semantic_targets(
    workspace: &Workspace,
    args: ResolveSemanticTargetsArgs,
) -> ToolResult {
    let Some(operation) = find_operation(&args.operation_id) else {
        return ToolResult::error(format!("Операция не найдена: {}", args.operation_id));
    };
    let graph = load_project_graph(workspace);
    let index = match ensure_semantic_index(workspace, &graph) {
        Ok(index) => index,
        Err(error) => return ToolResult::error(error.to_string()),
    };
    let catalog = semantic_catalog_map();
    let query = args.query.trim().to_ascii_lowercase();
    let annotation_map = index
        .nodes
        .iter()
        .map(|annotation| (annotation.node_id.as_str(), annotation))
        .collect::<BTreeMap<_, _>>();
    let candidate_nodes = graph
        .nodes
        .iter()
        .filter(|node| operation.target.allowed_node_kinds.contains(&node.kind))
        .filter(|node| {
            query.is_empty()
                || node.label.to_ascii_lowercase().contains(&query)
                || annotation_map
                    .get(node.id.as_str())
                    .is_some_and(|annotation| {
                        annotation.assignments.iter().any(|assignment| {
                            assignment.tag_id.to_ascii_lowercase().contains(&query)
                                || catalog.get(&assignment.tag_id).is_some_and(|definition| {
                                    definition.label.to_ascii_lowercase().contains(&query)
                                })
                        })
                    })
        })
        .collect::<Vec<_>>();
    let candidate_ids = candidate_nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let scores = rank_semantic_candidates(&index, &graph, &operation, &candidate_ids);
    let mut rows = candidate_nodes
        .into_iter()
        .filter_map(|node| {
            scores
                .get(&node.id)
                .map(|score| json!({ "node": node, "semantic": score }))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right["semantic"]["score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&left["semantic"]["score"].as_f64().unwrap_or_default())
    });
    rows.truncate(args.limit.clamp(1, 100));
    json_result(json!({
        "operation_id": operation.id,
        "query": args.query,
        "results": rows,
        "hard_filter": "target contract + current Project Map",
    }))
}

pub fn export_semantics(workspace: &Workspace, _args: ExportSemanticIndexArgs) -> ToolResult {
    match export_semantic_index(workspace) {
        Ok(path) => json_result(json!({ "path": path, "explicit_export": true })),
        Err(error) => ToolResult::error(error.to_string()),
    }
}

fn json_result(value: serde_json::Value) -> ToolResult {
    ToolResult::ok(serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()))
}
