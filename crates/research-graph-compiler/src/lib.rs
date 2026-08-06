//! Research Canvas 图编译器语义内核（独立 crate，spec §2）。
//!
//! 图属性一律由本模块硬计算，绝不由 LLM/代理生成。当前已落地：
//! 规范化（canonical）、双哈希（hash）、不变式检查（invariant）与
//! 编译管线（compile）；parse / patch / index / traversal / topology /
//! factor / scenario / diff / layout / export / cache 为骨架模块，
//! 按 spec GC-01…GC-15 逐步填充。每个模块 ≤500 行。
//!
//! v3 Schema 关键规则（canvas-format-v3）：
//! - 双哈希方案(§3)：每个 ① 区实体有 `blockHash`(12 hex)；全文件有
//!   `fileHash`(64 hex)；语义区整体有 `contentRootHash`(64 hex)。
//! - 规范化(§3.4)：对象键排序（含嵌套 data）、数组规范化后排序、数字
//!   规范序列化、文本 NFC 归一化 + 空白折叠。
//! - 编辑级联(§3.5)：实体内容变化 ⇒ blockHash ⇒ contentRootHash；
//!   任意字段（含布局）变化 ⇒ fileHash。
//! - 边界定案(E4/E5)：布局、审阅、时间戳、status、证据定位以及
//!   evidenceIds 一律不进入语义哈希 —— 主张=身份，证据=悬挂字段。

pub mod cache;
pub mod canonical;
pub mod compile;
pub mod diff;
pub mod error;
pub mod export;
pub mod factor;
pub mod hash;
pub mod index;
pub mod invariant;
pub mod layout;
pub mod model;
pub mod parse;
pub mod patch;
pub mod scenario;
pub mod topology;
pub mod traversal;

// ---------------------------------------------------------------------------
// 公共 API（与迁移前 `src-tauri/src/graph_compiler.rs` 保持一致，薄转发）。
// ---------------------------------------------------------------------------

pub use canonical::{canonical_number, canonicalize, normalize_key, normalize_text};
pub use compile::{
    compile, compile_project, compile_project_with_options, verify_hashes, CompileOptions,
    CompileResult, VerifyResult,
};
pub use error::CompileFailure;
pub use export::{export_mermaid, project_digest, Digest, ExportOptions};
pub use hash::{
    block_hash, compute_block_hashes, content_root_hash, content_root_hash_from_hashes, edge_claim,
    evidence_claim, file_hash, node_claim, sha256_hex,
};
pub use invariant::{check_invariants, InvariantViolation, Severity};
