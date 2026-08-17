# Anyway Schema v4 — 实施规划

**目标**：把 Anyway 从「通用研究画布」升级为「科学计算引擎」，按 handoff 规范的 16 步优先级顺序实现，每步一次 commit。

---

## 1. 已拍板的设计决策

| 决策 | 结论 |
|---|---|
| 根 schema 命名 | `myc.llm.v4`（LLM 抽取）+ `myc.graph-ir.v4`（图 IR） |
| 画布 edge | 从 12 种收敛为 5 个算子 `T/K/I/M/Q`，用**抽象类结构** |
| 存储 | **后端无关的抽象接口**（trait），MySQL / SQLite / MongoDB / Milvus 皆可，经 **data host bus** 交互 |
| 范围 | **完整 16 步**，每步一次 commit |

---

## 2. Edge 约束 + 抽象类结构（IR 计算基础）

### 2.1 现状问题

现有 `app/lib/research-types.ts` 的 `EDGE_TYPES` 有 12 种：

```
causes, correlates, supports, contradicts, depends_on, derived_from,
part_of, controls, mediates, moderates, uses, measures
```

太多且语义重叠，无法直接用于 IR 计算（例如 `supports` 和 `contradicts` 是证据属性，不是算子；`causes`/`controls`/`mediates`/`moderates` 都近似因果，但程度不同）。

### 2.2 目标：5 个抽象算子类

边收敛为 5 个算子，构成一个有继承层次的**抽象类结构**（Rust 用 trait/enum，TS 用抽象类/判别联合）：

```text
Operator (abstract)
├── T — Transform        (确定性变换:  y = T(x))
├── K — Kernel           (条件/随机依赖: K(Y | X, C, A))
├── I — Intervention     (配置改变:    I: X0 -> X1, 可含多变量 joint set)
├── M — Marginalization  (聚合/粗粒化:  M: X_fine -> X_coarse)
└── Q — Quotient         (抽象/商:      Q: X -> X/~)
```

- 每个算子有统一的数据结构：`{ id, operator, input_refs[], output_refs[], payload{}, context_ref, axiom_set_ref, evidence_refs[], semantic_hash, instance_hash }`。
- **证据不是边**：`supports`/`contradicts` 从边降级为变量/算子的 `evidence.status ∈ {supported, ambiguous, unsupported}` 注解。
- **因果必须经干预**：`causes`/`controls`/`mediates` 不直接成边，而是由 `I`（干预）+ `K`（核）+ 可识别性引擎推导；图拓扑本身不蕴含因果（V3-19）。

### 2.3 现有 12 边的收敛映射

| 现有 edge | 收敛到 |
|---|---|
| supports / contradicts | → evidence status（非边） |
| causes / controls | → K（仅当 I 干预可识别） |
| mediates / moderates | → K 的交互项（可识别性引擎） |
| depends_on / derived_from / part_of | → T 或 M（结构/确定关系） |
| uses / measures | → T |
| correlates | → K |

### 2.4 Rust/TS 双实现

- Rust 权威：`crates/research-graph-compiler`（或新 crate）用 `enum OperatorKind { T, K, I, M, Q }` + `struct Operator`（带 payload 的判别联合）。
- TS 镜像：`app/lib/` 用抽象类 `Operator` + 5 个子类（`TransformOperator`/`KernelOperator`/…），便于渲染层遍历与类型检查。
- 保持 **compiler-parity 逐位比对**（`tests/compiler-parity.test.ts`）。

---

## 3. 存储抽象 + data host bus

### 3.1 抽象接口

存储不直接暴露具体后端。定义 `Storage` trait（Rust）/ `StorageAdapter`（TS）：

```rust
trait Storage {
    fn put_block(&mut self, block: Block) -> Result<(), StorageError>;
    fn put_operator(&mut self, op: Operator) -> Result<(), StorageError>;
    fn put_chain(&mut self, chain: Chain) -> Result<(), StorageError>;
    fn query_neighbors(&self, state: &State, k: usize) -> Result<Vec<State>, StorageError>;
    fn query_fiber(&self, conditioning: &Hash) -> Result<Vec<Chain>, StorageError>;
    fn query_provenance(&self, evidence_id: &str) -> Result<Vec<Id>, StorageError>;
    // ... 16 步里逐步补齐
}
```

### 3.2 后端可替换

| 后端 | 用途 |
|---|---|
| SQLite / MySQL | 关系型主存储（documents/evidence/variables/operators/chains/…） |
| MongoDB | 文档型 JSONB 存储（可选） |
| Milvus | 向量检索（历史近邻实验匹配、表达式语义检索） |

### 3.3 与 data host bus 交互

- 存储作为 **data host bus 上的一个 provider** 暴露：所有读写走 Host SDK operation（`graph.storage.put` / `graph.storage.query.*`），而不是直接调 SQL/驱动。
- Blob 大数据（PDF、表达式 AST）走已有的 `blob.*` 多分块路径，存储只存 `BlobRef`。
- 审计：每次写入经已有 audit 账本（`kernel/audit.rs`）留痕。
- 好处：换后端只换一个 adapter，不改 IR 编译器与抽取器。

---

## 4. 16 步实施（每步 = 一次 commit）

> 步骤顺序取自 `handoff-spec.md` §108，与 `mvp-architecture.md` §33 的 Phase 对齐。**首个可用 MVP 里程碑在第 11 步**（此时已能把论文转成结构化多变量实验证据并判定哪些跨论文效应可计算）。

| # | 步骤 | 对应 § | 产出（commit 内容） | 复用/新建 |
|---|---|---|---|---|
| 1 | ExtractionV3 + CanvasIRV3 数据模型 | 108.1 | Rust serde 结构体 + TS 镜像类型，schema 版本串定稿为 `myc.llm.v4`/`myc.graph-ir.v4` | 新建（替换旧 `canvas-format-v3` 命名冲突） |
| 2 | JSON Schema 校验器 | 108.2 | `validate_extraction`/`validate_ir`，错误码族（VAR-*/REF-*/OP-*/…）+ JSON path | 复用 invariants.rs 模式 |
| 3 | evidence/reference 校验 | 108.3 | Evidence Gate（只 `supported` 进图）+ Reference Resolver（REF-001 悬空引用） | 新建 |
| 4 | concept/unit/expression 规范化 | 108.4 | Concept Canonicalizer + Unit Canonicalizer + Expression Parser | 复用 canonical.rs |
| 5 | State 模型 | 108.5 | baseline/proposed 稀疏配置（缺失 ≠ false） | 新建 |
| 6 | StateDiff | 108.6 | Bool Δ∈{-1,0,+1}、Number Δx、Expression (e0,e1) | 新建 |
| 7 | joint Intervention 编译器 | 108.7 | 多变量 diff ⇒ 单一 `I`（绝不拆独立边） | 新建（关键不变量 V3-06/07） |
| 8 | Block/Operator IR | 108.8 | Block（variable/state/result/concept/axiom）+ Operator（T/K/I/M/Q）+ OP-001..006 | 新建（§2 抽象类落地） |
| 9 | semantic/instance 哈希 | 108.9 | `semantic_hash`（忽略溯源）+ `instance_hash`（含溯源）+ 链递归哈希 | 复用 block_hash 基础设施 |
| 10 | historical matcher | 108.10 | 稀疏布尔距离 + 加权数值距离 + 表达式三级匹配 + 缺失 factorial controls 搜索 | 新建 |
| 11 | identifiability 引擎 | 108.11 | joint/分量/交互状态 + missing_controls（2^k 实验矩阵） | 新建（**里程碑**） |
| 12 | Chain builder | 108.12 | CHAIN-001..006（blocks=operators+1 等） | 新建 |
| 13 | Fiber grouping | 108.13 | 共享 conditioning 的链集合（many-to-many） | 新建 |
| 14 | Bundle grouping | 108.14 | 跨 context 聚合，保留 fiber identity/varying dims/targets | 新建 |
| 15 | consistency checks | 108.15 | path/representation/branch/abstraction/conflict 五类 + 冲突分类（Context→Axiom→Internal） | 新建 |
| 16 | Q candidate 校验 | 108.16 | Compression/PredictionLoss/CommutationError/ConflictIncrease 四指标（只存不自动晋升） | 新建 |

### 4.1 额外前置（在 16 步之外，尽早做）

- **Storage trait + data host bus provider**（§3）：第 1 步之后即可定义 trait，第 8-14 步的 put/query 逐步接入。
- **LLM 抽取管线**（两段式 extract + verify，`mvp-architecture.md` §10、§24）：依赖 `llm_client.rs`，与第 1-3 步并行准备，产出 `ExtractionV3`。
- **edge 收敛（§2）**：在第 8 步（Operator IR）落地抽象类结构，同时把旧画布 `EDGE_TYPES` 迁移/映射。

---

## 5. 验证与验收

- **确定性**：固定 `ExtractionV3 + ontology 版本 + 编译器版本` ⇒ 相同的 canonical 值 / 哈希 / 干预集 / IR 拓扑（V3-20）。
- **逐位比对**：Rust 权威实现 + TS 镜像 + `compiler-parity.test.ts`（沿用现有模式）。
- **20 条硬不变量**（V3-01..V3-20）作为校验器 + 测试断言。
- **10 个测试夹具**（handoff §91–§100：explicit Bool / unknown / Number / Expression / joint intervention / missing control / branch refinement / context conflict / axiom conflict / Q candidate）。
- 每步 commit 时：`cargo test` + 相关 TS 测试 + `vue-tsc` 保持绿。

---

## 6. 首个里程碑与后续

- **第 11 步完成 = 首个可用 MVP**：能把 PINN 论文转成结构化多变量实验证据，并判定跨论文效应的可识别性。
- 第 12–16 步把证据组织成链/纤维/束，并加入一致性检查与 Q 抽象候选校验。
- 之后的非目标（MVP 明确不做）：全自动因果发现、全局真值概率、通用本体、无限图极限、自动理论接受、区块链共识。
