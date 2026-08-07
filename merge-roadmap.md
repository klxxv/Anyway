# Research Canvas — Merge 路线图与冲突分析报告

> 生成时间：2026-08-05  
> 目标：为主分支协调 7 个待合并分支的先后顺序、标注冲突风险、列出未覆盖项  
> ⚠️ 本报告不执行 merge，仅出具审查结论

---

## 1. 分支全景

### 已合入 main（非审查对象，仅作参考）

| Todo# | 内容 | GitHub PR | 状态 |
|-------|------|-----------|------|
| #18 | 解耦 research-core.ts → `app/lib/graph/`、`layout/`、`analysis/`、`project/` | #19 merged | ✅ 已合入 |
| #21 | 新增 `graph_compiler.rs` 单体文件（规范化、双哈希、不变式、编译管线） | #20 merged | ✅ 已合入 |

### 待合并分支（按 merge-base 分为两组）

#### A 组：基于旧基线 `367a028`（在 #18/#21 合并之前创建）

| Todo# | 分支 | 变更范围 | merge-base |
|-------|------|----------|------------|
| **#17** | `tds/conv-019fcd8c-cf8a` | Ed25519 签名验证：新增 `signing.rs`、修改 `plugins.rs`/`lib.rs`/`Cargo.toml` | `367a028` ⚠️ |
| **#19** | `tds/conv-019fcd8c-d0af` | CI/CD 流水线：新增 `.github/workflows/ci.yml`、`release.yml`、`nightly-perf.yml` | `367a028` ⚠️ |
| **#20** | `tds/conv-019fcda6-5ae6` | 解耦 `use-workspace-project.ts`：拆分 `commit-logic.ts`/`patch-apply.ts`/`sync-logic.ts` | `367a028` ⚠️ |

#### B 组：基于当前 main 顶端 `85053bd`（在 #18/#21 合并之后创建）

| Todo# | 分支 | 变更范围 | merge-base |
|-------|------|----------|------------|
| **#24** | `tds/conv-019fcdc2-4ef7` | 图算法迁移：拆分 `graph_compiler/` 目录 → `algorithms.rs` + `analysis.rs` + `canonical.rs` + `invariants.rs` + `mod.rs`；升级 `workspace_host.rs` | `85053bd` ✅ |
| **#23** | `tds/conv-019fcdc3-e97c` | 确定性布局迁移：`graph_compiler/layout.rs`（1181 行）；新增 `graph_cmds.rs` Tauri 命令 | `85053bd` ✅ |
| **#22** | `tds/conv-019fcded-0f6c` | TS-Rust 比对测试：新增单体 `graph_algorithms.rs`(1382行)、`compiler-reference.ts`、`compiler-parity.test.ts`、`compile_harness.rs` | `85053bd` ✅ |

---

## 2. 关键冲突分析

### 🔴 冲突 C1：#23 ↔ #24（直接文件冲突 — 最高优先级）

两个分支**同时创建 `graph_compiler/` 目录**，以下文件直接冲突：

| 冲突文件 | #24（图算法） | #23（布局） | 冲突性质 |
|----------|--------------|-------------|----------|
| `graph_compiler/canonical.rs` | 327 行，仅规范化+哈希 | **701 行**，规范化+哈希+编译管线（`compile`/`verify_hashes`） | 内容差异大 |
| `graph_compiler/invariants.rs` | 394 行 | 395 行（几乎一致） | 轻微差异 |
| `graph_compiler/mod.rs` | `pub mod algorithms; pub mod analysis; …` | `pub mod canonical; pub mod invariants; pub mod layout;` | 互斥的模块声明 |
| `docs/module-boundaries.md` | 新增 algorithms/analysis 描述 | 新增 layout 描述 | 文本冲突 |

**根因**：Phase 1.2（#24）和 Phase 1.3（#23）都从单体 `graph_compiler.rs` 拆出 canonical/invariants，各自由不同 agent 独立实现，未在分支之间协调。

**建议处理**：
- **先合入 #24**（基础设施：algorithms + analysis + canonical + invariants）
- **然后 rebase #23 在 #24 之上**：保留 #23 的 `layout.rs`、`graph_cmds.rs`、扩展后的 `mod.rs`
- canonical.rs 以 #23 版本为准（701 行，含编译管线），因为它是 #24 的超集
- invariants.rs 以 #24 版本为准（差异极小，按需合并）
- mod.rs 合并双方的 `pub mod` 声明

---

### 🟡 冲突 C2：#22 ↔ #24（架构竞争 — 高优先级）

| 方面 | #22（比对测试） | #24（图算法） |
|------|----------------|--------------|
| 架构 | 单体 `src-tauri/src/graph_algorithms.rs`（1382 行） | 目录模块 `graph_compiler/algorithms.rs`（492 行）+ `analysis.rs`（286 行） |
| 数据类型 | raw `serde_json::Value`，返回 JSON | 强类型 Rust struct（`TraversalRequest`、`Cycle`、`LogicChainResult`） |
| 覆盖函数 | `traverse`、`detect_cycles`、`shortest_path`、`all_shortest_paths`、`compare_reachability`、`compute_logic_chain`、`propagate_influence`、`compute_layout` | `traverse_graph`、`shortest_path`、`all_shortest_paths`、`detect_cycles`、`contradiction_chains`、`compute_logic_chain`、`compare_scenario_reachability` |
| 定位 | Phase 1.4 比对验证 | Phase 1.2 正式迁移 |

**根因**：#22 作为比对测试，本应调用 #24/#23 已迁移的 Rust 模块做比对，但它自己重新实现了一套完整的算法。两个实现并存会在 lib.rs 同时注册两个模块（`graph_algorithms` + `graph_compiler`），造成符号冲突和代码重复。

**建议处理**：
- **#24 → #23 → #22** 顺序合并
- #22 合并时，**删除** `src-tauri/src/graph_algorithms.rs`（它的单体实现）
- **保留** #22 的 `tests/compiler-parity.test.ts`、`app/lib/compiler-reference.ts`、`src-tauri/examples/compile_harness.rs`
- 将 `compiler-parity.test.ts` 的 Rust 端调用从 `graph_algorithms` 重定向到 `graph_compiler` 模块
- 比对测试框架本身（TS 端调用 → Rust 端对比 → 逐位断言）是有价值的

---

### 🟡 冲突 C3：#17 ↔ main（基线滞后 — 中优先级）

**问题**：#17 的 `src-tauri/src/lib.rs` 缺少 `pub mod graph_compiler;`（该模块在 #21 合入主分支后才出现）

| 文件 | #17 状态 | main 要求 |
|------|---------|-----------|
| `src-tauri/src/lib.rs` | `mod signing;` | 需要同时有 `pub mod graph_compiler;` + `mod signing;` |
| `src-tauri/Cargo.toml` | 添加 ed25519-dalek、base64 | 需要位于 graph_compiler 的依赖之后 |

**建议处理**：
- 合并时手动补上 `pub mod graph_compiler;`
- 确认 Cargo.toml 依赖合并正确

---

### 🟡 冲突 C4：#20 ↔ #18（TypeScript 重构一致性 — 中优先级）

**问题**：#20 基于旧版 `use-workspace-project.ts`（依赖单体 `research-core.ts`），而 #18 已将 `research-core.ts` 拆分为 `app/lib/graph/`、`layout/`、`analysis/`、`project/`。

| 文件 | #20 变更 | #18/mian 状态 |
|------|---------|---------------|
| `use-workspace-project.ts` | 重写为薄编排层，import 来自新建的 `commit-logic.ts` 等 | import 已改为从 `app/lib/layout`、`app/lib/graph` 等分解模块 |

**兼容性判断**：#20 创建的新文件（`commit-logic.ts`、`patch-apply.ts`、`sync-logic.ts`）不直接依赖 `research-core.ts`，它们依赖的类型通过 `research-types.ts` 传入。合并时应：
- 确保 `use-workspace-project.ts` 的 import 指向已分解的模块路径
- 确认新 hook 模块的类型导入与 #18 一致

---

### 🟢 冲突 C5：#17 ↔ #23（lib.rs 轻微冲突 — 低优先级）

#17 在 lib.rs 添加 `mod signing;`，#23 添加 `mod graph_cmds;` + Tauri commands。两者触及同一区域，但为可合并的邻接变更。

---

### 🟢 无冲突：#19（CI/CD）

仅添加 `.github/workflows/` 下的 3 个 YAML 文件，不触碰任何源代码。可直接合并。

---

## 3. 推荐合并顺序

```
第 1 梯队（基础设施，无冲突）
  │
  ├── [1] #19  CI/CD 流水线
  │       原因：纯新增 YAML，零代码冲突。为后续分支的 CI 验证铺路。
  │
  ├── [2] #17  Ed25519 签名验证
  │       原因：独立的 security 模块，仅触碰 lib.rs/Cargo.toml/plugins.rs。
  │       合并时需手动补上 `pub mod graph_compiler;`。
  │
  ▼
第 2 梯队（graph_compiler 模块化迁移）
  │
  ├── [3] #24  图算法迁移（Phase 1.2）
  │       原因：创建 graph_compiler/ 目录骨架（canonical + invariants + algorithms + analysis），
  │       为 #23 铺路。升级 workspace_host.rs。
  │
  ├── [4] #23  确定性布局迁移（Phase 1.3）—— **在 #24 之上 rebase**
  │       原因：追加 layout.rs 到已有目录，新增 graph_cmds.rs Tauri 命令。
  │       需解决 C1 冲突，以 #23 的 canonical.rs（701 行）为准。
  │
  ▼
第 3 梯队（TS 侧重构 + 比对验证）
  │
  ├── [5] #20  workspace-project 解耦
  │       原因：重构 use-workspace-project.ts 为纯模块。确认与 #18 分解后的
  │       import 路径一致。新增 48 个 hook 单元测试。
  │
  ├── [6] #22  比对测试（Phase 1.4）—— **仅保留测试框架，删除 duplicate 实现**
  │       原因：保留 compiler-parity.test.ts + compiler-reference.ts + compile_harness.rs。
  │       删除 graph_algorithms.rs（与 #24 的 graph_compiler/algorithms.rs 重复）。
  │       将比对测试重定向到 graph_compiler 模块。
  │
  ▼
第 4 梯队（收尾）
  │
  └── [7] 全量测试 + lint 验证
          运行 npm test 全流程 + cargo test + cargo clippy
```

---

## 4. 冲突风险矩阵

| | #17 | #19 | #20 | #22 | #23 | #24 |
|---|---|---|---|---|---|---|
| **#17** | — | 🟢 无 | 🟢 无 | 🟢 无 | 🟡 lib.rs | 🟡 lib.rs |
| **#19** | 🟢 无 | — | 🟢 无 | 🟢 无 | 🟢 无 | 🟢 无 |
| **#20** | 🟢 无 | 🟢 无 | — | 🟢 无 | 🟢 无 | 🟢 无 |
| **#22** | 🟢 无 | 🟢 无 | 🟢 无 | — | 🟢 无 | 🔴 架构竞争 |
| **#23** | 🟡 lib.rs | 🟢 无 | 🟢 无 | 🟢 无 | — | 🔴 文件冲突 |
| **#24** | 🟡 lib.rs | 🟢 无 | 🟢 无 | 🔴 架构竞争 | 🔴 文件冲突 | — |

- 🔴 = 必须手动解决
- 🟡 = 需检查但合并简单
- 🟢 = 无冲突

---

## 5. 各分支测试验证状态

| 分支 | Rust 测试 | TS 测试 | 备注 |
|------|----------|---------|------|
| **#17** | 29/29 通过 ✅ | N/A | 签名验证测试全部通过，零 clippy 警告 |
| **#19** | N/A | N/A | 仅 YAML，无测试代码 |
| **#20** | N/A | 48/48 workspace + 全部套件通过 ✅ | lint、tsc、rendered-html 均通过 |
| **#24** | 29 单元 + 集成测试全部通过 ✅ | N/A | workspace_host 升级后测试通过 |
| **#23** | 53/53 通过 ✅ | N/A | 5 个 layout 测试在修正后全部通过，clippy 干净 |
| **#22** | 未独立验证 ⚠️ | 比对测试需 Rust harness 编译 | `compile_harness` 需在 Rust 端编译后运行 |

---

## 6. 未覆盖项与风险提示

### 6.1 测试覆盖缺口

| 缺口 | 影响 | 建议 |
|------|------|------|
| #22 比对测试未与 #24 的算法对齐 | 两个实现并存，比对没有覆盖正式 Rust 模块 | #22 合并时重定向到 `graph_compiler` |
| #20 workspace 测试未覆盖 #18 分解后的模块组合 | hook 重组可能与新的 import 路径不一致 | 合并 #20 后运行 `npm run test:workspace` |
| #17 缺少与 graph_compiler 的集成测试 | 签名后的插件 WASM 调用图编译器时无端到端验证 | 后续添加 |

### 6.2 架构一致性风险

| 风险 | 详情 |
|------|------|
| **#22 的 graph_algorithms.rs 和 #24 的 graph_compiler/ 是两套并行实现** | #22 使用 raw Value/JSON，#24 使用强类型 Rust struct。两套不能并存。本报告推荐保留 #24 的模块化方案。 |
| **#23 的 canonical.rs（701 行）是 #24 的 canonical.rs（327 行）的超集** | #23 增加了 `compile()` 和 `verify_hashes()` 编译管线函数。合并时以 #23 为准。 |
| **lib.rs 模块注册累积** | 最终 lib.rs 需同时注册：`graph_compiler`、`signing`、`graph_cmds`、`graph_algorithms`（若保留） |

### 6.3 v3 Schema 审计要点

| 检查项 | 对应分支 | 状态 |
|--------|---------|------|
| §3 双哈希方案（blockHash / contentRootHash / fileHash） | #21→#24→#23 | ✅ canonical.rs 实现 |
| §3.4 规范化（键排序、数字规范化、NFC） | #21→#24→#23 | ✅ canonical.rs 实现 |
| §3.5 编辑级联 + verify_hashes | #23 | ✅ canonical.rs 含编译管线 |
| §6 版控差异（基于 blockHash 的语义 diff） | #24 | ✅ mod.rs + workspace_host.rs |
| §11 布局意图（views[].layout = { mode, params }） | #23 | ✅ layout.rs + graph_cmds.rs |
| §15.1 编译管线 | #21→#24→#23 | ✅ mod.rs |
| §15.2 编译器独占清单（BFS/DFS/环/逻辑链等） | #24 | ✅ algorithms.rs + analysis.rs |
| §15.5 迁移路径（逐位比对 gate） | #22 | ⚠️ 需重定向到 graph_compiler |

---

## 7. 执行检查清单

### Merge [1] #19 CI/CD
- [ ] 确认 `.github/workflows/ci.yml` 包含所有测试脚本
- [ ] 确认 `release.yml` 构建矩阵覆盖 Windows/macOS/Linux
- [ ] 合入后触发首次 CI 运行

### Merge [2] #17 Ed25519
- [ ] `lib.rs`：补上 `pub mod graph_compiler;`（在 `mod signing;` 之前）
- [ ] `Cargo.toml`：确认 ed25519-dalek 依赖与 graph_compiler 依赖无冲突
- [ ] 运行 `cargo test` 确认 29 个测试通过
- [ ] 运行 `cargo clippy` 零警告

### Merge [3] #24 图算法
- [ ] 确认删除单体 `graph_compiler.rs`
- [ ] 确认 `graph_compiler/` 目录下所有文件就位
- [ ] 运行 `cargo test`（含 `tests/graph_algorithms.rs` 集成测试）
- [ ] 验证 `workspace_host.rs` 的 `graph_patch_from_commits` 升级正确

### Merge [4] #23 布局（rebase 在 #24 上）
- [ ] 解决 `canonical.rs` 冲突 → 以 #23 版本为准
- [ ] 解决 `invariants.rs` 冲突 → 合并双方差异
- [ ] 解决 `mod.rs` 冲突 → 合并双方 `pub mod` 声明
- [ ] 解决 `docs/module-boundaries.md` 冲突
- [ ] 运行 `cargo test` 确认 ≥ 53 个测试通过

### Merge [5] #20 workspace
- [ ] 确认 `use-workspace-project.ts` import 指向分解后的模块
- [ ] 运行 `npm run test:workspace`（48 个测试）
- [ ] 运行完整 `npm test`

### Merge [6] #22 比对测试（适配后）
- [ ] 删除 `src-tauri/src/graph_algorithms.rs`
- [ ] 保留 `tests/compiler-parity.test.ts`、`app/lib/compiler-reference.ts`、`src-tauri/examples/compile_harness.rs`
- [ ] 重写 `compiler-parity.test.ts` 的 Rust 端调用 → `graph_compiler` 模块
- [ ] 构建 `compile_harness` 示例：`cargo build --example compile_harness`
- [ ] 运行 `npm run test:compiler`

### 最终验证
- [ ] `npm run lint` 零错误
- [ ] `npm test` 全流程通过（tsc + rendered-html + core + platform + workspace + sdk + native + compiler）
- [ ] `cargo clippy` 零新增警告
- [ ] CI 全绿

---

## 8. merge 路线图总结

```
时间线：

  T+0  ─── Merge [#19] CI/CD ─── 无风险
  T+1  ─── Merge [#17] Ed25519 ─── 手动补 graph_compiler 引用
  T+2  ─── Merge [#24] 图算法 ─── graph_compiler/ 基础骨架
  T+3  ─── Rebase [#23] on #24 ─── 解决 canonical/mod.rs 冲突 → Merge [#23]
  T+4  ─── Merge [#20] workspace ─── 确认 TS import 兼容
  T+5  ─── 适配 [#22] 比对测试 ─── 删除 graph_algorithms.rs → Merge [#22]
  T+6  ─── 全量验证 + CI 门禁

风险缓冲：T+2 到 T+3 之间有文件冲突需要手动解决，预计耗时最长。
           如 #23 的 rebase 遇到困难，可考虑先将两个分支的 canonical.rs 做三方 diff 合并。
```

---

*报告由 PR 审核与 Github 管理 agent 基于代码审查生成，不替代人工判断。*
*所有分支的对话历史和实际变更均已逐项核实。*
