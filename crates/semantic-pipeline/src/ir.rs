//! 语义提取管线的中间表示（Intermediate Representation）类型。
//!
//! 所有 Pass 的输入输出结构体定义。

use serde::{Deserialize, Serialize};

// ── Pass A: 文档结构提取 ──

/// Pass A 输出：结构化文档元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureExtraction {
    pub title: Option<String>,
    pub authors: Vec<String>,
    /// 论文自身的发表年份(不是参考文献的年份)。
    #[serde(default)]
    pub year: Option<u32>,
    pub abstract_text: Option<String>,
    pub sections: Vec<SectionInfo>,
    pub references: Vec<ReferenceInfo>,
    pub meta: ExtractionMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionInfo {
    pub id: String,
    pub title: String,
    pub level: u8,
    pub summary: String,
    pub start_anchor: Option<AnchorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceInfo {
    pub ref_id: String,
    pub raw: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionMeta {
    pub language: Option<String>,
    pub total_pages: Option<u32>,
}

// ── Pass B: 局部实体提取 ──

/// Pass B 输出：提取的研究实体集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityExtraction {
    pub entities: Vec<ExtractedEntity>,
    pub meta: EntityExtractionMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityExtractionMeta {
    pub section_coverage: Vec<String>,
    pub total_entities: usize,
}

/// 提取的研究实体（所有八种类型统一表示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedEntity {
    /// 临时 ID（Pass B 分配，后续 Pass 可能被合并）。
    /// 格式：q1, h2, cl3, m4, exp5, r6, ev7, v8
    pub temp_id: String,
    /// 实体类别。
    pub kind: EntityKind,
    /// 简短标签（≤60 字符）。
    pub label: String,
    /// 实体在原文中的精确描述或引文。
    pub text: String,
    /// 提取置信度（0.0–1.0）。
    pub confidence: f64,
    /// 原文锚点列表。
    pub anchors: Vec<AnchorRef>,
    /// 实体特定属性（按 kind 选择性填充）。
    pub attributes: EntityAttributes,
}

/// 实体类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Question,
    Hypothesis,
    Claim,
    Method,
    Experiment,
    Result,
    Evidence,
    Variable,
}

impl EntityKind {
    /// 返回 tempId 前缀。
    pub fn prefix(&self) -> &'static str {
        match self {
            EntityKind::Question => "q",
            EntityKind::Hypothesis => "h",
            EntityKind::Claim => "cl",
            EntityKind::Method => "m",
            EntityKind::Experiment => "exp",
            EntityKind::Result => "r",
            EntityKind::Evidence => "ev",
            EntityKind::Variable => "v",
        }
    }

    /// 从前缀解析 EntityKind。
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "q" => Some(EntityKind::Question),
            "h" => Some(EntityKind::Hypothesis),
            "cl" => Some(EntityKind::Claim),
            "m" => Some(EntityKind::Method),
            "exp" => Some(EntityKind::Experiment),
            "r" => Some(EntityKind::Result),
            "ev" => Some(EntityKind::Evidence),
            "v" => Some(EntityKind::Variable),
            _ => None,
        }
    }
}

/// 实体特定属性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityAttributes {
    /// Claim 子类型：finding / assumption / definition。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_type: Option<String>,
    /// Variable 子类型：independent / dependent / controlled / measured / derived。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_type: Option<String>,
    /// Method/Experiment 的方法论描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub methodology: Option<String>,
    /// Experiment 的样本量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<f64>,
    /// Result 的 p 值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_value: Option<f64>,
    /// Result 的效应量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_size: Option<f64>,
}

// ── 锚点引用（所有 Pass 共用） ──

/// PDF 中的精确位置锚点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorRef {
    pub section_id: String,
    pub paragraph_id: String,
    pub start_offset: usize,
    pub end_offset: usize,
    /// ≤200 字符的原文片段。
    pub quote: String,
}

// ── Pass C: 变量裂变与实验矩阵 ──

/// Pass C 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableFissionResult {
    pub experiment_matrix: Vec<ExperimentMatrixEntry>,
    pub variable_registry: Vec<VariableRegistryEntry>,
}

/// 单个实验的变量矩阵。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentMatrixEntry {
    /// 对应 Pass B 中的实验 tempId。
    pub experiment_temp_id: String,
    /// 实验设计类型。
    pub design: Option<String>,
    /// 自变量列表。
    pub ivs: Vec<VariableRoleEntry>,
    /// 因变量列表。
    pub dvs: Vec<VariableRoleEntry>,
    /// 控制变量列表。
    pub controls: Vec<ControlEntry>,
    /// 调节变量列表。
    pub moderators: Vec<ModeratorEntry>,
    /// 中介变量列表。
    pub mediators: Vec<MediatorEntry>,
    /// 样本信息。
    pub sample: Option<SampleInfo>,
    /// 实验条件。
    pub conditions: Vec<ConditionEntry>,
}

/// 变量角色条目（IV/DV）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableRoleEntry {
    pub variable_temp_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurement: Option<String>,
    pub domain: VariableDomain,
}

/// 控制变量条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlEntry {
    pub variable_temp_id: String,
    pub name: String,
    pub held_at: Option<String>,
}

/// 调节变量条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeratorEntry {
    pub variable_temp_id: String,
    pub name: String,
    pub interaction_with: Option<String>,
}

/// 中介变量条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediatorEntry {
    pub variable_temp_id: String,
    pub name: String,
    pub pathway: Option<String>,
}

/// 变量值域定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableDomain {
    /// categorical | continuous | discrete
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// 样本信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleInfo {
    pub size: Option<f64>,
    pub description: Option<String>,
}

/// 实验条件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionEntry {
    pub label: String,
    pub iv_settings: serde_json::Value,
}

/// 变量注册表条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableRegistryEntry {
    pub temp_id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub domain: VariableDomain,
    /// independent | dependent | controlled | moderator | mediator
    pub role: String,
    /// 测量方式：问卷 / 仪器 / 观察 / 计算
    pub measured_as: Option<String>,
    /// 是否为 Pass C 新增（Pass B 中不存在）。
    #[serde(default)]
    pub is_new: bool,
}

// ── Pass D: 跨段合并 ──

/// Pass D 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossSegmentMergeResult {
    pub merge_groups: Vec<MergeGroup>,
    pub claim_evidence_bundles: Vec<ClaimEvidenceBundle>,
    pub metric_alignment: Vec<MetricAlignment>,
    pub dataset_registry: Vec<DatasetRegistryEntry>,
}

/// 合并组：将被合并的实体归入一个规范实体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeGroup {
    pub canonical_temp_id: String,
    pub canonical_name: String,
    pub canonical_description: String,
    pub merged_temp_ids: Vec<String>,
    /// same_entity | alias | synonym | abbreviation
    pub reason: String,
    pub confidence: f64,
}

/// 主张-证据束。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimEvidenceBundle {
    pub claim_temp_id: String,
    pub summary: String,
    pub evidence_temp_ids: Vec<String>,
    /// strong | moderate | weak
    pub strength: String,
    pub strength_rationale: String,
}

/// 指标名对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricAlignment {
    pub canonical_metric: String,
    pub aliases: Vec<String>,
    pub unit: Option<String>,
}

/// 数据集注册表条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetRegistryEntry {
    pub canonical_name: String,
    pub aliases: Vec<String>,
    pub description: Option<String>,
}

// ── Pass E: 论文级综合 ──

/// Pass E 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperSynthesisResult {
    pub main_conclusions: Vec<MainConclusion>,
    pub ablation_analysis: Vec<AblationAnalysis>,
    pub interaction_effects: Vec<InteractionEffect>,
    pub confounders: Vec<Confounder>,
    pub missing_controls: Vec<MissingControl>,
    pub internal_conflicts: Vec<InternalConflict>,
    pub synthesis_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainConclusion {
    pub temp_id: String,
    pub statement: String,
    pub supported_by: Vec<String>,
    pub confidence: f64,
    /// primary | secondary | exploratory
    pub conclusion_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AblationAnalysis {
    pub experiment_temp_id: String,
    /// component | hyperparameter | data | architecture
    pub ablation_type: String,
    pub target_component: String,
    pub finding: String,
    /// critical | moderate | minor
    pub impact_assessment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionEffect {
    pub variables: Vec<String>,
    pub claim_temp_id: String,
    pub effect_description: String,
    /// synergistic | antagonistic | conditional
    pub nature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Confounder {
    pub variable_temp_id: String,
    /// high | medium | low
    pub risk_level: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingControl {
    pub description: String,
    pub recommended_control: String,
    pub affects_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalConflict {
    pub claim_a: String,
    pub claim_b: String,
    pub conflict_description: String,
    /// possible | unresolved
    pub resolution: String,
    pub resolution_note: String,
}

// ── AgentCandidate（最终合并表示） ──

/// Pass D 合并后、Pass E 综合后的统一 AgentCandidate 表示。
/// 这是 GraphPatch 构建的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCandidates {
    pub paper_id: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub entities: Vec<ExtractedEntity>,
    pub variable_registry: Vec<VariableRegistryEntry>,
    pub experiment_matrix: Vec<ExperimentMatrixEntry>,
    pub merge_groups: Vec<MergeGroup>,
    pub claim_evidence_bundles: Vec<ClaimEvidenceBundle>,
    pub metric_alignment: Vec<MetricAlignment>,
    pub dataset_registry: Vec<DatasetRegistryEntry>,
    pub main_conclusions: Vec<MainConclusion>,
    pub ablation_analysis: Vec<AblationAnalysis>,
    pub interaction_effects: Vec<InteractionEffect>,
    pub confounders: Vec<Confounder>,
    pub missing_controls: Vec<MissingControl>,
    pub internal_conflicts: Vec<InternalConflict>,
    pub synthesis_summary: Option<String>,
}
