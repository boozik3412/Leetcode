use crate::project_graph::ProjectGraphEdgeKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTagGroup {
    Domain,
    System,
    Entity,
    Role,
    Capability,
    Importance,
    State,
    Scope,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticTagDefinition {
    pub id: String,
    pub label: String,
    pub group: SemanticTagGroup,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticEvidenceSource {
    UserConfirmed,
    AssetRegistry,
    BlueprintAnalysis,
    GraphRelation,
    PathAndName,
    AiProposal,
}

impl SemanticEvidenceSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::UserConfirmed => "подтверждено пользователем",
            Self::AssetRegistry => "Unreal Asset Registry",
            Self::BlueprintAnalysis => "анализ Blueprint",
            Self::GraphRelation => "связи Project Map",
            Self::PathAndName => "имя и расположение",
            Self::AiProposal => "предложение AI",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticEvidence {
    pub source: SemanticEvidenceSource,
    pub detail: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticTagAssignment {
    pub tag_id: String,
    pub confidence: f32,
    pub source: SemanticEvidenceSource,
    #[serde(default)]
    pub evidence: Vec<SemanticEvidence>,
    #[serde(default)]
    pub confirmed_by_user: bool,
    #[serde(default)]
    pub locked: bool,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticNodeAnnotation {
    pub node_id: String,
    pub object_path: Option<String>,
    pub label: String,
    #[serde(default)]
    pub assignments: Vec<SemanticTagAssignment>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticProposal {
    pub id: String,
    pub node_id: String,
    pub tag_id: String,
    pub confidence: f32,
    pub reason: String,
    pub source: SemanticEvidenceSource,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticRelation {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub kind: ProjectGraphEdgeKind,
    pub label: String,
    pub confidence: f32,
    pub source: SemanticEvidenceSource,
    pub confirmed_by_user: bool,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticIndexState {
    #[serde(default = "semantic_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub project_key: String,
    #[serde(default)]
    pub graph_fingerprint: String,
    #[serde(default)]
    pub nodes: Vec<SemanticNodeAnnotation>,
    #[serde(default)]
    pub proposals: Vec<SemanticProposal>,
    #[serde(default)]
    pub relations: Vec<SemanticRelation>,
    #[serde(default)]
    pub updated_at: u64,
}

impl Default for SemanticIndexState {
    fn default() -> Self {
        Self {
            schema_version: semantic_schema_version(),
            project_key: String::new(),
            graph_fingerprint: String::new(),
            nodes: Vec::new(),
            proposals: Vec::new(),
            relations: Vec::new(),
            updated_at: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticAnalysisReport {
    pub project_key: String,
    pub graph_fingerprint: String,
    pub analyzed_nodes: usize,
    pub annotated_nodes: usize,
    pub confirmed_assignments: usize,
    pub automatic_assignments: usize,
    pub pending_proposals: usize,
    pub changed: bool,
    pub duration_ms: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticCandidateScore {
    pub node_id: String,
    pub score: f32,
    pub direct_match: bool,
    pub relation_match: bool,
    pub tag_ids: Vec<String>,
    pub tag_labels: Vec<String>,
    pub reasons: Vec<String>,
    pub relation_labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AnalyzeProjectSemanticsArgs {
    #[serde(default)]
    pub force: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SemanticCatalogSnapshotArgs {
    #[serde(default)]
    pub group: Option<SemanticTagGroup>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SemanticNodeSnapshotArgs {
    pub node_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProposeSemanticLabelsArgs {
    pub node_id: String,
    pub tag_ids: Vec<String>,
    pub reason: String,
    #[serde(default = "default_ai_confidence")]
    pub confidence: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DecideSemanticProposalsArgs {
    pub proposal_ids: Vec<String>,
    pub accept: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateSemanticLabelsArgs {
    pub node_id: String,
    #[serde(default)]
    pub add_tag_ids: Vec<String>,
    #[serde(default)]
    pub remove_tag_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResolveSemanticTargetsArgs {
    pub operation_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_semantic_limit")]
    pub limit: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ExportSemanticIndexArgs {}

fn default_ai_confidence() -> f32 {
    0.65
}

fn default_semantic_limit() -> usize {
    20
}

fn semantic_schema_version() -> u32 {
    1
}
