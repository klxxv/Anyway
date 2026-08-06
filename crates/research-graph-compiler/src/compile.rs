//! 编译管线 / Compile pipeline (spec §5)。
//! 不变式检查 → 实体 blockHash → contentRootHash → fileHash，
//! git 式自编码（`fileHash` 字段本身置空）。哈希校验供保存/加载后自校验。

use crate::canonical::canonicalize;
use crate::hash::{compute_block_hashes, content_root_hash_from_hashes, file_hash};
use crate::invariant::{check_invariants, InvariantViolation};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// 编译产物：注入哈希后的项目 + 哈希明细 + 不变式违规。
/// Compile output: the project with hashes injected, hash details, and
/// invariant violations.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    /// 注入 blockHash / contentRootHash / fileHash 后的项目。
    pub project: Value,
    /// entityId → blockHash(12 hex)。
    pub block_hashes: HashMap<String, String>,
    /// 语义区根哈希（64 hex）/ Semantic zone root hash (64 hex).
    pub content_root_hash: String,
    /// 全文件哈希（64 hex）/ Whole-file hash (64 hex).
    pub file_hash: String,
    pub violations: Vec<InvariantViolation>,
}

/// 把 blockHash 注入每个 ① 区实体 / Inject blockHash into every ①-zone entity.
fn inject_block_hashes(project: &mut Value, block_hashes: &HashMap<String, String>) {
    for key in ["nodes", "edges", "evidence"] {
        if let Some(entities) = project.get_mut(key).and_then(Value::as_array_mut) {
            for entity in entities {
                if let Some(object) = entity.as_object_mut() {
                    if let Some(id) = object.get("id").and_then(Value::as_str) {
                        if let Some(hash) = block_hashes.get(id) {
                            object.insert("blockHash".to_string(), Value::String(hash.clone()));
                        }
                    }
                }
            }
        }
    }
}

/// 编译管线（§15.1）：不变式检查 → 实体 blockHash → contentRootHash → fileHash。
/// Compile pipeline: invariants → entity block hashes → content root hash →
/// file hash (git-style self-encoding, the `fileHash` field itself is blanked).
pub fn compile(project: &Value) -> CompileResult {
    let violations = check_invariants(project);
    let block_hashes = compute_block_hashes(project);
    let content_root_hash = content_root_hash_from_hashes(&block_hashes);

    let mut compiled = project.clone();
    inject_block_hashes(&mut compiled, &block_hashes);
    if let Some(root) = compiled.as_object_mut() {
        root.insert(
            "contentRootHash".to_string(),
            Value::String(content_root_hash.clone()),
        );
    }
    let file_hash = file_hash(&compiled);
    if let Some(root) = compiled.as_object_mut() {
        root.insert("fileHash".to_string(), Value::String(file_hash.clone()));
    }

    CompileResult {
        project: compiled,
        block_hashes,
        content_root_hash,
        file_hash,
        violations,
    }
}

/// 哈希校验结果 / Hash verification result.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub valid: bool,
    pub mismatches: Vec<String>,
}

/// 重新计算全部哈希并与文件内嵌值比对（编辑级联自校验）。
/// Recomputes every hash and compares it with the embedded values — the
/// self-check that catches edit cascades (§3.5).
pub fn verify_hashes(project: &Value) -> VerifyResult {
    let mut mismatches = Vec::new();
    let block_hashes = compute_block_hashes(project);

    for (kind, collection) in [
        ("node", "nodes"),
        ("edge", "edges"),
        ("evidence", "evidence"),
    ] {
        let entities = project.get(collection).and_then(Value::as_array);
        let empty: Vec<Value> = Vec::new();
        for entity in entities.unwrap_or(&empty) {
            let Some(id) = entity.get("id").and_then(Value::as_str) else {
                continue;
            };
            let expected = block_hashes.get(id);
            let embedded = entity.get("blockHash").and_then(Value::as_str);
            if let (Some(expected), Some(embedded)) = (expected, embedded) {
                if expected != embedded {
                    mismatches.push(format!("{kind}:{id} blockHash mismatch"));
                }
            } else {
                mismatches.push(format!("{kind}:{id} blockHash missing or unhashable"));
            }
        }
    }

    let expected_root = content_root_hash_from_hashes(&block_hashes);
    match project.get("contentRootHash").and_then(Value::as_str) {
        Some(embedded) if embedded == expected_root => {}
        _ => mismatches.push("contentRootHash mismatch".to_string()),
    }
    let expected_file = file_hash(project);
    match project.get("fileHash").and_then(Value::as_str) {
        Some(embedded) if embedded == expected_file => {}
        _ => mismatches.push("fileHash mismatch".to_string()),
    }

    VerifyResult {
        valid: mismatches.is_empty(),
        mismatches,
    }
}

// ---------------------------------------------------------------------------
// spec §5 全量编译入口（骨架）：bytes → parse → canonicalize → hashes →
// invariants → indexes → factors → algorithms → layout → serialize。
// 当前只接现有 compile 管线，后续任务按 spec GC-01…GC-15 逐步填充。
// ---------------------------------------------------------------------------

/// 编译选项（骨架占位，后续按 spec §3 扩展）。
#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    /// 严格 schema 模式（保留字段，暂不参与管线）。
    pub strict_schema: bool,
    /// 是否计算布局（保留字段，暂不参与管线）。
    pub compute_layouts: bool,
}

impl CompileOptions {
    /// 默认选项 / Default options.
    pub fn defaults() -> Self {
        Self::default()
    }
}

/// 全量编译入口：与现有 `compile` 等价，但以字节输入为契约（spec §5）。
/// 目前执行 canonicalize → compile 管线；parse 阶段（GC-01）由后续任务接入。
pub fn compile_project(bytes: &[u8]) -> Result<CompileResult, crate::error::CompileFailure> {
    compile_project_with_options(bytes, &CompileOptions::defaults())
}

/// 带选项的全量编译入口：`compile_project` 的选项版。
/// Compile entry with options — the options-aware twin of `compile_project`.
/// `canvas compile` CLI 经此传入 `--strict` / `--layout`；选项字段当前为
/// 保留项（spec §3，随 GC-04/GC-13 接入管线），结构与签名先行稳定。
pub fn compile_project_with_options(
    bytes: &[u8],
    _options: &CompileOptions,
) -> Result<CompileResult, crate::error::CompileFailure> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| crate::error::CompileFailure::Parse(error.to_string()))?;
    let canonical: Value = serde_json::from_slice(&canonicalize(&value))
        .map_err(|error| crate::error::CompileFailure::Parse(error.to_string()))?;
    Ok(compile(&canonical))
}
