//! 解析与 schema 迁移 / Parsing, schema & migration (spec GC-01)。
//! 原始字节 → v3 内存模型：空输入/BOM/非法 UTF-8 稳定报错、schema 版本
//! 检查（缺失/未来版本/未知字段/null 字段）、v2 → v3 迁移（旧 ID 内容
//! 派生重写与边同步）、资源上限（实体数预算 / JSON 嵌套深度）。

use crate::hash::block_hash;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// 当前 schema 版本（v3）/ Current schema version (v3).
pub const SCHEMA_VERSION: u32 = 3;

/// 旧版 schema 版本（v2）/ Legacy schema version (v2).
pub const SCHEMA_VERSION_V2: u32 = 2;

/// 解析错误（GC01-01…GC01-10）：机器可读错误码 + 字节偏移 + JSON 指针。
/// Parse error: machine-readable code + byte offset + JSON pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 机器可读错误码，如 "empty-input"、"invalid-utf8"。
    pub code: &'static str,
    /// 首个出错字节偏移（如适用）。
    pub offset: Option<usize>,
    /// 出错字段的 JSON 指针（如 "/schemaVersion"）。
    pub json_pointer: Option<String>,
    /// 人类可读细节（不参与稳定性契约）。
    pub detail: String,
}

impl ParseError {
    /// 构造解析错误 / Build a parse error.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code: &'static str,
        offset: Option<usize>,
        json_pointer: Option<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            offset,
            json_pointer,
            detail: detail.into(),
        }
    }

    /// 无偏移/无指针的错误 / Error without offset or pointer.
    pub fn simple(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(code, None, None, detail)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.code)?;
        if let Some(pointer) = &self.json_pointer {
            write!(formatter, " at {pointer}")?;
        } else if let Some(offset) = self.offset {
            write!(formatter, " at offset {offset}")?;
        }
        write!(formatter, ": {}", self.detail)
    }
}

impl std::error::Error for ParseError {}

/// 解析选项 / Parse options.
#[derive(Clone, Debug)]
pub struct ParseOptions {
    /// 严格 schema 模式：根对象未知字段报错（宽松模式保留）。默认 false。
    pub strict_schema: bool,
    /// 是否启用 v2 → v3 迁移（默认 true）。
    pub migrate_v2: bool,
    /// ① 区实体总数上限（资源预算，防 OOM）。默认 200_000。
    pub max_entities: usize,
    /// 最大 JSON 嵌套深度（防栈溢出）。默认 128。
    pub max_depth: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            strict_schema: false,
            migrate_v2: true,
            max_entities: 200_000,
            max_depth: 128,
        }
    }
}

impl ParseOptions {
    /// 默认选项 / Default options.
    pub fn defaults() -> Self {
        Self::default()
    }
}

/// v2 → v3 迁移报告 / Migration report (GC01-05).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// 旧 ID → 新 ID（仅实际发生重写的条目，按实体出现顺序）。
    pub id_remaps: Vec<(String, String)>,
    /// 迁移后的 schema 版本（= 3）。
    pub schema_version: u32,
}

/// 解析原始字节 → 内存 Value。
/// Parses raw bytes into an in-memory Value: empty input, BOM, invalid UTF-8,
/// non-finite number literals and nesting depth are all rejected up front with
/// stable error codes (GC01-01/02/03/09/10).
pub fn parse_bytes(bytes: &[u8], options: &ParseOptions) -> Result<Value, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::new(
            "empty-input",
            Some(0),
            None,
            "input is empty",
        ));
    }
    // GC01-03：非法 UTF-8 不做替换字符容错，报告首个非法偏移。
    let text = std::str::from_utf8(bytes).map_err(|error| {
        ParseError::new(
            "invalid-utf8",
            Some(error.valid_up_to()),
            None,
            format!("input is not valid UTF-8: {error}"),
        )
    })?;
    // GC01-02：仅 BOM 视为空输入；BOM + JSON 正常解析。
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.is_empty() {
        return Err(ParseError::new(
            "empty-input",
            Some(0),
            None,
            "input contains only a UTF-8 BOM",
        ));
    }
    // GC02-09：JSON 标准不允许多个顶层数字字面量之外的非有限数。
    reject_non_finite_literals(text)?;
    // GC01-10：迭代式深度预检，避免 serde_json 在深嵌套下抛非稳定错误。
    check_nesting_depth(text.as_bytes(), options.max_depth)?;
    serde_json::from_str(text).map_err(|error| {
        ParseError::new(
            "invalid-json",
            Some(error.line()),
            None,
            format!("invalid JSON: {error}"),
        )
    })
}

/// 拒绝顶层 NaN / Infinity / -Infinity 字面量（GC02-09）。
/// Rejects top-level non-finite number literals with a stable code.
fn reject_non_finite_literals(text: &str) -> Result<(), ParseError> {
    let trimmed = text.trim_start();
    for literal in ["NaN", "Infinity", "-Infinity"] {
        if trimmed.starts_with(literal) {
            let offset = text.len() - trimmed.len();
            return Err(ParseError::new(
                "invalid-number",
                Some(offset),
                None,
                format!("non-finite number literal {literal:?} is not valid JSON"),
            ));
        }
    }
    Ok(())
}

/// 迭代式 JSON 嵌套深度预检（不递归，防栈溢出）。
/// Iterative nesting-depth precheck that never recurses, so pathological
/// inputs cannot overflow the stack (GC01-10).
fn check_nesting_depth(bytes: &[u8], max_depth: usize) -> Result<(), ParseError> {
    let mut depth: usize = 0;
    let mut index = 0;
    let len = bytes.len();
    while index < len {
        match bytes[index] {
            b'{' | b'[' => {
                depth += 1;
                if depth > max_depth {
                    return Err(ParseError::new(
                        "nesting-too-deep",
                        Some(index),
                        None,
                        format!("JSON nesting depth exceeds limit {max_depth}"),
                    ));
                }
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            b'"' => {
                // 跳过字符串字面量（含转义），其中的括号不算深度。
                index += 1;
                while index < len {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(len),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            _ => index += 1,
        }
    }
    Ok(())
}

/// v3 顶层允许字段 / Allowed top-level keys in schema v3.
const TOP_LEVEL_FIELDS: &[&str] = &[
    "schemaVersion",
    "id",
    "title",
    "discipline",
    "updatedAt",
    "revision",
    "nodes",
    "edges",
    "evidence",
    "placements",
    "scenarios",
    "activity",
    "contentRootHash",
    "fileHash",
];

/// 集合字段（存在时必须为数组）/ Collection fields (must be arrays when present).
const COLLECTION_FIELDS: &[&str] = &[
    "nodes",
    "edges",
    "evidence",
    "placements",
    "scenarios",
    "activity",
];

/// ① 区实体数（nodes + edges + evidence）/ ①-zone entity count.
fn entity_count(root: &Map<String, Value>) -> usize {
    ["nodes", "edges", "evidence"]
        .iter()
        .fold(0, |count, field| {
            count
                + root
                    .get(*field)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
        })
}

/// schema 版本与根结构检查（GC01-04/06/07/08/09）。
/// Checks schema version, root shape, unknown fields (strict) and the entity
/// budget with stable error codes and JSON pointers.
pub fn check_schema(project: &Value, options: &ParseOptions) -> Result<(), ParseError> {
    let Some(root) = project.as_object() else {
        return Err(ParseError::new(
            "type-mismatch",
            None,
            Some(String::new()),
            "project root must be a JSON object",
        ));
    };
    let version = match root.get("schemaVersion") {
        Some(Value::Number(number)) => {
            let raw = number.as_u64().ok_or_else(|| {
                ParseError::new(
                    "type-mismatch",
                    None,
                    Some("/schemaVersion".to_string()),
                    "schemaVersion must be an integer",
                )
            })?;
            // 不得 `as u32` 截断:2³²+3 会被当成 v3 绕过版本闸门。
            u32::try_from(raw).map_err(|_| {
                ParseError::new(
                    "unsupported-schema-version",
                    None,
                    Some("/schemaVersion".to_string()),
                    format!("schema version {raw} is newer than supported v{SCHEMA_VERSION}"),
                )
            })?
        }
        Some(_) => {
            return Err(ParseError::new(
                "type-mismatch",
                None,
                Some("/schemaVersion".to_string()),
                "schemaVersion must be an integer",
            ));
        }
        None => {
            return Err(ParseError::new(
                "missing-schema-version",
                None,
                Some("/schemaVersion".to_string()),
                "schemaVersion is required",
            ));
        }
    };
    // GC01-06：未来版本拒绝；v1 及更早同样拒绝（最低支持 v2 迁移）。
    if version > SCHEMA_VERSION {
        return Err(ParseError::new(
            "unsupported-schema-version",
            None,
            Some("/schemaVersion".to_string()),
            format!("schema version {version} is newer than supported v{SCHEMA_VERSION}"),
        ));
    }
    if version < SCHEMA_VERSION_V2 {
        return Err(ParseError::new(
            "unsupported-schema-version",
            None,
            Some("/schemaVersion".to_string()),
            format!(
                "schema version {version} is older than the oldest supported v{SCHEMA_VERSION_V2}"
            ),
        ));
    }
    // GC01-07：严格模式拒绝未知顶层字段；宽松模式保留（opaque）。
    if options.strict_schema {
        for key in root.keys() {
            if !TOP_LEVEL_FIELDS.contains(&key.as_str()) {
                return Err(ParseError::new(
                    "unknown-field",
                    None,
                    Some(format!("/{key}")),
                    format!("unknown top-level field {key:?} in schema v{SCHEMA_VERSION}"),
                ));
            }
        }
    }
    // GC01-08：集合字段存在但为 null/非数组 → 类型不匹配，定位字段。
    for field in COLLECTION_FIELDS {
        if let Some(value) = root.get(*field) {
            if !value.is_array() {
                return Err(ParseError::new(
                    "type-mismatch",
                    None,
                    Some(format!("/{field}")),
                    format!("{field} must be an array"),
                ));
            }
        }
    }
    // GC01-09：实体数资源预算。
    let count = entity_count(root);
    if count > options.max_entities {
        return Err(ParseError::new(
            "resource-limit-exceeded",
            None,
            None,
            format!(
                "entity count {count} exceeds budget {}",
                options.max_entities
            ),
        ));
    }
    Ok(())
}

/// 迁移用的 claim 字段（不含 id —— 新 ID 由内容派生）。
const NODE_FIELDS: &[&str] = &["type", "title", "body", "tags", "data"];
const EDGE_FIELDS: &[&str] = &[
    "type",
    "source",
    "target",
    "directed",
    "polarity",
    "confidence",
    "conditions",
    "note",
    "experiment",
];
const EVIDENCE_FIELDS: &[&str] = &[
    "sourceType",
    "sourceId",
    "title",
    "authors",
    "year",
    "doi",
    "url",
];

/// 内容派生短 ID：claim（不含 id）的 12-hex blockHash。
/// Content-derived short ID: 12-hex blockHash of the claim minus `id`.
fn content_id(entity: &Value, fields: &[&str]) -> String {
    let mut map = Map::new();
    if let Some(object) = entity.as_object() {
        for field in fields {
            if let Some(value) = object.get(*field) {
                map.insert((*field).to_string(), value.clone());
            }
        }
    }
    block_hash(&Value::Object(map))
}

/// v2 → v3 迁移：旧 ID 内容派生重写（12-hex blockHash）与边同步。
/// Migrates a v2 project to v3: every ①-zone entity gets a content-derived
/// 12-hex ID and all references (edge endpoints, evidenceIds, placements,
/// scenario disables/overrides) are rewritten in sync (GC01-05).
pub fn migrate_v2_to_v3(project: Value) -> Result<(Value, MigrationReport), ParseError> {
    let mut migrated = project.clone();
    let mut remaps: Vec<(String, String)> = Vec::new();
    let mut seen_new_ids: HashMap<String, String> = HashMap::new();

    // 第一阶段：计算每个 ① 区实体的新 ID；内容相同 → 新 ID 相同 → 明确冲突。
    for (collection, fields) in [
        ("nodes", NODE_FIELDS),
        ("edges", EDGE_FIELDS),
        ("evidence", EVIDENCE_FIELDS),
    ] {
        let entities = migrated
            .get(collection)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for entity in &entities {
            let Some(old_id) = entity.get("id").and_then(Value::as_str) else {
                continue;
            };
            let new_id = content_id(entity, fields);
            if let Some(previous) = seen_new_ids.get(&new_id) {
                if previous != old_id {
                    return Err(ParseError::new(
                        "id-collision-after-migration",
                        None,
                        Some(format!("/{collection}/~/{old_id}/id")),
                        format!(
                            "entities {previous:?} and {old_id:?} share content-derived id {new_id:?}"
                        ),
                    ));
                }
            } else {
                seen_new_ids.insert(new_id.clone(), old_id.to_string());
            }
            if new_id != old_id {
                remaps.push((old_id.to_string(), new_id));
            }
        }
    }

    let remap = |id: &str| -> String {
        remaps
            .iter()
            .find(|(old, _)| old == id)
            .map(|(_, new)| new.clone())
            .unwrap_or_else(|| id.to_string())
    };

    // 第二阶段：重写实体 id 与全部引用（边同步）。
    for (collection, fields) in [
        ("nodes", NODE_FIELDS),
        ("edges", EDGE_FIELDS),
        ("evidence", EVIDENCE_FIELDS),
    ] {
        let _ = fields;
        if let Some(entities) = migrated.get_mut(collection).and_then(Value::as_array_mut) {
            for entity in entities {
                if let Some(object) = entity.as_object_mut() {
                    if let Some(old_id) = object.get("id").and_then(Value::as_str) {
                        let new_id = remap(old_id);
                        object.insert("id".to_string(), Value::String(new_id));
                    }
                    rewrite_evidence_ids(object, &remap);
                }
            }
        }
    }
    if let Some(edges) = migrated.get_mut("edges").and_then(Value::as_array_mut) {
        for edge in edges {
            if let Some(object) = edge.as_object_mut() {
                for field in ["source", "target"] {
                    if let Some(Value::String(value)) = object.get_mut(field) {
                        *value = remap(value);
                    }
                }
            }
        }
    }
    if let Some(placements) = migrated.get_mut("placements").and_then(Value::as_array_mut) {
        for placement in placements {
            if let Some(object) = placement.as_object_mut() {
                if let Some(Value::String(value)) = object.get_mut("nodeId") {
                    *value = remap(value);
                }
            }
        }
    }
    if let Some(scenarios) = migrated.get_mut("scenarios").and_then(Value::as_array_mut) {
        for scenario in scenarios {
            if let Some(object) = scenario.as_object_mut() {
                for field in ["disabledNodeIds", "disabledEdgeIds"] {
                    rewrite_id_array(object, field, &remap);
                }
                for field in ["nodeOverrides", "edgeOverrides"] {
                    if let Some(overrides) = object.get_mut(field).and_then(Value::as_object_mut) {
                        let keys: Vec<String> = overrides.keys().cloned().collect();
                        for key in keys {
                            if let Some(value) = overrides.remove(&key) {
                                overrides.insert(remap(&key), value);
                            }
                        }
                    }
                }
            }
        }
    }
    // 导航区:recentNodeIds/pinnedNodeIds 也是节点引用,漏改会在迁移后
    // 必报 dangling-node-reference。
    if let Some(navigation) = migrated.get_mut("navigation").and_then(Value::as_object_mut) {
        for field in ["recentNodeIds", "pinnedNodeIds"] {
            rewrite_id_array(navigation, field, &remap);
        }
    }
    if let Some(root) = migrated.as_object_mut() {
        root.insert(
            "schemaVersion".to_string(),
            Value::Number(serde_json::Number::from(SCHEMA_VERSION)),
        );
    }

    Ok((
        migrated,
        MigrationReport {
            id_remaps: remaps,
            schema_version: SCHEMA_VERSION,
        },
    ))
}

/// 重写实体 evidenceIds（引用证据 ID）/ Rewrite `evidenceIds` references.
fn rewrite_evidence_ids(object: &mut Map<String, Value>, remap: &impl Fn(&str) -> String) {
    if let Some(ids) = object.get_mut("evidenceIds").and_then(Value::as_array_mut) {
        for value in ids {
            if let Some(id) = value.as_str() {
                *value = Value::String(remap(id));
            }
        }
    }
}

/// 重写禁用 ID 数组 / Rewrite a disabled-ids array.
fn rewrite_id_array(object: &mut Map<String, Value>, field: &str, remap: &impl Fn(&str) -> String) {
    if let Some(ids) = object.get_mut(field).and_then(Value::as_array_mut) {
        for value in ids {
            if let Some(id) = value.as_str() {
                *value = Value::String(remap(id));
            }
        }
    }
}

/// 全流程：parse_bytes → check_schema →（v2 → v3 迁移）→ v3 项目。
/// Full pipeline: bytes → parse → schema check → optional v2→v3 migration.
pub fn parse_project(bytes: &[u8], options: &ParseOptions) -> Result<Value, ParseError> {
    let mut project = parse_bytes(bytes, options)?;
    check_schema(&project, options)?;
    let version = project
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    if version == SCHEMA_VERSION_V2 && options.migrate_v2 {
        let (migrated, _report) = migrate_v2_to_v3(project)?;
        project = migrated;
    }
    Ok(project)
}
