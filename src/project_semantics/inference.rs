use super::store::{
    append_semantic_audit, project_key, save_semantic_index, unix_timestamp, upsert_assignment,
};
use super::types::{
    SemanticAnalysisReport, SemanticEvidence, SemanticEvidenceSource, SemanticIndexState,
    SemanticNodeAnnotation, SemanticProposal, SemanticTagAssignment,
};
use crate::project_graph::{
    project_graph_fingerprint, ProjectGraphEdgeKind, ProjectGraphNode, ProjectGraphNodeKind,
    ProjectGraphState,
};
use crate::workspace::Workspace;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

const AUTO_ASSIGNMENT_THRESHOLD: f32 = 0.85;
const PROPOSAL_THRESHOLD: f32 = 0.50;
const MAX_RELATION_PROPOSALS: usize = 400;

#[derive(Clone, Debug)]
struct InferredTag {
    tag_id: String,
    confidence: f32,
    reason: String,
    source: SemanticEvidenceSource,
}

pub fn analyze_project_semantics(
    workspace: &Workspace,
    graph: &ProjectGraphState,
    previous: SemanticIndexState,
    force: bool,
) -> anyhow::Result<(SemanticIndexState, SemanticAnalysisReport)> {
    let started = Instant::now();
    let fingerprint = project_graph_fingerprint(graph);
    if !force && previous.graph_fingerprint == fingerprint && !previous.nodes.is_empty() {
        let report = report_for_state(
            &previous,
            graph.nodes.len(),
            false,
            started.elapsed().as_millis() as u64,
        );
        return Ok((previous, report));
    }

    let now = unix_timestamp();
    let mut preserved_assignments = BTreeMap::<String, Vec<SemanticTagAssignment>>::new();
    for annotation in &previous.nodes {
        let assignments = annotation
            .assignments
            .iter()
            .filter(|assignment| assignment.confirmed_by_user || assignment.locked)
            .cloned()
            .collect::<Vec<_>>();
        if !assignments.is_empty() {
            preserved_assignments.insert(annotation.node_id.clone(), assignments);
        }
    }
    let rejected = previous
        .proposals
        .iter()
        .filter(|proposal| proposal.status == "rejected")
        .map(|proposal| (proposal.node_id.clone(), proposal.tag_id.clone()))
        .collect::<BTreeSet<_>>();
    let mut proposals = previous
        .proposals
        .iter()
        .filter(|proposal| proposal.status == "rejected")
        .cloned()
        .collect::<Vec<_>>();
    let mut nodes = Vec::with_capacity(graph.nodes.len());

    for node in &graph.nodes {
        let mut annotation = SemanticNodeAnnotation {
            node_id: node.id.clone(),
            object_path: node
                .metadata
                .get("object_path")
                .cloned()
                .or_else(|| node.path.clone()),
            label: node.label.clone(),
            assignments: preserved_assignments.remove(&node.id).unwrap_or_default(),
            updated_at: now,
        };
        for inferred in infer_node_tags(node) {
            if annotation
                .assignments
                .iter()
                .any(|assignment| assignment.tag_id == inferred.tag_id)
            {
                continue;
            }
            if inferred.confidence >= AUTO_ASSIGNMENT_THRESHOLD {
                upsert_assignment(
                    &mut annotation.assignments,
                    assignment_from_inference(&inferred, now),
                );
            } else if inferred.confidence >= PROPOSAL_THRESHOLD
                && !rejected.contains(&(node.id.clone(), inferred.tag_id.clone()))
            {
                upsert_proposal(
                    &mut proposals,
                    proposal_from_inference(node, &inferred, now),
                );
            }
        }
        if !annotation.assignments.is_empty()
            || proposals.iter().any(|proposal| proposal.node_id == node.id)
        {
            nodes.push(annotation);
        }
    }

    add_relation_proposals(graph, &nodes, &rejected, &mut proposals, now);
    proposals.sort_by(|left, right| {
        left.status
            .cmp(&right.status)
            .then(right.confidence.total_cmp(&left.confidence))
            .then(left.node_id.cmp(&right.node_id))
            .then(left.tag_id.cmp(&right.tag_id))
    });

    let mut state = SemanticIndexState {
        schema_version: 1,
        project_key: project_key(workspace),
        graph_fingerprint: fingerprint,
        nodes,
        proposals,
        relations: previous.relations,
        updated_at: now,
    };
    state
        .nodes
        .sort_by(|left, right| left.node_id.cmp(&right.node_id));
    save_semantic_index(workspace, &state)?;
    let report = report_for_state(
        &state,
        graph.nodes.len(),
        true,
        started.elapsed().as_millis() as u64,
    );
    append_semantic_audit(
        workspace,
        "analyze",
        None,
        &[],
        &format!(
            "{} узлов с метками, {} предложений, fingerprint {}",
            report.annotated_nodes, report.pending_proposals, report.graph_fingerprint
        ),
    )?;
    Ok((state, report))
}

fn infer_node_tags(node: &ProjectGraphNode) -> Vec<InferredTag> {
    let mut tags = Vec::new();
    let source = node_evidence_source(node);
    let text = node_text(node);

    match node.kind {
        ProjectGraphNodeKind::UnrealBlueprint => {
            push(
                &mut tags,
                "entity.blueprint",
                0.98,
                "тип узла Blueprint",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealWidget => {
            push(
                &mut tags,
                "entity.widget",
                0.99,
                "тип узла UMG Widget",
                source,
            );
            push(
                &mut tags,
                "domain.ui_ux_accessibility",
                0.95,
                "Widget относится к интерфейсу",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealInputAsset => {
            push(
                &mut tags,
                "entity.input",
                0.99,
                "тип узла Input Asset",
                source,
            );
            push(
                &mut tags,
                "domain.ui_ux_accessibility",
                0.88,
                "Input Asset относится к управлению",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealAnimation
        | ProjectGraphNodeKind::UnrealAnimationBlueprint
        | ProjectGraphNodeKind::UnrealAnimationMontage
        | ProjectGraphNodeKind::UnrealControlRig => {
            push(
                &mut tags,
                "entity.animation",
                0.99,
                "тип анимационного ассета",
                source,
            );
            push(
                &mut tags,
                "domain.characters_animation",
                0.96,
                "анимационный ассет",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealSkeleton => {
            push(&mut tags, "entity.skeleton", 0.99, "тип Skeleton", source);
            push(
                &mut tags,
                "domain.characters_animation",
                0.95,
                "Skeleton персонажа",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealSkeletalMesh => {
            push(
                &mut tags,
                "entity.skeletal_mesh",
                0.99,
                "тип Skeletal Mesh",
                source,
            );
            push(
                &mut tags,
                "domain.characters_animation",
                0.90,
                "скелетная модель",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealStaticMesh => {
            push(
                &mut tags,
                "entity.static_mesh",
                0.99,
                "тип Static Mesh",
                source,
            );
            push(
                &mut tags,
                "domain.graphics_assets_vfx",
                0.90,
                "статическая модель",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealMap => {
            push(&mut tags, "entity.level", 0.99, "тип карты Unreal", source);
            push(
                &mut tags,
                "domain.world_level_design",
                0.96,
                "игровой уровень",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealSound => {
            push(
                &mut tags,
                "entity.audio",
                0.99,
                "тип звукового ассета",
                source,
            );
            push(
                &mut tags,
                "domain.audio_music_voice",
                0.97,
                "звуковой ассет",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealMaterial => {
            push(&mut tags, "entity.material", 0.99, "тип Material", source);
            push(
                &mut tags,
                "domain.graphics_assets_vfx",
                0.96,
                "графический ассет",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealNiagara => {
            push(&mut tags, "entity.vfx", 0.99, "тип Niagara", source);
            push(
                &mut tags,
                "domain.graphics_assets_vfx",
                0.96,
                "визуальный эффект",
                source,
            );
        }
        ProjectGraphNodeKind::UnrealDataAsset => {
            push(&mut tags, "entity.data", 0.97, "тип Data Asset", source);
        }
        _ => {}
    }

    if has_any(&text, &["character", "_char", "pawn"]) {
        push(
            &mut tags,
            "entity.character",
            0.88,
            "имя указывает на Character/Pawn",
            source,
        );
        push(
            &mut tags,
            "domain.characters_animation",
            0.72,
            "объект похож на персонажа",
            source,
        );
    }
    if has_any(&text, &["playercontroller", "player_controller", "pc_"]) {
        push(
            &mut tags,
            "entity.controller",
            0.90,
            "имя PlayerController",
            source,
        );
        push(
            &mut tags,
            "role.player.controller",
            0.78,
            "вероятный контроллер игрока",
            source,
        );
    }
    if has_any(
        &text,
        &[
            "bp_character_player",
            "bp_player_character",
            "playercharacter",
            "player_character",
        ],
    ) {
        push(
            &mut tags,
            "role.player.primary",
            0.82,
            "имя похоже на главного игрового персонажа",
            source,
        );
        push(
            &mut tags,
            "importance.primary",
            0.72,
            "вероятно основной объект системы",
            source,
        );
    }
    if has_any(&text, &["npc", "nonplayer"]) {
        push(
            &mut tags,
            "role.npc.base",
            0.72,
            "имя указывает на NPC",
            source,
        );
    }
    if has_any(&text, &["enemy", "hostile"]) {
        push(
            &mut tags,
            "role.enemy.base",
            0.74,
            "имя указывает на противника",
            source,
        );
    }
    if has_any(&text, &["companion", "follower"]) {
        push(
            &mut tags,
            "role.companion",
            0.74,
            "имя указывает на компаньона",
            source,
        );
    }
    if has_any(&text, &["hud", "heads_up"]) {
        push(
            &mut tags,
            "system.ui_ux_accessibility.hud",
            0.91,
            "имя содержит HUD",
            source,
        );
        push(
            &mut tags,
            "domain.ui_ux_accessibility",
            0.92,
            "объект относится к HUD",
            source,
        );
        if has_any(&text, &["wbp_hud", "main_hud", "hud_root"]) {
            push(
                &mut tags,
                "role.hud.root",
                0.82,
                "вероятный корневой HUD",
                source,
            );
        }
    }
    if has_any(&text, &["crosshair", "reticle"]) {
        push(
            &mut tags,
            "role.crosshair",
            0.93,
            "имя указывает на прицел",
            source,
        );
        push(
            &mut tags,
            "system.ui_ux_accessibility.hud",
            0.92,
            "прицел является частью HUD",
            source,
        );
        push(
            &mut tags,
            "capability.aim",
            0.91,
            "прицел участвует в прицеливании",
            source,
        );
    }
    if has_any(&text, &["aim", "ads_"]) {
        push(
            &mut tags,
            "capability.aim",
            0.78,
            "имя связано с прицеливанием",
            source,
        );
        push(
            &mut tags,
            "system.gameplay.combat",
            0.66,
            "возможная часть боевой системы",
            source,
        );
    }
    if has_any(&text, &["jump", "locomotion_jump"]) {
        push(
            &mut tags,
            "capability.jump",
            0.80,
            "имя связано с прыжком",
            source,
        );
        push(
            &mut tags,
            "system.gameplay.movement",
            0.72,
            "часть передвижения",
            source,
        );
    }
    if has_any(&text, &["movement", "locomotion", "move_"]) {
        push(
            &mut tags,
            "capability.move",
            0.78,
            "имя связано с перемещением",
            source,
        );
        push(
            &mut tags,
            "system.gameplay.movement",
            0.76,
            "часть передвижения",
            source,
        );
    }
    if has_any(&text, &["interact", "interaction"]) {
        push(
            &mut tags,
            "capability.interact",
            0.80,
            "имя связано со взаимодействием",
            source,
        );
    }
    if has_any(&text, &["healthbar", "health_bar", "hp_bar"]) {
        push(
            &mut tags,
            "capability.display_health",
            0.90,
            "элемент отображает здоровье",
            source,
        );
        push(
            &mut tags,
            "system.ui_ux_accessibility.hud",
            0.88,
            "элемент HUD",
            source,
        );
    }
    if has_any(&text, &["ammo", "ammunition"]) {
        push(
            &mut tags,
            "capability.display_ammo",
            0.78,
            "объект связан с боезапасом",
            source,
        );
    }
    if has_any(&text, &["weapon", "rifle", "pistol", "gun_"]) {
        push(
            &mut tags,
            "entity.weapon",
            0.82,
            "имя указывает на оружие",
            source,
        );
        push(
            &mut tags,
            "system.gameplay.combat",
            0.78,
            "часть боевой системы",
            source,
        );
    }
    if has_any(&text, &["mainmenu", "main_menu", "menu_root"]) {
        push(
            &mut tags,
            "role.menu.root",
            0.83,
            "вероятное главное меню",
            source,
        );
        push(
            &mut tags,
            "domain.ui_ux_accessibility",
            0.86,
            "экран интерфейса",
            source,
        );
    }
    if has_any(&text, &["playercamera", "player_camera", "camera_player"]) {
        push(
            &mut tags,
            "role.camera.player",
            0.82,
            "камера игрока",
            source,
        );
    }
    if has_any(&text, &["deprecated", "obsolete", "legacy"]) {
        push(
            &mut tags,
            "state.deprecated",
            0.90,
            "маркер устаревшего объекта",
            source,
        );
    } else if has_any(&text, &["prototype", "experimental", "wip_"]) {
        push(
            &mut tags,
            "state.experimental",
            0.82,
            "маркер экспериментального объекта",
            source,
        );
    }
    if has_any(&text, &["test", "fixture", "mock_"]) {
        push(&mut tags, "scope.test", 0.82, "тестовый объект", source);
    } else if has_any(&text, &["editor", "edmode", "factory"]) {
        push(&mut tags, "scope.editor", 0.76, "объект редактора", source);
    } else if is_unreal_runtime_kind(node.kind) {
        push(
            &mut tags,
            "scope.runtime",
            0.68,
            "игровой Unreal-ассет",
            source,
        );
    }
    if has_any(&text, &["base", "shared", "common"]) {
        push(
            &mut tags,
            "importance.shared",
            0.68,
            "переиспользуемая база",
            source,
        );
    }

    deduplicate(tags)
}

fn add_relation_proposals(
    graph: &ProjectGraphState,
    annotations: &[SemanticNodeAnnotation],
    rejected: &BTreeSet<(String, String)>,
    proposals: &mut Vec<SemanticProposal>,
    now: u64,
) {
    let annotation_map = annotations
        .iter()
        .map(|annotation| (annotation.node_id.as_str(), annotation))
        .collect::<BTreeMap<_, _>>();
    let node_map = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut added = 0usize;
    for edge in &graph.edges {
        if added >= MAX_RELATION_PROPOSALS || !is_semantic_edge(edge.kind) {
            break;
        }
        for (source_id, target_id) in [(&edge.from, &edge.to), (&edge.to, &edge.from)] {
            let Some(source) = annotation_map.get(source_id.as_str()) else {
                continue;
            };
            let Some(target_node) = node_map.get(target_id.as_str()) else {
                continue;
            };
            if !is_unreal_runtime_kind(target_node.kind) {
                continue;
            }
            for assignment in source.assignments.iter().filter(|assignment| {
                assignment.tag_id.starts_with("system.") || assignment.tag_id.starts_with("domain.")
            }) {
                if assignment.confidence < 0.75
                    || annotation_map
                        .get(target_id.as_str())
                        .is_some_and(|annotation| {
                            annotation
                                .assignments
                                .iter()
                                .any(|existing| existing.tag_id == assignment.tag_id)
                        })
                    || rejected.contains(&(target_id.clone(), assignment.tag_id.clone()))
                {
                    continue;
                }
                let confidence = (assignment.confidence * edge.confidence.max(0.6) * 0.78)
                    .clamp(PROPOSAL_THRESHOLD, 0.79);
                let inferred = InferredTag {
                    tag_id: assignment.tag_id.clone(),
                    confidence,
                    reason: format!(
                        "связан с «{}» через «{}»",
                        source.label,
                        if edge.label.is_empty() {
                            edge.kind.label()
                        } else {
                            &edge.label
                        }
                    ),
                    source: SemanticEvidenceSource::GraphRelation,
                };
                upsert_proposal(
                    proposals,
                    proposal_from_inference(target_node, &inferred, now),
                );
                added += 1;
                if added >= MAX_RELATION_PROPOSALS {
                    break;
                }
            }
        }
    }
}

fn report_for_state(
    state: &SemanticIndexState,
    analyzed_nodes: usize,
    changed: bool,
    duration_ms: u64,
) -> SemanticAnalysisReport {
    let assignments = state
        .nodes
        .iter()
        .flat_map(|annotation| annotation.assignments.iter())
        .collect::<Vec<_>>();
    SemanticAnalysisReport {
        project_key: state.project_key.clone(),
        graph_fingerprint: state.graph_fingerprint.clone(),
        analyzed_nodes,
        annotated_nodes: state.nodes.len(),
        confirmed_assignments: assignments
            .iter()
            .filter(|assignment| assignment.confirmed_by_user)
            .count(),
        automatic_assignments: assignments
            .iter()
            .filter(|assignment| !assignment.confirmed_by_user)
            .count(),
        pending_proposals: state
            .proposals
            .iter()
            .filter(|proposal| proposal.status == "pending")
            .count(),
        changed,
        duration_ms,
        updated_at: state.updated_at,
    }
}

fn assignment_from_inference(inferred: &InferredTag, now: u64) -> SemanticTagAssignment {
    SemanticTagAssignment {
        tag_id: inferred.tag_id.clone(),
        confidence: inferred.confidence,
        source: inferred.source,
        evidence: vec![SemanticEvidence {
            source: inferred.source,
            detail: inferred.reason.clone(),
            confidence: inferred.confidence,
        }],
        confirmed_by_user: false,
        locked: false,
        updated_at: now,
    }
}

fn proposal_from_inference(
    node: &ProjectGraphNode,
    inferred: &InferredTag,
    now: u64,
) -> SemanticProposal {
    SemanticProposal {
        id: stable_proposal_id(&node.id, &inferred.tag_id),
        node_id: node.id.clone(),
        tag_id: inferred.tag_id.clone(),
        confidence: inferred.confidence,
        reason: inferred.reason.clone(),
        source: inferred.source,
        status: "pending".to_string(),
        created_at: now,
        updated_at: now,
    }
}

fn upsert_proposal(proposals: &mut Vec<SemanticProposal>, proposal: SemanticProposal) {
    if let Some(existing) = proposals
        .iter_mut()
        .find(|existing| existing.id == proposal.id)
    {
        if existing.status == "pending" {
            *existing = proposal;
        }
    } else {
        proposals.push(proposal);
    }
}

fn stable_proposal_id(node_id: &str, tag_id: &str) -> String {
    let digest = Sha256::digest(format!("{node_id}\n{tag_id}").as_bytes());
    format!("semantic-proposal-{:x}", digest)[..34].to_string()
}

fn node_text(node: &ProjectGraphNode) -> String {
    let mut parts = vec![
        node.label.clone(),
        node.path.clone().unwrap_or_default(),
        node.summary.clone(),
        node.kind.as_str().to_string(),
    ];
    for (key, value) in &node.metadata {
        parts.push(key.clone());
        parts.push(value.clone());
    }
    parts.join(" ").to_ascii_lowercase()
}

fn node_evidence_source(node: &ProjectGraphNode) -> SemanticEvidenceSource {
    if node.source.to_ascii_lowercase().contains("asset_registry")
        || node
            .metadata
            .values()
            .any(|value| value.to_ascii_lowercase().contains("asset_registry"))
    {
        SemanticEvidenceSource::AssetRegistry
    } else {
        SemanticEvidenceSource::PathAndName
    }
}

fn has_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn push(
    tags: &mut Vec<InferredTag>,
    tag_id: &str,
    confidence: f32,
    reason: &str,
    source: SemanticEvidenceSource,
) {
    tags.push(InferredTag {
        tag_id: tag_id.to_string(),
        confidence,
        reason: reason.to_string(),
        source,
    });
}

fn deduplicate(tags: Vec<InferredTag>) -> Vec<InferredTag> {
    let mut unique = BTreeMap::<String, InferredTag>::new();
    for tag in tags {
        match unique.get(&tag.tag_id) {
            Some(existing) if existing.confidence >= tag.confidence => {}
            _ => {
                unique.insert(tag.tag_id.clone(), tag);
            }
        }
    }
    unique.into_values().collect()
}

fn is_unreal_runtime_kind(kind: ProjectGraphNodeKind) -> bool {
    matches!(
        kind,
        ProjectGraphNodeKind::UnrealBlueprint
            | ProjectGraphNodeKind::UnrealMap
            | ProjectGraphNodeKind::UnrealDataAsset
            | ProjectGraphNodeKind::UnrealMaterial
            | ProjectGraphNodeKind::UnrealNiagara
            | ProjectGraphNodeKind::UnrealAnimation
            | ProjectGraphNodeKind::UnrealSkeleton
            | ProjectGraphNodeKind::UnrealSkeletalMesh
            | ProjectGraphNodeKind::UnrealStaticMesh
            | ProjectGraphNodeKind::UnrealAnimationBlueprint
            | ProjectGraphNodeKind::UnrealAnimationMontage
            | ProjectGraphNodeKind::UnrealControlRig
            | ProjectGraphNodeKind::UnrealPhysicsAsset
            | ProjectGraphNodeKind::UnrealSound
            | ProjectGraphNodeKind::UnrealWidget
            | ProjectGraphNodeKind::UnrealInputAsset
            | ProjectGraphNodeKind::UnrealAsset
    )
}

fn is_semantic_edge(kind: ProjectGraphEdgeKind) -> bool {
    matches!(
        kind,
        ProjectGraphEdgeKind::References
            | ProjectGraphEdgeKind::UsesSkeleton
            | ProjectGraphEdgeKind::Animates
            | ProjectGraphEdgeKind::ControlledBy
            | ProjectGraphEdgeKind::HasComponent
            | ProjectGraphEdgeKind::CompatibleWith
            | ProjectGraphEdgeKind::SpawnedBy
            | ProjectGraphEdgeKind::OwnedBy
            | ProjectGraphEdgeKind::BoundToInput
            | ProjectGraphEdgeKind::Produces
            | ProjectGraphEdgeKind::Consumes
            | ProjectGraphEdgeKind::RelatedTo
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str, kind: ProjectGraphNodeKind) -> ProjectGraphNode {
        ProjectGraphNode {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            path: Some(format!("/Game/{label}")),
            summary: String::new(),
            source: "unreal_asset_registry".to_string(),
            confidence: 1.0,
            metadata: BTreeMap::new(),
            updated_at: 0,
        }
    }

    #[test]
    fn recognizes_hud_player_and_ignores_grass_roles() {
        let hud = infer_node_tags(&node("hud", "WBP_HUD", ProjectGraphNodeKind::UnrealWidget));
        assert!(hud
            .iter()
            .any(|tag| tag.tag_id == "system.ui_ux_accessibility.hud"));
        assert!(hud.iter().any(|tag| tag.tag_id == "entity.widget"));

        let player = infer_node_tags(&node(
            "player",
            "BP_Character_Player",
            ProjectGraphNodeKind::UnrealBlueprint,
        ));
        assert!(player.iter().any(|tag| tag.tag_id == "role.player.primary"));

        let grass = infer_node_tags(&node(
            "grass",
            "SM_Grass",
            ProjectGraphNodeKind::UnrealStaticMesh,
        ));
        assert!(!grass.iter().any(|tag| tag.tag_id.starts_with("role.")));
    }

    #[test]
    fn refresh_preserves_confirmed_labels_and_respects_rejections() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let graph = ProjectGraphState {
            nodes: vec![node(
                "player",
                "BP_Character_Player",
                ProjectGraphNodeKind::UnrealBlueprint,
            )],
            ..ProjectGraphState::default()
        };
        let now = unix_timestamp();
        let previous = SemanticIndexState {
            nodes: vec![SemanticNodeAnnotation {
                node_id: "player".to_string(),
                object_path: Some("/Game/BP_Character_Player".to_string()),
                label: "BP_Character_Player".to_string(),
                assignments: vec![SemanticTagAssignment {
                    tag_id: "role.player.primary".to_string(),
                    confidence: 1.0,
                    source: SemanticEvidenceSource::UserConfirmed,
                    evidence: Vec::new(),
                    confirmed_by_user: true,
                    locked: true,
                    updated_at: now,
                }],
                updated_at: now,
            }],
            proposals: vec![SemanticProposal {
                id: "rejected-character-domain".to_string(),
                node_id: "player".to_string(),
                tag_id: "domain.characters_animation".to_string(),
                confidence: 0.72,
                reason: "пользователь отклонил предположение".to_string(),
                source: SemanticEvidenceSource::PathAndName,
                status: "rejected".to_string(),
                created_at: now,
                updated_at: now,
            }],
            ..SemanticIndexState::default()
        };

        let (state, _) = analyze_project_semantics(&workspace, &graph, previous, true).unwrap();
        let annotation = state
            .nodes
            .iter()
            .find(|annotation| annotation.node_id == "player")
            .unwrap();

        assert!(annotation.assignments.iter().any(|assignment| {
            assignment.tag_id == "role.player.primary"
                && assignment.confirmed_by_user
                && assignment.locked
        }));
        assert!(!state.proposals.iter().any(|proposal| {
            proposal.node_id == "player"
                && proposal.tag_id == "domain.characters_animation"
                && proposal.status == "pending"
        }));
        assert!(state.proposals.iter().any(|proposal| {
            proposal.node_id == "player"
                && proposal.tag_id == "domain.characters_animation"
                && proposal.status == "rejected"
        }));

        let _ = std::fs::remove_dir_all(crate::project_semantics::semantic_data_dir(&workspace));
    }
}
