//! Host bus lease lifecycle: `lease.renew`.
//!
//! Extends a capability lease's expiry. Kernel-only; plugin leases must keep
//! an expiry. The lease store stays behind the kernel's `RwLock`; the caller
//! holds the lock (see `kernel_commands::dispatch`).

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use crate::kernel::policy::CapabilityPolicy;
use crate::kernel_commands::{inline_request, HostCallRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseRenewRequest {
    pub lease_id: u64,
    /// New expiry in epoch milliseconds; `null` clears the expiry (native UI
    /// principal only).
    pub expires_at: Option<u64>,
}

/// `lease.renew` — extend one active lease.
pub fn dispatch_lease_renew(
    request: &HostCallRequest,
    policy: &RwLock<CapabilityPolicy>,
    now_ms: u64,
) -> Result<Value, String> {
    let renew = inline_request::<LeaseRenewRequest>(request)
        .map_err(|error| format!("invalid lease.renew request: {error}"))?;
    let mut guard = policy
        .write()
        .map_err(|_| "capability policy lock is poisoned".to_string())?;
    // The kernel acts as the issuer on the native UI's behalf.
    let kernel_principal = guard.kernel_principal().clone();
    let lease = guard
        .renew_lease(&kernel_principal, renew.lease_id, renew.expires_at, now_ms)
        .map_err(|error| format!("lease.renew failed: {error}"))?;
    Ok(json!({
        "leaseId": lease.lease_id(),
        "capability": lease.capability().name(),
        "expiresAt": lease.expires_at(),
        "revokedAt": lease.revoked_at(),
    }))
}
