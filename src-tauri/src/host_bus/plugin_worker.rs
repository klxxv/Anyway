//! Generic plugin worker RPC service facade.
//!
//! This module owns plugin worker sessions for `plugin.worker.*` operations.
//! The concrete transport is the shared stdio framed JSON adapter; Python is
//! just one launcher shape, not a Host boundary.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ffi::OsString,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use crate::host_bus::python_worker::{
    SecretEnv, StdioFramedWorkerSession, WorkerError, WorkerSessionConfig, WORKER_TRANSPORT_ID,
};

const MAX_OPEN_SESSIONS: usize = 64;
const MAX_SESSION_CALLS: usize = 8;
const DEFAULT_CALL_DEADLINE_MS: u64 = 30_000;
const MAX_CALL_DEADLINE_MS: u64 = 10 * 60_000;

#[derive(Clone, Debug)]
pub struct PluginWorkerCommand {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub working_directory: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct PluginWorkerLaunchPlan {
    pub plugin_id: String,
    pub plugin_version: String,
    pub worker_id: String,
    pub language: String,
    pub transport: String,
    pub entrypoint: String,
    pub command: PluginWorkerCommand,
    pub allowed_operations: Vec<String>,
    pub host_operations: Vec<String>,
    pub fingerprint: String,
    pub environment: BTreeMap<String, String>,
    pub secret_environment: SecretEnv,
}

impl PluginWorkerLaunchPlan {
    pub fn fingerprint_for(
        plugin_id: &str,
        plugin_version: &str,
        worker_id: &str,
        language: &str,
        transport: &str,
        entrypoint: &str,
        entry_bytes: &[u8],
    ) -> String {
        let mut hash = Sha256::new();
        hash.update(plugin_id.as_bytes());
        hash.update(b"\0");
        hash.update(plugin_version.as_bytes());
        hash.update(b"\0");
        hash.update(worker_id.as_bytes());
        hash.update(b"\0");
        hash.update(language.as_bytes());
        hash.update(b"\0");
        hash.update(transport.as_bytes());
        hash.update(b"\0");
        hash.update(entrypoint.as_bytes());
        hash.update(b"\0");
        hash.update(entry_bytes);
        format!("{:x}", hash.finalize())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerOpenRequest {
    pub plugin_id: String,
    pub plugin_version: String,
    pub worker_id: String,
    pub session_id: String,
    #[serde(default)]
    pub deadline_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerOpenResponse {
    pub plugin_id: String,
    pub plugin_version: String,
    pub worker_id: String,
    pub session_id: String,
    pub fingerprint: String,
    pub transport: String,
    pub language: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerCallRequest {
    pub session_id: String,
    pub request_id: String,
    pub operation: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub deadline_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerCallResponse {
    pub session_id: String,
    pub request_id: String,
    pub result: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerCancelRequest {
    pub session_id: String,
    pub request_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerCloseRequest {
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkerCloseResponse {
    pub session_id: String,
    pub closed: bool,
}

struct SessionRecord {
    plugin_id: String,
    plugin_version: String,
    worker_id: String,
    fingerprint: String,
    allowed_host_operations: BTreeSet<String>,
    session: Mutex<StdioFramedWorkerSession>,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    active_calls: Mutex<HashSet<String>>,
}

#[derive(Default)]
pub struct PluginWorkerManager {
    sessions: Mutex<HashMap<String, Arc<SessionRecord>>>,
}

impl PluginWorkerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(
        &self,
        request: PluginWorkerOpenRequest,
        plan: PluginWorkerLaunchPlan,
    ) -> Result<PluginWorkerOpenResponse, WorkerError> {
        validate_id(&request.plugin_id, "plugin id")?;
        validate_id(&request.plugin_version, "plugin version")?;
        validate_id(&request.worker_id, "worker id")?;
        validate_id(&request.session_id, "worker session id")?;
        if request.plugin_id != plan.plugin_id
            || request.plugin_version != plan.plugin_version
            || request.worker_id != plan.worker_id
        {
            return Err(WorkerError::Protocol(
                "worker launch plan identity mismatch".to_string(),
            ));
        }
        if plan.transport != WORKER_TRANSPORT_ID {
            return Err(WorkerError::Protocol(format!(
                "unsupported worker transport: {}",
                plan.transport
            )));
        }

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| WorkerError::Protocol("worker session registry poisoned".to_string()))?;
        if sessions.len() >= MAX_OPEN_SESSIONS && !sessions.contains_key(&request.session_id) {
            return Err(WorkerError::Protocol(
                "plugin worker session limit exceeded".to_string(),
            ));
        }
        if sessions.contains_key(&request.session_id) {
            return Err(WorkerError::Protocol(format!(
                "duplicate plugin worker session: {}",
                request.session_id
            )));
        }

        let mut config = WorkerSessionConfig::stdio(
            plan.command.executable.clone(),
            plan.command.args.clone(),
            plan.command.working_directory.clone(),
            plan.language.clone(),
            plan.plugin_id.clone(),
            plan.plugin_version.clone(),
            plan.worker_id.clone(),
            request.session_id.clone(),
            plan.fingerprint.clone(),
            plan.allowed_operations.clone(),
        );
        config.environment = plan.environment.clone();
        config.secret_environment = plan.secret_environment.clone();
        if let Some(deadline_ms) = request.deadline_ms {
            config.handshake_timeout = bounded_deadline(deadline_ms);
        }
        let session = StdioFramedWorkerSession::spawn(config)?;
        let record = Arc::new(SessionRecord {
            plugin_id: plan.plugin_id.clone(),
            plugin_version: plan.plugin_version.clone(),
            worker_id: plan.worker_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            allowed_host_operations: plan.host_operations.into_iter().collect(),
            session: Mutex::new(session),
            cancellations: Mutex::new(HashMap::new()),
            active_calls: Mutex::new(HashSet::new()),
        });
        sessions.insert(request.session_id.clone(), record);

        Ok(PluginWorkerOpenResponse {
            plugin_id: plan.plugin_id,
            plugin_version: plan.plugin_version,
            worker_id: plan.worker_id,
            session_id: request.session_id,
            fingerprint: plan.fingerprint,
            transport: plan.transport,
            language: plan.language,
        })
    }

    pub fn call<F>(
        &self,
        request: PluginWorkerCallRequest,
        mut host_call: F,
    ) -> Result<PluginWorkerCallResponse, WorkerError>
    where
        F: FnMut(&str, Value, Duration) -> Result<Value, WorkerError>,
    {
        validate_id(&request.session_id, "worker session id")?;
        validate_id(&request.request_id, "worker request id")?;
        let record = self.session(&request.session_id)?;
        {
            let mut active = record
                .active_calls
                .lock()
                .map_err(|_| WorkerError::Protocol("worker call registry poisoned".to_string()))?;
            if active.len() >= MAX_SESSION_CALLS {
                return Err(WorkerError::Protocol(
                    "plugin worker concurrent call limit exceeded".to_string(),
                ));
            }
            if !active.insert(request.request_id.clone()) {
                return Err(WorkerError::Protocol(format!(
                    "duplicate worker request id: {}",
                    request.request_id
                )));
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        record
            .cancellations
            .lock()
            .map_err(|_| WorkerError::Protocol("worker cancel registry poisoned".to_string()))?
            .insert(request.request_id.clone(), cancel.clone());

        let timeout = bounded_deadline(request.deadline_ms.unwrap_or(DEFAULT_CALL_DEADLINE_MS));
        let result = record
            .session
            .lock()
            .map_err(|_| WorkerError::Protocol("worker session lock poisoned".to_string()))?
            .request_with_host_cancel_id(
                &request.operation,
                request.payload,
                timeout,
                Some(cancel.as_ref()),
                Some(&request.request_id),
                |operation, payload, remaining| {
                    if !record.allowed_host_operations.contains(operation) {
                        return Err(WorkerError::OperationNotAllowed(operation.to_string()));
                    }
                    host_call(operation, payload, remaining)
                },
            );

        record
            .cancellations
            .lock()
            .map_err(|_| WorkerError::Protocol("worker cancel registry poisoned".to_string()))?
            .remove(&request.request_id);
        record
            .active_calls
            .lock()
            .map_err(|_| WorkerError::Protocol("worker call registry poisoned".to_string()))?
            .remove(&request.request_id);

        match result {
            Ok(value) => Ok(PluginWorkerCallResponse {
                session_id: request.session_id,
                request_id: request.request_id,
                result: value,
            }),
            Err(error @ WorkerError::ProcessExited(_)) => {
                self.drop_session(&request.session_id);
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    pub fn cancel(&self, request: PluginWorkerCancelRequest) -> Result<Value, WorkerError> {
        validate_id(&request.session_id, "worker session id")?;
        validate_id(&request.request_id, "worker request id")?;
        let record = self.session(&request.session_id)?;
        let cancelled = record
            .cancellations
            .lock()
            .map_err(|_| WorkerError::Protocol("worker cancel registry poisoned".to_string()))?
            .get(&request.request_id)
            .map(|flag| {
                flag.store(true, Ordering::Release);
                true
            })
            .unwrap_or(false);
        Ok(serde_json::json!({
            "sessionId": request.session_id,
            "requestId": request.request_id,
            "cancelled": cancelled,
        }))
    }

    pub fn close(
        &self,
        request: PluginWorkerCloseRequest,
    ) -> Result<PluginWorkerCloseResponse, WorkerError> {
        validate_id(&request.session_id, "worker session id")?;
        let record = self.drop_session(&request.session_id);
        let closed = record.is_some();
        if let Some(record) = record {
            let _ = record
                .session
                .lock()
                .map_err(|_| WorkerError::Protocol("worker session lock poisoned".to_string()))?
                .shutdown();
        }
        Ok(PluginWorkerCloseResponse {
            session_id: request.session_id,
            closed,
        })
    }

    pub fn close_plugin(&self, plugin_id: &str, plugin_version: Option<&str>) -> usize {
        let removed = {
            let mut sessions = match self.sessions.lock() {
                Ok(sessions) => sessions,
                Err(_) => return 0,
            };
            let ids = sessions
                .iter()
                .filter(|(_, record)| {
                    record.plugin_id == plugin_id
                        && plugin_version.is_none_or(|version| record.plugin_version == version)
                })
                .map(|(session_id, _)| session_id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|session_id| sessions.remove(&session_id))
                .collect::<Vec<_>>()
        };
        let count = removed.len();
        for record in removed {
            if let Ok(mut session) = record.session.lock() {
                let _ = session.shutdown();
            }
        }
        count
    }

    pub fn snapshot(&self) -> Result<Value, WorkerError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| WorkerError::Protocol("worker session registry poisoned".to_string()))?;
        Ok(Value::Array(
            sessions
                .iter()
                .map(|(session_id, record)| {
                    serde_json::json!({
                        "sessionId": session_id,
                        "pluginId": record.plugin_id,
                        "pluginVersion": record.plugin_version,
                        "workerId": record.worker_id,
                        "fingerprint": record.fingerprint,
                    })
                })
                .collect(),
        ))
    }

    fn session(&self, session_id: &str) -> Result<Arc<SessionRecord>, WorkerError> {
        self.sessions
            .lock()
            .map_err(|_| WorkerError::Protocol("worker session registry poisoned".to_string()))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| WorkerError::Protocol(format!("unknown worker session: {session_id}")))
    }

    fn drop_session(&self, session_id: &str) -> Option<Arc<SessionRecord>> {
        self.sessions.lock().ok()?.remove(session_id)
    }
}

fn bounded_deadline(deadline_ms: u64) -> Duration {
    Duration::from_millis(deadline_ms.clamp(1, MAX_CALL_DEADLINE_MS))
}

fn validate_id(value: &str, label: &str) -> Result<(), WorkerError> {
    if value.is_empty()
        || value.len() > 160
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err(WorkerError::Protocol(format!("invalid {label}: {value}")));
    }
    Ok(())
}
