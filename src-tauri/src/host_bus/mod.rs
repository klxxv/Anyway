//! Host Bus v2 — 分域解耦的 handler + 中间件链。
//!
//! 信息流动路径与生命周期（每层独立函数，见 `kernel_commands::kernel_host_call`）：
//!
//! ```text
//! HostCallRequest
//!   ├─[1] transport_auth       仅 MAIN_WEBVIEW 主体可进入
//!   ├─[2] schema_validate      RPC 信封校验（apiVersion/requestId/deadline）
//!   ├─[3] principal_bind       绑定原生 UI principal
//!   ├─[4] lease_resolve        解析 capabilityLeaseIds
//!   ├─[5] capability_authorize 能力白名单授权（policy）
//!   ├─[6] admission_gate       并发/配额/死线（AdmissionRequest）
//!   ├─[7] dispatch             路由到 host_bus/*.rs 的分域 handler
//!   └─[8] audit                record_audit（成功/失败/拒绝留痕）
//!   ▼
//! HostCallResponse
//! ```
//!
//! 每个新增 host bus API 组一个文件（`services.rs` / `events.rs` / `workers.rs` /
//! `storage.rs` / `audit.rs` / `ir.rs` / `blob.rs` / `lease.rs`），handler 只依赖
//! 内核状态（`KernelState` 的 RwLock 分域），不互相 import 具体实现。

pub mod audit;
pub mod blob;
pub mod events;
pub mod graph_patch;
pub mod ir;
pub mod lease;
pub mod python_worker;
pub mod services;
pub mod storage;
pub mod workers;

/// A named middleware step in the fixed chain above.
pub struct MiddlewareStep {
    pub name: &'static str,
    pub description: &'static str,
}

/// The fixed host bus middleware chain, in execution order.
pub const MIDDLEWARE_CHAIN: &[MiddlewareStep] = &[
    MiddlewareStep {
        name: "transport_auth",
        description: "reject non-main webviews",
    },
    MiddlewareStep {
        name: "schema_validate",
        description: "validate the RPC envelope",
    },
    MiddlewareStep {
        name: "principal_bind",
        description: "bind the native UI principal",
    },
    MiddlewareStep {
        name: "lease_resolve",
        description: "resolve capability lease ids",
    },
    MiddlewareStep {
        name: "capability_authorize",
        description: "capability allowlist check",
    },
    MiddlewareStep {
        name: "admission_gate",
        description: "concurrency/quota/deadline admission",
    },
    MiddlewareStep {
        name: "dispatch",
        description: "route to a host_bus domain handler",
    },
    MiddlewareStep {
        name: "audit",
        description: "record the outcome in the audit ledger",
    },
];
