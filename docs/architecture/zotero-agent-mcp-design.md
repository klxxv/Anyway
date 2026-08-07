# Zotero Agent MCP

> **版本**: v1.0 | **日期**: 2026-08-06 | **目标 crate**: `zotero-agent-server`
> **传输协议**: MCP (Model Context Protocol) / stdio | **语言**: Rust

---

## §1 概述

### 1.1 设计目标

Zotero Agent MCP Server 是 Research Canvas 的「文献网关」——它通过本地 Zotero SQLite 数据库，为上层 Agent（特别是 PDF Agent MCP）和用户提供文献查询能力。

| 目标 | 含义 |
|------|------|
| **本地优先** | 全部通过本地 Zotero SQLite 数据库操作，不调用 Zotero Web API，离线可用 |
| **安全** | 仅返回文件路径和元数据，不传输 PDF 内容；文件系统访问限定 Zotero 数据目录 |
| **可验证** | 漂移检测：PDF sha256 与 asset.sha256 比对，不一致时触发重新锚定提示 |
| **可组合** | 作为 MCP Server 子进程运行，供 PDF Agent MCP 和 Research Canvas 调用 |

### 1.2 在 Research Canvas 生态中的位置

```
┌──────────────────────────────────────────────────────────────┐
│                     Research Canvas (Tauri)                   │
│                                                               │
│  ┌─────────────────┐     ┌──────────────┐                    │
│  │ PDF Agent MCP   │────▶│ Zotero Agent │                    │
│  │ (语义提取)       │     │ MCP (文献网关)│                    │
│  └────────┬────────┘     └──────┬───────┘                    │
│           │                      │                            │
│           │ getAttachment(path)  │ SQLite (本地)               │
│           │                      │                            │
│           ▼                      ▼                            │
│     PDF File ◀──────── Zotero Data Directory                  │
│                                                               │
│  产出: PluginGraphPatch ──▶ Workspace Project                 │
│        (review-gated)        (EvidenceRecord.sourceId         │
│                                = "zotero:{userId}/{itemKey}") │
└──────────────────────────────────────────────────────────────┘
```

---

## §2 MCP Server 架构

### 2.1 独立 Rust 二进制

```
zotero-agent/
├── Cargo.toml
└── src/
    ├── main.rs          # MCP transport (stdio JSON-RPC 2.0)
    ├── mcp/
    │   ├── mod.rs       # MCP 协议处理
    │   ├── tools.rs     # Tool 注册与分发
    │   └── types.rs     # JSON-RPC 类型
    ├── zotero/
    │   ├── mod.rs       # Zotero SQLite 连接管理
    │   ├── schema.rs    # Schema 映射与查询
    │   ├── search.rs    # 搜索实现
    │   └── attachments.rs # 附件路径解析
    ├── evidence/
    │   ├── mod.rs       # 证据身份集成
    │   ├── source_id.rs # sourceId 格式定义
    │   └── mapping.rs   # EvidenceRecord 字段映射
    └── security/
        ├── mod.rs       # 安全模块
        └── sandbox.rs   # PathSandbox 路径限定
```

### 2.2 关键依赖

```toml
# zotero-agent/Cargo.toml
[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
hex = "0.4"
tokio = { version = "1", features = ["full"] }
dirs = "6"          # 平台无关的用户目录
regex = "1"
```

### 2.3 模块职责

| 模块 | 职责 | 对外暴露 |
|------|------|---------|
| `mcp/` | JSON-RPC 2.0 协议实现，tool 注册与路由 | `McpServer` |
| `zotero/` | SQLite 连接池、Schema 查询、搜索、附件 | `ZoteroDb` |
| `evidence/` | sourceId 格式、EvidenceRecord 映射、锚定三角形 | `EvidenceIdentity` |
| `security/` | PathSandbox、路径规范化、符号链接拒绝 | `PathSandbox` |

---

## §3 MCP Tools 设计

### 3.1 八个 Tool

| # | Tool Name | 描述 | 输入 | 输出 |
|---|-----------|------|------|------|
| T1 | `zotero.search` | 搜索本地 Zotero 库 | `{ query, limit?, collectionId? }` | `{ items: ZoteroItem[] }` |
| T2 | `zotero.getAttachment` | 获取条目附件路径 | `{ sourceId }` | `{ path, sha256?, mimeType? }` |
| T3 | `zotero.getBibliography` | 获取 BibTeX/CSL 引用 | `{ sourceId, format? }` | `{ bibliography: string }` |
| T4 | `zotero.resolveCollection` | 列出集合内条目 | `{ collectionId }` | `{ items: ZoteroItem[] }` |
| T5 | `zotero.getAnnotations` | 获取 PDF 批注 | `{ sourceId }` | `{ annotations: Annotation[] }` |
| T6 | `zotero.verifyAnchor` | 验证锚点完整性 | `{ sourceId, sha256 }` | `{ valid, drift?: DriftInfo }` |
| T7 | `zotero.listCollections` | 列出所有集合 | `{}` | `{ collections: Collection[] }` |
| T8 | `zotero.getItem` | 获取单个条目详情 | `{ sourceId }` | `{ item: ZoteroItem }` |

### 3.2 完整签名定义

```rust
// zotero-agent/src/mcp/types.rs

/// T1: zotero.search 输入。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInput {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub collection_id: Option<String>,
}

fn default_limit() -> usize { 20 }

/// T1 输出：搜索结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutput {
    pub items: Vec<ZoteroItem>,
    pub total: usize,
}

/// T2: zotero.getAttachment 输入/输出。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAttachmentInput {
    pub source_id: String,  // "zotero:{userId}/{itemKey}"
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentOutput {
    pub path: String,           // PDF 文件的绝对路径
    pub sha256: Option<String>,  // 若本地已计算
    pub mime_type: Option<String>,
}

/// T3: zotero.getBibliography 输入/输出。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBibliographyInput {
    pub source_id: String,
    #[serde(default = "default_format")]
    pub format: BibliographyFormat,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BibliographyFormat {
    BibTeX,
    CSL,
}

fn default_format() -> BibliographyFormat { BibliographyFormat::BibTeX }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibliographyOutput {
    pub bibliography: String,
    pub format: BibliographyFormat,
}

/// T5: zotero.getAnnotations 输出。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationsOutput {
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: String,
    pub text: String,
    pub comment: Option<String>,
    pub color: Option<String>,
    pub page: usize,
    pub rect: Option<AnnotationRect>,  // PDF 页面上的矩形区域
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// T6: zotero.verifyAnchor 输出。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorVerification {
    pub valid: bool,
    pub drift: Option<DriftInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftInfo {
    pub current_sha256: String,
    pub expected_sha256: String,
    pub recommendation: String,  // "请重新锚定证据引用"
}

/// T7: zotero.listCollections 输出。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionsOutput {
    pub collections: Vec<Collection>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub item_count: usize,
}

/// Zotero 条目的统一表示。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroItem {
    pub source_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub item_type: String,        // "journalArticle", "book", "thesis", …
    pub publication: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub abstract_text: Option<String>,
    pub date_added: Option<String>,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
    pub attachments: Vec<AttachmentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub path: Option<String>,     // 绝对路径
    pub sha256: Option<String>,
}
```

---

## §4 Zotero SQLite Schema 映射

### 4.1 数据库位置（跨平台）

| 平台 | 默认路径 |
|------|---------|
| Linux | `~/Zotero/zotero.sqlite` |
| macOS | `~/Zotero/zotero.sqlite` |
| Windows | `%APPDATA%\Zotero\Zotero\Profiles\{profile}\zotero.sqlite` |

配置优先级：环境变量 `ZOTERO_DATA_DIR` > 默认路径。

### 4.2 核心表结构 ER 图

```
┌─────────────┐       ┌──────────────────┐       ┌─────────────┐
│   items     │       │  itemAttachments │       │ collections │
├─────────────┤       ├──────────────────┤       ├─────────────┤
│ itemID (PK) │──1:N──│ itemID (FK)      │       │ collectionID│
│ itemTypeID  │       │ parentItemID     │       │ collectionNm│
│ key         │       │ contentType      │       │ parentCollID│
│ dateAdded   │       │ path             │       └──────┬──────┘
│ dateModified│       └──────────────────┘              │
└──────┬──────┘                                         │
       │                                                │
       │ 1:N                                            │ N:M
       ▼                                                ▼
┌──────────────┐                              ┌──────────────────┐
│ itemData     │                              │ collectionItems  │
├──────────────┤                              ├──────────────────┤
│ itemID (FK)  │                              │ collectionID(FK) │
│ fieldID (FK) │                              │ itemID (FK)      │
│ valueID (FK) │                              └──────────────────┘
└──────┬───────┘
       │
       ▼
┌──────────────┐       ┌──────────────┐
│  fields      │       │ itemDataValues│
├──────────────┤       ├──────────────┤
│ fieldID (PK) │       │ valueID (PK)  │
│ fieldName    │       │ value         │
└──────────────┘       └──────────────┘

┌──────────────────┐       ┌──────────────┐
│  itemTypes       │       │ itemTags     │
├──────────────────┤       ├──────────────┤
│ itemTypeID (PK)  │       │ itemID (FK)  │
│ typeName         │       │ tagID  (FK)  │
└──────────────────┘       └──────┬───────┘
                                  │
                                  ▼
                          ┌──────────────┐
                          │  tags        │
                          ├──────────────┤
                          │ tagID (PK)   │
                          │ name         │
                          └──────────────┘

┌──────────────────┐
│ itemAnnotations  │
├──────────────────┤
│ itemID (FK)      │
│ parentItemID     │
│ type             │
│ text             │
│ comment          │
│ color            │
│ pageLabel        │
│ position         │  -- JSON: {"x":..., "y":..., "width":..., "height":...}
│ sortIndex        │
└──────────────────┘
```

### 4.3 关键 SQL 查询

```sql
-- 将 Zotero item key 转换为内部 itemID
SELECT itemID FROM items WHERE key = ?1;

-- 获取条目类型的 typeName
SELECT it.typeName
FROM items i
JOIN itemTypes it ON i.itemTypeID = it.itemTypeID
WHERE i.itemID = ?1;

-- 获取条目的完整元数据（所有字段聚合）
SELECT f.fieldName, idv.value
FROM itemData ida
JOIN fields f ON ida.fieldID = f.fieldID
JOIN itemDataValues idv ON ida.valueID = idv.valueID
WHERE ida.itemID = ?1;

-- 全文搜索（标题 + 摘要 + 作者）
SELECT i.itemID, i.key
FROM items i
WHERE i.itemID IN (
    SELECT ida.itemID FROM itemData ida
    JOIN itemDataValues idv ON ida.valueID = idv.valueID
    WHERE idv.value LIKE ?1
    LIMIT ?2
);

-- 获取条目附件
SELECT ia.itemID, ia.path, ia.contentType
FROM itemAttachments ia
WHERE ia.parentItemID = ?1
  AND ia.contentType = 'application/pdf';

-- 获取条目标签
SELECT t.name
FROM itemTags it
JOIN tags t ON it.tagID = t.tagID
WHERE it.itemID = ?1;

-- 获取 PDF 批注
SELECT ann.type, ann.text, ann.comment, ann.color,
       ann.pageLabel, ann.position, ann.sortIndex
FROM itemAnnotations ann
WHERE ann.parentItemID = ?1  -- parentItemID = 附件的 itemID
ORDER BY ann.sortIndex;

-- 获取集合内所有条目
SELECT i.itemID, i.key
FROM items i
JOIN collectionItems ci ON i.itemID = ci.itemID
WHERE ci.collectionID = ?1
ORDER BY ci.orderIndex;

-- 列出所有集合
SELECT c.collectionID, c.collectionName, c.parentCollectionID,
       (SELECT COUNT(*) FROM collectionItems ci WHERE ci.collectionID = c.collectionID) AS item_count
FROM collections c
ORDER BY c.collectionName;
```

### 4.4 完整元数据聚合查询

```sql
-- 获取单个条目的所有信息：基本信息 + 类型 + 元数据 + 标签 + 附件
WITH item_base AS (
    SELECT i.itemID, i.key, i.dateAdded, i.dateModified,
           it.typeName AS itemType, i.version
    FROM items i
    JOIN itemTypes it ON i.itemTypeID = it.itemTypeID
    WHERE i.itemID = ?1
),
item_meta AS (
    SELECT ida.itemID,
           MAX(CASE WHEN f.fieldName = 'title'       THEN idv.value END) AS title,
           MAX(CASE WHEN f.fieldName = 'abstractNote' THEN idv.value END) AS abstract,
           MAX(CASE WHEN f.fieldName = 'publicationTitle' THEN idv.value END) AS publication,
           MAX(CASE WHEN f.fieldName = 'volume'      THEN idv.value END) AS volume,
           MAX(CASE WHEN f.fieldName = 'issue'       THEN idv.value END) AS issue,
           MAX(CASE WHEN f.fieldName = 'pages'       THEN idv.value END) AS pages,
           MAX(CASE WHEN f.fieldName = 'DOI'         THEN idv.value END) AS doi,
           MAX(CASE WHEN f.fieldName = 'url'         THEN idv.value END) AS url,
           MAX(CASE WHEN f.fieldName = 'date'        THEN idv.value END) AS date,
           MAX(CASE WHEN f.fieldName = 'extra'       THEN idv.value END) AS extra
    FROM itemData ida
    JOIN fields f ON ida.fieldID = f.fieldID
    JOIN itemDataValues idv ON ida.valueID = idv.valueID
    WHERE ida.itemID = ?1
    GROUP BY ida.itemID
)
SELECT *
FROM item_base
LEFT JOIN item_meta USING (itemID);
```

---

## §5 证据身份集成

### 5.1 sourceId 格式

```
sourceId = "zotero:{userId}/{itemKey}"
```

其中：
- `userId`：Zotero 用户 ID（本地数据库中有唯一用户）
- `itemKey`：Zotero 条目在数据库中的 `items.key`（8 位随机字符串，如 "ABCD1234"）

示例：`"zotero:12345678/ABCD1234"`

### 5.2 与 EvidenceRecord 的集成

```typescript
// TypeScript 侧使用例
const evidenceRecord: EvidenceRecord = {
  id: "ev-paper-attention-2023",
  sourceType: "paper",
  sourceId: "zotero:12345678/ABCD1234",  // ← Zotero Agent MCP 提供的 sourceId
  title: "Attention Is All You Need",
  authors: "Vaswani et al.",
  year: 2017,
  doi: "10.48550/arXiv.1706.03762",
  url: "https://arxiv.org/abs/1706.03762",
  locator: {
    fileName: "/path/to/attention-is-all-you-need.pdf",
    page: 7,
    section: "4.2",
    quote: "The encoder is composed of a stack of N = 6 identical layers.",
    startOffset: 12345,
    endOffset: 12450,
  },
  status: "verified",
  provenance: {
    origin: "ai",
    modelId: "pdf-agent-v1",
  },
};
```

### 5.3 锚定三角形

每个证据引用由三个身份信号交叉验证：

```
        sourceId (zotero:{userId}/{itemKey})
             │
             │  "哪篇论文？"
             │
    ┌────────┼────────┐
    │        │        │
    ▼        ▼        ▼
  doi      sha256    anchor (locator: page/section/sentence/offset)
  "永久身份"  "内容指纹"   "精确位置"
```

- **sourceId**：Zotero 条目唯一标识，回答「哪篇论文？」
- **doi**：跨系统的永久身份，即使 Zotero 库丢失也能追踪
- **sha256**：PDF 文件的 sha256，回答「论文是否被修改过？」
- **anchor**：PDF 内的精确位置（页面/段落/句子/偏移），回答「这句话在论文的哪里？」

---

## §6 漂移检测

### 6.1 完整检测流程

```
User opens a project that references "zotero:{userId}/{itemKey}"
  │
  ▼
Research Canvas calls: zotero.verifyAnchor(sourceId, expectedSha256)
  │
  ▼
Zotero Agent MCP:
  │
  ├── 1. resolve sourceId → itemID
  │
  ├── 2. find PDF attachment (contentType = "application/pdf")
  │
  ├── 3. compute sha256 of the PDF file on disk
  │     (streaming: read in 8KB chunks, avoids loading full file into memory)
  │
  ├── 4. compare:
  │      currentSha256 == expectedSha256?
  │        │
  │        ├── YES → return { valid: true }
  │        │
  │        └── NO  → return {
  │                    valid: false,
  │                    drift: {
  │                      currentSha256,
  │                      expectedSha256,
  │                      recommendation: "PDF 内容已变更，请重新锚定证据引用"
  │                    }
  │                  }
```

### 6.2 Rust 实现

```rust
// zotero-agent/src/evidence/mod.rs

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// 流式计算文件的 sha256，避免将整个文件加载到内存。
pub fn compute_file_sha256(path: &Path) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(8192, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// 验证锚点完整性。
pub fn verify_anchor(
    db: &ZoteroDb,
    source_id: &str,
    expected_sha256: &str,
) -> Result<AnchorVerification, ZoteroError> {
    let attachment = db.get_pdf_attachment(source_id)?;
    let current_sha256 = compute_file_sha256(Path::new(&attachment.path))?;

    if current_sha256 == expected_sha256 {
        Ok(AnchorVerification {
            valid: true,
            drift: None,
        })
    } else {
        Ok(AnchorVerification {
            valid: false,
            drift: Some(DriftInfo {
                current_sha256,
                expected_sha256: expected_sha256.to_string(),
                recommendation: "PDF 内容已变更，请重新锚定证据引用".to_string(),
            }),
        })
    }
}
```

### 6.3 重新锚定流程

当漂移被检测到时，UI 层触发重新锚定：

```
检测到漂移
  │
  ▼
Research Canvas UI:
  ├── 标记受影响的 EvidenceRecord 状态为 "disputed"
  ├── 通知用户：「论文 XX 的 PDF 版本已变更，需要重新锚定 Y 条证据引用」
  │
  ▼
用户操作:
  ├── [重新锚定] → 启动 PDF Agent 重新提取证据锚点
  │                 → 更新 EvidenceRecord.locator
  │                 → 更新 asset.sha256
  │
  ├── [忽略] → 保留旧锚点，status 保持 "disputed"
  │
  └── [删除] → 移除受影响的证据引用
```

---

## §7 安全设计

### 7.1 PathSandbox 实现

```rust
// zotero-agent/src/security/sandbox.rs

use std::path::{Path, PathBuf};
use std::fs;

/// 路径沙箱：确保所有文件访问都限定在 Zotero 数据目录内。
pub struct PathSandbox {
    root: PathBuf,
}

impl PathSandbox {
    /// 创建一个新的沙箱，以 `zotero_data_dir` 为根。
    pub fn new(zotero_data_dir: PathBuf) -> Result<Self, SecurityError> {
        let root = fs::canonicalize(&zotero_data_dir)
            .map_err(|_| SecurityError::InvalidRoot(zotero_data_dir))?;
        if !root.is_dir() {
            return Err(SecurityError::NotADirectory(root));
        }
        Ok(Self { root })
    }

    /// 规范化给定路径并验证它在根目录内。
    pub fn resolve<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, SecurityError> {
        // 1. 仅当 path 是相对路径时才拼接 root
        let resolved = if path.as_ref().is_relative() {
            self.root.join(path.as_ref())
        } else {
            path.as_ref().to_path_buf()
        };

        // 2. 规范化（消除 ..、.、符号链接解析）
        let canonical = fs::canonicalize(&resolved)
            .map_err(|_| SecurityError::PathNotFound(resolved))?;

        // 3. 验证在 root 内
        if !canonical.starts_with(&self.root) {
            return Err(SecurityError::SandboxEscape {
                path: canonical,
                root: self.root.clone(),
            });
        }

        Ok(canonical)
    }

    /// 验证路径存在且是文件（拒绝符号链接）。
    pub fn resolve_file<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, SecurityError> {
        let resolved = self.resolve(path)?;
        let metadata = fs::symlink_metadata(&resolved)
            .map_err(|_| SecurityError::PathNotFound(resolved.clone()))?;

        if metadata.file_type().is_symlink() {
            return Err(SecurityError::SymlinkRejected(resolved));
        }
        if !metadata.is_file() {
            return Err(SecurityError::NotAFile(resolved));
        }
        Ok(resolved)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("invalid sandbox root: {0}")]
    InvalidRoot(PathBuf),
    #[error("sandbox root is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("path not found: {0}")]
    PathNotFound(PathBuf),
    #[error("symlink rejected: {0}")]
    SymlinkRejected(PathBuf),
    #[error("not a file: {0}")]
    NotAFile(PathBuf),
    #[error("sandbox escape: {path} outside {root}")]
    SandboxEscape { path: PathBuf, root: PathBuf },
}
```

### 7.2 安全原则表

| 原则 | 实现 |
|------|------|
| **不传输 PDF 内容** | 仅返回文件路径（字符串），不读取 PDF 文件内容 |
| **只读数据库** | SQLite 连接以 `SQLITE_OPEN_READ_ONLY` 模式打开 |
| **无网络** | crate 不依赖 `reqwest`、`hyper`、或任何网络库 |
| **路径限定** | `PathSandbox` 确保所有路径访问在 Zotero 数据目录内 |
| **拒绝符号链接** | `symlink_metadata` + `file_type().is_symlink()` 检查 |
| **SQL 注入防范** | 全部使用参数化查询（`rusqlite` 的 `?1` / `?2` 绑定） |
| **资源限制** | 搜索结果上限 100 条；附件路径解析超时 5s |

---

## §8 与 PDF Agent 协作

### 8.1 九步协作流程

```
Step 1: 用户选择一篇 Zotero 论文
Step 2: Research Canvas 调用 zotero.getItem(sourceId)
Step 3: 显示论文元数据（标题、作者、年份、doi）
Step 4: 用户点击「提取到 Canvas」
Step 5: Research Canvas 调用 zotero.getAttachment(sourceId)
Step 6: Zotero Agent 返回 PDF 文件路径
Step 7: Research Canvas 调用 pdf-agent.process_paper(pdfPath)
Step 8: PDF Agent 提取语义，生成 PluginGraphPatch
Step 9: 用户审阅并接受 GraphPatch → 进入图谱
```

### 8.2 GraphPatch 中的 Zotero 证据节点

由 PDF Agent 生成、Zotero Agent 提供 sourceId 的证据节点：

```json
{
  "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
  "source": {
    "pluginId": "pdf-agent",
    "operation": "process-paper"
  },
  "operations": [
    {
      "op": "add-evidence",
      "evidence": {
        "id": "ev-paper-attention",
        "sourceType": "paper",
        "sourceId": "zotero:12345678/ABCD1234",
        "title": "Attention Is All You Need",
        "authors": "Vaswani, Shazeer, Parmar, Uszkoreit, Jones, Gomez, Kaiser, Polosukhin",
        "year": 2017,
        "doi": "10.48550/arXiv.1706.03762",
        "locator": {
          "fileName": "/home/researcher/Zotero/storage/ABCD1234/attention-is-all-you-need.pdf",
          "page": 7,
          "section": "4.2",
          "quote": "The encoder is composed of a stack of N = 6 identical layers.",
          "startOffset": 12345,
          "endOffset": 12450
        }
      }
    }
  ]
}
```

### 8.3 角色职责契约

| 角色 | Zotero Agent MCP | PDF Agent MCP |
|------|-----------------|---------------|
| **提供 PDF 路径** | ✅ `getAttachment(sourceId)` → path | ❌ |
| **提供文献元数据** | ✅ `getItem`, `getBibliography` | ❌ |
| **PDF 文本提取** | ❌ | ✅ L1 Pipeline |
| **语义提取（LLM）** | ❌ | ✅ L2 Pipeline |
| **GraphPatch 生成** | ❌ | ✅ L3 Pipeline |
| **漂移检测** | ✅ `verifyAnchor` | ❌ |
| **PDF 批注提取** | ✅ `getAnnotations` | ❌ |

### 8.4 错误码约定

| 错误码 | 含义 | 来自 |
|--------|------|------|
| `ZOTERO_DB_NOT_FOUND` | 未找到 Zotero 数据库 | Zotero Agent |
| `ZOTERO_ITEM_NOT_FOUND` | 条目不存在 | Zotero Agent |
| `ZOTERO_NO_PDF_ATTACHMENT` | 条目无 PDF 附件 | Zotero Agent |
| `ZOTERO_SANDBOX_ESCAPE` | 路径出界 | Zotero Agent |
| `PDF_PARSE_FAILED` | PDF 解析失败 | PDF Agent |
| `PDF_LLM_TIMEOUT` | LLM 调用超时 | PDF Agent |
| `GRAPH_PATCH_INVALID` | GraphPatch 校验不通过 | PDF Agent |

---

## §9 配置

### 9.1 MCP 客户端配置

```json
{
  "mcpServers": {
    "zotero-agent": {
      "command": "zotero-agent-server",
      "args": [],
      "env": {
        "ZOTERO_DATA_DIR": "/home/researcher/Zotero",
        "ZOTERO_PROFILE": "default"
      }
    }
  }
}
```

### 9.2 环境变量

| 变量 | 描述 | 默认值 |
|------|------|--------|
| `ZOTERO_DATA_DIR` | Zotero 数据目录的绝对路径 | 平台默认（`~/Zotero` 或 `%APPDATA%\Zotero`） |
| `ZOTERO_PROFILE` | Zotero 配置文件名称 | 自动检测最新配置 |
| `ZOTERO_SEARCH_LIMIT` | 搜索最大返回数 | `100` |
| `LOG_LEVEL` | 日志级别 | `info` |

---

## §10 MCP 协议实现

### 10.1 JSON-RPC 2.0 请求/响应示例

```json
// → Request: zotero.search
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "zotero.search",
    "arguments": {
      "query": "transformer attention mechanism",
      "limit": 10
    }
  }
}

// ← Response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"items\":[{\"sourceId\":\"zotero:12345678/ABCD1234\",\"title\":\"Attention Is All You Need\",\"authors\":[\"Vaswani\"],\"year\":2017,\"doi\":\"10.48550/arXiv.1706.03762\",\"itemType\":\"journalArticle\"}],\"total\":1}"
    }]
  }
}

// → Request: zotero.getAttachment
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "zotero.getAttachment",
    "arguments": {
      "sourceId": "zotero:12345678/ABCD1234"
    }
  }
}

// ← Response
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"path\":\"/home/researcher/Zotero/storage/ABCD1234/attention-is-all-you-need.pdf\",\"sha256\":null,\"mimeType\":\"application/pdf\"}"
    }]
  }
}
```

### 10.2 tools/list 格式

```json
{
  "tools": [
    {
      "name": "zotero.search",
      "description": "搜索本地 Zotero 库，返回匹配的文献条目。",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "搜索关键词" },
          "limit": { "type": "integer", "default": 20 },
          "collectionId": { "type": "string" }
        },
        "required": ["query"]
      }
    }
    // … 其余 7 个 tool 的 schema
  ]
}
```

### 10.3 优雅关闭

```rust
// zotero-agent/src/main.rs

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化 Zotero 数据库连接
    let zotero_db = ZoteroDb::open(&get_zotero_data_dir()?)?;

    // 2. 创建 MCP Server
    let server = McpServer::new(ZoteroAgentTools::new(zotero_db)).await?;

    // 3. 监听 SIGTERM/SIGINT
    let shutdown = tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        eprintln!("Zotero Agent MCP shutting down…");
    });

    // 4. 运行
    tokio::select! {
        result = server.serve_stdio() => result,
        _ = shutdown => Ok(()),
    }
}
```

---

## §11 实现路线图

| Phase | 内容 | 预估工时 |
|-------|------|---------|
| **Phase 1: 核心数据层** | SQLite 连接、Schema 映射、`getItem`/`search` 实现 | 2 天 |
| **Phase 2: 完整工具集** | 全部 8 个 Tool 实现、MCP transport、tool 注册 | 3 天 |
| **Phase 3: 漂移检测与安全** | `verifyAnchor`、sha256 流式计算、`PathSandbox` | 2 天 |
| **Phase 4: 部署集成** | Tauri 集成、MCP 配置、E2E 测试、文档 | 2 天 |
| **总计** | | **~9 天** |

---

## §12 附录

### 12.1 Zotero itemTypeID 参考表

| itemTypeID | typeName |
|-----------|----------|
| 1 | note |
| 2 | book |
| 3 | bookSection |
| 4 | journalArticle |
| 5 | magazineArticle |
| 6 | newspaperArticle |
| 7 | thesis |
| 8 | letter |
| 9 | manuscript |
| 10 | interview |
| 11 | film |
| 12 | artwork |
| 13 | webpage |
| 14 | report |
| 15 | bill |
| 16 | case |
| 17 | hearing |
| 18 | patent |
| 19 | statute |
| 20 | email |
| 21 | map |
| 22 | blogPost |
| 23 | instantMessage |
| 24 | forumPost |
| 25 | audioRecording |
| 26 | presentation |
| 27 | videoRecording |
| 28 | tvBroadcast |
| 29 | radioBroadcast |
| 30 | podcast |
| 31 | computerProgram |
| 32 | conferencePaper |
| 33 | document |
| 34 | encyclopediaArticle |
| 35 | dictionaryEntry |
| 36 | preprint |
| 37 | attachment |
| 38 | annotation |

### 12.2 EvidenceRecord 字段映射表

| Zotero 字段 (fieldName) | EvidenceRecord 字段 | 说明 |
|------------------------|--------------------|------|
| `title` | `title` | 论文/书籍标题 |
| `DOI` | `doi` | 数字对象标识符 |
| `url` | `url` | 公开 URL |
| `date` | `year` | 出版/发表年份 |
| — | `authors` | 由 creator 表聚合（firstName + lastName） |
| `sourceId` | `sourceId` | 格式：`"zotero:{userId}/{itemKey}"` |

### 12.3 BibTeX 类型映射表

| Zotero itemType | BibTeX entry type |
|----------------|-------------------|
| `journalArticle` | `@article` |
| `conferencePaper` | `@inproceedings` |
| `book` | `@book` |
| `bookSection` | `@inbook` |
| `thesis` | `@phdthesis` / `@mastersthesis` |
| `report` | `@techreport` |
| `webpage` | `@misc` |
| `patent` | `@patent` |
| `preprint` | `@unpublished` |
| `manuscript` | `@unpublished` |

### 12.4 与现有代码库的对齐要点

| 本设计概念 | 对应现有代码 |
|-----------|------------|
| `sourceId = "zotero:{userId}/{itemKey}"` | `EvidenceRecord.sourceId`（`research-types.ts` L67） |
| `EvidenceRecord.locator` | `EvidenceRecord.locator`（`research-types.ts` L75-81）：fileName、page、section、quote、startOffset、endOffset |
| sha256 计算风格 | `graph_compiler.rs` 中的 `sha256_hex()`（64 hex）风格一致 |
| `sha2` crate | 与 `graph_compiler.rs` 使用相同的 `sha2 = "0.10"` |
| `evidence_anchors` → `locator` | PDF Agent L2 输出 → EvidenceRecord 持久化的桥接 |
| `PluginGraphPatch` | `contracts.ts` 中的 `PluginGraphPatch` 协议（`apiVersion: "researchcanvas.dev/graph-patch/v1alpha1"`） |
| `reviewRequired: true` | 所有 agent 产出必须经人工审阅 |
