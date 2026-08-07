//! 多阶段语义提取管线（Pass A–F）。
//!
//! 六阶段提取策略（§5）：
//! - **Pass A**: 文档结构提取 — title, authors, abstract, section tree, references
//! - **Pass B**: 局部实体提取 — question, hypothesis, claim, method, experiment, result, evidence, variable
//! - **Pass C**: 变量裂变与实验矩阵 — IV/DV/control/moderator/mediator, 取值域和单位
//! - **Pass D**: 跨段合并 — 统一别名、指标名、数据集；合并同一主张的多个证据
//! - **Pass E**: 论文级综合 — 识别主结论、消融设计、交互效应、混杂、缺失对照、冲突
//! - **Pass F**: 本地验证 — JSON Schema 校验、quote 回指验证、表格数值重算、anchor 唯一性、tempId 闭包
//!
//! Prompt 模板从配置加载，支持中英文。不合并。

pub mod config;
pub mod error;
pub mod ir;
pub mod pipeline;
pub mod prompts;
pub mod validation;

pub use config::PipelineConfig;
pub use error::PipelineError;
pub use ir::*;
pub use pipeline::{Pipeline, PipelineResult};
pub use prompts::PromptTemplate;
pub use validation::ValidationReport;
