//! Phase-one Inside RPC contract and state validation for the Anyway kernel.
//!
//! The wire model intentionally contains metadata or a [`BlobRef`] only. Raw
//! payload bytes belong to Blob Store data paths and never cross the control
//! plane as an inline RPC field.

use std::collections::BTreeMap;
use std::fmt;

use super::blob::BlobRef;
pub use super::identity::{CapabilityLease, PrincipalId};

#[cfg(test)]
use super::identity::{Capability, CapabilityScope};

pub const MAX_CONTROL_BYTES: usize = 16 * 1024;
pub const MAX_WINDOW_CREDITS: u32 = 4096;
const MAX_TARGET_PART_LEN: usize = 128;
const MAX_TRACE_PART_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    InvalidArgument(&'static str),
    ControlPayloadTooLarge,
    RequestExpired,
    CapabilityInactive,
    CapabilityPrincipalMismatch,
    DuplicateRequest,
    UnknownRequest,
    TooManyInflight,
    InvalidTransition,
    WrongCaller,
    WrongTarget,
    WrongTrace,
    WrongCapability,
    StreamCreditExhausted,
    SequenceMismatch,
}

impl fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidArgument(message) => message,
            Self::ControlPayloadTooLarge => "control payload too large",
            Self::RequestExpired => "request deadline expired",
            Self::CapabilityInactive => "capability lease is not active",
            Self::CapabilityPrincipalMismatch => "capability principal mismatch",
            Self::DuplicateRequest => "duplicate request",
            Self::UnknownRequest => "unknown request",
            Self::TooManyInflight => "too many in-flight requests",
            Self::InvalidTransition => "invalid rpc state transition",
            Self::WrongCaller => "rpc caller changed",
            Self::WrongTarget => "rpc target changed",
            Self::WrongTrace => "rpc trace changed",
            Self::WrongCapability => "rpc capability lease changed",
            Self::StreamCreditExhausted => "stream has no available credit",
            Self::SequenceMismatch => "stream sequence mismatch",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RpcError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u128);

impl RequestId {
    pub fn new(value: u128) -> Result<Self, RpcError> {
        if value == 0 {
            return Err(RpcError::InvalidArgument("request id must be non-zero"));
        }
        Ok(Self(value))
    }

    pub fn value(self) -> u128 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RpcTarget {
    service: String,
    method: String,
}

impl RpcTarget {
    pub fn new(service: impl Into<String>, method: impl Into<String>) -> Result<Self, RpcError> {
        let service = service.into();
        let method = method.into();
        validate_text(&service, MAX_TARGET_PART_LEN, "target service")?;
        validate_text(&method, MAX_TARGET_PART_LEN, "target method")?;
        Ok(Self { service, method })
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn method(&self) -> &str {
        &self.method
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadline {
    at_ms: u64,
}

impl Deadline {
    pub const fn at_ms(at_ms: u64) -> Self {
        Self { at_ms }
    }

    pub const fn value(self) -> u64 {
        self.at_ms
    }

    pub const fn is_expired(self, now_ms: u64) -> bool {
        now_ms >= self.at_ms
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceContext {
    trace_id: String,
    span_id: String,
    sampled: bool,
}

impl TraceContext {
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        sampled: bool,
    ) -> Result<Self, RpcError> {
        let trace_id = trace_id.into();
        let span_id = span_id.into();
        validate_text(&trace_id, MAX_TRACE_PART_LEN, "trace id")?;
        validate_text(&span_id, MAX_TRACE_PART_LEN, "span id")?;
        Ok(Self {
            trace_id,
            span_id,
            sampled,
        })
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    pub const fn sampled(&self) -> bool {
        self.sampled
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataCodec {
    Json,
    MessagePack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPayload {
    codec: MetadataCodec,
    bytes: Vec<u8>,
}

impl ControlPayload {
    pub fn new(codec: MetadataCodec, bytes: impl Into<Vec<u8>>) -> Result<Self, RpcError> {
        let bytes = bytes.into();
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(RpcError::ControlPayloadTooLarge);
        }
        if codec == MetadataCodec::Json && std::str::from_utf8(&bytes).is_err() {
            return Err(RpcError::InvalidArgument("json metadata must be utf-8"));
        }
        Ok(Self { codec, bytes })
    }

    pub fn codec(&self) -> MetadataCodec {
        self.codec
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    CallerRequested,
    Deadline,
    Shutdown,
    CapabilityRevoked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RpcPayload {
    Empty,
    Control(ControlPayload),
    Blob(BlobRef),
    Cancel(CancelReason),
}

impl RpcPayload {
    fn is_data_payload(&self) -> bool {
        matches!(self, Self::Empty | Self::Control(_) | Self::Blob(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcDirection {
    Request,
    Response,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcKind {
    Unary,
    StreamOpen,
    StreamItem,
    StreamEnd,
    Cancel,
    WindowUpdate,
}

/// A single control-plane envelope. Large data is represented only by BlobRef.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcEnvelope {
    request_id: RequestId,
    caller: PrincipalId,
    target: RpcTarget,
    deadline: Deadline,
    trace: TraceContext,
    capability_lease: CapabilityLease,
    direction: RpcDirection,
    kind: RpcKind,
    sequence: u64,
    credit: u32,
    payload: RpcPayload,
}

impl RpcEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: RequestId,
        caller: PrincipalId,
        target: RpcTarget,
        deadline: Deadline,
        trace: TraceContext,
        capability_lease: CapabilityLease,
        direction: RpcDirection,
        kind: RpcKind,
        sequence: u64,
        credit: u32,
        payload: RpcPayload,
    ) -> Result<Self, RpcError> {
        let envelope = Self {
            request_id,
            caller,
            target,
            deadline,
            trace,
            capability_lease,
            direction,
            kind,
            sequence,
            credit,
            payload,
        };
        envelope.validate_shape()?;
        Ok(envelope)
    }

    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub fn caller(&self) -> &PrincipalId {
        &self.caller
    }

    pub fn target(&self) -> &RpcTarget {
        &self.target
    }

    pub fn deadline(&self) -> Deadline {
        self.deadline
    }

    pub fn trace(&self) -> &TraceContext {
        &self.trace
    }

    pub fn capability_lease(&self) -> &CapabilityLease {
        &self.capability_lease
    }

    pub fn direction(&self) -> RpcDirection {
        self.direction
    }

    pub fn kind(&self) -> RpcKind {
        self.kind
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn credit(&self) -> u32 {
        self.credit
    }

    pub fn payload(&self) -> &RpcPayload {
        &self.payload
    }

    pub fn validate(&self, now_ms: u64) -> Result<(), RpcError> {
        self.validate_shape()?;
        if self.deadline.is_expired(now_ms) {
            return Err(RpcError::RequestExpired);
        }
        if self.capability_lease.principal() != &self.caller {
            return Err(RpcError::CapabilityPrincipalMismatch);
        }
        if !self.capability_lease.is_active_at(now_ms) {
            return Err(RpcError::CapabilityInactive);
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), RpcError> {
        if self.capability_lease.principal() != &self.caller {
            return Err(RpcError::CapabilityPrincipalMismatch);
        }
        match (self.direction, self.kind) {
            (RpcDirection::Request, RpcKind::Unary | RpcKind::StreamOpen) => {
                if self.sequence != 0 || self.credit != 0 || !self.payload.is_data_payload() {
                    return Err(RpcError::InvalidArgument("invalid request frame"));
                }
            }
            (RpcDirection::Request, RpcKind::Cancel) => {
                if self.sequence != 0
                    || self.credit != 0
                    || !matches!(self.payload, RpcPayload::Cancel(_))
                {
                    return Err(RpcError::InvalidArgument("invalid cancel frame"));
                }
            }
            (RpcDirection::Request, RpcKind::WindowUpdate) => {
                if self.sequence != 0
                    || self.credit == 0
                    || self.credit > MAX_WINDOW_CREDITS
                    || !matches!(self.payload, RpcPayload::Empty)
                {
                    return Err(RpcError::InvalidArgument("invalid window update"));
                }
            }
            (RpcDirection::Response, RpcKind::Unary) => {
                if self.sequence != 0 || self.credit != 0 || !self.payload.is_data_payload() {
                    return Err(RpcError::InvalidArgument("invalid unary response"));
                }
            }
            (RpcDirection::Response, RpcKind::StreamItem) => {
                if self.sequence == 0 || self.credit != 0 || !self.payload.is_data_payload() {
                    return Err(RpcError::InvalidArgument("invalid stream item"));
                }
            }
            (RpcDirection::Response, RpcKind::StreamEnd) => {
                if self.credit != 0 || !matches!(self.payload, RpcPayload::Empty) {
                    return Err(RpcError::InvalidArgument("invalid stream end"));
                }
            }
            (RpcDirection::Response, RpcKind::StreamOpen)
            | (RpcDirection::Response, RpcKind::Cancel)
            | (RpcDirection::Response, RpcKind::WindowUpdate)
            | (RpcDirection::Request, RpcKind::StreamItem)
            | (RpcDirection::Request, RpcKind::StreamEnd) => {
                return Err(RpcError::InvalidArgument("unsupported rpc direction"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptOutcome {
    Started,
    WindowUpdated { available_credit: u32 },
    StreamItemAccepted { remaining_credit: u32 },
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallState {
    UnaryPending,
    StreamOpen {
        next_sequence: u64,
        available_credit: u32,
    },
    Completed,
    Cancelled,
}

struct CallRecord {
    caller: PrincipalId,
    target: RpcTarget,
    deadline: Deadline,
    trace_id: String,
    capability_lease: CapabilityLease,
    state: CallState,
}

/// In-memory validator for request lifecycle, stream sequencing and backpressure.
pub struct RpcLedger {
    max_inflight: usize,
    calls: BTreeMap<RequestId, CallRecord>,
}

impl RpcLedger {
    pub fn new(max_inflight: usize) -> Result<Self, RpcError> {
        if max_inflight == 0 {
            return Err(RpcError::InvalidArgument("max inflight must be non-zero"));
        }
        Ok(Self {
            max_inflight,
            calls: BTreeMap::new(),
        })
    }

    pub fn accept(
        &mut self,
        envelope: &RpcEnvelope,
        now_ms: u64,
    ) -> Result<AcceptOutcome, RpcError> {
        envelope.validate(now_ms)?;
        match envelope.direction {
            RpcDirection::Request => self.accept_request(envelope),
            RpcDirection::Response => self.accept_response(envelope),
        }
    }

    pub fn state(&self, request_id: RequestId) -> Option<CallState> {
        self.calls.get(&request_id).map(|record| record.state)
    }

    pub fn active_count(&self) -> usize {
        self.calls
            .values()
            .filter(|record| {
                matches!(
                    record.state,
                    CallState::UnaryPending | CallState::StreamOpen { .. }
                )
            })
            .count()
    }

    fn accept_request(&mut self, envelope: &RpcEnvelope) -> Result<AcceptOutcome, RpcError> {
        match envelope.kind {
            RpcKind::Unary | RpcKind::StreamOpen => {
                if self.calls.contains_key(&envelope.request_id) {
                    return Err(RpcError::DuplicateRequest);
                }
                if self.active_count() >= self.max_inflight {
                    return Err(RpcError::TooManyInflight);
                }
                let state = match envelope.kind {
                    RpcKind::Unary => CallState::UnaryPending,
                    RpcKind::StreamOpen => CallState::StreamOpen {
                        next_sequence: 1,
                        available_credit: 0,
                    },
                    _ => unreachable!(),
                };
                self.calls.insert(
                    envelope.request_id,
                    CallRecord {
                        caller: envelope.caller.clone(),
                        target: envelope.target.clone(),
                        deadline: envelope.deadline,
                        trace_id: envelope.trace.trace_id.clone(),
                        capability_lease: envelope.capability_lease.clone(),
                        state,
                    },
                );
                Ok(AcceptOutcome::Started)
            }
            RpcKind::WindowUpdate => {
                let record = self
                    .calls
                    .get_mut(&envelope.request_id)
                    .ok_or(RpcError::UnknownRequest)?;
                validate_context(record, envelope)?;
                let CallState::StreamOpen {
                    available_credit, ..
                } = &mut record.state
                else {
                    return Err(RpcError::InvalidTransition);
                };
                *available_credit = available_credit
                    .checked_add(envelope.credit)
                    .filter(|value| *value <= MAX_WINDOW_CREDITS)
                    .ok_or(RpcError::InvalidArgument("stream credit overflow"))?;
                Ok(AcceptOutcome::WindowUpdated {
                    available_credit: *available_credit,
                })
            }
            RpcKind::Cancel => {
                let record = self
                    .calls
                    .get_mut(&envelope.request_id)
                    .ok_or(RpcError::UnknownRequest)?;
                validate_context(record, envelope)?;
                if !matches!(
                    record.state,
                    CallState::UnaryPending | CallState::StreamOpen { .. }
                ) {
                    return Err(RpcError::InvalidTransition);
                }
                record.state = CallState::Cancelled;
                Ok(AcceptOutcome::Cancelled)
            }
            RpcKind::StreamItem | RpcKind::StreamEnd => Err(RpcError::InvalidTransition),
        }
    }

    fn accept_response(&mut self, envelope: &RpcEnvelope) -> Result<AcceptOutcome, RpcError> {
        let record = self
            .calls
            .get_mut(&envelope.request_id)
            .ok_or(RpcError::UnknownRequest)?;
        validate_context(record, envelope)?;
        match (&mut record.state, envelope.kind) {
            (CallState::UnaryPending, RpcKind::Unary) => {
                record.state = CallState::Completed;
                Ok(AcceptOutcome::Completed)
            }
            (
                CallState::StreamOpen {
                    next_sequence,
                    available_credit,
                },
                RpcKind::StreamItem,
            ) => {
                if envelope.sequence != *next_sequence {
                    return Err(RpcError::SequenceMismatch);
                }
                if *available_credit == 0 {
                    return Err(RpcError::StreamCreditExhausted);
                }
                *available_credit -= 1;
                *next_sequence = next_sequence.saturating_add(1);
                Ok(AcceptOutcome::StreamItemAccepted {
                    remaining_credit: *available_credit,
                })
            }
            (CallState::StreamOpen { next_sequence, .. }, RpcKind::StreamEnd) => {
                if envelope.sequence != *next_sequence {
                    return Err(RpcError::SequenceMismatch);
                }
                record.state = CallState::Completed;
                Ok(AcceptOutcome::Completed)
            }
            _ => Err(RpcError::InvalidTransition),
        }
    }
}

fn validate_context(record: &CallRecord, envelope: &RpcEnvelope) -> Result<(), RpcError> {
    if record.caller != envelope.caller {
        return Err(RpcError::WrongCaller);
    }
    if record.target != envelope.target {
        return Err(RpcError::WrongTarget);
    }
    if record.trace_id != envelope.trace.trace_id {
        return Err(RpcError::WrongTrace);
    }
    if record.deadline != envelope.deadline {
        return Err(RpcError::InvalidArgument("deadline changed"));
    }
    if record.capability_lease != envelope.capability_lease {
        return Err(RpcError::WrongCapability);
    }
    Ok(())
}

fn validate_text(value: &str, max_len: usize, field: &'static str) -> Result<(), RpcError> {
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(RpcError::InvalidArgument(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::blob::{BlobQuota, BlobScope, BlobStore};
    use super::*;

    fn context() -> (PrincipalId, RpcTarget, TraceContext, CapabilityLease) {
        let caller = PrincipalId::new("plugin.ancordis").unwrap();
        let target = RpcTarget::new("blob", "read").unwrap();
        let trace = TraceContext::new("trace-1", "span-1", true).unwrap();
        let lease = CapabilityLease::issue(
            7,
            caller.clone(),
            Capability::BlobRead,
            CapabilityScope::Global,
            0,
            Some(1_000),
        )
        .unwrap();
        (caller, target, trace, lease)
    }

    fn envelope(
        request_id: u128,
        direction: RpcDirection,
        kind: RpcKind,
        sequence: u64,
        credit: u32,
        payload: RpcPayload,
    ) -> RpcEnvelope {
        let (caller, target, trace, lease) = context();
        RpcEnvelope::new(
            RequestId::new(request_id).unwrap(),
            caller,
            target,
            Deadline::at_ms(900),
            trace,
            lease,
            direction,
            kind,
            sequence,
            credit,
            payload,
        )
        .unwrap()
    }

    #[test]
    fn envelope_carries_only_small_metadata_or_blob_ref() {
        let payload =
            ControlPayload::new(MetadataCodec::Json, br#"{"op":"ping"}"#.to_vec()).unwrap();
        let request = envelope(
            1,
            RpcDirection::Request,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Control(payload),
        );
        assert_eq!(request.request_id().value(), 1);
        assert_eq!(request.capability_lease().capability().name(), "blob.read");

        let mut store = BlobStore::new(BlobQuota::default()).unwrap();
        let upload = store
            .begin_upload(
                "plugin.ancordis",
                BlobScope::Shared,
                "application/octet-stream",
                3,
                0,
                100,
            )
            .unwrap();
        store
            .upload_chunk(upload, "plugin.ancordis", b"abc", 1)
            .unwrap();
        let blob = store.commit_upload(upload, "plugin.ancordis", 2).unwrap();
        let request = envelope(
            2,
            RpcDirection::Request,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Blob(blob),
        );
        assert!(matches!(request.payload(), RpcPayload::Blob(_)));
        assert_eq!(
            ControlPayload::new(MetadataCodec::Json, vec![0xff]),
            Err(RpcError::InvalidArgument("json metadata must be utf-8"))
        );
        assert_eq!(
            ControlPayload::new(MetadataCodec::MessagePack, vec![0; MAX_CONTROL_BYTES + 1]),
            Err(RpcError::ControlPayloadTooLarge)
        );
    }

    #[test]
    fn unary_requires_matching_response_context() {
        let mut ledger = RpcLedger::new(2).unwrap();
        let request = envelope(
            1,
            RpcDirection::Request,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(ledger.accept(&request, 100), Ok(AcceptOutcome::Started));
        let response = envelope(
            1,
            RpcDirection::Response,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(ledger.accept(&response, 101), Ok(AcceptOutcome::Completed));
        assert_eq!(
            ledger.state(request.request_id()),
            Some(CallState::Completed)
        );
        assert_eq!(
            ledger.accept(&response, 102),
            Err(RpcError::InvalidTransition)
        );
    }

    #[test]
    fn stream_enforces_window_credit_sequence_and_end() {
        let mut ledger = RpcLedger::new(2).unwrap();
        let open = envelope(
            1,
            RpcDirection::Request,
            RpcKind::StreamOpen,
            0,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(ledger.accept(&open, 100), Ok(AcceptOutcome::Started));
        let window = envelope(
            1,
            RpcDirection::Request,
            RpcKind::WindowUpdate,
            0,
            2,
            RpcPayload::Empty,
        );
        assert_eq!(
            ledger.accept(&window, 101),
            Ok(AcceptOutcome::WindowUpdated {
                available_credit: 2
            })
        );
        let item_one = envelope(
            1,
            RpcDirection::Response,
            RpcKind::StreamItem,
            1,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(
            ledger.accept(&item_one, 102),
            Ok(AcceptOutcome::StreamItemAccepted {
                remaining_credit: 1
            })
        );
        let item_two = envelope(
            1,
            RpcDirection::Response,
            RpcKind::StreamItem,
            2,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(
            ledger.accept(&item_two, 103),
            Ok(AcceptOutcome::StreamItemAccepted {
                remaining_credit: 0
            })
        );
        let item_three = envelope(
            1,
            RpcDirection::Response,
            RpcKind::StreamItem,
            3,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(
            ledger.accept(&item_three, 104),
            Err(RpcError::StreamCreditExhausted)
        );
        let window = envelope(
            1,
            RpcDirection::Request,
            RpcKind::WindowUpdate,
            0,
            1,
            RpcPayload::Empty,
        );
        ledger.accept(&window, 105).unwrap();
        assert_eq!(
            ledger.accept(&item_three, 106),
            Ok(AcceptOutcome::StreamItemAccepted {
                remaining_credit: 0
            })
        );
        let end = envelope(
            1,
            RpcDirection::Response,
            RpcKind::StreamEnd,
            4,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(ledger.accept(&end, 107), Ok(AcceptOutcome::Completed));
    }

    #[test]
    fn deadline_cancel_and_capability_revocation_are_enforced() {
        let mut ledger = RpcLedger::new(2).unwrap();
        let expired = envelope(
            1,
            RpcDirection::Request,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(ledger.accept(&expired, 900), Err(RpcError::RequestExpired));

        let request = envelope(
            1,
            RpcDirection::Request,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Empty,
        );
        assert_eq!(ledger.accept(&request, 100), Ok(AcceptOutcome::Started));
        let cancel = envelope(
            1,
            RpcDirection::Request,
            RpcKind::Cancel,
            0,
            0,
            RpcPayload::Cancel(CancelReason::CallerRequested),
        );
        assert_eq!(ledger.accept(&cancel, 101), Ok(AcceptOutcome::Cancelled));

        let (caller, target, trace, mut revoked_lease) = context();
        revoked_lease.revoke(50);
        let revoked = RpcEnvelope::new(
            RequestId::new(2).unwrap(),
            caller,
            target,
            Deadline::at_ms(900),
            trace,
            revoked_lease,
            RpcDirection::Request,
            RpcKind::Unary,
            0,
            0,
            RpcPayload::Empty,
        )
        .unwrap();
        assert_eq!(
            ledger.accept(&revoked, 102),
            Err(RpcError::CapabilityInactive)
        );
    }
}
