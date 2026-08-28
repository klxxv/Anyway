//! Tauri-independent admission and routing state for the Anyway host bus.
//!
//! The bus deliberately stops at admission. It validates identity, capability,
//! operation, deadline, payload shape, and quotas, then returns a route for a
//! later dispatcher. It never invokes a handler and it never carries raw data.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use super::blob::BlobRef;
use super::identity::{Capability, CapabilityLease, CapabilityScope, PrincipalId};
use super::rpc::{CancelReason, Deadline, RequestId, RpcTarget};

pub const MAX_OPERATION_NAME: usize = 128;
pub const MAX_METADATA_ENTRIES: usize = 32;
pub const MAX_METADATA_TEXT_LENGTH: usize = 256;
pub const DEFAULT_MAX_INFLIGHT_PER_PRINCIPAL: usize = 64;
pub const DEFAULT_MAX_TERMINAL_RECORDS: usize = 4096;

/// Errors raised while admitting or completing a host-bus call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusError {
    InvalidArgument(&'static str),
    DuplicateOperation,
    UnknownOperation,
    DuplicateRequest,
    UnknownRequest,
    RequestExpired,
    DeadlineOverflow,
    CapabilityPrincipalMismatch,
    CapabilityInactive,
    TooManyInflight,
    InvalidTransition,
    WrongPrincipal,
}

impl fmt::Display for BusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidArgument(message) => *message,
            Self::DuplicateOperation => "operation already registered",
            Self::UnknownOperation => "unknown operation",
            Self::DuplicateRequest => "request id already used",
            Self::UnknownRequest => "unknown request",
            Self::RequestExpired => "request deadline expired",
            Self::DeadlineOverflow => "relative deadline overflow",
            Self::CapabilityPrincipalMismatch => "capability lease principal mismatch",
            Self::CapabilityInactive => "capability lease is not active",
            Self::TooManyInflight => "too many in-flight requests",
            Self::InvalidTransition => "invalid host-bus call transition",
            Self::WrongPrincipal => "request belongs to another principal",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for BusError {}

fn validate_token(value: &str, max_length: usize, kind: &'static str) -> Result<(), BusError> {
    if value.is_empty() {
        return Err(BusError::InvalidArgument(kind));
    }
    if value.chars().count() > max_length {
        return Err(BusError::InvalidArgument(kind));
    }
    if value.chars().any(char::is_control) {
        return Err(BusError::InvalidArgument(kind));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(BusError::InvalidArgument(kind));
    }
    Ok(())
}

/// Stable operation key used by the bus registry and admission requests.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationId(String);

impl OperationId {
    pub fn new(value: impl Into<String>) -> Result<Self, BusError> {
        let value = value.into();
        validate_token(&value, MAX_OPERATION_NAME, "operation name")?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OperationId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Metadata is intentionally scalar text. Binary or structured payload data
/// belongs in Blob Store and is represented by [`BlobRef`] in [`BusPayload`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata(BTreeMap<String, String>);

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Option<String>, BusError> {
        let key = key.into();
        let value = value.into();
        validate_token(&key, MAX_METADATA_TEXT_LENGTH, "metadata key")?;
        if value.chars().count() > MAX_METADATA_TEXT_LENGTH {
            return Err(BusError::InvalidArgument("metadata value"));
        }
        if value.chars().any(char::is_control) {
            return Err(BusError::InvalidArgument("metadata value"));
        }
        if !self.0.contains_key(&key) && self.0.len() >= MAX_METADATA_ENTRIES {
            return Err(BusError::InvalidArgument("metadata entry limit"));
        }
        Ok(self.0.insert(key, value))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

/// Control-plane data has no raw byte variant. Large or binary data must use a
/// content-addressed [`BlobRef`] and travel through the blob data path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusPayload {
    Empty,
    Metadata(Metadata),
    Blob(BlobRef),
}

/// Operation routing information. The bus records this descriptor but does not
/// resolve or execute the target handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationDescriptor {
    operation: OperationId,
    route: RpcTarget,
    required_capability: Capability,
    required_scope: CapabilityScope,
    max_inflight_per_principal: usize,
}

impl OperationDescriptor {
    pub fn new(
        operation: impl Into<String>,
        route: RpcTarget,
        required_capability: Capability,
        required_scope: CapabilityScope,
        max_inflight_per_principal: usize,
    ) -> Result<Self, BusError> {
        if max_inflight_per_principal == 0 {
            return Err(BusError::InvalidArgument(
                "operation max inflight must be non-zero",
            ));
        }
        Ok(Self {
            operation: OperationId::new(operation)?,
            route,
            required_capability,
            required_scope,
            max_inflight_per_principal,
        })
    }

    pub fn operation(&self) -> &OperationId {
        &self.operation
    }

    pub fn route(&self) -> &RpcTarget {
        &self.route
    }

    pub fn required_capability(&self) -> &Capability {
        &self.required_capability
    }

    pub fn required_scope(&self) -> &CapabilityScope {
        &self.required_scope
    }

    pub fn max_inflight_per_principal(&self) -> usize {
        self.max_inflight_per_principal
    }
}

/// A request supplied by a trusted transport after it has bound the caller.
/// There is intentionally no caller-provided transport or process identity
/// field that the bus could mistake for an authenticated identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionRequest {
    request_id: RequestId,
    principal: PrincipalId,
    operation: OperationId,
    deadline: Deadline,
    capability_lease: CapabilityLease,
    payload: BusPayload,
}

impl AdmissionRequest {
    pub fn new(
        request_id: RequestId,
        principal: PrincipalId,
        operation: impl Into<String>,
        deadline: Deadline,
        capability_lease: CapabilityLease,
        payload: BusPayload,
    ) -> Result<Self, BusError> {
        Ok(Self {
            request_id,
            principal,
            operation: OperationId::new(operation)?,
            deadline,
            capability_lease,
            payload,
        })
    }

    pub fn with_relative_deadline(
        request_id: RequestId,
        principal: PrincipalId,
        operation: impl Into<String>,
        relative_deadline_ms: u64,
        now_ms: u64,
        capability_lease: CapabilityLease,
        payload: BusPayload,
    ) -> Result<Self, BusError> {
        let deadline = deadline_from_relative(now_ms, relative_deadline_ms)?;
        Self::new(
            request_id,
            principal,
            operation,
            deadline,
            capability_lease,
            payload,
        )
    }

    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn operation(&self) -> &OperationId {
        &self.operation
    }

    pub fn deadline(&self) -> Deadline {
        self.deadline
    }

    pub fn capability_lease(&self) -> &CapabilityLease {
        &self.capability_lease
    }

    pub fn payload(&self) -> &BusPayload {
        &self.payload
    }
}

/// Convert a relative timeout to the absolute deterministic clock used by the
/// kernel. A zero timeout is valid to construct but is rejected at admission as
/// expired, which keeps the clock comparison consistent across transports.
pub fn deadline_from_relative(
    now_ms: u64,
    relative_deadline_ms: u64,
) -> Result<Deadline, BusError> {
    now_ms
        .checked_add(relative_deadline_ms)
        .map(Deadline::at_ms)
        .ok_or(BusError::DeadlineOverflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionState {
    Pending,
    Finished,
    Cancelled(CancelReason),
}

/// Opaque capability to finish or cancel one admitted call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionHandle {
    request_id: RequestId,
    principal: PrincipalId,
}

impl AdmissionHandle {
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }
}

struct CallRecord {
    principal: PrincipalId,
    operation: OperationId,
    deadline: Deadline,
    state: AdmissionState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostBusConfig {
    max_inflight_per_principal: usize,
    max_terminal_records: usize,
}

impl HostBusConfig {
    pub const fn new(max_inflight_per_principal: usize) -> Result<Self, BusError> {
        Self::with_limits(max_inflight_per_principal, DEFAULT_MAX_TERMINAL_RECORDS)
    }

    pub const fn with_limits(
        max_inflight_per_principal: usize,
        max_terminal_records: usize,
    ) -> Result<Self, BusError> {
        if max_inflight_per_principal == 0 {
            return Err(BusError::InvalidArgument(
                "bus max inflight must be non-zero",
            ));
        }
        if max_terminal_records == 0 {
            return Err(BusError::InvalidArgument(
                "bus terminal record limit must be non-zero",
            ));
        }
        Ok(Self {
            max_inflight_per_principal,
            max_terminal_records,
        })
    }

    pub const fn max_inflight_per_principal(self) -> usize {
        self.max_inflight_per_principal
    }

    pub const fn max_terminal_records(self) -> usize {
        self.max_terminal_records
    }
}

impl Default for HostBusConfig {
    fn default() -> Self {
        Self {
            max_inflight_per_principal: DEFAULT_MAX_INFLIGHT_PER_PRINCIPAL,
            max_terminal_records: DEFAULT_MAX_TERMINAL_RECORDS,
        }
    }
}

/// Thread-confined admission state. [`KernelState`](super::state::KernelState)
/// supplies the synchronization boundary for Tauri and other transports.
#[derive(Default)]
pub struct HostBus {
    config: HostBusConfig,
    operations: BTreeMap<OperationId, OperationDescriptor>,
    calls: BTreeMap<RequestId, CallRecord>,
    terminal_order: VecDeque<RequestId>,
    inflight_by_principal: BTreeMap<PrincipalId, usize>,
    inflight_by_operation: BTreeMap<(PrincipalId, OperationId), usize>,
}

impl HostBus {
    pub fn new(config: HostBusConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn config(&self) -> HostBusConfig {
        self.config
    }

    pub fn register_operation(&mut self, descriptor: OperationDescriptor) -> Result<(), BusError> {
        if self.operations.contains_key(descriptor.operation()) {
            return Err(BusError::DuplicateOperation);
        }
        self.operations
            .insert(descriptor.operation().clone(), descriptor);
        Ok(())
    }

    pub fn operation(&self, operation: &OperationId) -> Option<&OperationDescriptor> {
        self.operations.get(operation)
    }

    pub fn operations(&self) -> impl Iterator<Item = &OperationDescriptor> {
        self.operations.values()
    }

    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    pub fn begin(
        &mut self,
        request: AdmissionRequest,
        now_ms: u64,
    ) -> Result<AdmissionHandle, BusError> {
        if self.calls.contains_key(&request.request_id) {
            return Err(BusError::DuplicateRequest);
        }
        if request.deadline.is_expired(now_ms) {
            return Err(BusError::RequestExpired);
        }
        let descriptor = self
            .operations
            .get(&request.operation)
            .ok_or(BusError::UnknownOperation)?;
        if request.capability_lease.principal() != request.principal() {
            return Err(BusError::CapabilityPrincipalMismatch);
        }
        if !request.capability_lease.is_active_at(now_ms) {
            return Err(BusError::CapabilityInactive);
        }
        if !request.capability_lease.covers(
            request.principal(),
            descriptor.required_capability(),
            descriptor.required_scope(),
            now_ms,
        ) {
            return Err(BusError::CapabilityInactive);
        }

        let principal_count = self
            .inflight_by_principal
            .get(request.principal())
            .copied()
            .unwrap_or_default();
        let operation_key = (request.principal().clone(), request.operation.clone());
        let operation_count = self
            .inflight_by_operation
            .get(&operation_key)
            .copied()
            .unwrap_or_default();
        if principal_count >= self.config.max_inflight_per_principal
            || operation_count >= descriptor.max_inflight_per_principal()
        {
            return Err(BusError::TooManyInflight);
        }

        let handle = AdmissionHandle {
            request_id: request.request_id,
            principal: request.principal.clone(),
        };
        self.calls.insert(
            request.request_id,
            CallRecord {
                principal: request.principal.clone(),
                operation: request.operation.clone(),
                deadline: request.deadline,
                state: AdmissionState::Pending,
            },
        );
        *self
            .inflight_by_principal
            .entry(request.principal.clone())
            .or_default() += 1;
        *self.inflight_by_operation.entry(operation_key).or_default() += 1;
        Ok(handle)
    }

    pub fn state(&self, request_id: RequestId) -> Option<AdmissionState> {
        self.calls.get(&request_id).map(|record| record.state)
    }

    pub fn route_for(&self, request_id: RequestId) -> Result<&RpcTarget, BusError> {
        let record = self
            .calls
            .get(&request_id)
            .ok_or(BusError::UnknownRequest)?;
        let descriptor = self
            .operations
            .get(&record.operation)
            .ok_or(BusError::UnknownOperation)?;
        Ok(descriptor.route())
    }

    pub fn inflight_for(&self, principal: &PrincipalId) -> usize {
        self.inflight_by_principal
            .get(principal)
            .copied()
            .unwrap_or_default()
    }

    pub fn tracked_request_count(&self) -> usize {
        self.calls.len()
    }

    pub fn finish(&mut self, handle: &AdmissionHandle) -> Result<(), BusError> {
        let (principal, operation) = {
            let record = self
                .calls
                .get_mut(&handle.request_id)
                .ok_or(BusError::UnknownRequest)?;
            if record.principal != handle.principal {
                return Err(BusError::WrongPrincipal);
            }
            if record.state != AdmissionState::Pending {
                return Err(BusError::InvalidTransition);
            }
            record.state = AdmissionState::Finished;
            (record.principal.clone(), record.operation.clone())
        };
        self.release_inflight(&principal, &operation);
        self.record_terminal(handle.request_id);
        Ok(())
    }

    pub fn cancel(
        &mut self,
        handle: &AdmissionHandle,
        reason: CancelReason,
    ) -> Result<(), BusError> {
        let (principal, operation) = {
            let record = self
                .calls
                .get_mut(&handle.request_id)
                .ok_or(BusError::UnknownRequest)?;
            if record.principal != handle.principal {
                return Err(BusError::WrongPrincipal);
            }
            if record.state != AdmissionState::Pending {
                return Err(BusError::InvalidTransition);
            }
            record.state = AdmissionState::Cancelled(reason);
            (record.principal.clone(), record.operation.clone())
        };
        self.release_inflight(&principal, &operation);
        self.record_terminal(handle.request_id);
        Ok(())
    }

    /// Cancel pending calls whose absolute deadline has elapsed.
    pub fn cancel_expired(&mut self, now_ms: u64) -> usize {
        let mut expired = BTreeSet::new();
        for (request_id, record) in &self.calls {
            if record.state == AdmissionState::Pending && record.deadline.is_expired(now_ms) {
                expired.insert(*request_id);
            }
        }

        let mut cancelled = 0;
        for request_id in expired {
            let Some(record) = self.calls.get_mut(&request_id) else {
                continue;
            };
            if record.state != AdmissionState::Pending {
                continue;
            }
            record.state = AdmissionState::Cancelled(CancelReason::Deadline);
            let principal = record.principal.clone();
            let operation = record.operation.clone();
            self.release_inflight(&principal, &operation);
            self.record_terminal(request_id);
            cancelled += 1;
        }
        cancelled
    }

    fn release_inflight(&mut self, principal: &PrincipalId, operation: &OperationId) {
        decrement_count(&mut self.inflight_by_principal, principal);
        decrement_count(
            &mut self.inflight_by_operation,
            &(principal.clone(), operation.clone()),
        );
    }

    fn record_terminal(&mut self, request_id: RequestId) {
        self.terminal_order.push_back(request_id);
        while self.terminal_order.len() > self.config.max_terminal_records {
            let Some(expired_id) = self.terminal_order.pop_front() else {
                break;
            };
            if self
                .calls
                .get(&expired_id)
                .is_some_and(|record| record.state != AdmissionState::Pending)
            {
                self.calls.remove(&expired_id);
            }
        }
    }
}

fn decrement_count<K: Ord>(counts: &mut BTreeMap<K, usize>, key: &K) {
    let Some(count) = counts.get_mut(key) else {
        return;
    };
    *count -= 1;
    if *count == 0 {
        counts.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::identity::{Capability, CapabilityScope};

    fn principal() -> PrincipalId {
        PrincipalId::new("plugin.test").expect("valid principal")
    }

    fn lease() -> CapabilityLease {
        CapabilityLease::issue(
            1,
            principal(),
            Capability::GraphRead,
            CapabilityScope::Global,
            0,
            None,
        )
        .expect("valid lease")
    }

    fn bus(max_inflight: usize) -> HostBus {
        let mut bus = HostBus::new(HostBusConfig::new(max_inflight).expect("valid config"));
        bus.register_operation(
            OperationDescriptor::new(
                "graph.read",
                RpcTarget::new("graph", "read").expect("valid route"),
                Capability::GraphRead,
                CapabilityScope::Global,
                2,
            )
            .expect("valid operation"),
        )
        .expect("registered operation");
        bus
    }

    fn request(id: u128, deadline: u64) -> AdmissionRequest {
        AdmissionRequest::new(
            RequestId::new(id).expect("valid request id"),
            principal(),
            "graph.read",
            Deadline::at_ms(deadline),
            lease(),
            BusPayload::Metadata(Metadata::new()),
        )
        .expect("valid request")
    }

    #[test]
    fn admits_and_routes_without_executing_a_handler() {
        let mut bus = bus(2);
        let handle = bus.begin(request(1, 100), 10).expect("admitted");

        assert_eq!(bus.inflight_for(&principal()), 1);
        assert_eq!(
            bus.state(handle.request_id()),
            Some(AdmissionState::Pending)
        );
        let route = bus.route_for(handle.request_id()).expect("route");
        assert_eq!(route.service(), "graph");
        assert_eq!(route.method(), "read");
    }

    #[test]
    fn rejects_unknown_duplicate_expired_and_over_quota_requests() {
        let mut bus = bus(1);
        assert_eq!(
            bus.begin(
                AdmissionRequest::new(
                    RequestId::new(1).expect("id"),
                    principal(),
                    "unknown.operation",
                    Deadline::at_ms(100),
                    lease(),
                    BusPayload::Empty,
                )
                .expect("request"),
                10,
            ),
            Err(BusError::UnknownOperation)
        );
        let first = bus.begin(request(2, 100), 10).expect("admitted");
        assert_eq!(
            bus.begin(request(3, 100), 10),
            Err(BusError::TooManyInflight)
        );
        assert_eq!(
            bus.begin(request(2, 100), 10),
            Err(BusError::DuplicateRequest)
        );
        bus.finish(&first).expect("finished");
        assert_eq!(
            bus.begin(request(2, 100), 10),
            Err(BusError::DuplicateRequest)
        );
        assert_eq!(bus.begin(request(4, 10), 10), Err(BusError::RequestExpired));
    }

    #[test]
    fn finish_cancel_and_expiry_release_per_principal_quota() {
        let mut bus = bus(2);
        let first = bus.begin(request(1, 50), 10).expect("admitted");
        let second = bus.begin(request(2, 60), 10).expect("admitted");
        assert_eq!(bus.inflight_for(&principal()), 2);
        bus.cancel(&first, CancelReason::CallerRequested)
            .expect("cancelled");
        assert_eq!(
            bus.state(first.request_id()),
            Some(AdmissionState::Cancelled(CancelReason::CallerRequested))
        );
        assert_eq!(bus.inflight_for(&principal()), 1);
        assert_eq!(bus.cancel_expired(60), 1);
        assert_eq!(
            bus.state(second.request_id()),
            Some(AdmissionState::Cancelled(CancelReason::Deadline))
        );
        assert_eq!(bus.inflight_for(&principal()), 0);
        assert_eq!(bus.finish(&second), Err(BusError::InvalidTransition));
    }

    #[test]
    fn relative_deadlines_are_checked_for_overflow() {
        assert_eq!(
            deadline_from_relative(u64::MAX, 1),
            Err(BusError::DeadlineOverflow)
        );
        assert_eq!(
            deadline_from_relative(10, 25).expect("deadline").value(),
            35
        );
    }

    #[test]
    fn metadata_is_bounded_and_binary_data_is_a_blob_reference() {
        let mut metadata = Metadata::new();
        metadata
            .insert("content_type", "application/json")
            .expect("metadata");
        assert_eq!(metadata.get("content_type"), Some("application/json"));
        assert!(metadata.insert("bad\nkey", "value").is_err());
        let blob = BlobRef::from_content(
            b"payload",
            "application/octet-stream",
            super::super::blob::BlobScope::Shared,
        )
        .expect("blob ref");
        assert!(matches!(BusPayload::Blob(blob), BusPayload::Blob(_)));
    }

    #[test]
    fn terminal_request_ledger_is_bounded() {
        let mut bus = HostBus::new(HostBusConfig::with_limits(1, 2).expect("valid limits"));
        bus.register_operation(
            OperationDescriptor::new(
                "graph.read",
                RpcTarget::new("graph", "read").expect("valid route"),
                Capability::GraphRead,
                CapabilityScope::Global,
                1,
            )
            .expect("valid operation"),
        )
        .expect("registered operation");

        for id in 1..=3 {
            let handle = bus.begin(request(id, 100), 10).expect("admitted");
            bus.finish(&handle).expect("finished");
        }

        assert_eq!(bus.tracked_request_count(), 2);
        assert_eq!(bus.state(RequestId::new(1).unwrap()), None);
        assert_eq!(
            bus.state(RequestId::new(2).unwrap()),
            Some(AdmissionState::Finished)
        );
        assert_eq!(
            bus.state(RequestId::new(3).unwrap()),
            Some(AdmissionState::Finished)
        );
    }
}
