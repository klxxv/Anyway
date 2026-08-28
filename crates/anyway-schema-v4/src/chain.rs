//! Chain builder (handoff-spec.md §43, §44, §45).
//!
//! A chain is an ordered computational path `B0 -o1-> B1 -o2-> … -on-> Bn`.
//! The builder links operators along a **spine**: each operator's first input
//! is its spine input and its first output is its spine output, and two
//! operators chain when the first's spine output is the second's spine input.
//!
//! Invariants enforced (handoff-spec.md §44): CHAIN-001..CHAIN-006.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::hash::{chain_instance_hash, chain_semantic_hash};
use crate::ir::{Block, Chain, Operator};
use crate::validator::{ValidationError, ValidationReport};

/// Build every maximal chain over the compiled operator graph.
pub fn build_chains(blocks: &[Block], operators: &[Operator]) -> (Vec<Chain>, ValidationReport) {
    let mut report = ValidationReport::default();

    let block_semantic: HashMap<&str, &str> = blocks
        .iter()
        .map(|block| (block.id.as_str(), block.semantic_hash.as_str()))
        .collect();
    let block_ids: HashSet<&str> = blocks.iter().map(|block| block.id.as_str()).collect();
    let operator_by_id: HashMap<&str, &Operator> = operators
        .iter()
        .map(|operator| (operator.id.as_str(), operator))
        .collect();
    let operator_semantic: HashMap<&str, &str> = operators
        .iter()
        .map(|operator| (operator.id.as_str(), operator.semantic_hash.as_str()))
        .collect();

    // Successor map: op A -> op B when A's spine output is B's spine input.
    let mut successors: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots: Vec<String> = Vec::new();
    let mut has_predecessor: HashSet<String> = HashSet::new();

    for operator in operators {
        let Some(spine_output) = operator.output_refs.first() else {
            continue;
        };
        roots.push(operator.id.clone());
        for other in operators {
            let Some(spine_input) = other.input_refs.first() else {
                continue;
            };
            if spine_input == spine_output && other.id != operator.id {
                successors
                    .entry(operator.id.clone())
                    .or_default()
                    .push(other.id.clone());
                has_predecessor.insert(other.id.clone());
            }
        }
    }
    for successors_for_operator in successors.values_mut() {
        successors_for_operator.sort();
        successors_for_operator.dedup();
    }

    // Enumerate maximal simple paths from every operator with no predecessor
    // (true roots), so sub-chains are not emitted alongside their extension.
    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut ordered_roots: Vec<String> = roots
        .into_iter()
        .filter(|id| !has_predecessor.contains(id))
        .collect();
    ordered_roots.sort();
    ordered_roots.dedup();
    for root in ordered_roots {
        let mut path = vec![root.clone()];
        extend_path(&root, &mut path, &successors, &mut paths);
    }

    // Deduplicate by operator-path signature, keeping deterministic order.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut chains = Vec::new();
    let mut chain_index = 0usize;

    for path in paths {
        let signature = path.join("|");
        if !seen.insert(signature) {
            continue;
        }

        // CHAIN-004: every spine reference resolves to a block.
        let mut block_path = Vec::new();
        let mut context_ref: Option<String> = None;
        let mut axiom_set_ref: Option<String> = None;
        let mut evidence_refs: BTreeSet<String> = BTreeSet::new();
        let mut valid = true;

        for (index, operator_id) in path.iter().enumerate() {
            let Some(operator) = operator_by_id.get(operator_id.as_str()) else {
                report.errors.push(ValidationError {
                    code: "CHAIN-004".to_string(),
                    path: format!("$.chains.operator_path[{index}]"),
                    message: format!("unresolved operator reference: {operator_id}"),
                });
                valid = false;
                break;
            };

            let Some(spine_input) = operator.input_refs.first() else {
                valid = false;
                break;
            };
            let Some(spine_output) = operator.output_refs.first() else {
                valid = false;
                break;
            };

            if index == 0 {
                if !block_ids.contains(spine_input.as_str()) {
                    report.errors.push(ValidationError {
                        code: "CHAIN-004".to_string(),
                        path: format!("$.chains.block_path[0]"),
                        message: format!("unresolved block reference: {spine_input}"),
                    });
                    valid = false;
                    break;
                }
                block_path.push(spine_input.clone());
            }
            if !block_ids.contains(spine_output.as_str()) {
                report.errors.push(ValidationError {
                    code: "CHAIN-004".to_string(),
                    path: format!("$.chains.block_path[{}]", index + 1),
                    message: format!("unresolved block reference: {spine_output}"),
                });
                valid = false;
                break;
            }
            block_path.push(spine_output.clone());

            // CHAIN-005 / CHAIN-006: conditioning must be compatible across members.
            match (&context_ref, &operator.context_ref) {
                (Some(chain_ctx), Some(op_ctx)) if chain_ctx != op_ctx => {
                    report.errors.push(ValidationError {
                        code: "CHAIN-005".to_string(),
                        path: format!("$.chains.context_ref[{index}]"),
                        message: "chain context conflicts with a member operator context".to_string(),
                    });
                    valid = false;
                    break;
                }
                (None, Some(op_ctx)) => context_ref = Some(op_ctx.clone()),
                _ => {}
            }
            match (&axiom_set_ref, &operator.axiom_set_ref) {
                (Some(chain_ax), Some(op_ax)) if chain_ax != op_ax => {
                    report.errors.push(ValidationError {
                        code: "CHAIN-006".to_string(),
                        path: format!("$.chains.axiom_set_ref[{index}]"),
                        message: "chain axiom set conflicts with a member operator".to_string(),
                    });
                    valid = false;
                    break;
                }
                (None, Some(op_ax)) => axiom_set_ref = Some(op_ax.clone()),
                _ => {}
            }

            evidence_refs.extend(operator.evidence_refs.iter().cloned());
        }

        if !valid {
            continue;
        }

        // CHAIN-001: blocks = operators + 1.
        if block_path.len() != path.len() + 1 {
            report.errors.push(ValidationError {
                code: "CHAIN-001".to_string(),
                path: "$.chains".to_string(),
                message: format!(
                    "a chain must have one more block than operator ({} blocks vs {} operators)",
                    block_path.len(),
                    path.len()
                ),
            });
            continue;
        }

        let block_hashes: Vec<String> = block_path
            .iter()
            .map(|block_id| block_semantic.get(block_id.as_str()).copied().unwrap_or("").to_string())
            .collect();
        let operator_hashes: Vec<String> = path
            .iter()
            .map(|operator_id| {
                operator_semantic
                    .get(operator_id.as_str())
                    .copied()
                    .unwrap_or("")
                    .to_string()
            })
            .collect();

        let semantic_hash = chain_semantic_hash(&block_hashes, &operator_hashes);
        let evidence: Vec<String> = evidence_refs.into_iter().collect();
        let instance_hash = chain_instance_hash(&semantic_hash, &[], &evidence);

        chain_index += 1;
        chains.push(Chain {
            id: format!("chain_{chain_index:03}"),
            block_path,
            operator_path: path,
            context_ref,
            axiom_set_ref,
            source_experiment_refs: Vec::new(),
            semantic_hash,
            instance_hash,
        });
    }

    (chains, report)
}

fn extend_path(
    node: &str,
    path: &mut Vec<String>,
    successors: &HashMap<String, Vec<String>>,
    results: &mut Vec<Vec<String>>,
) {
    let Some(next_nodes) = successors.get(node) else {
        results.push(path.clone());
        return;
    };
    if next_nodes.is_empty() {
        results.push(path.clone());
        return;
    }
    let mut extended = false;
    for next in next_nodes {
        if path.contains(next) {
            continue;
        }
        extended = true;
        path.push(next.clone());
        extend_path(next, path, successors, results);
        path.pop();
    }
    if !extended {
        results.push(path.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OperatorKind;
    use serde_json::json;

    fn block(id: &str) -> Block {
        Block {
            id: id.to_string(),
            block_type: crate::ir::BlockType::Variable,
            concept_id: Some(id.to_string()),
            variable_ref: None,
            member_refs: vec![],
            context_ref: None,
            axiom_set_ref: None,
            semantic_hash: format!("sha256:{id}"),
            instance_hash: format!("sha256:{id}"),
        }
    }

    fn operator(id: &str, inputs: &[&str], outputs: &[&str], context: Option<&str>) -> Operator {
        Operator {
            id: id.to_string(),
            operator: OperatorKind::T,
            input_refs: inputs.iter().map(|s| s.to_string()).collect(),
            output_refs: outputs.iter().map(|s| s.to_string()).collect(),
            payload: json!({}),
            context_ref: context.map(str::to_string),
            axiom_set_ref: None,
            evidence_refs: vec![],
            semantic_hash: format!("sha256:{id}"),
            instance_hash: format!("sha256:{id}"),
        }
    }

    #[test]
    fn single_operator_is_a_minimal_chain() {
        let blocks = vec![block("A"), block("B")];
        let operators = vec![operator("o1", &["A"], &["B"], None)];
        let (chains, report) = build_chains(&blocks, &operators);
        assert!(report.ok());
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].block_path, vec!["A", "B"]);
        assert_eq!(chains[0].operator_path, vec!["o1"]);
        assert_eq!(chains[0].block_path.len(), chains[0].operator_path.len() + 1);
    }

    #[test]
    fn linked_operators_form_a_longer_chain() {
        let blocks = vec![block("A"), block("B"), block("C")];
        let operators = vec![
            operator("o1", &["A"], &["B"], None),
            operator("o2", &["B"], &["C"], None),
        ];
        let (chains, report) = build_chains(&blocks, &operators);
        assert!(report.ok());
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].block_path, vec!["A", "B", "C"]);
        assert_eq!(chains[0].operator_path, vec!["o1", "o2"]);
    }

    #[test]
    fn unlinked_operators_form_separate_chains() {
        let blocks = vec![block("A"), block("B"), block("C"), block("D")];
        let operators = vec![
            operator("o1", &["A"], &["B"], None),
            operator("o2", &["C"], &["D"], None),
        ];
        let (chains, report) = build_chains(&blocks, &operators);
        assert!(report.ok());
        assert_eq!(chains.len(), 2);
    }

    #[test]
    fn conflicting_operator_contexts_are_chain_005() {
        let blocks = vec![block("A"), block("B"), block("C")];
        let operators = vec![
            operator("o1", &["A"], &["B"], Some("ctx_1")),
            operator("o2", &["B"], &["C"], Some("ctx_2")),
        ];
        let (_, report) = build_chains(&blocks, &operators);
        assert!(report.errors.iter().any(|e| e.code == "CHAIN-005"));
    }
}
