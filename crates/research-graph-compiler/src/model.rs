//! 图模型基础类型 / Core model types (spec §3)。
//! 当前编译管线以 serde_json::Value 为工作表示；本模块是 v1 强类型
//! 模型（CanonicalProject / GraphIndexes / DerivedGraphProperties /
//! ProjectDigest）的落点，随 GC-01…GC-15 实现逐步填充。

/// 256 位哈希（64 位小写 hex）/ 256-bit hash (64 lowercase hex chars).
pub type Hash256 = String;

/// 短哈希（12 位小写 hex，blockHash）/ Short hash (12 lowercase hex chars).
pub type ShortHash = String;

/// 规范化后的项目模型（骨架占位）。
/// 目标：parse/canonical 阶段产出的强类型项目，取代散装 Value。
#[derive(Clone, Debug, Default)]
pub struct CanonicalProject {
    /// 预留：schema 版本（v3）。
    pub schema_version: Option<u32>,
}

/// 图索引（骨架占位）：正反邻接、证据、哈希与引用索引（GC-06）。
#[derive(Clone, Debug, Default)]
pub struct GraphIndexes {}

/// 派生图性质（骨架占位）：逻辑链、矛盾链、可达性、影响集合。
#[derive(Clone, Debug, Default)]
pub struct DerivedGraphProperties {}

/// 项目 digest（骨架占位）：规范输入摘要 + 稳定诊断（GC-13）。
#[derive(Clone, Debug, Default)]
pub struct ProjectDigest {}
