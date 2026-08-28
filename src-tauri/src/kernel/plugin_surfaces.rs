//! Generic plugin surface state and the Host-owned Python worker session pool.
//!
//! This module deliberately contains no PDF vocabulary. A manifest selects a
//! worker and its declared operations; the worker projects bounded generic
//! surface data back into this registry.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use rand::{rngs::OsRng, RngCore as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    host_bus::workers::{
        PythonWorkerSession, SecretEnv, WorkerError, WorkerSessionConfig, MAX_INLINE_BYTES,
    },
    plugins::PluginWorkerDescriptor,
};

const MAX_SESSION_ID_BYTES: usize = 160;
const MAX_SURFACES: usize = 32;
const MAX_ACTION_BYTES: usize = 64 * 1024;
const MAX_SESSIONS: usize = 256;
const MAX_MODEL_ITEMS: usize = 128;
const MAX_EVENTS: usize = 64;
const MAX_STRING_BYTES: usize = 2 * 1024;
const MAX_WORKER_DEPTH: usize = 8;
pub(crate) const MAX_SURFACE_CONTINUATIONS: usize = 64;
const SURFACE_CONTINUATION_TOTAL_BUDGET: Duration = Duration::from_secs(120);
// Direct provider SSE remains bounded by the 120-second continuation chain,
// but one network-backed worker request needs more than the old 10-second
// local parser budget. Cancellation still interrupts and retires the process.
const SURFACE_WORKER_REQUEST_BUDGET: Duration = Duration::from_secs(90);
const MAX_CONTINUATION_CURSOR_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceRequest {
    pub plugin_id: String,
    #[serde(default)]
    pub plugin_version: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub surface_ids: Vec<String>,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceModel {
    files: Vec<Value>,
    jobs: Vec<Value>,
    selected_job_id: Option<String>,
    public_events: Vec<Value>,
    errors: Vec<Value>,
    review_items: Vec<Value>,
}

#[derive(Clone, Debug, Default)]
struct SurfaceSession {
    surface_ids: Vec<String>,
    model: SurfaceModel,
    state: Value,
    last_action: Option<Value>,
    pending_continuation: Option<PendingContinuation>,
}

#[derive(Clone, Debug)]
struct PendingContinuation {
    host_cursor: String,
    action_id: String,
    worker_cursor: Option<String>,
    chain: SurfaceActionChain,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceActionChain {
    continuations: usize,
    deadline: std::time::Instant,
}

pub(crate) struct AuthorizedSurfaceAction {
    pub worker_payload: Value,
    pub timeout: Duration,
    pub chain: SurfaceActionChain,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkerLaunchConfiguration {
    pub fingerprint: String,
    pub environment: std::collections::BTreeMap<String, String>,
    pub secret_environment: SecretEnv,
    pub runtime_config: Value,
}

impl Default for WorkerLaunchConfiguration {
    fn default() -> Self {
        Self {
            fingerprint: "unconfigured".to_string(),
            environment: std::collections::BTreeMap::new(),
            secret_environment: SecretEnv::default(),
            runtime_config: json!({}),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PluginSurfaceRegistry {
    sessions: HashMap<String, SurfaceSession>,
}

/// Actual process ownership. `Supervisor` records declarative lifecycle facts;
/// this pool owns stdin/stdout and therefore is the only place that reuses a
/// live Python session.
pub struct PluginWorkerSessionRegistry {
    sessions: Mutex<HashMap<String, Arc<WorkerSessionEntry>>>,
    inflight: Mutex<HashMap<String, Arc<AtomicBool>>>,
    next_generation: AtomicU64,
}

struct WorkerSessionEntry {
    generation: u64,
    configuration_fingerprint: String,
    retired: AtomicBool,
    session: Mutex<Option<PythonWorkerSession>>,
}

impl Default for PluginWorkerSessionRegistry {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(1),
        }
    }
}

impl PluginWorkerSessionRegistry {
    pub fn request(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
    ) -> Result<Value, String> {
        self.request_with_host(
            plugin_id,
            plugin_version,
            install_root,
            worker,
            operation,
            payload,
            |operation, _, _| Err(WorkerError::OperationNotAllowed(operation.to_string())),
        )
    }

    pub fn request_with_host<F>(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
        host_call: F,
    ) -> Result<Value, String>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        self.request_with_host_timeout(
            plugin_id,
            plugin_version,
            install_root,
            worker,
            operation,
            payload,
            Duration::from_secs(10),
            host_call,
        )
    }

    pub(crate) fn request_with_host_timeout<F>(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
        timeout: Duration,
        host_call: F,
    ) -> Result<Value, String>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        self.request_with_host_timeout_control(
            plugin_id,
            plugin_version,
            install_root,
            worker,
            operation,
            payload,
            timeout,
            None,
            &WorkerLaunchConfiguration::default(),
            host_call,
        )
    }

    pub(crate) fn request_with_host_launch<F>(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
        timeout: Duration,
        launch: &WorkerLaunchConfiguration,
        host_call: F,
    ) -> Result<Value, String>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        self.request_with_host_timeout_control(
            plugin_id,
            plugin_version,
            install_root,
            worker,
            operation,
            payload,
            timeout,
            None,
            launch,
            host_call,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn request_with_host_control<F>(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
        timeout: Duration,
        control_id: &str,
        host_call: F,
    ) -> Result<Value, String>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        self.request_with_host_control_launch(
            plugin_id,
            plugin_version,
            install_root,
            worker,
            operation,
            payload,
            timeout,
            control_id,
            &WorkerLaunchConfiguration::default(),
            host_call,
        )
    }

    pub(crate) fn request_with_host_control_launch<F>(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
        timeout: Duration,
        control_id: &str,
        launch: &WorkerLaunchConfiguration,
        host_call: F,
    ) -> Result<Value, String>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        let cancellation = Arc::new(AtomicBool::new(false));
        self.inflight
            .lock()
            .map_err(|_| "plugin worker inflight lock is poisoned".to_string())?
            .insert(control_id.to_string(), Arc::clone(&cancellation));
        let result = self.request_with_host_timeout_control(
            plugin_id,
            plugin_version,
            install_root,
            worker,
            operation,
            payload,
            timeout,
            Some(Arc::clone(&cancellation)),
            launch,
            host_call,
        );
        if let Ok(mut inflight) = self.inflight.lock() {
            if inflight
                .get(control_id)
                .is_some_and(|current| Arc::ptr_eq(current, &cancellation))
            {
                inflight.remove(control_id);
            }
        }
        result
    }

    fn request_with_host_timeout_control<F>(
        &self,
        plugin_id: &str,
        plugin_version: &str,
        install_root: &Path,
        worker: &PluginWorkerDescriptor,
        operation: &str,
        payload: Value,
        timeout: Duration,
        cancellation: Option<Arc<AtomicBool>>,
        launch: &WorkerLaunchConfiguration,
        host_call: F,
    ) -> Result<Value, String>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        let key = format!("{plugin_id}@{plugin_version}");
        let mut host_call = host_call;
        loop {
            let (candidate, replaced) = {
                let mut sessions = self
                    .sessions
                    .lock()
                    .map_err(|_| "plugin worker registry lock is poisoned".to_string())?;
                let replaced = sessions
                    .get(&key)
                    .filter(|entry| entry.configuration_fingerprint != launch.fingerprint)
                    .cloned();
                if let Some(entry) = replaced.as_ref() {
                    entry.retired.store(true, Ordering::Release);
                    sessions.remove(&key);
                }
                let candidate = Arc::clone(sessions.entry(key.clone()).or_insert_with(|| {
                    Arc::new(WorkerSessionEntry {
                        generation: self.next_generation.fetch_add(1, Ordering::Relaxed),
                        configuration_fingerprint: launch.fingerprint.clone(),
                        retired: AtomicBool::new(false),
                        session: Mutex::new(None),
                    })
                }));
                (candidate, replaced)
            };
            if let Some(replaced) = replaced {
                if let Ok(mut old_session) = replaced.session.lock() {
                    if let Some(session) = old_session.as_mut() {
                        let _ = session.shutdown();
                    }
                    old_session.take();
                }
            }
            let mut session = candidate
                .session
                .lock()
                .map_err(|_| "plugin worker session lock is poisoned".to_string())?;
            if cancellation
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                let failed = session.take();
                candidate.retired.store(true, Ordering::Release);
                drop(session);
                drop(failed);
                self.remove_entry_if_current(&key, &candidate);
                return Err(format_worker_error(WorkerError::Cancelled(
                    "Host request was cancelled before worker dispatch".to_string(),
                )));
            }
            if candidate.retired.load(Ordering::Acquire) {
                drop(session);
                continue;
            }
            if session.is_none() {
                let python: std::path::PathBuf = std::env::var_os("ANYWAY_PYTHON_EXECUTABLE")
                    .map(Into::into)
                    .unwrap_or_else(|| std::path::PathBuf::from("python"));
                let entrypoint = install_root.join(&worker.entrypoint);
                let worker_id = format!(
                    "plugin-worker-{}-{}-{}",
                    plugin_id, plugin_version, candidate.generation
                );
                let mut config = WorkerSessionConfig::python(
                    python,
                    vec![entrypoint.into_os_string()],
                    Some(install_root.to_path_buf()),
                    plugin_id,
                    plugin_version,
                    worker_id,
                    worker.operations.iter().cloned(),
                );
                config.environment = launch.environment.clone();
                config.secret_environment = launch.secret_environment.clone();
                match PythonWorkerSession::spawn(config) {
                    Ok(spawned) => *session = Some(spawned),
                    Err(error) => {
                        drop(session);
                        self.retire_entry(&key, &candidate);
                        return Err(format_worker_error(error));
                    }
                }
            }
            let result = session
                .as_mut()
                .expect("worker session initialized above")
                .request_with_host_cancel(
                    operation,
                    payload,
                    timeout,
                    cancellation.as_deref(),
                    &mut host_call,
                );
            return match result {
                Ok(value) => Ok(value),
                Err(error) => {
                    if should_retire_session(&error) {
                        let failed = session.take();
                        candidate.retired.store(true, Ordering::Release);
                        drop(session);
                        drop(failed);
                        self.remove_entry_if_current(&key, &candidate);
                    }
                    Err(format_worker_error(error))
                }
            };
        }
    }

    pub fn cancel_request(&self, control_id: &str) -> bool {
        self.inflight
            .lock()
            .ok()
            .and_then(|inflight| inflight.get(control_id).cloned())
            .is_some_and(|flag| {
                flag.store(true, Ordering::Release);
                true
            })
    }

    fn retire_entry(&self, key: &str, entry: &Arc<WorkerSessionEntry>) {
        entry.retired.store(true, Ordering::Release);
        self.remove_entry_if_current(key, entry);
    }

    fn remove_entry_if_current(&self, key: &str, entry: &Arc<WorkerSessionEntry>) {
        if let Ok(mut sessions) = self.sessions.lock() {
            if sessions
                .get(key)
                .is_some_and(|current| Arc::ptr_eq(current, entry))
            {
                sessions.remove(key);
            }
        }
    }

    pub fn shutdown_plugin(&self, plugin_id: &str, plugin_version: &str) {
        let key = format!("{plugin_id}@{plugin_version}");
        let entry = self
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(&key));
        if let Some(entry) = entry {
            entry.retired.store(true, Ordering::Release);
            if let Ok(mut session) = entry.session.lock() {
                if let Some(session) = session.as_mut() {
                    let _ = session.shutdown();
                }
                session.take();
            }
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions
            .lock()
            .map(|sessions| sessions.len())
            .unwrap_or_default()
    }
}

fn should_retire_session(error: &WorkerError) -> bool {
    matches!(
        error,
        WorkerError::Io(_)
            | WorkerError::UnexpectedEof
            | WorkerError::InvalidUtf8
            | WorkerError::InvalidJson(_)
            | WorkerError::Protocol(_)
            | WorkerError::Timeout(_)
            | WorkerError::Cancelled(_)
            | WorkerError::ProcessExited(_)
    )
}

fn format_worker_error(error: WorkerError) -> String {
    match error {
        WorkerError::Remote { code, message } => format!("worker {code}: {message}"),
        other => other.to_string(),
    }
}

impl PluginSurfaceRegistry {
    pub fn cached_state(&self, request: &SurfaceRequest) -> Result<Option<Value>, String> {
        let key = session_key(request)?;
        Ok(self.sessions.get(&key).map(envelope))
    }

    pub(crate) fn append_public_event(
        &mut self,
        request: &SurfaceRequest,
        event: Value,
    ) -> Result<(), String> {
        let object = event
            .as_object()
            .ok_or("public surface event must be an object")?;
        let allowed = [
            "id",
            "jobId",
            "sequence",
            "createdAt",
            "phase",
            "status",
            "summary",
            "evidenceCount",
            "warningCount",
        ];
        if object.keys().any(|key| !allowed.contains(&key.as_str()))
            || contains_forbidden_key(&event)
        {
            return Err("public surface event contains an unapproved field".to_string());
        }
        validate_bounded_value(&event, 0)?;
        let key = session_key(request)?;
        let session = self.session_mut(&key, request)?;
        session.model.public_events.push(event);
        if session.model.public_events.len() > MAX_EVENTS {
            let remove = session.model.public_events.len() - MAX_EVENTS;
            session.model.public_events.drain(0..remove);
        }
        Ok(())
    }

    pub fn state(&mut self, request: &SurfaceRequest) -> Result<Value, String> {
        let key = session_key(request)?;
        let session = self.session_mut(&key, request)?;
        Ok(envelope(session))
    }

    /// Apply only a response created by the worker. Caller fields named event,
    /// result and host-event are rejected before this method is reached.
    pub fn apply_worker_response(
        &mut self,
        request: &SurfaceRequest,
        action_id: &str,
        worker_response: Value,
    ) -> Result<Value, String> {
        self.apply_worker_response_inner(request, action_id, worker_response, None)
    }

    pub(crate) fn authorize_action(
        &mut self,
        request: &SurfaceRequest,
        action_id: &str,
        payload: &Value,
    ) -> Result<AuthorizedSurfaceAction, String> {
        let key = session_key(request)?;
        let session = self.session_mut(&key, request)?;
        let object = payload
            .as_object()
            .ok_or("surface action payload must be an object")?;
        if let Some(cursor) = object.get("cursor") {
            if object.len() != 2 || !object.contains_key("actionId") {
                return Err(
                    "surface continuation payload may contain only actionId and cursor".to_string(),
                );
            }
            let cursor = cursor
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= MAX_CONTINUATION_CURSOR_BYTES)
                .ok_or("surface continuation cursor is invalid")?;
            let pending = session
                .pending_continuation
                .take()
                .ok_or("surface continuation is not pending for this plugin session")?;
            if pending.host_cursor != cursor || pending.action_id != action_id {
                return Err(
                    "surface continuation cursor does not match this plugin session".to_string(),
                );
            }
            let remaining = pending
                .chain
                .deadline
                .saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("surface continuation total deadline expired".to_string());
            }
            let mut worker_payload = json!({ "actionId": pending.action_id });
            if let Some(worker_cursor) = pending.worker_cursor {
                worker_payload["cursor"] = Value::String(worker_cursor);
            }
            return Ok(AuthorizedSurfaceAction {
                worker_payload,
                timeout: remaining.min(SURFACE_WORKER_REQUEST_BUDGET),
                chain: pending.chain,
            });
        }
        session.pending_continuation = None;
        Ok(AuthorizedSurfaceAction {
            worker_payload: payload.clone(),
            timeout: SURFACE_WORKER_REQUEST_BUDGET,
            chain: SurfaceActionChain {
                continuations: 0,
                deadline: std::time::Instant::now() + SURFACE_CONTINUATION_TOTAL_BUDGET,
            },
        })
    }

    pub(crate) fn apply_worker_action_response(
        &mut self,
        request: &SurfaceRequest,
        action_id: &str,
        worker_response: Value,
        chain: SurfaceActionChain,
    ) -> Result<Value, String> {
        self.apply_worker_response_inner(request, action_id, worker_response, Some(chain))
    }

    fn apply_worker_response_inner(
        &mut self,
        request: &SurfaceRequest,
        action_id: &str,
        worker_response: Value,
        chain: Option<SurfaceActionChain>,
    ) -> Result<Value, String> {
        let response = validate_worker_surface_response(worker_response)?;
        let key = session_key(request)?;
        let session = self.session_mut(&key, request)?;
        session.model = response.model;
        session.state = response.state;
        session.last_action = Some(json!({ "actionId": action_id, "pluginId": request.plugin_id }));
        let mut output = envelope(session);
        if let Some(mut event) = response.event {
            if let Some((next_action, worker_cursor)) = continuation_event(&event)? {
                let Some(mut chain) = chain else {
                    return Err(
                        "worker continuation is only valid after a surface action".to_string()
                    );
                };
                let remaining = chain
                    .deadline
                    .saturating_duration_since(std::time::Instant::now());
                if chain.continuations >= MAX_SURFACE_CONTINUATIONS || remaining.is_zero() {
                    session.pending_continuation = None;
                    event = json!({
                        "type": "surface.error",
                        "payload": {
                            "code": if remaining.is_zero() { "SURFACE_CONTINUATION_DEADLINE" } else { "SURFACE_CONTINUATION_LIMIT" },
                            "message": "The bounded surface continuation chain stopped before another worker request.",
                            "stage": "surface.continuation",
                            "retryable": true
                        }
                    });
                } else {
                    chain.continuations += 1;
                    let host_cursor = random_host_continuation_cursor();
                    session.pending_continuation = Some(PendingContinuation {
                        host_cursor: host_cursor.clone(),
                        action_id: next_action.clone(),
                        worker_cursor,
                        chain,
                    });
                    event = json!({
                        "type": "surface.continue",
                        "payload": { "actionId": next_action, "cursor": host_cursor }
                    });
                }
            } else if chain.is_some() {
                session.pending_continuation = None;
            }
            output["event"] = event;
        } else if chain.is_some() {
            session.pending_continuation = None;
        }
        output["accepted"] = Value::Bool(response.accepted);
        Ok(output)
    }

    fn session_mut(
        &mut self,
        key: &str,
        request: &SurfaceRequest,
    ) -> Result<&mut SurfaceSession, String> {
        if !self.sessions.contains_key(key) && self.sessions.len() >= MAX_SESSIONS {
            return Err("plugin surface session limit reached".to_string());
        }
        let entry = self.sessions.entry(key.to_string()).or_default();
        if !request.surface_ids.is_empty() {
            if request.surface_ids.len() > MAX_SURFACES
                || request
                    .surface_ids
                    .iter()
                    .any(|id| id.is_empty() || id.len() > MAX_SESSION_ID_BYTES)
            {
                return Err("invalid plugin surface identifiers".to_string());
            }
            entry.surface_ids = request.surface_ids.clone();
        }
        Ok(entry)
    }
}

pub fn validate_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty()
        || plugin_id.len() > MAX_SESSION_ID_BYTES
        || !plugin_id.is_ascii()
        || !plugin_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err("invalid or untrusted plugin id".to_string());
    }
    Ok(())
}

pub fn validate_caller_payload(payload: &Value) -> Result<String, String> {
    let object = payload
        .as_object()
        .ok_or("surface action payload must be an object")?;
    let action_id = object
        .get("actionId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 160)
        .ok_or("surface actionId is required")?;
    if object
        .keys()
        .any(|key| matches!(key.as_str(), "event" | "result" | "host-event"))
    {
        return Err("surface callers cannot provide worker events or results".to_string());
    }
    if contains_forbidden_key(payload) {
        return Err("surface payload contains a forbidden identity or secret field".to_string());
    }
    let size = serde_json::to_vec(payload)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_ACTION_BYTES || size > MAX_INLINE_BYTES {
        return Err("plugin surface action payload exceeds the inline limit".to_string());
    }
    Ok(action_id.to_string())
}

pub fn validate_caller_host_action(payload: &Value) -> Result<String, String> {
    let object = payload
        .as_object()
        .ok_or("surface host action payload must be an object")?;
    let action_type = object
        .get("type")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 160)
        .ok_or("surface host action type is required")?;
    if object.keys().any(|key| {
        matches!(
            key.as_str(),
            "event"
                | "result"
                | "host-event"
                | "principal"
                | "lease"
                | "leaseId"
                | "capabilityLeaseIds"
                | "localPath"
        )
    }) {
        return Err("surface host action contains a Host-owned field".to_string());
    }
    if contains_forbidden_key(payload) {
        return Err(
            "surface host action contains a forbidden identity or secret field".to_string(),
        );
    }
    let size = serde_json::to_vec(payload)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_ACTION_BYTES || size > MAX_INLINE_BYTES {
        return Err("plugin surface host action exceeds the inline limit".to_string());
    }
    Ok(action_type.to_string())
}

fn session_key(request: &SurfaceRequest) -> Result<String, String> {
    validate_plugin_id(&request.plugin_id)?;
    let key = request.session_id.as_deref().unwrap_or("default");
    if key.is_empty()
        || key.len() > MAX_SESSION_ID_BYTES
        || !key.is_ascii()
        || key.contains(['/', '\\'])
    {
        return Err("invalid plugin surface session id".to_string());
    }
    let version = request.plugin_version.as_deref().unwrap_or("installed");
    if version.is_empty()
        || version.len() > MAX_SESSION_ID_BYTES
        || !version.is_ascii()
        || version.contains(['/', '\\'])
    {
        return Err("invalid plugin surface plugin version".to_string());
    }
    Ok(format!("{}@{}:{}", request.plugin_id, version, key))
}

fn envelope(session: &SurfaceSession) -> Value {
    json!({ "model": session.model, "state": session.state, "surfaceIds": session.surface_ids, "lastAction": session.last_action })
}

struct ValidatedWorkerResponse {
    model: SurfaceModel,
    state: Value,
    event: Option<Value>,
    accepted: bool,
}

fn validate_worker_surface_response(value: Value) -> Result<ValidatedWorkerResponse, String> {
    let object = value
        .as_object()
        .ok_or("worker surface response must be an object")?;
    let allowed = ["model", "state", "event", "accepted"];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("worker surface response contains unknown fields".to_string());
    }
    validate_bounded_value(&value, 0)?;
    let raw_model = object.get("model").cloned().unwrap_or_else(|| json!({}));
    let model_object = raw_model
        .as_object()
        .ok_or("worker model must be an object")?;
    let model_keys = [
        "files",
        "jobs",
        "selectedJobId",
        "publicEvents",
        "errors",
        "reviewItems",
    ];
    if model_object
        .keys()
        .any(|key| !model_keys.contains(&key.as_str()))
    {
        return Err("worker model contains unknown fields".to_string());
    }
    let mut model = SurfaceModel::default();
    for (key, target) in [
        ("files", &mut model.files),
        ("jobs", &mut model.jobs),
        ("publicEvents", &mut model.public_events),
        ("errors", &mut model.errors),
        ("reviewItems", &mut model.review_items),
    ] {
        if let Some(items) = model_object.get(key).and_then(Value::as_array) {
            if items.len() > MAX_MODEL_ITEMS || (key == "publicEvents" && items.len() > MAX_EVENTS)
            {
                return Err(format!("worker model {key} exceeds its item limit"));
            }
            *target = items.clone();
        } else if model_object.contains_key(key) {
            return Err(format!("worker model {key} must be an array"));
        }
    }
    model.selected_job_id = model_object
        .get("selectedJobId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let event = object.get("event").cloned();
    if let Some(event) = event.as_ref() {
        validate_worker_event(event)?;
    }
    Ok(ValidatedWorkerResponse {
        model,
        state: object.get("state").cloned().unwrap_or_else(|| json!({})),
        event,
        accepted: object
            .get("accepted")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

pub(crate) fn worker_continuation_action(value: &Value) -> Result<Option<String>, String> {
    let Some(event) = value.get("event") else {
        return Ok(None);
    };
    validate_worker_event(event)?;
    Ok(continuation_event(event)?.map(|(action_id, _)| action_id))
}

pub(crate) fn worker_review_decision(value: &Value) -> Result<Option<(&str, &str)>, String> {
    let Some(event) = value.get("event") else {
        return Ok(None);
    };
    validate_worker_event(event)?;
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(event_type, "surface.commit" | "surface.reject") {
        return Ok(None);
    }
    let payload = event
        .get("payload")
        .and_then(Value::as_object)
        .ok_or("worker review event payload is required")?;
    Ok(Some((
        payload
            .get("proposalId")
            .and_then(Value::as_str)
            .expect("validated proposal id"),
        payload
            .get("decision")
            .and_then(Value::as_str)
            .expect("validated review decision"),
    )))
}

fn validate_worker_event(event: &Value) -> Result<(), String> {
    let object = event
        .as_object()
        .ok_or("worker surface event must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "payload"))
    {
        return Err("worker surface event contains unknown fields".to_string());
    }
    let event_type = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or("worker surface event type is required")?;
    if !matches!(
        event_type,
        "surface.error"
            | "surface.commit"
            | "surface.reject"
            | "surface.rollback"
            | "surface.continue"
    ) {
        return Err("worker surface event type is not allowed".to_string());
    }
    if event_type == "surface.continue" {
        let payload = object
            .get("payload")
            .and_then(Value::as_object)
            .ok_or("worker surface continuation payload is required")?;
        if payload
            .keys()
            .any(|key| !matches!(key.as_str(), "actionId" | "cursor"))
            || !payload.contains_key("actionId")
        {
            return Err(
                "worker surface continuation may contain only actionId and cursor".to_string(),
            );
        }
        let action_id = payload
            .get("actionId")
            .and_then(Value::as_str)
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 160
                    && value.chars().all(|character| {
                        character.is_ascii_alphanumeric() || ".-_".contains(character)
                    })
            })
            .ok_or("worker surface continuation actionId is invalid")?;
        let _ = action_id;
        if let Some(cursor) = payload.get("cursor") {
            cursor
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= MAX_CONTINUATION_CURSOR_BYTES)
                .ok_or("worker surface continuation cursor is invalid")?;
        }
    } else if matches!(event_type, "surface.commit" | "surface.reject") {
        let payload = object
            .get("payload")
            .and_then(Value::as_object)
            .ok_or("worker review event payload is required")?;
        if payload
            .keys()
            .any(|key| !matches!(key.as_str(), "proposalId" | "decision"))
        {
            return Err("worker review event may contain only proposalId and decision".to_string());
        }
        let proposal_id = payload
            .get("proposalId")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("proposal-") && value.len() <= 80)
            .ok_or("worker review proposalId is invalid")?;
        let _ = proposal_id;
        let expected = if event_type == "surface.commit" {
            "accept"
        } else {
            "reject"
        };
        if payload.get("decision").and_then(Value::as_str) != Some(expected) {
            return Err("worker review event decision does not match its event type".to_string());
        }
    }
    Ok(())
}

fn continuation_event(event: &Value) -> Result<Option<(String, Option<String>)>, String> {
    if event.get("type").and_then(Value::as_str) != Some("surface.continue") {
        return Ok(None);
    }
    validate_worker_event(event)?;
    let payload = event
        .get("payload")
        .and_then(Value::as_object)
        .ok_or("worker surface continuation payload is required")?;
    Ok(Some((
        payload
            .get("actionId")
            .and_then(Value::as_str)
            .expect("validated above")
            .to_string(),
        payload
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_string),
    )))
}

fn random_host_continuation_cursor() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    let mut cursor = String::with_capacity(53);
    cursor.push_str("host-");
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut cursor, "{byte:02x}").expect("writing to a String cannot fail");
    }
    cursor
}

fn contains_forbidden_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "principal"
                        | "apiKey"
                        | "authorization"
                        | "reasoning"
                        | "chainOfThought"
                        | "localPath"
                )
            }) || object.values().any(contains_forbidden_key)
        }
        Value::Array(values) => values.iter().any(contains_forbidden_key),
        _ => false,
    }
}

fn validate_bounded_value(value: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_WORKER_DEPTH {
        return Err("worker response nesting exceeds the limit".to_string());
    }
    let bytes = serde_json::to_vec(value)
        .map_err(|error| error.to_string())?
        .len();
    if bytes > MAX_INLINE_BYTES || contains_forbidden_key(value) {
        return Err("worker response exceeds the inline or secret-field limit".to_string());
    }
    match value {
        Value::String(text) if text.as_bytes().len() > MAX_STRING_BYTES => {
            Err("worker response string is too long".to_string())
        }
        Value::Array(items) => {
            if items.len() > MAX_MODEL_ITEMS {
                return Err("worker response array is too large".to_string());
            }
            items
                .iter()
                .try_for_each(|item| validate_bounded_value(item, depth + 1))
        }
        Value::Object(object) => {
            if object.len() > 48 {
                return Err("worker response object is too large".to_string());
            }
            object.iter().try_for_each(|(key, child)| {
                if key.len() > 128 {
                    return Err("worker response key is too long".to_string());
                }
                validate_bounded_value(child, depth + 1)
            })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, sync::Barrier, thread, time::Instant};

    fn request(plugin_id: &str, version: &str, payload: Option<Value>) -> SurfaceRequest {
        SurfaceRequest {
            plugin_id: plugin_id.to_string(),
            plugin_version: Some(version.to_string()),
            session_id: Some("same".to_string()),
            surface_ids: vec!["surface.same".to_string()],
            payload,
        }
    }

    fn python_worker_fixture() -> Option<(
        tempfile::TempDir,
        std::path::PathBuf,
        PluginWorkerDescriptor,
    )> {
        if Command::new("python").arg("--version").output().is_err() {
            return None;
        }
        let runtime = tempfile::tempdir().expect("lifecycle worker runtime");
        let script = runtime.path().join("lifecycle_worker.py");
        std::fs::write(
            &script,
            r#"import json, os, struct, sys, time
def read():
    header = sys.stdin.buffer.read(4)
    if not header:
        return None
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
operations = sorted(set(hello.get('operations', [])) & {'health', 'sleep', 'crash'})
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':operations})
while True:
    message = read()
    if message is None or message.get('type') == 'shutdown':
        break
    if message.get('type') != 'request':
        continue
    request_id = message['requestId']
    operation = message['operation']
    payload = message.get('payload') or {}
    if operation == 'crash':
        os._exit(42)
    if operation == 'health':
        delay_ms = int(payload.get('delayMs', 0))
        if delay_ms > 0:
            time.sleep(delay_ms / 1000)
        result = {'healthy': True, 'processId': os.getpid()}
    elif operation == 'sleep':
        delay_ms = int(payload.get('delayMs', 0))
        deadline = time.monotonic() + max(0, delay_ms) / 1000
        while time.monotonic() < deadline:
            time.sleep(0.01)
        result = {'slept': True}
    else:
        send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':request_id,'ok':False,'error':{'code':'NOT_ALLOWED','message':'operation is not allowed'}})
        continue
    send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':request_id,'ok':True,'result':result})
"#,
        )
        .expect("write lifecycle worker");
        let install_root = runtime.path().to_path_buf();
        Some((
            runtime,
            install_root,
            PluginWorkerDescriptor {
                id: "default".to_string(),
                language: "python".to_string(),
                entrypoint: "lifecycle_worker.py".to_string(),
                transport: "stdio-framed-json-v1".to_string(),
                host_mediated: true,
                operations: vec![
                    "health".to_string(),
                    "sleep".to_string(),
                    "crash".to_string(),
                ],
                host_operations: Vec::new(),
                provider_egress: Vec::new(),
            },
        ))
    }

    #[test]
    fn worker_response_round_trip_does_not_accept_caller_event() {
        let mut registry = PluginSurfaceRegistry::default();
        let initial = request("plugin.a", "1.0.0", None);
        registry.state(&initial).expect("state");
        let spoof = request(
            "plugin.a",
            "1.0.0",
            Some(json!({"actionId":"review.accept","event":{"type":"surface.commit"}})),
        );
        assert!(validate_caller_payload(spoof.payload.as_ref().unwrap()).is_err());
        let action = request(
            "plugin.a",
            "1.0.0",
            Some(json!({"actionId":"review.accept"})),
        );
        let result = registry.apply_worker_response(&action, "review.accept", json!({"accepted":true,"model":{},"state":{"status":"blocked"},"event":{"type":"surface.error","payload":{"code":"NOT_IMPLEMENTED"}}})).expect("worker response");
        assert_eq!(result["event"]["type"], "surface.error");
        assert_ne!(result["event"]["type"], "surface.commit");
    }

    #[test]
    fn host_published_public_event_is_visible_from_cache_before_worker_response() {
        let mut registry = PluginSurfaceRegistry::default();
        let surface = request("plugin.stream", "1.0.0", None);
        registry.state(&surface).expect("initialize surface cache");
        registry
            .append_public_event(
                &surface,
                json!({
                    "id": "job-1:1",
                    "jobId": "job-1",
                    "sequence": 1,
                    "createdAt": 1,
                    "phase": "model.progress",
                    "status": "published",
                    "summary": "A sanitized incremental frame",
                    "evidenceCount": 0,
                    "warningCount": 0
                }),
            )
            .expect("append Host event while worker request is in flight");
        let cached = registry
            .cached_state(&surface)
            .expect("cache lookup")
            .expect("cached session");
        assert_eq!(
            cached["model"]["publicEvents"][0]["phase"],
            "model.progress"
        );
        assert_eq!(
            cached["model"]["publicEvents"][0]["summary"],
            "A sanitized incremental frame"
        );

        assert!(registry
            .append_public_event(
                &surface,
                json!({
                    "id": "bad",
                    "phase": "model.progress",
                    "authorization": "Bearer hidden"
                })
            )
            .is_err());
    }

    #[test]
    fn cached_public_event_is_readable_while_the_producing_request_is_still_in_flight() {
        let registry = Arc::new(Mutex::new(PluginSurfaceRegistry::default()));
        let surface = request("plugin.stream", "1.0.0", None);
        registry
            .lock()
            .expect("surface registry")
            .state(&surface)
            .expect("initialize surface cache");

        let event_published = Arc::new(Barrier::new(2));
        let allow_response = Arc::new(Barrier::new(2));
        let producer_registry = Arc::clone(&registry);
        let producer_surface = surface.clone();
        let producer_published = Arc::clone(&event_published);
        let producer_response = Arc::clone(&allow_response);
        let producer = thread::spawn(move || {
            producer_registry
                .lock()
                .expect("surface registry")
                .append_public_event(
                    &producer_surface,
                    json!({
                        "id": "job-1:2",
                        "jobId": "job-1",
                        "sequence": 2,
                        "createdAt": 2,
                        "phase": "model.entity",
                        "status": "published",
                        "summary": "Entity frame arrived before the final response",
                        "evidenceCount": 1,
                        "warningCount": 0
                    }),
                )
                .expect("append event from in-flight request");
            producer_published.wait();
            producer_response.wait();
        });

        event_published.wait();
        let cached = registry
            .lock()
            .expect("surface registry")
            .cached_state(&surface)
            .expect("cache lookup")
            .expect("cached session");
        assert_eq!(cached["model"]["publicEvents"][0]["phase"], "model.entity");
        assert_eq!(
            cached["model"]["publicEvents"][0]["summary"],
            "Entity frame arrived before the final response"
        );

        allow_response.wait();
        producer.join().expect("in-flight request thread");
    }

    #[test]
    fn host_replaces_worker_cursor_and_binds_one_time_authorization_to_session() {
        let mut registry = PluginSurfaceRegistry::default();
        let initial = request(
            "plugin.a",
            "1.0.0",
            Some(json!({
                "actionId": "batch.next",
                "capability": "batch.run"
            })),
        );
        let authorized = registry
            .authorize_action(
                &initial,
                "batch.next",
                initial.payload.as_ref().expect("initial payload"),
            )
            .expect("initial action is authorized as a new chain");
        let result = registry
            .apply_worker_action_response(
                &initial,
                "batch.next",
                json!({
                    "accepted": true,
                    "model": {},
                    "state": {},
                    "event": {
                        "type": "surface.continue",
                        "payload": { "actionId": "batch.next", "cursor": "worker-fixed-cursor" }
                    }
                }),
                authorized.chain,
            )
            .expect("worker continuation is accepted");
        let host_cursor = result["event"]["payload"]["cursor"]
            .as_str()
            .expect("Host cursor")
            .to_string();
        assert_ne!(host_cursor, "worker-fixed-cursor");
        assert!(host_cursor.starts_with("host-"));

        let cross_session = SurfaceRequest {
            session_id: Some("other".to_string()),
            payload: Some(json!({"actionId":"batch.next","cursor":host_cursor})),
            ..initial.clone()
        };
        assert!(registry
            .authorize_action(
                &cross_session,
                "batch.next",
                cross_session.payload.as_ref().unwrap(),
            )
            .is_err());

        let continuation = SurfaceRequest {
            payload: Some(json!({"actionId":"batch.next","cursor":host_cursor})),
            ..initial.clone()
        };
        let resumed = registry
            .authorize_action(
                &continuation,
                "batch.next",
                continuation.payload.as_ref().unwrap(),
            )
            .expect("correct Host cursor resumes the bound session");
        assert_eq!(
            resumed.worker_payload,
            json!({
                "actionId": "batch.next",
                "cursor": "worker-fixed-cursor"
            })
        );
        assert!(!serde_json::to_string(&resumed.worker_payload)
            .unwrap()
            .contains(&host_cursor));
        assert!(
            registry
                .authorize_action(
                    &continuation,
                    "batch.next",
                    continuation.payload.as_ref().unwrap(),
                )
                .is_err(),
            "Host cursor is one-time and replay must fail"
        );
    }

    #[test]
    fn wrong_host_continuation_cursor_is_rejected_and_consumed() {
        let mut registry = PluginSurfaceRegistry::default();
        let initial = request(
            "plugin.a",
            "1.0.0",
            Some(json!({"actionId":"batch.next","capability":"batch.run"})),
        );
        let authorized = registry
            .authorize_action(&initial, "batch.next", initial.payload.as_ref().unwrap())
            .unwrap();
        let result = registry.apply_worker_action_response(
            &initial,
            "batch.next",
            json!({"model":{},"state":{},"event":{"type":"surface.continue","payload":{"actionId":"batch.next"}}}),
            authorized.chain,
        ).unwrap();
        let correct = result["event"]["payload"]["cursor"]
            .as_str()
            .unwrap()
            .to_string();
        let wrong = SurfaceRequest {
            payload: Some(json!({"actionId":"batch.next","cursor":"host-wrong"})),
            ..initial.clone()
        };
        assert!(registry
            .authorize_action(&wrong, "batch.next", wrong.payload.as_ref().unwrap())
            .is_err());
        let correct_request = SurfaceRequest {
            payload: Some(json!({"actionId":"batch.next","cursor":correct})),
            ..initial
        };
        assert!(registry
            .authorize_action(
                &correct_request,
                "batch.next",
                correct_request.payload.as_ref().unwrap()
            )
            .is_err());
    }

    #[test]
    fn two_plugins_with_same_surface_and_session_are_isolated() {
        let mut registry = PluginSurfaceRegistry::default();
        for plugin in ["plugin.a", "plugin.b"] {
            let request = request(plugin, "1.0.0", Some(json!({"actionId":"surface.ping"})));
            registry
                .apply_worker_response(
                    &request,
                    "surface.ping",
                    json!({"accepted":true,"model":{"selectedJobId":plugin},"state":{}}),
                )
                .expect("response");
        }
        assert_eq!(
            registry.state(&request("plugin.a", "1.0.0", None)).unwrap()["model"]["selectedJobId"],
            "plugin.a"
        );
        assert_eq!(
            registry.state(&request("plugin.b", "1.0.0", None)).unwrap()["model"]["selectedJobId"],
            "plugin.b"
        );
    }

    #[test]
    fn rejects_traversal_and_oversized_worker_model() {
        assert!(session_key(&request("plugin.a", "../1", None)).is_err());
        assert!(validate_worker_surface_response(
            json!({"model":{"files":["x".repeat(MAX_INLINE_BYTES)]},"state":{}})
        )
        .is_err());
    }

    #[test]
    fn different_plugin_workers_execute_in_parallel() {
        if Command::new("python").arg("--version").output().is_err() {
            return;
        }
        let runtime = tempfile::tempdir().expect("barrier worker runtime");
        let script = runtime.path().join("barrier_worker.py");
        std::fs::write(
            &script,
            r#"import json, os, struct, sys, time
def read():
    header = sys.stdin.buffer.read(4)
    if not header: return None
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':['barrier']})
while True:
    message = read()
    if message is None or message.get('type') == 'shutdown': break
    if message.get('type') != 'request': continue
    payload = message['payload']
    open(payload['readyFile'], 'wb').close()
    deadline = time.monotonic() + 2.0
    while not os.path.exists(payload['peerReadyFile']) and time.monotonic() < deadline:
        time.sleep(0.005)
    if os.path.exists(payload['peerReadyFile']):
        send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':message['requestId'],'ok':True,'result':{'metPeer':True}})
    else:
        send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':message['requestId'],'ok':False,'error':{'code':'BARRIER_TIMEOUT','message':'peer worker could not run concurrently'}})
"#,
        )
        .expect("write barrier worker");
        let worker = PluginWorkerDescriptor {
            id: "default".to_string(),
            language: "python".to_string(),
            entrypoint: "barrier_worker.py".to_string(),
            transport: "stdio-framed-json-v1".to_string(),
            host_mediated: true,
            operations: vec!["barrier".to_string()],
            host_operations: Vec::new(),
            provider_egress: Vec::new(),
        };
        let registry = Arc::new(PluginWorkerSessionRegistry::default());
        let barrier = Arc::new(Barrier::new(3));
        let ready_a = runtime.path().join("a.ready");
        let ready_b = runtime.path().join("b.ready");
        let mut threads = Vec::new();
        for (plugin_id, ready, peer) in [
            ("plugin.parallel.a", ready_a.clone(), ready_b.clone()),
            ("plugin.parallel.b", ready_b, ready_a),
        ] {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            let install_root = runtime.path().to_path_buf();
            let worker = worker.clone();
            threads.push(thread::spawn(move || {
                barrier.wait();
                registry.request(
                    plugin_id,
                    "1.0.0",
                    &install_root,
                    &worker,
                    "barrier",
                    json!({"readyFile": ready, "peerReadyFile": peer}),
                )
            }));
        }
        barrier.wait();
        for handle in threads {
            handle
                .join()
                .expect("worker request thread")
                .expect("parallel worker response");
        }
        assert_eq!(registry.session_count(), 2);
        registry.shutdown_plugin("plugin.parallel.a", "1.0.0");
        registry.shutdown_plugin("plugin.parallel.b", "1.0.0");
    }

    #[test]
    fn requests_to_the_same_worker_are_sequential() {
        let Some((_fixture, install_root, worker)) = python_worker_fixture() else {
            return;
        };
        let registry = Arc::new(PluginWorkerSessionRegistry::default());
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            let install_root = install_root.clone();
            let worker = worker.clone();
            threads.push(thread::spawn(move || {
                barrier.wait();
                registry.request(
                    "plugin.sequential",
                    "1.0.0",
                    &install_root,
                    &worker,
                    "health",
                    json!({"delayMs": 400}),
                )
            }));
        }
        barrier.wait();
        let started = Instant::now();
        for handle in threads {
            handle
                .join()
                .expect("worker request thread")
                .expect("sequential worker response");
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(750),
            "same worker requests overlapped unexpectedly: {elapsed:?}"
        );
        assert_eq!(registry.session_count(), 1);
        registry.shutdown_plugin("plugin.sequential", "1.0.0");
    }

    #[test]
    fn crashed_and_timed_out_worker_instances_are_retired_before_respawn() {
        let Some((_fixture, install_root, worker)) = python_worker_fixture() else {
            return;
        };
        let registry = PluginWorkerSessionRegistry::default();
        let crash = registry.request(
            "plugin.recovery",
            "1.0.0",
            &install_root,
            &worker,
            "crash",
            json!({}),
        );
        assert!(crash
            .as_ref()
            .is_err_and(|message| message.contains("process exited")));
        assert_eq!(
            registry.session_count(),
            0,
            "crashed entry must be removed by identity"
        );
        assert_eq!(
            registry
                .request(
                    "plugin.recovery",
                    "1.0.0",
                    &install_root,
                    &worker,
                    "health",
                    json!({}),
                )
                .expect("worker respawns after crash")["healthy"],
            true
        );

        let timeout = registry.request_with_host_timeout(
            "plugin.recovery",
            "1.0.0",
            &install_root,
            &worker,
            "sleep",
            json!({"delayMs": 500}),
            Duration::from_millis(20),
            |operation, _, _| Err(WorkerError::OperationNotAllowed(operation.to_string())),
        );
        assert!(timeout
            .as_ref()
            .is_err_and(|message| message.contains("timeout")));
        assert_eq!(
            registry.session_count(),
            0,
            "timed-out entry must be removed by identity"
        );
        assert_eq!(
            registry
                .request(
                    "plugin.recovery",
                    "1.0.0",
                    &install_root,
                    &worker,
                    "health",
                    json!({}),
                )
                .expect("worker respawns after timeout")["healthy"],
            true
        );
        registry.shutdown_plugin("plugin.recovery", "1.0.0");
    }

    #[test]
    fn host_request_cancellation_interrupts_and_retires_the_running_worker() {
        let Some((_fixture, install_root, worker)) = python_worker_fixture() else {
            return;
        };
        let registry = Arc::new(PluginWorkerSessionRegistry::default());
        let request_registry = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            request_registry.request_with_host_control(
                "plugin.cancel",
                "1.0.0",
                &install_root,
                &worker,
                "sleep",
                json!({"delayMs": 5_000}),
                Duration::from_secs(10),
                "host-request-cancel-1",
                |operation, _, _| Err(WorkerError::OperationNotAllowed(operation.to_string())),
            )
        });
        let mut observed = false;
        for _ in 0..100 {
            if registry.cancel_request("host-request-cancel-1") {
                observed = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            observed,
            "request must register its Host cancellation token"
        );
        let result = handle.join().expect("cancelled request thread");
        assert!(result
            .as_ref()
            .is_err_and(|message| message.contains("cancel")));
        assert_eq!(
            registry.session_count(),
            0,
            "cancelled worker generation must be retired"
        );
        assert!(
            !registry.cancel_request("host-request-cancel-1"),
            "completed cancellation tokens must be removed"
        );
    }

    #[test]
    fn provider_secret_fingerprint_rotates_process_without_leaking_values() {
        use sha2::{Digest as _, Sha256};
        if Command::new("python").arg("--version").output().is_err() {
            return;
        }
        let runtime = tempfile::tempdir().expect("secret worker runtime");
        let script = runtime.path().join("secret_worker.py");
        std::fs::write(&script, r#"import hashlib, json, os, pathlib, struct, sys
def read():
    header = sys.stdin.buffer.read(4)
    if not header: return None
    size = struct.unpack('>I', header)[0]
    return json.loads(sys.stdin.buffer.read(size))
def send(value):
    payload = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(payload)) + payload)
    sys.stdout.buffer.flush()
hello = read()
send({'type':'helloAck','apiVersion':'researchcanvas.dev/worker-rpc/v1','workerId':hello['workerId'],'operations':['probe']})
try:
    while True:
        message = read()
        if message is None or message.get('type') == 'shutdown': break
        if message.get('type') != 'request': continue
        secret = os.environ.get('ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY')
        digest = hashlib.sha256(secret.encode()).hexdigest() if secret is not None else None
        send({'type':'response','apiVersion':'researchcanvas.dev/worker-rpc/v1','requestId':message['requestId'],'ok':True,'result':{'pid':os.getpid(),'configured':secret is not None,'matches':secret is not None and digest == message['payload'].get('expectedDigest')}})
finally:
    marker = os.environ.get('EXIT_MARKER')
    if marker: pathlib.Path(marker).write_text(str(os.getpid()), encoding='utf-8')
"#).expect("write secret worker");
        let worker = PluginWorkerDescriptor {
            id: "default".to_string(),
            language: "python".to_string(),
            entrypoint: "secret_worker.py".to_string(),
            transport: "stdio-framed-json-v1".to_string(),
            host_mediated: true,
            operations: vec!["probe".to_string()],
            host_operations: Vec::new(),
            provider_egress: Vec::new(),
        };
        let registry = PluginWorkerSessionRegistry::default();
        let secret_name = "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY";
        let fingerprint = |source: &str, secret: Option<&str>| {
            let mut hash = Sha256::new();
            hash.update(source.as_bytes());
            if let Some(secret) = secret {
                hash.update(secret.as_bytes());
            }
            format!("{:x}", hash.finalize())
        };
        let digest = |secret: &str| format!("{:x}", Sha256::digest(secret.as_bytes()));
        let launch = |source: &str, secret: Option<&str>, marker: &std::path::Path| {
            let mut secret_environment = SecretEnv::default();
            if let Some(secret) = secret {
                secret_environment.insert(secret_name.to_string(), secret.to_string());
            }
            WorkerLaunchConfiguration {
                fingerprint: fingerprint(source, secret),
                environment: std::collections::BTreeMap::from([(
                    "EXIT_MARKER".to_string(),
                    marker.to_string_lossy().to_string(),
                )]),
                secret_environment,
                runtime_config: json!({"credentialSource": source, "secretConfigured": secret.is_some()}),
            }
        };
        let cases = [
            ("host-secret", Some("old-host-secret")),
            ("host-secret", Some("new-host-secret")),
            ("environment", Some("environment-secret")),
            ("reset", None),
        ];
        let mut previous_pid = None;
        let mut previous_marker: Option<std::path::PathBuf> = None;
        for (index, (source, secret)) in cases.into_iter().enumerate() {
            let marker = runtime.path().join(format!("worker-{index}.exited"));
            let launch = launch(source, secret, &marker);
            let expected = secret.map(&digest);
            let response = registry
                .request_with_host_launch(
                    "plugin.secret-lifecycle",
                    "1.0.0",
                    runtime.path(),
                    &worker,
                    "probe",
                    json!({"expectedDigest": expected}),
                    Duration::from_secs(3),
                    &launch,
                    |operation, _, _| Err(WorkerError::OperationNotAllowed(operation.to_string())),
                )
                .expect("secret lifecycle worker request");
            assert_eq!(response["configured"], secret.is_some());
            assert_eq!(response["matches"], secret.is_some());
            let pid = response["pid"].as_u64().expect("worker pid");
            if let Some(previous_pid) = previous_pid {
                assert_ne!(
                    pid, previous_pid,
                    "configuration fingerprint must replace the worker process"
                );
                assert!(
                    previous_marker.as_ref().unwrap().exists(),
                    "old worker must exit before replacement is used"
                );
            }
            let wire = serde_json::to_string(&response).unwrap();
            for forbidden in ["old-host-secret", "new-host-secret", "environment-secret"] {
                assert!(!wire.contains(forbidden));
            }
            previous_pid = Some(pid);
            previous_marker = Some(marker);
        }
        registry.shutdown_plugin("plugin.secret-lifecycle", "1.0.0");
        assert!(
            previous_marker.unwrap().exists(),
            "reset worker exits on shutdown"
        );
    }
}
