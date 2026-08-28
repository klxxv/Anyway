//! Host bus worker lifecycle: `worker.spawn` + `worker.stop`.
//!
//! Declarative registration against the kernel `Supervisor`, plus the concrete
//! Python stdio worker transport re-exported below. The supervisor stays
//! lock-free; the kernel's `RwLock<Supervisor>` is held by the caller.

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel::identity::{PrincipalId, WorkerId};
use crate::kernel::lifecycle::LifecycleSpec;
use crate::kernel::supervisor::{Supervisor, SupervisorAction, WorkerSpec};
use crate::kernel_commands::{inline_request, HostCallRequest};

#[path = "plugin_worker.rs"]
pub mod plugin_worker;

pub use super::python_worker::{
    decode_frame, encode_frame, read_frame, validate_hello_ack, write_frame, PythonWorkerSession,
    SecretEnv, WorkerError, WorkerSessionConfig, MAX_ERROR_MESSAGE_BYTES, MAX_EVENTS_PER_REQUEST,
    MAX_FRAME_BYTES, MAX_HOST_CALLS_PER_REQUEST, MAX_INLINE_BYTES, WORKER_RPC_API_VERSION,
};
pub use plugin_worker::{
    PluginWorkerCallRequest, PluginWorkerCallResponse, PluginWorkerCancelRequest,
    PluginWorkerCloseRequest, PluginWorkerCloseResponse, PluginWorkerCommand,
    PluginWorkerLaunchPlan, PluginWorkerManager, PluginWorkerOpenRequest, PluginWorkerOpenResponse,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerSpawnRequest {
    pub worker_id: String,
    pub package_id: String,
    pub package_version: String,
    pub entrypoint: String,
    /// `true` registers an external-process worker (isolated); `false` uses the
    /// shared thread pool.
    #[serde(default)]
    pub isolated: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStopRequest {
    pub worker_id: String,
}

fn action_label(action: &SupervisorAction) -> &'static str {
    match action {
        SupervisorAction::Start { .. } => "start",
        SupervisorAction::Stop { .. } => "stop",
        SupervisorAction::Restart { .. } => "restart",
        SupervisorAction::Quarantine { .. } => "quarantine",
        SupervisorAction::ReportFailure { .. } => "report-failure",
        SupervisorAction::Noop => "noop",
    }
}

/// `worker.spawn` — register and start one supervised worker.
pub fn dispatch_worker_spawn(
    request: &HostCallRequest,
    supervisor: &RwLock<Supervisor>,
) -> Result<Value, String> {
    let spawn = inline_request::<WorkerSpawnRequest>(request)
        .map_err(|error| format!("invalid worker.spawn request: {error}"))?;
    let worker_id = WorkerId::new(spawn.worker_id.clone()).map_err(|error| error.to_string())?;
    let principal = PrincipalId::new(format!("plugin.{}", spawn.package_id))
        .map_err(|error| error.to_string())?;
    let spec = if spawn.isolated {
        WorkerSpec::external_process(
            worker_id.clone(),
            principal,
            None,
            spawn.entrypoint.clone(),
            LifecycleSpec::default(),
        )
    } else {
        WorkerSpec::thread_pool(
            worker_id.clone(),
            principal,
            None,
            "default",
            LifecycleSpec::default(),
        )
    };
    let mut guard = supervisor
        .write()
        .map_err(|_| "supervisor lock is poisoned".to_string())?;
    guard
        .register(spec)
        .map_err(|error| format!("worker.spawn failed: {error}"))?;
    let action = guard
        .start(&worker_id)
        .map_err(|error| format!("worker.spawn failed: {error}"))?;
    let snapshot = guard
        .snapshot(&worker_id)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "workerId": worker_id.as_str(),
        "state": format!("{:?}", snapshot.state).to_lowercase(),
        "action": action_label(&action),
        "domain": format!("{:?}", snapshot.domain).to_lowercase(),
    }))
}

/// `worker.stop` — request a graceful stop for one supervised worker.
pub fn dispatch_worker_stop(
    request: &HostCallRequest,
    supervisor: &RwLock<Supervisor>,
) -> Result<Value, String> {
    let stop = inline_request::<WorkerStopRequest>(request)
        .map_err(|error| format!("invalid worker.stop request: {error}"))?;
    let worker_id = WorkerId::new(stop.worker_id).map_err(|error| error.to_string())?;
    let mut guard = supervisor
        .write()
        .map_err(|_| "supervisor lock is poisoned".to_string())?;
    let action = guard
        .request_stop(&worker_id)
        .map_err(|error| format!("worker.stop failed: {error}"))?;
    Ok(json!({
        "workerId": worker_id.as_str(),
        "stopped": true,
        "forced": false,
        "action": action_label(&action),
    }))
}
