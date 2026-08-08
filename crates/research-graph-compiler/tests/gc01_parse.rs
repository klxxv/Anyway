//! GC-01 解析、schema 与迁移测试套件（spec GC-01，12 项）。
//! 覆盖：空文件、BOM、非法 UTF-8、缺 schemaVersion、v2 迁移、未来版本、
//! 未知字段、null 字段、10 万节点极限、深层嵌套、CRLF vs LF、1e-4 vs 0.0001。
//!
//! Golden 文件位于 `tests/fixtures/`：首次运行生成并回读校验，之后严格比对。

use research_graph_compiler::*;
use serde_json::{json, Value};
use std::path::Path;

fn fixture_bytes(relative: &str) -> Vec<u8> {
    std::fs::read(Path::new("tests/fixtures").join(relative))
        .unwrap_or_else(|error| panic!("cannot read fixture {relative}: {error}"))
}

/// golden 比对：不存在则生成并回读校验（幂等），存在则严格相等。
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

/// GC01-01 空文件：0 字节 → 稳定错误码 empty-input，offset=0，无 panic。
#[test]
fn gc01_01_empty_input_reports_stable_error() {
    let error = parse_bytes(b"", &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "empty-input");
    assert_eq!(error.offset, Some(0));
    assert_eq!(error.to_string(), "empty-input at offset 0: input is empty");
}

/// GC01-02 仅 BOM 视为空输入；BOM + JSON 正常解析。
#[test]
fn gc01_02_bom_is_handled_consistently() {
    let bom_only = fixture_bytes("invalid/bom-only.json");
    let error = parse_bytes(&bom_only, &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "empty-input");
    assert_eq!(error.offset, Some(0));

    let bom_plus_json = b"\xef\xbb\xbf{\"schemaVersion\":3,\"nodes\":[]}".to_vec();
    let value = parse_bytes(&bom_plus_json, &ParseOptions::defaults()).unwrap();
    assert_eq!(value["schemaVersion"], 3);
}

/// GC01-03 非法 UTF-8：报告首个非法偏移，不做替换字符容错。
#[test]
fn gc01_03_invalid_utf8_reports_first_offset() {
    let bytes = fixture_bytes("invalid/invalid-utf8.bin");
    let error = parse_bytes(&bytes, &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "invalid-utf8");
    let offset = error.offset.expect("invalid-utf8 must carry an offset");
    assert_eq!(
        bytes[offset], 0xff,
        "offset must point at the first invalid byte"
    );
}

/// GC01-04 缺 schemaVersion：stable 错误 + JSON 指针 /schemaVersion。
#[test]
fn gc01_04_missing_schema_version_reports_pointer() {
    let project = json!({"nodes": [], "edges": [], "evidence": []});
    let options = ParseOptions {
        strict_schema: true,
        ..ParseOptions::defaults()
    };
    let error = check_schema(&project, &options).unwrap_err();
    assert_eq!(error.code, "missing-schema-version");
    assert_eq!(error.json_pointer.as_deref(), Some("/schemaVersion"));

    // 全流程同样稳定报错。
    let error = parse_project(br#"{"title":"no version"}"#, &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "missing-schema-version");
}

/// GC01-05 v2 → v3 迁移：旧 ID 内容派生重写、边与全部引用同步。
#[test]
fn gc01_05_v2_migration_rewrites_ids_and_syncs_references() {
    let bytes = fixture_bytes("migration/v2-project.json");
    let migrated = parse_project(&bytes, &ParseOptions::defaults()).unwrap();

    // 版本升级到 v3。
    assert_eq!(migrated["schemaVersion"], SCHEMA_VERSION);

    // 全部 ① 区实体 ID 重写为 12-hex 内容派生短 ID。
    let node_ids: Vec<String> = migrated["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    let edge_ids: Vec<String> = migrated["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    let evidence_ids: Vec<String> = migrated["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    for id in node_ids
        .iter()
        .chain(edge_ids.iter())
        .chain(evidence_ids.iter())
    {
        assert_eq!(
            id.len(),
            12,
            "migrated id {id:?} must be a 12-hex block hash"
        );
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // 边同步：source/target 指向重写后的节点 ID。
    let edge = &migrated["edges"][0];
    assert!(node_ids.contains(&edge["source"].as_str().unwrap().to_string()));
    assert!(node_ids.contains(&edge["target"].as_str().unwrap().to_string()));

    // evidenceIds 同步：节点与边的引用指向重写后的证据 ID。
    assert!(evidence_ids.contains(&edge["evidenceIds"][0].as_str().unwrap().to_string()));
    assert!(evidence_ids.contains(
        &migrated["nodes"][0]["evidenceIds"][0]
            .as_str()
            .unwrap()
            .to_string()
    ));

    // placements.nodeId 同步。
    assert!(node_ids.contains(
        &migrated["placements"][0]["nodeId"]
            .as_str()
            .unwrap()
            .to_string()
    ));

    // scenarios 禁用/覆盖引用同步。
    let scenario = &migrated["scenarios"][0];
    assert!(node_ids.contains(&scenario["disabledNodeIds"][0].as_str().unwrap().to_string()));
    assert!(edge_ids.contains(&scenario["disabledEdgeIds"][0].as_str().unwrap().to_string()));
    let override_key = scenario["nodeOverrides"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    assert!(node_ids.contains(&override_key));

    // 迁移报告：每个重写都有 old → new 条目。
    let (value, report) = migrate_v2_to_v3(serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(report.schema_version, 3);
    assert_eq!(
        report.id_remaps.len(),
        4,
        "n1/n2/x1/e1 must all be remapped"
    );
    assert_eq!(
        serde_json::to_vec_pretty(&value).unwrap(),
        serde_json::to_vec_pretty(&migrated).unwrap()
    );

    // Golden：迁移输出整体固化，防未来实现漂移。
    assert_golden(
        "migration/v2-migrated.golden.json",
        &serde_json::to_vec_pretty(&migrated).unwrap(),
    );
}

/// GC01-06 未来版本（999）：稳定拒绝；输入字节不被修改。
#[test]
fn gc01_06_future_schema_version_rejected() {
    let bytes = br#"{"schemaVersion":999,"nodes":[]}"#;
    let error = parse_project(bytes, &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "unsupported-schema-version");
    assert_eq!(error.json_pointer.as_deref(), Some("/schemaVersion"));
    // 原始字节不修改：错误路径为纯函数，输入保持不变。
    assert_eq!(bytes, br#"{"schemaVersion":999,"nodes":[]}"#);

    // v1 及更早版本同样拒绝。
    let error = parse_project(
        br#"{"schemaVersion":1,"nodes":[]}"#,
        &ParseOptions::defaults(),
    )
    .unwrap_err();
    assert_eq!(error.code, "unsupported-schema-version");

    // 回归:2³²+3 必须按"未来版本"拒绝,不得 as u32 截断成 v3 蒙混过关。
    let error = parse_project(
        br#"{"schemaVersion":4294967299,"nodes":[]}"#,
        &ParseOptions::defaults(),
    )
    .unwrap_err();
    assert_eq!(error.code, "unsupported-schema-version");
    assert!(error.detail.contains("4294967299"), "{}", error.detail);
}

/// GC01-07 未知字段：宽松模式保留（opaque）；严格模式拒绝，模式差异稳定。
#[test]
fn gc01_07_unknown_fields_strict_vs_lenient() {
    let bytes = br#"{"schemaVersion":3,"id":"p","title":"T","discipline":"D","updatedAt":"x","revision":1,"nodes":[],"edges":[],"evidence":[],"placements":[],"scenarios":[],"activity":[],"vendorExtension":{"opaque":true}}"#;

    // 宽松：保留未知字段并成功解析。
    let lenient = parse_project(bytes, &ParseOptions::defaults()).unwrap();
    assert_eq!(lenient["vendorExtension"]["opaque"], true);

    // 严格：未知字段 → 稳定错误 + 指针。
    let options = ParseOptions {
        strict_schema: true,
        ..ParseOptions::defaults()
    };
    let error = parse_project(bytes, &options).unwrap_err();
    assert_eq!(error.code, "unknown-field");
    assert_eq!(error.json_pointer.as_deref(), Some("/vendorExtension"));

    // 已知字段在严格模式下不报错。
    let known = br#"{"schemaVersion":3,"id":"p","title":"T","discipline":"D","updatedAt":"x","revision":1,"nodes":[],"edges":[],"evidence":[],"placements":[],"scenarios":[],"activity":[]}"#;
    assert!(parse_project(known, &options).is_ok());
}

/// GC01-08 null 字段：nodes 为 null → 类型不匹配，定位 /nodes。
#[test]
fn gc01_08_null_collection_is_type_mismatch() {
    let project = json!({"schemaVersion": 3, "nodes": null});
    let error = check_schema(&project, &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "type-mismatch");
    assert_eq!(error.json_pointer.as_deref(), Some("/nodes"));

    // 其他集合字段同理。
    for field in ["edges", "evidence", "placements", "scenarios", "activity"] {
        let mut project = json!({"schemaVersion": 3, "nodes": []});
        project[field] = Value::Null;
        let error = check_schema(&project, &ParseOptions::defaults()).unwrap_err();
        assert_eq!(error.code, "type-mismatch");
        assert_eq!(
            error.json_pointer.as_deref(),
            Some(format!("/{field}").as_str())
        );
    }
}

/// GC01-09 10 万节点极限：默认预算内解析成功；低预算 → 可预测的稳定终止。
#[test]
fn gc01_09_hundred_thousand_nodes_and_resource_budget() {
    let mut json = String::from(
        r#"{"schemaVersion":3,"id":"big","title":"T","discipline":"D","updatedAt":"x","revision":1,"nodes":["#,
    );
    for index in 0..100_000 {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"id":"n{index}","type":"concept","title":"T{index}","body":"b","tags":[],"data":{{}}}}"#
        ));
    }
    json.push_str(r#"],"edges":[],"evidence":[],"placements":[],"scenarios":[],"activity":[]}"#);

    let project = parse_project(json.as_bytes(), &ParseOptions::defaults())
        .expect("100k nodes must parse within the default budget");
    assert_eq!(project["nodes"].as_array().unwrap().len(), 100_000);

    // 低预算 → resource-limit-exceeded，无部分产物、无 OOM。
    let options = ParseOptions {
        max_entities: 1000,
        ..ParseOptions::defaults()
    };
    let error = parse_project(json.as_bytes(), &options).unwrap_err();
    assert_eq!(error.code, "resource-limit-exceeded");
    assert!(error.detail.contains("100000"));
}

/// GC01-10 深层嵌套：超过深度限制 → 稳定拒绝，防栈溢出。
#[test]
fn gc01_10_deep_nesting_rejected() {
    let bytes = fixture_bytes("invalid/deep-nesting.json");
    let error = parse_bytes(&bytes, &ParseOptions::defaults()).unwrap_err();
    assert_eq!(error.code, "nesting-too-deep");

    // 可配置深度：浅预算同样生效。
    let options = ParseOptions {
        max_depth: 4,
        ..ParseOptions::defaults()
    };
    let error = parse_bytes(br#"[[[[[0]]]]]"#, &options).unwrap_err();
    assert_eq!(error.code, "nesting-too-deep");

    // 限制内的嵌套正常解析。
    assert!(parse_bytes(br#"[[[[0]]]]"#, &options).is_ok());
}

/// GC01-11 CRLF 与 LF：解析后规范化语义相同，后续哈希相同。
#[test]
fn gc01_11_crlf_and_lf_parse_equally() {
    let lf = fixture_bytes("migration/v2-project.json");
    let crlf = String::from_utf8(lf.clone())
        .unwrap()
        .replace('\n', "\r\n")
        .into_bytes();

    let a = parse_project(&lf, &ParseOptions::defaults()).unwrap();
    let b = parse_project(&crlf, &ParseOptions::defaults()).unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    assert_eq!(file_hash(&a), file_hash(&b));
    assert_eq!(content_root_hash(&a), content_root_hash(&b));
}

/// GC01-12 数字 1e-4 与 0.0001：内部数值相等，规范序列化一致。
#[test]
fn gc01_12_exponent_and_decimal_serialize_equally() {
    let a: Value = serde_json::from_str(r#"{"confidence":1e-4}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"confidence":0.0001}"#).unwrap();
    assert_eq!(canonicalize(&a), canonicalize(&b));
    assert_eq!(
        String::from_utf8(canonicalize(&a)).unwrap(),
        r#"{"confidence":0.0001}"#
    );
    assert_eq!(file_hash(&a), file_hash(&b));
}

/// 非法输入 → 稳定错误码 golden：防错误码漂移（跨版本契约）。
#[test]
fn gc01_error_codes_are_stable_golden() {
    let options = ParseOptions::defaults();
    let strict = ParseOptions {
        strict_schema: true,
        ..ParseOptions::defaults()
    };
    let cases: Vec<(&str, Vec<u8>, &ParseOptions)> = vec![
        ("empty", b"".to_vec(), &options),
        ("bom-only", fixture_bytes("invalid/bom-only.json"), &options),
        (
            "invalid-utf8",
            fixture_bytes("invalid/invalid-utf8.bin"),
            &options,
        ),
        ("invalid-json", b"{not json".to_vec(), &options),
        ("nan", fixture_bytes("invalid/nan.json"), &options),
        ("infinity", fixture_bytes("invalid/infinity.json"), &options),
        (
            "deep-nesting",
            fixture_bytes("invalid/deep-nesting.json"),
            &options,
        ),
        (
            "missing-schema-version",
            br#"{"nodes":[]}"#.to_vec(),
            &options,
        ),
        (
            "future-schema",
            br#"{"schemaVersion":999,"nodes":[]}"#.to_vec(),
            &options,
        ),
        (
            "null-nodes",
            br#"{"schemaVersion":3,"nodes":null}"#.to_vec(),
            &options,
        ),
        (
            "strict-unknown-field",
            br#"{"schemaVersion":3,"nodes":[],"mystery":1}"#.to_vec(),
            &strict,
        ),
    ];
    let mut golden = serde_json::Map::new();
    for (name, bytes, opts) in cases {
        let code = parse_project(&bytes, opts)
            .err()
            .map(|e| e.code.to_string())
            .unwrap_or_else(|| panic!("case {name:?} must fail"));
        golden.insert(name.to_string(), Value::String(code));
    }
    let golden_value = Value::Object(golden.clone());
    assert_golden(
        "golden/error-codes.json",
        &serde_json::to_vec_pretty(&golden_value).unwrap(),
    );
    // 显式断言核心契约（错误码本身稳定，golden 只防回归）。
    assert_eq!(golden["empty"], "empty-input");
    assert_eq!(golden["invalid-utf8"], "invalid-utf8");
    assert_eq!(golden["nan"], "invalid-number");
    assert_eq!(golden["deep-nesting"], "nesting-too-deep");
    assert_eq!(golden["future-schema"], "unsupported-schema-version");
    assert_eq!(golden["strict-unknown-field"], "unknown-field");
}
