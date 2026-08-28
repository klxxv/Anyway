//! 图编译器核心测试（迁移自原 `src-tauri/src/graph_compiler.rs` 的单元测试）。
//! 仅调用 crate 公共 API，保持原断言逐字不变。

use research_graph_compiler::*;
use serde_json::json;

fn sample_project() -> serde_json::Value {
    json!({
        "schemaVersion": 2,
        "id": "project-pinn",
        "title": "PINN architecture",
        "discipline": "Physics-informed neural networks",
        "updatedAt": "2026-08-01T00:00:00Z",
        "revision": 1,
        "nodes": [
            {
                "id": "n1",
                "type": "question",
                "title": "主问题",
                "body": "如何建模?",
                "tags": ["问题"],
                "status": "confirmed",
                "evidenceIds": ["e1"],
                "data": {"shape": "circle"},
                "provenance": {"origin": "human", "actorId": "researcher"},
                "createdAt": "2026-08-01T00:00:00Z",
                "updatedAt": "2026-08-01T00:00:00Z"
            },
            {
                "id": "n2",
                "type": "concept",
                "title": "先验约束",
                "body": "物理守恒",
                "tags": [],
                "status": "confirmed",
                "evidenceIds": [],
                "data": {},
                "provenance": {"origin": "human"},
                "createdAt": "2026-08-01T00:00:00Z",
                "updatedAt": "2026-08-01T00:00:00Z"
            }
        ],
        "edges": [
            {
                "id": "x1",
                "type": "supports",
                "source": "n1",
                "target": "n2",
                "directed": true,
                "polarity": "positive",
                "confidence": 0.9,
                "conditions": [],
                "evidenceIds": ["e1"],
                "note": "支持关系",
                "provenance": {"origin": "human"}
            }
        ],
        "evidence": [
            {
                "id": "e1",
                "sourceType": "paper",
                "sourceId": "paper-rope-2024",
                "title": "RoPE 论文",
                "authors": "Chen, Rao & Li",
                "year": 2024,
                "doi": "10.0000/example.rope",
                "url": "https://example.org/papers/rope",
                "locator": {"page": 7, "section": "4.2", "quote": "引文", "startOffset": 1, "endOffset": 2},
                "status": "verified",
                "provenance": {"origin": "human"}
            }
        ],
        "placements": [],
        "scenarios": [],
        "activity": []
    })
}

#[test]
fn canonicalize_sorts_object_keys_and_nested_data() {
    let a: serde_json::Value = serde_json::from_str(r#"{"z":1,"a":{"y":2,"x":3}}"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"{"a":{"x":3,"y":2},"z":1}"#).unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    assert_eq!(
        String::from_utf8(canonicalize(&a)).unwrap(),
        r#"{"a":{"x":3,"y":2},"z":1}"#
    );
}

#[test]
fn canonicalize_sorts_array_elements_by_canonical_bytes() {
    let a: serde_json::Value = serde_json::from_str(r#"[3,1,2]"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"[1,2,3]"#).unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    let objects: serde_json::Value = serde_json::from_str(r#"[{"b":1},{"a":2}]"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&objects)).unwrap(),
        r#"[{"a":2},{"b":1}]"#
    );
}

#[test]
fn canonicalize_normalizes_numbers() {
    let one_int: serde_json::Value = serde_json::from_str(r#"{"n":1}"#).unwrap();
    let one_float: serde_json::Value = serde_json::from_str(r#"{"n":1.0}"#).unwrap();
    assert_eq!(canonicalize(&one_int), canonicalize(&one_float));
    assert_eq!(
        String::from_utf8(canonicalize(&one_int)).unwrap(),
        r#"{"n":1}"#
    );
    let negative_zero: serde_json::Value = serde_json::from_str(r#"{"n":-0.0}"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&negative_zero)).unwrap(),
        r#"{"n":0}"#
    );
    let large_integer: serde_json::Value =
        serde_json::from_str(r#"{"n":9007199254740993}"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&large_integer)).unwrap(),
        r#"{"n":9007199254740993}"#
    );
    let fraction: serde_json::Value = serde_json::from_str(r#"{"n":3.5}"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&fraction)).unwrap(),
        r#"{"n":3.5}"#
    );
}

#[test]
fn canonicalize_applies_nfc_normalization() {
    let composed: serde_json::Value = serde_json::from_str(r#"{"text":"\u00e9"}"#).unwrap();
    let decomposed: serde_json::Value = serde_json::from_str(r#"{"text":"e\u0301"}"#).unwrap();
    assert_eq!(canonicalize(&composed), canonicalize(&decomposed));
    assert_eq!(
        String::from_utf8(canonicalize(&composed)).unwrap(),
        r#"{"text":"é"}"#
    );
}

#[test]
fn canonicalize_folds_whitespace() {
    let value: serde_json::Value = serde_json::from_str(r#"{"body":"  a\t\n b  c\r\n "}"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&value)).unwrap(),
        r#"{"body":"a b c"}"#
    );
}

#[test]
fn block_hash_is_12_hex_and_deterministic() {
    let claim = node_claim(&sample_project()["nodes"][0]);
    let hash = block_hash(&claim);
    assert_eq!(hash.len(), 12);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(block_hash(&claim), block_hash(&claim));
}

#[test]
fn block_hash_ignores_hanging_and_editorial_fields() {
    let with_metadata = json!({
        "id": "n1", "type": "question", "title": "Q", "body": "b",
        "tags": [], "data": {},
        "evidenceIds": ["e1"], "status": "confirmed",
        "provenance": {"origin": "human", "reviewedBy": "x"},
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-02-02T00:00:00Z"
    });
    let plain =
        json!({"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {}});
    assert_eq!(block_hash(&with_metadata), block_hash(&plain));

    // 嵌套 status（experiment.status）同样被剔除。
    let with_experiment_status = json!({
        "id": "x1", "type": "supports", "source": "n1", "target": "n2",
        "directed": true, "polarity": "positive", "conditions": [],
        "experiment": {"id": "exp", "label": "L", "metric": "acc", "outcome": "supports", "status": "completed"}
    });
    let without = json!({
        "id": "x1", "type": "supports", "source": "n1", "target": "n2",
        "directed": true, "polarity": "positive", "conditions": [],
        "experiment": {"id": "exp", "label": "L", "metric": "acc", "outcome": "supports"}
    });
    assert_eq!(block_hash(&with_experiment_status), block_hash(&without));
}

#[test]
fn block_hash_ignores_evidence_locator() {
    let with_locator = json!({
        "id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T",
        "locator": {"page": 3, "quote": "q", "startOffset": 1, "endOffset": 2, "section": "s"}
    });
    let plain = json!({"id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T"});
    assert_eq!(block_hash(&with_locator), block_hash(&plain));
}

#[test]
fn block_hash_changes_with_content_but_not_with_metadata() {
    let project = sample_project();
    let claim = node_claim(&project["nodes"][0]);
    let mut renamed = project.clone();
    renamed["nodes"][0]["title"] = json!("另一个问题");
    assert_ne!(
        block_hash(&claim),
        block_hash(&node_claim(&renamed["nodes"][0]))
    );
    // 键顺序无关：同一对象的字段重排后哈希不变。
    let mut reordered = json!({"id": "n1", "title": "Q", "body": "b", "tags": [], "data": {}});
    assert_eq!(block_hash(&reordered), block_hash(&reordered));
    // 语义相同、键序不同 → 相同哈希。
    let other_order = json!({"body": "b", "tags": [], "data": {}, "id": "n1", "title": "Q"});
    let _ = &mut reordered;
    assert_eq!(
        block_hash(&json!({"id": "n1", "title": "Q", "body": "b", "tags": [], "data": {}})),
        block_hash(&other_order)
    );
}

#[test]
fn content_root_hash_covers_semantics_only() {
    let base = sample_project();

    let mut layout_changed = base.clone();
    layout_changed["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 1, "y": 2, "width": 3, "height": 4}]);
    let mut title_changed = base.clone();
    title_changed["title"] = json!("另一个标题");
    let mut node_changed = base.clone();
    node_changed["nodes"][0]["body"] = json!("编辑后的正文");
    let mut evidence_hanging_changed = base.clone();
    evidence_hanging_changed["nodes"][0]["evidenceIds"] = json!([]);

    assert_eq!(content_root_hash(&base), content_root_hash(&layout_changed));
    assert_eq!(content_root_hash(&base), content_root_hash(&title_changed));
    // evidenceIds 是悬挂字段：增删证据不改变主张哈希。
    assert_eq!(
        content_root_hash(&base),
        content_root_hash(&evidence_hanging_changed)
    );
    assert_ne!(content_root_hash(&base), content_root_hash(&node_changed));
    assert_eq!(content_root_hash(&base).len(), 64);
}

#[test]
fn file_hash_is_64_hex_and_covers_layout() {
    let base = sample_project();
    let mut layout_changed = base.clone();
    layout_changed["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 1, "y": 2, "width": 3, "height": 4}]);
    let mut node_changed = base.clone();
    node_changed["nodes"][0]["body"] = json!("编辑后的正文");

    assert_eq!(file_hash(&base).len(), 64);
    assert_eq!(file_hash(&base), file_hash(&base));
    assert_ne!(file_hash(&base), file_hash(&layout_changed));
    assert_ne!(file_hash(&base), file_hash(&node_changed));
}

#[test]
fn file_hash_is_stable_across_key_order() {
    let a: serde_json::Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
    let b: serde_json::Value = serde_json::from_str(r#"{"a":2,"z":1}"#).unwrap();
    assert_eq!(file_hash(&a), file_hash(&b));
}

#[test]
fn invariants_report_dangling_references_and_duplicates() {
    let broken = json!({
        "schemaVersion": 2, "id": "p", "title": "T", "discipline": "D",
        "updatedAt": "2026-01-01T00:00:00Z", "revision": 1,
        "nodes": [
            {"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {}, "evidenceIds": ["ghost-ev"], "status": "confirmed", "provenance": {}},
            {"id": "n1", "type": "concept", "title": "Dup", "body": "b", "tags": [], "data": {}, "evidenceIds": [], "status": "draft", "provenance": {}},
            {"type": "note", "title": "无 id", "body": "b", "tags": [], "data": {}, "evidenceIds": [], "status": "draft", "provenance": {}}
        ],
        "edges": [
            {"id": "x1", "type": "contradicts", "source": "ghost", "target": "n1", "directed": true, "polarity": "positive", "confidence": 0.9, "conditions": [], "evidenceIds": [], "provenance": {}},
            {"id": "x2", "type": "supports", "source": "n1", "target": "n1", "directed": true, "polarity": "negative", "conditions": [], "evidenceIds": [], "provenance": {}}
        ],
        "evidence": [
            {"id": "e1", "sourceType": "paper", "sourceId": "p", "title": "T", "status": "confirmed", "provenance": {}}
        ],
        "placements": [{"id": "pl", "viewId": "v", "nodeId": "ghost", "x": 0, "y": 0, "width": 1, "height": 1}],
        "scenarios": [{"id": "s1", "name": "S", "disabledNodeIds": ["ghost"], "disabledEdgeIds": ["ghost-edge"], "nodeOverrides": {"ghost2": {}}, "edgeOverrides": {}, "parameters": {}, "hypothesis": "h", "expectedEffect": "e", "createdAt": "x"}],
        "activity": []
    });
    let violations = check_invariants(&broken);
    let codes: Vec<&str> = violations.iter().map(|v| v.code.as_str()).collect();
    assert!(codes.contains(&"duplicate-id"), "{codes:?}");
    assert!(codes.contains(&"missing-id"), "{codes:?}");
    assert!(codes.contains(&"dangling-node-reference"), "{codes:?}");
    assert!(codes.contains(&"dangling-edge-reference"), "{codes:?}");
    assert!(
        codes.contains(&"unresolved-evidence-reference"),
        "{codes:?}"
    );
    assert!(codes.contains(&"uncited-evidence"), "{codes:?}");
    assert!(codes.contains(&"polarity-conflict"), "{codes:?}");
    // dangling 位置标注正确。
    assert!(violations
        .iter()
        .any(|v| v.entity == "edge:x1" && v.code == "dangling-node-reference"));
    assert!(violations
        .iter()
        .any(|v| v.entity == "placement:pl" && v.code == "dangling-node-reference"));
}

#[test]
fn compile_project_rejects_error_level_violations() {
    // 重复 id:两个节点同 id → duplicate-id(Error)→ compile_project 必须拒绝,
    // 否则最后写入胜出的哈希表会静默损坏 contentRootHash。
    let mut broken = sample_project();
    broken["nodes"].as_array_mut().unwrap().push(json!({
        "id": "n1",
        "type": "concept",
        "title": "撞车节点",
        "data": {}
    }));
    let bytes = serde_json::to_vec(&broken).unwrap();
    let result = compile_project(&bytes);
    match result {
        Err(CompileFailure::Invariant(messages)) => {
            assert!(
                messages.iter().any(|m| m.contains("duplicate-id")),
                "{messages:?}"
            );
        }
        other => panic!("duplicate ids must fail compilation, got {other:?}"),
    }

    // 干净项目照常编译。
    let bytes = serde_json::to_vec(&sample_project()).unwrap();
    assert!(compile_project(&bytes).is_ok());
}

#[test]
fn clean_project_has_no_invariant_violations() {
    let violations = check_invariants(&sample_project());
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn compile_injects_hashes_and_verifies() {
    let result = compile(&sample_project());
    assert_eq!(
        result.block_hashes.len(),
        4,
        "{:?}",
        result.block_hashes.keys()
    );
    assert!(result.violations.is_empty(), "{:?}", result.violations);
    assert_eq!(result.content_root_hash.len(), 64);
    assert_eq!(result.file_hash.len(), 64);

    for collection in ["nodes", "edges", "evidence"] {
        for entity in result.project[collection].as_array().unwrap() {
            let hash = entity["blockHash"].as_str().expect("blockHash injected");
            assert_eq!(hash.len(), 12);
        }
    }
    assert_eq!(
        result.project["contentRootHash"].as_str().unwrap().len(),
        64
    );
    assert_eq!(result.project["fileHash"].as_str().unwrap().len(), 64);

    let verified = verify_hashes(&result.project);
    assert!(verified.valid, "{:?}", verified.mismatches);
}

#[test]
fn verify_catches_edit_cascades() {
    let result = compile(&sample_project());
    // 编辑级联：任一内容变化 → fileHash 失效 → 校验失败。
    let mut edited = result.project.clone();
    edited["nodes"][0]["body"] = json!("编辑后的正文");
    let after = verify_hashes(&edited);
    assert!(!after.valid);
    assert!(after.mismatches.iter().any(|m| m.contains("node:n1")));
    assert!(after
        .mismatches
        .iter()
        .any(|m| m == "contentRootHash mismatch"));
    assert!(after.mismatches.iter().any(|m| m == "fileHash mismatch"));

    // 布局变化 → fileHash 失效，但 contentRootHash 与 blockHash 仍有效。
    let mut moved = result.project.clone();
    moved["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 9, "y": 9, "width": 1, "height": 1}]);
    let after_move = verify_hashes(&moved);
    assert!(!after_move.valid);
    assert!(after_move
        .mismatches
        .iter()
        .any(|m| m == "fileHash mismatch"));
    assert!(!after_move
        .mismatches
        .iter()
        .any(|m| m.contains("blockHash") || m.contains("contentRootHash")));
}

#[test]
fn verify_rejects_raw_project_without_hashes() {
    assert!(!verify_hashes(&sample_project()).valid);
}

#[test]
fn compiled_project_round_trips_through_canonical_form() {
    let result = compile(&sample_project());
    let bytes = canonicalize(&result.project);
    let reparsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(canonicalize(&reparsed), bytes);
    assert!(verify_hashes(&reparsed).valid);
}
