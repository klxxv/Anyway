# 核心工作流：PDF → Agent → Canvas → 逻辑链

> 文档状态：实施总结  
> 关联迭代：#35–#42  
> 目标读者：项目维护者、架构评审者

## 0. 概述

核心工作流定义了 Research Canvas 从学术 PDF 输入到可交互研究画布的完整数据处理管线：

```
PDF 文献 → [PDF Agent] → 结构化提取 → [Agent] → GraphPatch 提案
    → [Canvas] → 因子图编译 → [逻辑链] → 信念传播 + 矛盾检测
```

本工作流横跨四个子系统，由 #35–#42 八个迭代逐步实现。

## 1. 子系统拆分

### 1.1 Canvas CLI 二进制（#35）

`crates/canvas-cli` 提供 `canvas compile` 命令，作为 Rust 图编译器的命令行入口。

- 支持 `--strict`（严格模式）、`--layout`（确定性布局）、`--logic`（逻辑链）、`--bp`（信念传播）
- 输出格式：JSON / Mermaid / Text
- 退出码：0 通过、1 诊断/解析失败、2 用法错误、3 输入不可读
- 同 Rust 内核通过 bit-identical 双实现测试（`tests/compiler-parity.test.ts`）验证一致性

### 1.2 图编译器内核（#36）

`src-tauri/src/graph_compiler/` 拆分为独立 crate `research-graph-compiler`，包含：

- **因子编译**：逻辑图到因子图的确定性编译（GC-08..GC-11）
- **BP 信念传播**：双通道 Loopy BP，支持阻尼收敛与矛盾检测
- **不变式校验**：图结构、类型约束、证据接地
- **确定性布局**：意图驱动的布局算法（§11 of canvas-format-v3.md）

### 1.3 PDF Agent DeepSeek Client（#39）

基于 DeepSeek API 的 PDF→Canvas Agent：

- **API 网关**：统一的 DeepSeek 兼容请求层，支持 `deepseek-v4-flash`/`deepseek-v4-pro`
- **宿主管理**：API Key、网络、文件和审批均由 Tauri 宿主管理
- **多阶段提取**：解析论文结构，提取 question/hypothesis/variable/method/experiment/result/evidence/claim
- **证据锚定**：使用 `section/para/sentence` 逻辑锚，保留 page/offset 验证提示

### 1.4 PDF Agent 多阶段提取 + GraphPatch（#40）

- **结构化提取**：从 PDF 中提取表格数值、p值、效应量、样本量和控制条件
- **变量裂变**：自动从复合节点裂变变量
- **消融实验矩阵**：重建消融实验矩阵
- **关系提议**：`supports`、`contradicts`、`depends_on`、`derived_from`
- **GraphPatch 构建**：输出可审阅的 `PluginGraphPatch`，标记不确定/缺失/混杂/矛盾

### 1.5 EdgeStylePlugin → ThemePlugin 合并（#41）

将 `EdgeStylePlugin` 合并到 `ThemePlugin` 中，统一视觉风格管理。

### 1.6 LLM Provider 抽象（#42）

- **Provider Trait**：抽象的 LLM Provider 接口
- **ProviderPlugin**：可插拔的 Provider 架构
- **OpenAiCompatibleClient**：OpenAI 兼容的通用客户端实现

### 1.7 测试套件（#38）

- **GC-01 解析与规范化测试**
- **GC-02 不变式校验测试**
- **编译器奇偶性测试**：TS↔Rust bit-by-bit parity gate
- **PDF Agent 端到端测试**（`tests/pdf-agent-e2e.test.ts`）
- **Workspace Hooks 测试**（`tests/workspace-hooks.test.ts`）

## 2. 数据流

```
┌──────────┐    ┌──────────────┐    ┌──────────────┐    ┌───────────┐
│ PDF 输入  │───▶│ PDF Agent     │───▶│ GraphPatch    │───▶│ Canvas    │
│ (.pdf)   │    │ (DeepSeek)   │    │ (提案/审阅)   │    │ 渲染器    │
└──────────┘    └──────────────┘    └──────────────┘    └───────────┘
                                          │                    │
                                          ▼                    ▼
                                   ┌──────────────┐    ┌───────────┐
                                   │ 图编译器      │◀───│ 逻辑链    │
                                   │ (Rust 内核)  │    │ 因子图    │
                                   └──────────────┘    └───────────┘
                                          │
                                          ▼
                                   ┌──────────────┐
                                   │ BP 信念传播  │
                                   │ 矛盾检测     │
                                   └──────────────┘
```

## 3. 关键设计原则

1. **Agent 只提案，编译器只计算**：Agent 输出 `GraphPatch` 提案，Rust 编译器独立计算所有确定性图性质
2. **确定性优先**：同一输入在不同环境下（Tauri/Registry/CLI）产生 bit-identical 输出
3. **宿主管理安全边界**：API Key、网络、文件访问均由 Tauri 宿主控制，Agent/插件不可越权
4. **证据锚定**：所有语义关系均锚定于原文 `section/para/sentence`

## 4. 变更规模

| 迭代 | 模块 | 变更文件数 |
|------|------|-----------|
| #35 | Canvas CLI | ~15 |
| #36 | 因子编译 + BP | ~25 |
| #38 | 测试套件 | ~10 |
| #39 | PDF Agent + API 网关 | ~35 |
| #40 | 多阶段提取 + GraphPatch | ~30 |
| #41 | ThemePlugin 合并 | ~10 |
| #42 | LLM Provider 抽象 | ~15 |
| **合计** | | **~140** |

> 总体变更：~140 文件，+26,000 行插入，-2,700 行删除
