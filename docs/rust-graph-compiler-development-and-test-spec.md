# Research Canvas Rust 图编译器开发与测试规范

> 文档状态：Implementation Specification  
> 目标版本：Graph Compiler v1 / `.mycproj` schema v3  
> 目标读者：Rust 内核开发者、Tauri 集成开发者、测试工程师、CI/注册表维护者  
> 规范关键词：**MUST**=必须，**SHOULD**=建议，**MAY**=可选

## 0. 文档目的

本文定义 `src-tauri/src/graph_compiler.rs` 及其建议拆分 crate 的功能、接口、算法、错误模型、确定性要求、性能目标和测试矩阵。图编译器是 Research Canvas 的语义硬内核：Agent、连接器和 UI 只提交 `GraphPatch`，所有图性质、哈希、结构校验、逻辑链、矛盾链、消融差异和确定性布局均由 Rust 重新计算。

本规范以以下产品材料为基线：

- `canvas-format-v3.md`：schema v3、双哈希、GraphPatch、证据锚、图编译器边界。
- `module-boundaries.md`：Rust 内核、Tauri/Registry/CLI 三处复用、插件安全边界。
- `product-vision.md`：14类研究节点、12类语义边、消融场景、逻辑链和影响传播。
- 当前对话中的逻辑计算图设计：变量裂变、因子节点、统计证据、双通道信念和阻尼 Loopy BP。

## 1. 核心设计决策

### 1.1 编译器负责的事实

编译器 MUST 负责：

1. schema 解析、版本迁移和规范化；
2. `blockHash`、`contentRootHash`、`fileHash`；
3. 图不变式、类型约束和证据接地校验；
4. `GraphPatch` 预演、原子应用、ID 联动重写；
5. 索引、BFS/DFS、可达性、SCC、拓扑序和路径；
6. 逻辑图到因子图的确定性编译；
7. 统计证据标准化、边效力和有效消息；
8. 树形 BP 与阻尼 Loopy BP；
9. 结构矛盾见证、最小矛盾子图；
10. 场景消融、版本 Diff 和影响集合；
11. 确定性布局、digest、Mermaid 和机器可读报告；
12. 增量缓存、资源预算、CLI/Tauri/Registry 一致性。

LLM/Agent MAY 提议节点、变量、边、证据锚和实验设计；编译器 MUST 拒绝让 Agent 注入哈希、后验概率、逻辑链结果或布局坐标作为可信事实。

### 1.2 图模型

用户语义层为有向、带类型、带属性、允许多边的研究图：

\[
G=(V,E,\tau_V,\tau_E,A_V,A_E)
\]

逻辑计算层为因子图：

\[
\mathcal F=(X,F,E_F)
\]

其中 `X` 是布尔/有限枚举主张变量，`F` 是 `supports`、`contradicts`、`implies`、`and`、`or`、`depends_on`、`statistical_test`、`meta_evidence` 等因子。

### 1.3 确定性

相同规范输入、相同编译器版本和相同配置 MUST 得到逐字节相同的：

- 规范 JSON；
- 三类哈希；
- 错误排序；
- 遍历和路径排序；
- BP 更新顺序、停止状态和舍入；
- 布局坐标；
- digest/Mermaid；
- `CompileReport`。

浮点结果 MUST 采用固定算法、固定边遍历顺序和规定的量化方式。推荐内部使用 `f64`，外部序列化前执行 decimal quantization；涉及哈希的数值按 schema v3 数字规范序列化。

## 2. 建议 crate 结构

```text
crates/research-graph-compiler/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── model.rs
    ├── error.rs
    ├── parse.rs
    ├── canonical.rs
    ├── hash.rs
    ├── invariant.rs
    ├── patch.rs
    ├── index.rs
    ├── traversal.rs
    ├── topology.rs
    ├── factor/
    │   ├── mod.rs
    │   ├── compile.rs
    │   ├── statistics.rs
    │   ├── bp.rs
    │   └── contradiction.rs
    ├── scenario.rs
    ├── diff.rs
    ├── layout.rs
    ├── export.rs
    ├── cache.rs
    └── compile.rs
src-tauri/src/graph_compiler.rs
src/bin/canvas.rs
```

## 3. 公共接口

```rust
pub struct CompileOptions {
    pub strict_schema: bool,
    pub compute_layouts: bool,
    pub compute_logic: bool,
    pub compute_belief: bool,
    pub max_paths: usize,
    pub max_depth: usize,
    pub bp: BpOptions,
    pub resource_limits: ResourceLimits,
}

pub struct CompileArtifact {
    pub canonical_project: CanonicalProject,
    pub file_hash: Hash256,
    pub content_root_hash: Hash256,
    pub diagnostics: Vec<Diagnostic>,
    pub indexes: GraphIndexes,
    pub derived: DerivedGraphProperties,
    pub digest: ProjectDigest,
}

pub fn compile_project(bytes: &[u8], options: &CompileOptions)
    -> Result<CompileArtifact, CompileFailure>;

pub fn plan_patch(base: &CanonicalProject, patch: &GraphPatch, options: &CompileOptions)
    -> Result<PatchPlan, CompileFailure>;

pub fn apply_patch(base: &CanonicalProject, plan: PatchPlan, options: &CompileOptions)
    -> Result<CompileArtifact, CompileFailure>;

pub fn diff_projects(left: &CanonicalProject, right: &CanonicalProject) -> GraphDiff;
```

### 3.1 稳定错误结构

```rust
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub entity_ref: Option<String>,
    pub json_pointer: Option<String>,
    pub message_key: &'static str,
    pub args: BTreeMap<String, String>,
    pub suggested_fixes: Vec<FixProposal>,
}
```

错误文本可以本地化，`code`、排序和结构 MUST 稳定。

## 4. 逻辑计算数据结构

```rust
pub enum FactorKind {
    Supports, Contradicts, Implies, Equivalent, And, Or,
    DependsOn, StatisticalTest, MetaEvidence, Interaction,
}

pub struct EdgeQuality {
    pub design: f64,
    pub source: f64,
    pub condition_match: f64,
    pub independence: f64,
    pub reproducibility: f64,
}

pub struct CompiledEdgeMetric {
    pub local_log_evidence: Option<f64>,
    pub efficacy: f64,
    pub effective_message: Option<f64>,
    pub calibration: CalibrationMethod,
    pub warnings: Vec<DiagnosticCode>,
}

pub struct BeliefState {
    pub support_logit: f64,
    pub refutation_logit: f64,
    pub support: f64,
    pub refutation: f64,
    pub net_belief: f64,
    pub conflict: f64,
}
```

边效力：

\[
\eta_e=q_{\mathrm{design}}q_{\mathrm{source}}q_{\mathrm{match}}
q_{\mathrm{independence}}q_{\mathrm{reproducibility}}
\]

有效证据消息：

\[
w_e=\eta_e\lambda_e,\qquad
\lambda_e=\log\frac{P(D_e\mid H)}{P(D_e\mid \neg H)}
\]

`pValue` 是观测统计量，不能直接等同于主张为真的概率。只有 p 值时，编译器采用显式、保守、可追踪的校准方法，并在输出中标记 `calibration`.

## 5. 编译管线

```text
bytes
  → parse + migrate
  → canonicalize
  → rebuild IDs and hashes
  → validate invariants/types
  → build indexes
  → compile semantic factors
  → normalize statistical evidence
  → run graph algorithms
  → run BP / contradiction / scenarios
  → deterministic layout/export
  → canonical serialize + fileHash
  → CompileArtifact
```

所有阶段均返回稳定诊断；致命错误终止，警告进入报告。`GraphPatch` 必须先 `plan`，展示将新增、修改、删除、ID 重映射和受影响范围，再由用户确认后原子应用。

## 6. 示例：Residual Attention 消融图

```json
{
  "nodes": [
    {"type":"variable","title":{"en":"Residual mode"},"data":{"name":"residualMode","valueType":"enum","enumValues":["ARL","NAL"]}},
    {"type":"variable","title":{"en":"Network depth"},"data":{"name":"depth","valueType":"enum","enumValues":[56,92,128,164]}},
    {"type":"metric","title":{"en":"Top-1 error"},"data":{"unit":"percent","direction":"lower_is_better"}},
    {"type":"claim","title":{"en":"ARL prevents degradation in deeper attention networks"},"data":{}}
  ],
  "scenarios": [
    {"name":"ARL-164","assign":{"residualMode":"ARL","depth":164},"observations":{"top1Error":4.31}},
    {"name":"NAL-164","assign":{"residualMode":"NAL","depth":164},"observations":{"top1Error":7.18}}
  ]
}
```

编译器应派生：

\[
\Delta_{164}=7.18-4.31=2.87
\]

派生结论属于 `CompileArtifact.derived`，不会写入语义哈希区，除非用户将其审阅后显式转成 claim。

## 7. 测试策略总则

- 本文把“可独立验收的子系统”定义为测试小项；每个小项给出至少10组边界测试。
- 单元测试使用固定 fixture；属性测试使用 `proptest`；模糊测试使用 `cargo-fuzz`.
- TS/Rust 迁移期对同一 fixture 执行 bit-identical 对比。
- Golden 文件只保存规范化输入、稳定诊断和编译产物。
- 所有复杂度测试记录节点数、边数、峰值内存、耗时和编译器版本。

## GC-01 解析、schema与迁移

验证原始字节到 v3 内存模型的安全性、版本兼容和资源上限。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC01-01 | 空文件，0字节 | 调用 compile_project | 返回 ParseEmptyInput；无 panic | 错误码、offset=0 |
| GC01-02 | 仅 UTF-8 BOM | 解析 | 视为空输入并返回稳定错误 | BOM 处理一致 |
| GC01-03 | 非法 UTF-8 字节 | 解析 | 返回 InvalidUtf8，报告首个非法偏移 | 不进行替换字符容错 |
| GC01-04 | 有效 JSON，缺 schemaVersion | strict=true 编译 | 返回 MissingSchemaVersion | JSON pointer=/schemaVersion |
| GC01-05 | schemaVersion=2 | 启用迁移 | 迁移到v3并生成迁移报告 | 旧ID重写、边同步 |
| GC01-06 | schemaVersion=999 | 编译 | 返回 UnsupportedFutureSchema | 原始字节不修改 |
| GC01-07 | 根对象含未知字段 | strict=false/true各编译 | 宽松模式保留opaque或警告；严格模式拒绝 | 模式差异稳定 |
| GC01-08 | nodes字段为null | 编译 | 返回 TypeMismatch | 定位/nodes |
| GC01-09 | 10万节点JSON接近资源上限 | 低内存预算编译 | ResourceLimitExceeded，可预测终止 | 无OOM、无部分产物 |
| GC01-10 | JSON深度超过限制 | 编译 | 拒绝过深嵌套 | 防栈溢出 |
| GC01-11 | 同一文件CRLF与LF两版 | 分别解析规范化 | 规范化语义相同 | 后续blockHash相同 |
| GC01-12 | 数字1e-4与0.0001 | 解析 | 内部数值相等 | 规范序列化一致 |

## GC-02 规范化 canonicalization

规范化是所有哈希、Diff 和跨机器一致性的根。字段必须区分语义集合、有序序列、内容字段和悬挂字段。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC02-01 | 对象键顺序完全反转 | canonicalize | 输出键字典序一致 | 字节相同 |
| GC02-02 | 嵌套data键顺序不同 | canonicalize | 递归排序 | 深层字节相同 |
| GC02-03 | 数组输入乱序但规范要求集合排序 | canonicalize | 按实体规范键排序 | 输出稳定 |
| GC02-04 | 语义有序数组如路径步骤 | canonicalize | 保持顺序 | 不得误排序 |
| GC02-05 | 文本含NFD与NFC等价字符 | canonicalize | 统一NFC | hash相同 |
| GC02-06 | 连续空格、制表、换行 | 文本规范 | 按字段规则折叠 | title/body结果符合规范 |
| GC02-07 | 前后空白与不可断空格 | 规范化 | 去边界空白并映射空格 | 输出稳定 |
| GC02-08 | -0、0、0.0 | 数字规范 | 全部规范为0 | hash相同 |
| GC02-09 | NaN/Infinity | 解析/规范化 | 拒绝非JSON有限数 | 稳定错误 |
| GC02-10 | 多语言map语言键乱序 | canonicalize | 按lang排序 | 新增语言导致内容hash变化 |
| GC02-11 | evidence quote差异 | 规范证据身份 | quote不进入证据ID输入 | 证据ID相同 |
| GC02-12 | claim evidenceIds顺序与集合变化 | 规范claim身份 | evidenceIds不进入claim blockHash | claim ID稳定 |

## GC-03 三层哈希与引用

实现 `blockHash`、`contentRootHash`、自编码 `fileHash` 和 `rc:` 引用验证。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC03-01 | 同一节点不同JSON键序 | 计算blockHash | 相同 | 全64hex与短ID一致 |
| GC03-02 | claim只增加evidenceIds | 重算 | claim blockHash不变；fileHash变化 | 职责分离 |
| GC03-03 | claim body改一个字 | 重算 | blockHash、节点ID、fileHash变化 | 引用联动 |
| GC03-04 | review事件新增 | 重算 | contentRootHash不变；fileHash变化 | 发布锚不移动 |
| GC03-05 | view布局意图变化 | 重算 | contentRootHash不变；fileHash变化 | 布局不进内容根 |
| GC03-06 | fileHash字段含错误旧值 | verify | 按置空规则重算并报Mismatch | 不得把旧值纳入输入 |
| GC03-07 | 节点短hash前12位碰撞fixture | 构造碰撞注册表 | 拒绝或扩展ID策略 | 全hash验证生效 |
| GC03-08 | 实体数组增删后contentRoot | 重算 | 仅语义区blockHash集合影响 | 排序聚合稳定 |
| GC03-09 | 相同内容在不同git历史 | 重算 | fileHash相同 | git commit不参与身份 |
| GC03-10 | 不同浮点文本同数值 | 重算 | hash相同 | 数字规范生效 |
| GC03-11 | locator页码变化 | 证据重算 | evidence ID不变；fileHash变化 | locator为版本提示 |
| GC03-12 | anchor句号从3改4 | 证据重算 | evidence ID变化 | 逻辑锚进入身份 |

## GC-04 图不变式与语义类型检查

对结构完整性、节点/边类型、变量域、因子元数和谱系进行硬校验。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC04-01 | 重复节点ID、内容相同 | validate | 归并或明确DuplicateId诊断 | 策略由模式决定 |
| GC04-02 | 重复节点ID、内容不同 | validate | 致命冲突 | 不得静默覆盖 |
| GC04-03 | 边source不存在 | validate | DanglingSource | 定位边ID |
| GC04-04 | 边target不存在 | validate | DanglingTarget | 给出删除边/恢复节点修复 |
| GC04-05 | evidenceIds引用不存在 | validate | DanglingEvidence | claim仍可读但编译失败/警告按strict |
| GC04-06 | controls边连接两个paper节点 | 类型检查 | EdgeTypeEndpointMismatch | 显示允许类型 |
| GC04-07 | contradicts边polarity=positive | 类型检查 | PolaritySemanticMismatch | 不得自动改写 |
| GC04-08 | VariableSpec enum为空 | 类型检查 | InvalidVariableDomain | 域至少2值用于消融 |
| GC04-09 | float min>max或step<=0 | 类型检查 | InvalidNumericDomain | 精确字段指针 |
| GC04-10 | AND因子只有一个输入 | 类型检查 | FactorArityMismatch | 要求>=2输入+1输出 |
| GC04-11 | 同一边自环supports | 类型检查 | 按规则警告RedundantSelfSupport | 不会进入BP重复放大 |
| GC04-12 | derivedFrom形成谱系环 | validate | LineageCycle | 与普通研究图环分开报告 |

## GC-05 GraphPatch 计划与原子应用

所有编辑先形成可审阅计划；ID生成、级联、并发控制和回滚必须确定。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC05-01 | 空GraphPatch | plan/apply | no-op；fileHash不变 | 报告0项影响 |
| GC05-02 | 新增节点无ID | plan | host生成blockHash ID | Agent ID被忽略 |
| GC05-03 | Agent提供伪造ID | plan | 拒绝或覆盖并警告 | ID只由编译器生成 |
| GC05-04 | 更新节点导致ID变化 | apply | 所有source/target和引用原子重写 | 无悬空中间态 |
| GC05-05 | 删除节点仍有边 | plan | 列出级联策略并要求显式选择 | 默认拒绝 |
| GC05-06 | patch内先加边后加节点 | plan | 按依赖排序后成功 | 操作顺序不影响语义 |
| GC05-07 | patch包含两个互相冲突更新 | plan | PatchConflict | 报告冲突路径 |
| GC05-08 | baseFileHash已过期 | apply | OptimisticConcurrencyConflict | 不修改项目 |
| GC05-09 | 应用到一半资源超限 | apply | 事务回滚 | 输出无部分文件 |
| GC05-10 | 重复提交同patchId | apply两次 | 第二次幂等返回已应用结果 | fileHash不重复变化 |
| GC05-11 | 添加同内容节点 | plan | 识别blockHash相同并建议复用 | 证据集合可合并 |
| GC05-12 | patch修改review与内容 | plan | 分成语义变更和事件变更 | contentRoot变化仅来自内容 |

## GC-06 索引、BFS/DFS与可达性

构建正反邻接、证据、哈希和引用索引，并提供稳定遍历。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC06-01 | 0节点0边 | 建索引/BFS | 返回空索引和空结果 | 无特殊panic |
| GC06-02 | 单节点 | BFS depth=0 | 只含起点 | 距离0 |
| GC06-03 | 平行多边 | BFS | 节点只访问一次；边全部可枚举 | 稳定边序 |
| GC06-04 | 有向链反向查询 | direction=reverse | 得到祖先集合 | 方向正确 |
| GC06-05 | type filter排除起点类型 | BFS | 起点保留策略按规范；后继过滤 | 行为固定 |
| GC06-06 | maxDepth=1 | BFS | 只返回一跳 | 边界不多一层 |
| GC06-07 | 环A→B→A | DFS | 标记back edge并终止 | 无无限循环 |
| GC06-08 | 多源可达有重叠 | multi-source | 去重并保留最小距离和来源集合 | 排序稳定 |
| GC06-09 | 禁用边overlay | reachability | 该边不参与 | base索引不被污染 |
| GC06-10 | 10万节点稀疏链 | BFS资源预算 | 线性完成或预算错误 | 峰值内存受控 |
| GC06-11 | Unicode标题相同但ID不同 | 排序 | 使用规范ID作为最终tie-break | 跨locale一致 |
| GC06-12 | 证据反向索引同证据挂多节点/边 | lookup | 返回全部且去重 | 顺序稳定 |

## GC-07 SCC、拓扑排序与路径

把循环依赖压缩为凝聚 DAG，并提供稳定最短路径和有限多路径。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC07-01 | DAG单链 | SCC | 每节点一个SCC | 凝聚图同构 |
| GC07-02 | 三节点环 | SCC | 一个3节点SCC | 成员按ID排序 |
| GC07-03 | 自环节点 | SCC/环检测 | 单节点循环组件 | 与无环单点区分 |
| GC07-04 | 两个SCC单向连接 | 凝聚图 | 得到2节点DAG | 拓扑序稳定 |
| GC07-05 | DAG多个合法拓扑序 | toposort | 按固定tie-break选择一个 | 逐位一致 |
| GC07-06 | 起终点无路径 | shortest_path | 返回None | 非错误 |
| GC07-07 | 两条等长最短路径 | all_shortest | 返回两条并稳定排序 | 不漏不重 |
| GC07-08 | 路径数超过maxPaths | enumerate | 截断并标记truncated | 先返回最短/稳定 |
| GC07-09 | 路径含禁用节点 | scenario path | 排除该路径 | base图不变 |
| GC07-10 | 负极性边路径 | signed path | 计算路径极性乘积 | 奇数负边为negative |
| GC07-11 | 深度超maxDepth | path search | 不返回超限路径 | 报告depth_limited |
| GC07-12 | 大型稠密图 | SCC/path预算 | SCC完成；路径枚举受限 | 避免指数爆炸 |

## GC-08 语义图到逻辑因子图

把类型边和复合逻辑节点编译成明确的因子及变量域。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC08-01 | supports二元边 | compile factors | 生成二元Supports因子 | 端点映射正确 |
| GC08-02 | contradicts边 | compile | 生成翻转极性因子 | 支持/反驳通道交换 |
| GC08-03 | implies A→B | compile | 只惩罚A真B假 | 不得反向等价 |
| GC08-04 | AND节点2输入1输出 | compile | 生成三变量truth-table因子 | 元数正确 |
| GC08-05 | OR节点3输入1输出 | compile | 生成n元OR因子 | 组合状态完整 |
| GC08-06 | depends_on带efficacy | compile | 生成门控因子 | η=0时无影响 |
| GC08-07 | 枚举变量域3值 | compile | 生成有限域变量 | 状态顺序按规范 |
| GC08-08 | 连续float变量直接进入BP | compile | 拒绝或要求离散化适配器 | 诊断明确 |
| GC08-09 | 同一语义边重复两次同证据 | compile | 按provenance去重/相关组处理 | 不双计 |
| GC08-10 | 无证据的supports claim | compile | 生成逻辑因子但证据效力为先验/待审 | 标记ungrounded |
| GC08-11 | 因子图出现孤立变量 | compile | 保留先验状态 | 诊断NoEvidence |
| GC08-12 | 语义图环 | compile | 允许生成loopy factor graph | 交给BP收敛层 |

## GC-09 统计证据与边效力

把实验统计量转换为可追踪的局部证据量，并按设计、来源、匹配、独立性和复现度衰减。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC09-01 | p=0.05，只有p值 | normalize | 使用配置的保守校准 | calibration非DirectProbability |
| GC09-02 | p=0 | normalize | 夹紧到最小正数并警告 | 有限LLR |
| GC09-03 | p=1 | normalize | 证据量接近0/反向按方向规则 | 无log(0) |
| GC09-04 | p<0或p>1 | validate | InvalidPValue | 拒绝 |
| GC09-05 | Bayes factor=10 | normalize | λ=ln(10) | 数值误差阈值 |
| GC09-06 | Bayes factor<=0 | validate | InvalidBayesFactor | 拒绝 |
| GC09-07 | effect与SE齐全 | normalize | 按指定统计模型生成证据量 | 记录模型名 |
| GC09-08 | CI上下界反转 | validate | InvalidConfidenceInterval | 字段定位 |
| GC09-09 | 五个quality均1 | efficacy | η=1 | 有效消息=局部消息 |
| GC09-10 | 任一quality=0 | efficacy | η=0 | 消息归零但保留原统计 |
| GC09-11 | quality超[0,1] | validate | InvalidQualityScore | 不得自动截断 |
| GC09-12 | 同一cohort两项证据ρ=1 | meta evidence | 避免当独立证据相加 | 相关矩阵奇异处理 |

## GC-10 双通道信念传播 BP

计算支持、反驳、净信念与冲突；树图精确，环图采用固定顺序和阻尼。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC10-01 | 单变量无因子，先验0 | BP | support/refutation=0.5 | net=0.5 conflict=0.25按定义 |
| GC10-02 | 单强支持证据 | tree BP | 目标support上升 | 方向正确 |
| GC10-03 | 单强反驳证据 | tree BP | refutation上升 | 支持通道不误增 |
| GC10-04 | 等强支持与反驳 | BP | support与refutation同时高 | conflict高 |
| GC10-05 | supports链A→B→C | tree BP | 消息沿链衰减传播 | 无反向泄漏除非配置 |
| GC10-06 | implies中B真 | BP | 不会强推A真 | 非逆否/逆命题误用 |
| GC10-07 | AND一个输入低可信 | BP | 输出受限 | 逻辑真值一致 |
| GC10-08 | 三节点正反馈环 | loopy BP | 阻尼收敛或报告未收敛 | 迭代<=max |
| GC10-09 | 二节点强矛盾振荡 | loopy BP | 检测振荡并返回残差 | 结果标记unstable |
| GC10-10 | η=0边 | BP | 与删除边结果相同 | 数值逐位一致 |
| GC10-11 | 边插入顺序不同 | BP | 固定排序后结果相同 | 确定性 |
| GC10-12 | 极大LLR导致logit溢出 | BP | 使用log域/稳定sigmoid | 输出有限0..1 |

## GC-11 结构矛盾与最小见证

输出可复算的图结构矛盾，明确区分结构见证和形式化证明不可满足。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC11-01 | 直接A contradicts B | witness | 返回1边见证 | 最小 |
| GC11-02 | A到B同时有正路径和负路径 | witness | 返回路径对 | 极性计算正确 |
| GC11-03 | 仅一条负路径，无正路径 | witness | 不构成双路径冲突 | 状态为refutation |
| GC11-04 | 奇数负边环 | signed cycle | 标记结构不平衡 | 返回最短环 |
| GC11-05 | 偶数负边环 | signed cycle | 不标记符号矛盾 | 可报告普通循环 |
| GC11-06 | 多个同长度见证 | minimum witness | 按稳定排序选择首个并可列全部 | 确定性 |
| GC11-07 | 见证超过maxDepth | search | 标记可能存在但未搜索完 | 不得声称无矛盾 |
| GC11-08 | 禁用关键边场景 | scenario witness | 矛盾消失 | 报告resolved |
| GC11-09 | 置信/效力低于阈值 | thresholded witness | 按配置排除并记录阈值 | 原始结构仍可查询 |
| GC11-10 | 自相矛盾claim边 | witness | 返回self-contradiction | 高严重度 |
| GC11-11 | AND因子造成不可满足局部状态 | factor consistency | 返回局部赋值冲突 | 与Lean证明区分 |
| GC11-12 | 10万边图最小见证预算 | search | 预算内返回或truncated | 无无限枚举 |

## GC-12 场景消融与结构化 Diff

以不可变基图加 overlay 计算直接禁用、失去可达性、替代路径、实验矩阵和版本变更。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC12-01 | 空场景overlay | compare | 与base相同 | diff为空 |
| GC12-02 | 直接禁用一个孤立节点 | compare | directlyDisabled含该节点 | newlyUnreachable为空 |
| GC12-03 | 禁用链首节点 | compare | 下游进入newlyUnreachable | 原因路径可追踪 |
| GC12-04 | 存在替代路径 | compare | 目标仍reachable | lostPaths列出消失路径 |
| GC12-05 | 禁用不存在ID | validate | InvalidScenarioReference | 不忽略 |
| GC12-06 | 覆盖变量值不在域内 | validate | ScenarioAssignmentOutOfDomain | 字段定位 |
| GC12-07 | 2×4实验矩阵缺1单元 | matrix analysis | 报告具体缺失组合 | 不计算完整交互或标记不完整 |
| GC12-08 | metric lower_is_better | effect | 按control-treatment方向得到正改善 | 方向元数据生效 |
| GC12-09 | 版本仅review变化 | graph diff | semantic diff为空；review diff有项 | 层次分开 |
| GC12-10 | 节点内容改导致ID联动 | graph diff | 归约为modifiedNode+idRemap | 边变化标记derived |
| GC12-11 | 同一实体数组顺序变化 | diff | 无语义变化 | 规范化后比较 |
| GC12-12 | 场景引入新矛盾 | compare | newContradictions列出见证 | 可追踪到overlay |

## GC-13 确定性布局、digest与导出

布局是可重算编译产物；导出必须稳定、可转义、可预算截断。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC13-01 | 空图所有布局 | layout | 返回空坐标集合 | 无错误 |
| GC13-02 | 单节点 | layout | 固定原点/规范位置 | 逐位一致 |
| GC13-03 | 输入节点顺序随机 | layout | 坐标相同 | 排序独立 |
| GC13-04 | pinned节点 | layout | 保持固定坐标 | 其他节点确定重排 |
| GC13-05 | 两个pinned重叠 | layout | 稳定诊断并按策略保留 | 不得随机抖动 |
| GC13-06 | SCC用于层级布局 | layout | 组件同层/内部确定排列 | 无循环拓扑失败 |
| GC13-07 | 极长标题 | layout | 几何不依赖字体测量或用固定度量 | 跨平台一致 |
| GC13-08 | 浮点坐标序列化 | export | 固定小数/规范数值 | golden稳定 |
| GC13-09 | Mermaid含特殊字符 | export | 正确转义 | 可被解析 |
| GC13-10 | digest超大图 | export | 按预算截断并带摘要 | 不输出不完整JSON |
| GC13-11 | 同一输入Linux/Windows/macOS | golden | 产物字节一致 | 换行固定LF |
| GC13-12 | 未知view mode | layout | UnsupportedLayoutMode或fallback显式 | 不得静默改变 |

## GC-14 增量编译、缓存与资源预算

确保大型图的交互性能，同时保持全量编译等价、可取消和无陈旧缓存。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC14-01 | 第二次编译相同fileHash | cache | 命中全部派生缓存 | 结果逐位相同 |
| GC14-02 | 只改review | cache | 复用语义图算法缓存；重算fileHash/report | contentRoot键生效 |
| GC14-03 | 改一个叶节点 | incremental | 只失效相关索引/路径/布局区 | 与全量编译一致 |
| GC14-04 | 删除高出度节点 | incremental | 失效所有下游依赖 | 无陈旧结果 |
| GC14-05 | 并行编译同一项目 | concurrency | 结果相同且无数据竞争 | Miri/loom可选 |
| GC14-06 | 取消令牌在SCC阶段触发 | cancel | 安全终止且不写缓存 | 可重试 |
| GC14-07 | 内存预算极低 | compile | ResourceLimitExceeded | 无OOM |
| GC14-08 | 路径枚举指数图 | compile | maxPaths/maxWork生效 | 稳定truncated |
| GC14-09 | 5k/50k/100k规模基准 | bench | 达到约定SLO或生成基线报告 | 回归阈值 |
| GC14-10 | 缓存文件损坏 | load cache | 丢弃并全量重算 | 项目不受影响 |
| GC14-11 | 编译器版本变化 | cache key | 旧缓存不复用或执行迁移 | 版本隔离 |
| GC14-12 | 不同线程数 | compile | 语义输出相同 | 并行调度不影响排序 |

## GC-15 CLI、Tauri、Registry、安全与兼容验收

同一 crate 在三个运行面保持一致，并对插件输入、日志、迁移和模糊输入提供硬边界。

| ID | 边界输入/前置条件 | 执行步骤 | 期望结果 | 关键断言 |
| --- | --- | --- | --- | --- |
| GC15-01 | CLI compile有效项目 | 执行命令 | exit 0，stdout机器JSON可选 | stderr无错误 |
| GC15-02 | CLI compile致命错误 | 执行 | 非0稳定退出码 | 诊断JSON完整 |
| GC15-03 | Tauri command与CLI同输入 | 对比 | CompileArtifact一致 | 序列化一致 |
| GC15-04 | Registry服务与桌面同crate版本 | 对比 | 哈希/见证一致 | 版本指纹一致 |
| GC15-05 | API请求含绝对路径 | 诊断 | 输出不泄漏路径，只用逻辑文件名 | 隐私 |
| GC15-06 | 恶意超长错误字段 | 编译 | 诊断截断且安全转义 | 无日志注入 |
| GC15-07 | GraphPatch来自未授权插件 | host调用 | 能力校验失败，编译器不应用 | 安全边界 |
| GC15-08 | TS/Rust fixture全集 | 双实现测试 | 逐字段一致 | 迁移门槛100% |
| GC15-09 | 旧v2 fixture迁移后再编译 | round trip | 二次编译幂等 | 无重复review事件 |
| GC15-10 | canonical serialize→parse→serialize | round trip | 字节相同 | 规范闭包 |
| GC15-11 | 随机fuzz JSON | 持续运行 | 无panic/UB | 错误受控 |
| GC15-12 | 签名验证输入被篡改 | verify | fileHash/contentRoot不匹配 | 拒绝发布 |

# 8. 测试工程目录建议

```text
crates/research-graph-compiler/
├── tests/
│   ├── fixtures/
│   │   ├── canonical/
│   │   ├── invalid/
│   │   ├── migration/
│   │   ├── residual-attention/
│   │   └── large-generated/
│   ├── golden_compile.rs
│   ├── parity_ts.rs
│   ├── patch_atomicity.rs
│   ├── factor_bp.rs
│   └── cli_contract.rs
├── fuzz/
│   ├── fuzz_targets/parse_project.rs
│   ├── fuzz_targets/apply_patch.rs
│   └── fuzz_targets/canonical_roundtrip.rs
└── benches/
    ├── compile_scale.rs
    ├── reachability.rs
    └── loopy_bp.rs
```

## 8.1 示例单元测试

```rust
#[test]
fn evidence_ids_do_not_change_claim_identity() {
    let a = claim_fixture(vec!["ev-a"]);
    let b = claim_fixture(vec!["ev-a", "ev-b"]);

    assert_eq!(block_hash(&a).unwrap(), block_hash(&b).unwrap());

    let pa = project_with_claim(a);
    let pb = project_with_claim(b);
    assert_ne!(file_hash(&pa).unwrap(), file_hash(&pb).unwrap());
}

#[test]
fn patch_is_atomic_when_id_remap_creates_dangling_edge() {
    let base = load_fixture("residual-attention/base.mycproj");
    let patch = load_patch("invalid/dangling-after-remap.json");

    let before = canonical_bytes(&base);
    let result = apply_patch(
        &base,
        plan_patch(&base, &patch, &opts()).unwrap(),
        &opts(),
    );

    assert!(matches!(result, Err(CompileFailure::Invariant(_))));
    assert_eq!(before, canonical_bytes(&base));
}
```

## 8.2 属性测试

```rust
proptest! {
    #[test]
    fn canonicalization_is_idempotent(project in arbitrary_project()) {
        let a = canonicalize(project.clone())?;
        let b = canonicalize(a.clone())?;
        prop_assert_eq!(a, b);
    }

    #[test]
    fn serialization_roundtrip_is_closed(project in arbitrary_valid_project()) {
        let a = canonical_serialize(&canonicalize(project)?)?;
        let parsed = parse_project(&a)?;
        let b = canonical_serialize(&canonicalize(parsed)?)?;
        prop_assert_eq!(a, b);
    }
}
```

# 9. CI 门禁

每个 PR MUST 执行：

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --test parity_ts
cargo fuzz run parse_project -- -max_total_time=60
cargo bench --bench compile_scale -- --save-baseline pr
canvas compile tests/fixtures/residual-attention/project.mycproj --json
```

合并条件：

- 150+项边界测试全部通过；
- Rust/TS 双实现 fixture 100%一致；
- canonical/hash golden 无非预期变化；
- 性能基线回退低于团队设定阈值；
- 无 panic、无未处理 NaN/Infinity、无绝对路径泄漏；
- 所有编译产物均能通过 `parse → canonicalize → serialize` 闭环。

# 10. 完成定义 Definition of Done

图编译器 v1 完成需同时满足：

1. Tauri、CLI、Registry 使用同一 crate；
2. schema v3 规范化和三哈希完全实现；
3. GraphPatch 原子应用和审阅计划可用；
4. BFS/DFS/SCC/路径/消融/Diff 稳定；
5. 六类核心逻辑因子和双通道 BP 可用；
6. p 值、效应量、Bayes factor 和边效力有可追踪校准；
7. 矛盾见证、收敛状态和不确定性均显式输出；
8. 所有派生结果可由任意机器重新编译；
9. 文中每个测试小项至少10组边界用例已自动化；
10. Residual Attention fixture 能产生主效应、交互效应、缺失实验和证据链报告。
