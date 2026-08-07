//! GraphPatch 生成器——将 AgentCandidates 转换为可审阅的 GraphPatch 操作。
//!
//! 核心流程：AgentCandidates → build_graph_patch() → plan_patch 预演 → 用户审阅 → apply_patch
//!
//! - **tempId → 永久 ID**：通过 sha256(content + doi) 生成确定性 12 hex ID
//! - **plan_patch**：调用 research-graph-compiler 预演，展示影响范围
//! - **output 为 PluginGraphPatch**：兼容前端 contracts.ts 中的 GraphPatchOperation 类型

pub mod ids;
pub mod mapper;
pub mod plan;
pub mod types;

pub use ids::generate_permanent_id;
pub use mapper::build_graph_patch;
pub use plan::preview_patch;
pub use types::*;
