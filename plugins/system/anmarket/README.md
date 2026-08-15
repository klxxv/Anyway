# AnMarket system plugin

_Dependency-free contract skeleton for the official Anyway supply-chain system plugin._

---

AnMarket is the proposed official Anyway system plugin for plugin supply-chain
coordination. It runs in the non-kernel AnCordis process pool and uses the
Kernel inside-RPC boundary for all package, blob, scan, policy, and activation
operations.

This directory is a dependency-free contract skeleton. It does not change the
current plugin installer or make `SystemPlugin` executable on baseline
`ae47fe6`; the manifest describes the target Kernel contract.

## 🔐 Boundary

AnMarket may register and compose the following provider types:

- `RegistryProvider` discovers and fetches package candidates
- `AnalyzerProvider` returns bounded findings for a read-only `BlobRef`
- `ReputationProvider` returns reputation signals and signed blocklist feeds
- `PolicyAdvisor` returns a recommendation for the Kernel to evaluate

The Kernel remains the sole authority for trust roots, subject hashes,
signature verification, quarantine, permission grants, atomic activation,
rollback, and audit state. No provider can approve itself or call installation
directly.

## 📚 Files

| File | Purpose |
| --- | --- |
| `plugin.yml` | Proposed official system-plugin manifest |
| `types.ts` | Dependency-free provider, report, RPC, BlobRef, and Vue IR types |
| `index.ts` | Type-only public entry point |

## 🧩 Provider rules

Providers receive JSON metadata and immutable references. An analyzer never
receives a local path, writable file handle, credential, or mutable workspace
store. If it needs bytes, it uses the host read-only blob RPC with bounded
ranges and quotas. Large payloads do not travel inside JSON RPC envelopes.

Every `ScanReport` binds the subject hash, every analyzer identity and version,
the policy version, the permission diff, and a Kernel-recorded report hash.
Reports are evidence; only the Kernel can turn them into an activation decision.

## 🎨 Vue IR rules

AnMarket contributes only allowlisted Vue IR components. The renderer owns the
actual Vue components and validates the IR before rendering. Raw HTML, dynamic
component names, arbitrary JavaScript, renderer callbacks, and direct DOM
access are not part of this contract. UI actions resolve to named Host RPC
commands and are checked against the caller principal and capability lease.

## ⚙️ Parallel scans

The AnCordis plugin coordinates provider registration and lifecycle. The Kernel
scheduler owns scan admission, per-principal quotas, worker-pool placement,
deadlines, cancellation, and fail-closed behavior. Independent analyzers may
run in parallel; a timeout or crash produces an incomplete report and keeps the
subject quarantined until the Kernel policy is satisfied.
