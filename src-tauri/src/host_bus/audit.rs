//! Host bus audit read surface: `audit.read`.
//!
//! Read-only window over the kernel audit ledger. Sequences are strictly
//! monotonic, so callers resume from the last consumed sequence. The ledger
//! itself stays lock-free; the kernel's `RwLock<AuditLedger>` is held by the
//! caller.

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel::audit::AuditLedger;
use crate::kernel_commands::{inline_request, HostCallRequest};

const MAX_AUDIT_READ_LIMIT: usize = 512;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReadRequest {
    /// Events with `sequence >= from_sequence_inclusive` are returned.
    #[serde(default)]
    pub from_sequence_inclusive: u64,
    /// Window size, clamped to [`MAX_AUDIT_READ_LIMIT`]; `0` returns nothing.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    MAX_AUDIT_READ_LIMIT
}

/// `audit.read` — snapshot a bounded window of immutable audit records.
pub fn dispatch_audit_read(
    request: &HostCallRequest,
    ledger: &RwLock<AuditLedger>,
) -> Result<Value, String> {
    let read = inline_request::<AuditReadRequest>(request)
        .map_err(|error| format!("invalid audit.read request: {error}"))?;
    let limit = read.limit.min(MAX_AUDIT_READ_LIMIT);
    let guard = ledger
        .read()
        .map_err(|_| "audit ledger lock is poisoned".to_string())?;
    let events = guard.query(read.from_sequence_inclusive, limit);
    let next_sequence = events
        .last()
        .map(|event| event.sequence.saturating_add(1))
        .unwrap_or(read.from_sequence_inclusive);
    let window = events
        .iter()
        .map(|event| {
            json!({
                "sequence": event.sequence,
                "principal": event.principal.as_str(),
                "operation": event.operation,
                "traceParent": event.trace_parent,
                "timestampMs": event.timestamp_ms,
                "outcome": event.outcome.to_string(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "events": window,
        "nextSequence": next_sequence,
    }))
}
