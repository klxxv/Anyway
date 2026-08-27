//! Pure in-memory AnMarket supply-chain gate for the Phase 7 migration slice.
//!
//! A package candidate must pass quarantine -> scan -> approval -> atomic
//! activation before it may be installed. This module is the PURE state
//! machine for that transaction: it is lock-free (the kernel's [`RwLock`] is
//! held by the caller), bounded (digest, scanner identity, finding, and
//! candidate caps), and fail-closed (every transition requires the exact
//! predecessor state, and a rejected candidate can never be approved or
//! activated). Activation emits a typed [`ActivationDecision`]; a later host
//! adapter maps that decision to the real install. This module deliberately
//! contains no Tauri, tokio, or Host Bus types.

use std::collections::BTreeMap;

/// Default maximum number of concurrently tracked package candidates.
pub const DEFAULT_MAX_CANDIDATES: usize = 64;

/// A digest is exactly 64 lowercase hex characters (`^[a-f0-9]{64}$`).
const DIGEST_HEX_CHARS: usize = 64;

/// Upper bound shared by `scanner_id` and `scanner_version`.
const MAX_SCANNER_TEXT_CHARS: usize = 128;

/// A scan report carries at most this many findings.
const MAX_FINDINGS: usize = 64;

/// One finding is at most this many characters.
const MAX_FINDING_CHARS: usize = 256;

/// Where a candidate is in the supply-chain gate.
///
/// Every candidate starts explicitly in [`CandidateStatus::Quarantined`] via
/// `submit`; there is no `Default` so a candidate can never silently skip the
/// first stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateStatus {
    Quarantined,
    Scanned,
    Approved,
    Rejected { reason: String },
    Activated,
}

impl std::fmt::Display for CandidateStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Quarantined => formatter.write_str("quarantined"),
            Self::Scanned => formatter.write_str("scanned"),
            Self::Approved => formatter.write_str("approved"),
            Self::Rejected { .. } => formatter.write_str("rejected"),
            Self::Activated => formatter.write_str("activated"),
        }
    }
}

/// A bounded scanner report for one candidate digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanReport {
    pub digest: String,
    pub passed: bool,
    pub findings: Vec<String>,
    pub scanner_id: String,
    pub scanner_version: String,
}

impl ScanReport {
    pub fn new(
        digest: impl Into<String>,
        passed: bool,
        findings: Vec<String>,
        scanner_id: impl Into<String>,
        scanner_version: impl Into<String>,
    ) -> Result<Self, PackageGateError> {
        let report = Self {
            digest: digest.into(),
            passed,
            findings,
            scanner_id: scanner_id.into(),
            scanner_version: scanner_version.into(),
        };
        report.validate()?;
        Ok(report)
    }

    /// Re-validate a report regardless of how it was constructed. The fields
    /// are public, so `scan` calls this again as defense in depth.
    fn validate(&self) -> Result<(), PackageGateError> {
        if !valid_digest(&self.digest) {
            return Err(PackageGateError::Invalid(format!(
                "scan report digest must match ^[a-f0-9]{{{DIGEST_HEX_CHARS}}}$"
            )));
        }
        if !valid_scanner_identity(&self.scanner_id) {
            return Err(PackageGateError::Invalid(format!(
                "scanner_id must be 1..={MAX_SCANNER_TEXT_CHARS} characters with no control or whitespace characters"
            )));
        }
        if !valid_scanner_identity(&self.scanner_version) {
            return Err(PackageGateError::Invalid(format!(
                "scanner_version must be 1..={MAX_SCANNER_TEXT_CHARS} characters with no control or whitespace characters"
            )));
        }
        if self.findings.len() > MAX_FINDINGS {
            return Err(PackageGateError::Invalid(format!(
                "scan report carries at most {MAX_FINDINGS} findings"
            )));
        }
        for finding in &self.findings {
            if !valid_finding(finding) {
                return Err(PackageGateError::Invalid(format!(
                    "scan findings must be 1..={MAX_FINDING_CHARS} characters with no control characters"
                )));
            }
        }
        Ok(())
    }
}

/// Failure domain for the package supply-chain gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageGateError {
    DuplicateCandidate(String),
    UnknownCandidate(String),
    DigestMismatch {
        expected: String,
        got: String,
    },
    InvalidTransition {
        from: CandidateStatus,
        to: &'static str,
    },
    TooManyCandidates {
        max: usize,
    },
    Invalid(String),
}

impl std::fmt::Display for PackageGateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCandidate(digest) => {
                write!(formatter, "package candidate {digest} is already submitted")
            }
            Self::UnknownCandidate(digest) => {
                write!(formatter, "unknown package candidate {digest}")
            }
            Self::DigestMismatch { expected, got } => write!(
                formatter,
                "scan report digest {got} does not match candidate {expected}"
            ),
            Self::InvalidTransition { from, to } => {
                write!(formatter, "invalid transition from {from} to {to}")
            }
            Self::TooManyCandidates { max } => {
                write!(formatter, "package gate is full (max {max} candidates)")
            }
            Self::Invalid(reason) => {
                write!(formatter, "invalid package gate input: {reason}")
            }
        }
    }
}

impl std::error::Error for PackageGateError {}

/// Gate bounds validated at construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageGateConfig {
    pub max_candidates: usize,
}

impl PackageGateConfig {
    pub fn new(max_candidates: usize) -> Result<Self, PackageGateError> {
        if max_candidates == 0 {
            return Err(PackageGateError::Invalid(
                "package gate max_candidates must be non-zero".to_string(),
            ));
        }
        Ok(Self { max_candidates })
    }
}

impl Default for PackageGateConfig {
    fn default() -> Self {
        Self {
            max_candidates: DEFAULT_MAX_CANDIDATES,
        }
    }
}

/// The typed decision emitted when a candidate crosses the gate.
///
/// A later host adapter maps this decision to the real install; the gate
/// itself never installs anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationDecision {
    pub digest: String,
    pub activated_at_ms: u64,
}

struct CandidateRecord {
    status: CandidateStatus,
    /// Recorded as part of the submission contract for future audit and
    /// snapshot surfaces.
    #[allow(dead_code)]
    submitted_at_ms: u64,
}

/// A bounded, in-memory, lock-free package supply-chain gate.
///
/// The gate never locks or spawns; the kernel's [`RwLock`] is held by the
/// caller. Every transition is fail-closed: `approve` before `scan`, `activate`
/// before `approve`, scanning twice, and touching a `Rejected` candidate are
/// all rejected without changing state.
#[derive(Default)]
pub struct PackageGate {
    candidates: BTreeMap<String, CandidateRecord>,
    config: PackageGateConfig,
}

impl PackageGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: PackageGateConfig) -> Self {
        Self {
            candidates: BTreeMap::new(),
            config,
        }
    }

    pub fn config(&self) -> &PackageGateConfig {
        &self.config
    }

    /// Number of tracked candidates, for ledger and test snapshots.
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Current status of a candidate, for tests and the ledger.
    pub fn status(&self, digest: &str) -> Option<CandidateStatus> {
        self.candidates
            .get(digest)
            .map(|record| record.status.clone())
    }

    /// Enter a candidate into quarantine. Rejects non-hex digests,
    /// duplicates, and the candidate cap.
    pub fn submit(&mut self, digest: &str, now_ms: u64) -> Result<(), PackageGateError> {
        if !valid_digest(digest) {
            return Err(PackageGateError::Invalid(format!(
                "package digest must match ^[a-f0-9]{{{DIGEST_HEX_CHARS}}}$"
            )));
        }
        if self.candidates.contains_key(digest) {
            return Err(PackageGateError::DuplicateCandidate(digest.to_string()));
        }
        if self.candidates.len() >= self.config.max_candidates {
            return Err(PackageGateError::TooManyCandidates {
                max: self.config.max_candidates,
            });
        }
        self.candidates.insert(
            digest.to_string(),
            CandidateRecord {
                status: CandidateStatus::Quarantined,
                submitted_at_ms: now_ms,
            },
        );
        Ok(())
    }

    /// Apply a scan report. Only a quarantined candidate may be scanned, the
    /// report digest must match the candidate, and a failing report moves the
    /// candidate to `Rejected` (never approvable, never activatable).
    pub fn scan(
        &mut self,
        digest: &str,
        report: &ScanReport,
        _now_ms: u64,
    ) -> Result<(), PackageGateError> {
        report.validate()?;
        let record = self
            .candidates
            .get_mut(digest)
            .ok_or_else(|| PackageGateError::UnknownCandidate(digest.to_string()))?;
        if report.digest != digest {
            return Err(PackageGateError::DigestMismatch {
                expected: digest.to_string(),
                got: report.digest.clone(),
            });
        }
        if record.status != CandidateStatus::Quarantined {
            return Err(PackageGateError::InvalidTransition {
                from: record.status.clone(),
                to: "scanned",
            });
        }
        record.status = if report.passed {
            CandidateStatus::Scanned
        } else {
            let reason = report
                .findings
                .first()
                .cloned()
                .unwrap_or_else(|| "scan failed".to_string());
            CandidateStatus::Rejected { reason }
        };
        Ok(())
    }

    /// Approve a scanned candidate. A rejected candidate must never be
    /// approvable.
    pub fn approve(&mut self, digest: &str, _now_ms: u64) -> Result<(), PackageGateError> {
        let record = self
            .candidates
            .get_mut(digest)
            .ok_or_else(|| PackageGateError::UnknownCandidate(digest.to_string()))?;
        if record.status != CandidateStatus::Scanned {
            return Err(PackageGateError::InvalidTransition {
                from: record.status.clone(),
                to: "approved",
            });
        }
        record.status = CandidateStatus::Approved;
        Ok(())
    }

    /// Atomically activate an approved candidate, emitting the typed decision.
    pub fn activate(
        &mut self,
        digest: &str,
        now_ms: u64,
    ) -> Result<ActivationDecision, PackageGateError> {
        let record = self
            .candidates
            .get_mut(digest)
            .ok_or_else(|| PackageGateError::UnknownCandidate(digest.to_string()))?;
        if record.status != CandidateStatus::Approved {
            return Err(PackageGateError::InvalidTransition {
                from: record.status.clone(),
                to: "activated",
            });
        }
        record.status = CandidateStatus::Activated;
        Ok(ActivationDecision {
            digest: digest.to_string(),
            activated_at_ms: now_ms,
        })
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == DIGEST_HEX_CHARS
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_scanner_identity(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_SCANNER_TEXT_CHARS
        && !value.chars().any(char::is_control)
        && !value.chars().any(char::is_whitespace)
}

fn valid_finding(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_FINDING_CHARS
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest_a() -> String {
        "a".repeat(DIGEST_HEX_CHARS)
    }

    fn digest_b() -> String {
        "b".repeat(DIGEST_HEX_CHARS)
    }

    fn passing_report(digest: &str) -> ScanReport {
        ScanReport::new(
            digest,
            true,
            Vec::new(),
            "anyway.scanner.virustotal",
            "1.0.0",
        )
        .expect("passing report")
    }

    fn failing_report(digest: &str) -> ScanReport {
        ScanReport::new(
            digest,
            false,
            vec!["malware signature match".to_string()],
            "anyway.scanner.virustotal",
            "1.0.0",
        )
        .expect("failing report")
    }

    #[test]
    fn happy_path_reaches_activated_and_returns_the_decision() {
        let mut gate = PackageGate::new();
        let digest = digest_a();

        gate.submit(&digest, 1_000).expect("submits");
        assert_eq!(gate.status(&digest), Some(CandidateStatus::Quarantined));

        gate.scan(&digest, &passing_report(&digest), 2_000)
            .expect("passes scan");
        assert_eq!(gate.status(&digest), Some(CandidateStatus::Scanned));

        gate.approve(&digest, 3_000).expect("approves");
        assert_eq!(gate.status(&digest), Some(CandidateStatus::Approved));

        let decision = gate.activate(&digest, 4_000).expect("activates");
        assert_eq!(
            decision,
            ActivationDecision {
                digest: digest.clone(),
                activated_at_ms: 4_000,
            }
        );
        assert_eq!(gate.status(&digest), Some(CandidateStatus::Activated));
        assert_eq!(gate.candidate_count(), 1);
    }

    #[test]
    fn failing_scan_rejects_and_locks_out_approval_and_activation() {
        let mut gate = PackageGate::new();
        let digest = digest_a();

        gate.submit(&digest, 1_000).expect("submits");
        gate.scan(&digest, &failing_report(&digest), 2_000)
            .expect("failing scan is recorded");

        assert_eq!(
            gate.status(&digest),
            Some(CandidateStatus::Rejected {
                reason: "malware signature match".to_string()
            })
        );
        assert_eq!(
            gate.approve(&digest, 3_000),
            Err(PackageGateError::InvalidTransition {
                from: CandidateStatus::Rejected {
                    reason: "malware signature match".to_string()
                },
                to: "approved",
            })
        );
        assert_eq!(
            gate.activate(&digest, 4_000),
            Err(PackageGateError::InvalidTransition {
                from: CandidateStatus::Rejected {
                    reason: "malware signature match".to_string()
                },
                to: "activated",
            })
        );
        assert_eq!(
            gate.status(&digest).unwrap(),
            CandidateStatus::Rejected {
                reason: "malware signature match".to_string()
            }
        );
    }

    #[test]
    fn failing_scan_without_findings_uses_the_default_reason() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        gate.submit(&digest, 1_000).expect("submits");

        let report = ScanReport::new(
            &digest,
            false,
            Vec::new(),
            "anyway.scanner.virustotal",
            "1.0.0",
        )
        .expect("report");
        gate.scan(&digest, &report, 2_000).expect("records failure");
        assert_eq!(
            gate.status(&digest),
            Some(CandidateStatus::Rejected {
                reason: "scan failed".to_string()
            })
        );
    }

    #[test]
    fn approve_before_scan_is_an_invalid_transition() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        gate.submit(&digest, 1_000).expect("submits");

        assert_eq!(
            gate.approve(&digest, 2_000),
            Err(PackageGateError::InvalidTransition {
                from: CandidateStatus::Quarantined,
                to: "approved",
            })
        );
        assert_eq!(
            gate.status(&digest),
            Some(CandidateStatus::Quarantined),
            "failed transition must not change status"
        );
    }

    #[test]
    fn activate_before_approve_is_an_invalid_transition() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        gate.submit(&digest, 1_000).expect("submits");

        assert_eq!(
            gate.activate(&digest, 2_000),
            Err(PackageGateError::InvalidTransition {
                from: CandidateStatus::Quarantined,
                to: "activated",
            })
        );
        gate.scan(&digest, &passing_report(&digest), 3_000)
            .expect("passes scan");

        assert_eq!(
            gate.activate(&digest, 4_000),
            Err(PackageGateError::InvalidTransition {
                from: CandidateStatus::Scanned,
                to: "activated",
            })
        );
        assert_eq!(
            gate.status(&digest),
            Some(CandidateStatus::Scanned),
            "failed transition must not change status"
        );
    }

    #[test]
    fn scanning_twice_is_an_invalid_transition() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        gate.submit(&digest, 1_000).expect("submits");
        gate.scan(&digest, &passing_report(&digest), 2_000)
            .expect("first scan");

        assert_eq!(
            gate.scan(&digest, &passing_report(&digest), 3_000),
            Err(PackageGateError::InvalidTransition {
                from: CandidateStatus::Scanned,
                to: "scanned",
            })
        );
        assert_eq!(gate.status(&digest), Some(CandidateStatus::Scanned));
    }

    #[test]
    fn duplicate_submit_is_rejected() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        gate.submit(&digest, 1_000).expect("first submission");

        assert_eq!(
            gate.submit(&digest, 2_000),
            Err(PackageGateError::DuplicateCandidate(digest))
        );
        assert_eq!(gate.candidate_count(), 1);
    }

    #[test]
    fn unknown_digests_are_rejected_everywhere() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        let report = passing_report(&digest);

        assert_eq!(
            gate.submit(&digest, 1_000),
            Ok(()),
            "digest must be submit-able"
        );
        assert_eq!(
            gate.scan(&digest_b(), &report, 2_000),
            Err(PackageGateError::UnknownCandidate(digest_b()))
        );
        assert_eq!(
            gate.approve(&digest_b(), 3_000),
            Err(PackageGateError::UnknownCandidate(digest_b()))
        );
        assert_eq!(
            gate.activate(&digest_b(), 4_000),
            Err(PackageGateError::UnknownCandidate(digest_b()))
        );
        assert_eq!(gate.status(&digest_b()), None);
    }

    #[test]
    fn scan_report_digest_mismatch_is_rejected() {
        let mut gate = PackageGate::new();
        let digest = digest_a();
        gate.submit(&digest, 1_000).expect("submits");

        assert_eq!(
            gate.scan(&digest, &passing_report(&digest_b()), 2_000),
            Err(PackageGateError::DigestMismatch {
                expected: digest.clone(),
                got: digest_b(),
            })
        );
        assert_eq!(
            gate.status(&digest),
            Some(CandidateStatus::Quarantined),
            "mismatched scan must not change status"
        );
    }

    #[test]
    fn the_candidate_cap_is_enforced() {
        let config = PackageGateConfig::new(2).expect("config");
        let mut gate = PackageGate::with_config(config);
        gate.submit(&digest_a(), 1_000).expect("first candidate");
        gate.submit(&digest_b(), 2_000).expect("second candidate");

        let third = "c".repeat(DIGEST_HEX_CHARS);
        assert_eq!(
            gate.submit(&third, 3_000),
            Err(PackageGateError::TooManyCandidates { max: 2 })
        );
        assert_eq!(gate.candidate_count(), 2);
        assert_eq!(gate.config(), &PackageGateConfig { max_candidates: 2 });
    }

    #[test]
    fn non_hex_digests_are_rejected_at_submit() {
        let mut gate = PackageGate::new();
        let too_short = "a".repeat(DIGEST_HEX_CHARS - 1);
        let too_long = "a".repeat(DIGEST_HEX_CHARS + 1);
        let uppercase = "A".repeat(DIGEST_HEX_CHARS);
        let non_hex = "g".repeat(DIGEST_HEX_CHARS);
        let spaced = format!(
            "{} {}",
            "a".repeat(DIGEST_HEX_CHARS / 2),
            "a".repeat(DIGEST_HEX_CHARS / 2)
        );

        for invalid in [too_short, too_long, uppercase, non_hex, spaced] {
            assert!(
                matches!(
                    gate.submit(&invalid, 1_000),
                    Err(PackageGateError::Invalid(_))
                ),
                "accepted invalid digest {invalid:?}"
            );
        }
        assert_eq!(gate.candidate_count(), 0);
    }

    #[test]
    fn scan_report_validation_rejects_bad_input() {
        let bad_digest = "A".repeat(DIGEST_HEX_CHARS);
        assert!(matches!(
            ScanReport::new(
                bad_digest,
                true,
                Vec::new(),
                "anyway.scanner.virustotal",
                "1.0.0",
            ),
            Err(PackageGateError::Invalid(_))
        ));

        let whitespace_scanner =
            ScanReport::new(digest_a(), true, Vec::new(), "anyway scanner", "1.0.0");
        assert!(
            matches!(whitespace_scanner, Err(PackageGateError::Invalid(_))),
            "scanner_id with whitespace must be rejected"
        );

        let empty_version = ScanReport::new(digest_a(), true, Vec::new(), "anyway.scanner", "");
        assert!(
            matches!(empty_version, Err(PackageGateError::Invalid(_))),
            "empty scanner_version must be rejected"
        );

        let too_long_scanner = ScanReport::new(
            digest_a(),
            true,
            Vec::new(),
            "s".repeat(MAX_SCANNER_TEXT_CHARS + 1),
            "1.0.0",
        );
        assert!(
            matches!(too_long_scanner, Err(PackageGateError::Invalid(_))),
            "oversized scanner_id must be rejected"
        );

        let too_many_findings = ScanReport::new(
            digest_a(),
            false,
            vec!["finding".to_string(); MAX_FINDINGS + 1],
            "anyway.scanner",
            "1.0.0",
        );
        assert!(
            matches!(too_many_findings, Err(PackageGateError::Invalid(_))),
            "too many findings must be rejected"
        );

        let empty_finding = ScanReport::new(
            digest_a(),
            false,
            vec![String::new()],
            "anyway.scanner",
            "1.0.0",
        );
        assert!(
            matches!(empty_finding, Err(PackageGateError::Invalid(_))),
            "empty finding must be rejected"
        );

        let control_finding = ScanReport::new(
            digest_a(),
            false,
            vec!["bad\u{0007}finding".to_string()],
            "anyway.scanner",
            "1.0.0",
        );
        assert!(
            matches!(control_finding, Err(PackageGateError::Invalid(_))),
            "control character finding must be rejected"
        );

        let oversized_finding = ScanReport::new(
            digest_a(),
            false,
            vec!["f".repeat(MAX_FINDING_CHARS + 1)],
            "anyway.scanner",
            "1.0.0",
        );
        assert!(
            matches!(oversized_finding, Err(PackageGateError::Invalid(_))),
            "oversized finding must be rejected"
        );

        // The gate re-validates even a hand-rolled invalid report.
        let mut gate = PackageGate::new();
        gate.submit(&digest_a(), 1_000).expect("submits");
        let hand_rolled = ScanReport {
            digest: digest_a(),
            passed: true,
            findings: vec!["ok".to_string(); MAX_FINDINGS + 1],
            scanner_id: "anyway.scanner".to_string(),
            scanner_version: "1.0.0".to_string(),
        };
        assert!(matches!(
            gate.scan(&digest_a(), &hand_rolled, 2_000),
            Err(PackageGateError::Invalid(_))
        ));
    }

    #[test]
    fn config_rejects_zero_max_candidates() {
        assert!(matches!(
            PackageGateConfig::new(0),
            Err(PackageGateError::Invalid(_))
        ));
        assert_eq!(
            PackageGateConfig::default(),
            PackageGateConfig {
                max_candidates: DEFAULT_MAX_CANDIDATES
            }
        );
    }

    #[test]
    fn a_fresh_gate_is_empty_with_default_bounds() {
        let gate = PackageGate::new();
        assert_eq!(gate.candidate_count(), 0);
        assert_eq!(gate.status(&digest_a()), None);
        assert_eq!(gate.config().max_candidates, DEFAULT_MAX_CANDIDATES);
    }

    #[test]
    fn gate_errors_stringify_for_transport_boundaries() {
        let message = PackageGateError::DuplicateCandidate(digest_a()).to_string();
        assert!(message.contains("already submitted"), "message: {message}");
        assert!(message.contains(&digest_a()), "message: {message}");

        let message = PackageGateError::InvalidTransition {
            from: CandidateStatus::Quarantined,
            to: "approved",
        }
        .to_string();
        assert!(message.contains("invalid transition"), "message: {message}");
        assert!(message.contains("quarantined"), "message: {message}");
        assert!(message.contains("approved"), "message: {message}");

        let message = PackageGateError::DigestMismatch {
            expected: digest_a(),
            got: digest_b(),
        }
        .to_string();
        assert!(message.contains("does not match"), "message: {message}");

        let message = PackageGateError::TooManyCandidates { max: 2 }.to_string();
        assert!(message.contains("full"), "message: {message}");
        assert!(message.contains("2"), "message: {message}");

        let message = PackageGateError::Invalid("bad input".to_string()).to_string();
        assert!(message.contains("bad input"), "message: {message}");
        assert!(
            message.contains("invalid package gate input"),
            "message: {message}"
        );
    }
}
