//! 图编译器语义内核 / Semantic kernel of the graph compiler.
//!
//! v3 Schema 关键规则（canvas-format-v3）：
//! - 双哈希方案(§3)：每个 ① 区实体有 `blockHash`(12 hex)；全文件有 `fileHash`(64 hex)；
//!   语义区整体有 `contentRootHash`(64 hex)。
//! - 规范化(§3.4)：对象键排序（含嵌套 data）、数组规范化后排序、数字规范序列化、
//!   文本 NFC 归一化 + 空白折叠。
//! - 编辑级联(§3.5)：实体内容变化 ⇒ 该实体 blockHash 变化 ⇒ contentRootHash 变化；
//!   任意字段（含布局）变化 ⇒ fileHash 变化。`verify_hashes` 供保存/加载后自校验。
//! - 边界定案(E4/E5)：布局、审阅、时间戳、status、证据定位（locator/quote/页码/偏移）
//!   以及 evidenceIds 一律不进入语义哈希 —— 主张=身份，证据=悬挂字段。
//! - 布局(§11)：`views[].layout = { mode, params }` 是唯一布局意图；坐标由纯函数
//!   硬计算，仅人工 pinned 的 placement 覆盖计算结果，未 pinned 坐标不持久化。
//!
//! 分区 / Zones：
//! - ① 语义区（semantic zone）：nodes + edges + evidence，由 contentRootHash 覆盖。
//! - ② 布局区（layout zone）：views[] 意图 + pinned placements，仅影响 fileHash。
//! - ③ 元数据/场景区：title、discipline、scenarios、navigation、activity、时间戳，
//!   仅影响 fileHash。
//!
//! 模块拆分（Phase 1.2/1.3）：canonical（规范化+哈希）、invariants（不变式）、
//! layout（确定性布局）；algorithms（图算法）由 Phase 1.2 任务补充。

pub mod algorithms;
pub mod analysis;
pub mod canonical;
pub mod invariants;
pub mod layout;

pub use algorithms::{TraversalDirection, TraversalRequest, TraversalResult, TraversalStrategy};
pub use analysis::{
    compare_scenario_reachability, compute_logic_chain, contradiction_chains, detect_cycles,
    graph_patch_from_diff,
};
pub use canonical::{
    block_hash, canonicalize, compile, compute_block_hashes, content_root_hash,
    content_root_hash_from_hashes, edge_claim, evidence_claim, file_hash, node_claim,
    normalize_key, normalize_text, verify_hashes, CompileResult, VerifyResult,
};
pub use invariants::{check_invariants, InvariantViolation, Severity};
pub use layout::{
    apply_fallback, apply_pinned, compute_layout, defaults_for_mode, layout_view, resolve_params,
    LayoutParams, LayoutPosition, LayoutResult, LAYOUT_MODES,
};
