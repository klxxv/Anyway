//! Thread-safe ownership boundary for the Tauri-independent kernel models.

use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::audit::AuditLedger;
use super::blob::{BlobQuota, BlobStore};
use super::bus::HostBus;
use super::events::EventBus;
use super::graph_patches::graph_projects::GraphProjectRegistry;
use super::graph_patches::GraphPatchProposalRegistry;
use super::package_gate::PackageGate;
use super::plugin_surfaces::{PluginSurfaceRegistry, PluginWorkerSessionRegistry};
use super::rpc::RpcLedger;
use super::scheduler::Scheduler;
use super::service_registry::ServiceRegistry;
use super::supervisor::Supervisor;
use crate::host_bus::workers::PluginWorkerManager;
use anyway_schema_v4::storage::InMemoryStorage;

/// Application state that can be registered with Tauri's managed state.
///
/// The state owns synchronization and the admission/routing model plus the
/// Phase 3 data and control planes, the Phase 4 scheduler and supervisor
/// planes, and the audit plane required by the Host SDK gateway. It has no
/// Tauri types so unit tests and non-Tauri transports can share the same
/// kernel state boundary.
pub struct KernelState {
    bus: Arc<RwLock<HostBus>>,
    blobs: Arc<RwLock<BlobStore>>,
    rpc: Arc<RwLock<RpcLedger>>,
    scheduler: Arc<RwLock<Scheduler>>,
    supervisor: Arc<RwLock<Supervisor>>,
    services: Arc<RwLock<ServiceRegistry>>,
    packages: Arc<RwLock<PackageGate>>,
    plugin_surfaces: Arc<RwLock<PluginSurfaceRegistry>>,
    plugin_worker_sessions: Arc<PluginWorkerSessionRegistry>,
    plugin_workers: Arc<PluginWorkerManager>,
    audit: Arc<RwLock<AuditLedger>>,
    events: Arc<RwLock<EventBus>>,
    graph_storage: Arc<RwLock<InMemoryStorage>>,
    graph_patch_proposals: Arc<RwLock<GraphPatchProposalRegistry>>,
    graph_projects: Arc<RwLock<GraphProjectRegistry>>,
}

impl KernelState {
    pub fn new(bus: HostBus, blobs: BlobStore, rpc: RpcLedger) -> Self {
        Self {
            bus: Arc::new(RwLock::new(bus)),
            blobs: Arc::new(RwLock::new(blobs)),
            rpc: Arc::new(RwLock::new(rpc)),
            scheduler: Arc::new(RwLock::new(Scheduler::default())),
            supervisor: Arc::new(RwLock::new(Supervisor::default())),
            services: Arc::new(RwLock::new(ServiceRegistry::default())),
            packages: Arc::new(RwLock::new(PackageGate::default())),
            plugin_surfaces: Arc::new(RwLock::new(PluginSurfaceRegistry::default())),
            plugin_worker_sessions: Arc::new(PluginWorkerSessionRegistry::default()),
            plugin_workers: Arc::new(PluginWorkerManager::new()),
            audit: Arc::new(RwLock::new(AuditLedger::default())),
            events: Arc::new(RwLock::new(EventBus::default())),
            graph_storage: Arc::new(RwLock::new(InMemoryStorage::default())),
            graph_patch_proposals: Arc::new(RwLock::new(GraphPatchProposalRegistry::default())),
            graph_projects: Arc::new(RwLock::new(GraphProjectRegistry::default())),
        }
    }

    /// Build the thread-safe kernel with default Phase 3 quota and inflight
    /// limits. `max_inflight` is the RPC ledger limit; blob quota uses its own
    /// defaults unless overridden.
    pub fn with_defaults(max_inflight: usize) -> Self {
        Self::new(
            HostBus::default(),
            BlobStore::new(BlobQuota::default()).expect("default blob quota is valid"),
            RpcLedger::new(max_inflight).expect("rpc ledger max inflight is non-zero"),
        )
    }

    /// Build the kernel around a pre-registered bus while keeping default
    /// Phase 3 blob quota and RPC ledger limits.
    pub fn with_bus(bus: HostBus, max_inflight: usize) -> Self {
        Self::new(
            bus,
            BlobStore::new(BlobQuota::default()).expect("default blob quota is valid"),
            RpcLedger::new(max_inflight).expect("rpc ledger max inflight is non-zero"),
        )
    }

    pub fn bus(&self) -> &RwLock<HostBus> {
        self.bus.as_ref()
    }

    pub fn blobs(&self) -> &RwLock<BlobStore> {
        self.blobs.as_ref()
    }

    pub fn rpc(&self) -> &RwLock<RpcLedger> {
        self.rpc.as_ref()
    }

    pub fn schedulers(&self) -> &RwLock<Scheduler> {
        self.scheduler.as_ref()
    }

    pub fn supervisor(&self) -> &RwLock<Supervisor> {
        self.supervisor.as_ref()
    }

    pub fn shared_bus(&self) -> Arc<RwLock<HostBus>> {
        Arc::clone(&self.bus)
    }

    pub fn shared_blobs(&self) -> Arc<RwLock<BlobStore>> {
        Arc::clone(&self.blobs)
    }

    pub fn shared_rpc(&self) -> Arc<RwLock<RpcLedger>> {
        Arc::clone(&self.rpc)
    }

    pub fn shared_schedulers(&self) -> Arc<RwLock<Scheduler>> {
        Arc::clone(&self.scheduler)
    }

    pub fn shared_supervisor(&self) -> Arc<RwLock<Supervisor>> {
        Arc::clone(&self.supervisor)
    }

    pub fn services(&self) -> &RwLock<ServiceRegistry> {
        self.services.as_ref()
    }

    pub fn shared_services(&self) -> Arc<RwLock<ServiceRegistry>> {
        Arc::clone(&self.services)
    }

    pub fn packages(&self) -> &RwLock<PackageGate> {
        self.packages.as_ref()
    }

    pub fn plugin_surfaces(&self) -> &RwLock<PluginSurfaceRegistry> {
        self.plugin_surfaces.as_ref()
    }

    pub fn shared_plugin_surfaces(&self) -> Arc<RwLock<PluginSurfaceRegistry>> {
        Arc::clone(&self.plugin_surfaces)
    }

    pub fn plugin_worker_sessions(&self) -> &PluginWorkerSessionRegistry {
        self.plugin_worker_sessions.as_ref()
    }

    pub fn shared_plugin_worker_sessions(&self) -> Arc<PluginWorkerSessionRegistry> {
        Arc::clone(&self.plugin_worker_sessions)
    }

    pub fn plugin_workers(&self) -> &PluginWorkerManager {
        self.plugin_workers.as_ref()
    }

    pub fn shared_plugin_workers(&self) -> Arc<PluginWorkerManager> {
        Arc::clone(&self.plugin_workers)
    }

    pub fn shared_packages(&self) -> Arc<RwLock<PackageGate>> {
        Arc::clone(&self.packages)
    }

    pub fn audit(&self) -> &RwLock<AuditLedger> {
        self.audit.as_ref()
    }

    pub fn shared_audit(&self) -> Arc<RwLock<AuditLedger>> {
        Arc::clone(&self.audit)
    }

    pub fn events(&self) -> &RwLock<EventBus> {
        self.events.as_ref()
    }

    pub fn shared_events(&self) -> Arc<RwLock<EventBus>> {
        Arc::clone(&self.events)
    }

    pub fn graph_storage(&self) -> &RwLock<InMemoryStorage> {
        self.graph_storage.as_ref()
    }

    pub fn shared_graph_storage(&self) -> Arc<RwLock<InMemoryStorage>> {
        Arc::clone(&self.graph_storage)
    }

    pub fn graph_patch_proposals(&self) -> &RwLock<GraphPatchProposalRegistry> {
        self.graph_patch_proposals.as_ref()
    }

    pub fn shared_graph_patch_proposals(&self) -> Arc<RwLock<GraphPatchProposalRegistry>> {
        Arc::clone(&self.graph_patch_proposals)
    }

    pub fn graph_projects(&self) -> &RwLock<GraphProjectRegistry> {
        self.graph_projects.as_ref()
    }

    pub fn shared_graph_projects(&self) -> Arc<RwLock<GraphProjectRegistry>> {
        Arc::clone(&self.graph_projects)
    }

    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, HostBus>> {
        self.bus.read()
    }

    pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, HostBus>> {
        self.bus.write()
    }
}

impl Default for KernelState {
    fn default() -> Self {
        Self::with_defaults(super::bus::DEFAULT_MAX_INFLIGHT_PER_PRINCIPAL)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn state_exposes_a_shared_thread_safe_bus() {
        let state = Arc::new(KernelState::default());
        let shared = state.shared_bus();
        let worker = std::thread::spawn(move || {
            let bus = shared.read().expect("bus read lock");
            bus.operation_count()
        });

        assert_eq!(worker.join().expect("worker completed"), 0);
        assert_eq!(state.read().expect("bus read lock").operation_count(), 0);
    }

    #[test]
    fn state_owns_blob_and_rpc_planes_synchronously() {
        let state = Arc::new(KernelState::default());
        let shared_blobs = state.shared_blobs();
        let shared_rpc = state.shared_rpc();
        let worker = std::thread::spawn(move || {
            let blobs = shared_blobs.read().expect("blob read lock");
            let rpc = shared_rpc.read().expect("rpc read lock");
            (blobs.stored_blob_count(), rpc.active_count())
        });

        assert_eq!(worker.join().expect("worker completed"), (0, 0));
        assert_eq!(
            state
                .blobs()
                .read()
                .expect("blob read lock")
                .stored_blob_count(),
            0
        );
        assert_eq!(state.rpc().read().expect("rpc read lock").active_count(), 0);
    }

    #[test]
    fn state_owns_scheduler_and_supervisor_planes_synchronously() {
        let state = Arc::new(KernelState::default());
        let shared_schedulers = state.shared_schedulers();
        let shared_supervisor = state.shared_supervisor();
        let worker = std::thread::spawn(move || {
            let schedulers = shared_schedulers.read().expect("scheduler read lock");
            let supervisor = shared_supervisor.read().expect("supervisor read lock");
            (
                schedulers.config().default_per_principal_quota,
                supervisor.worker_count(),
            )
        });

        assert_eq!(
            worker.join().expect("worker completed"),
            (crate::kernel::scheduler::DEFAULT_PER_PRINCIPAL_QUOTA, 0)
        );
        assert_eq!(
            state
                .schedulers()
                .read()
                .expect("scheduler read lock")
                .config()
                .default_per_principal_quota,
            crate::kernel::scheduler::DEFAULT_PER_PRINCIPAL_QUOTA
        );
        assert_eq!(
            state
                .supervisor()
                .read()
                .expect("supervisor read lock")
                .worker_count(),
            0
        );
    }

    #[test]
    fn state_owns_the_service_registry_plane_synchronously() {
        let state = Arc::new(KernelState::default());
        let shared_services = state.shared_services();
        let worker = std::thread::spawn(move || {
            let services = shared_services.read().expect("service registry read lock");
            services.service_count()
        });

        assert_eq!(worker.join().expect("worker completed"), 0);
        assert_eq!(
            state
                .services()
                .read()
                .expect("service registry read lock")
                .service_count(),
            0
        );
    }

    #[test]
    fn state_owns_the_package_gate_plane_synchronously() {
        let state = Arc::new(KernelState::default());
        let shared_packages = state.shared_packages();
        let worker = std::thread::spawn(move || {
            let packages = shared_packages.read().expect("package gate read lock");
            packages.candidate_count()
        });

        assert_eq!(worker.join().expect("worker completed"), 0);
        assert_eq!(
            state
                .packages()
                .read()
                .expect("package gate read lock")
                .candidate_count(),
            0
        );
    }

    #[test]
    fn state_owns_the_audit_plane_synchronously() {
        let state = Arc::new(KernelState::default());
        let shared_audit = state.shared_audit();
        let worker = std::thread::spawn(move || {
            let audit = shared_audit.read().expect("audit read lock");
            audit.len()
        });

        assert_eq!(worker.join().expect("worker completed"), 0);
        assert_eq!(
            state.audit().read().expect("audit read lock").len(),
            0,
            "a fresh kernel owns an empty audit ledger"
        );
    }
}
