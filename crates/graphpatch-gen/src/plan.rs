//! GraphPatch 预演——调用 research-graph-compiler 预演影响范围而不修改项目。
//!
//! plan_patch 流程：
//! 1. 验证 base_file_hash（乐观并发控制）
//! 2. 虚拟应用补丁操作 → 计算 blockHash / contentRootHash
//! 3. 报告影响范围（哪些现有节点/边会受到牵连）
//! 4. 返回 PatchPreview 供用户审阅
//!
//! 审阅通过后，由 workspace_host 的 git diff 管线执行 apply_patch。

use crate::ids::entity_node_id;
use crate::mapper::build_temp_id_map_from_candidates;
use crate::types::*;
use semantic_pipeline::validation::validate_candidates;
use semantic_pipeline::ir::AgentCandidates;
use std::collections::HashSet;

/// 对 AgentCandidates 进行预演：
/// 1. 运行 Pass F 验证
/// 2. 构建 GraphPatch 时收集影响范围
/// 3. 生成预演报告
pub fn preview_patch(
    candidates: &AgentCandidates,
    full_text: &str,
    base_file_hash: Option<&str>,
    existing_node_ids: &HashSet<String>,
    existing_edge_ids: &HashSet<String>,
) -> PatchPreview {
    let _ = existing_edge_ids; // 保留接口兼容性
    // 1. 验证
    let validation = validate_candidates(candidates, full_text);

    let mut warnings: Vec<String> = Vec::new();
    let mut nodes_added = 0usize;
    let mut edges_added = 0usize;
    let mut evidence_added = 0usize;
    let mut affected_node_ids: Vec<String> = Vec::new();
    let affected_edge_ids: Vec<String> = Vec::new();

    // 收集验证警告
    for check in &validation.checks {
        if !check.passed {
            warnings.push(format!("[{}] {check:?}", check.category));
        }
    }

    // 2. 统计新增数量
    let doi = candidates.doi.as_deref().unwrap_or("unknown");
    // 与 mapper 同一份合并解析:canonical 未知时合并不生效,实体照常计数。
    // 预演的 ID 必须与写入路径完全一致,否则冲突检测永远对不上实际写入。
    let mut temp_id_map = build_temp_id_map_from_candidates(candidates, doi);
    let mut merged_resolved: HashSet<String> = HashSet::new();
    for group in &candidates.merge_groups {
        if let Some(canonical_perm) = temp_id_map.get(&group.canonical_temp_id).cloned() {
            for tid in &group.merged_temp_ids {
                temp_id_map.insert(tid.clone(), canonical_perm.clone());
                merged_resolved.insert(tid.clone());
            }
        }
    }

    for entity in &candidates.entities {
        if merged_resolved.contains(&entity.temp_id) {
            continue;
        }
        let perm_id = temp_id_map
            .get(&entity.temp_id)
            .cloned()
            .unwrap_or_else(|| entity_node_id(&entity.label, &entity.text, doi));

        // 检查是否与现有节点冲突
        if existing_node_ids.contains(&perm_id) {
            affected_node_ids.push(perm_id.clone());
            warnings.push(format!(
                "节点 {} ({}) 可能与现有节点 ID 冲突（hash 碰撞）",
                entity.label, entity.temp_id
            ));
        }
        nodes_added += 1;
    }

    // 新变量
    for var in &candidates.variable_registry {
        if var.is_new {
            nodes_added += 1;
        }
    }

    // 实验节点
    nodes_added += candidates.experiment_matrix.len();

    // 边
    for bundle in &candidates.claim_evidence_bundles {
        edges_added += bundle.evidence_temp_ids.len();
    }
    for ie in &candidates.interaction_effects {
        if ie.variables.len() >= 2 {
            edges_added += 1;
        }
    }
    for _ic in &candidates.internal_conflicts {
        edges_added += 1;
    }
    for exp in &candidates.experiment_matrix {
        edges_added += exp.ivs.len();       // IV → Experiment
        edges_added += exp.dvs.len();       // Experiment → DV
        edges_added += exp.controls.len();  // Control → Experiment
        edges_added += exp.moderators.iter().filter(|m| m.interaction_with.is_some()).count();
        edges_added += exp.mediators.len();
    }

    // 证据(evidence 实体以 add-node 形式写入,此处仅统计构成)
    for entity in &candidates.entities {
        if entity.kind == semantic_pipeline::ir::EntityKind::Evidence
            && !merged_resolved.contains(&entity.temp_id)
        {
            evidence_added += 1;
        }
    }

    // 3. 验证结果添加
    if !validation.passed {
        warnings.push(format!("Pass F 验证未完全通过: {}", validation.summary));
    }

    // 4. 检查必要的警告
    if candidates.title.is_none() {
        warnings.push("论文标题缺失".into());
    }
    if candidates.entities.is_empty() {
        warnings.push("未提取到任何实体，补丁将为空".into());
    }

    // valid 直接取验证结果:旧实现按 "[error]" 前缀计数,但警告实际以
    // 类别为前缀,导致任何 Error 级违规都无法使预览失效(valid 恒 true)。
    let valid = validation.passed;

    PatchPreview {
        base_file_hash: base_file_hash.map(String::from),
        nodes_added,
        edges_added,
        evidence_added,
        affected_node_ids,
        affected_edge_ids,
        warnings,
        valid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semantic_pipeline::ir::*;

    fn test_candidates() -> AgentCandidates {
        AgentCandidates {
            paper_id: "test".into(),
            title: Some("Test".into()),
            authors: vec!["Author".into()],
            year: Some(2024),
            doi: Some("10.0/test".into()),
            entities: vec![ExtractedEntity {
                temp_id: "cl1".into(),
                kind: EntityKind::Claim,
                label: "Test claim".into(),
                text: "A claim".into(),
                confidence: 0.9,
                anchors: vec![],
                attributes: Default::default(),
            }],
            variable_registry: vec![],
            experiment_matrix: vec![],
            merge_groups: vec![],
            claim_evidence_bundles: vec![],
            metric_alignment: vec![],
            dataset_registry: vec![],
            main_conclusions: vec![],
            ablation_analysis: vec![],
            interaction_effects: vec![],
            confounders: vec![],
            missing_controls: vec![],
            internal_conflicts: vec![],
            synthesis_summary: None,
        }
    }

    #[test]
    fn preview_reports_correct_counts() {
        let candidates = test_candidates();
        let existing_nodes: HashSet<String> = HashSet::new();
        let existing_edges: HashSet<String> = HashSet::new();
        let preview = preview_patch(&candidates, "", None, &existing_nodes, &existing_edges);

        assert_eq!(preview.nodes_added, 1);
        assert_eq!(preview.edges_added, 0);
        assert_eq!(preview.evidence_added, 0);
        assert!(preview.valid);
    }

    #[test]
    fn preview_detects_id_conflict() {
        let candidates = test_candidates();
        let doi = candidates.doi.as_deref().unwrap_or("unknown");
        // 与写入路径同一公式:label + text 都参与哈希。
        let perm_id = entity_node_id("Test claim", "A claim", doi);

        let mut existing_nodes: HashSet<String> = HashSet::new();
        existing_nodes.insert(perm_id);

        let existing_edges: HashSet<String> = HashSet::new();
        let preview = preview_patch(&candidates, "", None, &existing_nodes, &existing_edges);

        assert_eq!(preview.affected_node_ids.len(), 1);
        assert!(!preview.warnings.is_empty());
    }
}
