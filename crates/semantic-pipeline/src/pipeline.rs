//! 多阶段提取管线——编排 Pass A–F 的执行流程。
//!
//! 管线设计（各 Pass 依赖关系）：
//! - Pass A 和 Pass B 可并行执行（都依赖原始文本）。
//! - Pass C 依赖 Pass B 的实体列表。
//! - Pass D 依赖 Pass B + Pass C 的结果。
//! - Pass E 依赖所有前置 Pass 的结果。
//! - Pass F 是纯本地验证，不依赖 LLM，在所有 Pass 之后运行。

use crate::config::PipelineConfig;
use crate::error::PipelineError;
use crate::ir::*;
use crate::validation::{validate_candidates, ValidationReport};
use std::collections::HashMap;

/// 管线执行器。
pub struct Pipeline {
    pub config: PipelineConfig,
}

/// 管线完整产物。
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// Pass A 输出。
    pub structure: Option<StructureExtraction>,
    /// Pass B 输出。
    pub entities: Option<EntityExtraction>,
    /// Pass C 输出。
    pub variable_fission: Option<VariableFissionResult>,
    /// Pass D 输出。
    pub merge_result: Option<CrossSegmentMergeResult>,
    /// Pass E 输出。
    pub synthesis: Option<PaperSynthesisResult>,
    /// 最终合并的 AgentCandidates。
    pub candidates: Option<AgentCandidates>,
    /// Pass F 验证报告。
    pub validation: Option<ValidationReport>,
    /// 各阶段的部分失败记录。
    pub phase_failures: Vec<PhaseFailure>,
}

/// 阶段失败记录。
#[derive(Debug, Clone)]
pub struct PhaseFailure {
    pub pass: String,
    pub error: String,
    pub retried: bool,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }

    // ── LLM 调用抽象（由外部注入） ──

    /// 调用 LLM 执行单个 Pass 的提取。
    ///
    /// 实际项目中由 reqwest → OpenAI/Claude/本地模型 完成；
    /// 此处提供接口定义，具体实现由 `llm_provider` 参数注入。
    pub async fn call_llm(
        &self,
        pass_name: &str,
        template_vars: &HashMap<String, String>,
        llm_provider: &dyn LlmProvider,
    ) -> Result<String, PipelineError> {
        let locale = &self.config.locale;
        let system = self
            .config
            .system_prompt(pass_name, locale)
            .ok_or_else(|| PipelineError::Template(format!("system prompt not found: {pass_name}/{locale}")))?;
        let user = self
            .config
            .render_user_template(pass_name, locale, template_vars)?;

        llm_provider
            .chat(system, &user, ResponseFormat::Json)
            .await
            .map_err(|e| PipelineError::PhaseFailed {
                pass: "?",
                reason: e,
                retryable: true,
            })
    }

    // ── Pass A: 文档结构提取 ──

    pub fn prepare_pass_a_input(
        &self,
        full_text: &str,
        document_structure_json: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("document_structure".into(), document_structure_json.into());
        vars.insert("full_text".into(), full_text.into());
        vars
    }

    pub fn parse_pass_a_output(json: &str) -> Result<StructureExtraction, PipelineError> {
        serde_json::from_str(json).map_err(|e| PipelineError::JsonParse {
            pass: "A",
            raw_output: json.to_string(),
            error: e.to_string(),
        })
    }

    // ── Pass B: 局部实体提取 ──

    pub fn prepare_pass_b_input(
        &self,
        document_structure_json: &str,
        section_title: &str,
        section_text: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("document_structure".into(), document_structure_json.into());
        vars.insert("section_title".into(), section_title.into());
        vars.insert("section_text".into(), section_text.into());
        vars
    }

    pub fn parse_pass_b_output(json: &str) -> Result<EntityExtraction, PipelineError> {
        serde_json::from_str(json).map_err(|e| PipelineError::JsonParse {
            pass: "B",
            raw_output: json.to_string(),
            error: e.to_string(),
        })
    }

    // ── Pass C: 变量裂变与实验矩阵 ──

    pub fn prepare_pass_c_input(
        &self,
        pass_b_entities_json: &str,
        experiment_paragraphs: &str,
        full_text: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("pass_b_entities_json".into(), pass_b_entities_json.into());
        vars.insert("experiment_paragraphs".into(), experiment_paragraphs.into());
        vars.insert("full_text".into(), full_text.into());
        vars
    }

    pub fn parse_pass_c_output(json: &str) -> Result<VariableFissionResult, PipelineError> {
        serde_json::from_str(json).map_err(|e| PipelineError::JsonParse {
            pass: "C",
            raw_output: json.to_string(),
            error: e.to_string(),
        })
    }

    // ── Pass D: 跨段合并 ──

    pub fn prepare_pass_d_input(
        &self,
        pass_b_all_entities_json: &str,
        pass_c_variable_registry_json: &str,
        full_text: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("pass_b_all_entities_json".into(), pass_b_all_entities_json.into());
        vars.insert("pass_c_variable_registry_json".into(), pass_c_variable_registry_json.into());
        vars.insert("full_text".into(), full_text.into());
        vars
    }

    pub fn parse_pass_d_output(json: &str) -> Result<CrossSegmentMergeResult, PipelineError> {
        serde_json::from_str(json).map_err(|e| PipelineError::JsonParse {
            pass: "D",
            raw_output: json.to_string(),
            error: e.to_string(),
        })
    }

    // ── Pass E: 论文级综合 ──

    pub fn prepare_pass_e_input(
        &self,
        title: &str,
        authors: &str,
        pass_a_structure_json: &str,
        merged_entities_json: &str,
        pass_c_experiment_matrix_json: &str,
        pass_d_merge_json: &str,
        full_text: &str,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("title".into(), title.into());
        vars.insert("authors".into(), authors.into());
        vars.insert("pass_a_structure_json".into(), pass_a_structure_json.into());
        vars.insert("merged_entities_json".into(), merged_entities_json.into());
        vars.insert("pass_c_experiment_matrix_json".into(), pass_c_experiment_matrix_json.into());
        vars.insert("pass_d_merge_json".into(), pass_d_merge_json.into());
        vars.insert("full_text".into(), full_text.into());
        vars
    }

    pub fn parse_pass_e_output(json: &str) -> Result<PaperSynthesisResult, PipelineError> {
        serde_json::from_str(json).map_err(|e| PipelineError::JsonParse {
            pass: "E",
            raw_output: json.to_string(),
            error: e.to_string(),
        })
    }

    // ── AgentCandidates 构建 ──

    /// 将所有 Pass 的结果汇总为统一的 AgentCandidates。
    pub fn build_candidates(
        paper_id: &str,
        doi: Option<&str>,
        structure: Option<&StructureExtraction>,
        entities: Option<&EntityExtraction>,
        variable_fission: Option<&VariableFissionResult>,
        merge_result: Option<&CrossSegmentMergeResult>,
        synthesis: Option<&PaperSynthesisResult>,
    ) -> AgentCandidates {
        let title = structure.and_then(|s| s.title.clone());
        let authors = structure.map(|s| s.authors.clone()).unwrap_or_default();
        let year = structure.and_then(|s| {
            s.references.first().and_then(|r| r.year)
        });
        let entities_vec = entities.map(|e| e.entities.clone()).unwrap_or_default();
        let variable_registry = variable_fission
            .map(|v| v.variable_registry.clone())
            .unwrap_or_default();
        let experiment_matrix = variable_fission
            .map(|v| v.experiment_matrix.clone())
            .unwrap_or_default();
        let merge_groups = merge_result
            .map(|m| m.merge_groups.clone())
            .unwrap_or_default();
        let claim_evidence_bundles = merge_result
            .map(|m| m.claim_evidence_bundles.clone())
            .unwrap_or_default();
        let metric_alignment = merge_result
            .map(|m| m.metric_alignment.clone())
            .unwrap_or_default();
        let dataset_registry = merge_result
            .map(|m| m.dataset_registry.clone())
            .unwrap_or_default();
        let main_conclusions = synthesis
            .map(|s| s.main_conclusions.clone())
            .unwrap_or_default();
        let ablation_analysis = synthesis
            .map(|s| s.ablation_analysis.clone())
            .unwrap_or_default();
        let interaction_effects = synthesis
            .map(|s| s.interaction_effects.clone())
            .unwrap_or_default();
        let confounders = synthesis
            .map(|s| s.confounders.clone())
            .unwrap_or_default();
        let missing_controls = synthesis
            .map(|s| s.missing_controls.clone())
            .unwrap_or_default();
        let internal_conflicts = synthesis
            .map(|s| s.internal_conflicts.clone())
            .unwrap_or_default();
        let synthesis_summary = synthesis.map(|s| s.synthesis_summary.clone());

        AgentCandidates {
            paper_id: paper_id.to_string(),
            title,
            authors,
            year,
            doi: doi.map(String::from),
            entities: entities_vec,
            variable_registry,
            experiment_matrix,
            merge_groups,
            claim_evidence_bundles,
            metric_alignment,
            dataset_registry,
            main_conclusions,
            ablation_analysis,
            interaction_effects,
            confounders,
            missing_controls,
            internal_conflicts,
            synthesis_summary,
        }
    }

    /// 运行 Pass F 本地验证。
    pub fn run_validation(
        candidates: &AgentCandidates,
        full_text: &str,
    ) -> ValidationReport {
        validate_candidates(candidates, full_text)
    }
}

// ── LLM Provider 抽象接口 ──

/// LLM 响应格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    Text,
    Json,
}

/// LLM Provider 抽象 trait。
/// 实现者：OpenAI、Claude、本地模型等。
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// 发送 chat 请求，返回 LLM 原始响应文本。
    async fn chat(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        format: ResponseFormat,
    ) -> Result<String, String>;

    /// Provider 名称。
    fn name(&self) -> &str;

    /// 模型名称。
    fn model(&self) -> &str;
}
