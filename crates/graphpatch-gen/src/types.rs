//! GraphPatch 类型定义——与前端 contracts.ts 中的 PluginGraphPatch + GraphPatchOperation 对齐。

use serde::{Deserialize, Serialize};

/// PluginGraphPatch 的 API 版本。
pub const GRAPH_PATCH_API_VERSION: &str = "researchcanvas.dev/graph-patch/v1alpha1";

// ── GraphPatch 操作类型 ──

/// 单个 GraphPatch 操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum GraphPatchOp {
    /// 添加节点。
    AddNode { node: GraphNodeData },
    /// 添加边。
    AddEdge { edge: GraphEdgeData },
    /// 修改节点。
    UpdateNode {
        node_id: String,
        changes: serde_json::Value,
    },
    /// 修改边。
    UpdateEdge {
        edge_id: String,
        changes: serde_json::Value,
    },
    /// 添加证据记录。
    AddEvidence { evidence: EvidenceData },
}

/// 图节点数据（对应 ResearchNode）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeData {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub data: serde_json::Value,
    /// 溯源信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceData>,
}

/// 图边数据（对应 ResearchEdge）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeData {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub data: serde_json::Value,
    /// 极性。
    #[serde(default)]
    pub polarity: Option<String>,
    /// 置信度。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// 实验数据（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment: Option<ExperimentData>,
    /// 溯源信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceData>,
}

/// 证据记录数据（对应 EvidenceRecord）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceData {
    pub id: String,
    pub source_type: String,
    pub source_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<EvidenceLocatorData>,
}

/// 证据定位器。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceLocatorData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<usize>,
}

/// 实验数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentData {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    #[serde(default = "default_outcome")]
    pub outcome: String,
    #[serde(default = "default_experiment_status")]
    pub status: String,
}

fn default_outcome() -> String { "neutral".into() }
fn default_experiment_status() -> String { "completed".into() }

/// 溯源数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceData {
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,
}

// ── PluginGraphPatch ──

/// 可移植、需审阅的图谱同步协议——与前端 contracts.ts 完全对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginGraphPatch {
    pub api_version: String,
    pub source: PatchSource,
    pub title: String,
    pub summary: String,
    pub review_required: bool,
    pub operations: Vec<GraphPatchOp>,
}

/// 补丁来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSource {
    pub plugin_id: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

// ── 预演结果 ──

/// plan_patch 预演结果：展示影响范围而不修改项目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPreview {
    /// 基础文件哈希（乐观并发控制）。
    pub base_file_hash: Option<String>,
    /// 新增节点数。
    pub nodes_added: usize,
    /// 新增边数。
    pub edges_added: usize,
    /// 新增证据记录数。
    pub evidence_added: usize,
    /// 将受到影响的现有节点 ID。
    pub affected_node_ids: Vec<String>,
    /// 将受到影响的现有边 ID。
    pub affected_edge_ids: Vec<String>,
    /// 潜在冲突提示。
    pub warnings: Vec<String>,
    /// 预演是否成功。
    pub valid: bool,
}
