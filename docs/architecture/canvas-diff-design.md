# Canvas Diff 算法——内核结构 diff

> **版本**: v1.0 | **日期**: 2026-08-06 | **模块**: `src-tauri/src/graph_compiler/diff.rs`
> **所属层**: 内核层（Rust graph_compiler） | **目标行数**: ≤500

---

## §1 设计背景与约束

### 1.1 两层 Diff 模型定案

Research Canvas 的变更追踪模型分为两个正交层次：

| 层 | 名称 | 实现 | 职责 |
|---|------|------|------|
| **内核层** | Canvas Diff | Rust `graph_compiler::diff` | 确定性结构比较，二进制安全 |
| **Agent 层** | Cross-Canvas Diff | LLM（语义 diff） | 识别跨图谱语义等价/矛盾，提议跨图边 |

**内核层 Canvas Diff** 是本设计文档的主题。它的入参与出参为：

```
Canvas Diff: (ProjectState_v1, ProjectState_v2) → CanvasDiffResult
```

其中 `CanvasDiffResult` 输出 `{ addedNodes, removedNodes, modifiedNodes, changedBlockHashes, addedEdges, removedEdges }`。不做语义理解，只比较结构。

### 1.2 约束条件矩阵

| 约束 | 说明 |
|------|------|
| **确定性** | 相同输入永远产生相同输出，无随机性、无时间戳依赖 |
| **幂等性** | `diff(P, P) = 空 diff`，同项目的比较结果不产生任何变更 |
| **对称性** | `diff(A, B)` 与 `diff(B, A)` 的 added/removed 互换，modified 的 old/new 互换 |
| **传递性（部分）** | 若 `diff(A, B)` + `diff(B, C)` 不冲突，则应与 `diff(A, C)` 兼容 |
| **性能** | 10k 节点 + 5k 边 + 3k 证据 < 200ms |
| **内存** | 峰值 < 5MB 额外分配 |
| **复杂度** | O(N log N)，N 为三区实体总数 |

### 1.3 分区语义回顾

参照 `canvas-format-v3` 和现有 `graph_compiler.rs`：

- **① 语义区（semantic zone）**：`nodes` + `edges` + `evidence`，由 `contentRootHash` 覆盖
- **② 布局区（layout zone）**：`placements`，仅影响 `fileHash`
- **③ 元数据/场景区**：`title`、`discipline`、`scenarios`、`navigation`、`activity`、时间戳，仅影响 `fileHash`

**Canvas Diff 仅比较 ① 语义区**（nodes、edges、evidence）。布局区和元数据区的变化由 `fileHash` 的比较捕获（属于编辑级联自校验，不在 diff 算法范围）。

---

## §2 数据结构定义

### 2.1 Rust 端（`graph_compiler::diff`）

```rust
/// Canvas Diff 结果——内核层结构比较的完整产物。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasDiffResult {
    /// 新增节点（v2 有、v1 无）
    pub added_nodes: Vec<String>,
    /// 移除节点（v1 有、v2 无）
    pub removed_nodes: Vec<String>,
    /// 修改节点（id 相同但 blockHash 不同）
    pub modified_nodes: Vec<ModifiedEntity>,

    /// 新增边
    pub added_edges: Vec<String>,
    /// 移除边
    pub removed_edges: Vec<String>,
    /// 修改边
    pub modified_edges: Vec<ModifiedEntity>,

    /// 新增证据
    pub added_evidence: Vec<String>,
    /// 移除证据
    pub removed_evidence: Vec<String>,
    /// 修改证据
    pub modified_evidence: Vec<ModifiedEntity>,

    /// changedBlockHashes：entityId → (oldHash, newHash)
    /// 覆盖 added（old=""）、removed（new=""）、modified（两者不同）。
    pub changed_block_hashes: HashMap<String, (String, String)>,

    /// Diff 计算耗时（毫秒）
    pub duration_ms: u64,
}

/// 一个被修改的实体。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifiedEntity {
    pub entity_id: String,
    /// 实体类别：node / edge / evidence
    pub entity_kind: String,
    pub old_block_hash: String,
    pub new_block_hash: String,
    /// 具体变更的字段列表（可选，用于前端精确高亮）
    pub changed_fields: Vec<String>,
}

/// 差异块的精细描述（向前端渲染器提供 git-diff-hunk 风格信息）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub entity_id: String,
    pub entity_kind: String,
    pub operation: DiffOperation,
    pub old_block_hash: String,
    pub new_block_hash: String,
    pub changed_fields: Vec<FieldChange>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffOperation {
    Added,
    Removed,
    Modified,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

/// 比较粒度控制。
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffGranularity {
    /// 仅比较 blockHash（默认，最快）
    BlockHash,
    /// 比较到字段级（较慢但提供精确变更信息）
    FieldLevel,
}
```

### 2.2 TypeScript 侧契约

```typescript
// 与 Rust `CanvasDiffResult` 对应的 TS 类型
export interface CanvasDiffResult {
  addedNodes: string[];
  removedNodes: string[];
  modifiedNodes: ModifiedEntity[];

  addedEdges: string[];
  removedEdges: string[];
  modifiedEdges: ModifiedEntity[];

  addedEvidence: string[];
  removedEvidence: string[];
  modifiedEvidence: ModifiedEntity[];

  changedBlockHashes: Record<string, [string, string]>;

  durationMs: number;
}

export interface ModifiedEntity {
  entityId: string;
  entityKind: "node" | "edge" | "evidence";
  oldBlockHash: string;
  newBlockHash: string;
  changedFields: string[];
}

export interface DiffHunk {
  entityId: string;
  entityKind: string;
  operation: "added" | "removed" | "modified";
  oldBlockHash: string;
  newBlockHash: string;
  changedFields: FieldChange[];
}

export interface FieldChange {
  field: string;
  oldValue?: string;
  newValue?: string;
}
```

---

## §3 算法设计

### 3.1 总体流程图

```
┌─────────────────────────────────────────────────────────────┐
│ 输入: (ProjectState_v1, ProjectState_v2, DiffGranularity)     │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 0: 提取 ① 语义区实体（nodes/edges/evidence）               │
│         → Vec<Entity> from v1, Vec<Entity> from v2            │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 1: 计算 blockHash（复用 compute_block_hashes 原语）        │
│         → HashMap<entityId, blockHash(12 hex)> for v1 & v2    │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 2: id 集合求差（三个实体类别分治、可并行）                   │
│         added_ids   = ids_v2 - ids_v1                         │
│         removed_ids = ids_v1 - ids_v2                         │
│         common_ids  = ids_v1 ∩ ids_v2                         │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 3: 对 common_ids 用 blockHash 比较                     │
│         若 hash_v1 == hash_v2 → 无变化                        │
│         若 hash_v1 != hash_v2 → 归入 modified                 │
│         [可选] granularity=FieldLevel → 逐字段比较             │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 4: 构建 changedBlockHashes                              │
│         added:    (id) → ("", newHash)                        │
│         removed:  (id) → (oldHash, "")                        │
│         modified: (id) → (oldHash, newHash)                   │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 5: 排序（BTreeSet -> Vec）、计时、产出 CanvasDiffResult   │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 核心算法——Algorithm A: `canvas_diff`

```
Algorithm A: canvas_diff(v1, v2, granularity)
─────────────────────────────────────────────
Input:
  v1: ProjectState (基图)
  v2: ProjectState (目标图)
  granularity: DiffGranularity (BlockHash | FieldLevel)

Output: CanvasDiffResult

1. start ← now()
2. for kind in ["nodes", "edges", "evidence"]:
3.   hashes_v1[kind] ← compute_block_hashes_for(v1, kind)
4.   hashes_v2[kind] ← compute_block_hashes_for(v2, kind)
5.
6.   ids_v1 ← keys(hashes_v1[kind])
7.   ids_v2 ← keys(hashes_v2[kind])
8.
9.   added[kind]   ← sorted(ids_v2 - ids_v1)
10.  removed[kind] ← sorted(ids_v1 - ids_v2)
11.  common         ← ids_v1 ∩ ids_v2
12.
13.  for each id in common:
14.    h1 ← hashes_v1[kind][id]
15.    h2 ← hashes_v2[kind][id]
16.    if h1 != h2:
17.      modified[kind] ← push ModifiedEntity(id, kind, h1, h2, …)
18.
19. Build changedBlockHashes from added/removed/modified
20.
21. return CanvasDiffResult{
22.   added[kind], removed[kind], modified[kind],
23.   changedBlockHashes, duration_ms: elapsed(start)
24. }
```

### 3.3 可选的字段级 diff——Algorithm B: `diff_hunks`

```
Algorithm B: diff_hunks(v1, v2, modified_entities)
─────────────────────────────────────────────────
Input:
  v1, v2: ProjectState
  modified_entities: 已通过 blockHash 识别出的 modified 实体列表

Output: Vec<DiffHunk>

1. hunks ← []
2. for each m in modified_entities:
3.   e1 ← find_entity_in(v1, m.entity_kind, m.entity_id)
4.   e2 ← find_entity_in(v2, m.entity_kind, m.entity_id)
5.   changed_fields ← []
6.   for each (key, val1) in claim(e1):
7.     val2 ← e2[key] // or absent
8.     if val1 != val2:
9.       changed_fields.push FieldChange(key, val1, val2)
10.  hunks.push DiffHunk(m.entity_id, m.entity_kind, Modified,
11.                      m.old_block_hash, m.new_block_hash, changed_fields)
12.
13. for each id in added[kind]:
14.   hunks.push DiffHunk(id, kind, Added, "", newHash, [])
15. for each id in removed[kind]:
16.   hunks.push DiffHunk(id, kind, Removed, oldHash, "", [])
17.
18. return hunks
```

### 3.4 五项关键设计决策

| ID | 决策 | 理由 |
|----|------|------|
| **D1** | 基于 blockHash 而非逐字段递归 JSON 比较 | 复用现有 `block_hash()` / `compute_block_hashes()` 原语；相同 hash = 相同内容，判断 O(1) |
| **D2** | 三个实体类别独立 diff | `nodes` / `edges` / `evidence` 各自计算 added/removed/modified，逻辑清晰、可并行 |
| **D3** | `changedBlockHashes` 作为单一数据源 | 不仅是 modified 实体，added（old=""）和 removed（new=""）也记录；可直接驱动 GraphPatch 生成 |
| **D4** | 全部使用 `BTreeSet` 排序 | 确定性保证的核心手段；输出次序不依赖输入次序 |
| **D5** | 零侵入集成 | `diff.rs` 纯新增模块，`graph_compiler.rs` 无需改动，仅通过 `use crate::graph_compiler::*` 导入已有原语 |

### 3.5 八种边界情况

| # | 情况 | 处理方式 |
|---|------|---------|
| B1 | v1 或 v2 缺少 `nodes`/`edges`/`evidence` 键 | 视为空数组 `[]` |
| B2 | 实体无 `id` 字段 | 以 JSON 路径标注警告，跳过该实体 |
| B3 | v1 和 v2 完全一致 | 所有 added/removed/modified 均为空，`changedBlockHashes` 为空 |
| B4 | 实体 id 相同但 `type` 变化 | `type` 属于 claim 字段，blockHash 必然变化 → 归入 modified |
| B5 | 实体 data 字段包含嵌套对象 | `canonicalize` 已递归排序键，blockHash 已捕获差异 |
| B6 | v1 为 `null` 或非对象 | 返回错误结果（`diff_error`），不 panic |
| B7 | 超大项目（> 100k 实体） | 返回错误，防止 OOM |
| B8 | evidenceIds 变化但主张不变 | `evidenceIds` 是悬挂字段，不入 blockHash → 不产生 diff |

---

## §4 API 签名

### 4.1 三个公开函数

```rust
// src-tauri/src/graph_compiler/diff.rs

/// 两个 ProjectState 的完整结构差异（默认 blockHash 级别）。
pub fn canvas_diff(
    v1: &serde_json::Value,
    v2: &serde_json::Value,
) -> CanvasDiffResult;

/// 带粒度控制的 diff：FieldLevel 慢但提供精确字段变更信息。
pub fn canvas_diff_with_granularity(
    v1: &serde_json::Value,
    v2: &serde_json::Value,
    granularity: DiffGranularity,
) -> CanvasDiffResult;

/// 从 CanvasDiffResult 生成 git-diff hunk 风格的结构化变更列表。
pub fn diff_hunks(
    v1: &serde_json::Value,
    v2: &serde_json::Value,
    result: &CanvasDiffResult,
) -> Vec<DiffHunk>;
```

### 4.2 Tauri command 暴露

```rust
// src-tauri/src/lib.rs 或 projects.rs 中添加

#[tauri::command]
async fn diff_projects(
    project_v1: serde_json::Value,
    project_v2: serde_json::Value,
) -> Result<CanvasDiffResult, String> {
    Ok(graph_compiler::diff::canvas_diff(&project_v1, &project_v2))
}

#[tauri::command]
async fn diff_projects_with_hunks(
    project_v1: serde_json::Value,
    project_v2: serde_json::Value,
) -> Result<Vec<DiffHunk>, String> {
    let result = graph_compiler::diff::canvas_diff(&project_v1, &project_v2);
    Ok(graph_compiler::diff::diff_hunks(&project_v1, &project_v2, &result))
}
```

### 4.3 TypeScript 调用接口

```typescript
// app/lib/graph/diff.ts（TS 侧包装）

import { invoke } from "@tauri-apps/api/core";
import type { CanvasDiffResult, DiffHunk } from "./research-types";

export async function diffProjects(
  v1: ProjectState,
  v2: ProjectState,
): Promise<CanvasDiffResult> {
  return invoke<CanvasDiffResult>("diff_projects", {
    projectV1: v1,
    projectV2: v2,
  });
}

export async function diffProjectsDetailed(
  v1: ProjectState,
  v2: ProjectState,
): Promise<DiffHunk[]> {
  return invoke<DiffHunk[]>("diff_projects_with_hunks", {
    projectV1: v1,
    projectV2: v2,
  });
}
```

---

## §5 输出格式与渲染

### 5.1 git-diff hunk 风格

对 `diff_hunks()` 产出的 `DiffHunk` 列表，可渲染为类 git-diff 风格文本：

```
diff --canvas project:example
--- a/project  (base)
+++ b/project  (target)

@@ nodes @@
+ added:  node:n3  Hypothesis   "新假设"        [a1b2c3d4e5f6]
- removed: node:n5  Note         "旧笔记"        [f6e5d4c3b2a1]
~ modified: node:n2 Concept     "先验约束"       [abc123 → def456]
    field: body: "物理守恒" → "物理约束与守恒律"

@@ edges @@
+ added:   edge:x3  supports   n3 → n1          [111111111111]
~ modified: edge:x1 supports   n1 → n2          [aaa111 → bbb222]
    field: confidence: 0.9 → 0.85

@@ evidence @@
- removed: evidence:e2 paper   "旧论文"          [eeee22222222]
```

### 5.2 结构化 JSON

```json
{
  "addedNodes": ["n3"],
  "removedNodes": ["n5"],
  "modifiedNodes": [
    {
      "entityId": "n2",
      "entityKind": "node",
      "oldBlockHash": "abc123def456",
      "newBlockHash": "def456abc123",
      "changedFields": ["body"]
    }
  ],
  "addedEdges": ["x3"],
  "removedEdges": [],
  "modifiedEdges": [
    {
      "entityId": "x1",
      "entityKind": "edge",
      "oldBlockHash": "aaa111bbb222",
      "newBlockHash": "bbb222aaa111",
      "changedFields": ["confidence"]
    }
  ],
  "addedEvidence": [],
  "removedEvidence": ["e2"],
  "modifiedEvidence": [],
  "changedBlockHashes": {
    "n3": ["", "a1b2c3d4e5f6"],
    "n5": ["f6e5d4c3b2a1", ""],
    "n2": ["abc123def456", "def456abc123"],
    "x3": ["", "111111111111"],
    "x1": ["aaa111bbb222", "bbb222aaa111"],
    "e2": ["eeee22222222", ""]
  },
  "durationMs": 42
}
```

### 5.3 作为 GraphPatch 输入

`changedBlockHashes` 可直接驱动 `PluginGraphPatch` 生成：

```typescript
function buildPatchFromDiff(diff: CanvasDiffResult): PluginGraphPatch {
  const ops: GraphPatchOperation[] = [];
  for (const [id, [oldHash, newHash]] of Object.entries(diff.changedBlockHashes)) {
    if (oldHash === "") ops.push({ op: "add-node", node: { id, type: "...", title: "..." }});
    else if (newHash === "") ops.push({ op: "delete-node", nodeId: id });
    else ops.push({ op: "update-node", nodeId: id, changes: { /* field-level patch */ }});
  }
  return {
    apiVersion: "researchcanvas.dev/graph-patch/v1alpha1",
    source: { pluginId: "graph-compiler", operation: "canvas-diff" },
    title: "Canvas Diff Patch",
    summary: `${ops.length} operations from structural diff`,
    reviewRequired: true,
    operations: ops,
  };
}
```

---

## §6 不变式保证

### 6.1 确定性（Determinism）

**定理**：`canvas_diff(P, Q)` 仅依赖 `compute_block_hashes` 和集合运算。

- `compute_block_hashes` → `block_hash` → `canonicalize` → 纯函数，无时间/随机依赖
- 集合运算 `HashMap` + `BTreeSet` 排序 → 输出顺序由 `Ord` 决定，与插序无关
- ∵ 同输入 ⇒ 同 blockHash 映射 ⇒ 同集合求差 ⇒ 同排序结果

### 6.2 幂等性（Idempotence）

**定理**：`canvas_diff(P, P)` 的 `added` / `removed` / `modified` 及 `changedBlockHashes` 均为空。

**证明**：
- `ids_v1 = ids_v2`（相同集合）⇒ `added = ∅`, `removed = ∅`
- `hashes_v1 = hashes_v2`（相同内容）⇒ 对所有 `id ∈ common`，`h1 == h2` ⇒ `modified = ∅`
- ∴ `changedBlockHashes = {}`

### 6.3 对称性（Symmetry）

**定理**：`diff(A, B).added = diff(B, A).removed`，`diff(A, B).removed = diff(B, A).added`，modified 的 old/new hash 互换。

**证明**：集合运算的对称性：
- `ids_v2 - ids_v1`（in `diff(A,B)`) = `ids_v1 - ids_v2`（in `diff(B,A)`）

### 6.4 部分传递性

**定理**：若无冲突修改（同一实体在 `diff(A,B)` 和 `diff(B,C)` 中都 modified 但字段不重叠或无冲突），则 `diff(A, C)` 的实体变更集 = `diff(A, B)` ⊕ `diff(B, C)`（对称差分）。

**注意**：冲突修改时（同一实体在两个 diff 中修改了同一字段到不同值），不保证传递性。

---

## §7 性能分析

### 7.1 复杂度分析

| 操作 | 复杂度 | 说明 |
|------|--------|------|
| `compute_block_hashes` | O(N) | N = 实体总数，每个实体一次 canonicalize + sha256 |
| 集合求差（HashSet） | O(N) | 插入 + 查找 O(1)，总 O(N) |
| BTreeSet 排序输出 | O(K log K) | K = added/removed 数量，K ≤ N |
| `changedBlockHashes` 构建 | O(N) | 遍历三区 |
| FieldLevel diff（可选） | O(N·F) | F = 平均字段数，仅在 granularity=FieldLevel 时触发 |
| **总计** | **O(N + K log K)** ≈ **O(N log N)** | |

### 7.2 规模测试预估

| 规模 | 节点 | 边 | 证据 | 预估耗时 | 内存 |
|------|------|-----|------|---------|------|
| 小型 | 100 | 50 | 30 | < 1ms | < 100KB |
| 中型 | 1,000 | 500 | 300 | < 5ms | < 500KB |
| 大型 | 10,000 | 5,000 | 3,000 | < 200ms | < 5MB |
| 临界 | 100,000 | — | — | 拒绝（返回错误） | — |

### 7.3 可选的优化技术

| 技术 | 场景 | 收益 |
|------|------|------|
| **彩虹表缓存** | 同一实体集合多次 diff | 若 `contentRootHash(v1) == contentRootHash(v2)`，直接返回空 diff，跳过逐实体比较 |
| **并行化（rayon）** | 三个实体类别并行处理 | 在 N > 1000 时约 2.5-3x 加速 |
| **增量 diff** | Git 式版本链 | 每版本存储 blockHash 快照，diff 时比较最近公共祖先 |

---

## §8 测试策略

### 8.1 四层测试金字塔

```
         ┌─────────────┐
         │  E2E (Tauri) │ ← 1 个: Tauri command 端到端
         ├─────────────┤
         │ 对比测试      │ ← 3 个: TS↔Rust 双实现比对
         ├─────────────┤
         │ 集成测试      │ ← 4 个: MNIST/社交 基元图谱
         ├─────────────┤
         │ 单元测试      │ ← 7 个: 纯函数 + 不变式证明
         └─────────────┘
```

### 8.2 15 个测试用例

| # | 层 | 名称 | 输入 | 期望 |
|---|-----|------|------|------|
| T1 | 单元 | 空项目 diff | `{}`, `{}` | 无变更 |
| T2 | 单元 | 单节点新增 | v1: `[]`, v2: `[n1]` | addedNodes: `["n1"]` |
| T3 | 单元 | 单节点删除 | v1: `[n1]`, v2: `[]` | removedNodes: `["n1"]` |
| T4 | 单元 | 单节点修改 | title 变化 | modifiedNodes: `[n1]` |
| T5 | 单元 | 幂等性 | v1 == v2 (MNIST) | 全部为空 |
| T6 | 单元 | 对称性 | A→B, B→A | added↔removed 互换 |
| T7 | 单元 | evidenceIds 不变 | 仅 evidenceIds 变化 | modified 为空 |
| T8 | 集成 | MNIST base vs 添加节点 | 在 MNIST fixture 上增 1 节点 | 1 added |
| T9 | 集成 | MNIST base vs 删除边 | 删除一条 supports 边 | 1 removed edge |
| T10 | 集成 | 社交图 base vs 修改实验 | 修改 experiment.metric | 1 modified edge |
| T11 | 集成 | 混合变更 | 增 2 节点 + 删 1 边 + 改 1 证据 | 三类变更均检出 |
| T12 | 对比 | TS↔Rust MNIST | 同一对 MNIST 图 | 结果 bit-identical |
| T13 | 对比 | TS↔Rust 社交图 | 同一对社会图 | 结果 bit-identical |
| T14 | 对比 | TS↔Rust 边界情况 | B1-B8 的 TS 实现 | 结果 bit-identical |
| T15 | E2E | Tauri command | 通过 `invoke("diff_projects")` | 返回正确 JSON |

### 8.3 TS 双实现比对方案

在 `app/lib/graph/diff.ts` 中维护纯 TypeScript 版本（不含 Tauri 调用），用于比对测试：

```typescript
// 测试辅助：比对 Rust 和 TS 实现的 diff 结果
export function assertBitIdentical(
  rustResult: CanvasDiffResult,
  tsResult: CanvasDiffResult,
  label: string,
): void {
  const deepCompare = (a: unknown, b: unknown): boolean =>
    JSON.stringify(a) === JSON.stringify(b);

  if (!deepCompare(rustResult, tsResult)) {
    throw new Error(
      `Bit-identical assertion failed for "${label}".\n` +
      `Rust: ${JSON.stringify(rustResult, null, 2)}\n` +
      `TS:   ${JSON.stringify(tsResult, null, 2)}`
    );
  }
}
```

---

## §9 与 graph_compiler 集成

### 9.1 模块布局

```
src-tauri/src/
├── graph_compiler.rs       # 已有：规范化、双哈希、不变式、编译管线
├── graph_compiler/
│   └── diff.rs             # 新增（本设计文档目标）
├── lib.rs
├── main.rs
├── ...
```

### 9.2 `diff.rs` 结构规划（≤500 行）

```
diff.rs (~480 lines)
├── §1 数据结构     (~100 lines) CanvasDiffResult, ModifiedEntity, DiffHunk, DiffGranularity, etc.
├── §2 辅助函数     (~60 lines)  extract_semantic_zone, compare_claims, get_claim_fields
├── §3 核心算法     (~150 lines) canvas_diff, diff_single_category, build_changed_block_hashes
├── §4 diff_hunks  (~100 lines) diff_hunks, field_diff, serialize_claim_value
└── §5 测试         (~70 lines)  #[cfg(test)] mod tests { … }
```

### 9.3 与现有原语的集成关系

```rust
// diff.rs 只引用 graph_compiler 的公开符号
use crate::graph_compiler::{
    block_hash,              // 已有
    compute_block_hashes,    // 已有
    node_claim,              // 已有
    edge_claim,              // 已有
    evidence_claim,          // 已有
    // canonicalize 和 sha256_hex 为 crate-private，diff.rs 不需直接使用
};
```

**graph_compiler.rs 无需任何改动**——`diff.rs` 是完全的新增模块，通过 `pub mod diff;` 挂入。

### 9.4 编译管线中的位置

```
┌────────────────────────────────────────────────┐
│ graph_compiler 编译管线                         │
│                                                │
│ 1. check_invariants(v) → Vec<Violation>        │
│ 2. compute_block_hashes(v) → HashMap<id, hash> │
│ 3. content_root_hash(v) → String               │
│ 4. file_hash(v) → String                       │
│ 5. canvas_diff(v1, v2) → CanvasDiffResult  ← ✨│
│ 6. compile(v) → CompileResult                  │
│ 7. verify_hashes(v) → VerifyResult             │
└────────────────────────────────────────────────┘
```

### 9.5 CLI 集成点

在 `canvas compile` CLI 中增加 `diff` 子命令：

```
canvas diff base.project.json target.project.json
canvas diff base.project.json target.project.json --field-level
canvas diff base.project.json target.project.json --hunks
```

---

## §10 附录

### 10.1 Algorithm A 完整伪代码

```
canvas_diff(v1: ProjectState, v2: ProjectState, granularity: DiffGranularity)
────────────────────────────────────────────────────────────────────────
CONST CATEGORIES = ["nodes", "edges", "evidence"]

01:  start ← now()
02:  result ← CanvasDiffResult::default()
03:
04:  for cat in CATEGORIES:
05:    entities_v1 ← get_entities(v1, cat)  // 若无则 []
06:    entities_v2 ← get_entities(v2, cat)
07:
08:    hashes_v1 ← Map<id, hash>  // 由 compute_block_hashes 子集得出
09:    hashes_v2 ← Map<id, hash>
10:
11:    for e in entities_v1:
12:      if has_id(e):
13:        hashes_v1[e.id] ← block_hash(claim(e))
14:    for e in entities_v2:
15:      if has_id(e):
16:        hashes_v2[e.id] ← block_hash(claim(e))
17:
18:    ids_v1 ← Set(keys(hashes_v1))
19:    ids_v2 ← Set(keys(hashes_v2))
20:
21:    added_ids   ← ids_v2 - ids_v1
22:    removed_ids ← ids_v1 - ids_v2
23:    common_ids  ← ids_v1 ∩ ids_v2
24:
25:    added[cat]   ← sorted(added_ids)
26:    removed[cat] ← sorted(removed_ids)
27:
28:    modified_list ← []
29:    for id in common_ids:
30:      h1 ← hashes_v1[id]
31:      h2 ← hashes_v2[id]
32:      if h1 != h2:
33:        changed_fields ← []
34:        if granularity == FieldLevel:
35:          e1 ← find_entity_by_id(entities_v1, id)
36:          e2 ← find_entity_by_id(entities_v2, id)
37:          changed_fields ← diff_claim_fields(claim(e1), claim(e2))
38:        modified_list.push(ModifiedEntity{id, cat, h1, h2, changed_fields})
39:
40:    modified[cat] ← modified_list
41:
42:  // 构建 changedBlockHashes
43:  for cat, ids in added:
44:    for id in ids: ch[id] ← ("", hashes_v2[id])
45:  for cat, ids in removed:
46:    for id in ids: ch[id] ← (hashes_v1[id], "")
47:  for cat, entities in modified:
48:    for m in entities: ch[m.entity_id] ← (m.old_block_hash, m.new_block_hash)
49:
50:  result.changed_block_hashes ← ch
51:  result.duration_ms ← elapsed(start)
52:
53:  return result
```

### 10.2 Algorithm B 完整伪代码

```
diff_hunks(v1, v2, result: CanvasDiffResult)
───────────────────────────────────────────
01:  hunks ← []
02:
03:  // added
04:  for cat, ids in [("node", result.added_nodes), …]:
05:    for id in ids:
06:      new_hash ← get_block_hash(v2, cat, id)
07:      hunks.push(DiffHunk{id, cat, Added, "", new_hash, []})
08:
09:  // removed
10:  for cat, ids in [("node", result.removed_nodes), …]:
11:    for id in ids:
12:      old_hash ← get_block_hash(v1, cat, id)
13:      hunks.push(DiffHunk{id, cat, Removed, old_hash, "", []})
14:
15:  // modified
16:  for cat, entities in [("node", result.modified_nodes), …]:
17:    for m in entities:
18:      e1 ← find_entity(v1, m.entity_kind, m.entity_id)
19:      e2 ← find_entity(v2, m.entity_kind, m.entity_id)
20:      field_changes ← diff_claim_fields(e1, e2)
21:      hunks.push(DiffHunk{
22:        m.entity_id, m.entity_kind, Modified,
23:        m.old_block_hash, m.new_block_hash, field_changes
24:      })
25:
26:  return hunks
```

### 10.3 与现有 `graph_compiler.rs` 中 claim 函数的关系

```
diff.rs 使用的原语              graph_compiler.rs 提供
─────────────────────────────────────────────────
block_hash(claim)              pub fn block_hash(content: &Value) -> String
node_claim(node)               pub fn node_claim(node: &Value) -> Value
edge_claim(edge)               pub fn edge_claim(edge: &Value) -> Value
evidence_claim(evidence)       pub fn evidence_claim(evidence: &Value) -> Value
compute_block_hashes(project)  pub fn compute_block_hashes(project: &Value) -> HashMap<String, String>
```

注意：`canonicalize` 和 `sha256_hex` 是 crate-private 函数，diff.rs 通过 `block_hash` 间接使用它们，不直接引用——遵循最小暴露原则。
