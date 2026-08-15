//! Pure supervisor model shared by thread-pool tasks and external processes.
//!
//! This phase intentionally stops at planning and bookkeeping.  `Supervisor`
//! emits typed actions; a future platform adapter will map them to Tokio task
//! cancellation, a bounded thread pool, or OS process control.  No OS sandbox
//! is implemented by this module.

use super::{identity::*, lifecycle::*};

use std::collections::BTreeMap;

/// Failure domain used for restart and blast-radius accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureDomain {
    /// Tasks share the host process and may share a pool's executor state.
    SharedThreadPool,
    /// The worker is expected to have a process boundary, but this type does
    /// not create or sandbox that process.
    ExternalProcess,
}

/// Execution kind selected by a worker declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerKind {
    ThreadPool { pool: String },
    ExternalProcess { executable: String },
}

impl WorkerKind {
    pub fn failure_domain(&self) -> FailureDomain {
        match self {
            Self::ThreadPool { .. } => FailureDomain::SharedThreadPool,
            Self::ExternalProcess { .. } => FailureDomain::ExternalProcess,
        }
    }
}

/// Declarative worker registration consumed by the supervisor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerSpec {
    pub worker_id: WorkerId,
    pub principal: PrincipalId,
    pub plugin_instance: Option<PluginInstanceId>,
    pub kind: WorkerKind,
    pub lifecycle: LifecycleSpec,
}

impl WorkerSpec {
    pub fn thread_pool(
        worker_id: WorkerId,
        principal: PrincipalId,
        plugin_instance: Option<PluginInstanceId>,
        pool: impl Into<String>,
        lifecycle: LifecycleSpec,
    ) -> Self {
        Self {
            worker_id,
            principal,
            plugin_instance,
            kind: WorkerKind::ThreadPool { pool: pool.into() },
            lifecycle,
        }
    }

    pub fn external_process(
        worker_id: WorkerId,
        principal: PrincipalId,
        plugin_instance: Option<PluginInstanceId>,
        executable: impl Into<String>,
        lifecycle: LifecycleSpec,
    ) -> Self {
        Self {
            worker_id,
            principal,
            plugin_instance,
            kind: WorkerKind::ExternalProcess {
                executable: executable.into(),
            },
            lifecycle,
        }
    }
}

/// Observations delivered by a platform adapter or RPC endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerObservation {
    Started,
    Stopped,
    Failed(FailureReason),
    RetryTimerElapsed,
    Healthy { ticks: u64 },
}

/// Side effects requested from a platform adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorAction {
    Start {
        worker_id: WorkerId,
        domain: FailureDomain,
    },
    Stop {
        worker_id: WorkerId,
        domain: FailureDomain,
    },
    Restart {
        worker_id: WorkerId,
        domain: FailureDomain,
        attempt: u32,
        delay_ticks: u64,
    },
    Quarantine {
        worker_id: WorkerId,
        reason: FailureReason,
    },
    ReportFailure {
        worker_id: WorkerId,
        domain: FailureDomain,
        reason: FailureReason,
    },
    Noop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerSnapshot {
    pub worker_id: WorkerId,
    pub principal: PrincipalId,
    pub plugin_instance: Option<PluginInstanceId>,
    pub domain: FailureDomain,
    pub state: LifecycleState,
    pub restart_attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorError {
    DuplicateWorker(WorkerId),
    UnknownWorker(WorkerId),
    Lifecycle(InvalidTransition),
}

impl From<InvalidTransition> for SupervisorError {
    fn from(error: InvalidTransition) -> Self {
        Self::Lifecycle(error)
    }
}

struct WorkerRecord {
    spec: WorkerSpec,
    lifecycle: LifecycleMachine,
}

/// A deterministic registry and action planner for supervised workers.
#[derive(Default)]
pub struct Supervisor {
    workers: BTreeMap<WorkerId, WorkerRecord>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: WorkerSpec) -> Result<(), SupervisorError> {
        if self.workers.contains_key(&spec.worker_id) {
            return Err(SupervisorError::DuplicateWorker(spec.worker_id));
        }
        let lifecycle = LifecycleMachine::new(spec.lifecycle);
        self.workers
            .insert(spec.worker_id.clone(), WorkerRecord { spec, lifecycle });
        Ok(())
    }

    pub fn snapshot(&self, worker_id: &WorkerId) -> Result<WorkerSnapshot, SupervisorError> {
        let record = self
            .workers
            .get(worker_id)
            .ok_or_else(|| SupervisorError::UnknownWorker(worker_id.clone()))?;
        Ok(WorkerSnapshot {
            worker_id: record.spec.worker_id.clone(),
            principal: record.spec.principal.clone(),
            plugin_instance: record.spec.plugin_instance.clone(),
            domain: record.spec.kind.failure_domain(),
            state: record.lifecycle.state(),
            restart_attempts: record.lifecycle.restart_attempts(),
        })
    }

    pub fn start(&mut self, worker_id: &WorkerId) -> Result<SupervisorAction, SupervisorError> {
        let record = self.record_mut(worker_id)?;
        record.lifecycle.apply(LifecycleEvent::StartRequested)?;
        Ok(SupervisorAction::Start {
            worker_id: worker_id.clone(),
            domain: record.spec.kind.failure_domain(),
        })
    }

    pub fn request_stop(
        &mut self,
        worker_id: &WorkerId,
    ) -> Result<SupervisorAction, SupervisorError> {
        let record = self.record_mut(worker_id)?;
        record.lifecycle.apply(LifecycleEvent::StopRequested)?;
        Ok(SupervisorAction::Stop {
            worker_id: worker_id.clone(),
            domain: record.spec.kind.failure_domain(),
        })
    }

    pub fn observe(
        &mut self,
        worker_id: &WorkerId,
        observation: WorkerObservation,
    ) -> Result<SupervisorAction, SupervisorError> {
        let record = self.record_mut(worker_id)?;
        match observation {
            WorkerObservation::Started => {
                record.lifecycle.apply(LifecycleEvent::StartSucceeded)?;
                Ok(SupervisorAction::Noop)
            }
            WorkerObservation::Stopped => {
                record.lifecycle.apply(LifecycleEvent::StopSucceeded)?;
                Ok(SupervisorAction::Noop)
            }
            WorkerObservation::RetryTimerElapsed => {
                record.lifecycle.apply(LifecycleEvent::RetryTimerElapsed)?;
                Ok(SupervisorAction::Start {
                    worker_id: worker_id.clone(),
                    domain: record.spec.kind.failure_domain(),
                })
            }
            WorkerObservation::Healthy { ticks } => {
                record.lifecycle.observe_healthy(ticks);
                Ok(SupervisorAction::Noop)
            }
            WorkerObservation::Failed(reason) => {
                let transition = record.lifecycle.apply(LifecycleEvent::Failed(reason))?;
                let domain = record.spec.kind.failure_domain();
                Ok(match transition.restart {
                    RestartDecision::Restart {
                        attempt,
                        delay_ticks,
                    } => SupervisorAction::Restart {
                        worker_id: worker_id.clone(),
                        domain,
                        attempt,
                        delay_ticks,
                    },
                    RestartDecision::Quarantine => SupervisorAction::Quarantine {
                        worker_id: worker_id.clone(),
                        reason,
                    },
                    RestartDecision::Fail => SupervisorAction::ReportFailure {
                        worker_id: worker_id.clone(),
                        domain,
                        reason,
                    },
                    RestartDecision::NoRestart => SupervisorAction::ReportFailure {
                        worker_id: worker_id.clone(),
                        domain,
                        reason,
                    },
                })
            }
        }
    }

    fn record_mut(&mut self, worker_id: &WorkerId) -> Result<&mut WorkerRecord, SupervisorError> {
        self.workers
            .get_mut(worker_id)
            .ok_or_else(|| SupervisorError::UnknownWorker(worker_id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(suffix: &str) -> (WorkerId, PrincipalId, PluginInstanceId) {
        let principal = PrincipalId::new(format!("plugin.{suffix}")).expect("principal");
        let instance = PluginInstanceId::new(principal.clone(), "instance-1").expect("instance");
        let worker = WorkerId::new(format!("worker.{suffix}")).expect("worker");
        (worker, principal, instance)
    }

    #[test]
    fn thread_and_process_workers_share_the_supervisor_but_not_domain() {
        let (thread_worker, principal, instance) = ids("thread");
        let (process_worker, process_principal, process_instance) = ids("process");
        let mut supervisor = Supervisor::new();
        supervisor
            .register(WorkerSpec::thread_pool(
                thread_worker.clone(),
                principal,
                Some(instance),
                "analysis",
                LifecycleSpec::default(),
            ))
            .expect("thread worker registers");
        supervisor
            .register(WorkerSpec::external_process(
                process_worker.clone(),
                process_principal,
                Some(process_instance),
                "anmarket-worker",
                LifecycleSpec::default(),
            ))
            .expect("process worker registers");

        assert_eq!(
            supervisor.snapshot(&thread_worker).unwrap().domain,
            FailureDomain::SharedThreadPool
        );
        assert_eq!(
            supervisor.snapshot(&process_worker).unwrap().domain,
            FailureDomain::ExternalProcess
        );
    }

    #[test]
    fn supervisor_maps_a_crash_to_a_typed_restart_action() {
        let (worker, principal, instance) = ids("restart");
        let mut supervisor = Supervisor::new();
        supervisor
            .register(WorkerSpec::external_process(
                worker.clone(),
                principal,
                Some(instance),
                "worker",
                LifecycleSpec::default(),
            ))
            .unwrap();
        assert!(matches!(
            supervisor.start(&worker).unwrap(),
            SupervisorAction::Start {
                domain: FailureDomain::ExternalProcess,
                ..
            }
        ));
        supervisor
            .observe(&worker, WorkerObservation::Started)
            .unwrap();

        assert_eq!(
            supervisor
                .observe(&worker, WorkerObservation::Failed(FailureReason::Crash))
                .unwrap(),
            SupervisorAction::Restart {
                worker_id: worker.clone(),
                domain: FailureDomain::ExternalProcess,
                attempt: 1,
                delay_ticks: 1,
            }
        );
        assert_eq!(
            supervisor.snapshot(&worker).unwrap().state,
            LifecycleState::Restarting
        );
    }

    #[test]
    fn protocol_violation_is_reported_without_restart() {
        let (worker, principal, instance) = ids("protocol");
        let mut supervisor = Supervisor::new();
        supervisor
            .register(WorkerSpec::thread_pool(
                worker.clone(),
                principal,
                Some(instance),
                "analysis",
                LifecycleSpec::default(),
            ))
            .unwrap();
        supervisor.start(&worker).unwrap();
        supervisor
            .observe(&worker, WorkerObservation::Started)
            .unwrap();

        assert!(matches!(
            supervisor
                .observe(
                    &worker,
                    WorkerObservation::Failed(FailureReason::ProtocolViolation)
                )
                .unwrap(),
            SupervisorAction::ReportFailure {
                domain: FailureDomain::SharedThreadPool,
                reason: FailureReason::ProtocolViolation,
                ..
            }
        ));
        assert_eq!(
            supervisor.snapshot(&worker).unwrap().state,
            LifecycleState::Failed
        );
    }

    #[test]
    fn duplicate_and_unknown_workers_are_rejected() {
        let (worker, principal, instance) = ids("duplicate");
        let spec = WorkerSpec::thread_pool(
            worker.clone(),
            principal,
            Some(instance),
            "analysis",
            LifecycleSpec::default(),
        );
        let mut supervisor = Supervisor::new();
        supervisor.register(spec.clone()).unwrap();
        assert_eq!(
            supervisor.register(spec),
            Err(SupervisorError::DuplicateWorker(worker.clone()))
        );
        let missing = WorkerId::new("missing").unwrap();
        assert_eq!(
            supervisor.start(&missing),
            Err(SupervisorError::UnknownWorker(missing))
        );
    }
}
