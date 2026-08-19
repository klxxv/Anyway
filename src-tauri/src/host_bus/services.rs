//! Host bus service lifecycle: `service.list` + `service.unregister`.
//!
//! These extend the AnCordis host-bus service registry surface. The registry
//! itself stays lock-free; the kernel's `RwLock<ServiceRegistry>` is held by
//! the caller (see `kernel_commands::dispatch`).

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel::service_registry::ServiceRegistry;
use crate::kernel_commands::{inline_request, HostCallRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUnregisterRequest {
    pub service_id: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// `service.list` — snapshot every non-expired registration.
pub fn dispatch_service_list(
    services: &RwLock<ServiceRegistry>,
) -> Result<Value, String> {
    let registry = services
        .read()
        .map_err(|_| "service registry lock is poisoned".to_string())?;
    serde_json::to_value(registry.list(now_ms())).map_err(|error| error.to_string())
}

/// `service.unregister` — remove one registration; a missing service is an
/// error so callers can tell "already gone" from "removed now".
pub fn dispatch_service_unregister(
    request: &HostCallRequest,
    services: &RwLock<ServiceRegistry>,
) -> Result<Value, String> {
    let unregister = inline_request::<ServiceUnregisterRequest>(request)
        .map_err(|error| format!("invalid service.unregister request: {error}"))?;
    let mut registry = services
        .write()
        .map_err(|_| "service registry lock is poisoned".to_string())?;
    registry
        .unregister(&unregister.service_id)
        .map_err(|error| format!("service.unregister failed: {error}"))?;
    Ok(json!({ "serviceId": unregister.service_id, "removed": true }))
}
