//! 图编译器语义内核 —— 薄转发层。
//! 实现已迁移至独立 crate `crates/research-graph-compiler`（spec §2）；
//! 本文件仅 re-export 其公共 API，调用方（`lib.rs` 的 `pub mod graph_compiler;`
//! 及后续 Tauri commands / workspace_host）无需任何改动。
//!
//! 语义内核职责（不变）：规范化、双哈希、不变式检查、编译管线 ——
//! 图属性一律由 Rust 硬计算，绝不由 LLM/代理生成。

pub use research_graph_compiler::{
    block_hash, canonical_number, canonicalize, check_invariants, compile, compile_project,
    compute_block_hashes, content_root_hash, content_root_hash_from_hashes, edge_claim,
    evidence_claim, file_hash, node_claim, normalize_key, normalize_text, sha256_hex,
    verify_hashes, CompileFailure, CompileOptions, CompileResult, InvariantViolation, Severity,
    VerifyResult,
};
