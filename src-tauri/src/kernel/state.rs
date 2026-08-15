//! Thread-safe ownership boundary for the Tauri-independent host bus.

use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::bus::HostBus;

/// Application state that can be registered with Tauri's managed state.
///
/// The state owns only synchronization and the admission/routing model. It has
/// no Tauri types so unit tests and non-Tauri transports can share the same
/// kernel state boundary.
pub struct KernelState {
    bus: Arc<RwLock<HostBus>>,
}

impl KernelState {
    pub fn new(bus: HostBus) -> Self {
        Self {
            bus: Arc::new(RwLock::new(bus)),
        }
    }

    pub fn bus(&self) -> &RwLock<HostBus> {
        self.bus.as_ref()
    }

    pub fn shared_bus(&self) -> Arc<RwLock<HostBus>> {
        Arc::clone(&self.bus)
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
        Self::new(HostBus::default())
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
}
