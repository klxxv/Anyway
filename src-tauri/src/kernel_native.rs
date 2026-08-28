//! 原生（非 webview）host bus 入口 / Native (non-webview) host bus entry.
//!
//! 宿主进程内的受信管线代码(例如官方 pdf-canvas-agent 的 myc.llm.v4 抽取器)
//! 通过 [`kernel_bus_call`] 走与 `kernel_host_call` 完全相同的中间件链:
//! schema 校验 → 原生 UI principal 绑定 → 租约解析 → 能力授权 → 准入门 →
//! 分域 dispatch → 审计。它绝不绕过 policy 或审计账本。
//!
//! 原生入口只服务 kernel 平面操作(8 个 host-bus 域:`graph.ir.*`、
//! `graph.storage.*`、`event.*`、`lease.renew`、`worker.*`、`service.*`、
//! `audit.read`、`blob.*`)。这些 handler 全部是同步的;依赖应用句柄或
//! 网络的 webview 操作(如 `plugin.install`)仍只经 `kernel_host_call`。
//!
//! Hosted trusted pipeline code (for example the official pdf-canvas-agent
//! myc.llm.v4 extractor) drives `graph.ir.compile`, `graph.storage.*`,
//! `event.publish`, and friends through this exact path — it never bypasses
//! policy or the audit ledger.
//!
//! The native entry serves the eight kernel-plane host-bus domains only; all
//! of those handlers are synchronous. Webview operations that need an
//! application handle or the network (such as `plugin.install`) keep going
//! through `kernel_host_call`.

use crate::kernel::audit::AuditOutcome;
use crate::kernel::bus::{AdmissionRequest, BusPayload};
use crate::kernel::identity::PrincipalId;
use crate::kernel::policy::{NATIVE_UI_PRINCIPAL_NAME, PLUGIN_LIST_OPERATION};
use crate::kernel::state::KernelState;
use crate::kernel_commands::{
    authorize_for_bus, bus_failure, parse_lease_ids, policy_failure, record_audit, request_key,
    CapabilityPolicyState, HostCallRequest, HostCallResponse,
};

/// Dispatch the eight host-bus domains. Every handler is synchronous, so the
/// native entry needs no async runtime on this path.
fn dispatch_kernel_plane(
    request: &HostCallRequest,
    kernel: &KernelState,
    policy: &CapabilityPolicyState,
) -> Result<serde_json::Value, String> {
    match request.operation.as_str() {
        "service.list" => crate::host_bus::services::dispatch_service_list(kernel.services()),
        "service.unregister" => {
            crate::host_bus::services::dispatch_service_unregister(request, kernel.services())
        }
        "audit.read" => crate::host_bus::audit::dispatch_audit_read(request, kernel.audit()),
        "blob.list" => crate::host_bus::blob::dispatch_blob_list(kernel.blobs()),
        "blob.release" => crate::host_bus::blob::dispatch_blob_release(request, kernel.blobs()),
        "lease.renew" => {
            crate::host_bus::lease::dispatch_lease_renew(request, policy.policy(), policy.now_ms())
        }
        "event.subscribe" => {
            crate::host_bus::events::dispatch_event_subscribe(request, kernel.events())
        }
        "event.publish" => {
            crate::host_bus::events::dispatch_event_publish(request, kernel.events())
        }
        "event.poll" => crate::host_bus::events::dispatch_event_poll(request, kernel.events()),
        "worker.spawn" => {
            crate::host_bus::workers::dispatch_worker_spawn(request, kernel.supervisor())
        }
        "worker.stop" => {
            crate::host_bus::workers::dispatch_worker_stop(request, kernel.supervisor())
        }
        "graph.storage.put" => {
            crate::host_bus::storage::dispatch_graph_storage_put(request, kernel.graph_storage())
        }
        "graph.storage.query" => {
            crate::host_bus::storage::dispatch_graph_storage_query(request, kernel.graph_storage())
        }
        "graph.ir.compile" => crate::host_bus::ir::dispatch_graph_ir_compile(request),
        "graph.ir.query" => crate::host_bus::ir::dispatch_graph_ir_query(request),
        other => Err(format!(
            "operation {other} is not a native kernel-plane operation"
        )),
    }
}

/// Run one kernel-plane host-bus request through the full middleware chain as
/// the trusted native UI principal.
pub fn kernel_bus_call(
    kernel: &KernelState,
    policy: &CapabilityPolicyState,
    request: HostCallRequest,
) -> Result<HostCallResponse, String> {
    let response_request_id = request.request_id.clone();
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
    let lease = match authorize_for_bus(policy, &request, &principal, &selected_lease_ids, now_ms) {
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

    let handler_result = dispatch_kernel_plane(&request, kernel, policy);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_bus_operations_resolve_to_their_design_capabilities() {
        let policy = CapabilityPolicyState::default();
        let principal = PrincipalId::new(NATIVE_UI_PRINCIPAL_NAME).unwrap();
        for (operation, capability) in [
            ("graph.storage.put", "graph.storage.write"),
            ("graph.storage.query", "graph.storage.read"),
            ("graph.ir.compile", "graph.ir"),
            ("graph.ir.query", "graph.ir"),
            ("lease.renew", "host-bus.lease"),
            ("event.subscribe", "host-bus.event"),
            ("event.publish", "host-bus.event"),
            ("event.poll", "host-bus.event"),
            ("worker.stop", "host-bus.worker"),
            ("service.list", "host-bus.service"),
            ("service.unregister", "host-bus.service"),
            ("audit.read", "audit.read"),
            ("blob.list", "blob.manage"),
            ("blob.release", "blob.manage"),
        ] {
            let guard = policy.policy().read().expect("policy lock");
            let authorization = guard
                .authorize(operation, &principal, &[], 1_000)
                .unwrap_or_else(|error| {
                    panic!("{operation} must authorize via native bootstrap: {error}")
                });
            assert_eq!(
                authorization.capability().name(),
                capability,
                "capability mismatch for {operation}"
            );
        }
    }

    #[test]
    fn kernel_bus_call_runs_graph_ir_compile_through_the_middleware_chain() {
        let kernel = crate::kernel_commands::create_kernel_state().expect("kernel state");
        let policy = CapabilityPolicyState::default();

        // An empty extraction is a valid myc.llm.v4 root and must compile to
        // an empty myc.graph-ir.v4 canvas — proving the full middleware chain
        // (validate → authorize → admission → dispatch → audit) accepts the
        // official host-bus surface from native code.
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": crate::kernel_commands::HOST_SDK_API_VERSION,
            "requestId": "ir-compile-1",
            "operation": "graph.ir.compile",
            "payload": {
                "kind": "inline",
                "value": {
                    "extraction": { "schema_version": "myc.llm.v4" }
                }
            },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let response = kernel_bus_call(&kernel, &policy, request).expect("envelope stays Ok");
        let value = serde_json::to_value(&response).expect("serializable response");
        assert!(value.get("error").is_none(), "unexpected error: {value}");
        let result = value.get("result").expect("result payload");
        assert_eq!(result["canvas"]["schema_version"], "myc.graph-ir.v4");
        assert_eq!(result["errors"].as_array().map(Vec::len), Some(0));

        // The call is recorded in the audit ledger with the native principal.
        let audit = kernel.audit().read().expect("audit lock");
        assert!(
            audit.query(0, 1024).iter().any(|entry| {
                entry.operation == "graph.ir.compile"
                    && entry.principal.as_str() == NATIVE_UI_PRINCIPAL_NAME
            }),
            "graph.ir.compile must be audited"
        );
    }

    #[test]
    fn native_bus_call_rejects_an_unknown_operation_before_admission() {
        let kernel = crate::kernel_commands::create_kernel_state().expect("kernel state");
        let policy = CapabilityPolicyState::default();
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": crate::kernel_commands::HOST_SDK_API_VERSION,
            "requestId": "unknown-op-1",
            "operation": "graph.ir.nope",
            "payload": { "kind": "inline", "value": {} },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let response = kernel_bus_call(&kernel, &policy, request).expect("envelope stays Ok");
        let value = serde_json::to_value(&response).expect("serializable response");
        assert!(
            value.get("error").is_some(),
            "unknown operations must fail before admission: {value}"
        );
    }

    #[test]
    fn native_bus_call_returns_a_structured_failure_for_invalid_payloads() {
        let kernel = crate::kernel_commands::create_kernel_state().expect("kernel state");
        let policy = CapabilityPolicyState::default();
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": crate::kernel_commands::HOST_SDK_API_VERSION,
            "requestId": "bad-payload-1",
            "operation": "graph.ir.compile",
            "payload": { "kind": "inline", "value": {} },
            "deadlineMs": 30_000
        }))
        .unwrap();
        let response = kernel_bus_call(&kernel, &policy, request).expect("envelope stays Ok");
        let value = serde_json::to_value(&response).expect("serializable response");
        assert_eq!(
            value["error"]["code"], "HOST_HANDLER_FAILED",
            "invalid payloads must surface a structured handler failure: {value}"
        );
    }
}
