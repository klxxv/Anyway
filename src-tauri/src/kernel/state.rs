//! Thread-safe ownership boundary for the Tauri-independent kernel models.

use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::blob::{BlobQuota, BlobStore};
use super::bus::HostBus;
use super::rpc::RpcLedger;
use super::scheduler::Scheduler;
use super::supervisor::Supervisor;

/// Application state that can be registered with Tauri's managed state.
///
/// The state owns synchronization and the admission/routing model plus the
/// Phase 3 data and control planes and the Phase 4 scheduler and supervisor
/// planes. It has no Tauri types so unit tests and non-Tauri transports can
/// share the same kernel state boundary.
pub struct KernelState {
    bus: Arc<RwLock<HostBus>>,
    blobs: Arc<RwLock<BlobStore>>,
    rpc: Arc<RwLock<RpcLedger>>,
    scheduler: Arc<RwLock<Scheduler>>,
    supervisor: Arc<RwLock<Supervisor>>,
}

impl KernelState {
    pub fn new(bus: HostBus, blobs: BlobStore, rpc: RpcLedger) -> Self {
        Self {
            bus: Arc::new(RwLock::new(bus)),
            blobs: Arc::new(RwLock::new(blobs)),
            rpc: Arc::new(RwLock::new(rpc)),
            scheduler: Arc::new(RwLock::new(Scheduler::default())),
            supervisor: Arc::new(RwLock::new(Supervisor::default())),
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
            state.blobs().read().expect("blob read lock").stored_blob_count(),
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
            (schedulers.config().default_per_principal_quota, supervisor.worker_count())
        });

        assert_eq!(
            worker.join().expect("worker completed"),
            (
                crate::kernel::scheduler::DEFAULT_PER_PRINCIPAL_QUOTA,
                0
            )
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
}
