//! Tauri's single, transport-bound entry point into the Kernel Host Bus.
//!
//! The public request deliberately has no principal field. The command binds a
//! principal from the invoking webview before policy and bus admission.

use std::sync::RwLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State, WebviewWindow};

use crate::kernel::bus::{AdmissionRequest, BusError, BusPayload, HostBus, OperationDescriptor};
use crate::kernel::identity::{CapabilityLease, PrincipalId};
use crate::kernel::policy::{
    AuthorizationSource, CapabilityPolicy, PolicyError, NATIVE_UI_PRINCIPAL_NAME,
    PLUGIN_LIST_OPERATION,
};
use crate::kernel::rpc::{RequestId, RpcTarget};
use crate::kernel::state::KernelState;

pub const HOST_SDK_API_VERSION: &str = "anyway.dev/host-rpc/v1alpha1";
pub const MAX_HOST_DEADLINE_MS: u64 = 5 * 60 * 1_000;
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 64 * 1_024;
const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_OPERATION_BYTES: usize = 160;
const MAX_LEASE_IDS: usize = 32;
const MAX_LEASE_ID_BYTES: usize = 128;
const MAX_TRACE_PARENT_BYTES: usize = 256;
const MAX_BLOB_TEXT_BYTES: usize = 256;
const MAIN_WEBVIEW_LABEL: &str = "main";
const PLUGIN_LIST_MAX_INFLIGHT: usize = 8;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostCallRequest {
    api_version: String,
    request_id: String,
    operation: String,
    payload: HostPayload,
    deadline_ms: u64,
    #[serde(default)]
    capability_lease_ids: Vec<String>,
    trace_parent: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum HostPayload {
    Inline { value: Value },
    Blob { r#ref: HostBlobRef },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostBlobRef {
    algorithm: String,
    digest: String,
    size: u64,
    media_type: String,
    scope: String,
    owner: String,
    retention_class: BlobRetentionClass,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum BlobRetentionClass {
    Request,
    Session,
    Plugin,
    Persistent,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCallResponse {
    api_version: &'static str,
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<HostCallError>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostCallError {
    code: String,
    message: String,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

/// Policy synchronization is separate from Bus admission so read-only
/// authorization does not hold the routing lock.
pub struct CapabilityPolicyState {
    policy: RwLock<CapabilityPolicy>,
    clock_origin: Instant,
}

impl Default for CapabilityPolicyState {
    fn default() -> Self {
        Self {
            policy: RwLock::new(CapabilityPolicy::default()),
            clock_origin: Instant::now(),
        }
    }
}

impl CapabilityPolicyState {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.clock_origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RequestValidationError {
    ApiVersion,
    RequestId,
    Operation,
    Deadline,
    TooManyLeaseIds,
    LeaseId,
    TraceParent,
    InlinePayloadTooLarge,
    BlobRef,
    ExpectedEmptyInlinePayload,
}

impl RequestValidationError {
    fn message(&self) -> &'static str {
        match self {
            Self::ApiVersion => "unsupported Host SDK API version",
            Self::RequestId => "invalid request id",
            Self::Operation => "invalid operation",
            Self::Deadline => "invalid request deadline",
            Self::TooManyLeaseIds => "too many capability lease ids",
            Self::LeaseId => "invalid capability lease id",
            Self::TraceParent => "invalid trace parent",
            Self::InlinePayloadTooLarge => "inline payload exceeds the Host SDK limit",
            Self::BlobRef => "invalid BlobRef",
            Self::ExpectedEmptyInlinePayload => "operation expects an empty inline payload",
        }
    }
}

impl HostCallRequest {
    fn validate(&self) -> Result<(), RequestValidationError> {
        if self.api_version != HOST_SDK_API_VERSION {
            return Err(RequestValidationError::ApiVersion);
        }
        if !bounded_wire_token(&self.request_id, MAX_REQUEST_ID_BYTES) {
            return Err(RequestValidationError::RequestId);
        }
        if !valid_operation(&self.operation) {
            return Err(RequestValidationError::Operation);
        }
        if self.deadline_ms == 0 || self.deadline_ms > MAX_HOST_DEADLINE_MS {
            return Err(RequestValidationError::Deadline);
        }
        if self.capability_lease_ids.len() > MAX_LEASE_IDS {
            return Err(RequestValidationError::TooManyLeaseIds);
        }
        if self
            .capability_lease_ids
            .iter()
            .any(|id| !bounded_wire_token(id, MAX_LEASE_ID_BYTES))
        {
            return Err(RequestValidationError::LeaseId);
        }
        if self.trace_parent.as_ref().is_some_and(|trace| {
            trace.is_empty()
                || trace.len() > MAX_TRACE_PARENT_BYTES
                || trace.chars().any(char::is_control)
        }) {
            return Err(RequestValidationError::TraceParent);
        }
        match &self.payload {
            HostPayload::Inline { value } => {
                let size = serde_json::to_vec(value)
                    .map_err(|_| RequestValidationError::InlinePayloadTooLarge)?
                    .len();
                if size > MAX_INLINE_PAYLOAD_BYTES {
                    return Err(RequestValidationError::InlinePayloadTooLarge);
                }
            }
            HostPayload::Blob { r#ref } => r#ref.validate()?,
        }
        Ok(())
    }

    fn require_empty_inline_payload(&self) -> Result<(), RequestValidationError> {
        match &self.payload {
            HostPayload::Inline { value }
                if value.as_object().is_some_and(|map| map.is_empty()) =>
            {
                Ok(())
            }
            _ => Err(RequestValidationError::ExpectedEmptyInlinePayload),
        }
    }
}

impl HostBlobRef {
    fn validate(&self) -> Result<(), RequestValidationError> {
        let valid_digest = self.digest.len() == 64
            && self
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        let valid_text = [&self.media_type, &self.scope, &self.owner]
            .into_iter()
            .all(|text| bounded_text(text, MAX_BLOB_TEXT_BYTES));
        let _ = self.size;
        let _ = self.retention_class;
        if self.algorithm != "sha256" || !valid_digest || !valid_text {
            return Err(RequestValidationError::BlobRef);
        }
        Ok(())
    }
}

impl HostCallResponse {
    fn success(request_id: String, result: Value) -> Self {
        Self {
            api_version: HOST_SDK_API_VERSION,
            request_id,
            result: Some(result),
            error: None,
        }
    }

    fn failure(
        request_id: String,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            api_version: HOST_SDK_API_VERSION,
            request_id,
            result: None,
            error: Some(HostCallError {
                code: code.into(),
                message: message.into(),
                retryable,
                details: None,
            }),
        }
    }
}

/// Construct the managed state used by the single Tauri gateway.
pub fn create_kernel_state() -> Result<KernelState, String> {
    let mut bus = HostBus::default();
    let requirement = CapabilityPolicy::operation_requirement(PLUGIN_LIST_OPERATION)
        .map_err(|error| error.to_string())?;
    let route = RpcTarget::new("plugins", "list").map_err(|error| error.to_string())?;
    let descriptor = OperationDescriptor::new(
        PLUGIN_LIST_OPERATION,
        route,
        requirement.capability().clone(),
        requirement.scope().clone(),
        PLUGIN_LIST_MAX_INFLIGHT,
    )
    .map_err(|error| error.to_string())?;
    bus.register_operation(descriptor)
        .map_err(|error| error.to_string())?;
    Ok(KernelState::new(bus))
}

/// The only Host SDK command exposed to the trusted Vue shell.
///
/// Identity is derived from the invoking WebView and is never deserialized
/// from `request`. The initial policy intentionally admits only `main`.
#[tauri::command]
pub fn kernel_host_call(
    window: WebviewWindow,
    app: AppHandle,
    kernel: State<'_, KernelState>,
    policy: State<'_, CapabilityPolicyState>,
    request: HostCallRequest,
) -> HostCallResponse {
    let response_request_id = request.request_id.clone();
    if window.label() != MAIN_WEBVIEW_LABEL {
        return HostCallResponse::failure(
            response_request_id,
            "HOST_TRANSPORT_DENIED",
            "webview is not authorized for the native UI principal",
            false,
        );
    }
    if let Err(error) = request.validate() {
        return HostCallResponse::failure(
            response_request_id,
            "HOST_INVALID_REQUEST",
            error.message(),
            false,
        );
    }
    if request.operation == PLUGIN_LIST_OPERATION {
        if let Err(error) = request.require_empty_inline_payload() {
            return HostCallResponse::failure(
                response_request_id,
                "HOST_INVALID_REQUEST",
                error.message(),
                false,
            );
        }
    }

    let now_ms = policy.now_ms();
    let principal = PrincipalId::new(NATIVE_UI_PRINCIPAL_NAME)
        .expect("the native UI principal constant is valid");
    let selected_lease_ids = match parse_lease_ids(&request.capability_lease_ids) {
        Ok(ids) => ids,
        Err(message) => {
            return HostCallResponse::failure(
                response_request_id,
                "HOST_INVALID_REQUEST",
                message,
                false,
            )
        }
    };
    let lease = match authorize_for_bus(&policy, &request, &principal, &selected_lease_ids, now_ms)
    {
        Ok(lease) => lease,
        Err(error) => return policy_failure(response_request_id, error),
    };

    let request_key = request_key(&request.request_id);
    let admission = match AdmissionRequest::with_relative_deadline(
        request_key,
        principal,
        request.operation.clone(),
        request.deadline_ms,
        now_ms,
        lease,
        BusPayload::Empty,
    ) {
        Ok(admission) => admission,
        Err(error) => return bus_failure(response_request_id, error),
    };
    let handle = match kernel.write() {
        Ok(mut bus) => match bus.begin(admission, now_ms) {
            Ok(handle) => handle,
            Err(error) => return bus_failure(response_request_id, error),
        },
        Err(_) => {
            return HostCallResponse::failure(
                response_request_id,
                "HOST_INTERNAL",
                "kernel bus lock is poisoned",
                false,
            )
        }
    };

    let handler_result = dispatch(&request, app);
    let finish_result = kernel
        .write()
        .map_err(|_| "kernel bus lock is poisoned".to_string())
        .and_then(|mut bus| bus.finish(&handle).map_err(|error| error.to_string()));
    if let Err(message) = finish_result {
        return HostCallResponse::failure(response_request_id, "HOST_INTERNAL", message, false);
    }

    match handler_result {
        Ok(value) => HostCallResponse::success(response_request_id, value),
        Err(message) => {
            HostCallResponse::failure(response_request_id, "HOST_HANDLER_FAILED", message, false)
        }
    }
}

fn dispatch(request: &HostCallRequest, app: AppHandle) -> Result<Value, String> {
    match request.operation.as_str() {
        PLUGIN_LIST_OPERATION => {
            let plugins = crate::plugins::query_installed_plugins(&app)?;
            serde_json::to_value(plugins).map_err(|error| error.to_string())
        }
        _ => Err("operation has no registered kernel handler".to_string()),
    }
}

fn authorize_for_bus(
    policy_state: &CapabilityPolicyState,
    request: &HostCallRequest,
    principal: &PrincipalId,
    selected_lease_ids: &[u64],
    now_ms: u64,
) -> Result<CapabilityLease, PolicyError> {
    let policy = policy_state
        .policy
        .read()
        .map_err(|_| PolicyError::InvalidArgument("capability policy lock is poisoned"))?;
    let authorization =
        policy.authorize(&request.operation, principal, selected_lease_ids, now_ms)?;
    match authorization.source() {
        AuthorizationSource::Lease(lease_id) => policy
            .lease(lease_id)
            .cloned()
            .ok_or(PolicyError::UnknownLease(lease_id)),
        AuthorizationSource::NativeBootstrap => {
            let expires_at =
                now_ms
                    .checked_add(request.deadline_ms)
                    .ok_or(PolicyError::InvalidArgument(
                        "capability lease expiry overflow",
                    ))?;
            CapabilityLease::issue(
                bootstrap_lease_id(&request.request_id),
                principal.clone(),
                authorization.capability().clone(),
                authorization.scope().clone(),
                now_ms,
                Some(expires_at),
            )
            .map_err(|_| PolicyError::InvalidArgument("invalid bootstrap capability lease"))
        }
    }
}

fn parse_lease_ids(ids: &[String]) -> Result<Vec<u64>, &'static str> {
    ids.iter()
        .map(|id| {
            id.parse::<u64>()
                .ok()
                .filter(|value| *value != 0)
                .ok_or("capability lease ids must be non-zero unsigned integers")
        })
        .collect()
}

fn request_key(request_id: &str) -> RequestId {
    let digest = Sha256::digest(request_id.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    let value = u128::from_be_bytes(bytes).max(1);
    RequestId::new(value).expect("the hashed request id is non-zero")
}

fn bootstrap_lease_id(request_id: &str) -> u64 {
    let digest = Sha256::digest(request_id.as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[16..24]);
    u64::from_be_bytes(bytes).max(1)
}

fn policy_failure(request_id: String, error: PolicyError) -> HostCallResponse {
    let code = match &error {
        PolicyError::InvalidArgument("capability policy lock is poisoned") => "HOST_INTERNAL",
        _ => "HOST_CAPABILITY_DENIED",
    };
    HostCallResponse::failure(request_id, code, error.to_string(), false)
}

fn bus_failure(request_id: String, error: BusError) -> HostCallResponse {
    let (code, retryable) = match error {
        BusError::TooManyInflight => ("HOST_BUSY", true),
        BusError::RequestExpired => ("HOST_DEADLINE_EXCEEDED", true),
        BusError::DuplicateRequest => ("HOST_DUPLICATE_REQUEST", false),
        BusError::CapabilityInactive | BusError::CapabilityPrincipalMismatch => {
            ("HOST_CAPABILITY_DENIED", false)
        }
        _ => ("HOST_ROUTING_FAILED", false),
    };
    HostCallResponse::failure(request_id, code, error.to_string(), retryable)
}

fn bounded_wire_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._:".contains(&byte))
}

fn bounded_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn valid_operation(operation: &str) -> bool {
    if operation.is_empty()
        || operation.len() > MAX_OPERATION_BYTES
        || !operation.as_bytes()[0].is_ascii_lowercase()
    {
        return false;
    }
    let mut previous_separator = false;
    for byte in operation.bytes() {
        let separator = matches!(byte, b'.' | b'_' | b'/' | b'-');
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || separator)
            || (separator && previous_separator)
        {
            return false;
        }
        previous_separator = separator;
    }
    !previous_separator
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> HostCallRequest {
        serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "request-1",
            "operation": "plugin.list",
            "payload": { "kind": "inline", "value": {} },
            "deadlineMs": 30_000
        }))
        .expect("valid request")
    }

    #[test]
    fn request_schema_has_no_principal_escape_hatch() {
        let value = json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "request-1",
            "operation": "plugin.list",
            "payload": { "kind": "inline", "value": {} },
            "deadlineMs": 30_000,
            "principal": "plugin.attacker"
        });

        assert!(serde_json::from_value::<HostCallRequest>(value).is_err());
    }

    #[test]
    fn validates_the_version_operation_deadline_and_empty_payload() {
        let mut value = serde_json::to_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "request-1",
            "operation": "plugin.list",
            "payload": { "kind": "inline", "value": {} },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let valid: HostCallRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(valid.validate(), Ok(()));
        assert_eq!(valid.require_empty_inline_payload(), Ok(()));

        value["operation"] = json!("../unsafe");
        let invalid: HostCallRequest = serde_json::from_value(value).unwrap();
        assert_eq!(invalid.validate(), Err(RequestValidationError::Operation));
    }

    #[test]
    fn rejects_oversized_inline_payloads() {
        let mut request = request();
        request.payload = HostPayload::Inline {
            value: json!({ "data": "x".repeat(MAX_INLINE_PAYLOAD_BYTES) }),
        };
        assert_eq!(
            request.validate(),
            Err(RequestValidationError::InlinePayloadTooLarge)
        );
    }

    #[test]
    fn validates_blob_identity_without_treating_it_as_authority() {
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "request-2",
            "operation": "blob.consume",
            "payload": {
                "kind": "blob",
                "ref": {
                    "algorithm": "sha256",
                    "digest": "a".repeat(64),
                    "size": 4096,
                    "mediaType": "application/pdf",
                    "scope": "workspace:example",
                    "owner": "plugin:example",
                    "retentionClass": "request"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(request.validate(), Ok(()));
    }

    #[test]
    fn error_response_is_versioned_and_exclusive() {
        let response = HostCallResponse::failure(
            "request-1".to_string(),
            "HOST_INVALID_REQUEST",
            RequestValidationError::Deadline.message(),
            false,
        );
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["apiVersion"], HOST_SDK_API_VERSION);
        assert!(value.get("result").is_none());
        assert_eq!(value["error"]["retryable"], false);
    }

    #[test]
    fn success_response_is_versioned_and_exclusive() {
        let response = HostCallResponse::success("request-1".to_string(), json!([]));
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["apiVersion"], HOST_SDK_API_VERSION);
        assert!(value.get("error").is_none());
        assert_eq!(value["result"], json!([]));
    }

    #[test]
    fn request_hash_is_stable_and_non_zero() {
        assert_eq!(request_key("request-1"), request_key("request-1"));
        assert_ne!(request_key("request-1"), request_key("request-2"));
        assert_ne!(request_key("request-1").value(), 0);
        assert_ne!(bootstrap_lease_id("request-1"), 0);
    }

    #[test]
    fn default_kernel_state_registers_the_plugin_list_route() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let operation = crate::kernel::bus::OperationId::new(PLUGIN_LIST_OPERATION).unwrap();
        let descriptor = bus.operation(&operation).expect("registered operation");
        assert_eq!(descriptor.route().service(), "plugins");
        assert_eq!(descriptor.route().method(), "list");
        assert_eq!(
            descriptor.required_capability().name(),
            "plugin.catalog.read"
        );
    }

    #[test]
    fn native_bootstrap_authorization_materializes_a_bounded_bus_lease() {
        let policy = CapabilityPolicyState::default();
        let request = request();
        let principal = PrincipalId::new(NATIVE_UI_PRINCIPAL_NAME).unwrap();
        let lease = authorize_for_bus(&policy, &request, &principal, &[], 1_000).unwrap();

        assert_eq!(lease.principal(), &principal);
        assert_eq!(lease.capability().name(), "plugin.catalog.read");
        assert!(lease.is_active_at(1_000));
        assert!(!lease.is_active_at(31_000));
    }
}
