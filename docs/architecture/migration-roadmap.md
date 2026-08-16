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
| Kernel audit ledger | ✅ Phase 8 — `kernel/audit.rs` adds a bounded (1024-event) ledger with a monotonic sequence; the gateway records `Denied` on authorization failure and `Completed`/`Failed` on dispatch outcome, persisting the validated `traceParent`. This satisfies the audit prerequisite for migrating state-changing routes. |
| Settings write/reset | ✅ Phase 8 — `set_plugin_settings` / `reset_plugin_settings` now route through `plugin.settings.write` / `plugin.settings.reset`; first write commands migrated after the audit ledger landed. |
| Project save/import | ✅ Phase 8 — `save_project_file` / `import_project_file` now route through `project.save` / `project.import`; `importProjectAtPath` simplified to the Host SDK directly. |
| Sync workspace writes | ✅ Phase 8 — `scan_project_folder`, `initialize_git_workspace`, `generate_github_ssh_key`, and `git_autosave_project` now route through `workspace.folder.scan` / `workspace.git.init` / `workspace.github.ssh.generate` / `workspace.git.autosave`. |
| Install / uninstall / VSIX | ✅ Phase 8 — `install_myc_plugin`, `uninstall_myc_plugin`, and `import_vscode_vsix` now route through `plugin.install` / `plugin.uninstall` / `plugin.vsix.import`. |
| Agent review / cancel | ✅ Phase 8 — `review_patch` and `cancel_job` now route through `agent.job.review` / `agent.job.cancel`. This completes the synchronous command migration surface (21 sync commands). |
| Async gateway + login/ssh-upload | ✅ Phase 8 — `kernel_host_call`/`dispatch` are now async (Tauri async contract via `Result`), and `login_github_account`/`upload_github_ssh_key` route through `workspace.github.login` / `workspace.github.ssh.upload`. |
| Agent start | ✅ Phase 8 — `start_pdf_job` and `start_document_batch` now route through `agent.job.start` / `agent.batch.start` via shared `queue_pdf_job`/`queue_document_batch` helpers. |
| Graph compile/diff + analysis run | ✅ Phase 8 — `compile_project`, `compute_diff`, and `execute_myc_plugin` now route through `graph.compile` / `graph.diff` / `plugin.analysis.run` (correcting an earlier under-count). |
| Plugin connection test | ✅ Phase 8 — `test_plugin_connection` now routes through `plugin.connection.test` (async, secrets-bearing DTO). |

Remaining entry is `save_plugin_artifact` (binary `Vec<u8>`, needs the Blob data path) — plus the package-discovery transaction, scoped `BlobRef` paths, unsigned-package provenance, streaming WASM, and manifest-grant separation.

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
