//! AgentCandidate → GraphPatch 映射规则。
//!
//! 核心映射逻辑：将语义候选（Entity, Variable, Experiment, Relation…）转换为
//! NODE_TYPES / EDGE_TYPES 兼容的 GraphPatch 操作序列。

use semantic_pipeline::ir::*;
use crate::ids::{entity_node_id, edge_id, TempIdMap};
use crate::types::*;

// ── EntityKind → NodeType 映射表 ──

/// ClaimType 到 Research Node Type 的映射。
pub fn map_claim_type_to_node(claim_type: Option<&str>) -> &str {
    match claim_type.unwrap_or("finding") {
        "hypothesis" => "hypothesis",
        "finding" => "result",
        "assumption" => "concept",
        "definition" => "concept",
        _ => "note",
    }
}

/// EntityKind 到 Research Node Type 的映射。
pub fn map_entity_kind_to_node(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Question => "question",
        EntityKind::Hypothesis => "hypothesis",
        EntityKind::Claim => "result",  // Claim 映射到 result，claimType 在 data 中留存
        EntityKind::Method => "method",
        EntityKind::Experiment => "experiment",
        EntityKind::Result => "result",
        EntityKind::Evidence => "evidence",
        EntityKind::Variable => "variable",
    }
}

/// RelationType 到 Research Edge Type 的映射。
pub fn map_relation_type_to_edge(rt: &str) -> &str {
    match rt {
        "supports" => "supports",
        "contradicts" => "contradicts",
        "causes" => "causes",
        "measures" => "measures",
        "uses" => "uses",
        "derived_from" => "derived_from",
        "correlates" => "correlates",
        "depends_on" => "depends_on",
        "part_of" => "part_of",
        "controls" => "controls",
        "mediates" => "mediates",
        "moderates" => "moderates",
        _ => "supports",
    }
}

// ── 主映射函数 ──

/// 将 AgentCandidates 转换为 PluginGraphPatch。
///
/// 此函数按以下顺序生成操作：
/// 1. 所有 entities → add-node（排除被合并的）
/// 2. 所有 variable_registry 中的新变量 → add-node
/// 3. evidence → add-evidence
/// 4. 关系推导 → add-edge
/// 5. 交互效应 → add-edge
/// 6. 内部冲突 → add-edge
pub fn build_graph_patch(
    candidates: &AgentCandidates,
    plugin_id: &str,
) -> PluginGraphPatch {
    let doi = candidates.doi.as_deref().unwrap_or("unknown");
    let mut operations: Vec<GraphPatchOp> = Vec::new();

    // 构建 tempId → permanent ID 映射(含全部实体,包括将被合并的)
    let mut temp_id_map = build_temp_id_map_from_candidates(candidates, doi);

    // 合并解析:merged tempId → canonical 的永久 ID。
    // canonical 本身不在映射里(LLM 给出坏组合)时合并无效——实体照常发射,
    // 否则指向被抑制实体的边会变成悬挂边。
    let mut merged_resolved: std::collections::HashSet<String> = std::collections::HashSet::new();
    for group in &candidates.merge_groups {
        if let Some(canonical_perm) = temp_id_map.get(&group.canonical_temp_id).cloned() {
            for tid in &group.merged_temp_ids {
                temp_id_map.insert(tid.clone(), canonical_perm.clone());
                merged_resolved.insert(tid.clone());
            }
        }
    }

    // ── 1. 实体 → Node ──
    for entity in &candidates.entities {
        // 跳过已并入 canonical 的实体(其 tempId 已全部改指 canonical)
        if merged_resolved.contains(&entity.temp_id) {
            continue;
        }

        let node_type = map_entity_kind_to_node(entity.kind);
        let perm_id = temp_id_map
            .get(&entity.temp_id)
            .cloned()
            .unwrap_or_else(|| entity_node_id(&entity.label, &entity.text, doi));

        let mut node_data = serde_json::json!({
            "tempId": entity.temp_id,
            "kind": format!("{:?}", entity.kind).to_lowercase(),
            "confidence": entity.confidence,
            "anchorCount": entity.anchors.len(),
        });

        // 证据实体保留首个锚点的定位信息(原 AddEvidence 的 locator 随节点落地)
        if entity.kind == EntityKind::Evidence {
            if let Some(anchor) = entity.anchors.first() {
                node_data["locator"] = serde_json::json!({
                    "fileName": format!("DOI:{doi}"),
                    "section": anchor.section_id,
                    "quote": truncate_str(&anchor.quote, 200),
                    "startOffset": anchor.start_offset,
                    "endOffset": anchor.end_offset,
                });
            }
            node_data["sourceId"] = serde_json::Value::String(doi.to_string());
        }

        // 附加 attributes 中的子类型信息
        if let Some(ref ct) = entity.attributes.claim_type {
            node_data["claimType"] = serde_json::Value::String(ct.clone());
        }
        if let Some(ref vt) = entity.attributes.variable_type {
            node_data["variableType"] = serde_json::Value::String(vt.clone());
        }
        if let Some(ref method) = entity.attributes.methodology {
            node_data["methodology"] = serde_json::Value::String(method.clone());
        }
        if let Some(ss) = entity.attributes.sample_size {
            node_data["sampleSize"] = serde_json::json!(ss);
        }
        if let Some(pv) = entity.attributes.p_value {
            node_data["pValue"] = serde_json::json!(pv);
        }
        if let Some(es) = entity.attributes.effect_size {
            node_data["effectSize"] = serde_json::json!(es);
        }

        operations.push(GraphPatchOp::AddNode {
            node: GraphNodeData {
                id: perm_id,
                node_type: node_type.to_string(),
                title: entity.label.clone(),
                body: entity.text.clone(),
                tags: entity
                    .anchors
                    .iter()
                    .map(|a| a.section_id.clone())
                    .collect(),
                data: node_data,
                provenance: Some(ProvenanceData {
                    origin: "ai".into(),
                    model_id: Some("pdf-agent".into()),
                    prompt_version: Some("pass-b-v1".into()),
                    actor_id: Some(plugin_id.into()),
                }),
            },
        });
    }

    // ── 2. 新增变量 → Node ──
    for var in &candidates.variable_registry {
        if var.is_new {
            let perm_id = temp_id_map
                .get(&var.temp_id)
                .cloned()
                .unwrap_or_else(|| entity_node_id(&var.name, &var.role, doi));

            let var_data = serde_json::json!({
                "tempId": var.temp_id,
                "role": var.role,
                "aliases": var.aliases,
                "domain": {
                    "type": var.domain.r#type,
                    "unit": var.domain.unit,
                },
                "measuredAs": var.measured_as,
            });

            operations.push(GraphPatchOp::AddNode {
                node: GraphNodeData {
                    id: perm_id,
                    node_type: "variable".into(),
                    title: var.name.clone(),
                    body: format!("{:?}: {} ({})", var.role, var.name, var.aliases.join(", ")),
                    tags: vec![],
                    data: var_data,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-c-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }
    }

    // ── 3. 实验 → Node + IV/DV 关系边 ──
    for exp in &candidates.experiment_matrix {
        let exp_perm_id = temp_id_map
            .get(&exp.experiment_temp_id)
            .cloned()
            .unwrap_or_else(|| entity_node_id("experiment", &exp.experiment_temp_id, doi));

        // 实验节点
        operations.push(GraphPatchOp::AddNode {
            node: GraphNodeData {
                id: exp_perm_id.clone(),
                node_type: "experiment".into(),
                title: format!("Experiment {}", exp.experiment_temp_id),
                body: exp.design.clone().unwrap_or_default(),
                tags: vec![],
                data: serde_json::json!({
                    "tempId": exp.experiment_temp_id,
                    "design": exp.design,
                    "sampleSize": exp.sample.as_ref().and_then(|s| s.size),
                }),
                provenance: Some(ProvenanceData {
                    origin: "ai".into(),
                    model_id: Some("pdf-agent".into()),
                    prompt_version: Some("pass-c-v1".into()),
                    actor_id: Some(plugin_id.into()),
                }),
            },
        });

        // IV → Experiment (measures)
        for iv in &exp.ivs {
            let iv_perm_id = resolve_temp_id(&temp_id_map, &iv.variable_temp_id, &iv.name, doi);
            let rel_id = edge_id(&iv_perm_id, &exp_perm_id, "measures", doi);
            operations.push(GraphPatchOp::AddEdge {
                edge: GraphEdgeData {
                    id: rel_id,
                    source: iv_perm_id,
                    target: exp_perm_id.clone(),
                    edge_type: "measures".into(),
                    note: Some(format!("IV: {}", iv.name)),
                    data: serde_json::json!({ "role": "iv" }),
                    polarity: Some("positive".into()),
                    confidence: None,
                    experiment: None,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-c-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }

        // Experiment → DV (measures)
        for dv in &exp.dvs {
            let dv_perm_id = resolve_temp_id(&temp_id_map, &dv.variable_temp_id, &dv.name, doi);
            let rel_id = edge_id(&exp_perm_id, &dv_perm_id, "measures", doi);
            operations.push(GraphPatchOp::AddEdge {
                edge: GraphEdgeData {
                    id: rel_id,
                    source: exp_perm_id.clone(),
                    target: dv_perm_id,
                    edge_type: "measures".into(),
                    note: Some(format!("DV: {}", dv.name)),
                    data: serde_json::json!({ "role": "dv" }),
                    polarity: Some("positive".into()),
                    confidence: None,
                    experiment: None,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-c-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }

        // Controls (controls)
        for ctrl in &exp.controls {
            let ctrl_perm_id = resolve_temp_id(&temp_id_map, &ctrl.variable_temp_id, &ctrl.name, doi);
            let rel_id = edge_id(&ctrl_perm_id, &exp_perm_id, "controls", doi);
            operations.push(GraphPatchOp::AddEdge {
                edge: GraphEdgeData {
                    id: rel_id,
                    source: ctrl_perm_id,
                    target: exp_perm_id.clone(),
                    edge_type: "controls".into(),
                    note: ctrl.held_at.clone(),
                    data: serde_json::json!({ "role": "control" }),
                    polarity: Some("positive".into()),
                    confidence: None,
                    experiment: None,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-c-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }

        // Moderators (moderates)
        for modv in &exp.moderators {
            if let Some(ref iv_temp_id) = modv.interaction_with {
                let mod_perm_id = resolve_temp_id(&temp_id_map, &modv.variable_temp_id, &modv.name, doi);
                let iv_perm_id = resolve_temp_id(&temp_id_map, iv_temp_id, iv_temp_id, doi);
                let rel_id = edge_id(&mod_perm_id, &iv_perm_id, "moderates", doi);
                operations.push(GraphPatchOp::AddEdge {
                    edge: GraphEdgeData {
                        id: rel_id,
                        source: mod_perm_id,
                        target: iv_perm_id,
                        edge_type: "moderates".into(),
                        note: Some(format!("Moderator: {}", modv.name)),
                        data: serde_json::json!({ "role": "moderator" }),
                        polarity: Some("mixed".into()),
                        confidence: None,
                        experiment: None,
                        provenance: Some(ProvenanceData {
                            origin: "ai".into(),
                            model_id: Some("pdf-agent".into()),
                            prompt_version: Some("pass-c-v1".into()),
                            actor_id: Some(plugin_id.into()),
                        }),
                    },
                });
            }
        }

        // Mediators (mediates)
        for med in &exp.mediators {
            let med_perm_id = resolve_temp_id(&temp_id_map, &med.variable_temp_id, &med.name, doi);
            let rel_id = edge_id(&med_perm_id, &exp_perm_id, "mediates", doi);
            operations.push(GraphPatchOp::AddEdge {
                edge: GraphEdgeData {
                    id: rel_id,
                    source: med_perm_id,
                    target: exp_perm_id.clone(),
                    edge_type: "mediates".into(),
                    note: med.pathway.clone(),
                    data: serde_json::json!({ "role": "mediator" }),
                    polarity: Some("positive".into()),
                    confidence: None,
                    experiment: None,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-c-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }
    }

    // ── 4. Claim-Evidence 关系边 ──
    for bundle in &candidates.claim_evidence_bundles {
        let claim_perm_id = resolve_temp_id(&temp_id_map, &bundle.claim_temp_id, "claim", doi);
        for ev_tid in &bundle.evidence_temp_ids {
            let ev_perm_id = resolve_temp_id(&temp_id_map, ev_tid, "evidence", doi);
            let rel_id = edge_id(&ev_perm_id, &claim_perm_id, "supports", doi);
            operations.push(GraphPatchOp::AddEdge {
                edge: GraphEdgeData {
                    id: rel_id,
                    source: ev_perm_id,
                    target: claim_perm_id.clone(),
                    edge_type: "supports".into(),
                    note: Some(format!("strength={}", bundle.strength)),
                    data: serde_json::json!({
                        "bundleSummary": bundle.summary,
                        "strength": bundle.strength,
                    }),
                    polarity: Some("positive".into()),
                    confidence: Some(match bundle.strength.as_str() {
                        "strong" => 0.9,
                        "moderate" => 0.6,
                        _ => 0.3,
                    }),
                    experiment: None,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-d-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }
    }

    // ── 5. 交互效应 → Edge ──
    for ie in &candidates.interaction_effects {
        if ie.variables.len() >= 2 {
            let v1_perm = resolve_temp_id(&temp_id_map, &ie.variables[0], "var", doi);
            let v2_perm = resolve_temp_id(&temp_id_map, &ie.variables[1], "var", doi);
            let rel_id = edge_id(&v1_perm, &v2_perm, "correlates", doi);
            operations.push(GraphPatchOp::AddEdge {
                edge: GraphEdgeData {
                    id: rel_id,
                    source: v1_perm,
                    target: v2_perm,
                    edge_type: "correlates".into(),
                    note: Some(ie.effect_description.clone()),
                    data: serde_json::json!({
                        "nature": ie.nature,
                        "claimTempId": ie.claim_temp_id,
                    }),
                    polarity: Some(match ie.nature.as_str() {
                        "synergistic" => "positive",
                        "antagonistic" => "negative",
                        _ => "mixed",
                    }.into()),
                    confidence: None,
                    experiment: None,
                    provenance: Some(ProvenanceData {
                        origin: "ai".into(),
                        model_id: Some("pdf-agent".into()),
                        prompt_version: Some("pass-e-v1".into()),
                        actor_id: Some(plugin_id.into()),
                    }),
                },
            });
        }
    }

    // ── 6. 内部冲突 → Edge ──
    for ic in &candidates.internal_conflicts {
        let claim_a_perm = resolve_temp_id(&temp_id_map, &ic.claim_a, "claim", doi);
        let claim_b_perm = resolve_temp_id(&temp_id_map, &ic.claim_b, "claim", doi);
        let rel_id = edge_id(&claim_a_perm, &claim_b_perm, "contradicts", doi);
        operations.push(GraphPatchOp::AddEdge {
            edge: GraphEdgeData {
                id: rel_id,
                source: claim_a_perm,
                target: claim_b_perm,
                edge_type: "contradicts".into(),
                note: Some(ic.conflict_description.clone()),
                data: serde_json::json!({
                    "resolution": ic.resolution,
                    "resolutionNote": ic.resolution_note,
                }),
                polarity: Some("negative".into()),
                confidence: None,
                experiment: None,
                provenance: Some(ProvenanceData {
                    origin: "ai".into(),
                    model_id: Some("pdf-agent".into()),
                    prompt_version: Some("pass-e-v1".into()),
                    actor_id: Some(plugin_id.into()),
                }),
            },
        });
    }

    // 注:Evidence 实体只在第 1 段作为 type="evidence" 节点发射一次。
    // 前端 GraphPatch 契约只有 add-node/add-edge/update-node/update-edge 四种 op,
    // 额外的 add-evidence op 会让整个补丁在规范化时被判无效;同 id 双发射也必
    // 触发编译器 duplicate-id 不变量。证据定位信息已随节点 data.locator 落地。
    let ev_count = 0usize;

    // 构建补丁元数据
    let node_count = operations.iter().filter(|op| matches!(op, GraphPatchOp::AddNode { .. })).count();
    let edge_count = operations.iter().filter(|op| matches!(op, GraphPatchOp::AddEdge { .. })).count();

    PluginGraphPatch {
        api_version: GRAPH_PATCH_API_VERSION.to_string(),
        source: PatchSource {
            plugin_id: plugin_id.to_string(),
            operation: "extract-from-paper".to_string(),
            external_id: candidates.doi.clone(),
        },
        title: format!(
            "Extracted from: {}",
            candidates.title.as_deref().unwrap_or("Untitled")
        ),
        summary: format!(
            "{} nodes ({q} questions, {h} hypotheses, {cl} claims, {m} methods, \
             {exp} experiments, {r} results, {ev} evidence, {v} variables), \
             {edge_count} edges, {ev_count} evidence records",
            node_count,
            q = count_kind(&candidates.entities, EntityKind::Question),
            h = count_kind(&candidates.entities, EntityKind::Hypothesis),
            cl = count_kind(&candidates.entities, EntityKind::Claim),
            m = count_kind(&candidates.entities, EntityKind::Method),
            exp = count_kind(&candidates.entities, EntityKind::Experiment),
            r = count_kind(&candidates.entities, EntityKind::Result),
            ev = count_kind(&candidates.entities, EntityKind::Evidence),
            v = count_kind(&candidates.entities, EntityKind::Variable),
        ),
        review_required: true,
        operations,
    }
}

// ── 辅助函数 ──

/// 从 AgentCandidates 构建 tempId → permanent ID 映射。
pub(crate) fn build_temp_id_map_from_candidates(
    candidates: &AgentCandidates,
    doi: &str,
) -> TempIdMap {
    let mut map = TempIdMap::new();
    for entity in &candidates.entities {
        let perm_id = entity_node_id(&entity.label, &entity.text, doi);
        map.insert(entity.temp_id.clone(), perm_id);
    }
    for var in &candidates.variable_registry {
        if !map.contains_key(&var.temp_id) {
            let perm_id = entity_node_id(&var.name, &var.role, doi);
            map.insert(var.temp_id.clone(), perm_id);
        }
    }
    map
}

/// 从 tempId 映射表中解析永久 ID，若缺失则动态生成。
fn resolve_temp_id(map: &TempIdMap, temp_id: &str, fallback_label: &str, doi: &str) -> String {
    map.get(temp_id)
        .cloned()
        .unwrap_or_else(|| entity_node_id(temp_id, fallback_label, doi))
}

fn count_kind(entities: &[ExtractedEntity], kind: EntityKind) -> usize {
    entities.iter().filter(|e| e.kind == kind).count()
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len { s.to_string() } else {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_candidates() -> AgentCandidates {
        AgentCandidates {
            paper_id: "test".into(),
            title: Some("Test Paper".into()),
            authors: vec!["A. Author".into()],
            year: Some(2024),
            doi: Some("10.0000/test".into()),
            entities: vec![
                ExtractedEntity {
                    temp_id: "h1".into(),
                    kind: EntityKind::Hypothesis,
                    label: "Test Hypothesis".into(),
                    text: "A testable hypothesis".into(),
                    confidence: 0.9,
                    anchors: vec![AnchorRef {
                        section_id: "s1".into(),
                        paragraph_id: "p1".into(),
                        start_offset: 10,
                        end_offset: 50,
                        quote: "the hypothesis states...".into(),
                    }],
                    attributes: EntityAttributes::default(),
                },
                ExtractedEntity {
                    temp_id: "v1".into(),
                    kind: EntityKind::Variable,
                    label: "Learning Rate".into(),
                    text: "The learning rate parameter".into(),
                    confidence: 0.95,
                    anchors: vec![AnchorRef {
                        section_id: "s2".into(),
                        paragraph_id: "p3".into(),
                        start_offset: 100,
                        end_offset: 140,
                        quote: "learning rate was set to 0.001".into(),
                    }],
                    attributes: EntityAttributes {
                        variable_type: Some("independent".into()),
                        ..Default::default()
                    },
                },
                ExtractedEntity {
                    temp_id: "r1".into(),
                    kind: EntityKind::Result,
                    label: "Accuracy 95%".into(),
                    text: "The model achieved 95% accuracy".into(),
                    confidence: 0.98,
                    anchors: vec![AnchorRef {
                        section_id: "s3".into(),
                        paragraph_id: "p5".into(),
                        start_offset: 200,
                        end_offset: 240,
                        quote: "achieved 95% accuracy".into(),
                    }],
                    attributes: EntityAttributes {
                        claim_type: Some("finding".into()),
                        effect_size: Some(0.8),
                        ..Default::default()
                    },
                },
            ],
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
    fn builds_graph_patch_with_nodes_only() {
        let candidates = make_test_candidates();
        let patch = build_graph_patch(&candidates, "pdf-agent");

        assert_eq!(patch.api_version, GRAPH_PATCH_API_VERSION);
        assert!(patch.review_required);
        assert!(patch.operations.len() >= 3);

        let node_ops: Vec<_> = patch.operations.iter()
            .filter(|op| matches!(op, GraphPatchOp::AddNode { .. }))
            .collect();
        assert_eq!(node_ops.len(), 3);
    }

    #[test]
    fn merge_into_existing_canonical_excludes_merged_and_repoints_edges() {
        let mut candidates = make_test_candidates();
        // v1 并入 h1(canonical 真实存在)→ v1 不发射,其 tempId 改指 h1 的永久 ID。
        candidates.merge_groups.push(MergeGroup {
            canonical_temp_id: "h1".into(),
            canonical_name: "Test Hypothesis".into(),
            canonical_description: "desc".into(),
            merged_temp_ids: vec!["v1".into()],
            reason: "alias".into(),
            confidence: 0.9,
        });

        let patch = build_graph_patch(&candidates, "pdf-agent");
        let node_ops: Vec<_> = patch.operations.iter()
            .filter(|op| matches!(op, GraphPatchOp::AddNode { .. }))
            .collect();
        assert_eq!(node_ops.len(), 2, "merged v1 is absorbed into h1");
    }

    #[test]
    fn merge_with_unknown_canonical_keeps_entity_to_avoid_dangling_edges() {
        let mut candidates = make_test_candidates();
        // canonical "v10" 不在实体列表里(LLM 坏组合)→ 合并无效,v1 照常发射,
        // 否则引用 v1 的边会解析到一个从未发射的节点。
        candidates.merge_groups.push(MergeGroup {
            canonical_temp_id: "v10".into(),
            canonical_name: "Ghost Variable".into(),
            canonical_description: "desc".into(),
            merged_temp_ids: vec!["v1".into()],
            reason: "alias".into(),
            confidence: 0.9,
        });

        let patch = build_graph_patch(&candidates, "pdf-agent");
        let node_ops: Vec<_> = patch.operations.iter()
            .filter(|op| matches!(op, GraphPatchOp::AddNode { .. }))
            .collect();
        assert_eq!(node_ops.len(), 3, "unresolvable merge must not suppress the entity");
    }

    #[test]
    fn evidence_entities_emit_once_as_nodes_not_add_evidence() {
        let mut candidates = make_test_candidates();
        candidates.entities.push(ExtractedEntity {
            temp_id: "ev1".into(),
            kind: EntityKind::Evidence,
            label: "Supporting quote".into(),
            text: "the quoted passage".into(),
            confidence: 0.8,
            anchors: vec![AnchorRef {
                section_id: "s4".into(),
                paragraph_id: "p9".into(),
                start_offset: 300,
                end_offset: 340,
                quote: "supporting quote text".into(),
            }],
            attributes: EntityAttributes::default(),
        });

        let patch = build_graph_patch(&candidates, "pdf-agent");
        // 不再发射 add-evidence(前端契约只认四种 op,未知 op 使整包无效);
        // 证据以 evidence 节点形式出现且仅一次,定位信息随节点 data.locator 落地。
        assert!(
            !patch.operations.iter().any(|op| matches!(op, GraphPatchOp::AddEvidence { .. })),
            "add-evidence must not be emitted"
        );
        let evidence_nodes: Vec<_> = patch.operations.iter()
            .filter_map(|op| match op {
                GraphPatchOp::AddNode { node } if node.node_type == "evidence" => Some(node),
                _ => None,
            })
            .collect();
        assert_eq!(evidence_nodes.len(), 1, "evidence emits exactly one node");
        assert!(evidence_nodes[0].data["locator"]["quote"].is_string());
    }

    #[test]
    fn claim_type_mapping_is_correct() {
        assert_eq!(map_claim_type_to_node(Some("hypothesis")), "hypothesis");
        assert_eq!(map_claim_type_to_node(Some("finding")), "result");
        assert_eq!(map_claim_type_to_node(Some("assumption")), "concept");
        assert_eq!(map_claim_type_to_node(Some("definition")), "concept");
        assert_eq!(map_claim_type_to_node(None), "result");
    }

    #[test]
    fn entity_kind_mapping_is_correct() {
        assert_eq!(map_entity_kind_to_node(EntityKind::Question), "question");
        assert_eq!(map_entity_kind_to_node(EntityKind::Hypothesis), "hypothesis");
        assert_eq!(map_entity_kind_to_node(EntityKind::Method), "method");
        assert_eq!(map_entity_kind_to_node(EntityKind::Experiment), "experiment");
        assert_eq!(map_entity_kind_to_node(EntityKind::Variable), "variable");
        assert_eq!(map_entity_kind_to_node(EntityKind::Evidence), "evidence");
    }
}
