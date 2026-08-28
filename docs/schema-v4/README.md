# Anyway Schema v4

本目录是 Anyway MVP 科学计算引擎的**规范 + 实施规划**来源。它定义了两个版本化根契约，并约束了画布边的模型与存储抽象。

## 两份根契约（命名已按决定重命名）

| 根对象 | 版本字符串 | 产出方 | 内容 | 来源文档 |
|---|---|---|---|---|
| **LLM 抽取 schema** | `myc.llm.v4` | LLM 抽取器 | document / evidence / variables / contexts / axiom_sets / experiments / operator_candidates / abstraction_candidates | `handoff-spec.md` §3–§35 |
| **图 IR schema** | `myc.graph-ir.v4` | 确定性编译器 | blocks / operators / chains / fibers / bundles / identifiability / consistency_checks / provenance_index | `handoff-spec.md` §36–§109 |

> 命名说明：原 handoff 文档中写作 `anyway.extract.v3` / `anyway.canvas-ir.v3`；已决定重命名为 `myc.llm.v4` / `myc.graph-ir.v4`，以与现有 `.mycproj` 画布格式 `canvas-format-v3.md`（`schemaVersion: 3`）区分。实现中 `crates/anyway-schema-v4` 的版本常量 `LLM_SCHEMA_VERSION` / `GRAPH_IR_SCHEMA_VERSION` 已定为这两个字符串；`handoff-spec.md` 正文里的旧版本串保留为历史，不影响实现。

## 核心模型（一句话）

- **3 种科学原语**：`Bool ∪ Number ∪ Expression`（`false ≠ unknown`）。
- **5 种算子（边）**：`T`(Transform) / `K`(Kernel) / `I`(Intervention) / `M`(Marginalization) / `Q`(Quotient)。
- **核心边界**：**LLM 抽取语义；图引擎决定计算。**

## 目录

| 文件 | 内容 |
|---|---|
| `handoff-spec.md` | Schema v4 完整 handoff 规范（两个根 schema、20 条不变量、测试夹具、16 步优先级） |
| `mvp-architecture.md` | MVP 系统架构（计算范畴、5 条一致性公理、模块链、存储、API 面） |
| `implementation-plan.md` | **实施规划**：16 步（每步一次 commit）、edge 约束 + 抽象类结构、存储抽象 + data host bus |

## 与现有画布的关系

现有 `docs/canvas-format-v3.md`（`.mycproj` 画布）与新 Schema v4 是两个不同的契约：

- 画布 = 通用语义图（question/hypothesis/method/evidence 节点 + supports/contradicts 等边）。
- Schema v4 = 科学计算层（变量/算子/链/纤维/束 + T/K/I/M/Q）。

实施规划中已**把画布边收敛到 5 个算子**（`implementation-plan.md` §2）：`crates/anyway-schema-v4/src/compiler.rs` 的 `converge_edge` 与 `app/lib/schema-v4/compiler.ts` 的 `convergeEdge` 提供 12 种旧边的收敛映射（`supports`/`contradicts` → 证据状态，其余 → T/K/M）。

## 实现状态

16 步已全部落地，每步一次 commit（见 `implementation-plan.md` §5）。实现位于：

- **Rust 权威 crate**：`crates/anyway-schema-v4/`（`name = "anyway-schema-v4"`）。模块：
  - `extract.rs` — `myc.llm.v4`（ExtractionV3）
  - `ir.rs` — `myc.graph-ir.v4`（CanvasIRV3）
  - `validator.rs` / `reference.rs` — 结构校验 + 引用解析 + 证据 gate
  - `canonicalize.rs` — 概念/单位/表达式规范化
  - `state.rs` / `state_diff.rs` / `intervention.rs` — 稀疏状态 + 差 + joint 干预
  - `compiler.rs` — Block/Operator IR + edge 收敛
  - `hash.rs` — 语义/实例/链哈希
  - `matcher.rs` / `identifiability.rs` — 历史匹配 + 可识别性引擎
  - `chain.rs` / `fiber.rs` / `bundle.rs` — 链/纤维/束
  - `consistency.rs` / `q_validation.rs` — 一致性检查 + Q 候选校验
  - `storage.rs` — 后端无关 `Storage` trait + data host bus 操作
- **TS 镜像**：`app/lib/schema-v4/`（`index.ts` 汇总导出，与 Rust 逐字段对齐）。
- 测试：`cargo test --manifest-path crates/anyway-schema-v4/Cargo.toml`（85 用例）+ `tests/schema-v4.test.ts`（6 用例）+ `vue-tsc` 类型检查。
