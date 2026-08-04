# Torch→Research Canvas 转换插件 · 技术分析

> 本文档是一次**只读分析**的产物：通读 `plugins/sdk/python/research_canvas.py`、
> `scripts/train_mnist_ablation.py`、`examples/python_connector_sdk.py`，并沿
> GraphPatch 协议与 RunResult 导入链路追到前端消费方，为后续编写可把
> `torch.nn.Module` 映射成 Research Canvas 图谱的转换插件做好技术准备。
> 本文不做模型训练，也不改这三份被分析的源码。

---

## 1. 三条主线的任务分工

| 文件 | 角色 | 关键结论 |
|---|---|---|
| `plugins/sdk/python/research_canvas.py` | **去依赖的 Python 参考契约** | 定义 GraphPatch 提案与 `NetworkBlockExtractor` 协议；MVP 不执行 Python 插件，只交换可审阅的 manifest 与 artifact |
| `examples/python_connector_sdk.py` | **文件协议最小示例**（scenario-run） | 读取受限 manifest，产出一个结构化 `RunResult` JSON 供导航器导入 |
| `scripts/train_mnist_ablation.py` | **真实实验工件生成器** | 产出 `app/data/mnist-experiment-results.json`，被 `app/lib/mnist-fixture.ts` 映射为研究图 |

三者共同确认了一件事：**Python（乃至未来的 Torch）侧只负责在宿主之外生成高度结构化的、可审阅的 JSON 契约；宿主只负责校验、展示、在审阅通过后应用。** 两端靠协议文件解耦，宿主永不对插件暴露可变图谱/文件句柄。

---

## 2. Python→Canvas 连接器协议

### 2.1 三层协议

整个链路有三组独立版本化的协议，理解它们才不会在 Torch 适配器里串层：

1. **图谱交换协议** `researchcanvas.dev/graph-patch/v1alpha1`
   - 由 `GraphPatch` JSON 承载；`reviewRequired` 恒为 `true`。
   - 操作集合有界：`add-node` / `add-edge` / `update-node` / `update-edge`。
   - 是 **Torch/ONNX 适配器的唯一返回值通道**(`NetworkBlockExtractor.extract -> GraphPatch`)。
2. **运行清单协议** `research-canvas-run/1`
   - `examples/run-manifest.example.json` 携带 `projectId`、`projectRevision`、`scenario`
     （含 `disabledNodeIds/EdgeIds`、`nodeOverrides/edgeOverrides`、`parameters`）。
   - `create_result` 从 `scenario.parameters` 读取受审阅的参数，交给受审阅的 Python 代码执行后填入结果。
3. **运行结果协议** `research-canvas-result/1`（即 `RunResult`）
   - 由 `python_connector_sdk.create_result()` 产出：`scenarioId + metric + value + summary + artifact{kind,path} + completedAt`。
   - 是 navigator "导入 RunResult" 的输入。

> 设计意图：**scenario (run manifest) 定义"受审阅的变量组合与静态参数"，RunResult 回传"单个实验的度量"。** GraphPatch 则是更高层的语义映射。对一个 Torch 适配器而言，测量指标走 run/result 协议，而模型结构拓扑走 graph-patch 协议。

### 2.2 主机侧唯一入口与审阅门（关键约束）

`examples/python_connector_sdk.py` **明确不执行 manifest 里的任意 shell 文本**，只做两件事：
- 验证稳定 ID（`projectId`、`scenario.id` 必须是非空 string）；
- 校验 `protocolVersion` 精确等于 `research-canvas-run/1`；
- 然后产出 `RunResult`，其余全部交给审阅。

这对应前端 `app/plugins/workspace.ts` 的 `normalizePluginGraphPatch()`（纯校验、不改状态）
与 `app/features/research-workspace/hooks/use-workspace-project.ts` 的 `applyGraphPatch()`
（唯一可应用提案、并创建 layout/provenance/undo/activity 的地方）。`app/features/research-workspace/ResearchWorkspaceApp.tsx`
对 `gitSnapshot?.graphPatch` 先 `normalizePluginGraphPatch` 再在审后进入 `applyGraphPatch`。
**审阅门必须保持；Torch 适配器绝不可自行改图。**

---

## 3. 可用的图谱类型白名单（适配器的"出口约束"）

适配器产出的 node.type / edge.type **必须命中白名单**，否则在
`applyGraphPatch` 里被 `continue` 静默跳过。这是将来最容易踩的坑。

`app/lib/research-types.ts`:

```ts
NODE_TYPES = [question, concept, variable, hypothesis, method, evidence,
              paper, dataset, experiment, result, metric, formula, artifact, note]

EDGE_TYPES = [causes, correlates, supports, contradicts, depends_on,
              derived_from, part_of, controls, mediates, moderates, uses, measures]
```

其它 `applyGraphPatch` 边界（写适配器时要遵守）：
- `add-edge` 两端节点必须已存在（先 add-node 或 update-node）。
- 节点 `status` 固定为 `"draft"`，`provenance.origin="import"`，`actorId=pluginId`。
- `polarity = negative` 当且仅当 edge.type 为 `contradicts`。
- 操作数上限 2000（schema 与 `normalizePluginGraphPatch` 双重限制）；`id/title` 等均有长度上限（见 `graph-patch.schema.json`）。
- GraphPatch 也可以携带 `source.externalId`（用于绑定外部提交/证据引用），与 `mnist-fixture` 里 `sourceRefs` 的语义一致。

---

## 4. MNIST 实验：从 artifact 到研究图的完整映射

`scripts/train_mnist_ablation.py` 是理解"把一个 torch 模型实验落地成研究图"的活范例，
`app/lib/mnist-fixture.ts` 是它的消费端。映射结构：

```
artifact (app/data/mnist-experiment-results.json, schemaVersion=1)
 ├── dataset{name, source, trainSamples, testSamples, inputShape, classes}
 ├── environment{runtime, device, randomState, gitCommit, repository}
 └── results[]
      ├── id/label/hypothesis
      ├── normalized / hiddenUnits / activation   (自变量)
      ├── accuracy / logLoss / iterations / finalTrainingLoss / durationSeconds  (因变量)
      └── deltaAccuracy / evidenceOutcome(baseline|supports|refutes)
```

`mnist-fixture.ts` 的映射模式（Torch 适配器应复用的"名词表"）：
- **节点**：`question`、`variable`(数据/预处理/隐宽/激活/优化器/表征)、`metric`(test accuracy)、`result`(结论)。
- **边**：`supports`(变量→表征)、`contradicts`(反证)、`controls`(约束条件)、`measures`(表征→度量)、`derived_from`(问题→数据)。边携带 `conditions`、`confidence`、`evidenceIds` 与内嵌 `experiment` 块（baseline/Δ/outcome）。
- **EvidenceRecord**：每个 result 转一条 `evidence-<id>`，挂在 metric 节点上，`provenance.origin="python"`，`sourceRefs=[commit:…, seed:…]`。
- **Scenario**：每个非 baseline 变量生成一个 `scenario-<id>`，通过 `disabledNodeIds` 切出"关闭该变量"的对比世界，`parameters` 精确回填训练参数（`normalized/hiddenUnits/activation`）。

> 这套"变量 → 表征 → 度量 → 结论"的骨架，与一个 CNN/MLP 的
> `输入→各层(变量)→hidden 表征→输出(metric)` 是同构的。Torch 适配器可直接照搬该模式。

---

## 5. 现有 Python SDK 对 Torch 的支持（缺口与可复用件）

`plugins/sdk/python/research_canvas.py` 已内建 Torch 化粘合点：

```python
class NetworkBlockExtractor(Protocol):
    """Torch/ONNX adapters implement this without importing application stores."""
    def extract(self, model: Any, *, external_id: str | None = None) -> GraphPatch: ...
```

可直接复用：
- `GraphNodeProposal` / `GraphEdgeProposal`（`to_operation()`，`node_id/node_type/title/body/tags/data`）；
- `GraphPatch`（`to_mapping()`，自动注入 `apiVersion` 与 `reviewRequired=True`）；
- `PluginManifest` / `PluginContext` / `ResearchCanvasPlugin`（宿主生命周期接口，当前为去依赖描述性契约）。

缺口（留给实现阶段）：
- **没有** torch/nn 感知的遍历器：需要一个 `torch.nn.Module` 递归遍历器（按 `named_children`/`named_modules`）来产出稳定的 `node_id`（如 `module:<path>`）与拓扑边。
- **没有** 张量形状/参数统计 → `data` 负载的序列化助手。
- `extract` 目前只拿 `model`；对带输入样例的 ONNX 图导出，可在实现时再扩展。

---

## 6. 结论与开工前的"技术准备清单"

1. **协议分层写清楚**：Torch 适配器产 GraphPatch（拓扑），实验度量产 RunResult（数值），两者都走审阅门，均不可直改图谱。
2. **node/edge 类型一定落在白名单内**，否则被宿主静默丢弃。
3. **add-edge 前必须保证 source/target 节点已存在**；`contradicts` 对应 `polarity=negative`。
4. **复用** `NetworkBlockExtractor` + `GraphPatch*Proposal`：新增一个不依赖应用 store 的
   `torch_extractor.py`（递归 `named_modules` → add-node / add-edge，天然符合 `graph-patch.schema.json` 的有界约束）。
5. **落盘位置对齐 `examples/python_connector_sdk.py`**：输出仍为受审阅的结构化 JSON；MV PV 阶段宿主只审阅、不执行 Torch 代码。
6. **后续可测性**：`tests/plugin-sdk.test.ts` 已校验 Python SDK 必须含 `NetworkBlockExtractor(Protocol)` 与 `"reviewRequired": True`；`tests/platform.test.ts` 校验 `normalizePluginGraphPatch` 的审阅门。Torch 适配器应补一个 `tests/` 用例，用 `mirror graph-patch.schema.json` 校验其产出的 Patch（对齐 `tests/plugin-sdk.test.ts:27` 的模式）。

---

*文档性质：技术准备 / 只读分析。未改动 `research_canvas.py`、`train_mnist_ablation.py`、`python_connector_sdk.py`。*
