# Kernel Host migration roadmap

_Incremental migration from the current Tauri plugin host to the Kernel Host Bus without a flag-day rewrite._

---

## 🎯 Strategy

The migration follows a strangler pattern. Existing Tauri commands continue to serve the application while their implementation moves behind a new kernel façade. Each phase has a compatibility adapter, acceptance tests, and a rollback point.

```mermaid
flowchart LR
    accTitle: Kernel Host Migration Phases
    accDescr: The migration freezes contracts first, introduces a kernel facade and principals, then moves data, workers, UI, Cordis, and the marketplace before retiring legacy commands

    contracts["Freeze contracts"] --> facade["Add kernel facade"]
    facade --> identity["Enforce principals"]
    identity --> blob_rpc["Add Blob and RPC"]
    blob_rpc --> workers["Migrate worker pools"]
    workers --> vue_ir["Enable Vue IR"]
    vue_ir --> ancordis["Activate AnCordis"]
    ancordis --> anmarket["Activate AnMarket"]
    anmarket --> retire(["Retire legacy paths"])

    classDef phase fill:#dbeafe,stroke:#2563eb,stroke-width:2px,color:#1e3a5f
    classDef finish fill:#dcfce7,stroke:#16a34a,stroke-width:2px,color:#14532d
    class contracts,facade,identity,blob_rpc,workers,vue_ir,ancordis,anmarket phase
    class retire finish
```

## 📋 Phase plan

| Phase | Deliverable | Compatibility boundary | Exit criterion |
| --- | --- | --- | --- |
| 0 | Architecture and protocol types | No runtime change | Cross-system invariants reviewed |
| 1 | `KernelState` and Host Bus façade | Tauri commands call façade | Existing tests pass unchanged |
| 2 | Principals and capability leases | Native UI gets an explicit trusted principal | No caller-supplied plugin identity is trusted |
| 3 | Blob Store and Inside RPC | Inline payloads remain below threshold | Large PDF/plugin data uses BlobRef |
| 4 | Scheduler and supervisor | Existing agent job APIs remain | Independent jobs run concurrently with quotas |
| 5 | Vue UI IR and PluginSlot | Existing host-rendered settings remain | One safe IR contribution renders end to end |
| 6 | AnCordis Extension Host | Legacy plugin types remain | One service registers and executes through Host Bus |
| 7 | AnMarket | Existing local package browser remains | Candidate scan-to-activation transaction works |
| 8 | Cleanup | Temporary adapters deprecated | No privileged operation bypasses Kernel RPC |

## ⚙️ Phase 1 implementation slice

The first code slice intentionally avoids changing product behavior:

1. Add pure Rust protocol, identity, lifecycle, Blob, and RPC types with tests.
2. Add a `kernel` module but do not route every Tauri command through it yet.
3. Add TypeScript UI IR and official system-extension protocol skeletons.
4. Keep the current PDF agent, WASM executor, plugin installer, and Vue store operational.
5. Document every temporary bypass in the migration ledger below.

## 🔗 Phase 2 vertical slice

The first runtime route is now complete for the read-only plugin catalog:

1. Vue calls `HostSdk.call("plugin.list", {})` through the lazy Tauri transport.
2. The single `kernel_host_call` Gateway rejects unknown DTO fields and binds `native.ui` from the `main` WebView.
3. Capability Policy resolves `plugin.list` to `plugin.catalog.read` and returns the explicit native bootstrap proof.
4. The Gateway attenuates that proof into a deadline-bounded lease.
5. Host Bus admission validates the lease, deadline, duplicate request ID, and per-principal limits.
6. A read-only Rust plugin catalog query executes behind the admitted route.
7. The old `list_installed_plugins` command remains registered only as a rollback adapter.

This slice proves the common path without combining a new security boundary with a high-risk write operation. Plugin installation and settings mutation remain on legacy commands until their transaction and audit contracts are defined.

## 🔐 Migration ledger

| Temporary behavior | Risk | Removal phase |
| --- | --- | --- |
| Most Tauri operations still invoke commands directly; `plugin.list` is migrated | Inconsistent envelope and identity outside the first route | Phase 2–8 |
| Pending package discovery runs during kernel startup | Package activation is not yet a Policy/AnMarket transaction | Phase 7 |
| Frontend passes plugin ID/version | Possible confused identity if host validation regresses | Phase 2 |
| Manifest declarations act as grants | No separate user/policy grant | Phase 2 |
| Agent jobs use one global async mutex | Avoidable serialization | Phase 4 |
| PDF jobs hold file paths | Path capability is not a scoped BlobRef | Phase 3–4 |
| WASM execution uses one-shot JSON bytes | No streaming or Blob inputs | Phase 3–4 |
| Unsigned packages can install | Provenance policy is implicit | Phase 7 |
| Plugin UI is limited to existing host dialogs | No general safe UI contribution model | Phase 5 |

### Resolved ledger entries

| Entry | Resolution |
| --- | --- |
| Agent jobs use one global async mutex | ✅ Phase 4 — `kernel/scheduler.rs` per-principal quota + `AgentJobGate` (tokio semaphore backed by the kernel scheduler); two independent batches run concurrently within quota. |
| Plugin UI is limited to existing host dialogs | ✅ Phase 5 slice — `PluginSlot` + host slot registry + `UiIrActionRequest` → Host SDK dispatch (`app/platform/ui-ir-dispatch.ts`); the renderer is wired but not yet mounted in the workspace shell. |
| `list_installed_plugins` rollback adapter | ✅ Phase 8 — the direct Tauri command was retired from the invoke handler after all frontend callers moved to `plugin.list`; the Rust function remains only for the internal LLM-provider registry. |
| Plugin settings read | ✅ Phase 8 — `get_plugin_settings` now routes through `plugin.settings.read` on the Host SDK (frontend uses `HostSdk.call`); the legacy command stays as a rollback adapter and is retired once the write/reset paths follow. |
| Workspace reads | ✅ Phase 8 — `list_folder_entries`, `read_git_workspace`, and `read_github_account` now route through `workspace.folder.list` / `workspace.git.read` / `workspace.github.read`; the DTOs drop the never-read `capability` field while the host re-checks each plugin's declared capability internally. |
| Icon-theme + agent reads | ✅ Phase 8 — `read_icon_theme_asset`, `get_job_status`, `list_import_jobs`, and `get_import_batch_status` now route through `plugin.icon-theme.read` / `agent.job.status` / `agent.job.list` / `agent.batch.status`. The gateway now injects the managed `AgentHostState`, so State-backed handlers run behind Host Bus admission. |

Remaining entries are now exclusively WRITE/state-changing paths (settings write/reset, install/uninstall, VSIX import, project save/import, workspace scan/init/login/ssh/autosave, agent start/review/cancel) plus the package-discovery transaction, scoped `BlobRef` paths, unsigned-package provenance, streaming WASM, and manifest-grant separation. These stay open for the AnMarket/install wiring, the Blob data path, and the write-path audit/idempotency follow-ups.

## 🧪 Verification gates

Every phase must pass:

- Rust unit tests for state transitions, authorization, malformed input, quotas, and cancellation
- TypeScript tests for protocol parsing and hostile UI IR
- Existing plugin SDK compatibility tests
- Existing native plugin and PDF workflow tests
- A negative security test proving the old bypass is closed
- A crash/restart test for any newly supervised worker
- A migration note listing any compatibility adapter still active

Performance acceptance for the first complete vertical slice:

- Small unary RPC adds no additional process hop after route resolution
- Bulk inputs cross the serialization boundary once before becoming BlobRefs
- Two independent document jobs can run concurrently within configured limits
- UI control requests remain responsive while CPU and process queues are saturated
- Cancelling a parent request revokes child work and temporary Blob leases

## ✍️ Ownership during parallel refactor

| Workstream | Initial write scope | Integration owner |
| --- | --- | --- |
| Kernel identity and lifecycle | `src-tauri/src/kernel/identity.rs`, `lifecycle.rs`, `supervisor.rs` | Main branch integrator |
| Blob and RPC | `src-tauri/src/kernel/blob.rs`, `rpc.rs` | Main branch integrator |
| Vue UI IR | `app/plugins/ui-ir.ts`, `src/vue/runtime/vue-ir/` | Main branch integrator |
| AnCordis | `plugins/system/ancordis/` | Main branch integrator |
| AnMarket | `plugins/system/anmarket/` | Main branch integrator |

The integration owner controls `src-tauri/src/kernel/mod.rs`, `src-tauri/src/lib.rs`, shared manifest types, Cargo dependencies, and generated SDK entry points so parallel workers cannot create conflicting roots.
