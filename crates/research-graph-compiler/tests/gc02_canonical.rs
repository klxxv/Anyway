//! GC-02 规范化测试套件（spec GC-02，12+ 项）。
//! 覆盖：键序反转、嵌套 data 排序、集合 vs 序列数组、NFD/NFC 归一化、
//! 空白折叠、NaN/Infinity 拒绝、多语言 map 排序、evidence quote 不入 ID、
//! claim evidenceIds 不入 claim 身份，以及幂等性与转义稳定性。
//!
//! Golden 文件位于 `tests/fixtures/`：首次运行生成并回读校验，之后严格比对。

use research_graph_compiler::*;
use serde_json::{json, Value};
use std::path::Path;

fn fixture_bytes(relative: &str) -> Vec<u8> {
    std::fs::read(Path::new("tests/fixtures").join(relative))
        .unwrap_or_else(|error| panic!("cannot read fixture {relative}: {error}"))
}

fn assert_golden(relative: &str, actual: &[u8]) {
    let path = Path::new("tests/fixtures").join(relative);
    if path.exists() {
        let expected = std::fs::read(&path).unwrap();
        assert_eq!(expected, actual, "golden mismatch: {}", path.display());
    } else {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        let reread = std::fs::read(&path).unwrap();
        assert_eq!(
            reread,
            actual,
            "golden write-back mismatch: {}",
            path.display()
        );
    }
}

/// GC02-01 对象键顺序完全反转 → 输出字节逐字节相同（字典序稳定）。
#[test]
fn gc02_01_reversed_key_order_identical_bytes() {
    let a: Value = serde_json::from_str(
        r#"{"zeta":1,"gamma":2,"alpha":{"deep":{"y":1,"x":2}},"delta":" d "}"#,
    )
    .unwrap();
    let b: Value = serde_json::from_str(
        r#"{"delta":" d ","alpha":{"deep":{"x":2,"y":1}},"gamma":2,"zeta":1}"#,
    )
    .unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    let bytes = canonicalize(&a);
    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        r#"{"alpha":{"deep":{"x":2,"y":1}},"delta":"d","gamma":2,"zeta":1}"#
    );
    assert_golden("canonical/keys.golden.txt", &bytes);
}

/// GC02-02 嵌套 data 键顺序不同 → 递归排序后字节相同。
#[test]
fn gc02_02_nested_data_keys_sorted_recursively() {
    let a: Value = serde_json::from_str(
        r#"{"nodes":[{"id":"n1","data":{"z":{"b":1,"a":2},"top":[{"q":1,"p":2}]}}]}"#,
    )
    .unwrap();
    let b: Value = serde_json::from_str(
        r#"{"nodes":[{"id":"n1","data":{"top":[{"p":2,"q":1}],"z":{"a":2,"b":1}}}]}"#,
    )
    .unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    assert_eq!(
        String::from_utf8(canonicalize(&a)).unwrap(),
        r#"{"nodes":[{"data":{"top":[{"p":2,"q":1}],"z":{"a":2,"b":1}},"id":"n1"}]}"#
    );
}

/// GC02-03 集合语义数组：乱序输入按规范字节稳定排序（含对象元素）。
#[test]
fn gc02_03_unordered_arrays_sorted_as_sets() {
    let a: Value = serde_json::from_str(r#"{"tags":["c","a","b"]}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"tags":["b","c","a"]}"#).unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    assert_eq!(
        String::from_utf8(canonicalize(&a)).unwrap(),
        r#"{"tags":["a","b","c"]}"#
    );

    let objects: Value = serde_json::from_str(r#"[{"b":1},{"a":2},{"c":0}]"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&objects)).unwrap(),
        r#"[{"a":2},{"b":1},{"c":0}]"#
    );
}

/// GC02-04 序列语义数组：SEQUENCE_FIELDS 中的数组保持元素顺序，绝不排序。
#[test]
fn gc02_04_ordered_sequence_arrays_keep_order() {
    assert!(SEQUENCE_FIELDS.contains(&"pathSteps"));
    let steps: Value = serde_json::from_str(r#"[3,1,2]"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize_field("pathSteps", &steps)).unwrap(),
        "[3,1,2]"
    );
    // 同一数组在集合字段下仍按集合排序。
    assert_eq!(
        String::from_utf8(canonicalize_field("tags", &steps)).unwrap(),
        "[1,2,3]"
    );
    // 顺序差异 → 序列字节不同；集合语义下相同。
    let steps_2: Value = serde_json::from_str(r#"[1,2,3]"#).unwrap();
    assert_ne!(
        canonicalize_field("pathSteps", &steps),
        canonicalize_field("pathSteps", &steps_2)
    );
    assert_eq!(
        canonicalize_field("tags", &steps),
        canonicalize_field("tags", &steps_2)
    );
}

/// 回归:顶层 canonicalize 的对象分支必须走 canonicalize_field——
/// 之前统一 canonicalize 导致序列字段在对象内被排序(canonicalize_field 死代码)。
#[test]
fn gc02_04_object_members_preserve_sequence_field_order() {
    let chain_a: Value = serde_json::from_str(
        r#"{"id":"c1","pathSteps":["n1","n2","n3"],"type":"logic-chain"}"#,
    )
    .unwrap();
    let chain_b: Value = serde_json::from_str(
        r#"{"id":"c1","pathSteps":["n3","n2","n1"],"type":"logic-chain"}"#,
    )
    .unwrap();
    assert_ne!(
        canonicalize(&chain_a),
        canonicalize(&chain_b),
        "chains with different step orders must not hash the same"
    );
    // 同序不同写法(键顺序、空白)仍等价。
    let chain_c: Value = serde_json::from_str(
        r#"{"type":"logic-chain", "pathSteps": ["n1","n2","n3"],  "id":"c1"}"#,
    )
    .unwrap();
    assert_eq!(canonicalize(&chain_a), canonicalize(&chain_c));
}

/// GC02-05 NFD 与 NFC 等价字符 → 统一 NFC，哈希相同。
#[test]
fn gc02_05_nfd_nfc_equivalent_hashes() {
    let composed: Value = serde_json::from_str(r#"{"text":"\u00e9"}"#).unwrap();
    let decomposed: Value = serde_json::from_str(r#"{"text":"e\u0301"}"#).unwrap();
    assert_eq!(canonicalize(&composed), canonicalize(&decomposed));
    assert_eq!(block_hash(&composed), block_hash(&decomposed));
    // 键同样 NFC 归一化。
    let a: Value = serde_json::from_str(r#"{"e\u0301":1}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"\u00e9":1}"#).unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
}

/// GC02-06 连续空格、制表、换行 → 按字段规则折叠为单个空格。
#[test]
fn gc02_06_whitespace_folding_tabs_newlines() {
    let value: Value = serde_json::from_str(r#"{"body":"a\t\tb\n\nc  d"}"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&value)).unwrap(),
        r#"{"body":"a b c d"}"#
    );
    // 键不折叠空白（结构标识，合并键会掩盖数据错误）。
    assert_eq!(normalize_key("a  b"), "a  b");
    assert_eq!(normalize_text("a  b"), "a b");
}

/// GC02-07 前后空白与不可断空格 → 去边界空白并映射为普通空格。
#[test]
fn gc02_07_boundary_and_non_breaking_whitespace() {
    let value: Value =
        serde_json::from_str("{\"title\":\"  \\u00a0a\\u3000b\\u00a0 \\t \"}").unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&value)).unwrap(),
        r#"{"title":"a b"}"#
    );
    assert_eq!(normalize_text(" \u{200b}"), "\u{200b}"); // 零宽空格非空白，保留
    assert_eq!(normalize_text("\u{200b}"), "\u{200b}");
}

/// GC02-08 -0、0、0.0 → 全部规范为 0，哈希相同。
#[test]
fn gc02_08_negative_zero_and_integral_floats_unify() {
    let values: Vec<Value> = ["0", "-0", "-0.0", "0.0"]
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    for value in &values {
        assert_eq!(canonicalize(value), b"0".to_vec());
    }
    for pair in values.windows(2) {
        assert_eq!(
            file_hash(&json!({"n": &pair[0]})),
            file_hash(&json!({"n": &pair[1]}))
        );
    }
    // 非整小数保留最短往返表示。
    assert_eq!(
        canonicalize(&serde_json::from_str::<Value>("3.5").unwrap()),
        b"3.5".to_vec()
    );
}

/// GC02-09 NaN/Infinity → 拒绝非 JSON 有限数，稳定错误码。
#[test]
fn gc02_09_nan_infinity_rejected() {
    // serde_json 数值模型本身无法持有 NaN/Infinity。
    assert!(serde_json::Number::from_f64(f64::NAN).is_none());
    assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());

    // 字面量在解析层稳定拒绝。
    for (fixture, expected) in [
        ("invalid/nan.json", "NaN"),
        ("invalid/infinity.json", "Infinity"),
    ] {
        let bytes = fixture_bytes(fixture);
        let error = parse_bytes(&bytes, &ParseOptions::defaults()).unwrap_err();
        assert_eq!(error.code, "invalid-number");
        assert!(error.detail.contains(expected), "{}", error.detail);
    }
    let error = parse_bytes(b"-Infinity", &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "invalid-number");
}

/// GC02-10 多语言 map：语言键乱序 → 按 UTF-8 字节序稳定排序；
/// 新增语言键 → 内容哈希变化。
#[test]
fn gc02_10_multilingual_map_sorted_and_hash_sensitive() {
    let a: Value = serde_json::from_str(
        r#"{"title":{"zh":"研究","en":"Research","ja":"研究","\u03b2eta":"b"}}"#,
    )
    .unwrap();
    let b: Value = serde_json::from_str(
        r#"{"title":{"en":"Research","ja":"研究","\u03b2eta":"b","zh":"研究"}}"#,
    )
    .unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    let bytes = canonicalize(&a);
    assert_golden("canonical/multilingual.golden.txt", &bytes);

    // 新增语言键 → 内容哈希变化（语言集合是 claim 的一部分）。
    let with_ko: Value =
        serde_json::from_str(r#"{"title":{"en":"Research","ja":"研究","ko":"연구","zh":"研究"}}"#)
            .unwrap();
    assert_ne!(file_hash(&a), file_hash(&with_ko));

    // blockHash 层面：data 内多语言 map 参与 claim 身份。
    let project_a = json!({"nodes":[{"id":"n1","type":"concept","title":"T","body":"b","tags":[],"data":{"labels":{"zh":"x","en":"y"}}}]});
    let project_b = json!({"nodes":[{"id":"n1","type":"concept","title":"T","body":"b","tags":[],"data":{"labels":{"en":"y","ko":"k","zh":"x"}}}]});
    assert_ne!(content_root_hash(&project_a), content_root_hash(&project_b));
}

/// GC02-11 evidence quote 差异 → quote/locator 不进入证据 ID 输入。
#[test]
fn gc02_11_evidence_quote_not_part_of_id() {
    let base = json!({
        "id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T",
        "authors": "A", "year": 2024, "doi": "10.x", "url": "https://x"
    });
    let mut quoted = base.clone();
    quoted["locator"] = json!({"page": 3, "quote": "完全不同的一句话"});
    let mut reworded = base.clone();
    reworded["locator"] = json!({"page": 99, "section": "5", "quote": "还是那句话"});

    assert_eq!(evidence_claim(&base), evidence_claim(&quoted));
    assert_eq!(evidence_claim(&base), evidence_claim(&reworded));
    assert_eq!(
        block_hash(&evidence_claim(&base)),
        block_hash(&evidence_claim(&quoted))
    );
    // 但 sourceId（逻辑锚）变化 → 证据身份变化。
    let mut other_source = base.clone();
    other_source["sourceId"] = json!("p-2");
    assert_ne!(
        block_hash(&evidence_claim(&base)),
        block_hash(&evidence_claim(&other_source))
    );
}

/// GC02-12 claim evidenceIds 顺序与集合变化 → 不进入 claim blockHash。
#[test]
fn gc02_12_claim_evidence_ids_not_part_of_identity() {
    let base = json!({
        "id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {},
        "evidenceIds": ["e1"]
    });
    let mut reordered = base.clone();
    reordered["evidenceIds"] = json!(["e1", "e2"]);
    let mut removed = base.clone();
    removed["evidenceIds"] = json!([]);

    assert_eq!(node_claim(&base), node_claim(&reordered));
    assert_eq!(node_claim(&base), node_claim(&removed));
    assert_eq!(
        block_hash(&node_claim(&base)),
        block_hash(&node_claim(&reordered))
    );
    assert_eq!(
        block_hash(&node_claim(&base)),
        block_hash(&node_claim(&removed))
    );

    // 职责分离：evidenceIds 是悬挂字段，仍参与 fileHash。
    let project_a = json!({"schemaVersion":3,"id":"p","title":"T","discipline":"D","updatedAt":"x","revision":1,"nodes":[base],"edges":[],"evidence":[{"id":"e1","sourceType":"paper","sourceId":"p","title":"T"}],"placements":[],"scenarios":[],"activity":[]});
    let mut project_b = project_a.clone();
    project_b["nodes"][0]["evidenceIds"] = json!([]);
    assert_ne!(file_hash(&project_a), file_hash(&project_b));
}

/// GC02-13 规范化幂等性：canonicalize 的输出再规范化后字节不变。
#[test]
fn gc02_13_canonicalization_is_idempotent() {
    let value: Value =
        serde_json::from_str(r#"{"b":[3,1,{"z":2,"a":1}],"a":{"d":"x  y","c":"\u00e9"}}"#).unwrap();
    let once = canonicalize(&value);
    let reparsed: Value = serde_json::from_slice(&once).unwrap();
    let twice = canonicalize(&reparsed);
    assert_eq!(once, twice);

    // 规范闭包：canonical → parse → canonical 字节相同（GC15-10 前置）。
    let again: Value = serde_json::from_slice(&twice).unwrap();
    assert_eq!(canonicalize(&again), twice);
}

/// GC02-14 特殊字符稳定转义：引号、反斜杠、控制字符。
#[test]
fn gc02_14_special_characters_escaped_stably() {
    let value: Value = serde_json::from_str(r#"{"note":"a\"b\\c\nd\te"}"#).unwrap();
    assert_eq!(
        String::from_utf8(canonicalize(&value)).unwrap(),
        r#"{"note":"a\"b\\c d e"}"#
    );
    // 空白折叠把 \n \t 折叠为空格，但引号/反斜杠转义保持。
    let bytes = canonicalize(&value);
    let reparsed: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(canonicalize(&reparsed), bytes);
}
