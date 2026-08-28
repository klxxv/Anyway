# Anyway architecture index

_Working architecture for the Kernel Host Bus and the next-generation plugin platform._

---

## 📋 Documents

| Document | Purpose | Status |
| --- | --- | --- |
| [Kernel Host architecture](anyway-kernel-host-architecture.md) | Authoritative system boundary, data flow, security model, and extension roles | Accepted for phase 1 |
| [Migration roadmap](migration-roadmap.md) | Incremental delivery plan from flat Tauri commands to the Host SDK | Accepted for phase 1 |
| [Kernel lifecycle](kernel-lifecycle.md) | Principals, capability leases, workers, supervision, and restart policy | Phase 4 implemented (scheduler + supervisor wired) |
| [Blob and Inside RPC](blob-inside-rpc.md) | Bulk-data plane and local RPC contract | Phase 3 implemented |
| [Host Bus runtime](host-bus-runtime.md) | Runtime operation registry, admission ledger, deadlines, cancellation, and quotas | Phase 2 implemented |
| [Capability policy](capability-policy.md) | Requested, granted, leased, and active authorization stages | Phase 2 implemented |
| [Tauri Host SDK gateway](host-gateway.md) | First Vue-to-Kernel vertical slice and compatibility boundary | `plugin.list`, Blob, and service ops migrated |
| [Vue UI IR](vue-ui-ir.md) | Safe plugin UI and native Vue integration | Phase 5 implemented (PluginSlot + Host SDK dispatch) |
| [AnCordis](ancordis.md) | Official Cordis-based extension host | Phase 6 slice (service registry + register/call ops) |
| [AnMarket](anmarket.md) | Official marketplace and supply-chain extension | Phase 7 slice (package-candidate gate) |

## 🎯 Decision summary

- Rust remains the privileged kernel and owns every non-bypassable authorization decision.
- Native application code and plugins use one versioned Host SDK envelope, but never share authority implicitly.
- `PrincipalId` is preserved through every proxy and child worker to prevent confused-deputy escalation.
- Inside RPC transports control metadata; immutable `BlobRef` values carry bulk data.
- AnCordis is a non-kernel extension host and control plane, not a sandbox or bulk-data proxy.
- AnMarket supplies registries, analyzers, reputation, and policy evidence; the kernel retains trust roots and activation authority.
- Untrusted UI is declarative Vue IR rendered by an allowlist. Raw HTML and executable component code do not cross the boundary.
- Graph mutation remains a kernel-mediated, revision-checked `GraphPatch` commit.
- `plugin.list` is the first production call routed through the shared Host SDK, transport-bound Gateway, Policy, and Host Bus; its direct command remains only as a rollback adapter.
- Phase 4 added a per-principal workload scheduler and wired the in-process agent host as a supervised worker under its own `worker.agent-host` principal.
- Phase 6 added an in-kernel, bounded service registry (`service.register` / `service.call`) with a startup `anyway.system.ping` example service.
- Phase 7 added a fail-closed package-candidate gate (`quarantine → scan → approve → activate`) that emits a typed activation decision for a future install adapter.

## 🔐 Normative language

The terms **MUST**, **MUST NOT**, **SHOULD**, and **MAY** describe architecture requirements. A subsystem is not complete while it violates a MUST-level invariant, even if its public API appears functional.
