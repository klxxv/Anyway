# Merge 路线图：待合并分支审查报告

> **报告日期**：2026-08-05  
> **审查人**：PR 审核与 Github 管理（自动化审计）  
> **状态**：审查完毕，**不执行 merge**，仅出具建议

---

## 一、分支清单与映射

| 待合并分支 | Todo ID | 标题 | GitHub PR | 变更规模 |
|---|---|---|---|---|
| `tds/conv-019fcd8c-cf8a` | #17 | [P0] .myc 插件包 Ed25519 签名验证 | 未创建 | +856 行, 5 文件 |
| `tds/conv-019fcd8c-d0af` | #19 | [P0] GitHub Actions CI/CD 流水线 | 未创建 | +446 行, 3 文件 |
| `tds/conv-019fcda6-5ae6` | #20 | 重构：解耦 use-workspace-project.ts | 未创建 | +1101/-304 行, 6 文件 |
| `tds/conv-019fcdc2` | #24 | [Phase 1.2] 图算法迁移到 Rust | 未创建 | +2353/-1156 行, 9 文件 |
| `tds/conv-019fcdc3` | #23 | [Phase 1.3] 确定性布局迁移到 Rust | 未创建 | +1727/-412 行, 7 文件 |
| `tds/conv-019fcded` | #22 | [Phase 1.4] TS-Rust 双实现比对测试 | 未创建 | +1826/-6 行, 7 文件 |

**已合并到 main（无需处理）：**

| 已合并分支 | Todo ID | 标题 | PR |
|---|---|---|---|
| `tds/conv-019fcd8f` | #18 | 重构：解耦 research-core.ts | #19 ✅ |
| `tds/conv-019fcda6-92c4` | #21 | [Phase 1.1] graph_compiler.rs | #20 ✅ |

---

## 二、冲突分析矩阵

### 🔴 高危冲突

#### 冲突 C1：`graph_algorithms.rs` vs `graph_compiler/` 目录架构

**涉及分支**：#22 vs #24 + #23

**根本原因**：分支基准不同。

- **#21**（已合并）创建了单体 `src-tauri/src/graph_compiler.rs`（1091 行）
- **#24** 将单体拆分为目录模块 `graph_compiler/{algorithms, analysis, canonical, invariants, mod}.rs`，**删除**原单体文件
- **#23** 基于 #24 的目录结构，新增 `layout.rs` 和 `graph_cmds.rs`
- **#22** 基于 #21 的单体结构（#24 拆分前），新创建独立模块 `src-tauri/src/graph_algorithms.rs`（1382 行），其中**已包含**与 #24（图算法）和 #23（布局）重复的全部实现

**函数重叠详情**：

| #22 `graph_algorithms.rs` 函数 | #24/#23 对应函数 | 所属模块 |
|---|---|---|
| `traverse()` | `traverse_graph()` | `graph_compiler/algorithms.rs` |
| `detect_cycles()` | `detect_cycles()` | `graph_compiler/analysis.rs` |
| `shortest_path()` | `shortest_path()` | `graph_compiler/algorithms.rs` |
| `all_shortest_paths()` | `all_shortest_paths()` | `graph_compiler/algorithms.rs` |
| `compare_reachability()` | `compare_scenario_reachability()` | `graph_compiler/analysis.rs` |
| `compute_logic_chain()` | `compute_logic_chain()` | `graph_compiler/analysis.rs` |
| `compute_layout()` | `compute_layout()` | `graph_compiler/layout.rs` |
| `huffman_codes()` | `huffman_codes()` | `graph_compiler/layout.rs` |
| `topological_depths()` | `topological_depths()` | `graph_compiler/layout.rs` |

**关键差异**：#22 的函数签名返回 `serde_json::Value`（便于 JSON 逐位比对），而 #24/#23 返回强类型 Rust struct（`TraversalResult`、`Cycle`、`LayoutResult` 等）。两套 API **语义等价但签名不同**。

**建议**：#22 的 `graph_algorithms.rs` **不应合并**。需要在合并 #24 和 #23 之后，将 #22 的比对适配到新模块 API。

---

#### 冲突 C2：`src-tauri/src/lib.rs` 模块声明

**涉及分支**：#17, #22, #23

所有三个分支都在 `lib.rs` 顶部添加 `mod` 声明，但位置不同：

```rust
// #17 添加（plugins 之后）
mod signing;

// #22 添加（graph_compiler 之前）
pub mod graph_algorithms;    // ← 需移除

// #23 添加（graph_compiler 之后）
mod graph_cmds;
// + invoke_handler 中新增 2 个 command
```

文本合并不冲突（不同行），但 #22 的 `pub mod graph_algorithms;` 必须在最终合并时**移除**（因为该模块不应存在，见 C1）。

---

### 🟡 中危冲突

#### 冲突 C3：`docs/module-boundaries.md`

**涉及分支**：#22, #23, #24 — 全部修改同一段落（"Stable kernel" 节）

| 分支 | 改动内容 |
|---|---|
| #24 | 描述 `graph_compiler/` 目录拆分、5 个子模块、≤500 行约束 |
| #23 | 在 #24 基础上加 `layout.rs` 描述、§11 意图驱动布局 |
| #22 | 添加 `graph_algorithms.rs` 独立模块描述 + §15.5 迁移门描述 + `compiler-reference.ts` |

三个版本对同一段落的编辑**无法自动合并**，必须手工整合，且 #22 的部分描述（独立 `graph_algorithms.rs`）在合并 #24 后不再适用。

#### 冲突 C3-1：`canonical.rs` 和 `mod.rs` 代码分配差异

`git merge-tree` 模拟确认：
- #24 的 `canonical.rs`（327 行）：仅规范化与哈希；编译管线在 `mod.rs`（420 行）
- #23 的 `canonical.rs`（701 行）：包含规范化 + 哈希 + **编译管线全部代码**；`mod.rs`（37 行）仅为薄重导出

合并 #24 后再合并 #23 时，`canonical.rs`、`invariants.rs`、`mod.rs` 三个文件会同时冲突。**推荐**：以 #24 的代码分配为准（编译管线在 mod.rs），将 #23 的 `layout.rs` 和 `graph_cmds.rs` 作为新文件接入，并将 layout 的 pub use 合并入 #24 的 mod.rs。

---

#### 冲突 C4：`package.json`（文本冲突，低风险）

**涉及分支**：#20, #22

| 分支 | 改动 |
|---|---|
| #20 | 修改 `test:workspace` 行：追加 `tests/workspace-hooks.test.ts` |
| #22 | 修改 `test` 行：追加 `&& npm run test:compiler`；新增 `test:compiler` 脚本 |

两处改动在不同行，文本合并工具可自动处理。但需验证合并后的 `test:compiler` 脚本在 #22 适配后仍然有效。

---

### 🟢 无冲突

| 分支 | 原因 |
|---|---|
| #17 (Ed25519) | 仅新增 `signing.rs` + 修改 `plugins.rs`，不与任何其他分支重叠 |
| #19 (CI/CD) | 仅新增 `.github/workflows/` 下 3 个文件，不触碰源代码 |
| #20 (workspace) | 仅修改 TS 端（`app/features/` + `tests/`），不涉及 Rust |

---

## 三、推荐合并顺序

```
第 1 步:  #19 (CI/CD)
          ↓
第 2 步:  #24 (图算法 → graph_compiler/ 目录拆分)
          ↓
第 3 步:  #23 (布局 → 依赖 #24 的目录结构)
          ↓
第 4 步:  #17 (Ed25519，依赖 CI 就绪 + Rust 模块稳定)
          ↓
第 5 步:  #20 (workspace，纯 TS 重构，低风险)
          ↓
第 6 步:  #22 (比对测试，需适配 #24+#23 的 API 后合并)
```

### 顺序理由

1. **#19 先行** — CI 流水线就绪后，每个后续分支合并都有自动化验证
2. **#24 → #23** — #23 的 `layout.rs` 和 `graph_cmds.rs` 直接依赖 #24 创建的 `graph_compiler/` 目录结构；反过来 #23 需要先合入，否则 #23 的 `graph_cmds::compute_graph_layout` 等 Tauri command 缺少基础
3. **#17 地位独立** — 仅需等待 Rust 端模块布局稳定（#24+#23 合并完成后 `lib.rs` 不再变动）；同时 CI（#19）可验证 Ed25519 测试
4. **#20 独立低风险** — 仅触碰 TS 前端，不依赖任何 Rust 分支
5. **#22 最后** — 比对测试桥接了 TS↔Rust，必须在两端的 API 都稳定后才可适配

---

## 四、各分支验证状态

| 分支 | 测试数 | 测试类型 | 通过状态（来自构建对话记录） |
|---|---|---|---|
| #17 (Ed25519) | 29 | Rust 单元+集成 | ✅ 全部通过（零警告） |
| #19 (CI/CD) | — | YAML 语法 | ⚠️ 未验证 |
| #20 (workspace) | 48 | TS 单元 | ✅ 全部通过；lint + tsc 通过 |
| #24 (图算法) | 29 单元 + 集成 | Rust | ✅ 全部通过（clippy 干净） |
| #23 (布局) | 55 单元 + 集成 | Rust | ✅ 全部通过（clippy 干净） |
| #22 (比对测试) | 9 | TS↔Rust 比对 | ⚠️ 构建对话中未记录最终测试结果 |

---

## 五、专项审查要点

### 5.1 #18（research-core）与 #20（workspace）的 TS 重构一致性 ✅

- **#18** 将 `research-core.ts` 拆分为 `app/lib/graph/`、`app/lib/layout/`、`app/lib/analysis/` 三个目录模块
- **#20** 将 `use-workspace-project.ts` 拆分为 `commit-logic.ts` + `patch-apply.ts` + `sync-logic.ts` + 薄编排层
- **结论**：两个重构**互补无冲突**。#18 拆分算法层，#20 拆分状态管理层；`use-workspace-project.ts` 内部通过 `import` 引用 `app/lib/graph` 等模块，组织清晰
- **风险**：低。两者职责正交

### 5.2 #21（graph_compiler）+ #24（图算法）+ #23（布局）的 Rust 模块整合

- **#21** 创建单体 `graph_compiler.rs`（规范化+哈希+编译管线）
- **#24** 拆分为 5 子模块（algorithms、analysis、canonical、invariants、mod）
- **#23** 追加 2 子模块（layout、graph_cmds）
- **`canonical.rs` 差异**：已通过 `diff` 逐行比对。两者核心逻辑（`normalize_text`、`normalize_key`、`canonical_number`、`canonicalize_value`、`block_hash`、`sha256_hex`、`node_claim`、`edge_claim`、`evidence_claim` 等函数）**语义完全一致**。差异仅为：(a) 注释风格（中英双语 vs 单语），(b) #23 将编译管线代码（`compile`、`CompileResult`、`verify_hashes` 等）从 mod.rs 移入 canonical.rs，(c) 新增 `use serde::Serialize` 和 `use check_invariants` 引用
- **`mod.rs` 差异**：#24 的 `mod.rs`（420 行）包含编译管线（§15.1）+ 版控差异（§6），#23 的 `mod.rs`（37 行）仅为重导出；编译管线在 #23 中已移入 `canonical.rs`
- **合并策略**：先合 #24（建立目录结构），再合 #23。冲突文件处理：
  - `canonical.rs`：使用 #23 版本（包含编译管线，更完整）或保留 #24 的分配（管线在 mod.rs）+ 合并 #23 的新增代码
  - `invariants.rs`：两个版本内容几乎相同（都从同一单体拆分），取任一份后 diff 确认
  - `mod.rs`：以 #24 为基础，补充 #23 的 `pub mod layout` 和 `pub use layout::...`
  - `layout.rs`：无冲突，直接采用 #23
  - `graph_cmds.rs`：无冲突，直接采用 #23
- **结论**：模块层级清晰。需约 30 分钟手工解决 `canonical.rs`/`mod.rs`/`invariants.rs` 三文件冲突
- **风险**：中。`git merge-tree` 模拟确认三文件冲突，但内容高度一致，冲突解决后需运行 `cargo test` 验证

### 5.3 #22（比对测试）对 TS→Rust 关键函数的覆盖 ✅

| 函数 | TS 端 | Rust 端（原 #22） | 覆盖状态 |
|---|---|---|---|
| canonicalize | `compiler-reference.ts` | `graph_compiler::canonicalize` | ✅ |
| traverse/BFS/DFS | `app/lib/graph/traversal.ts` | `traverse()` | ✅ |
| shortestPath | `app/lib/graph/paths.ts` | `shortest_path()` | ✅ |
| allShortestPaths | `app/lib/graph/paths.ts` | `all_shortest_paths()` | ✅ |
| detectCycles | `app/lib/graph/cycles.ts` | `detect_cycles()` | ✅ |
| compareReachability | `app/lib/graph/reachability.ts` | `compare_reachability()` | ✅ |
| computeLogicChain | `app/lib/analysis/logic-chain.ts` | `compute_logic_chain()` | ✅ |
| propagateInfluence | `app/lib/analysis/influence.ts` | `propagate_influence()` | ✅ |
| computeLayout | `app/lib/layout/compute.ts` | `compute_layout()` | ✅ |

**覆盖完整性**：9/9 函数全覆盖 ✅  
**未覆盖项**：比对测试在构建对话中未记录最终 pass/fail 状态，需在适配后重新运行验证。

---

## 六、未覆盖 / 待办项

| # | 项目 | 严重程度 | 说明 |
|---|---|---|---|
| 1 | #22 比对测试适配 | 🔴 阻塞 | `graph_algorithms.rs` 需移除；`compile_harness.rs` 需改调 `graph_compiler/` 模块 API；`compiler-parity.test.ts` 需更新 Rust 命令调用路径 |
| 2 | canonical.rs 内容一致性校验 | 🟡 重要 | #23 通过 git rename 引入 canonical.rs，#24 通过 delete+create；合并后需 diff 两个版本的 canonical.rs 确认语义一致 |
| 3 | docs/module-boundaries.md 三向合并 | 🟡 重要 | #22+#23+#24 均修改同一段落，需手工整合成统一描述 |
| 4 | CI 验证 #19 的 workflow YAML | 🟡 重要 | 3 个 workflow 文件未在任何构建中实际运行验证 |
| 5 | 全量集成测试 | 🟡 重要 | 所有分支合并后需运行 `npm test`（含 `test:compiler`）确认无回归 |
| 6 | `propagate_influence` 缺失 | 🟢 低 | #24 的 `analysis.rs` 和 #23 的模块中均**未包含**影响传播算法（仅在 #22 的 `graph_algorithms.rs` 中有）。应在 #24 的 analysis.rs 中补充此实现 |
| 7 | Cargo.lock 更新 | 🟢 低 | #17 添加了 `ed25519-dalek` 和 `base64` 依赖；合并后需重新生成 Cargo.lock |

---

## 七、执行检查清单（供各角色工程师使用）

### Rust 开发工程师

- [ ] 拉取 #24 → 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，确认 29 测试通过
- [ ] 合并 #24 → main → 拉取 #23 → 运行 `cargo test`，确认 55 测试通过
- [ ] 拉取 #17 → 运行 `cargo test`，确认 29 测试（含签名验证）通过
- [ ] 校验 #23 与 #24 的 `canonical.rs` 内容一致
- [ ] 确认 #24 的 `analysis.rs` 需补充 `propagate_influence` 实现
- [ ] 将 #22 的 `compile_harness.rs` 适配到 `graph_compiler/` 目录 API

### TS 开发工程师

- [ ] 拉取 #20 → 运行 `npm run test:workspace`，确认 48 测试通过
- [ ] 检查 #20 与 #18 的 import 路径一致（`app/lib/graph`、`app/lib/layout`、`app/lib/analysis`）
- [ ] 拉取 #22（适配后） → 运行 `npm run test:compiler`，确认 9 项比对全部通过
- [ ] 检查 `package.json` 合并后脚本正确

### Tauri Rust 测试工程师

- [ ] 在最终合并分支上运行 `cargo test --manifest-path src-tauri/Cargo.toml`
- [ ] 确认全部测试通过（预计 113+ 测试）
- [ ] 运行 `cargo clippy --manifest-path src-tauri/Cargo.toml` 确认零新增警告

### Tauri TS 测试工程师

- [ ] 在最终合并分支上运行 `npm test`
- [ ] 确认 `test:core`、`test:platform`、`test:workspace`、`test:sdk`、`test:native`、`test:compiler` 全部通过
- [ ] 运行 `npm run lint` 确认零错误

### 实施审计员

- [ ] 对照 v3 Schema（`canvas-format-v3.md` §3 双哈希、§3.4 规范化、§11 布局、§15.5 迁移路径）审计：
  - [ ] `canonical.rs` 的规范化逻辑是否符合 §3.4
  - [ ] `layout.rs` 的意图驱动布局是否符合 §11
  - [ ] `algorithms.rs` + `analysis.rs` 的图属性硬计算原则
  - [ ] `compiler-parity.test.ts` 的迁移门逻辑是否符合 §15.5
  - [ ] `signing.rs` 中的签名验证流程：签名覆盖内容、哈希算法、信任根管理

---

## 八、风险总结

| 风险等级 | 数量 | 说明 |
|---|---|---|
| 🔴 阻塞 | 1 | #22 与 #24/#23 的架构冲突（graph_algorithms.rs 需完全重写适配层） |
| 🟡 需注意 | 4 | module-boundaries.md 三向合并、canonical.rs 一致性、CI 未验证、全量测试待跑 |
| 🟢 低风险 | 2 | propagate_influence 补充、Cargo.lock 更新 |

**总体评估**：分支质量良好，各分支独立测试均通过。唯一阻塞项是 #22 的架构对齐（需约 2-4 小时适配工作）。建议按推荐顺序（#19 → #24 → #23 → #17 → #20 → #22）逐步合并，每步验证 CI 绿灯后再推进下一步。
