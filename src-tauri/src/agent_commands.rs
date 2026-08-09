//! Agent Tauri 命令——安全边界：Agent 不持有 API Key、文件系统句柄、网络访问、Graph store 写权限。
//! 宿主管理一切，Agent 输出只能进入 reviewRequired GraphPatch。
//!
//! Agent Tauri commands — security boundary: Agents hold no API keys, file handles,
//! network access, or graph store write permissions. The host manages everything;
//! agent output can only enter a reviewRequired GraphPatch.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

use crate::agent_host::{AgentHost, AgentJob, JobState};
use crate::pdf_pipeline::PdfPipeline;

/// 宿主管理的全局 AgentHost 状态 / Host-managed global AgentHost state.
pub struct AgentHostState(pub Mutex<AgentHost>);

// ── 命令输入 / 输出类型 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPdfJobRequest {
    pub pdf_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfJobStatus {
    pub job_id: String,
    pub pdf_path: String,
    pub file_hash: String,
    pub state: String,
    pub progress: (usize, usize),
    pub created_at: u64,
    pub updated_at: u64,
    pub error: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPatchRequest {
    pub job_id: String,
    pub accept: bool,
}

impl From<&AgentJob> for PdfJobStatus {
    fn from(job: &AgentJob) -> Self {
        Self {
            job_id: job.job_id.clone(),
            pdf_path: job.pdf_path.clone(),
            file_hash: job.file_hash.clone(),
            state: job.state.label().to_string(),
            progress: job.progress(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            error: job.error.clone(),
            result: job.result.clone(),
        }
    }
}

// ── Tauri Commands ──

/// 启动 PDF 处理 Job。自动运行完整管线：
/// 文件校验 → 文本提取 → OCR fallback → DocumentMap → 语义提取 → GraphPatch 生成 → 进入审阅。
///
/// Start a PDF processing job. Automatically runs the full pipeline:
/// validate → extract text → OCR fallback → build DocumentMap → extract semantics →
/// generate GraphPatch → await review.
///
/// 并发模型:异步命令 + 每阶段短锁 + 重活进线程池。状态锁只在 create/advance
/// 时短暂持有,PDF 解析在 spawn_blocking 里跑 — UI 不冻结,cancel_job 能在
/// 阶段边界介入(下一阶段推进时撞终态而干净退出)。
#[tauri::command]
pub async fn start_pdf_job(
    state: State<'_, AgentHostState>,
    request: StartPdfJobRequest,
) -> Result<PdfJobStatus, String> {
    let pdf_path = Path::new(&request.pdf_path);
    let abs_path = pdf_path
        .canonicalize()
        .map_err(|e| format!("Cannot resolve path: {e}"))?;

    // 创建 job（含文件校验）——短锁,拿到 id 即释放。
    let job_id = {
        let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
        host.create_job(&abs_path)?.job_id.clone()
    };

    let outcome = run_pdf_stages(&state.0, &job_id, &abs_path).await;
    if let Err(error) = outcome {
        // 管线失败必须落 Failed 终态,否则 job 永久卡死在非终态;
        // 若 job 已被并发取消/裁决,保持既有终态。
        let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
        let _ = host.advance_job(&job_id, JobState::Failed, None, None, Some(&error));
        return Err(error);
    }

    let host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    let job = host.get_job(&job_id).ok_or_else(|| "Job vanished".to_string())?;
    Ok(PdfJobStatus::from(job))
}

/// 短锁推进一个阶段;并发取消/失败导致转换非法时,若已是终态则返回干净的并发终止错误。
fn advance_stage(
    hosts: &Mutex<AgentHost>,
    job_id: &str,
    next: JobState,
    output_hash: Option<&str>,
    data: Option<Value>,
) -> Result<(), String> {
    let mut host = hosts.lock().map_err(|e| format!("Lock error: {e}"))?;
    match host.advance_job(job_id, next, output_hash, data, None) {
        Ok(_) => Ok(()),
        Err(error) => {
            let terminated = host
                .get_job(job_id)
                .map(|job| job.state.is_terminal())
                .unwrap_or(false);
            if terminated {
                Err("Job was cancelled or failed concurrently".to_string())
            } else {
                Err(error)
            }
        }
    }
}

/// 管线主体:阶段推进用短锁,重计算进线程池,失败即返回由调用方落 Failed。
async fn run_pdf_stages(
    hosts: &Mutex<AgentHost>,
    job_id: &str,
    abs_path: &Path,
) -> Result<(), String> {
    // ── 阶段 1：ValidatingFile ──
    advance_stage(hosts, job_id, JobState::ValidatingFile, Some("v1"), None)?;

    // ── 阶段 2：ExtractingText ──
    let extracted = {
        let path = abs_path.to_path_buf();
        tauri::async_runtime::spawn_blocking(move || PdfPipeline::extract_text(&path))
            .await
            .map_err(|e| format!("Pipeline task join error: {e}"))??
    };
    let text_hash = format!("{:x}", sha2::Sha256::digest(extracted.full_text.as_bytes()));
    advance_stage(
        hosts,
        job_id,
        JobState::ExtractingText,
        Some(&text_hash),
        Some(serde_json::to_value(&extracted).map_err(|e| e.to_string())?),
    )?;

    // ── 阶段 3：OcrOptional ──
    // 复用阶段 2 的提取结果，不再重新解析 PDF。
    let ocr_triggered = PdfPipeline::needs_ocr(&extracted);
    let (final_extracted, ocr_confidence, ocr_error) = if ocr_triggered {
        let extracted_for_ocr = extracted.clone();
        match tauri::async_runtime::spawn_blocking(move || PdfPipeline::ocr_fallback(&extracted_for_ocr))
            .await
            .map_err(|e| format!("Pipeline task join error: {e}"))?
        {
            Ok(ocr_text) => (ocr_text, Some(1.0_f64), None),
            Err(error) => {
                // OCR 失败非致命：保留原文本并记录警告，后续阶段仍可推进。
                (extracted.clone(), None, Some(error))
            }
        }
    } else {
        (extracted.clone(), None, None)
    };
    let final_text_hash = format!("{:x}", sha2::Sha256::digest(final_extracted.full_text.as_bytes()));
    advance_stage(
        hosts,
        job_id,
        JobState::OcrOptional,
        Some(&final_text_hash),
        Some(serde_json::json!({
            "ocrTriggered": ocr_triggered,
            "ocrConfidence": ocr_confidence,
            "ocrError": ocr_error
        })),
    )?;

    // ── 阶段 4：BuildingDocumentMap ──
    // 直接由已提取文本构建文档结构，不再第三次解析 PDF。
    let doc = {
        let extracted_for_doc = final_extracted.clone();
        tauri::async_runtime::spawn_blocking(move || {
            PdfPipeline::build_structured_document(extracted_for_doc, ocr_triggered, ocr_confidence)
        })
        .await
        .map_err(|e| format!("Pipeline task join error: {e}"))?
    };
    let doc_hash = format!("{:x}", sha2::Sha256::digest(
        serde_json::to_string(&doc).unwrap_or_default().as_bytes()
    ));
    advance_stage(
        hosts,
        job_id,
        JobState::BuildingDocumentMap,
        Some(&doc_hash),
        Some(serde_json::to_value(&doc).map_err(|e| e.to_string())?),
    )?;

    // ── 阶段 5：ExtractingSemantics ──
    // 从 StructuredDocument 提取语义节点/边作为 GraphPatch 待审阅 operations
    let patch = build_graph_patch_from_document(&doc, job_id);
    let semantic_hash = format!("{:x}", sha2::Sha256::digest(
        serde_json::to_string(&patch).unwrap_or_default().as_bytes()
    ));
    advance_stage(
        hosts,
        job_id,
        JobState::ExtractingSemantics,
        Some(&semantic_hash),
        Some(patch.clone()),
    )?;

    // ── 阶段 6：GeneratingPatch ──
    advance_stage(
        hosts,
        job_id,
        JobState::GeneratingPatch,
        Some(&semantic_hash),
        Some(patch.clone()),
    )?;

    // ── 阶段 7:AwaitingReview(data 即审阅载荷,advance_job 写入 job.result)──
    advance_stage(
        hosts,
        job_id,
        JobState::AwaitingReview,
        Some(&semantic_hash),
        Some(patch),
    )?;
    Ok(())
}

/// 查询 Job 状态 / Query job status.
#[tauri::command]
pub fn get_job_status(
    state: State<'_, AgentHostState>,
    job_id: String,
) -> Result<PdfJobStatus, String> {
    let host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    let job = host.get_job(&job_id).ok_or_else(|| format!("Job not found: {job_id}"))?;
    Ok(PdfJobStatus::from(job))
}

/// 审阅裁决：接受或拒绝 Agent 输出的 GraphPatch。
/// Review decision: accept or reject the agent's proposed GraphPatch.
#[tauri::command]
pub fn review_patch(
    state: State<'_, AgentHostState>,
    request: ReviewPatchRequest,
) -> Result<PdfJobStatus, String> {
    let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    host.review_patch(&request.job_id, request.accept)?;
    let job = host.get_job(&request.job_id).ok_or_else(|| "Job vanished".to_string())?;
    Ok(PdfJobStatus::from(job))
}

/// 取消进行中的 Job / Cancel an in-progress job.
#[tauri::command]
pub fn cancel_job(
    state: State<'_, AgentHostState>,
    job_id: String,
    reason: Option<String>,
) -> Result<PdfJobStatus, String> {
    let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    let reason = reason.unwrap_or_else(|| "Cancelled by user".to_string());
    host.cancel_job(&job_id, &reason)?;
    let job = host.get_job(&job_id).ok_or_else(|| "Job vanished".to_string())?;
    Ok(PdfJobStatus::from(job))
}

// ── GraphPatch 构建（语义提取） ──

/// 从 StructuredDocument 构建 reviewRequired GraphPatch。
/// Agent 不能直接修改图存储——它的输出只能是这个待审阅的 GraphPatch。
///
/// Build a reviewRequired GraphPatch from a StructuredDocument.
/// The agent cannot directly mutate the graph store — its output can only be
/// this review-gated GraphPatch.
fn build_graph_patch_from_document(
    doc: &crate::pdf_pipeline::StructuredDocument,
    job_id: &str,
) -> Value {
    let mut operations = Vec::new();

    // 每个章节 → 一个 note 节点
    for section in &doc.sections {
        let node_id = format!("pdf-sec-{}", section.id);
        operations.push(serde_json::json!({
            "op": "add-node",
            "node": {
                "id": node_id,
                "type": "note",
                "title": section.title,
                "body": format!("Section level {} (offsets {}-{})", section.level, section.start_offset, section.end_offset),
                "tags": ["pdf-import", "section"],
                "data": {
                    "sectionId": section.id,
                    "level": section.level,
                    "childSectionIds": section.child_section_ids,
                    "sourceJobId": job_id
                }
            }
        }));

        // 父子章节 → depends_on 边
        for child_id in &section.child_section_ids {
            operations.push(serde_json::json!({
                "op": "add-edge",
                "edge": {
                    "id": format!("pdf-edge-{}-{}", section.id, child_id),
                    "source": format!("pdf-sec-{}", section.id),
                    "target": format!("pdf-sec-{}", child_id),
                    "type": "part_of",
                    "note": "section hierarchy"
                }
            }));
        }
    }

    // 每个段落 → 一个 evidence 节点
    for para in &doc.paragraphs {
        let snippet: String = para.text.chars().take(200).collect();
        operations.push(serde_json::json!({
            "op": "add-node",
            "node": {
                "id": format!("pdf-{}", para.id),
                "type": "evidence",
                "title": snippet,
                "body": para.text,
                "tags": ["pdf-import", "paragraph"],
                "data": {
                    "paragraphId": para.id,
                    "sectionId": para.section_id,
                    "startOffset": para.start_offset,
                    "endOffset": para.end_offset,
                    "sourceJobId": job_id
                }
            }
        }));

        // 段落 → 所属章节
        operations.push(serde_json::json!({
            "op": "add-edge",
            "edge": {
                "id": format!("pdf-edge-para-{}", para.id),
                "source": format!("pdf-{}", para.id),
                "target": format!("pdf-sec-{}", para.section_id),
                "type": "part_of",
                "note": "paragraph belongs to section"
            }
        }));
    }

    // 图表引用 → 独立的 evidence/concept 节点
    for ft in &doc.figures_tables {
        let kind_label = match ft.kind {
            crate::pdf_pipeline::FigureTableKind::Figure => "figure",
            crate::pdf_pipeline::FigureTableKind::Table => "table",
        };
        operations.push(serde_json::json!({
            "op": "add-node",
            "node": {
                "id": format!("pdf-{}", ft.id),
                "type": "concept",
                "title": ft.caption,
                "body": format!("{} reference at offset {}", kind_label, ft.caption_offset),
                "tags": ["pdf-import", kind_label],
                "data": {
                    "figureTableId": ft.id,
                    "kind": kind_label,
                    "captionOffset": ft.caption_offset,
                    "sourceJobId": job_id
                }
            }
        }));
    }

    serde_json::json!({
        "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
        "source": {
            "pluginId": "pdf-canvas-agent",
            "operation": "pdf-document-extraction",
            "externalId": job_id
        },
        "title": format!("PDF structure extraction ({} sections, {} paragraphs)",
            doc.sections.len(), doc.paragraphs.len()),
        "summary": format!(
            "Extracted document structure from PDF. {} sections, {} paragraphs, {} figures/tables{}",
            doc.sections.len(),
            doc.paragraphs.len(),
            doc.figures_tables.len(),
            if doc.ocr_triggered { " (OCR assisted)" } else { "" }
        ),
        "reviewRequired": true,
        "operations": operations
    })
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_graph_patch_from_document_produces_review_required_operations() {
        // 构造最小 StructuredDocument
        let pages = vec![crate::pdf_pipeline::PageText {
            page_number: 1,
            text: "test".into(),
            char_count: 4,
        }];
        let sections = vec![
            crate::pdf_pipeline::StructureSection {
                id: "s1".into(),
                title: "Introduction".into(),
                level: 1,
                start_offset: 0,
                end_offset: 100,
                child_section_ids: vec!["s1.1".into()],
            },
            crate::pdf_pipeline::StructureSection {
                id: "s1.1".into(),
                title: "Background".into(),
                level: 2,
                start_offset: 50,
                end_offset: 100,
                child_section_ids: vec![],
            },
        ];
        let paragraphs = vec![crate::pdf_pipeline::StructureParagraph {
            id: "p1".into(),
            section_id: "s1".into(),
            text: "This is a test paragraph with important claims.".into(),
            start_offset: 0,
            end_offset: 45,
        }];
        let figures_tables = vec![crate::pdf_pipeline::FigureTableRef {
            id: "fig1".into(),
            kind: crate::pdf_pipeline::FigureTableKind::Figure,
            caption: "Overview of the proposed method.".into(),
            caption_offset: 60,
        }];
        let document_map = crate::pdf_pipeline::DocumentMap::build(&sections, &paragraphs, &pages);
        let doc = crate::pdf_pipeline::StructuredDocument {
            sections,
            paragraphs,
            figures_tables,
            document_map,
            ocr_triggered: false,
            ocr_confidence: None,
        };

        let patch = build_graph_patch_from_document(&doc, "test-job");

        assert_eq!(patch["apiVersion"], "researchcanvas.dev/graph-patch/v1alpha1");
        assert_eq!(patch["reviewRequired"], true);
        assert_eq!(patch["source"]["pluginId"], "pdf-canvas-agent");

        let ops = patch["operations"].as_array().expect("operations array");
        // 2 sections + 3 edges (hierarchy + paragraph→section + figure node) + 1 paragraph + 1 figure = 7+
        assert!(ops.len() >= 6, "expected at least 6 operations, got {}", ops.len());

        // 有 section 节点
        assert!(ops.iter().any(|op| op["node"]["id"] == "pdf-sec-s1"));
        // 有 paragraph 节点
        assert!(ops.iter().any(|op| op["node"]["id"] == "pdf-p1"));
        // 有 figure 节点
        assert!(ops.iter().any(|op| op["node"]["id"] == "pdf-fig1"));
        // 有结构边
        assert!(ops.iter().any(|op| {
            op["edge"]["id"] == "pdf-edge-s1-s1.1" && op["edge"]["type"] == "part_of"
        }));
    }
}
