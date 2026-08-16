//! Tauri's single, transport-bound entry point into the Kernel Host Bus.
//!
//! The public request deliberately has no principal field. The command binds a
//! principal from the invoking webview before policy and bus admission.

use std::sync::RwLock;
use std::time::Instant;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State, WebviewWindow};

use crate::kernel::audit::{AuditLedger, AuditOutcome};
use crate::kernel::blob::{BlobError, BlobRef, BlobScope, BlobStore};
use crate::kernel::bus::{AdmissionRequest, BusError, BusPayload, HostBus, OperationDescriptor};
use crate::kernel::identity::{Capability, CapabilityLease, PrincipalId};
use crate::kernel::policy::{
    AuthorizationSource, CapabilityPolicy, PolicyError, NATIVE_UI_PRINCIPAL_NAME,
    PLUGIN_LIST_OPERATION,
};
use crate::kernel::rpc::{RequestId, RpcTarget};
use crate::kernel::service_registry::{ServiceDescriptor, ServiceMethodDescriptor, ServiceRegistry};
use crate::kernel::state::KernelState;

/// Named worker declaration for the in-process document agent pool.
pub const AGENT_HOST_WORKER_ID: &str = "worker.agent-host";
pub const AGENT_HOST_POOL: &str = "agent";

/// Dedicated principal for the in-process agent host so its workload pool
/// never consumes the native UI principal's quotas or capabilities.
pub const AGENT_HOST_PRINCIPAL: &str = "worker.agent-host";

pub const HOST_SDK_API_VERSION: &str = "anyway.dev/host-rpc/v1alpha1";
pub const MAX_HOST_DEADLINE_MS: u64 = 5 * 60 * 1_000;
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 64 * 1_024;
pub const MAX_BLOB_CHUNK_BYTES: usize = 16 * 1_024;
const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_OPERATION_BYTES: usize = 160;
const MAX_LEASE_IDS: usize = 32;
const MAX_LEASE_ID_BYTES: usize = 128;
const MAX_TRACE_PARENT_BYTES: usize = 256;
const MAX_BLOB_TEXT_BYTES: usize = 256;
const MAIN_WEBVIEW_LABEL: &str = "main";
const PLUGIN_LIST_MAX_INFLIGHT: usize = 8;
const PLUGIN_SETTINGS_READ_MAX_INFLIGHT: usize = 16;
const PLUGIN_SETTINGS_WRITE_MAX_INFLIGHT: usize = 8;
const PLUGIN_SETTINGS_RESET_MAX_INFLIGHT: usize = 8;
const WORKSPACE_FOLDER_LIST_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GIT_READ_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GITHUB_READ_MAX_INFLIGHT: usize = 8;
const ICON_THEME_READ_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_STATUS_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_LIST_MAX_INFLIGHT: usize = 8;
const AGENT_BATCH_STATUS_MAX_INFLIGHT: usize = 8;
const BLOB_WRITE_MAX_INFLIGHT: usize = 8;
const BLOB_READ_MAX_INFLIGHT: usize = 16;
const BLOB_UPLOAD_TTL_MS: u64 = 60_000;
const BLOB_READ_TTL_MS: u64 = 30_000;
const SERVICE_REGISTER_MAX_INFLIGHT: usize = 8;
const SERVICE_CALL_MAX_INFLIGHT: usize = 16;

/// Registry TTL applied to services registered through the Host Bus. Kept in
/// sync with the kernel registry's default so the example service expires
/// after the same window the register operation promises.
const SERVICE_TTL_MS: u64 = crate::kernel::service_registry::DEFAULT_TTL_MS;

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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlobWriteRequest {
    scope: String,
    media_type: String,
    content_base64: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlobReadRequest {
    r#ref: HostBlobRef,
    #[serde(default)]
    workspace: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceRegisterRequest {
    service: WireServiceDescriptor,
}

/// Wire mirror of the kernel `ServiceDescriptor` in camelCase.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireServiceDescriptor {
    service_id: String,
    version: String,
    display_name: String,
    #[serde(default)]
    methods: Vec<WireServiceMethodDescriptor>,
    #[serde(default)]
    required_capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireServiceMethodDescriptor {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceCallRequest {
    service_id: String,
    method: String,
    #[serde(default)]
    args: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginSettingsReadRequest {
    plugin_id: String,
    plugin_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginSettingsWriteRequest {
    plugin_id: String,
    plugin_version: String,
    values: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginSettingsResetRequest {
    plugin_id: String,
    plugin_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceFolderListRequest {
    plugin_id: String,
    plugin_version: String,
    root: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceGitReadRequest {
    plugin_id: String,
    plugin_version: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceGithubReadRequest {
    plugin_id: String,
    plugin_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IconThemeReadRequest {
    plugin_id: String,
    plugin_version: String,
    asset_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobStatusRequest {
    job_id: String,
}

/// Unit-style DTO: `agent.job.list` carries no required fields and still
/// deserializes an empty inline payload.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentJobListRequest {}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchStatusRequest {
    batch_id: String,
}

impl TryFrom<WireServiceDescriptor> for ServiceDescriptor {
    type Error = String;

    fn try_from(wire: WireServiceDescriptor) -> Result<Self, String> {
        let mut methods = Vec::with_capacity(wire.methods.len());
        for method in wire.methods {
            methods.push(
                ServiceMethodDescriptor::new(method.name, method.description)
                    .map_err(|error| format!("invalid service descriptor: {error}"))?,
            );
        }
        ServiceDescriptor::new(
            wire.service_id,
            wire.version,
            wire.display_name,
            methods,
            wire.required_capabilities,
        )
        .map_err(|error| format!("invalid service descriptor: {error}"))
    }
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
    register_operation(
        &mut bus,
        PLUGIN_LIST_OPERATION,
        RpcTarget::new("plugins", "list"),
        "plugin.catalog.read",
        PLUGIN_LIST_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "blob.write",
        RpcTarget::new("blob", "write"),
        "blob.write",
        BLOB_WRITE_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "blob.read",
        RpcTarget::new("blob", "read"),
        "blob.read",
        BLOB_READ_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "service.register",
        RpcTarget::new("ancordis", "register"),
        "service.register",
        SERVICE_REGISTER_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "service.call",
        RpcTarget::new("ancordis", "call"),
        "service.call",
        SERVICE_CALL_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.settings.read",
        RpcTarget::new("plugin", "settings.read"),
        "plugin.settings.read",
        PLUGIN_SETTINGS_READ_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.settings.write",
        RpcTarget::new("plugin", "settings.write"),
        "plugin.settings.write",
        PLUGIN_SETTINGS_WRITE_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.settings.reset",
        RpcTarget::new("plugin", "settings.reset"),
        "plugin.settings.reset",
        PLUGIN_SETTINGS_RESET_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.folder.list",
        RpcTarget::new("workspace", "folder.list"),
        "workspace.folder.list",
        WORKSPACE_FOLDER_LIST_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.git.read",
        RpcTarget::new("workspace", "git.read"),
        "workspace.git.read",
        WORKSPACE_GIT_READ_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.github.read",
        RpcTarget::new("workspace", "github.read"),
        "workspace.github.read",
        WORKSPACE_GITHUB_READ_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.icon-theme.read",
        RpcTarget::new("plugin", "icon-theme.read"),
        "plugin.icon-theme.read",
        ICON_THEME_READ_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "agent.job.status",
        RpcTarget::new("agent", "job.status"),
        "agent.job.status",
        AGENT_JOB_STATUS_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "agent.job.list",
        RpcTarget::new("agent", "job.list"),
        "agent.job.list",
        AGENT_JOB_LIST_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "agent.batch.status",
        RpcTarget::new("agent", "batch.status"),
        "agent.batch.status",
        AGENT_BATCH_STATUS_MAX_INFLIGHT,
    )?;
    let kernel = KernelState::with_bus(bus, 64);
    register_agent_worker(&kernel)?;
    register_example_service(&kernel)?;
    Ok(kernel)
}

/// Declare and start the in-process agent host as a supervised worker.
///
/// The adapter for a shared thread pool does not spawn a process; starting
/// means the pool is ready to accept queued jobs. The supervisor still
/// records the incarnation and receives health observations from the gate.
fn register_agent_worker(kernel: &KernelState) -> Result<(), String> {
    use crate::kernel::lifecycle::LifecycleSpec;
    use crate::kernel::supervisor::{WorkerObservation, WorkerSpec};

    let worker_id = crate::kernel::identity::WorkerId::new(AGENT_HOST_WORKER_ID)
        .map_err(|error| error.to_string())?;
    let principal = PrincipalId::new(AGENT_HOST_PRINCIPAL).map_err(|e| e.to_string())?;
    let mut supervisor = kernel.supervisor().write().map_err(|error| error.to_string())?;
    supervisor
        .register(WorkerSpec::thread_pool(
            worker_id.clone(),
            principal,
            None,
            AGENT_HOST_POOL,
            LifecycleSpec::default(),
        ))
        .map_err(|error| error.to_string())?;
    supervisor
        .start(&worker_id)
        .map_err(|error| error.to_string())?;
    supervisor
        .observe(&worker_id, WorkerObservation::Started)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Register the Phase 6 example service into the kernel services plane.
///
/// The AnCordis extension host slice needs one service that registers at
/// startup and can be called back through the Host Bus. The service expires
/// after the registry TTL, which the Host Bus exposes as `SERVICE_TTL_MS`.
fn register_example_service(kernel: &KernelState) -> Result<(), String> {
    let descriptor = ServiceDescriptor::new(
        "anyway.system.ping",
        "1.0.0",
        "Ping",
        vec![ServiceMethodDescriptor::new("ping", None).map_err(|error| error.to_string())?],
        Vec::new(),
    )
    .map_err(|error| error.to_string())?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let mut registry = kernel
        .services()
        .write()
        .map_err(|_| "service registry lock is poisoned".to_string())?;
    debug_assert_eq!(
        registry.config().ttl_ms,
        SERVICE_TTL_MS,
        "the example service must expire after the Host Bus service TTL"
    );
    registry
        .register(descriptor, now_ms)
        .map_err(|error| format!("failed to register the example service: {error}"))?;
    Ok(())
}

/// Build the transport gate that lets agent batches run against the kernel
/// scheduler and supervisor planes instead of a global serial mutex.
pub fn agent_gate_for(kernel: &KernelState) -> crate::agent_commands::AgentJobGate {
    use crate::kernel::scheduler::DEFAULT_PER_PRINCIPAL_QUOTA;

    crate::agent_commands::AgentJobGate::new(
        kernel.shared_schedulers(),
        kernel.shared_supervisor(),
        crate::kernel::identity::WorkerId::new(AGENT_HOST_WORKER_ID)
            .expect("agent worker id is valid"),
        PrincipalId::new(AGENT_HOST_PRINCIPAL).expect("agent host principal is valid"),
        DEFAULT_PER_PRINCIPAL_QUOTA,
    )
}

fn register_operation(
    bus: &mut HostBus,
    operation: &str,
    route: Result<RpcTarget, crate::kernel::rpc::RpcError>,
    capability: &str,
    max_inflight: usize,
) -> Result<(), String> {
    let route = route.map_err(|error| error.to_string())?;
    let capability = Capability::custom(capability).map_err(|error| error.to_string())?;
    let descriptor = OperationDescriptor::new(
        operation,
        route,
        capability,
        crate::kernel::identity::CapabilityScope::Global,
        max_inflight,
    )
    .map_err(|error| error.to_string())?;
    bus.register_operation(descriptor).map_err(|error| error.to_string())
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
    agent: State<'_, crate::agent_commands::AgentHostState>,
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
        Err(error) => {
            record_audit(
                kernel.audit(),
                &principal,
                &request,
                now_ms,
                AuditOutcome::Denied,
            );
            return policy_failure(response_request_id, error);
        }
    };

    // The admission builder takes ownership of the principal; keep a clone
    // for the audit event recorded after dispatch.
    let audit_principal = principal.clone();
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

    let handler_result = dispatch(&request, Some(app), &*agent, kernel.blobs(), kernel.services());
    let outcome = if handler_result.is_ok() {
        AuditOutcome::Completed
    } else {
        AuditOutcome::Failed
    };
    record_audit(kernel.audit(), &audit_principal, &request, now_ms, outcome);
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

fn dispatch(
    request: &HostCallRequest,
    app: Option<AppHandle>,
    agent: &crate::agent_commands::AgentHostState,
    blobs: &RwLock<BlobStore>,
    services: &RwLock<ServiceRegistry>,
) -> Result<Value, String> {
    match request.operation.as_str() {
        PLUGIN_LIST_OPERATION => {
            let app = app.ok_or("plugin.list requires an application handle")?;
            let plugins = crate::plugins::query_installed_plugins(&app)?;
            serde_json::to_value(plugins).map_err(|error| error.to_string())
        }
        "plugin.settings.read" => {
            let app = app.ok_or("plugin.settings.read requires an application handle")?;
            dispatch_plugin_settings_read(request, app)
        }
        "plugin.settings.write" => {
            let app = app.ok_or("plugin.settings.write requires an application handle")?;
            dispatch_plugin_settings_write(request, app)
        }
        "plugin.settings.reset" => {
            let app = app.ok_or("plugin.settings.reset requires an application handle")?;
            dispatch_plugin_settings_reset(request, app)
        }
        "plugin.icon-theme.read" => {
            let app = app.ok_or("plugin.icon-theme.read requires an application handle")?;
            dispatch_icon_theme_read(request, app)
        }
        "workspace.folder.list" => {
            let app = app.ok_or("workspace.folder.list requires an application handle")?;
            dispatch_workspace_folder_list(request, app)
        }
        "workspace.git.read" => {
            let app = app.ok_or("workspace.git.read requires an application handle")?;
            dispatch_workspace_git_read(request, app)
        }
        "workspace.github.read" => {
            let app = app.ok_or("workspace.github.read requires an application handle")?;
            dispatch_workspace_github_read(request, app)
        }
        "agent.job.status" => dispatch_agent_job_status(request, agent),
        "agent.job.list" => dispatch_agent_job_list(request, agent),
        "agent.batch.status" => dispatch_agent_batch_status(request, agent),
        "blob.write" => dispatch_blob_write(request, blobs),
        "blob.read" => dispatch_blob_read(request, blobs),
        "service.register" => dispatch_service_register(request, services),
        "service.call" => dispatch_service_call(request, services),
        _ => Err("operation has no registered kernel handler".to_string()),
    }
}

fn dispatch_plugin_settings_read(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let read_request = inline_request::<PluginSettingsReadRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.settings.read request: {error}")))?;
    let snapshot = crate::plugins::get_plugin_settings(
        app,
        read_request.plugin_id,
        read_request.plugin_version,
    )?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
}

fn dispatch_plugin_settings_write(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let write_request = inline_request::<PluginSettingsWriteRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.settings.write request: {error}")))?;
    let snapshot = crate::plugins::set_plugin_settings(
        app,
        write_request.plugin_id,
        write_request.plugin_version,
        write_request.values,
    )?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
}

fn dispatch_plugin_settings_reset(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let reset_request = inline_request::<PluginSettingsResetRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.settings.reset request: {error}")))?;
    let snapshot = crate::plugins::reset_plugin_settings(
        app,
        reset_request.plugin_id,
        reset_request.plugin_version,
    )?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
}

fn dispatch_icon_theme_read(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let read_request = inline_request::<IconThemeReadRequest>(request).or_else(|error| {
        Err(format!("invalid plugin.icon-theme.read request: {error}"))
    })?;
    let data_url = crate::plugins::read_icon_theme_asset(
        app,
        read_request.plugin_id,
        read_request.plugin_version,
        read_request.asset_path,
    )?;
    serde_json::to_value(data_url).map_err(|error| error.to_string())
}

fn dispatch_agent_job_status(
    request: &HostCallRequest,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    let status_request = inline_request::<JobStatusRequest>(request)
        .or_else(|error| Err(format!("invalid agent.job.status request: {error}")))?;
    let host = agent
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    let job = host
        .get_job(&status_request.job_id)
        .ok_or_else(|| format!("Job not found: {}", status_request.job_id))?;
    serde_json::to_value(crate::agent_commands::PdfJobStatus::from(job))
        .map_err(|error| error.to_string())
}

fn dispatch_agent_job_list(
    request: &HostCallRequest,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    inline_request::<AgentJobListRequest>(request)
        .or_else(|error| Err(format!("invalid agent.job.list request: {error}")))?;
    let host = agent
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    let mut jobs = host
        .list_jobs()
        .into_iter()
        .map(crate::agent_commands::PdfJobStatus::from)
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    serde_json::to_value(jobs).map_err(|error| error.to_string())
}

fn dispatch_agent_batch_status(
    request: &HostCallRequest,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    let status_request = inline_request::<BatchStatusRequest>(request)
        .or_else(|error| Err(format!("invalid agent.batch.status request: {error}")))?;
    let host = agent
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    let batch = host
        .get_batch(&status_request.batch_id)
        .ok_or_else(|| format!("Batch not found: {}", status_request.batch_id))?;
    let status = crate::agent_commands::import_batch_status(&host, batch)?;
    serde_json::to_value(status).map_err(|error| error.to_string())
}

fn dispatch_workspace_folder_list(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let list_request = inline_request::<WorkspaceFolderListRequest>(request).or_else(|error| {
        Err(format!("invalid workspace.folder.list request: {error}"))
    })?;
    let entries = crate::workspace_host::list_folder_entries(
        app,
        list_request.plugin_id,
        list_request.plugin_version,
        list_request.root,
        list_request.path,
    )?;
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

fn dispatch_workspace_git_read(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let read_request = inline_request::<WorkspaceGitReadRequest>(request)
        .or_else(|error| Err(format!("invalid workspace.git.read request: {error}")))?;
    let snapshot = crate::workspace_host::read_git_workspace(
        app,
        read_request.plugin_id,
        read_request.plugin_version,
        read_request.path,
    )?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
}

fn dispatch_workspace_github_read(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let read_request = inline_request::<WorkspaceGithubReadRequest>(request)
        .or_else(|error| Err(format!("invalid workspace.github.read request: {error}")))?;
    let status = crate::workspace_host::read_github_account(
        app,
        read_request.plugin_id,
        read_request.plugin_version,
    )?;
    serde_json::to_value(status).map_err(|error| error.to_string())
}

fn dispatch_blob_write(request: &HostCallRequest, blobs: &RwLock<BlobStore>) -> Result<Value, String> {
    let write_request = inline_request::<BlobWriteRequest>(request).or_else(|error| {
        Err(format!("invalid blob.write request: {error}"))
    })?;
    let content = base64::engine::general_purpose::STANDARD
        .decode(&write_request.content_base64)
        .map_err(|_| "blob content must be base64".to_string())?;
    if content.len() > MAX_BLOB_CHUNK_BYTES {
        return Err(format!(
            "blob.content exceeds the {} byte chunk limit",
            MAX_BLOB_CHUNK_BYTES
        ));
    }
    let scope = BlobScope::from_wire(&write_request.scope)
        .map_err(|error| format!("invalid blob scope: {error}"))?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let owner = NATIVE_UI_PRINCIPAL_NAME;
    let mut store = blobs
        .write()
        .map_err(|_| "blob store lock is poisoned".to_string())?;
    let lease = store
        .begin_upload(
            owner,
            scope,
            write_request.media_type.clone(),
            content.len() as u64,
            now_ms,
            BLOB_UPLOAD_TTL_MS,
        )
        .map_err(|error| blob_error_message("begin upload", &error))?;
    store
        .upload_chunk(lease, owner, &content, now_ms)
        .map_err(|error| blob_error_message("upload chunk", &error))?;
    let reference = store
        .commit_upload(lease, owner, now_ms)
        .map_err(|error| blob_error_message("commit upload", &error))?;
    Ok(blob_ref_to_json(&reference, owner))
}

fn dispatch_blob_read(request: &HostCallRequest, blobs: &RwLock<BlobStore>) -> Result<Value, String> {
    let read_request = inline_request::<BlobReadRequest>(request).or_else(|error| {
        Err(format!("invalid blob.read request: {error}"))
    })?;
    let reference = host_blob_ref_to_kernel(&read_request.r#ref)?;
    let workspace = read_request.workspace.as_deref();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let owner = NATIVE_UI_PRINCIPAL_NAME;
    let mut store = blobs
        .write()
        .map_err(|_| "blob store lock is poisoned".to_string())?;
    if let Some(workspace) = workspace {
        validate_token_text(workspace, "blob.read workspace")?;
    }
    let read_lease = store
        .open_read(&reference, owner, workspace, now_ms, BLOB_READ_TTL_MS)
        .map_err(|error| blob_error_message("open read", &error))?;
    let chunk = store
        .read_chunk(read_lease, owner, 0, MAX_BLOB_CHUNK_BYTES, now_ms)
        .map_err(|error| blob_error_message("read chunk", &error))?;
    store
        .close_read(read_lease, owner)
        .map_err(|error| blob_error_message("close read", &error))?;
    Ok(json!({
        "digest": reference.digest().to_hex(),
        "size": reference.size(),
        "mediaType": reference.media_type(),
        "contentBase64": base64::engine::general_purpose::STANDARD.encode(chunk),
    }))
}

fn dispatch_service_register(
    request: &HostCallRequest,
    services: &RwLock<ServiceRegistry>,
) -> Result<Value, String> {
    let register_request = inline_request::<ServiceRegisterRequest>(request)
        .or_else(|error| Err(format!("invalid service.register request: {error}")))?;
    let descriptor = ServiceDescriptor::try_from(register_request.service)?;
    let service_id = descriptor.service_id.clone();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let mut registry = services
        .write()
        .map_err(|_| "service registry lock is poisoned".to_string())?;
    registry
        .register(descriptor, now_ms)
        .map_err(|error| format!("service.register failed: {error}"))?;
    Ok(json!(service_id))
}

fn dispatch_service_call(
    request: &HostCallRequest,
    services: &RwLock<ServiceRegistry>,
) -> Result<Value, String> {
    let call_request = inline_request::<ServiceCallRequest>(request)
        .or_else(|error| Err(format!("invalid service.call request: {error}")))?;
    let args = call_request.args.unwrap_or(Value::Null);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let registry = services
        .read()
        .map_err(|_| "service registry lock is poisoned".to_string())?;
    registry
        .call(&call_request.service_id, &call_request.method, args, now_ms)
        .map_err(|error| format!("service.call failed: {error}"))
}

fn inline_request<T: for<'de> serde::Deserialize<'de>>(
    request: &HostCallRequest,
) -> Result<T, String> {
    match &request.payload {
        HostPayload::Inline { value } => {
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())
        }
        HostPayload::Blob { .. } => Err("operation expects an inline payload".to_string()),
    }
}

fn host_blob_ref_to_kernel(r#ref: &HostBlobRef) -> Result<BlobRef, String> {
    let digest_bytes = hex_decode(&r#ref.digest).ok_or("blob digest must be lowercase hex")?;
    let digest =
        <[u8; 32]>::try_from(digest_bytes.as_slice()).map_err(|_| "blob digest must be 32 bytes")?;
    let scope = BlobScope::from_wire(&r#ref.scope).map_err(|error| error.to_string())?;
    BlobRef::new(
        crate::kernel::blob::BlobDigest::new(digest),
        r#ref.size,
        r#ref.media_type.clone(),
        scope,
    )
    .map_err(|error| error.to_string())
}

fn blob_ref_to_json(reference: &BlobRef, owner: &str) -> Value {
    json!({
        "algorithm": "sha256",
        "digest": reference.digest().to_hex(),
        "size": reference.size(),
        "mediaType": reference.media_type(),
        "scope": reference.scope().to_wire(),
        "owner": owner,
        "retentionClass": "session",
    })
}

fn blob_error_message(stage: &str, error: &BlobError) -> String {
    format!("blob {stage} failed: {error}")
}

fn validate_token_text(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_BLOB_TEXT_BYTES || value.chars().any(char::is_control) {
        return Err(format!("invalid {kind}"));
    }
    Ok(())
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
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

/// Record one Host SDK call outcome in the kernel audit ledger.
///
/// The ledger plane is synchronous and best-effort: take the write lock,
/// append the event, drop the guard. Auditing must never change the gateway
/// response shape or error codes, so a poisoned ledger lock is swallowed
/// rather than surfaced to the caller.
fn record_audit(
    ledger: &RwLock<AuditLedger>,
    principal: &PrincipalId,
    request: &HostCallRequest,
    now_ms: u64,
    outcome: AuditOutcome,
) {
    if let Ok(mut ledger) = ledger.write() {
        ledger.record(
            principal.clone(),
            request.operation.clone(),
            request.trace_parent.clone(),
            now_ms,
            outcome,
        );
    }
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
    fn default_kernel_state_registers_the_plugin_settings_read_route() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let operation = crate::kernel::bus::OperationId::new("plugin.settings.read").unwrap();
        let descriptor = bus.operation(&operation).expect("registered operation");
        assert_eq!(descriptor.route().service(), "plugin");
        assert_eq!(descriptor.route().method(), "settings.read");
        assert_eq!(
            descriptor.required_capability().name(),
            "plugin.settings.read"
        );
    }

    #[test]
    fn plugin_settings_read_dto_parses_identity_and_rejects_unknown_fields() {
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "settings-read-1",
            "operation": "plugin.settings.read",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(request.validate(), Ok(()));

        let dto: PluginSettingsReadRequest =
            inline_request(&request).expect("plugin settings read DTO deserializes");
        assert_eq!(dto.plugin_id, "myc.onedarkpro");
        assert_eq!(dto.plugin_version, "1.3.0");

        let unknown_field = json!({
            "pluginId": "myc.onedarkpro",
            "pluginVersion": "1.3.0",
            "sneaky": true
        });
        assert!(
            serde_json::from_value::<PluginSettingsReadRequest>(unknown_field).is_err(),
            "the DTO must reject unknown fields"
        );

        let missing_version = json!({ "pluginId": "myc.onedarkpro" });
        assert!(
            serde_json::from_value::<PluginSettingsReadRequest>(missing_version).is_err(),
            "the DTO must require the plugin version"
        );
    }

    #[test]
    fn default_kernel_state_registers_plugin_settings_write_and_reset_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("plugin.settings.write", "plugin", "settings.write"),
            ("plugin.settings.reset", "plugin", "settings.reset"),
        ];
        for (operation, service, method) in expectations {
            let id = crate::kernel::bus::OperationId::new(operation).unwrap();
            let descriptor = bus.operation(&id).expect("registered operation");
            assert_eq!(descriptor.route().service(), service);
            assert_eq!(descriptor.route().method(), method);
            assert_eq!(descriptor.required_capability().name(), operation);
        }
    }

    #[test]
    fn plugin_settings_write_and_reset_dtos_parse_and_reject_unknown_fields() {
        let write_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "settings-write-1",
            "operation": "plugin.settings.write",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "values": {
                        "theme": "dark",
                        "retries": 3
                    }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(write_request.validate(), Ok(()));

        let write: PluginSettingsWriteRequest =
            inline_request(&write_request).expect("plugin settings write DTO deserializes");
        assert_eq!(write.plugin_id, "myc.onedarkpro");
        assert_eq!(write.plugin_version, "1.3.0");
        assert_eq!(write.values.get("theme").and_then(Value::as_str), Some("dark"));
        assert_eq!(write.values.get("retries").and_then(Value::as_u64), Some(3));
        assert_eq!(write.values.len(), 2);

        assert!(
            serde_json::from_value::<PluginSettingsWriteRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "values": {},
                "sneaky": true
            }))
            .is_err(),
            "the write DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<PluginSettingsWriteRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0"
            }))
            .is_err(),
            "the write DTO must require the values map"
        );

        let reset_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "settings-reset-1",
            "operation": "plugin.settings.reset",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(reset_request.validate(), Ok(()));

        let reset: PluginSettingsResetRequest =
            inline_request(&reset_request).expect("plugin settings reset DTO deserializes");
        assert_eq!(reset.plugin_id, "myc.onedarkpro");
        assert_eq!(reset.plugin_version, "1.3.0");

        assert!(
            serde_json::from_value::<PluginSettingsResetRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "sneaky": true
            }))
            .is_err(),
            "the reset DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<PluginSettingsResetRequest>(json!({
                "pluginId": "myc.onedarkpro"
            }))
            .is_err(),
            "the reset DTO must require the plugin version"
        );
    }

    #[test]
    fn default_kernel_state_registers_workspace_read_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("workspace.folder.list", "workspace", "folder.list"),
            ("workspace.git.read", "workspace", "git.read"),
            ("workspace.github.read", "workspace", "github.read"),
        ];
        for (operation, service, method) in expectations {
            let id = crate::kernel::bus::OperationId::new(operation).unwrap();
            let descriptor = bus.operation(&id).expect("registered operation");
            assert_eq!(descriptor.route().service(), service);
            assert_eq!(descriptor.route().method(), method);
            assert_eq!(descriptor.required_capability().name(), operation);
        }
    }

    #[test]
    fn workspace_dtos_parse_identity_and_reject_unknown_fields() {
        let folder_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "folder-list-1",
            "operation": "workspace.folder.list",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "root": "/workspace",
                    "path": "src"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let folder: WorkspaceFolderListRequest =
            inline_request(&folder_request).expect("folder list DTO deserializes");
        assert_eq!(folder.plugin_id, "myc.onedarkpro");
        assert_eq!(folder.plugin_version, "1.3.0");
        assert_eq!(folder.root, "/workspace");
        assert_eq!(folder.path, "src");
        assert!(
            serde_json::from_value::<WorkspaceFolderListRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "root": "/workspace",
                "path": "src",
                "capability": "project.folder"
            }))
            .is_err(),
            "the folder list DTO must reject the legacy capability field"
        );

        let git_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "git-read-1",
            "operation": "workspace.git.read",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "path": "/workspace"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let git: WorkspaceGitReadRequest =
            inline_request(&git_request).expect("git read DTO deserializes");
        assert_eq!(git.plugin_id, "myc.onedarkpro");
        assert_eq!(git.plugin_version, "1.3.0");
        assert_eq!(git.path, "/workspace");
        assert!(
            serde_json::from_value::<WorkspaceGitReadRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "path": "/workspace",
                "capability": "git.repository.read"
            }))
            .is_err(),
            "the git read DTO must reject the legacy capability field"
        );

        let github_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "github-read-1",
            "operation": "workspace.github.read",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let github: WorkspaceGithubReadRequest =
            inline_request(&github_request).expect("github read DTO deserializes");
        assert_eq!(github.plugin_id, "myc.onedarkpro");
        assert_eq!(github.plugin_version, "1.3.0");
        assert!(
            serde_json::from_value::<WorkspaceGithubReadRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "capability": "git.account.read"
            }))
            .is_err(),
            "the github read DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<WorkspaceGithubReadRequest>(json!({
                "pluginId": "myc.onedarkpro"
            }))
            .is_err(),
            "the github read DTO must require the plugin version"
        );
    }

    #[test]
    fn default_kernel_state_registers_agent_and_icon_theme_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("plugin.icon-theme.read", "plugin", "icon-theme.read"),
            ("agent.job.status", "agent", "job.status"),
            ("agent.job.list", "agent", "job.list"),
            ("agent.batch.status", "agent", "batch.status"),
        ];
        for (operation, service, method) in expectations {
            let id = crate::kernel::bus::OperationId::new(operation).unwrap();
            let descriptor = bus.operation(&id).expect("registered operation");
            assert_eq!(descriptor.route().service(), service);
            assert_eq!(descriptor.route().method(), method);
            assert_eq!(descriptor.required_capability().name(), operation);
        }
    }

    #[test]
    fn agent_and_icon_theme_dtos_parse_identity_and_reject_unknown_fields() {
        let icon_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "icon-read-1",
            "operation": "plugin.icon-theme.read",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "assetPath": "icons/theme.json"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let icon: IconThemeReadRequest =
            inline_request(&icon_request).expect("icon theme read DTO deserializes");
        assert_eq!(icon.plugin_id, "myc.onedarkpro");
        assert_eq!(icon.plugin_version, "1.3.0");
        assert_eq!(icon.asset_path, "icons/theme.json");
        assert!(
            serde_json::from_value::<IconThemeReadRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "assetPath": "icons/theme.json",
                "capability": "plugin.icon-theme.read"
            }))
            .is_err(),
            "the icon theme read DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<IconThemeReadRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0"
            }))
            .is_err(),
            "the icon theme read DTO must require the asset path"
        );

        let job_status_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "job-status-1",
            "operation": "agent.job.status",
            "payload": {
                "kind": "inline",
                "value": { "jobId": "job-1" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let job_status: JobStatusRequest =
            inline_request(&job_status_request).expect("job status DTO deserializes");
        assert_eq!(job_status.job_id, "job-1");
        assert!(
            serde_json::from_value::<JobStatusRequest>(json!({
                "jobId": "job-1",
                "sneaky": true
            }))
            .is_err(),
            "the job status DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<JobStatusRequest>(json!({})).is_err(),
            "the job status DTO must require the job id"
        );

        let job_list_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "job-list-1",
            "operation": "agent.job.list",
            "payload": {
                "kind": "inline",
                "value": {}
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        inline_request::<AgentJobListRequest>(&job_list_request)
            .expect("job list DTO deserializes an empty payload");
        assert!(
            serde_json::from_value::<AgentJobListRequest>(json!({
                "jobId": "job-1"
            }))
            .is_err(),
            "the job list DTO must reject unknown fields"
        );

        let batch_status_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "batch-status-1",
            "operation": "agent.batch.status",
            "payload": {
                "kind": "inline",
                "value": { "batchId": "batch-1" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let batch_status: BatchStatusRequest =
            inline_request(&batch_status_request).expect("batch status DTO deserializes");
        assert_eq!(batch_status.batch_id, "batch-1");
        assert!(
            serde_json::from_value::<BatchStatusRequest>(json!({
                "batchId": "batch-1",
                "sneaky": true
            }))
            .is_err(),
            "the batch status DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<BatchStatusRequest>(json!({})).is_err(),
            "the batch status DTO must require the batch id"
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

    #[test]
    fn default_kernel_state_registers_blob_operations() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        for operation in ["blob.write", "blob.read"] {
            let id = crate::kernel::bus::OperationId::new(operation).unwrap();
            let descriptor = bus.operation(&id).expect("registered operation");
            assert_eq!(descriptor.route().service(), "blob");
            assert_eq!(descriptor.required_capability().name(), operation);
        }
    }

    #[test]
    fn blob_write_and_read_round_trip_through_the_store() {
        let state = create_kernel_state().expect("kernel state");

        let write_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-write-1",
            "operation": "blob.write",
            "payload": {
                "kind": "inline",
                "value": {
                    "scope": "shared",
                    "mediaType": "text/plain",
                    "contentBase64": "YW55d2F5IGJsb2IgY29udGVudA=="
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let written = dispatch_blob_write(&write_request, state.blobs())
            .expect("blob.write succeeded");
        assert_eq!(written["algorithm"], "sha256");
        assert_eq!(written["size"], 19);
        assert_eq!(written["owner"], NATIVE_UI_PRINCIPAL_NAME);
        assert_eq!(written["scope"], "shared");

        let read_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-read-1",
            "operation": "blob.read",
            "payload": {
                "kind": "inline",
                "value": {
                    "ref": written,
                    "workspace": null
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let read = dispatch_blob_read(&read_request, state.blobs()).expect("blob.read succeeded");
        assert_eq!(read["size"], 19);
        assert_eq!(
            read["contentBase64"],
            "YW55d2F5IGJsb2IgY29udGVudA=="
        );
        assert_eq!(read["digest"], written["digest"]);
    }

    #[test]
    fn blob_write_rejects_oversized_and_malformed_content() {
        let state = create_kernel_state().expect("kernel state");

        let oversized: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-write-2",
            "operation": "blob.write",
            "payload": {
                "kind": "inline",
                "value": {
                    "scope": "shared",
                    "mediaType": "text/plain",
                    "contentBase64": "A".repeat(MAX_BLOB_CHUNK_BYTES * 2)
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert!(dispatch_blob_write(&oversized, state.blobs()).is_err());

        let not_base64: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-write-3",
            "operation": "blob.write",
            "payload": {
                "kind": "inline",
                "value": {
                    "scope": "shared",
                    "mediaType": "text/plain",
                    "contentBase64": "!!!not-base64!!!"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert!(dispatch_blob_write(&not_base64, state.blobs()).is_err());
    }

    #[test]
    fn service_register_and_call_route_through_the_registry() {
        let state = create_kernel_state().expect("kernel state");

        let register_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "service-register-1",
            "operation": "service.register",
            "payload": {
                "kind": "inline",
                "value": {
                    "service": {
                        "serviceId": "anyway.system.echo",
                        "version": "1.0.0",
                        "displayName": "Echo",
                        "methods": [{ "name": "echo" }],
                        "requiredCapabilities": []
                    }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let registered = dispatch_service_register(&register_request, state.services())
            .expect("service.register succeeded");
        assert_eq!(registered, json!("anyway.system.echo"));

        let call_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "service-call-1",
            "operation": "service.call",
            "payload": {
                "kind": "inline",
                "value": {
                    "serviceId": "anyway.system.echo",
                    "method": "echo",
                    "args": { "echo": true }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let result = dispatch_service_call(&call_request, state.services())
            .expect("service.call succeeded");
        assert_eq!(result["serviceId"], "anyway.system.echo");
        assert_eq!(result["method"], "echo");
        assert_eq!(result["args"], json!({ "echo": true }));
    }

    #[test]
    fn example_service_is_registered_at_startup_and_can_be_called() {
        let state = create_kernel_state().expect("kernel state");
        let call_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "service-call-ping",
            "operation": "service.call",
            "payload": {
                "kind": "inline",
                "value": {
                    "serviceId": "anyway.system.ping",
                    "method": "ping",
                    "args": { "probe": 1 }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let result = dispatch_service_call(&call_request, state.services())
            .expect("example service call succeeded");
        assert_eq!(result["serviceId"], "anyway.system.ping");
        assert_eq!(result["method"], "ping");
        assert_eq!(result["args"], json!({ "probe": 1 }));
    }

    #[test]
    fn successful_dispatch_records_a_completed_audit_event() {
        let state = create_kernel_state().expect("kernel state");
        let call_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "audit-call-1",
            "operation": "service.call",
            "payload": {
                "kind": "inline",
                "value": {
                    "serviceId": "anyway.system.ping",
                    "method": "ping",
                    "args": {}
                }
            },
            "deadlineMs": 30_000,
            "traceParent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        }))
        .unwrap();
        let principal = PrincipalId::new(NATIVE_UI_PRINCIPAL_NAME).unwrap();

        // The gateway path after authorization: the service.call route
        // dispatches through `dispatch_service_call`, then records. The route
        // helper is called directly (like the other handler tests) so the test
        // binary never links the Tauri GUI dispatch arms.
        let result = dispatch_service_call(&call_request, state.services())
            .expect("example service call succeeded");
        assert_eq!(result["serviceId"], "anyway.system.ping");
        assert_eq!(result["method"], "ping");
        record_audit(state.audit(), &principal, &call_request, 42, AuditOutcome::Completed);

        let ledger = state.audit().read().expect("audit read lock");
        assert_eq!(ledger.len(), 1);
        let events = ledger.query(1, 10);
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(event.sequence >= 1, "sequence must be assigned from 1");
        assert_eq!(event.sequence, 1);
        assert_eq!(event.principal, principal);
        assert_eq!(event.operation, "service.call");
        assert_eq!(
            event.trace_parent.as_deref(),
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
            "the gateway must persist the validated trace parent"
        );
        assert_eq!(event.timestamp_ms, 42);
        assert_eq!(event.outcome, AuditOutcome::Completed);
    }

    #[test]
    fn host_bus_services_expire_after_the_service_ttl() {
        use crate::kernel::service_registry::{
            ServiceDescriptor as KernelServiceDescriptor, ServiceMethodDescriptor, ServiceRegistry,
            ServiceRegistryError,
        };

        let mut registry = ServiceRegistry::new();
        let descriptor = KernelServiceDescriptor::new(
            "anyway.system.ping",
            "1.0.0",
            "Ping",
            vec![ServiceMethodDescriptor::new("ping", None).expect("method")],
            Vec::new(),
        )
        .expect("descriptor");
        registry.register(descriptor, 1_000).expect("registers");
        registry
            .call("anyway.system.ping", "ping", Value::Null, 1_000 + SERVICE_TTL_MS - 1)
            .expect("still live just before the TTL");
        assert!(matches!(
            registry.call(
                "anyway.system.ping",
                "ping",
                Value::Null,
                1_000 + SERVICE_TTL_MS
            ),
            Err(ServiceRegistryError::Expired { .. })
        ));
    }
}
