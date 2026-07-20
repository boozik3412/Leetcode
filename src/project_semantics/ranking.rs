use super::catalog::semantic_catalog_map;
use super::types::{SemanticCandidateScore, SemanticIndexState};
use crate::game_task_builder::{TargetContractKind, TaskOperation};
use crate::project_graph::{ProjectGraphNodeKind, ProjectGraphState};
use std::collections::{BTreeMap, BTreeSet};

pub fn rank_semantic_candidates(
    index: &SemanticIndexState,
    graph: &ProjectGraphState,
    operation: &TaskOperation,
    candidate_ids: &BTreeSet<String>,
) -> BTreeMap<String, SemanticCandidateScore> {
    let catalog = semantic_catalog_map();
    let annotations = index
        .nodes
        .iter()
        .map(|annotation| (annotation.node_id.as_str(), annotation))
        .collect::<BTreeMap<_, _>>();
    let node_kinds = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.kind))
        .collect::<BTreeMap<_, _>>();
    let wanted_domain = format!("domain.{}", operation.domain_id);
    let wanted_system = format!("system.{}", operation.direction_id);
    let wanted_entities = expected_entity_tags(operation.target.kind);
    let mut scores = BTreeMap::<String, SemanticCandidateScore>::new();

    for node_id in candidate_ids {
        let mut score = SemanticCandidateScore {
            node_id: node_id.clone(),
            score: 0.0,
            direct_match: false,
            relation_match: false,
            tag_ids: Vec::new(),
            tag_labels: Vec::new(),
            reasons: Vec::new(),
            relation_labels: Vec::new(),
        };
        if let Some(annotation) = annotations.get(node_id.as_str()) {
            for assignment in &annotation.assignments {
                let weight = if assignment.tag_id == wanted_system {
                    score.direct_match = true;
                    score.reasons.push("та же игровая подсистема".to_string());
                    0.42
                } else if assignment.tag_id == wanted_domain {
                    score.direct_match = true;
                    score.reasons.push("та же сфера разработки".to_string());
                    0.28
                } else if wanted_entities.contains(&assignment.tag_id.as_str()) {
                    score
                        .reasons
                        .push("подходящая игровая сущность".to_string());
                    0.16
                } else if assignment.tag_id.starts_with("role.") {
                    0.06
                } else if assignment.tag_id.starts_with("capability.") {
                    0.05
                } else {
                    0.0
                };
                if weight > 0.0 {
                    score.score += weight * assignment.confidence.max(0.5);
                    score.tag_ids.push(assignment.tag_id.clone());
                    if let Some(definition) = catalog.get(&assignment.tag_id) {
                        score.tag_labels.push(definition.label.clone());
                    }
                    if assignment.confirmed_by_user {
                        score.score += 0.08;
                        score
                            .reasons
                            .push("метка подтверждена пользователем".to_string());
                    }
                }
            }
        }
        if let Some(kind) = node_kinds.get(node_id.as_str()) {
            if node_kind_matches_contract(*kind, operation.target.kind) {
                score.score += 0.08;
            }
        }
        scores.insert(node_id.clone(), score);
    }

    for edge in &graph.edges {
        for (candidate_id, neighbor_id) in [(&edge.from, &edge.to), (&edge.to, &edge.from)] {
            if !candidate_ids.contains(candidate_id) {
                continue;
            }
            let Some(neighbor) = annotations.get(neighbor_id.as_str()) else {
                continue;
            };
            let relation_strength = neighbor
                .assignments
                .iter()
                .filter_map(|assignment| {
                    if assignment.tag_id == wanted_system {
                        Some(0.24 * assignment.confidence)
                    } else if assignment.tag_id == wanted_domain {
                        Some(0.14 * assignment.confidence)
                    } else {
                        None
                    }
                })
                .fold(0.0_f32, f32::max);
            if relation_strength <= 0.0 {
                continue;
            }
            if let Some(score) = scores.get_mut(candidate_id) {
                score.relation_match = true;
                score.score += relation_strength * edge.confidence.max(0.55);
                let label = if edge.label.is_empty() {
                    edge.kind.label().to_string()
                } else {
                    edge.label.clone()
                };
                score.relation_labels.push(label.clone());
                score
                    .reasons
                    .push(format!("связан через «{}» с {}", label, neighbor.label));
            }
        }
    }

    for score in scores.values_mut() {
        score.score = score.score.clamp(0.0, 1.0);
        score.tag_ids.sort();
        score.tag_ids.dedup();
        score.tag_labels.sort();
        score.tag_labels.dedup();
        score.reasons.sort();
        score.reasons.dedup();
        score.relation_labels.sort();
        score.relation_labels.dedup();
    }
    scores
}

fn expected_entity_tags(kind: TargetContractKind) -> &'static [&'static str] {
    match kind {
        TargetContractKind::Character | TargetContractKind::Ai => {
            &["entity.character", "entity.controller", "entity.blueprint"]
        }
        TargetContractKind::CharacterAnimation => &[
            "entity.character",
            "entity.animation",
            "entity.skeleton",
            "entity.skeletal_mesh",
        ],
        TargetContractKind::Map => &["entity.level"],
        TargetContractKind::StaticMesh => &["entity.static_mesh"],
        TargetContractKind::Material => &["entity.material"],
        TargetContractKind::Niagara => &["entity.vfx"],
        TargetContractKind::Ui => &["entity.widget", "entity.input", "entity.blueprint"],
        TargetContractKind::Audio => &["entity.audio"],
        _ => &[],
    }
}

fn node_kind_matches_contract(kind: ProjectGraphNodeKind, contract: TargetContractKind) -> bool {
    match contract {
        TargetContractKind::Character | TargetContractKind::Ai => matches!(
            kind,
            ProjectGraphNodeKind::UnrealBlueprint | ProjectGraphNodeKind::UnrealSkeletalMesh
        ),
        TargetContractKind::CharacterAnimation => matches!(
            kind,
            ProjectGraphNodeKind::UnrealAnimation
                | ProjectGraphNodeKind::UnrealAnimationBlueprint
                | ProjectGraphNodeKind::UnrealAnimationMontage
                | ProjectGraphNodeKind::UnrealControlRig
                | ProjectGraphNodeKind::UnrealSkeleton
                | ProjectGraphNodeKind::UnrealSkeletalMesh
                | ProjectGraphNodeKind::UnrealBlueprint
        ),
        TargetContractKind::Ui => matches!(
            kind,
            ProjectGraphNodeKind::UnrealWidget
                | ProjectGraphNodeKind::UnrealInputAsset
                | ProjectGraphNodeKind::UnrealBlueprint
        ),
        TargetContractKind::Map => kind == ProjectGraphNodeKind::UnrealMap,
        TargetContractKind::StaticMesh => kind == ProjectGraphNodeKind::UnrealStaticMesh,
        TargetContractKind::Material => kind == ProjectGraphNodeKind::UnrealMaterial,
        TargetContractKind::Niagara => kind == ProjectGraphNodeKind::UnrealNiagara,
        TargetContractKind::Audio => kind == ProjectGraphNodeKind::UnrealSound,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_task_builder::find_operation;
    use crate::project_graph::{ProjectGraphEdge, ProjectGraphNode};
    use crate::project_semantics::{
        SemanticEvidenceSource, SemanticNodeAnnotation, SemanticTagAssignment,
    };

    fn node(id: &str, label: &str, kind: ProjectGraphNodeKind) -> ProjectGraphNode {
        ProjectGraphNode {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            path: None,
            summary: String::new(),
            source: "test".to_string(),
            confidence: 1.0,
            metadata: BTreeMap::new(),
            updated_at: 1,
        }
    }

    fn annotation(node_id: &str, label: &str, tags: &[(&str, bool)]) -> SemanticNodeAnnotation {
        SemanticNodeAnnotation {
            node_id: node_id.to_string(),
            object_path: None,
            label: label.to_string(),
            assignments: tags
                .iter()
                .map(|(tag_id, confirmed)| SemanticTagAssignment {
                    tag_id: (*tag_id).to_string(),
                    confidence: 1.0,
                    source: if *confirmed {
                        SemanticEvidenceSource::UserConfirmed
                    } else {
                        SemanticEvidenceSource::PathAndName
                    },
                    evidence: Vec::new(),
                    confirmed_by_user: *confirmed,
                    locked: *confirmed,
                    updated_at: 1,
                })
                .collect(),
            updated_at: 1,
        }
    }

    #[test]
    fn confirmed_hud_player_ranks_above_unrelated_blueprint() {
        let operation = find_operation("ui_ux_accessibility.hud.modify").unwrap();
        let graph = ProjectGraphState {
            nodes: vec![
                node(
                    "player",
                    "BP_Character_Player",
                    ProjectGraphNodeKind::UnrealBlueprint,
                ),
                node(
                    "foliage",
                    "BP_Procedural_Grass",
                    ProjectGraphNodeKind::UnrealBlueprint,
                ),
            ],
            ..ProjectGraphState::default()
        };
        let index = SemanticIndexState {
            nodes: vec![
                annotation(
                    "player",
                    "BP_Character_Player",
                    &[
                        ("system.ui_ux_accessibility.hud", true),
                        ("role.player.primary", true),
                    ],
                ),
                annotation(
                    "foliage",
                    "BP_Procedural_Grass",
                    &[("domain.graphics_assets_vfx", false)],
                ),
            ],
            ..SemanticIndexState::default()
        };
        let candidates = ["player".to_string(), "foliage".to_string()]
            .into_iter()
            .collect();

        let scores = rank_semantic_candidates(&index, &graph, &operation, &candidates);

        assert!(scores["player"].direct_match);
        assert!(scores["player"].score > scores["foliage"].score);
        assert!(scores["player"]
            .tag_labels
            .iter()
            .any(|label| label == "HUD"));
    }

    #[test]
    fn graph_neighbor_can_make_character_related_to_hud_task() {
        let operation = find_operation("ui_ux_accessibility.hud.modify").unwrap();
        let graph = ProjectGraphState {
            nodes: vec![
                node(
                    "player",
                    "BP_Character_Player",
                    ProjectGraphNodeKind::UnrealBlueprint,
                ),
                node("hud", "WBP_HUD", ProjectGraphNodeKind::UnrealWidget),
            ],
            edges: vec![ProjectGraphEdge {
                id: "hud-owned-by-player".to_string(),
                from: "hud".to_string(),
                to: "player".to_string(),
                kind: crate::project_graph::ProjectGraphEdgeKind::OwnedBy,
                label: "принадлежит".to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                updated_at: 1,
            }],
            ..ProjectGraphState::default()
        };
        let index = SemanticIndexState {
            nodes: vec![annotation(
                "hud",
                "WBP_HUD",
                &[("system.ui_ux_accessibility.hud", false)],
            )],
            ..SemanticIndexState::default()
        };
        let candidates = ["player".to_string()].into_iter().collect();

        let scores = rank_semantic_candidates(&index, &graph, &operation, &candidates);

        assert!(scores["player"].relation_match);
        assert!(scores["player"].score >= 0.12);
        assert!(scores["player"]
            .reasons
            .iter()
            .any(|reason| reason.contains("WBP_HUD")));
    }
}
