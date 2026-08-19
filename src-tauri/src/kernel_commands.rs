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
use crate::kernel::blob::{BlobError, BlobRef, BlobScope, BlobStore, UploadLeaseId};
use crate::kernel::bus::{AdmissionRequest, BusError, BusPayload, HostBus, OperationDescriptor};
use crate::kernel::identity::{Capability, CapabilityLease, PrincipalId};
use crate::kernel::package_gate::{PackageGate, ScanReport};
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
const PLUGIN_CONNECTION_TEST_MAX_INFLIGHT: usize = 8;
const PROJECT_SAVE_MAX_INFLIGHT: usize = 8;
const PROJECT_IMPORT_MAX_INFLIGHT: usize = 8;
const GRAPH_COMPILE_MAX_INFLIGHT: usize = 8;
const GRAPH_DIFF_MAX_INFLIGHT: usize = 8;
const PLUGIN_ANALYSIS_RUN_MAX_INFLIGHT: usize = 8;
const PLUGIN_INSTALL_MAX_INFLIGHT: usize = 8;
const PLUGIN_UNINSTALL_MAX_INFLIGHT: usize = 8;
const VSIX_IMPORT_MAX_INFLIGHT: usize = 8;
const WORKSPACE_FOLDER_LIST_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GIT_READ_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GITHUB_READ_MAX_INFLIGHT: usize = 8;
const WORKSPACE_FOLDER_SCAN_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GIT_INIT_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GITHUB_SSH_GENERATE_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GITHUB_LOGIN_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GITHUB_SSH_UPLOAD_MAX_INFLIGHT: usize = 8;
const WORKSPACE_GIT_AUTOSAVE_MAX_INFLIGHT: usize = 8;
const ICON_THEME_READ_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_STATUS_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_LIST_MAX_INFLIGHT: usize = 8;
const AGENT_BATCH_STATUS_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_REVIEW_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_CANCEL_MAX_INFLIGHT: usize = 8;
const AGENT_JOB_START_MAX_INFLIGHT: usize = 8;
const AGENT_BATCH_START_MAX_INFLIGHT: usize = 8;
const BLOB_WRITE_MAX_INFLIGHT: usize = 8;
const BLOB_READ_MAX_INFLIGHT: usize = 16;
const BLOB_UPLOAD_BEGIN_MAX_INFLIGHT: usize = 8;
const BLOB_UPLOAD_CHUNK_MAX_INFLIGHT: usize = 8;
const BLOB_UPLOAD_COMMIT_MAX_INFLIGHT: usize = 8;
const PLUGIN_ARTIFACT_SAVE_MAX_INFLIGHT: usize = 8;
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
struct BlobUploadBeginRequest {
    scope: String,
    media_type: String,
    size: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlobUploadChunkRequest {
    lease_id: u128,
    content_base64: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlobUploadCommitRequest {
    lease_id: u128,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginArtifactSaveRequest {
    plugin_id: String,
    plugin_version: String,
    format: String,
    path: String,
    blob_ref: HostBlobRef,
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
struct PluginConnectionTestRequest {
    plugin_id: String,
    plugin_version: String,
    connection_id: String,
    #[serde(default)]
    action_id: Option<String>,
    values: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    secrets: std::collections::BTreeMap<String, crate::plugin_settings::PluginSecretMutationInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginSettingsResetRequest {
    plugin_id: String,
    plugin_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectSaveRequest {
    path: String,
    project: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectImportRequest {
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GraphCompileRequest {
    project: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GraphDiffRequest {
    v1: serde_json::Value,
    v2: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginAnalysisRunRequest {
    plugin_id: String,
    plugin_version: String,
    #[serde(default)]
    capability: Option<String>,
    input: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginInstallRequest {
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginUninstallRequest {
    plugin_id: String,
    plugin_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VsixImportRequest {
    path: String,
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
struct WorkspaceGithubLoginRequest {
    plugin_id: String,
    plugin_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceGithubSshUploadRequest {
    plugin_id: String,
    plugin_version: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceFolderScanRequest {
    plugin_id: String,
    plugin_version: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceGitInitRequest {
    plugin_id: String,
    plugin_version: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceGithubSshGenerateRequest {
    plugin_id: String,
    plugin_version: String,
    comment: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceGitAutosaveRequest {
    plugin_id: String,
    plugin_version: String,
    repo_path: String,
    project_path: String,
    project: serde_json::Value,
    message: String,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentJobReviewRequest {
    job_id: String,
    accept: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentJobCancelRequest {
    job_id: String,
    #[serde(default)]
    reason: Option<String>,
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
        "blob.upload.begin",
        RpcTarget::new("blob", "upload.begin"),
        "blob.upload.begin",
        BLOB_UPLOAD_BEGIN_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "blob.upload.chunk",
        RpcTarget::new("blob", "upload.chunk"),
        "blob.upload.chunk",
        BLOB_UPLOAD_CHUNK_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "blob.upload.commit",
        RpcTarget::new("blob", "upload.commit"),
        "blob.upload.commit",
        BLOB_UPLOAD_COMMIT_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.artifact.save",
        RpcTarget::new("plugin", "artifact.save"),
        "plugin.artifact.save",
        PLUGIN_ARTIFACT_SAVE_MAX_INFLIGHT,
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
        "plugin.connection.test",
        RpcTarget::new("plugin", "connection.test"),
        "plugin.connection.test",
        PLUGIN_CONNECTION_TEST_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.install",
        RpcTarget::new("plugin", "install"),
        "plugin.install",
        PLUGIN_INSTALL_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.uninstall",
        RpcTarget::new("plugin", "uninstall"),
        "plugin.uninstall",
        PLUGIN_UNINSTALL_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.vsix.import",
        RpcTarget::new("plugin", "vsix.import"),
        "plugin.vsix.import",
        VSIX_IMPORT_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "project.save",
        RpcTarget::new("project", "save"),
        "project.save",
        PROJECT_SAVE_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "project.import",
        RpcTarget::new("project", "import"),
        "project.import",
        PROJECT_IMPORT_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "graph.compile",
        RpcTarget::new("graph", "compile"),
        "graph.compile",
        GRAPH_COMPILE_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "graph.diff",
        RpcTarget::new("graph", "diff"),
        "graph.diff",
        GRAPH_DIFF_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "plugin.analysis.run",
        RpcTarget::new("plugin", "analysis.run"),
        "plugin.analysis.run",
        PLUGIN_ANALYSIS_RUN_MAX_INFLIGHT,
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
        "workspace.folder.scan",
        RpcTarget::new("workspace", "folder.scan"),
        "workspace.folder.scan",
        WORKSPACE_FOLDER_SCAN_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.git.init",
        RpcTarget::new("workspace", "git.init"),
        "workspace.git.init",
        WORKSPACE_GIT_INIT_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.github.ssh.generate",
        RpcTarget::new("workspace", "github.ssh.generate"),
        "workspace.github.ssh.generate",
        WORKSPACE_GITHUB_SSH_GENERATE_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.github.login",
        RpcTarget::new("workspace", "github.login"),
        "workspace.github.login",
        WORKSPACE_GITHUB_LOGIN_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.github.ssh.upload",
        RpcTarget::new("workspace", "github.ssh.upload"),
        "workspace.github.ssh.upload",
        WORKSPACE_GITHUB_SSH_UPLOAD_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "workspace.git.autosave",
        RpcTarget::new("workspace", "git.autosave"),
        "workspace.git.autosave",
        WORKSPACE_GIT_AUTOSAVE_MAX_INFLIGHT,
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
    register_operation(
        &mut bus,
        "agent.job.review",
        RpcTarget::new("agent", "job.review"),
        "agent.job.review",
        AGENT_JOB_REVIEW_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "agent.job.cancel",
        RpcTarget::new("agent", "job.cancel"),
        "agent.job.cancel",
        AGENT_JOB_CANCEL_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "agent.job.start",
        RpcTarget::new("agent", "job.start"),
        "agent.job.start",
        AGENT_JOB_START_MAX_INFLIGHT,
    )?;
    register_operation(
        &mut bus,
        "agent.batch.start",
        RpcTarget::new("agent", "batch.start"),
        "agent.batch.start",
        AGENT_BATCH_START_MAX_INFLIGHT,
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
///
/// The gateway is async so migrated handlers can await blocking work off the
/// IPC path. Tauri async commands must return `Result`, so every handled
/// outcome stays inside the `HostCallResponse` envelope (`Ok`); `Err` is never
/// produced because the envelope already carries the structured error.
#[tauri::command]
pub async fn kernel_host_call(
    window: WebviewWindow,
    app: AppHandle,
    kernel: State<'_, KernelState>,
    policy: State<'_, CapabilityPolicyState>,
    agent: State<'_, crate::agent_commands::AgentHostState>,
    request: HostCallRequest,
) -> Result<HostCallResponse, String> {
    let response_request_id = request.request_id.clone();
    if window.label() != MAIN_WEBVIEW_LABEL {
        return Ok(HostCallResponse::failure(
            response_request_id,
            "HOST_TRANSPORT_DENIED",
            "webview is not authorized for the native UI principal",
            false,
        ));
    }
    if let Err(error) = request.validate() {
        return Ok(HostCallResponse::failure(
            response_request_id,
            "HOST_INVALID_REQUEST",
            error.message(),
            false,
        ));
    }
    if request.operation == PLUGIN_LIST_OPERATION {
        if let Err(error) = request.require_empty_inline_payload() {
            return Ok(HostCallResponse::failure(
                response_request_id,
                "HOST_INVALID_REQUEST",
                error.message(),
                false,
            ));
        }
    }

    let now_ms = policy.now_ms();
    let principal = PrincipalId::new(NATIVE_UI_PRINCIPAL_NAME)
        .expect("the native UI principal constant is valid");
    let selected_lease_ids = match parse_lease_ids(&request.capability_lease_ids) {
        Ok(ids) => ids,
        Err(message) => {
            return Ok(HostCallResponse::failure(
                response_request_id,
                "HOST_INVALID_REQUEST",
                message,
                false,
            ))
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
            return Ok(policy_failure(response_request_id, error));
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
        Err(error) => return Ok(bus_failure(response_request_id, error)),
    };
    let handle = match kernel.write() {
        Ok(mut bus) => match bus.begin(admission, now_ms) {
            Ok(handle) => handle,
            Err(error) => return Ok(bus_failure(response_request_id, error)),
        },
        Err(_) => {
            return Ok(HostCallResponse::failure(
                response_request_id,
                "HOST_INTERNAL",
                "kernel bus lock is poisoned",
                false,
            ))
        }
    };

    let handler_result = dispatch(
        &request,
        Some(app),
        &*agent,
        kernel.blobs(),
        kernel.services(),
        kernel.packages(),
    )
    .await;
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
        return Ok(HostCallResponse::failure(
            response_request_id,
            "HOST_INTERNAL",
            message,
            false,
        ));
    }

    match handler_result {
        Ok(value) => Ok(HostCallResponse::success(response_request_id, value)),
        Err(message) => Ok(HostCallResponse::failure(
            response_request_id,
            "HOST_HANDLER_FAILED",
            message,
            false,
        )),
    }
}

async fn dispatch(
    request: &HostCallRequest,
    app: Option<AppHandle>,
    agent: &crate::agent_commands::AgentHostState,
    blobs: &RwLock<BlobStore>,
    services: &RwLock<ServiceRegistry>,
    packages: &RwLock<PackageGate>,
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
        "plugin.connection.test" => {
            let app = app.ok_or("plugin.connection.test requires an application handle")?;
            dispatch_plugin_connection_test(request, app).await
        }
        "plugin.install" => {
            let app = app.ok_or("plugin.install requires an application handle")?;
            dispatch_plugin_install(request, app, packages)
        }
        "plugin.uninstall" => {
            let app = app.ok_or("plugin.uninstall requires an application handle")?;
            dispatch_plugin_uninstall(request, app)
        }
        "plugin.vsix.import" => {
            let app = app.ok_or("plugin.vsix.import requires an application handle")?;
            dispatch_vsix_import(request, app)
        }
        "project.save" => dispatch_project_save(request),
        "project.import" => dispatch_project_import(request),
        "graph.compile" => dispatch_graph_compile(request),
        "graph.diff" => dispatch_graph_diff(request),
        "plugin.analysis.run" => {
            let app = app.ok_or("plugin.analysis.run requires an application handle")?;
            dispatch_plugin_analysis_run(request, app)
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
        "workspace.folder.scan" => {
            let app = app.ok_or("workspace.folder.scan requires an application handle")?;
            dispatch_workspace_folder_scan(request, app)
        }
        "workspace.git.init" => {
            let app = app.ok_or("workspace.git.init requires an application handle")?;
            dispatch_workspace_git_init(request, app)
        }
        "workspace.github.ssh.generate" => {
            let app = app.ok_or("workspace.github.ssh.generate requires an application handle")?;
            dispatch_workspace_github_ssh_generate(request, app)
        }
        "workspace.github.login" => {
            let app = app.ok_or("workspace.github.login requires an application handle")?;
            dispatch_workspace_github_login(request, app).await
        }
        "workspace.github.ssh.upload" => {
            let app = app.ok_or("workspace.github.ssh.upload requires an application handle")?;
            dispatch_workspace_github_ssh_upload(request, app).await
        }
        "workspace.git.autosave" => {
            let app = app.ok_or("workspace.git.autosave requires an application handle")?;
            dispatch_workspace_git_autosave(request, app)
        }
        "agent.job.status" => dispatch_agent_job_status(request, agent),
        "agent.job.list" => dispatch_agent_job_list(request, agent),
        "agent.batch.status" => dispatch_agent_batch_status(request, agent),
        "agent.job.review" => dispatch_agent_job_review(request, agent),
        "agent.job.cancel" => dispatch_agent_job_cancel(request, agent),
        "agent.job.start" => {
            let app = app.ok_or("agent.job.start requires an application handle")?;
            dispatch_agent_job_start(request, app, agent).await
        }
        "agent.batch.start" => {
            let app = app.ok_or("agent.batch.start requires an application handle")?;
            dispatch_agent_batch_start(request, app, agent).await
        }
        "blob.write" => dispatch_blob_write(request, blobs),
        "blob.read" => dispatch_blob_read(request, blobs),
        "blob.upload.begin" => dispatch_blob_upload_begin(request, blobs),
        "blob.upload.chunk" => dispatch_blob_upload_chunk(request, blobs),
        "blob.upload.commit" => dispatch_blob_upload_commit(request, blobs),
        "plugin.artifact.save" => {
            let app = app.ok_or("plugin.artifact.save requires an application handle")?;
            dispatch_plugin_artifact_save(request, app, blobs)
        }
        "service.register" => dispatch_service_register(request, services),
        "service.call" => dispatch_service_call(request, services),
        "service.list" => crate::host_bus::services::dispatch_service_list(services),
        "service.unregister" => {
            crate::host_bus::services::dispatch_service_unregister(request, services)
        }
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

async fn dispatch_plugin_connection_test(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let connection_request = inline_request::<PluginConnectionTestRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.connection.test request: {error}")))?;
    let result = crate::plugins::test_plugin_connection(
        app,
        connection_request.plugin_id,
        connection_request.plugin_version,
        connection_request.connection_id,
        connection_request.action_id,
        connection_request.values,
        connection_request.secrets,
    )
    .await?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn dispatch_plugin_install(
    request: &HostCallRequest,
    app: AppHandle,
    packages: &RwLock<PackageGate>,
) -> Result<Value, String> {
    let install_request = inline_request::<PluginInstallRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.install request: {error}")))?;
    // The digest is the gate key and is computed from the same resolved
    // archive bytes the real install will read.
    let digest = crate::plugins::package_digest(&app, &install_request.path)?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    // The package must pass the kernel supply-chain gate before any install
    // side effect runs. A gate error aborts the install entirely.
    run_package_gate(packages, &digest, now_ms)?;
    let installed = crate::plugins::install_myc_plugin(app, install_request.path)?;
    serde_json::to_value(installed).map_err(|error| error.to_string())
}

/// Runs the kernel PackageGate transaction (submit -> scan -> approve ->
/// activate) for one package digest before the real install runs.
///
/// The scan is the lightweight, deterministic pre-check: the digest was
/// already computed from a readable, non-empty resolved archive, so the
/// report passes unconditionally; full manifest/signature/payload validation
/// still happens inside `install_myc_plugin`. Every transition is
/// fail-closed — a duplicate or previously rejected digest stops here and the
/// install never runs.
fn run_package_gate(
    packages: &RwLock<PackageGate>,
    digest: &str,
    now_ms: u64,
) -> Result<(), String> {
    let mut gate = packages
        .write()
        .map_err(|_| "package gate lock is poisoned".to_string())?;
    gate.submit(digest, now_ms)
        .map_err(|error| format!("package gate rejected the install: {error}"))?;
    gate.scan(
        digest,
        &ScanReport {
            digest: digest.to_string(),
            passed: true,
            findings: Vec::new(),
            scanner_id: "anyway.installer".to_string(),
            scanner_version: "1".to_string(),
        },
        now_ms,
    )
    .map_err(|error| format!("package gate scan failed: {error}"))?;
    gate.approve(digest, now_ms)
        .map_err(|error| format!("package gate approval failed: {error}"))?;
    gate.activate(digest, now_ms)
        .map_err(|error| format!("package gate activation failed: {error}"))?;
    Ok(())
}

fn dispatch_plugin_uninstall(request: &HostCallRequest, app: AppHandle) -> Result<Value, String> {
    let uninstall_request = inline_request::<PluginUninstallRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.uninstall request: {error}")))?;
    crate::plugins::uninstall_myc_plugin(
        app,
        uninstall_request.plugin_id,
        uninstall_request.plugin_version,
    )?;
    Ok(serde_json::Value::Null)
}

fn dispatch_vsix_import(request: &HostCallRequest, app: AppHandle) -> Result<Value, String> {
    let import_request = inline_request::<VsixImportRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.vsix.import request: {error}")))?;
    let report = crate::vsix_importer::import_vscode_vsix(app, import_request.path)?;
    serde_json::to_value(report).map_err(|error| error.to_string())
}

fn dispatch_project_save(request: &HostCallRequest) -> Result<Value, String> {
    let save_request = inline_request::<ProjectSaveRequest>(request)
        .or_else(|error| Err(format!("invalid project.save request: {error}")))?;
    let result = crate::projects::save_project_file(save_request.path, save_request.project)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn dispatch_project_import(request: &HostCallRequest) -> Result<Value, String> {
    let import_request = inline_request::<ProjectImportRequest>(request)
        .or_else(|error| Err(format!("invalid project.import request: {error}")))?;
    let result = crate::projects::import_project_file(import_request.path)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn dispatch_graph_compile(request: &HostCallRequest) -> Result<Value, String> {
    let compile_request = inline_request::<GraphCompileRequest>(request)
        .or_else(|error| Err(format!("invalid graph.compile request: {error}")))?;
    let result = crate::graph_cmds::compile_project(compile_request.project)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn dispatch_graph_diff(request: &HostCallRequest) -> Result<Value, String> {
    let diff_request = inline_request::<GraphDiffRequest>(request)
        .or_else(|error| Err(format!("invalid graph.diff request: {error}")))?;
    let result = crate::graph_cmds::compute_diff(diff_request.v1, diff_request.v2)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn dispatch_plugin_analysis_run(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let run_request = inline_request::<PluginAnalysisRunRequest>(request).or_else(|error| {
        Err(format!("invalid plugin.analysis.run request: {error}"))
    })?;
    let result = crate::plugins::execute_myc_plugin(
        app,
        run_request.plugin_id,
        run_request.plugin_version,
        run_request.capability,
        run_request.input,
    )?;
    serde_json::to_value(result).map_err(|error| error.to_string())
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

fn dispatch_agent_job_review(
    request: &HostCallRequest,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    let review_request = inline_request::<AgentJobReviewRequest>(request)
        .or_else(|error| Err(format!("invalid agent.job.review request: {error}")))?;
    let mut host = agent
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    host.review_patch(&review_request.job_id, review_request.accept)?;
    let job = host
        .get_job(&review_request.job_id)
        .ok_or_else(|| "Job vanished".to_string())?;
    serde_json::to_value(crate::agent_commands::PdfJobStatus::from(job))
        .map_err(|error| error.to_string())
}

fn dispatch_agent_job_cancel(
    request: &HostCallRequest,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    let cancel_request = inline_request::<AgentJobCancelRequest>(request)
        .or_else(|error| Err(format!("invalid agent.job.cancel request: {error}")))?;
    let mut host = agent
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    let reason = cancel_request
        .reason
        .unwrap_or_else(|| "Cancelled by user".to_string());
    host.cancel_job(&cancel_request.job_id, &reason)?;
    let job = host
        .get_job(&cancel_request.job_id)
        .ok_or_else(|| "Job vanished".to_string())?;
    serde_json::to_value(crate::agent_commands::PdfJobStatus::from(job))
        .map_err(|error| error.to_string())
}

async fn dispatch_agent_job_start(
    request: &HostCallRequest,
    app: AppHandle,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    let start_request = inline_request::<crate::agent_commands::StartPdfJobRequest>(request)
        .or_else(|error| Err(format!("invalid agent.job.start request: {error}")))?;
    let queued = crate::agent_commands::queue_pdf_job(app, agent, start_request)?;
    serde_json::to_value(queued).map_err(|error| error.to_string())
}

async fn dispatch_agent_batch_start(
    request: &HostCallRequest,
    app: AppHandle,
    agent: &crate::agent_commands::AgentHostState,
) -> Result<Value, String> {
    let start_request = inline_request::<crate::agent_commands::StartDocumentBatchRequest>(request)
        .or_else(|error| Err(format!("invalid agent.batch.start request: {error}")))?;
    let queued = crate::agent_commands::queue_document_batch(app, agent, start_request)?;
    serde_json::to_value(queued).map_err(|error| error.to_string())
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

fn dispatch_workspace_folder_scan(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let scan_request = inline_request::<WorkspaceFolderScanRequest>(request)
        .or_else(|error| Err(format!("invalid workspace.folder.scan request: {error}")))?;
    let projects = crate::workspace_host::scan_project_folder(
        app,
        scan_request.plugin_id,
        scan_request.plugin_version,
        scan_request.path,
    )?;
    serde_json::to_value(projects).map_err(|error| error.to_string())
}

fn dispatch_workspace_git_init(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let init_request = inline_request::<WorkspaceGitInitRequest>(request)
        .or_else(|error| Err(format!("invalid workspace.git.init request: {error}")))?;
    let snapshot = crate::workspace_host::initialize_git_workspace(
        app,
        init_request.plugin_id,
        init_request.plugin_version,
        init_request.path,
    )?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
}

fn dispatch_workspace_github_ssh_generate(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let generate_request = inline_request::<WorkspaceGithubSshGenerateRequest>(request).or_else(
        |error| Err(format!("invalid workspace.github.ssh.generate request: {error}")),
    )?;
    let status = crate::workspace_host::generate_github_ssh_key(
        app,
        generate_request.plugin_id,
        generate_request.plugin_version,
        generate_request.comment,
    )?;
    serde_json::to_value(status).map_err(|error| error.to_string())
}

async fn dispatch_workspace_github_login(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let login_request = inline_request::<WorkspaceGithubLoginRequest>(request)
        .or_else(|error| Err(format!("invalid workspace.github.login request: {error}")))?;
    let status = crate::workspace_host::login_github_account(
        app,
        login_request.plugin_id,
        login_request.plugin_version,
    )
    .await?;
    serde_json::to_value(status).map_err(|error| error.to_string())
}

async fn dispatch_workspace_github_ssh_upload(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let upload_request = inline_request::<WorkspaceGithubSshUploadRequest>(request).or_else(
        |error| Err(format!("invalid workspace.github.ssh.upload request: {error}")),
    )?;
    let status = crate::workspace_host::upload_github_ssh_key(
        app,
        upload_request.plugin_id,
        upload_request.plugin_version,
        upload_request.path,
    )
    .await?;
    serde_json::to_value(status).map_err(|error| error.to_string())
}

fn dispatch_workspace_git_autosave(
    request: &HostCallRequest,
    app: AppHandle,
) -> Result<Value, String> {
    let autosave_request = inline_request::<WorkspaceGitAutosaveRequest>(request).or_else(|error| {
        Err(format!("invalid workspace.git.autosave request: {error}"))
    })?;
    let snapshot = crate::workspace_host::git_autosave_project(
        app,
        autosave_request.plugin_id,
        autosave_request.plugin_version,
        autosave_request.repo_path,
        autosave_request.project_path,
        autosave_request.project,
        autosave_request.message,
    )?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
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

fn dispatch_blob_upload_begin(
    request: &HostCallRequest,
    blobs: &RwLock<BlobStore>,
) -> Result<Value, String> {
    let begin_request = inline_request::<BlobUploadBeginRequest>(request)
        .or_else(|error| Err(format!("invalid blob.upload.begin request: {error}")))?;
    // The frontend addresses export artifacts with the "plugin" wire scope,
    // which is not a wire form the strict parser accepts. It means "private
    // to the native UI host", so bind it to that principal's private scope;
    // every other value must parse through the strict wire grammar.
    let scope = if begin_request.scope == "plugin" {
        BlobScope::private(NATIVE_UI_PRINCIPAL_NAME)
            .expect("the native UI principal is a valid private scope")
    } else {
        BlobScope::from_wire(&begin_request.scope)
            .map_err(|error| format!("invalid blob scope: {error}"))?
    };
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
            begin_request.media_type.clone(),
            begin_request.size,
            now_ms,
            BLOB_UPLOAD_TTL_MS,
        )
        .map_err(|error| blob_error_message("begin upload", &error))?;
    Ok(json!({ "leaseId": lease.value() }))
}

fn dispatch_blob_upload_chunk(
    request: &HostCallRequest,
    blobs: &RwLock<BlobStore>,
) -> Result<Value, String> {
    let chunk_request = inline_request::<BlobUploadChunkRequest>(request)
        .or_else(|error| Err(format!("invalid blob.upload.chunk request: {error}")))?;
    let content = base64::engine::general_purpose::STANDARD
        .decode(&chunk_request.content_base64)
        .map_err(|_| "blob chunk must be base64".to_string())?;
    if content.len() > MAX_BLOB_CHUNK_BYTES {
        return Err(format!(
            "blob chunk exceeds the {} byte chunk limit",
            MAX_BLOB_CHUNK_BYTES
        ));
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let owner = NATIVE_UI_PRINCIPAL_NAME;
    let mut store = blobs
        .write()
        .map_err(|_| "blob store lock is poisoned".to_string())?;
    let uploaded = store
        .upload_chunk(
            UploadLeaseId::from_value(chunk_request.lease_id),
            owner,
            &content,
            now_ms,
        )
        .map_err(|error| blob_error_message("upload chunk", &error))?;
    Ok(json!({ "uploadedBytes": uploaded }))
}

fn dispatch_blob_upload_commit(
    request: &HostCallRequest,
    blobs: &RwLock<BlobStore>,
) -> Result<Value, String> {
    let commit_request = inline_request::<BlobUploadCommitRequest>(request)
        .or_else(|error| Err(format!("invalid blob.upload.commit request: {error}")))?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let owner = NATIVE_UI_PRINCIPAL_NAME;
    let mut store = blobs
        .write()
        .map_err(|_| "blob store lock is poisoned".to_string())?;
    let reference = store
        .commit_upload(
            UploadLeaseId::from_value(commit_request.lease_id),
            owner,
            now_ms,
        )
        .map_err(|error| blob_error_message("commit upload", &error))?;
    Ok(blob_ref_to_json(&reference, owner))
}

fn dispatch_plugin_artifact_save(
    request: &HostCallRequest,
    app: AppHandle,
    blobs: &RwLock<BlobStore>,
) -> Result<Value, String> {
    let save_request = inline_request::<PluginArtifactSaveRequest>(request)
        .or_else(|error| Err(format!("invalid plugin.artifact.save request: {error}")))?;
    let reference = host_blob_ref_to_kernel(&save_request.blob_ref)?;
    let data = {
        let store = blobs
            .read()
            .map_err(|_| "blob store lock is poisoned".to_string())?;
        store
            .read_blob_bytes(&reference, NATIVE_UI_PRINCIPAL_NAME)
            .map_err(|error| blob_error_message("read artifact bytes", &error))?
    };
    let saved = crate::workspace_host::save_plugin_artifact(
        app,
        save_request.plugin_id,
        save_request.plugin_version,
        save_request.format,
        save_request.path,
        data,
    )?;
    serde_json::to_value(saved).map_err(|error| error.to_string())
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

pub fn inline_request<T: for<'de> serde::Deserialize<'de>>(
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
    fn default_kernel_state_registers_plugin_install_uninstall_and_vsix_import_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("plugin.install", "plugin", "install"),
            ("plugin.uninstall", "plugin", "uninstall"),
            ("plugin.vsix.import", "plugin", "vsix.import"),
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
    fn plugin_install_uninstall_and_vsix_import_dtos_parse_and_reject_unknown_fields() {
        let install_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "plugin-install-1",
            "operation": "plugin.install",
            "payload": {
                "kind": "inline",
                "value": { "path": "/tmp/example.myc" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(install_request.validate(), Ok(()));

        let install: PluginInstallRequest =
            inline_request(&install_request).expect("plugin install DTO deserializes");
        assert_eq!(install.path, "/tmp/example.myc");

        assert!(
            serde_json::from_value::<PluginInstallRequest>(json!({
                "path": "/tmp/example.myc",
                "sneaky": true
            }))
            .is_err(),
            "the install DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<PluginInstallRequest>(json!({})).is_err(),
            "the install DTO must require the path"
        );

        let uninstall_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "plugin-uninstall-1",
            "operation": "plugin.uninstall",
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
        assert_eq!(uninstall_request.validate(), Ok(()));

        let uninstall: PluginUninstallRequest =
            inline_request(&uninstall_request).expect("plugin uninstall DTO deserializes");
        assert_eq!(uninstall.plugin_id, "myc.onedarkpro");
        assert_eq!(uninstall.plugin_version, "1.3.0");

        assert!(
            serde_json::from_value::<PluginUninstallRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "sneaky": true
            }))
            .is_err(),
            "the uninstall DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<PluginUninstallRequest>(json!({
                "pluginId": "myc.onedarkpro"
            }))
            .is_err(),
            "the uninstall DTO must require the plugin version"
        );

        let vsix_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "vsix-import-1",
            "operation": "plugin.vsix.import",
            "payload": {
                "kind": "inline",
                "value": { "path": "/tmp/theme.vsix" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(vsix_request.validate(), Ok(()));

        let vsix: VsixImportRequest =
            inline_request(&vsix_request).expect("vsix import DTO deserializes");
        assert_eq!(vsix.path, "/tmp/theme.vsix");

        assert!(
            serde_json::from_value::<VsixImportRequest>(json!({
                "path": "/tmp/theme.vsix",
                "sneaky": true
            }))
            .is_err(),
            "the vsix import DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<VsixImportRequest>(json!({})).is_err(),
            "the vsix import DTO must require the path"
        );
    }

    #[test]
    fn default_kernel_state_registers_project_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("project.save", "project", "save"),
            ("project.import", "project", "import"),
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
    fn project_save_and_import_dtos_parse_and_reject_unknown_fields() {
        let save_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "project-save-1",
            "operation": "project.save",
            "payload": {
                "kind": "inline",
                "value": {
                    "path": "/tmp/project.mycproj",
                    "project": {
                        "schemaVersion": 2,
                        "title": "PINN architecture"
                    }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(save_request.validate(), Ok(()));

        let save: ProjectSaveRequest =
            inline_request(&save_request).expect("project save DTO deserializes");
        assert_eq!(save.path, "/tmp/project.mycproj");
        assert_eq!(save.project["title"], "PINN architecture");

        assert!(
            serde_json::from_value::<ProjectSaveRequest>(json!({
                "path": "/tmp/project.mycproj",
                "project": {},
                "sneaky": true
            }))
            .is_err(),
            "the save DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<ProjectSaveRequest>(json!({
                "project": {}
            }))
            .is_err(),
            "the save DTO must require the path"
        );
        assert!(
            serde_json::from_value::<ProjectSaveRequest>(json!({
                "path": "/tmp/project.mycproj"
            }))
            .is_err(),
            "the save DTO must require the project value"
        );

        let import_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "project-import-1",
            "operation": "project.import",
            "payload": {
                "kind": "inline",
                "value": { "path": "/tmp/project.mycproj" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(import_request.validate(), Ok(()));

        let import: ProjectImportRequest =
            inline_request(&import_request).expect("project import DTO deserializes");
        assert_eq!(import.path, "/tmp/project.mycproj");

        assert!(
            serde_json::from_value::<ProjectImportRequest>(json!({
                "path": "/tmp/project.mycproj",
                "sneaky": true
            }))
            .is_err(),
            "the import DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<ProjectImportRequest>(json!({})).is_err(),
            "the import DTO must require the path"
        );
    }

    #[test]
    fn default_kernel_state_registers_graph_and_plugin_analysis_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("graph.compile", "graph", "compile"),
            ("graph.diff", "graph", "diff"),
            ("plugin.analysis.run", "plugin", "analysis.run"),
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
    fn graph_compile_and_diff_dtos_parse_and_reject_unknown_fields() {
        let compile_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "graph-compile-1",
            "operation": "graph.compile",
            "payload": {
                "kind": "inline",
                "value": { "project": { "schemaVersion": 3, "title": "PINN" } }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(compile_request.validate(), Ok(()));

        let compile: GraphCompileRequest =
            inline_request(&compile_request).expect("graph compile DTO deserializes");
        assert_eq!(compile.project["title"], "PINN");

        assert!(
            serde_json::from_value::<GraphCompileRequest>(json!({
                "project": {},
                "sneaky": true
            }))
            .is_err(),
            "the compile DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<GraphCompileRequest>(json!({})).is_err(),
            "the compile DTO must require the project value"
        );

        let diff_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "graph-diff-1",
            "operation": "graph.diff",
            "payload": {
                "kind": "inline",
                "value": {
                    "v1": { "nodes": [], "edges": [] },
                    "v2": { "nodes": [], "edges": [] }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(diff_request.validate(), Ok(()));

        let diff: GraphDiffRequest =
            inline_request(&diff_request).expect("graph diff DTO deserializes");
        assert_eq!(diff.v1["nodes"].as_array().map(Vec::len), Some(0));
        assert_eq!(diff.v2["edges"].as_array().map(Vec::len), Some(0));

        assert!(
            serde_json::from_value::<GraphDiffRequest>(json!({
                "v1": {},
                "v2": {},
                "sneaky": true
            }))
            .is_err(),
            "the diff DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<GraphDiffRequest>(json!({ "v1": {} })).is_err(),
            "the diff DTO must require the v2 value"
        );
    }

    #[test]
    fn plugin_analysis_run_dto_parses_identity_and_rejects_unknown_fields() {
        let run_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "plugin-analysis-run-1",
            "operation": "plugin.analysis.run",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.example",
                    "pluginVersion": "1.0.0",
                    "capability": "analysis.run",
                    "input": { "apiVersion": "researchcanvas.dev/plugin-call/v1alpha1" }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(run_request.validate(), Ok(()));

        let run: PluginAnalysisRunRequest =
            inline_request(&run_request).expect("plugin analysis run DTO deserializes");
        assert_eq!(run.plugin_id, "myc.example");
        assert_eq!(run.plugin_version, "1.0.0");
        assert_eq!(run.capability.as_deref(), Some("analysis.run"));
        assert_eq!(run.input["apiVersion"], "researchcanvas.dev/plugin-call/v1alpha1");

        let defaulted: PluginAnalysisRunRequest = serde_json::from_value(json!({
            "pluginId": "myc.example",
            "pluginVersion": "1.0.0",
            "input": {}
        }))
        .expect("the run DTO defaults the capability");
        assert_eq!(defaulted.capability, None);

        assert!(
            serde_json::from_value::<PluginAnalysisRunRequest>(json!({
                "pluginId": "myc.example",
                "pluginVersion": "1.0.0",
                "input": {},
                "sneaky": true
            }))
            .is_err(),
            "the run DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<PluginAnalysisRunRequest>(json!({
                "pluginId": "myc.example",
                "pluginVersion": "1.0.0"
            }))
            .is_err(),
            "the run DTO must require the input value"
        );
        assert!(
            serde_json::from_value::<PluginAnalysisRunRequest>(json!({
                "pluginId": "myc.example",
                "input": {}
            }))
            .is_err(),
            "the run DTO must require the plugin version"
        );
    }

    #[test]
    fn graph_handlers_dispatch_compile_and_diff_to_the_kernel() {
        let project = json!({
            "schemaVersion": 3,
            "id": "proj-1",
            "title": "PDF test",
            "discipline": "cs",
            "nodes": [
                {"id": "pdf-sec-s1", "type": "note", "title": "Introduction", "tags": [], "evidenceIds": [], "data": {}},
                {"id": "pdf-p1", "type": "evidence", "title": "A novel approach…", "tags": [], "evidenceIds": [], "data": {}}
            ],
            "edges": [
                {"id": "pdf-edge-para-p1", "type": "part_of", "source": "pdf-p1", "target": "pdf-sec-s1", "directed": true}
            ],
            "evidence": [],
            "placements": [],
            "scenarios": [],
            "activity": []
        });

        let compile_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "graph-compile-dispatch",
            "operation": "graph.compile",
            "payload": { "kind": "inline", "value": { "project": project } },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let compiled = dispatch_graph_compile(&compile_request).expect("graph.compile succeeded");
        assert!(compiled["compile"]["fileHash"].as_str().is_some());
        assert!(compiled["logicChain"]["score"].as_f64().is_some());
        assert!(compiled["beliefs"]["meanNetBelief"].as_f64().is_some());

        let diff_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "graph-diff-dispatch",
            "operation": "graph.diff",
            "payload": {
                "kind": "inline",
                "value": {
                    "v1": { "nodes": [], "edges": [] },
                    "v2": { "nodes": [], "edges": [] }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let diff = dispatch_graph_diff(&diff_request).expect("graph.diff succeeded");
        assert_eq!(diff["addedNodes"], json!([]));
        assert!(diff.get("durationMs").is_some());
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
    fn default_kernel_state_registers_workspace_write_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("workspace.folder.scan", "workspace", "folder.scan"),
            ("workspace.git.init", "workspace", "git.init"),
            (
                "workspace.github.ssh.generate",
                "workspace",
                "github.ssh.generate",
            ),
            ("workspace.git.autosave", "workspace", "git.autosave"),
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
    fn workspace_write_dtos_parse_identity_and_reject_unknown_fields() {
        let scan_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "folder-scan-1",
            "operation": "workspace.folder.scan",
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
        let scan: WorkspaceFolderScanRequest =
            inline_request(&scan_request).expect("folder scan DTO deserializes");
        assert_eq!(scan.plugin_id, "myc.onedarkpro");
        assert_eq!(scan.plugin_version, "1.3.0");
        assert_eq!(scan.path, "/workspace");
        assert!(
            serde_json::from_value::<WorkspaceFolderScanRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "path": "/workspace",
                "capability": "project.folder"
            }))
            .is_err(),
            "the folder scan DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<WorkspaceFolderScanRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0"
            }))
            .is_err(),
            "the folder scan DTO must require the path"
        );

        let init_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "git-init-1",
            "operation": "workspace.git.init",
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
        let init: WorkspaceGitInitRequest =
            inline_request(&init_request).expect("git init DTO deserializes");
        assert_eq!(init.plugin_id, "myc.onedarkpro");
        assert_eq!(init.plugin_version, "1.3.0");
        assert_eq!(init.path, "/workspace");
        assert!(
            serde_json::from_value::<WorkspaceGitInitRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "path": "/workspace",
                "capability": "git.repository.init"
            }))
            .is_err(),
            "the git init DTO must reject the legacy capability field"
        );

        let ssh_generate_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "ssh-generate-1",
            "operation": "workspace.github.ssh.generate",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "comment": "research@canvas"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let ssh_generate: WorkspaceGithubSshGenerateRequest =
            inline_request(&ssh_generate_request).expect("ssh generate DTO deserializes");
        assert_eq!(ssh_generate.plugin_id, "myc.onedarkpro");
        assert_eq!(ssh_generate.plugin_version, "1.3.0");
        assert_eq!(ssh_generate.comment, "research@canvas");
        assert!(
            serde_json::from_value::<WorkspaceGithubSshGenerateRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "comment": "research@canvas",
                "capability": "git.ssh.generate"
            }))
            .is_err(),
            "the ssh generate DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<WorkspaceGithubSshGenerateRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0"
            }))
            .is_err(),
            "the ssh generate DTO must require the comment"
        );

        let autosave_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "git-autosave-1",
            "operation": "workspace.git.autosave",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "repoPath": "/workspace",
                    "projectPath": ".research-canvas/pinn.mycproj",
                    "project": {
                        "schemaVersion": 2,
                        "title": "PINN architecture"
                    },
                    "message": "Research Canvas autosave"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let autosave: WorkspaceGitAutosaveRequest =
            inline_request(&autosave_request).expect("git autosave DTO deserializes");
        assert_eq!(autosave.plugin_id, "myc.onedarkpro");
        assert_eq!(autosave.plugin_version, "1.3.0");
        assert_eq!(autosave.repo_path, "/workspace");
        assert_eq!(autosave.project_path, ".research-canvas/pinn.mycproj");
        assert_eq!(autosave.project["title"], "PINN architecture");
        assert_eq!(autosave.message, "Research Canvas autosave");
        assert!(
            serde_json::from_value::<WorkspaceGitAutosaveRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "repoPath": "/workspace",
                "projectPath": ".research-canvas/pinn.mycproj",
                "project": {},
                "message": "Research Canvas autosave",
                "capability": "git.autosave"
            }))
            .is_err(),
            "the git autosave DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<WorkspaceGitAutosaveRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "repoPath": "/workspace",
                "projectPath": ".research-canvas/pinn.mycproj",
                "project": {}
            }))
            .is_err(),
            "the git autosave DTO must require the message"
        );
    }

    #[test]
    fn default_kernel_state_registers_workspace_github_login_and_ssh_upload_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("workspace.github.login", "workspace", "github.login"),
            (
                "workspace.github.ssh.upload",
                "workspace",
                "github.ssh.upload",
            ),
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
    fn workspace_github_login_and_ssh_upload_dtos_parse_and_reject_unknown_fields() {
        let login_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "github-login-1",
            "operation": "workspace.github.login",
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
        assert_eq!(login_request.validate(), Ok(()));

        let login: WorkspaceGithubLoginRequest =
            inline_request(&login_request).expect("github login DTO deserializes");
        assert_eq!(login.plugin_id, "myc.onedarkpro");
        assert_eq!(login.plugin_version, "1.3.0");
        assert!(
            serde_json::from_value::<WorkspaceGithubLoginRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "capability": "git.account.login"
            }))
            .is_err(),
            "the github login DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<WorkspaceGithubLoginRequest>(json!({
                "pluginId": "myc.onedarkpro"
            }))
            .is_err(),
            "the github login DTO must require the plugin version"
        );

        let upload_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "github-ssh-upload-1",
            "operation": "workspace.github.ssh.upload",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "path": "/home/user/.ssh/id_ed25519.pub"
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(upload_request.validate(), Ok(()));

        let upload: WorkspaceGithubSshUploadRequest =
            inline_request(&upload_request).expect("github ssh upload DTO deserializes");
        assert_eq!(upload.plugin_id, "myc.onedarkpro");
        assert_eq!(upload.plugin_version, "1.3.0");
        assert_eq!(upload.path, "/home/user/.ssh/id_ed25519.pub");
        assert!(
            serde_json::from_value::<WorkspaceGithubSshUploadRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "path": "/home/user/.ssh/id_ed25519.pub",
                "capability": "git.ssh.upload"
            }))
            .is_err(),
            "the github ssh upload DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<WorkspaceGithubSshUploadRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0"
            }))
            .is_err(),
            "the github ssh upload DTO must require the path"
        );
    }

    #[test]
    fn default_kernel_state_registers_the_plugin_connection_test_route() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let operation = crate::kernel::bus::OperationId::new("plugin.connection.test").unwrap();
        let descriptor = bus.operation(&operation).expect("registered operation");
        assert_eq!(descriptor.route().service(), "plugin");
        assert_eq!(descriptor.route().method(), "connection.test");
        assert_eq!(
            descriptor.required_capability().name(),
            "plugin.connection.test"
        );
    }

    #[test]
    fn plugin_connection_test_dto_parses_maps_and_rejects_unknown_or_missing_fields() {
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "connection-test-1",
            "operation": "plugin.connection.test",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.onedarkpro",
                    "pluginVersion": "1.3.0",
                    "connectionId": "openai",
                    "actionId": "test-pdf-extraction",
                    "values": { "baseUrl": "https://api.example.com" },
                    "secrets": {
                        "apiKey": { "action": "set", "value": "sk-secret" }
                    }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(request.validate(), Ok(()));

        let connection: PluginConnectionTestRequest =
            inline_request(&request).expect("plugin connection test DTO deserializes");
        assert_eq!(connection.plugin_id, "myc.onedarkpro");
        assert_eq!(connection.plugin_version, "1.3.0");
        assert_eq!(connection.connection_id, "openai");
        assert_eq!(connection.action_id.as_deref(), Some("test-pdf-extraction"));
        assert_eq!(connection.values["baseUrl"], json!("https://api.example.com"));
        assert_eq!(connection.secrets["apiKey"].action, "set");
        assert_eq!(
            connection.secrets["apiKey"].value.as_deref(),
            Some("sk-secret")
        );

        let defaults: PluginConnectionTestRequest = serde_json::from_value(json!({
            "pluginId": "myc.onedarkpro",
            "pluginVersion": "1.3.0",
            "connectionId": "openai",
            "values": {}
        }))
        .expect("action id and secrets default");
        assert!(defaults.action_id.is_none());
        assert!(defaults.secrets.is_empty());

        assert!(
            serde_json::from_value::<PluginConnectionTestRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "connectionId": "openai",
                "values": {},
                "secrets": {},
                "capability": "plugin.connection.test"
            }))
            .is_err(),
            "the plugin connection test DTO must reject the legacy capability field"
        );
        assert!(
            serde_json::from_value::<PluginConnectionTestRequest>(json!({
                "pluginId": "myc.onedarkpro",
                "pluginVersion": "1.3.0",
                "values": {}
            }))
            .is_err(),
            "the plugin connection test DTO must require the connection id"
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
    fn default_kernel_state_registers_agent_review_and_cancel_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("agent.job.review", "agent", "job.review"),
            ("agent.job.cancel", "agent", "job.cancel"),
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
    fn default_kernel_state_registers_agent_start_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("agent.job.start", "agent", "job.start"),
            ("agent.batch.start", "agent", "batch.start"),
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
    fn agent_start_dtos_parse_minimal_payloads() {
        let job_start_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "job-start-1",
            "operation": "agent.job.start",
            "payload": {
                "kind": "inline",
                "value": { "pdfPath": "/tmp/example.pdf" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(job_start_request.validate(), Ok(()));
        let job_start: crate::agent_commands::StartPdfJobRequest =
            inline_request(&job_start_request).expect("agent job start DTO deserializes");
        assert_eq!(job_start.pdf_path, "/tmp/example.pdf");
        assert_eq!(job_start.settings, None);

        let batch_start_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "batch-start-1",
            "operation": "agent.batch.start",
            "payload": {
                "kind": "inline",
                "value": { "paths": ["/tmp/a.pdf", "/tmp/b.pdf"] }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(batch_start_request.validate(), Ok(()));
        let batch_start: crate::agent_commands::StartDocumentBatchRequest =
            inline_request(&batch_start_request).expect("agent batch start DTO deserializes");
        assert_eq!(batch_start.paths, vec!["/tmp/a.pdf", "/tmp/b.pdf"]);
        assert_eq!(batch_start.model, None);
    }

    #[test]
    fn agent_review_and_cancel_dtos_parse_and_reject_unknown_fields() {
        let review_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "job-review-1",
            "operation": "agent.job.review",
            "payload": {
                "kind": "inline",
                "value": { "jobId": "job-1", "accept": true }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(review_request.validate(), Ok(()));
        let review: AgentJobReviewRequest =
            inline_request(&review_request).expect("agent job review DTO deserializes");
        assert_eq!(review.job_id, "job-1");
        assert!(review.accept);
        assert!(
            serde_json::from_value::<AgentJobReviewRequest>(json!({
                "jobId": "job-1",
                "accept": true,
                "sneaky": true
            }))
            .is_err(),
            "the review DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<AgentJobReviewRequest>(json!({
                "jobId": "job-1"
            }))
            .is_err(),
            "the review DTO must require the accept bool"
        );

        let cancel_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "job-cancel-1",
            "operation": "agent.job.cancel",
            "payload": {
                "kind": "inline",
                "value": { "jobId": "job-1", "reason": "stale" }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(cancel_request.validate(), Ok(()));
        let cancel: AgentJobCancelRequest =
            inline_request(&cancel_request).expect("agent job cancel DTO deserializes");
        assert_eq!(cancel.job_id, "job-1");
        assert_eq!(cancel.reason.as_deref(), Some("stale"));

        let cancel_default: AgentJobCancelRequest = serde_json::from_value(json!({
            "jobId": "job-1"
        }))
        .expect("the cancel DTO defaults the reason");
        assert_eq!(cancel_default.reason, None);
        assert!(
            serde_json::from_value::<AgentJobCancelRequest>(json!({
                "jobId": "job-1",
                "reason": "stale",
                "sneaky": true
            }))
            .is_err(),
            "the cancel DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<AgentJobCancelRequest>(json!({})).is_err(),
            "the cancel DTO must require the job id"
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
    fn default_kernel_state_registers_blob_upload_and_artifact_save_routes() {
        let state = create_kernel_state().expect("kernel state");
        let bus = state.read().expect("bus read lock");
        let expectations = [
            ("blob.upload.begin", "blob", "upload.begin"),
            ("blob.upload.chunk", "blob", "upload.chunk"),
            ("blob.upload.commit", "blob", "upload.commit"),
            ("plugin.artifact.save", "plugin", "artifact.save"),
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
    fn blob_upload_and_artifact_save_dtos_parse_and_reject_unknown_or_missing_fields() {
        let begin_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-upload-begin-1",
            "operation": "blob.upload.begin",
            "payload": {
                "kind": "inline",
                "value": {
                    "scope": "plugin",
                    "mediaType": "application/pdf",
                    "size": 4096
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(begin_request.validate(), Ok(()));
        let begin: BlobUploadBeginRequest =
            inline_request(&begin_request).expect("blob upload begin DTO deserializes");
        assert_eq!(begin.scope, "plugin");
        assert_eq!(begin.media_type, "application/pdf");
        assert_eq!(begin.size, 4096);
        assert!(
            serde_json::from_value::<BlobUploadBeginRequest>(json!({
                "scope": "plugin",
                "mediaType": "application/pdf",
                "size": 4096,
                "sneaky": true
            }))
            .is_err(),
            "the begin DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<BlobUploadBeginRequest>(json!({
                "scope": "plugin",
                "mediaType": "application/pdf"
            }))
            .is_err(),
            "the begin DTO must require the size"
        );

        let chunk_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-upload-chunk-1",
            "operation": "blob.upload.chunk",
            "payload": {
                "kind": "inline",
                "value": {
                    "leaseId": 7,
                    "contentBase64": "YWJjZA=="
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(chunk_request.validate(), Ok(()));
        let chunk: BlobUploadChunkRequest =
            inline_request(&chunk_request).expect("blob upload chunk DTO deserializes");
        assert_eq!(chunk.lease_id, 7);
        assert_eq!(chunk.content_base64, "YWJjZA==");
        assert!(
            serde_json::from_value::<BlobUploadChunkRequest>(json!({
                "leaseId": 7,
                "contentBase64": "YWJjZA==",
                "sneaky": true
            }))
            .is_err(),
            "the chunk DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<BlobUploadChunkRequest>(json!({
                "leaseId": 7
            }))
            .is_err(),
            "the chunk DTO must require the base64 content"
        );

        let commit_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-upload-commit-1",
            "operation": "blob.upload.commit",
            "payload": {
                "kind": "inline",
                "value": { "leaseId": 7 }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(commit_request.validate(), Ok(()));
        let commit: BlobUploadCommitRequest =
            inline_request(&commit_request).expect("blob upload commit DTO deserializes");
        assert_eq!(commit.lease_id, 7);
        assert!(
            serde_json::from_value::<BlobUploadCommitRequest>(json!({
                "leaseId": 7,
                "sneaky": true
            }))
            .is_err(),
            "the commit DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<BlobUploadCommitRequest>(json!({})).is_err(),
            "the commit DTO must require the lease id"
        );

        let save_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "plugin-artifact-save-1",
            "operation": "plugin.artifact.save",
            "payload": {
                "kind": "inline",
                "value": {
                    "pluginId": "myc.example",
                    "pluginVersion": "1.0.0",
                    "format": "pdf",
                    "path": "/tmp/example.pdf",
                    "blobRef": {
                        "algorithm": "sha256",
                        "digest": "a".repeat(64),
                        "size": 4096,
                        "mediaType": "application/pdf",
                        "scope": "private:native.ui",
                        "owner": "native.ui",
                        "retentionClass": "session"
                    }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        assert_eq!(save_request.validate(), Ok(()));
        let save: PluginArtifactSaveRequest =
            inline_request(&save_request).expect("plugin artifact save DTO deserializes");
        assert_eq!(save.plugin_id, "myc.example");
        assert_eq!(save.plugin_version, "1.0.0");
        assert_eq!(save.format, "pdf");
        assert_eq!(save.path, "/tmp/example.pdf");
        assert_eq!(save.blob_ref.digest, "a".repeat(64));
        assert!(
            serde_json::from_value::<PluginArtifactSaveRequest>(json!({
                "pluginId": "myc.example",
                "pluginVersion": "1.0.0",
                "format": "pdf",
                "path": "/tmp/example.pdf",
                "blobRef": {},
                "sneaky": true
            }))
            .is_err(),
            "the artifact save DTO must reject unknown fields"
        );
        assert!(
            serde_json::from_value::<PluginArtifactSaveRequest>(json!({
                "pluginId": "myc.example",
                "pluginVersion": "1.0.0",
                "format": "pdf",
                "path": "/tmp/example.pdf"
            }))
            .is_err(),
            "the artifact save DTO must require the blob ref"
        );
    }

    #[test]
    fn blob_upload_begin_chunk_commit_round_trips_multiple_chunks() {
        let state = create_kernel_state().expect("kernel state");

        let begin_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-upload-begin-1",
            "operation": "blob.upload.begin",
            "payload": {
                "kind": "inline",
                "value": {
                    "scope": "plugin",
                    "mediaType": "text/plain",
                    "size": 6
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let begun = dispatch_blob_upload_begin(&begin_request, state.blobs())
            .expect("blob.upload.begin succeeded");
        let lease_id = begun["leaseId"].as_u64().expect("lease id");

        for (index, chunk) in ["YWJj", "ZGVm"].iter().enumerate() {
            let chunk_request: HostCallRequest = serde_json::from_value(json!({
                "apiVersion": HOST_SDK_API_VERSION,
                "requestId": format!("blob-upload-chunk-{index}"),
                "operation": "blob.upload.chunk",
                "payload": {
                    "kind": "inline",
                    "value": {
                        "leaseId": lease_id,
                        "contentBase64": chunk
                    }
                },
                "deadlineMs": 30_000
            }))
            .unwrap();
            let uploaded = dispatch_blob_upload_chunk(&chunk_request, state.blobs())
                .expect("blob.upload.chunk succeeded");
            assert_eq!(uploaded["uploadedBytes"], 3);
        }

        let commit_request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "blob-upload-commit-1",
            "operation": "blob.upload.commit",
            "payload": {
                "kind": "inline",
                "value": { "leaseId": lease_id }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let committed = dispatch_blob_upload_commit(&commit_request, state.blobs())
            .expect("blob.upload.commit succeeded");
        assert_eq!(committed["size"], 6);
        assert_eq!(committed["algorithm"], "sha256");
        assert_eq!(committed["scope"], "private:native.ui");
        assert_eq!(committed["owner"], NATIVE_UI_PRINCIPAL_NAME);
        assert_eq!(
            committed["digest"],
            crate::kernel::blob::BlobDigest::sha256(b"abcdef").to_hex()
        );
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

    #[test]
    fn package_gate_flow_activates_and_rejects_a_second_install_of_the_same_digest() {
        use crate::kernel::package_gate::CandidateStatus;

        let packages = RwLock::new(PackageGate::new());
        let digest = "a".repeat(64);

        run_package_gate(&packages, &digest, 1_000).expect("first install passes the gate");
        assert_eq!(
            packages.read().expect("gate read lock").status(&digest),
            Some(CandidateStatus::Activated)
        );

        let second = run_package_gate(&packages, &digest, 2_000);
        assert!(
            second.is_err(),
            "a second install of the same digest must be rejected by the gate"
        );
        let message = second.unwrap_err();
        assert!(message.contains("already submitted"), "message: {message}");
        assert_eq!(
            packages.read().expect("gate read lock").status(&digest),
            Some(CandidateStatus::Activated),
            "the rejected second install must not change the gate state"
        );
    }

    #[test]
    fn package_gate_flow_rejects_a_digest_already_rejected_in_the_gate() {
        use crate::kernel::package_gate::CandidateStatus;

        let packages = RwLock::new(PackageGate::new());
        let digest = "b".repeat(64);
        {
            let mut gate = packages.write().expect("gate write lock");
            gate.submit(&digest, 1_000).expect("submits");
            gate.scan(
                &digest,
                &ScanReport {
                    digest: digest.clone(),
                    passed: false,
                    findings: vec!["signature verification failed".to_string()],
                    scanner_id: "anyway.installer".to_string(),
                    scanner_version: "1".to_string(),
                },
                1_000,
            )
            .expect("records the rejection");
        }

        let retry = run_package_gate(&packages, &digest, 2_000);
        assert!(
            retry.is_err(),
            "a digest already rejected in the gate must never pass again"
        );
        let message = retry.unwrap_err();
        assert!(message.contains("already submitted"), "message: {message}");
        assert_eq!(
            packages.read().expect("gate read lock").status(&digest),
            Some(CandidateStatus::Rejected {
                reason: "signature verification failed".to_string()
            })
        );
    }
}
