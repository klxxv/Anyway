//! Pure per-principal concurrency scheduler for the kernel.
//!
//! The scheduler owns concurrency accounting only. It never starts a thread,
//! waits on a timer, or runs a job. A transport adapter pairs it with an
//! async semaphore so admission here is always backed by a bounded gate.
//!
//! ## Relation to Host Bus admission
//!
//! The [`super::bus::HostBus`] enforces per-operation and per-principal RPC
//! admission quotas for control-plane calls; this [`Scheduler`] enforces
//! workload-pool concurrency for long-running agent jobs. The two planes are
//! distinct and are intentionally not merged in this phase.

use super::identity::PrincipalId;

use std::collections::BTreeMap;

/// Default per-principal concurrency quota used by the kernel.
///
/// The migration roadmap's Phase 4 acceptance requires two independent
/// document jobs to run concurrently within configured limits.
pub const DEFAULT_PER_PRINCIPAL_QUOTA: usize = 2;

/// Errors returned by the scheduler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerError {
    /// A config with a zero quota is rejected.
    InvalidConfig,
    /// The principal already holds its full concurrency quota.
    QuotaExhausted {
        principal: PrincipalId,
        quota: usize,
    },
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig => {
                write!(formatter, "scheduler quota must be non-zero")
            }
            Self::QuotaExhausted { principal, quota } => {
                write!(
                    formatter,
                    "principal {principal} already holds its quota of {quota} inflight units"
                )
            }
        }
    }
}

impl std::error::Error for SchedulerError {}

/// Scheduler configuration validated at construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerConfig {
    pub default_per_principal_quota: usize,
}

impl SchedulerConfig {
    pub fn new(default_per_principal_quota: usize) -> Result<Self, SchedulerError> {
        if default_per_principal_quota == 0 {
            return Err(SchedulerError::InvalidConfig);
        }
        Ok(Self {
            default_per_principal_quota,
        })
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            default_per_principal_quota: DEFAULT_PER_PRINCIPAL_QUOTA,
        }
    }
}

/// A snapshot of one principal's scheduler state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrincipalQuotaSnapshot {
    pub principal: PrincipalId,
    pub quota: usize,
    pub inflight: usize,
}

/// Deterministic per-principal concurrency accounting.
///
/// `acquire`/`release` are synchronous and lock-free; an async transport
/// adapter owns the waiting and calls `release` exactly once per acquired
/// slot, typically from a guard's `Drop`.
#[derive(Debug)]
pub struct Scheduler {
    config: SchedulerConfig,
    inflight: BTreeMap<PrincipalId, usize>,
}

impl Scheduler {
    pub fn with_config(config: SchedulerConfig) -> Self {
        Self {
            config,
            inflight: BTreeMap::new(),
        }
    }

    pub fn with_default_quota(quota: usize) -> Result<Self, SchedulerError> {
        Ok(Self::with_config(SchedulerConfig::new(quota)?))
    }

    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    pub fn inflight(&self, principal: &PrincipalId) -> usize {
        self.inflight.get(principal).copied().unwrap_or(0)
    }

    /// Reserve one slot of `principal`'s quota, or fail closed when the
    /// quota is already exhausted.
    pub fn acquire(&mut self, principal: &PrincipalId) -> Result<(), SchedulerError> {
        let quota = self.config.default_per_principal_quota;
        let current = self.inflight.get(principal).copied().unwrap_or(0);
        if current >= quota {
            return Err(SchedulerError::QuotaExhausted {
                principal: principal.clone(),
                quota,
            });
        }
        self.inflight.insert(principal.clone(), current + 1);
        Ok(())
    }

    /// Release one slot. Releasing an empty counter is a no-op so transport
    /// guards can release exactly once on every drop path.
    pub fn release(&mut self, principal: &PrincipalId) {
        let current = self.inflight.get(principal).copied().unwrap_or(0);
        if current <= 1 {
            self.inflight.remove(principal);
        } else {
            self.inflight.insert(principal.clone(), current - 1);
        }
    }

    pub fn snapshot(&self) -> Vec<PrincipalQuotaSnapshot> {
        self.inflight
            .iter()
            .map(|(principal, inflight)| PrincipalQuotaSnapshot {
                principal: principal.clone(),
                quota: self.config.default_per_principal_quota,
                inflight: *inflight,
            })
            .collect()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::with_config(SchedulerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(suffix: &str) -> PrincipalId {
        PrincipalId::new(format!("plugin.{suffix}")).expect("principal")
    }

    #[test]
    fn quota_honors_the_configured_default_per_principal() {
        let mut scheduler = Scheduler::with_default_quota(2).expect("quota two");
        let native = principal("native.ui");
        assert_eq!(scheduler.inflight(&native), 0);
        scheduler.acquire(&native).expect("first slot");
        scheduler.acquire(&native).expect("second slot");
        assert_eq!(
            scheduler.acquire(&native),
            Err(SchedulerError::QuotaExhausted {
                principal: native.clone(),
                quota: 2,
            })
        );
        assert_eq!(scheduler.inflight(&native), 2);
    }

    #[test]
    fn independent_principals_do_not_share_quota() {
        let mut scheduler = Scheduler::with_default_quota(1).expect("quota one");
        let first = principal("first");
        let second = principal("second");
        scheduler.acquire(&first).expect("first principal slot");
        scheduler
            .acquire(&second)
            .expect("second principal has its own slot");
        assert_eq!(scheduler.inflight(&first), 1);
        assert_eq!(scheduler.inflight(&second), 1);
        assert_eq!(scheduler.snapshot().len(), 2);
    }

    #[test]
    fn release_returns_a_slot_and_drains_the_counter() {
        let mut scheduler = Scheduler::with_default_quota(2).expect("quota two");
        let native = principal("native.ui");
        scheduler.acquire(&native).expect("first slot");
        scheduler.acquire(&native).expect("second slot");
        scheduler.release(&native);
        assert_eq!(scheduler.inflight(&native), 1);
        scheduler
            .acquire(&native)
            .expect("slot returns after release");
        scheduler.release(&native);
        scheduler.release(&native);
        assert_eq!(scheduler.inflight(&native), 0);
        assert!(scheduler.snapshot().is_empty());
    }

    #[test]
    fn zero_quota_is_rejected_at_construction() {
        assert!(matches!(
            Scheduler::with_default_quota(0),
            Err(SchedulerError::InvalidConfig)
        ));
    }
}
