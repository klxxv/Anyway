//! Pure, bounded in-memory audit ledger for the AnCordis extension host.
//!
//! This module records one audit event per Host SDK call that reaches the
//! kernel gateway, including the trace parent the gateway previously validated
//! but never persisted. The ledger is bounded (`max_events` config, eviction
//! from the front) and deliberately has no worker: callers hold the kernel's
//! [`std::sync::RwLock`] around the ledger and the ledger itself is lock-free.
//! Events carry a monotonically increasing sequence that keeps increasing even
//! as old events are evicted, so `query` windows can be followed over time.
//! This module contains no Tauri, tokio, or Host Bus types.

use std::collections::VecDeque;

use super::identity::PrincipalId;

/// Default maximum number of audit events retained in memory.
pub const DEFAULT_MAX_EVENTS: usize = 1024;

/// Upper bound on the `operation` string recorded in one event.
pub const MAX_AUDIT_OPERATION_CHARS: usize = 160;

/// Upper bound on the `trace_parent` string recorded in one event.
pub const MAX_AUDIT_TRACE_PARENT_CHARS: usize = 256;

/// The outcome of one Host SDK call as seen by the kernel gateway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditOutcome {
    Authorized,
    Denied,
    Completed,
    Failed,
}

impl std::fmt::Display for AuditOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Authorized => "authorized",
            Self::Denied => "denied",
            Self::Completed => "completed",
            Self::Failed => "failed",
        })
    }
}

/// One immutable audit record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    pub sequence: u64,
    pub principal: PrincipalId,
    pub operation: String,
    pub trace_parent: Option<String>,
    pub timestamp_ms: u64,
    pub outcome: AuditOutcome,
}

/// Ledger bounds validated at construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditConfig {
    pub max_events: usize,
}

impl AuditConfig {
    pub fn new(max_events: usize) -> Result<Self, AuditError> {
        if max_events == 0 {
            return Err(AuditError::Invalid(
                "audit ledger max_events must be non-zero".to_string(),
            ));
        }
        Ok(Self { max_events })
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            max_events: DEFAULT_MAX_EVENTS,
        }
    }
}

/// Failure domain for the audit ledger.
///
/// Only construction validates input (`record` and `query` are infallible;
/// oversized text fields are truncated, never rejected).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditError {
    Invalid(String),
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => {
                write!(formatter, "invalid audit ledger input: {reason}")
            }
        }
    }
}

impl std::error::Error for AuditError {}

/// A bounded, in-memory, lock-free audit ledger.
///
/// The ledger never locks or spawns; the kernel's [`RwLock`] is held by the
/// caller. `record` appends an event with the next sequence number and evicts
/// from the front once the retained window exceeds `max_events`.
#[derive(Default)]
pub struct AuditLedger {
    events: VecDeque<AuditEvent>,
    next_sequence: u64,
    config: AuditConfig,
}

impl AuditLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: AuditConfig) -> Self {
        Self {
            events: VecDeque::new(),
            next_sequence: 0,
            config,
        }
    }

    pub fn config(&self) -> &AuditConfig {
        &self.config
    }

    /// Number of events currently retained (bounded by `max_events`).
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Append one event and return its assigned sequence number.
    ///
    /// Sequences start at 1 and are strictly monotonic even as old events are
    /// evicted from the front, so a caller can resume `query` from the last
    /// sequence it consumed.
    pub fn record(
        &mut self,
        principal: PrincipalId,
        operation: String,
        trace_parent: Option<String>,
        timestamp_ms: u64,
        outcome: AuditOutcome,
    ) -> u64 {
        self.next_sequence = self.next_sequence.saturating_add(1);
        let sequence = self.next_sequence;
        self.events.push_back(AuditEvent {
            sequence,
            principal,
            operation: bound_chars(operation, MAX_AUDIT_OPERATION_CHARS),
            trace_parent: trace_parent
                .map(|value| bound_chars(value, MAX_AUDIT_TRACE_PARENT_CHARS)),
            timestamp_ms,
            outcome,
        });
        while self.events.len() > self.config.max_events {
            self.events.pop_front();
        }
        sequence
    }

    /// Clone retained events with `sequence >= from_sequence_inclusive`, in
    /// sequence order, returning at most `limit` events. A zero limit returns
    /// an empty window.
    pub fn query(&self, from_sequence_inclusive: u64, limit: usize) -> Vec<AuditEvent> {
        if limit == 0 {
            return Vec::new();
        }
        self.events
            .iter()
            .filter(|event| event.sequence >= from_sequence_inclusive)
            .take(limit)
            .cloned()
            .collect()
    }
}

/// Truncate a string to at most `max_chars` characters.
///
/// Defense in depth: the gateway already bounds these fields on the wire, so
/// the ledger only ever needs to cap an oversized value, never reject it.
fn bound_chars(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value
    } else {
        value.chars().take(max_chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(name: &str) -> PrincipalId {
        PrincipalId::new(name).expect("principal")
    }

    fn record_one(ledger: &mut AuditLedger, sequence_label: u64, outcome: AuditOutcome) -> u64 {
        ledger.record(
            principal("native.ui"),
            format!("op.{sequence_label}"),
            Some(format!("trace-{sequence_label}")),
            sequence_label * 1_000,
            outcome,
        )
    }

    #[test]
    fn sequences_start_at_one_and_are_monotonic() {
        let mut ledger = AuditLedger::new();
        assert_eq!(ledger.len(), 0);

        let first = ledger.record(
            principal("native.ui"),
            "op.one".to_string(),
            None,
            1,
            AuditOutcome::Completed,
        );
        assert_eq!(first, 1);
        let second = ledger.record(
            principal("native.ui"),
            "op.two".to_string(),
            None,
            2,
            AuditOutcome::Failed,
        );
        assert_eq!(second, 2);

        let events = ledger.query(1, 10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
    }

    #[test]
    fn eviction_keeps_the_newest_max_events_and_sequences_keep_increasing() {
        let config = AuditConfig::new(3).expect("config");
        let mut ledger = AuditLedger::with_config(config);
        for label in 1..=5 {
            record_one(&mut ledger, label, AuditOutcome::Completed);
        }

        assert_eq!(ledger.len(), 3);
        let events = ledger.query(1, 100);
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![3, 4, 5],
            "the oldest events must be evicted from the front"
        );

        let next = record_one(&mut ledger, 6, AuditOutcome::Denied);
        assert_eq!(next, 6, "sequences keep increasing across evictions");
        assert_eq!(ledger.len(), 3);
        let events = ledger.query(1, 100);
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![4, 5, 6]
        );
    }

    #[test]
    fn query_windows_by_sequence_and_limit() {
        let mut ledger = AuditLedger::new();
        for label in 1..=5 {
            record_one(&mut ledger, label, AuditOutcome::Completed);
        }

        let sequences = |window: Vec<AuditEvent>| {
            window
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>()
        };
        assert_eq!(sequences(ledger.query(1, 2)), vec![1, 2]);
        assert_eq!(sequences(ledger.query(3, 2)), vec![3, 4]);
        assert_eq!(sequences(ledger.query(3, 100)), vec![3, 4, 5]);
        assert!(ledger.query(6, 100).is_empty(), "past the newest sequence");
        assert_eq!(ledger.query(1, 0), Vec::new());
        assert_eq!(ledger.query(0, 10).len(), 5);
    }

    #[test]
    fn outcomes_display_in_lowercase() {
        assert_eq!(AuditOutcome::Authorized.to_string(), "authorized");
        assert_eq!(AuditOutcome::Denied.to_string(), "denied");
        assert_eq!(AuditOutcome::Completed.to_string(), "completed");
        assert_eq!(AuditOutcome::Failed.to_string(), "failed");
    }

    #[test]
    fn principal_operation_and_trace_are_recorded_verbatim() {
        let mut ledger = AuditLedger::new();
        let sequence = ledger.record(
            principal("plugin.acme"),
            "service.call".to_string(),
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string()),
            7_777,
            AuditOutcome::Completed,
        );
        assert_eq!(sequence, 1);

        let event = &ledger.query(1, 1)[0];
        assert_eq!(event.principal.as_str(), "plugin.acme");
        assert_eq!(event.operation, "service.call");
        assert_eq!(
            event.trace_parent.as_deref(),
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
        );
        assert_eq!(event.timestamp_ms, 7_777);
        assert_eq!(event.outcome, AuditOutcome::Completed);
    }

    #[test]
    fn oversized_operation_and_trace_are_truncated_not_rejected() {
        let mut ledger = AuditLedger::new();
        ledger.record(
            principal("native.ui"),
            "x".repeat(MAX_AUDIT_OPERATION_CHARS + 50),
            Some("y".repeat(MAX_AUDIT_TRACE_PARENT_CHARS + 50)),
            1,
            AuditOutcome::Failed,
        );

        let event = &ledger.query(1, 1)[0];
        assert_eq!(event.operation.chars().count(), MAX_AUDIT_OPERATION_CHARS);
        assert_eq!(
            event
                .trace_parent
                .as_ref()
                .expect("trace parent recorded")
                .chars()
                .count(),
            MAX_AUDIT_TRACE_PARENT_CHARS
        );
    }

    #[test]
    fn config_rejects_zero_max_events_and_defaults_to_1024() {
        assert!(matches!(AuditConfig::new(0), Err(AuditError::Invalid(_))));
        assert_eq!(
            AuditConfig::default(),
            AuditConfig {
                max_events: DEFAULT_MAX_EVENTS
            }
        );
        assert_eq!(AuditLedger::new().config().max_events, DEFAULT_MAX_EVENTS);
        assert_eq!(
            AuditLedger::new().config(),
            &AuditConfig {
                max_events: DEFAULT_MAX_EVENTS
            }
        );
    }

    #[test]
    fn audit_errors_stringify_for_transport_boundaries() {
        let message = AuditError::Invalid("max_events must be non-zero".to_string()).to_string();
        assert!(
            message.contains("invalid audit ledger input"),
            "message: {message}"
        );
        assert!(message.contains("max_events"), "message: {message}");
    }
}
