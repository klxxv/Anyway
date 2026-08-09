//! PDF 管线——文本提取（lopdf）、结构识别、OCR fallback、DocumentMap 构建。
//! Decoupled from agent_host; only transforms PDF → StructuredDocument.

use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;

const MIN_TEXT_CHARS: usize = 500;

// ── 核心数据结构 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedText {
    pub full_text: String,
    pub pages: Vec<PageText>,
    pub total_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageText {
    pub page_number: usize,
    pub text: String,
    pub char_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureSection {
    pub id: String,
    pub title: String,
    pub level: u8,
    pub start_offset: usize,
    pub end_offset: usize,
    pub child_section_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureParagraph {
    pub id: String,
    pub section_id: String,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FigureTableRef {
    pub id: String,
    pub kind: FigureTableKind,
    pub caption: String,
    pub caption_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FigureTableKind {
    Figure,
    Table,
}

/// 结构化文档——PDF 管线最终输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredDocument {
    pub sections: Vec<StructureSection>,
    pub paragraphs: Vec<StructureParagraph>,
    pub figures_tables: Vec<FigureTableRef>,
    pub document_map: DocumentMap,
    pub ocr_triggered: bool,
    pub ocr_confidence: Option<f64>,
}

// ── DocumentMap ──

/// 字符偏移 → (section_id, paragraph_id, page) 的二分查找映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMap {
    paragraph_spans: Vec<ParagraphSpan>,
    page_offsets: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParagraphSpan {
    paragraph_id: String,
    section_id: String,
    start_offset: usize,
    end_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedAnchor {
    pub section_id: String,
    pub paragraph_id: String,
    pub page: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl DocumentMap {
    pub fn build(_sections: &[StructureSection], paragraphs: &[StructureParagraph], pages: &[PageText]) -> Self {
        let paragraph_spans: Vec<ParagraphSpan> = paragraphs
            .iter()
            .map(|p| ParagraphSpan {
                paragraph_id: p.id.clone(),
                section_id: p.section_id.clone(),
                start_offset: p.start_offset,
                end_offset: p.end_offset,
            })
            .collect();

        let mut cum = 0usize;
        let page_offsets: Vec<(usize, usize)> = pages
            .iter()
            .map(|p| {
                let start = cum;
                cum += p.text.len() + 1;
                (p.page_number, start)
            })
            .collect();

        Self { paragraph_spans, page_offsets }
    }

    /// O(log N) 二分查找：给定全局字符偏移，解析为完整锚点。
    pub fn resolve(&self, offset: usize) -> Option<ResolvedAnchor> {
        let para = binary_search_span(&self.paragraph_spans, offset)?;
        let page = self.page_offsets.iter().rev()
            .find(|(_, start)| offset >= *start)
            .map(|(p, _)| *p)
            .unwrap_or(1);

        Some(ResolvedAnchor {
            section_id: para.section_id.clone(),
            paragraph_id: para.paragraph_id.clone(),
            page,
            start_offset: para.start_offset,
            end_offset: para.end_offset,
        })
    }

    /// 给定范围 [start, end)，解析为锚点列表。
    pub fn resolve_range(&self, start: usize, end: usize) -> Vec<ResolvedAnchor> {
        let mut anchors = Vec::new();
        let mut pos = start;
        while pos < end {
            if let Some(anchor) = self.resolve(pos) {
                let next = anchor.end_offset.min(end);
                anchors.push(anchor);
                pos = next;
            } else {
                pos += 1;
            }
        }
        anchors
    }
}

fn binary_search_span<'a>(spans: &'a [ParagraphSpan], offset: usize) -> Option<&'a ParagraphSpan> {
    let idx = spans.partition_point(|s| s.start_offset <= offset);
    if idx == 0 { return None; }
    let candidate = &spans[idx - 1];
    if offset >= candidate.start_offset && offset < candidate.end_offset { Some(candidate) } else { None }
}

// ── PDF 管线 ──

/// PDF 处理管线——纯函数式：输入路径，输出结构。
pub struct PdfPipeline;

impl PdfPipeline {
    /// 使用 lopdf 从 PDF 提取所有文本。
    pub fn extract_text(pdf_path: &Path) -> Result<ExtractedText, String> {
        let document = Document::load(pdf_path)
            .map_err(|e| format!("lopdf load error: {e}"))?;

        let mut pages = Vec::with_capacity(document.get_pages().len());
        let mut full_parts = Vec::new();

        for (page_num, _) in &document.get_pages() {
            let text = document.extract_text(&[*page_num]).unwrap_or_default();
            let char_count = text.chars().count();
            pages.push(PageText { page_number: *page_num as usize, text: text.clone(), char_count });
            full_parts.push(text);
        }

        let full_text = full_parts.join("\n");
        let total_chars = full_text.chars().count();

        Ok(ExtractedText { full_text, pages, total_chars })
    }

    /// 判断是否需要 OCR fallback。
    pub fn needs_ocr(extracted: &ExtractedText) -> bool {
        extracted.total_chars < MIN_TEXT_CHARS
    }

/// OCR fallback 桩：接收已提取文本，不再重复解析 PDF。
    pub fn ocr_fallback(extracted: &ExtractedText) -> Result<ExtractedText, String> {
        if !Self::needs_ocr(extracted) {
            return Ok(extracted.clone());
        }
        Err(format!(
            "OCR not available: only {} chars extracted",
            extracted.total_chars
        ))
    }

    /// 从提取文本中识别章节、段落和图表引用。
    pub(crate) fn recognize_structure(text: &str) -> StructureResult {
        let sections = detect_sections(text);
        let paragraphs = detect_paragraphs(text, &sections);
        let figures_tables = detect_figures_tables(text);
        StructureResult { sections, paragraphs, figures_tables }
    }

    /// 由已提取文本直接构建结构化文档，避免重复解析 PDF。
    pub fn build_structured_document(
        extracted: ExtractedText,
        ocr_triggered: bool,
        ocr_confidence: Option<f64>,
    ) -> StructuredDocument {
        let structure = Self::recognize_structure(&extracted.full_text);
        let document_map = DocumentMap::build(&structure.sections, &structure.paragraphs, &extracted.pages);

        StructuredDocument {
            sections: structure.sections,
            paragraphs: structure.paragraphs,
            figures_tables: structure.figures_tables,
            document_map,
            ocr_triggered,
            ocr_confidence,
        }
    }

    /// 运行完整 L1 管线（仅对未提取过文本的场景使用；
    /// 否则用 `extract_text` + `build_structured_document` 避免重复解析）。
    pub fn run(pdf_path: &Path) -> Result<StructuredDocument, String> {
        let extracted = Self::extract_text(pdf_path)?;
        let ocr_triggered = Self::needs_ocr(&extracted);
        let (final_extracted, ocr_confidence) = if ocr_triggered {
            match Self::ocr_fallback(&extracted) {
                Ok(ocr_text) => (ocr_text, Some(1.0)),
                Err(_) => (extracted, None),
            }
        } else {
            (extracted, None)
        };
        Ok(Self::build_structured_document(final_extracted, ocr_triggered, ocr_confidence))
    }
}

// ── 结构识别内部逻辑 ──

pub(crate) struct StructureResult {
    sections: Vec<StructureSection>,
    paragraphs: Vec<StructureParagraph>,
    figures_tables: Vec<FigureTableRef>,
}

fn detect_sections(text: &str) -> Vec<StructureSection> {
    let mut sections = Vec::new();
    let re = regex::Regex::new(r"(?m)^\s*(\d{1,3}(?:\.\d{1,3})*)\s+(.{3,120})\s*$").unwrap();

    for caps in re.captures_iter(text) {
        let num = caps.get(1).unwrap().as_str();
        let title = caps.get(2).unwrap().as_str().trim().to_string();
        let offset = caps.get(0).unwrap().start();
        let level = (num.matches('.').count() as u8 + 1).min(5);

        sections.push(StructureSection {
            id: format!("s{num}"),
            title,
            level,
            start_offset: offset,
            end_offset: offset,
            child_section_ids: Vec::new(),
        });
    }

    for i in 0..sections.len() {
        sections[i].end_offset = if i + 1 < sections.len() { sections[i + 1].start_offset } else { text.len() };
    }

    let mut parent_stack: Vec<usize> = Vec::new();
    for i in 0..sections.len() {
        let level = sections[i].level;
        while parent_stack.last().map_or(false, |&p| sections[p].level >= level) {
            parent_stack.pop();
        }
        if let Some(&parent_idx) = parent_stack.last() {
            let child_id = sections[i].id.clone();
            sections[parent_idx].child_section_ids.push(child_id);
        }
        parent_stack.push(i);
    }

    sections
}

fn detect_paragraphs(text: &str, sections: &[StructureSection]) -> Vec<StructureParagraph> {
    let mut paragraphs = Vec::new();
    let mut para_id = 0usize;

    for block in text.split("\n\n") {
        let trimmed = block.trim();
        if trimmed.is_empty() || trimmed.len() < 10 { continue; }

        let block_start = block.as_ptr() as usize - text.as_ptr() as usize;
        let section_id = sections.iter().rev()
            .find(|s| block_start >= s.start_offset && block_start < s.end_offset)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "s0".to_string());

        para_id += 1;
        paragraphs.push(StructureParagraph {
            id: format!("p{para_id}"),
            section_id,
            text: trimmed.to_string(),
            start_offset: block_start,
            end_offset: block_start + block.len(),
        });
    }

    paragraphs
}

fn detect_figures_tables(text: &str) -> Vec<FigureTableRef> {
    let mut refs = Vec::new();
    let re = regex::Regex::new(r"(?mi)^\s*(Figure|Table)\s+(\d+(?:\.\d+)?)\s*[:.]\s*(.{10,300})$").unwrap();

    for caps in re.captures_iter(text) {
        let kind = match caps.get(1).unwrap().as_str().to_lowercase().as_str() {
            "figure" => FigureTableKind::Figure,
            "table" => FigureTableKind::Table,
            _ => continue,
        };
        let num = caps.get(2).unwrap().as_str();
        let caption = caps.get(3).unwrap().as_str().trim().to_string();
        let offset = caps.get(0).unwrap().start();
        let id = match kind {
            FigureTableKind::Figure => format!("fig{num}"),
            FigureTableKind::Table => format!("tab{num}"),
        };

        refs.push(FigureTableRef { id, kind, caption, caption_offset: offset });
    }

    refs
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
1 Introduction\n\nThis paper presents a novel approach to neural network training.\n\n\
2 Method\n\n2.1 Gradient Computation\n\nThe gradient is computed using automatic differentiation.\n\n\
2.2 Update Rule\n\nOur update rule modifies the standard SGD formulation.\n\n\
Figure 1: Overview of the proposed architecture.\n\n\
Table 1: Comparison of convergence rates across optimizers.\n\n\
3 Results\n\nWe evaluate on CIFAR-10, ImageNet, and WMT-14 benchmarks.\n";

    #[test]
    fn detects_sections_with_hierarchy() {
        let sections = detect_sections(SAMPLE);
        assert!(sections.len() >= 3);
        let intro = sections.iter().find(|s| s.id == "s1").expect("s1");
        assert_eq!(intro.level, 1);
        let method = sections.iter().find(|s| s.id == "s2").expect("s2");
        assert!(method.child_section_ids.contains(&"s2.1".to_string()));
    }

    #[test]
    fn detects_figures_and_tables() {
        let refs = detect_figures_tables(SAMPLE);
        assert!(refs.iter().any(|r| r.id == "fig1" && r.kind == FigureTableKind::Figure));
        assert!(refs.iter().any(|r| r.id == "tab1" && r.kind == FigureTableKind::Table));
    }

    #[test]
    fn detects_paragraphs_under_sections() {
        let sections = detect_sections(SAMPLE);
        let paragraphs = detect_paragraphs(SAMPLE, &sections);
        assert!(!paragraphs.is_empty());
        for p in &paragraphs { assert!(!p.section_id.is_empty()); }
    }

    #[test]
    fn document_map_resolves_offsets() {
        let sections = detect_sections(SAMPLE);
        let paragraphs = detect_paragraphs(SAMPLE, &sections);
        let pages = vec![PageText { page_number: 1, text: SAMPLE.to_string(), char_count: SAMPLE.chars().count() }];
        let map = DocumentMap::build(&sections, &paragraphs, &pages);

        let target = SAMPLE.find("novel approach").expect("find");
        let anchor = map.resolve(target).expect("resolve");
        assert!(!anchor.section_id.is_empty());
        assert!(!anchor.paragraph_id.is_empty());
        assert_eq!(anchor.page, 1);

        let end = SAMPLE.find("Adam").unwrap_or(target + 4) + 4;
        let anchors = map.resolve_range(target, end);
        assert!(!anchors.is_empty());
    }

    #[test]
    fn needs_ocr_detection() {
        let sparse = ExtractedText {
            full_text: "short".into(),
            pages: vec![PageText { page_number: 1, text: "short".into(), char_count: 5 }],
            total_chars: 5,
        };
        assert!(PdfPipeline::needs_ocr(&sparse));

        let sufficient = ExtractedText {
            full_text: "a".repeat(1000),
            pages: vec![PageText { page_number: 1, text: "a".repeat(1000), char_count: 1000 }],
            total_chars: 1000,
        };
        assert!(!PdfPipeline::needs_ocr(&sufficient));
    }

    #[test]
    fn ocr_fallback_reuses_extracted_text_without_re_parsing() {
        let sufficient = ExtractedText {
            full_text: "a".repeat(1000),
            pages: vec![PageText { page_number: 1, text: "a".repeat(1000), char_count: 1000 }],
            total_chars: 1000,
        };
        let result = PdfPipeline::ocr_fallback(&sufficient).expect("sufficient text needs no OCR");
        assert_eq!(result.total_chars, sufficient.total_chars);

        let sparse = ExtractedText {
            full_text: "short".into(),
            pages: vec![PageText { page_number: 1, text: "short".into(), char_count: 5 }],
            total_chars: 5,
        };
        assert!(PdfPipeline::ocr_fallback(&sparse).is_err());
    }

    #[test]
    fn build_structured_document_from_extracted_text_preserves_ocr_state() {
        let extracted = ExtractedText {
            full_text: "a".repeat(1000),
            pages: vec![PageText { page_number: 1, text: "a".repeat(1000), char_count: 1000 }],
            total_chars: 1000,
        };
        let doc = PdfPipeline::build_structured_document(extracted, true, Some(0.95));
        assert!(doc.ocr_triggered);
        assert_eq!(doc.ocr_confidence, Some(0.95));
    }

    #[test]
    fn binary_search_correctness() {
        let spans = vec![
            ParagraphSpan { paragraph_id: "p1".into(), section_id: "s1".into(), start_offset: 0, end_offset: 100 },
            ParagraphSpan { paragraph_id: "p2".into(), section_id: "s2".into(), start_offset: 100, end_offset: 200 },
        ];
        assert_eq!(binary_search_span(&spans, 50).unwrap().paragraph_id, "p1");
        assert_eq!(binary_search_span(&spans, 150).unwrap().paragraph_id, "p2");
        assert!(binary_search_span(&spans, 300).is_none());
    }
}
