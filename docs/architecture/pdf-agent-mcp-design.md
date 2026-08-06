# PDF Agent MCP——Rust 原生高性能

> **版本**: v1.0 | **日期**: 2026-08-06 | **目标 crate**: `pdf-agent-server`
> **传输协议**: MCP (Model Context Protocol) / stdio | **语言**: Rust

---

## §1 产品背景

### 1.1 冷启动的杠杆解

Research Canvas 的核心价值在于「研究图谱」——将论文中的假设、变量、实验、证据结构化。但手工录入是最大的冷启动障碍。

**PDF→Canvas Agent** 是唯一的杠杆解：
- 研究者扔一篇 PDF 进来 → Agent 自动提取假设/变量/实验/证据
- 生成初始 `ProjectState` → 人工审阅 → 接受或修订
- 与传统文献管理工具（Zotero）集成，不硬拷贝 PDF

### 1.2 三层递进模型

```
┌──────────────────────────────────────────────┐
│ L3: 逻辑重建（Logic Layer）                    │
│ Claim 关系图谱 → GraphPatch（review-gated）     │
├──────────────────────────────────────────────┤
│ L2: 语义提取（Semantic Layer）                  │
│ LLM: Claims/Variables/Experiments/Evidence     │
├──────────────────────────────────────────────┤
│ L1: 结构提取（Structure Layer）                 │
│ PDF 文本提取 + 章节/段落/图表标题识别             │
└──────────────────────────────────────────────┘
```

### 1.3 设计原则

| 原则 | 说明 |
|------|------|
| **Rust 原生** | 整个管线在 Rust 中运行，无 Node.js/Python FFI 开销 |
| **安全边界** | PDF 解析永不访问网络；LLM 调用走独立沙箱 |
| **MCP 协议** | 通过 stdio JSON-RPC 2.0 与 Research Canvas 通信 |
| **review-gated** | 所有输出为 `PluginGraphPatch | ReviewRequired`：人审阅后才进入图谱 |
| **可验证** | 每个 Claim/Evidence 带 `evidence_anchors`，指向 PDF 的精确偏移 |

---

## §2 系统架构总览

### 2.1 ASCII 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                     PDF Agent MCP Server                         │
│                                                                  │
│  ┌────────────────┐    ┌────────────────┐    ┌───────────────┐  │
│  │ MCP Transport  │───▶│ Orchestrator   │───▶│ L1 Pipeline   │  │
│  │ (stdio JSON-RPC)│    │ (workflow ctrl)│    │ PDF→Structure │  │
│  └────────────────┘    └───────┬────────┘    └───────┬───────┘  │
│                                │                      │          │
│                                ▼                      ▼          │
│                         ┌────────────┐    ┌──────────────────┐  │
│                         │ L2 Pipeline│◀───│ AnchorMap         │  │
│                         │ LLM→Semantic│   │ offset→locator    │  │
│                         └─────┬──────┘    └──────────────────┘  │
│                               │                                  │
│                               ▼                                  │
│                        ┌──────────────┐                         │
│                        │ GraphPatchGen│                         │
│                        │ Semantic→Patch│                        │
│                        └──────┬───────┘                         │
│                               │                                  │
│  ┌────────────────────────────┼──────────────────────────────┐ │
│  │  External Services         │                               │ │
│  │  ┌──────────┐    ┌────────▼───────┐                       │ │
│  │  │ Zotero   │    │ LLM Provider(s)│                       │ │
│  │  │ Agent MCP│    │ (OpenAI/Claude/ │                       │ │
│  │  │ (stdio)  │    │  local)         │                       │ │
│  │  └──────────┘    └────────────────┘                       │ │
│  └───────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 数据流方向

```
PDF File Path (from Zotero Agent MCP)
  │
  ▼
[L1: Structure Pipeline]
  ├── lopdf: 文本提取 + 页面布局
  ├── 结构识别: 章节/段落/标题层级
  └── AnchorMap: 字符偏移 → (section, para, sentence, page)
  │
  ▼
[L2: Semantic Pipeline]
  ├── Phase 1: Claim 识别（并行）
  ├── Phase 2: Variable 识别（并行）
  ├── Phase 3: Experiment 识别（并行）
  ├── Phase 4: Evidence 引用识别（并行）
  └── Phase 5: Relation 推导（串行，依赖 Phase 1-4）
  │
  ▼
[L3: GraphPatch Generator]
  ├── ClaimCandidate → Node
  ├── RelationCandidate → Edge
  ├── EvidenceCandidate → EvidenceRecord
  └── 确定性 ID 生成（sha256 12 hex）
  │
  ▼
PluginGraphPatch {
  operations: [addNode, addEdge, addEvidence, ...],
  reviewRequired: true
}
```

---

## §3 Cargo 包结构

### 3.1 五个 crate

```
pdf-agent/
├── pdf-pipeline/          # L1: PDF 文本提取与结构识别
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── extractor.rs   # lopdf 文本提取
│       ├── structure.rs   # 章节/段落/图表标题识别
│       └── anchor_map.rs  # 偏移→locator 二分查找映射
│
├── semantic-pipeline/     # L2: 语义提取
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── claims.rs      # Phase 1: Claim 提取
│       ├── variables.rs   # Phase 2: Variable 提取
│       ├── experiments.rs # Phase 3: Experiment 提取
│       ├── evidence.rs    # Phase 4: Evidence 引用提取
│       ├── relations.rs   # Phase 5: Relation 推导
│       └── prompts.rs     # LLM prompt 模板
│
├── graphpatch-gen/        # L3: GraphPatch 生成
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── mapper.rs      # 语义候选 → GraphPatch 映射
│       ├── ids.rs         # 确定性 ID 生成
│       └── validator.rs   # patch 预校验
│
├── shared-schema/         # 共享数据结构
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── ir.rs          # 中间表示（IR）结构体
│       ├── types.rs       # 公共类型
│       └── error.rs       # 错误类型
│
└── pdf-agent-server/      # MCP Server 主二进制
    ├── Cargo.toml
    └── src/
        ├── main.rs        # MCP transport + 启动
        ├── tools.rs       # MCP tool 注册与分发
        ├── orchestrator.rs # 管线编排
        └── providers.rs   # LLM provider 抽象
```

### 3.2 关键依赖

```toml
# pdf-pipeline/Cargo.toml
[dependencies]
lopdf = "0.34"
unicode-segmentation = "1.12"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# semantic-pipeline/Cargo.toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
sha2 = "0.10"

# graphpatch-gen/Cargo.toml
[dependencies]
sha2 = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
shared-schema = { path = "../shared-schema" }

# pdf-agent-server/Cargo.toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
pdf-pipeline = { path = "../pdf-pipeline" }
semantic-pipeline = { path = "../semantic-pipeline" }
graphpatch-gen = { path = "../graphpatch-gen" }
shared-schema = { path = "../shared-schema" }
```

---

## §4 MCP Tool 列表

### 4.1 六个 Tool

| # | Tool Name | 描述 | 输入 | 输出 | 触发管线 |
|---|-----------|------|------|------|---------|
| T1 | `process-paper` | 全管线处理一篇论文 | `{ pdfPath, provider? }` | `{ jobId, status }` | L1→L2→L3（异步） |
| T2 | `extract-structure` | 仅 L1：提取结构 | `{ pdfPath }` | `{ sections, paragraphs, anchorMap }` | L1 |
| T3 | `extract-semantics` | 仅 L2：语义提取 | `{ structuredText, provider? }` | `{ claims, variables, experiments, evidence, relations }` | L2 |
| T4 | `generate-patch` | 仅 L3：生成 GraphPatch | `{ semanticResult }` | `PluginGraphPatch` | L3 |
| T5 | `list-providers` | 列出可用 LLM provider | `{}` | `{ providers: [...] }` | — |
| T6 | `get-paper-status` | 查询处理状态 | `{ jobId }` | `{ status, progress, result? }` | — |

### 4.2 完整 IR 结构体定义

```rust
// shared-schema/src/ir.rs

/// MCP Tool: process-paper 的输入。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPaperInput {
    /// PDF 文件的本地路径（由 Zotero Agent MCP 提供）。
    pub pdf_path: String,
    /// LLM provider（默认 "openai"）。
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_provider() -> String { "openai".to_string() }

/// 异步任务的标识。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobHandle {
    pub job_id: String,
    pub status: JobStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

/// L1 输出：结构化文本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredTextOutput {
    pub sections: Vec<Section>,
    pub paragraphs: Vec<Paragraph>,
    pub figures: Vec<FigureReference>,
    pub tables: Vec<TableReference>,
    pub anchor_map: AnchorMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,        // "s1", "s2.1", …
    pub title: String,
    pub level: u8,         // 1, 2, 3 …
    pub start_offset: usize,
    pub end_offset: usize,
    pub child_section_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    pub id: String,        // "p1", "p2", …
    pub section_id: String,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FigureReference {
    pub id: String,
    pub caption: String,
    pub caption_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReference {
    pub id: String,
    pub caption: String,
    pub caption_offset: usize,
}

/// L2 输出：语义提取结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticExtractionResult {
    pub claims: Vec<ClaimCandidate>,
    pub variables: Vec<VariableCandidate>,
    pub experiments: Vec<ExperimentCandidate>,
    pub evidence_references: Vec<EvidenceReferenceCandidate>,
    pub relations: Vec<RelationCandidate>,
    pub metadata: ExtractionMetadata,
}

/// 论文元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionMetadata {
    pub paper_title: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub provider_used: String,
    pub model_used: String,
    pub duration_seconds: f64,
}

// ── 语义候选类型 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimCandidate {
    pub id: String,
    pub text: String,
    pub claim_type: ClaimType,
    pub confidence: f64,              // 0.0–1.0
    pub evidence_anchors: Vec<AnchorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClaimType {
    Hypothesis,
    Finding,
    Assumption,
    Question,
    Definition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableCandidate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variable_type: VariableType,
    pub evidence_anchors: Vec<AnchorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VariableType {
    Independent,
    Dependent,
    Controlled,
    Measured,
    Derived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentCandidate {
    pub id: String,
    pub label: String,
    pub description: String,
    pub design: Option<String>,
    pub evidence_anchors: Vec<AnchorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReferenceCandidate {
    pub id: String,
    pub text: String,
    pub reference_type: ReferenceType,
    pub evidence_anchors: Vec<AnchorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceType {
    Inline,
    Bibliography,
    Footnote,
    Figure,
    Table,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationCandidate {
    pub id: String,
    pub from_id: String,           // source 候选 ID
    pub to_id: String,             // target 候选 ID
    pub relation_type: RelationType,
    pub polarity: Polarity,
    pub evidence_anchors: Vec<AnchorRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RelationType {
    Supports,
    Contradicts,
    Causes,
    Measures,
    Uses,
    DerivedFrom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Polarity {
    Positive,
    Negative,
    Mixed,
    Unknown,
}

/// PDF 中的精确锚点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorRef {
    pub section_id: String,
    pub paragraph_id: String,
    pub sentence_index: usize,
    pub page: usize,
    pub start_offset: usize,
    pub end_offset: usize,
    pub snippet: String,            // 引文片段（≤200 chars）
}
```

---

## §5 PDF 解析管线（L1）

### 5.1 管线流程

```
PDF File ──▶ lopdf::Document ──▶ 文本提取 ──▶ 结构识别 ──▶ AnchorMap
                │                    │              │
                │                    ▼              ▼
                │              raw_text (UTF-8)   Section[]
                │                                 Paragraph[]
                ▼                                 FigureRef[]
          pages: Vec<Page>                        TableRef[]
```

### 5.2 文本提取（`extractor.rs`）

核心依赖：`lopdf` crate。

```rust
// pdf-pipeline/src/extractor.rs

use lopdf::{Document, Object};
use std::path::Path;

pub struct PdfExtractor {
    document: Document,
    pages: Vec<PageText>,
}

pub struct PageText {
    pub page_number: usize,
    pub text: String,
    pub char_offsets: Vec<CharPosition>,  // 每个字符的 (x, y, page) 位置
}

pub struct CharPosition {
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PdfExtractor {
    /// 从文件路径加载 PDF。
    pub fn open(path: &Path) -> Result<Self, PdfError> {
        let document = Document::load(path)?;
        let pages = Self::extract_pages(&document)?;
        Ok(Self { document, pages })
    }

    fn extract_pages(document: &Document) -> Result<Vec<PageText>, PdfError> {
        // 遍历页面树，提取每页文本内容和字符位置。
        // lopdf 的 content stream 解析需要处理：
        // - BT/ET 文本块
        // - Tj/TJ 文本绘制操作符
        // - Tm 文本矩阵（用于确定字符位置和字号）
        let mut pages = Vec::new();
        for (page_num, page_obj) in document.get_pages() {
            let text = document.extract_text(&[page_num])?;
            let char_offsets = Self::extract_char_positions(document, page_obj)?;
            pages.push(PageText {
                page_number: page_num as usize,
                text,
                char_offsets,
            });
        }
        Ok(pages)
    }

    /// 获取全文本（所有页面拼接）。
    pub fn full_text(&self) -> String {
        self.pages.iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 获取全局字符偏移量（累积偏移，含换行符）。
    pub fn global_offset(&self, page: usize, page_offset: usize) -> usize {
        self.pages.iter()
            .take(page.saturating_sub(1))
            .map(|p| p.text.len() + 1) // +1 for newline
            .sum::<usize>() + page_offset
    }
}
```

### 5.3 结构识别（`structure.rs`）

**识别策略**：

1. **章节标题**：基于字体大小突变 + 编号模式（`^\d+(\.\d+)*\s`）
2. **段落边界**：基于连续空白行或缩进变化
3. **图表标题**：匹配 `Figure \d+:` / `Table \d+:` 模式

```rust
// pdf-pipeline/src/structure.rs

pub struct StructureRecognizer {
    sections: Vec<Section>,
    paragraphs: Vec<Paragraph>,
    figures: Vec<FigureReference>,
    tables: Vec<TableReference>,
}

impl StructureRecognizer {
    /// 从全文和各页字符位置识别结构。
    pub fn recognize(
        full_text: &str,
        pages: &[PageText],
    ) -> Result<Self, StructureError> {
        let mut recognizer = Self::default();
        recognizer.detect_sections(full_text)?;
        recognizer.detect_paragraphs(full_text)?;
        recognizer.detect_figures_tables(full_text)?;
        // 为所有元素计算 start_offset / end_offset
        recognizer.compute_offsets(full_text);
        Ok(recognizer)
    }

    fn detect_sections(&mut self, text: &str) -> Result<(), StructureError> {
        // 正则匹配章节标题：行首 + 编号 + 空格 + 标题文本
        let re = Regex::new(r"(?m)^\s*(\d+(?:\.\d+)*)\s+(.+?)\s*$")?;
        // 字体大小阈值检测（通过 loCharPosition 结合）
        // …
        Ok(())
    }
}
```

### 5.4 锚点映射（`anchor_map.rs`）

**AnchorMap** 是连接 L1 和 L2 的关键桥梁：将全局字符偏移量映射到结构化位置。

```rust
// pdf-pipeline/src/anchor_map.rs

/// offset → (section, paragraph, sentence, page) 的二分查找映射。
pub struct AnchorMap {
    sections: Vec<Section>,
    paragraphs: Vec<Paragraph>,
    /// 每个句子的 (start_offset, end_offset)
    sentences: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedAnchor {
    pub section_id: String,
    pub paragraph_id: String,
    pub sentence_index: usize,
    pub page: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl AnchorMap {
    /// 从全文和已识别的结构构建映射。
    pub fn build(text: &str, sections: &[Section], paragraphs: &[Paragraph]) -> Self;

    /// O(log N) 二分查找：给定全局字符偏移，解析为完整锚点。
    pub fn resolve(&self, global_offset: usize) -> Option<ResolvedAnchor>;

    /// 给定范围 [start, end)，解析为锚点列表（跨段落时有多个）。
    pub fn resolve_range(&self, start: usize, end: usize) -> Vec<ResolvedAnchor>;
}
```

**二分查找实现**：

```
resolve(global_offset):
  1. section  ← binary_search(sections, offset)    // 按 start_offset 排序
  2. paragraph ← binary_search(paragraphs, offset)  // 在 section 内
  3. sentence_idx ← binary_search(sentences, offset) // 按 start_offset 排序
  4. page ← page_of_offset(offset)                  // 从 PageText 累积偏移计算
  5. return ResolvedAnchor{ section_id, paragraph_id, sentence_idx, page, … }
```

---

## §6 语义提取管线（L2）

### 6.1 五个 Phase

| Phase | 名称 | 执行模式 | 输入 | 输出 |
|-------|------|---------|------|------|
| P1 | Claim 提取 | 并行 | 结构化文本 | `Vec<ClaimCandidate>` |
| P2 | Variable 提取 | 并行 | 结构化文本 | `Vec<VariableCandidate>` |
| P3 | Experiment 提取 | 并行 | 结构化文本 | `Vec<ExperimentCandidate>` |
| P4 | Evidence 引用提取 | 并行 | 结构化文本 | `Vec<EvidenceReferenceCandidate>` |
| P5 | Relation 推导 | **串行**（依赖 P1-P4） | Phase 1-4 产物 | `Vec<RelationCandidate>` |

**P5 串行的理由**：Relation 推导需要知道有哪些 Claim/Variable/Experiment 存在，才能确定它们之间的关系。它是一个典型的「先识别实体、再推导关系」的二阶段链路。

### 6.2 LLM Provider 抽象

```rust
// pdf-agent-server/src/providers.rs

use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        response_format: ResponseFormat,
    ) -> Result<String, LlmError>;

    fn name(&self) -> &str;
    fn model(&self) -> &str;
}

pub enum ResponseFormat {
    Text,
    Json,  // 要求 LLM 输出严格 JSON
}

pub struct OpenAiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(&self, system: &str, user: &str, format: ResponseFormat) -> Result<String, LlmError> {
        let response_format = match format {
            ResponseFormat::Json => json!({ "type": "json_object" }),
            ResponseFormat::Text => json!({ "type": "text" }),
        };
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "response_format": response_format,
            "temperature": 0.1  // 低温度，要求确定性输出
        });
        // POST https://api.openai.com/v1/chat/completions …
        todo!()
    }
}
```

### 6.3 并行 + 串行编排

```rust
// pdf-agent-server/src/orchestrator.rs

use tokio::try_join;

pub async fn run_semantic_pipeline(
    structured: &StructuredTextOutput,
    provider: &dyn LlmProvider,
) -> Result<SemanticExtractionResult, PipelineError> {
    // Phase 1-4 并行（四个独立的 LLM 调用）
    let (claims, variables, experiments, evidence) = try_join!(
        extract_claims(structured, provider),
        extract_variables(structured, provider),
        extract_experiments(structured, provider),
        extract_evidence_references(structured, provider),
    )?;

    // Phase 5 串行（依赖 Phase 1-4 产物）
    let relations = extract_relations(
        structured,
        provider,
        &claims,
        &variables,
        &experiments,
        &evidence,
    ).await?;

    Ok(SemanticExtractionResult {
        claims,
        variables,
        experiments,
        evidence_references: evidence,
        relations,
        metadata: ExtractionMetadata::collect(provider, start_time),
    })
}
```

---

## §7 LLM Prompt 模板

### 7.1 Phase 1: Claim 提取

```
System:
You are a research paper analyzer specialized in extracting structured
scientific claims from academic papers. Extract every claim, hypothesis,
finding, assumption, and question from the provided text.

For each claim, provide:
- id: unique identifier (c1, c2, …)
- text: the exact claim text (verbatim from paper, ≤300 chars)
- claim_type: "hypothesis" | "finding" | "assumption" | "question" | "definition"
- confidence: your confidence score (0.0–1.0)
- evidence_anchors: array of anchor references with section_id, paragraph_id,
  sentence_index, page, start_offset, end_offset, and snippet (verbatim text)

Output format: strict JSON with a "claims" array.

User:
## Paper Structure
{JSON representation of sections with their paragraphs}

## Full Text
{structured text with offset annotations}

Extract all scientific claims from this paper.
```

### 7.2 Phase 2: Variable 提取

```
System:
You are a research paper analyzer specialized in extracting variables
from academic papers. Identify all independent, dependent, controlled,
measured, and derived variables described in the text.

For each variable, provide:
- id: "v1", "v2", …
- name: short label (≤50 chars)
- description: what this variable represents
- variable_type: "independent" | "dependent" | "controlled" | "measured" | "derived"
- evidence_anchors: array of anchor references

Output: strict JSON with a "variables" array.
```

### 7.3 Phase 3: Experiment 提取

```
System:
You are a research paper analyzer specialized in extracting experiment
descriptions from academic papers. Identify all experiments, ablation
studies, benchmarks, and evaluation setups.

For each experiment, provide:
- id: "exp1", "exp2", …
- label: short experiment name
- description: what was tested and how
- design: experimental design details (if present)
- evidence_anchors: array of anchor references

Output: strict JSON with an "experiments" array.
```

### 7.4 Phase 4: Evidence 引用提取

```
System:
You are a research paper analyzer. Extract all references to prior work,
datasets, benchmarks, and external evidence from the paper text.

For each reference, provide:
- id: "ref1", "ref2", …
- text: the citation context
- reference_type: "inline" | "bibliography" | "footnote" | "figure" | "table"
- evidence_anchors: array of anchor references

Output: strict JSON with an "evidence_references" array.
```

### 7.5 Phase 5: Relation 推导

```
System:
You are a research paper analyzer. Given the claims, variables, experiments,
and evidence references already extracted, identify the semantic relations
between them.

For each relation, provide:
- id: "rel1", "rel2", …
- from_id: the source entity id (claim, variable, or experiment)
- to_id: the target entity id
- relation_type: "supports" | "contradicts" | "causes" | "measures" | "uses" | "derived_from"
- polarity: "positive" | "negative" | "mixed" | "unknown"
- evidence_anchors: anchor references

Output: strict JSON with a "relations" array.

User:
## Extracted Claims
{JSON array of claims}

## Extracted Variables
{JSON array of variables}

## Extracted Experiments
{JSON array of experiments}

## Evidence References
{JSON array of evidence references}

## Paper Text
{full text}

Identify all semantic relations among the extracted entities.
```

---

## §8 GraphPatch 映射规则

### 8.1 ClaimCandidate → Node 映射表

| ClaimType | Research Node Type | 说明 |
|-----------|-------------------|------|
| Hypothesis | `hypothesis` | 可验证的假设 |
| Finding | `result` | 实验结果/发现 |
| Assumption | `concept` | 前提假设 |
| Question | `question` | 开放研究问题 |
| Definition | `concept` | 概念定义 |

### 8.2 RelationCandidate → Edge 映射表

| RelationType | Research Edge Type | Polarity |
|-------------|-------------------|----------|
| Supports | `supports` | `positive` |
| Contradicts | `contradicts` | `negative` |
| Causes | `causes` | `positive` |
| Measures | `measures` | `positive` |
| Uses | `uses` | `positive` |
| DerivedFrom | `derived_from` | `positive` |

### 8.3 确定性 ID 生成

使用 `sha256` 12 hex（与 `graph_compiler` 中的 `block_hash` 风格一致）：

```rust
// graphpatch-gen/src/ids.rs

use sha2::{Digest, Sha256};

/// 从语义候选生成确定性 ID（12 hex）。
/// 输入：候选类型前缀 + 候选内容文本 + 来源论文 DOI。
/// 相同候选在不同运行中产生相同 ID，避免重复添加。
pub fn generate_candidate_id(prefix: &str, content: &str, doi: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(b":");
    hasher.update(content.as_bytes());
    hasher.update(b":");
    hasher.update(doi.as_bytes());
    hex::encode(&hasher.finalize()[..6]) // 6 bytes = 12 hex chars
}
```

### 8.4 完整的 `build_graph_patch()` 示例

```rust
// graphpatch-gen/src/mapper.rs

use shared_schema::ir::*;

/// 将语义提取结果映射为 PluginGraphPatch。
pub fn build_graph_patch(
    semantic: &SemanticExtractionResult,
    doi: &str,
) -> PluginGraphPatch {
    let mut operations: Vec<GraphPatchOperation> = Vec::new();

    // (1) Claim → add-node
    for claim in &semantic.claims {
        let node_type = map_claim_type(&claim.claim_type);
        let node_id = generate_candidate_id("claim", &claim.text, doi);
        operations.push(GraphPatchOperation::AddNode {
            node: GraphNode {
                id: node_id,
                type_: node_type.to_string(),
                title: claim.text.clone(),
                body: String::new(),
                tags: vec![],
                data: serde_json::json!({}),
            },
        });
    }

    // (2) Variable → add-node
    for var in &semantic.variables {
        let node_id = generate_candidate_id("var", &var.name, doi);
        operations.push(GraphPatchOperation::AddNode {
            node: GraphNode {
                id: node_id,
                type_: "variable".to_string(),
                title: var.name.clone(),
                body: var.description.clone(),
                tags: vec![],
                data: serde_json::json!({
                    "variableType": var.variable_type
                }),
            },
        });
    }

    // (3) Relation → add-edge
    for rel in &semantic.relations {
        let edge_type = map_relation_type(&rel.relation_type);
        let edge_id = generate_candidate_id("rel", &format!("{}->{}", rel.from_id, rel.to_id), doi);
        operations.push(GraphPatchOperation::AddEdge {
            edge: GraphEdge {
                id: edge_id,
                source: rel.from_id.clone(),
                target: rel.to_id.clone(),
                type_: edge_type.to_string(),
                note: None,
                data: serde_json::json!({
                    "polarity": rel.polarity
                }),
            },
        });
    }

    // (4) Evidence → add-evidence
    for ev in &semantic.evidence_references {
        let ev_id = generate_candidate_id("ev", &ev.text, doi);
        operations.push(GraphPatchOperation::AddEvidence {
            evidence: EvidenceRecordData {
                id: ev_id,
                source_type: "paper".to_string(),
                source_id: doi.to_string(),
                title: ev.text.clone(),
                // …
            },
        });
    }

    PluginGraphPatch {
        api_version: "researchcanvas.dev/graph-patch/v1alpha1".to_string(),
        source: PatchSource {
            plugin_id: "pdf-agent".to_string(),
            operation: "process-paper".to_string(),
        },
        title: format!("Extracted from DOI: {}", doi),
        summary: format!(
            "{} claims, {} variables, {} relations, {} evidence refs",
            semantic.claims.len(),
            semantic.variables.len(),
            semantic.relations.len(),
            semantic.evidence_references.len()
        ),
        review_required: true,
        operations,
    }
}

fn map_claim_type(ct: &ClaimType) -> &str {
    match ct {
        ClaimType::Hypothesis => "hypothesis",
        ClaimType::Finding => "result",
        ClaimType::Assumption | ClaimType::Definition => "concept",
        ClaimType::Question => "question",
    }
}

fn map_relation_type(rt: &RelationType) -> &str {
    match rt {
        RelationType::Supports => "supports",
        RelationType::Contradicts => "contradicts",
        RelationType::Causes => "causes",
        RelationType::Measures => "measures",
        RelationType::Uses => "uses",
        RelationType::DerivedFrom => "derived_from",
    }
}
```

### 8.5 与现有类型系统的对齐

| GraphPatch 协议字段 | 对应 `research-types.ts` | 对应 NODE_TYPES / EDGE_TYPES |
|--------------------|--------------------------|------------------------------|
| `add-node.node.type` | `ResearchNode.type` | `NODE_TYPES[14]` 全部支持 |
| `add-edge.edge.type` | `ResearchEdge.type` | `EDGE_TYPES[12]` 全部支持 |
| `add-edge.edge.source/target` | `ResearchEdge.source/target` | — |
| `evidence.sourceId` | `EvidenceRecord.sourceId` | `zotero:{userId}/{itemKey}` |
| `evidence.locator` | `EvidenceRecord.locator` | fileName/page/section/quote/startOffset/endOffset |
| `polarity` | `ResearchEdge.polarity` | positive/negative/mixed/unknown |

---

## §9 Evidence 锚点系统

### 9.1 EvidenceRecord.locator 字段映射

从 `AnchorMap.resolve()` 的 `ResolvedAnchor` 到 `EvidenceRecord.locator`：

```rust
fn anchor_to_locator(anchor: &ResolvedAnchor, pdf_path: &str) -> EvidenceLocator {
    EvidenceLocator {
        file_name: Some(pdf_path.to_string()),
        page: Some(anchor.page),
        section: Some(anchor.section_id.clone()),
        quote: Some(anchor.snippet.clone()),
        start_offset: Some(anchor.start_offset),
        end_offset: Some(anchor.end_offset),
    }
}
```

### 9.2 锚点生命周期

```
PDF 加载 → AnchorMap 构建 ────────────────────────────────────────▶ 释放
              │                                                       ▲
              │ L1: 结构识别时创建                                     │
              │                                                       │
              ▼                                                       │
         L2: LLM 输出 evidence_anchors ──▶ GraphPatch ──▶ EvidenceRecord.locator
              (offset-based)                  (review-gated)  (持久化到 ProjectState)
```

---

## §10 安全边界

### 10.1 PDF 解析的隔离承诺

| 承诺 | 实现方式 |
|------|---------|
| **永不访问网络** | `lopdf` 仅读取本地文件；无 `reqwest` 依赖在 `pdf-pipeline` crate 中 |
| **文件系统限定** | 仅接收路径参数，不自动遍历目录 |
| **解析沙箱** | `lopdf` 的流解析在 Rust 内存安全边界内；不执行 JavaScript |
| **内存限制** | PDF 文件大小限制 50MB（MCP 入口校验） |

### 10.2 LLM 调用的安全

| 承诺 | 实现方式 |
|------|---------|
| **独立沙箱** | LLM 调用通过 `semantic-pipeline` → `reqwest` 发起，不经过 Tauri IPC |
| **敏感字段不泄露** | 仅传递结构化文本和 prompt，不传递项目路径/用户名等 |
| **超时限制** | 每次 LLM 调用硬超时 120s |
| **速率限制** | token-bucket 限制：每分钟最多 10 次 LLM 调用 |

---

## §11 性能目标与估算

### 11.1 目标分解

| 阶段 | 目标耗时 | 说明 |
|------|---------|------|
| L1: PDF 解析 | < 2s | 30 页论文的 lopdf 文本提取 + 结构识别 |
| L2: 语义提取 | < 25s（不含 LLM） | 令牌化 + 编排；LLM 调用受 API 延迟支配 |
| L3: GraphPatch 生成 | < 100ms | 纯内存映射操作 |
| **端到端（不含 LLM）** | **< 30s** | |
| **端到端（含 LLM，5 API 调用）** | **< 60s** | 取决于 LLM API 延迟 |

### 11.2 单篇 30 页论文内存估算

| 组件 | 内存 |
|------|------|
| PDF 文件缓冲区 | ~5 MB |
| 提取文本 | ~200 KB |
| AnchorMap | ~500 KB |
| 语义候选（~50 claims + 30 vars + 5 exps + 20 refs） | ~100 KB |
| LLM 响应缓冲 | ~50 KB |
| **总计** | **~6 MB** |

---

## §12 MCP 协议实现

### 12.1 JSON-RPC 2.0 请求/响应格式

```json
// → Request: process-paper
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "process-paper",
    "arguments": {
      "pdfPath": "/home/researcher/papers/attention-is-all-you-need.pdf",
      "provider": "openai"
    }
  }
}

// ← Response (synchronous for small tools, async job for process-paper)
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"jobId\":\"job-abc123\",\"status\":\"queued\"}"
    }]
  }
}
```

### 12.2 tools/list 响应格式

```json
{
  "tools": [
    {
      "name": "process-paper",
      "description": "Process a PDF paper through the full extraction pipeline (L1→L2→L3). Returns a job handle for async status tracking.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "pdfPath": { "type": "string", "description": "Absolute path to the PDF file" },
          "provider": { "type": "string", "description": "LLM provider name", "default": "openai" }
        },
        "required": ["pdfPath"]
      }
    }
    // … 其余 5 个 tool 的 schema
  ]
}
```

### 12.3 优雅关闭

```rust
// pdf-agent-server/src/main.rs

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = McpServer::new(PdfAgentTools::new()).await?;

    // 监听 SIGTERM/SIGINT → 等待活跃任务完成（最长 30s）→ 退出
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = tx.send(());
    });

    tokio::select! {
        result = server.serve_stdio() => result,
        _ = rx => {
            eprintln!("Shutting down gracefully…");
            server.shutdown(Duration::from_secs(30)).await;
            Ok(())
        }
    }
}
```

---

## §13 部署架构

### 13.1 集成到 Research Canvas

```
Research Canvas (Tauri App)
  │
  ├── spawns: pdf-agent-server (child process, stdio MCP)
  │     │
  │     └── communicates via: JSON-RPC 2.0 over stdin/stdout
  │
  ├── spawns: zotero-agent-server (child process, stdio MCP)
  │     │
  │     └── provides PDF file paths to pdf-agent-server
  │
  └── invokes: GraphPatch review UI
```

### 13.2 Tauri 端配置

```json
// tauri.conf.json (MCP client 配置)
{
  "plugins": {
    "mcp": {
      "servers": {
        "pdf-agent": {
          "command": "pdf-agent-server",
          "args": [],
          "env": {
            "OPENAI_API_KEY": "$OPENAI_API_KEY",
            "ANTHROPIC_API_KEY": "$ANTHROPIC_API_KEY"
          }
        },
        "zotero-agent": {
          "command": "zotero-agent-server",
          "args": [],
          "env": {
            "ZOTERO_DATA_DIR": "$ZOTERO_DATA_DIR"
          }
        }
      }
    }
  }
}
```

### 13.3 独立 CLI 模式

```bash
# 独立运行（不通过 MCP）
pdf-agent process --pdf paper.pdf --provider openai --output patch.json
pdf-agent structure --pdf paper.pdf --output structure.json
pdf-agent semantics --structure structure.json --provider openai --output semantics.json
```

---

## §14 错误处理策略

### 14.1 错误类型与恢复

| 错误类别 | 示例 | 处理 |
|---------|------|------|
| **PDF 解析错误** | 损坏的 PDF、加密的 PDF | 返回错误，不重试 |
| **LLM 超时** | API 超时 120s | 重试 1 次，仍失败则返回部分结果 |
| **LLM 限流** | 429 Too Many Requests | 指数退避重试（1s, 2s, 4s, 8s），最多 3 次 |
| **JSON 解析失败** | LLM 输出非 JSON | 重试时在 prompt 中强调 JSON 格式要求 |
| **Phase 5 失败** | Relation 推导因 token 限制截断 | 分批处理（每批 ≤20 entities） |

### 14.2 优雅降级

当个别 Phase 失败时，Pipeline 仍返回已成功的部分结果：

```rust
pub struct SemanticExtractionResult {
    // … 各字段
    pub partial_failures: Vec<PhaseFailure>,  // 记录失败的 Phase
}

pub struct PhaseFailure {
    pub phase: String,        // "claim_extraction", "variable_extraction", …
    pub error: String,
    pub retried: bool,
}
```

---

## §15 测试策略

### 15.1 测试金字塔

| 层 | 数量 | 内容 |
|----|------|------|
| 单元测试 | 12+ | `AnchorMap::resolve` 二分查找、`generate_candidate_id` 确定性、`map_claim_type` 映射表 |
| 集成测试 | 5 | 每个 L2 Phase 的完整 prompt → JSON 解析回路（使用 LLM mock） |
| E2E 测试 | 2 | 真实 PDF（单页 + 10 页）端到端处理 |
| Prompt 快照 | 5 | 每个 Phase 的 prompt 模板固化快照测试 |

### 15.2 LLM Mock 策略

```rust
#[cfg(test)]
struct MockProvider {
    responses: Vec<String>,
}

#[async_trait]
impl LlmProvider for MockProvider {
    async fn chat(&self, _system: &str, _user: &str, _format: ResponseFormat) -> Result<String, LlmError> {
        Ok(self.responses[0].clone())
    }
}
```

---

## §16 实现路线图

| Phase | 内容 | 预估工时 |
|-------|------|---------|
| **Phase 1: 核心解析** | `pdf-pipeline` crate、lopdf 集成、AnchorMap、结构识别 | 3 天 |
| **Phase 2: 语义管线** | `semantic-pipeline` crate、LLM provider 抽象、5 个 Phase 实现 | 5 天 |
| **Phase 3: GraphPatch** | `graphpatch-gen` crate、映射规则、确定性 ID、验证器 | 2 天 |
| **Phase 4: MCP Server** | `pdf-agent-server` binary、JSON-RPC transport、tool 注册 | 2 天 |
| **Phase 5: 集成与测试** | Tauri 集成、E2E 测试、Prompt 调优 | 3 天 |
| **总计** | | **~15 天** |

---

## §17 附录

### 17.1 与 `research-types.ts` 的对照

| 本文档结构体 | 对应 `research-types.ts` |
|-------------|-------------------------|
| `ClaimCandidate` | `ResearchNode`（当 type = hypothesis/question/concept/result） |
| `VariableCandidate` | `ResearchNode`（当 type = variable） |
| `ExperimentCandidate` | `ResearchNode`（当 type = experiment） |
| `EvidenceReferenceCandidate` | `EvidenceRecord` |
| `RelationCandidate` | `ResearchEdge` |
| `AnchorRef` | `EvidenceRecord.locator`（fileName/page/section/quote/startOffset/endOffset） |
| `PluginGraphPatch` | `PluginGraphPatch`（来自 `contracts.ts`） |
| `generate_candidate_id` | `graph_compiler.rs` 中的 `sha256_hex` + `block_hash`（风格一致，12 hex） |

### 17.2 NODE_TYPES 完整列表（来自 `research-types.ts`）

```
question, concept, variable, hypothesis, method, evidence, paper,
dataset, experiment, result, metric, formula, artifact, note
```

### 17.3 EDGE_TYPES 完整列表（来自 `research-types.ts`）

```
causes, correlates, supports, contradicts, depends_on, derived_from,
part_of, controls, mediates, moderates, uses, measures
```

### 17.4 术语对齐

| 本文档术语 | 代码库术语 | 说明 |
|-----------|-----------|------|
| `contentRootHash` | `content_root_hash` | 语义区根哈希（64 hex） |
| `blockHash` | `block_hash` | 实体内容哈希（12 hex） |
| `fileHash` | `file_hash` | 全文件哈希（64 hex） |
| `PluginGraphPatch` | `PluginGraphPatch`（来自 `contracts.ts`） | `apiVersion: "researchcanvas.dev/graph-patch/v1alpha1"` |
| `reviewRequired: true` | `reviewRequired: true`（来自 `contracts.ts`） | 所有 agent 产出必须人工审阅 |
