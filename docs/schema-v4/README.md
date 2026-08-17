# Anyway Schema v4

本目录是 Anyway MVP 科学计算引擎的**规范 + 实施规划**来源。它定义了两个版本化根契约，并约束了画布边的模型与存储抽象。

## 两份根契约（命名已按决定重命名）

| 根对象 | 版本字符串 | 产出方 | 内容 | 来源文档 |
|---|---|---|---|---|
| **LLM 抽取 schema** | `myc.llm.v4` | LLM 抽取器 | document / evidence / variables / contexts / axiom_sets / experiments / operator_candidates / abstraction_candidates | `handoff-spec.md` §3–§35 |
| **图 IR schema** | `myc.graph-ir.v4` | 确定性编译器 | blocks / operators / chains / fibers / bundles / identifiability / consistency_checks / provenance_index | `handoff-spec.md` §36–§109 |

> 命名说明：原 handoff 文档中写作 `anyway.extract.v3` / `anyway.canvas-ir.v3`；已决定重命名为 `myc.llm.v4` / `myc.graph-ir.v4`，以与现有 `.mycproj` 画布格式 `canvas-format-v3.md`（`schemaVersion: 3`）区分。`handoff-spec.md` 正文中的旧版本串尚未替换，**在 16 步的第 1 步（数据模型）落地时统一改为 `myc.llm.v4` / `myc.graph-ir.v4`**。

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

实施规划中会**把画布边收敛到 5 个算子**（见 `implementation-plan.md` §2），使二者最终共享同一计算语义。
