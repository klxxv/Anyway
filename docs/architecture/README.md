# Anyway architecture index

_Working architecture for the Kernel Host Bus and the next-generation plugin platform._

---

## 📋 Documents

| Document | Purpose | Status |
| --- | --- | --- |
| [Kernel Host architecture](anyway-kernel-host-architecture.md) | Authoritative system boundary, data flow, security model, and extension roles | Accepted for phase 1 |
| [Migration roadmap](migration-roadmap.md) | Incremental delivery plan from flat Tauri commands to the Host SDK | Accepted for phase 1 |
| [Kernel lifecycle](kernel-lifecycle.md) | Principals, capability leases, workers, supervision, and restart policy | Phase 1 implemented |
| [Blob and Inside RPC](blob-inside-rpc.md) | Bulk-data plane and local RPC contract | Phase 1 implemented |
| [Vue UI IR](vue-ui-ir.md) | Safe plugin UI and native Vue integration | Phase 1 implemented |
| [AnCordis](ancordis.md) | Official Cordis-based extension host | Protocol skeleton implemented |
| [AnMarket](anmarket.md) | Official marketplace and supply-chain extension | Protocol skeleton implemented |

## 🎯 Decision summary

- Rust remains the privileged kernel and owns every non-bypassable authorization decision.
- Native application code and plugins use one versioned Host SDK envelope, but never share authority implicitly.
- `PrincipalId` is preserved through every proxy and child worker to prevent confused-deputy escalation.
- Inside RPC transports control metadata; immutable `BlobRef` values carry bulk data.
- AnCordis is a non-kernel extension host and control plane, not a sandbox or bulk-data proxy.
- AnMarket supplies registries, analyzers, reputation, and policy evidence; the kernel retains trust roots and activation authority.
- Untrusted UI is declarative Vue IR rendered by an allowlist. Raw HTML and executable component code do not cross the boundary.
- Graph mutation remains a kernel-mediated, revision-checked `GraphPatch` commit.

## 🔐 Normative language

The terms **MUST**, **MUST NOT**, **SHOULD**, and **MAY** describe architecture requirements. A subsystem is not complete while it violates a MUST-level invariant, even if its public API appears functional.
