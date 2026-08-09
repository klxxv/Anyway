//! Agent 宿主——文件安全校验、Job 状态机、checkpoint 持久化与幂等恢复。
//! Decoupled from pdf_pipeline; only manages job lifecycle.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_PDF_BYTES: u64 = 50 * 1024 * 1024;
const JOB_ID_PREFIX_LEN: usize = 16;
const MAX_CHECKPOINT_BYTES: usize = 50 * 1024 * 1024;

// ── Job 状态机 ──

/// CREATED → VALIDATING_FILE → EXTRACTING_TEXT → OCR_OPTIONAL →
/// BUILDING_DOCUMENT_MAP → EXTRACTING_SEMANTICS → GENERATING_PATCH →
/// AWAITING_REVIEW → ACCEPTED / REJECTED。任一阶段可转入 FAILED。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobState {
    Created = 0,
    ValidatingFile = 1,
    ExtractingText = 2,
    OcrOptional = 3,
    BuildingDocumentMap = 4,
    ExtractingSemantics = 5,
    GeneratingPatch = 6,
    AwaitingReview = 7,
    Accepted = 8,
    Rejected = 9,
    Failed = 10,
}

impl JobState {
    pub fn can_transition_to(&self, next: JobState) -> bool {
        use JobState::*;
        matches!(
            (self, next),
            (Created, ValidatingFile)
                | (ValidatingFile, ExtractingText)
                | (ValidatingFile, Failed)
                | (ExtractingText, OcrOptional)
                | (ExtractingText, Failed)
                | (OcrOptional, BuildingDocumentMap)
                | (OcrOptional, ExtractingText)
                | (OcrOptional, Failed)
                | (BuildingDocumentMap, ExtractingSemantics)
                | (BuildingDocumentMap, Failed)
                | (ExtractingSemantics, GeneratingPatch)
                | (ExtractingSemantics, Failed)
                | (GeneratingPatch, AwaitingReview)
                | (GeneratingPatch, Failed)
                | (AwaitingReview, Accepted)
                | (AwaitingReview, Rejected)
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, JobState::Accepted | JobState::Rejected | JobState::Failed)
    }

    pub fn label(&self) -> &'static str {
        match self {
            JobState::Created => "created",
            JobState::ValidatingFile => "validating_file",
            JobState::ExtractingText => "extracting_text",
            JobState::OcrOptional => "ocr_optional",
            JobState::BuildingDocumentMap => "building_document_map",
            JobState::ExtractingSemantics => "extracting_semantics",
            JobState::GeneratingPatch => "generating_patch",
            JobState::AwaitingReview => "awaiting_review",
            JobState::Accepted => "accepted",
            JobState::Rejected => "rejected",
            JobState::Failed => "failed",
        }
    }
}

// ── Checkpoint ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageCheckpoint {
    pub stage: JobState,
    pub input_hash: String,
    pub output_hash: Option<String>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
    pub data: Option<serde_json::Value>,
}

// ── AgentJob ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentJob {
    pub job_id: String,
    pub pdf_path: String,
    pub file_hash: String,
    pub state: JobState,
    pub checkpoints: Vec<StageCheckpoint>,
    pub created_at: u64,
    pub updated_at: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl AgentJob {
    pub fn progress(&self) -> (usize, usize) {
        let completed = self
            .checkpoints
            .iter()
            .filter(|cp| cp.completed_at.is_some())
            .count();
        (completed, self.checkpoints.len())
    }

    pub fn last_completed_stage(&self) -> Option<JobState> {
        self.checkpoints
            .iter()
            .rev()
            .find(|cp| cp.completed_at.is_some())
            .map(|cp| cp.stage)
    }
}

// ── AgentHost ──

pub struct AgentHost {
    jobs: HashMap<String, AgentJob>,
    workspace_path: PathBuf,
}

impl AgentHost {
    pub fn new(workspace_path: PathBuf) -> Self {
        Self { jobs: HashMap::new(), workspace_path }
    }

    // ── 文件安全校验 ──

    pub fn compute_file_hash(path: &Path) -> Result<String, String> {
        let bytes = fs::read(path)
            .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        if bytes.len() as u64 > MAX_PDF_BYTES {
            return Err(format!(
                "File exceeds {}MB limit ({} bytes)",
                MAX_PDF_BYTES / (1024 * 1024),
                bytes.len()
            ));
        }
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    /// PDF 文件安全校验：扩展名 + PDF 魔数 + SHA-256。
    pub fn validate_pdf_file(path: &Path) -> Result<String, String> {
        if !path.is_file() {
            return Err(format!("Not a regular file: {}", path.display()));
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("pdf") {
            return Err("File must have a .pdf extension".to_string());
        }
        let header = fs::read(path).map_err(|e| format!("Cannot read file: {e}"))?;
        if header.len() < 5 || &header[..5] != b"%PDF-" {
            return Err("Invalid PDF: missing %PDF- header".to_string());
        }
        Self::compute_file_hash(path)
    }

    // ── Job 生命周期 ──

    /// 创建 Agent Job。幂等：相同路径 + 相同哈希的进行中 job 直接复用;
    /// 终态 job(Accepted/Rejected/Failed)不阻挡重新导入,保证可重试。
    pub fn create_job(&mut self, pdf_path: &Path) -> Result<&AgentJob, String> {
        let path_str = pdf_path.to_string_lossy().into_owned();
        let file_hash = Self::validate_pdf_file(pdf_path)?;
        let now = unix_millis();

        // 幂等查找:只复用非终态 job。终态 job 必须允许重建,否则一次失败后
        // 同一 PDF 永远无法重试(恒真谓词 is_terminal || !is_terminal 的修复)。
        let existing_id = self.jobs.values().find(|j| {
            j.pdf_path == path_str && j.file_hash == file_hash && !j.state.is_terminal()
        }).map(|j| j.job_id.clone());
        if let Some(ref id) = existing_id {
            return Ok(&self.jobs[id]);
        }

        let job_id = job_id_from(&path_str, now);
        let job = AgentJob {
            job_id: job_id.clone(),
            pdf_path: path_str,
            file_hash,
            state: JobState::Created,
            checkpoints: Vec::new(),
            created_at: now,
            updated_at: now,
            result: None,
            error: None,
        };

        self.jobs.insert(job_id.clone(), job);
        Ok(&self.jobs[&job_id])
    }

    pub fn get_job(&self, job_id: &str) -> Option<&AgentJob> {
        self.jobs.get(job_id)
    }

    pub fn list_jobs(&self) -> Vec<&AgentJob> {
        self.jobs.values().collect()
    }

    /// 推进 job 到下一阶段，记录 checkpoint（含输入/输出哈希）。
    pub fn advance_job(
        &mut self,
        job_id: &str,
        next_state: JobState,
        output_hash: Option<&str>,
        data: Option<serde_json::Value>,
        error: Option<&str>,
    ) -> Result<&AgentJob, String> {
        let job = self.jobs.get_mut(job_id)
            .ok_or_else(|| format!("Job not found: {job_id}"))?;

        if !job.state.can_transition_to(next_state) {
            return Err(format!(
                "Invalid transition {} -> {}",
                job.state.label(), next_state.label()
            ));
        }

        let now = unix_millis();

        // 合拢上一个 checkpoint
        if let Some(cp) = job.checkpoints.last_mut() {
            if cp.completed_at.is_none() {
                cp.completed_at = Some(now);
                cp.output_hash = output_hash.map(String::from);
                cp.error = error.map(String::from);
            }
        }

        // 新 checkpoint：输入哈希取自上一阶段产出
        let input_hash = job.checkpoints.last()
            .and_then(|cp| cp.output_hash.clone())
            .unwrap_or_else(|| job.file_hash.clone());

        let checkpoint = StageCheckpoint {
            stage: next_state,
            input_hash,
            output_hash: if next_state.is_terminal() { output_hash.map(String::from) } else { None },
            started_at: now,
            completed_at: if next_state.is_terminal() { Some(now) } else { None },
            error: error.map(String::from),
            data: data.clone(),
        };

        job.checkpoints.push(checkpoint);
        job.state = next_state;
        job.updated_at = now;
        if next_state == JobState::AwaitingReview {
            // 审阅载荷:进入待审阅时,result 固定为待审 GraphPatch,
            // 审阅面板凭 result.operations 渲染 Apply/Reject。
            job.result = data;
        }
        if next_state.is_terminal() {
            job.error = error.map(String::from);
        }

        Ok(job)
    }

    /// 幂等恢复：从磁盘 checkpoint 恢复 job 到上一个已完成阶段。
    pub fn recover_job(&mut self, job_id: &str) -> Result<JobState, String> {
        let path = self.checkpoint_path(job_id);
        let job = self.jobs.get_mut(job_id)
            .ok_or_else(|| format!("Job not found: {job_id}"))?;

        if !path.is_file() {
            return Ok(job.state);
        }

        let bytes = fs::read(&path)
            .map_err(|e| format!("Checkpoint read error: {e}"))?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(format!(
                "Checkpoint exceeds size limit: {} > {}",
                bytes.len(),
                MAX_CHECKPOINT_BYTES
            ));
        }
        let persisted: Vec<StageCheckpoint> = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Checkpoint deserialize error: {e}"))?;

        let last_stage = job.last_completed_stage();
        let new_checkpoints: Vec<_> = persisted
            .into_iter()
            .filter(|cp| last_stage.map_or(true, |ls| cp.stage as u8 > ls as u8))
            .collect();

        if !new_checkpoints.is_empty() {
            let recovered_state = new_checkpoints.last().map(|cp| cp.stage).unwrap_or(JobState::Created);
            job.checkpoints.extend(new_checkpoints);
            job.state = recovered_state;
            job.updated_at = unix_millis();
        }

        Ok(job.state)
    }

    /// 持久化所有 checkpoint 到磁盘。
    pub fn persist_checkpoints(&self) -> Result<(), String> {
        let dir = self.checkpoint_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("Cannot create checkpoint dir: {e}"))?;
        for job in self.jobs.values() {
            let json = serde_json::to_vec_pretty(&job.checkpoints)
                .map_err(|e| format!("Serialize error: {e}"))?;
            if json.len() > MAX_CHECKPOINT_BYTES {
                return Err(format!(
                    "Checkpoint for job {} exceeds size limit: {} > {}",
                    job.job_id,
                    json.len(),
                    MAX_CHECKPOINT_BYTES
                ));
            }
            fs::write(self.checkpoint_path(&job.job_id), json)
                .map_err(|e| format!("Write checkpoint error: {e}"))?;
        }
        Ok(())
    }

    /// 取消 job：只能在非终态调用，直接转入 Failed。
    pub fn cancel_job(&mut self, job_id: &str, reason: &str) -> Result<&AgentJob, String> {
        let job = self.jobs.get_mut(job_id)
            .ok_or_else(|| format!("Job not found: {job_id}"))?;
        if job.state.is_terminal() {
            return Err(format!("Job already terminal: {}", job.state.label()));
        }
        let now = unix_millis();
        // 合拢上一个 checkpoint
        if let Some(cp) = job.checkpoints.last_mut() {
            if cp.completed_at.is_none() {
                cp.completed_at = Some(now);
                cp.error = Some(reason.to_string());
            }
        }
        let checkpoint = StageCheckpoint {
            stage: JobState::Failed,
            input_hash: job.file_hash.clone(),
            output_hash: None,
            started_at: now,
            completed_at: Some(now),
            error: Some(reason.to_string()),
            data: None,
        };
        job.checkpoints.push(checkpoint);
        job.state = JobState::Failed;
        job.updated_at = now;
        job.error = Some(reason.to_string());
        Ok(job)
    }

    /// 审阅裁决：接受或拒绝待审阅的 GraphPatch。
    pub fn review_patch(&mut self, job_id: &str, accept: bool) -> Result<&AgentJob, String> {
        let job = self.jobs.get_mut(job_id)
            .ok_or_else(|| format!("Job not found: {job_id}"))?;
        if job.state != JobState::AwaitingReview {
            return Err(format!(
                "Job must be AwaitingReview, but is {}",
                job.state.label()
            ));
        }
        let target = if accept { JobState::Accepted } else { JobState::Rejected };
        let now = unix_millis();
        if let Some(cp) = job.checkpoints.last_mut() {
            if cp.completed_at.is_none() {
                cp.completed_at = Some(now);
            }
        }
        let checkpoint = StageCheckpoint {
            stage: target,
            input_hash: job.file_hash.clone(),
            output_hash: None,
            started_at: now,
            completed_at: Some(now),
            error: None,
            data: None,
        };
        job.checkpoints.push(checkpoint);
        job.state = target;
        job.updated_at = now;
        Ok(job)
    }

    /// 清理终态 job（10 分钟后）。
    pub fn cleanup_completed(&mut self) -> Result<usize, String> {
        let now = unix_millis();
        let stale: Vec<String> = self.jobs.iter()
            .filter(|(_, j)| j.state.is_terminal() && now.saturating_sub(j.updated_at) > 600_000)
            .map(|(id, _)| id.clone())
            .collect();

        let count = stale.len();
        for id in &stale {
            self.jobs.remove(id);
            let path = self.checkpoint_path(id);
            if path.is_file() { let _ = fs::remove_file(&path); }
        }
        Ok(count)
    }

    fn checkpoint_dir(&self) -> PathBuf {
        self.workspace_path.join("agent-checkpoints")
    }

    fn checkpoint_path(&self, job_id: &str) -> PathBuf {
        self.checkpoint_dir().join(format!("{job_id}.json"))
    }
}

fn unix_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

static JOB_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn job_id_from(pdf_path: &str, now: u64) -> String {
    // 毫秒时间戳在快速重建时会碰撞(同一 ms 内重试得到同 id 并互相覆盖),
    // 叠加单调序号保证进程内唯一。
    let seq = JOB_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(pdf_path.as_bytes());
    hasher.update(b":");
    hasher.update(now.to_le_bytes());
    hasher.update(seq.to_le_bytes());
    format!("{:x}", hasher.finalize())[..JOB_ID_PREFIX_LEN].to_string()
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn dummy_pdf(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("test.pdf");
        fs::write(&path, b"%PDF-1.4\n%%EOF").expect("write dummy pdf");
        path
    }

    #[test]
    fn validates_pdf_magic_and_rejects_non_pdfs() {
        let dir = tempdir().expect("tempdir");
        let pdf = dummy_pdf(dir.path());
        assert!(AgentHost::validate_pdf_file(&pdf).is_ok());
        let txt = dir.path().join("notes.txt");
        fs::write(&txt, b"not a pdf").expect("write txt");
        assert!(AgentHost::validate_pdf_file(&txt).is_err());
    }

    #[test]
    fn full_state_machine_flow() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let job_id = {
            let job = host.create_job(&pdf).expect("create job");
            assert_eq!(job.state, JobState::Created);
            job.job_id.clone()
        };

        let stages = [
            JobState::ValidatingFile, JobState::ExtractingText, JobState::OcrOptional,
            JobState::BuildingDocumentMap, JobState::ExtractingSemantics,
            JobState::GeneratingPatch, JobState::AwaitingReview, JobState::Accepted,
        ];
        for &stage in &stages {
            let hash = format!("{:x}", Sha256::digest(stage.label().as_bytes()));
            host.advance_job(&job_id, stage, Some(&hash), None, None).expect("advance");
        }

        let job = host.get_job(&job_id).expect("get job");
        assert_eq!(job.state, JobState::Accepted);
        assert!(job.state.is_terminal());
        assert_eq!(job.checkpoints.len(), stages.len());
        assert_eq!(job.progress().0, stages.len());
    }

    #[test]
    fn rejects_invalid_transition() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let id = { let j = host.create_job(&pdf).expect("create"); j.job_id.clone() };
        assert!(host.advance_job(&id, JobState::Accepted, None, None, None).is_err());
    }

    #[test]
    fn idempotent_job_creation() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let j1 = { host.create_job(&pdf).expect("first").job_id.clone() };
        let j2 = { host.create_job(&pdf).expect("second").job_id.clone() };
        assert_eq!(j1, j2);
        assert_eq!(host.list_jobs().len(), 1);
    }

    #[test]
    fn terminal_job_does_not_block_retry() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let first = { host.create_job(&pdf).expect("first").job_id.clone() };
        host.cancel_job(&first, "transient failure").expect("cancel");
        // 终态 job 不再被幂等复用:同一 PDF 可以重建新 job 重试。
        let second = { host.create_job(&pdf).expect("retry after terminal").job_id.clone() };
        assert_ne!(first, second);
        assert_eq!(host.list_jobs().len(), 2);
        // 新 job 是进行中的非终态,幂等查找仍然复用它。
        let third = { host.create_job(&pdf).expect("reuse in-flight").job_id.clone() };
        assert_eq!(second, third);
        assert_eq!(host.list_jobs().len(), 2);
    }

    #[test]
    fn awaiting_review_fills_result_with_review_payload() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let id = { host.create_job(&pdf).expect("create").job_id.clone() };
        let stages = [
            JobState::ValidatingFile, JobState::ExtractingText, JobState::OcrOptional,
            JobState::BuildingDocumentMap, JobState::ExtractingSemantics,
            JobState::GeneratingPatch,
        ];
        for &stage in &stages {
            host.advance_job(&id, stage, Some("h"), None, None).expect("advance");
        }
        assert!(host.get_job(&id).unwrap().result.is_none());
        let patch = serde_json::json!({
            "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
            "operations": [{"op": "add-node", "node": {"id": "n1", "type": "note", "title": "t"}}]
        });
        host.advance_job(&id, JobState::AwaitingReview, Some("h2"), Some(patch.clone()), None)
            .expect("awaiting review");
        let job = host.get_job(&id).unwrap();
        assert_eq!(job.result, Some(patch), "review payload must be exposed as result");
    }

    #[test]
    fn checkpoint_persistence_and_recovery_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let pdf = dummy_pdf(dir.path());
        let (job_id, file_hash);
        {
            let mut host = AgentHost::new(dir.path().to_path_buf());
            let job = host.create_job(&pdf).expect("create");
            job_id = job.job_id.clone();
            file_hash = job.file_hash.clone();
            host.advance_job(&job_id, JobState::ValidatingFile, Some("h1"), None, None).expect("a1");
            host.advance_job(&job_id, JobState::ExtractingText, Some("h2"), None, None).expect("a2");
            host.persist_checkpoints().expect("persist");
        }
        {
            let mut host = AgentHost::new(dir.path().to_path_buf());
            host.jobs.insert(job_id.clone(), AgentJob {
                job_id: job_id.clone(),
                pdf_path: pdf.to_string_lossy().into_owned(),
                file_hash,
                state: JobState::Created,
                checkpoints: Vec::new(),
                created_at: unix_millis(),
                updated_at: unix_millis(),
                result: None,
                error: None,
            });
            let recovered = host.recover_job(&job_id).expect("recover");
            assert_eq!(recovered, JobState::ExtractingText);
            let job = host.get_job(&job_id).expect("get");
            assert_eq!(job.checkpoints.len(), 2);
            assert!(job.progress().0 >= 1);
        }
    }

    #[test]
    fn cancel_job_from_any_non_terminal_state() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let id = { host.create_job(&pdf).expect("create").job_id.clone() };
        host.advance_job(&id, JobState::ValidatingFile, Some("h1"), None, None).expect("a1");
        host.cancel_job(&id, "user requested cancel").expect("cancel");
        let job = host.get_job(&id).expect("get");
        assert_eq!(job.state, JobState::Failed);
        assert_eq!(job.error, Some("user requested cancel".to_string()));
        assert!(job.state.is_terminal());
    }

    #[test]
    fn review_patch_accepts_and_rejects() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let stages = [
            JobState::ValidatingFile, JobState::ExtractingText, JobState::OcrOptional,
            JobState::BuildingDocumentMap, JobState::ExtractingSemantics,
            JobState::GeneratingPatch, JobState::AwaitingReview,
        ];

        // Accept case
        let pdf1 = dummy_pdf(dir.path());
        let job_id1 = {
            let job = host.create_job(&pdf1).expect("create1");
            let id = job.job_id.clone();
            for &stage in &stages {
                host.advance_job(&id, stage, Some("h"), None, None).expect("advance");
            }
            id
        };
        host.review_patch(&job_id1, true).expect("accept");
        assert_eq!(host.get_job(&job_id1).unwrap().state, JobState::Accepted);

        // Reject case — 需要不同的 PDF 文件以绕过幂等
        let pdf2 = dir.path().join("test2.pdf");
        fs::write(&pdf2, b"%PDF-1.5\n%%EOF").expect("write second dummy pdf");
        let job_id2 = {
            let job = host.create_job(&pdf2).expect("create2");
            let id = job.job_id.clone();
            for &stage in &stages {
                host.advance_job(&id, stage, Some("h"), None, None).expect("advance");
            }
            id
        };
        host.review_patch(&job_id2, false).expect("reject");
        assert_eq!(host.get_job(&job_id2).unwrap().state, JobState::Rejected);
    }

    #[test]
    fn cancel_job_rejects_already_terminal() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let id = { host.create_job(&pdf).expect("create").job_id.clone() };
        host.advance_job(&id, JobState::ValidatingFile, Some("h"), None, None).expect("a1");
        host.advance_job(&id, JobState::Failed, Some("h2"), None, Some("err")).expect("a2");
        assert!(host.cancel_job(&id, "too late").is_err());
    }

    #[test]
    fn cleanup_stale_terminal_jobs() {
        let dir = tempdir().expect("tempdir");
        let mut host = AgentHost::new(dir.path().to_path_buf());
        let pdf = dummy_pdf(dir.path());
        let id = { host.create_job(&pdf).expect("create").job_id.clone() };
        host.advance_job(&id, JobState::ValidatingFile, Some("h1"), None, None).expect("a1");
        host.advance_job(&id, JobState::Failed, Some("h2"), None, Some("test failure")).expect("a2");
        if let Some(job) = host.jobs.get_mut(&id) { job.updated_at = 0; }
        let removed = host.cleanup_completed().expect("cleanup");
        assert_eq!(removed, 1);
        assert!(host.get_job(&id).is_none());
    }
}
