//! Declarative lifecycle state machine for kernel-supervised workers.
//!
//! The machine only decides state transitions.  It does not spawn a thread,
//! signal a process, sleep for a backoff, or perform an OS-level kill.  Those
//! side effects belong to a supervisor adapter introduced in a later phase.

use std::fmt;

/// Desired lifecycle state expressed by a host or policy controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesiredState {
    Running,
    Stopped,
    Disabled,
}

/// Runtime lifecycle states shared by thread-pool tasks and external workers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleState {
    Declared,
    Starting,
    Ready,
    Draining,
    Stopped,
    Restarting,
    Failed,
    Quarantined,
}

impl LifecycleState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Failed | Self::Quarantined)
    }
}

/// Failure observations that can reach the state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureReason {
    Crash,
    StartupTimeout,
    HeartbeatTimeout,
    ProtocolViolation,
    ResourceExhausted,
    ShutdownTimeout,
    Requested,
}

/// Events accepted by the lifecycle machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleEvent {
    StartRequested,
    StartSucceeded,
    StopRequested,
    StopSucceeded,
    Failed(FailureReason),
    RetryTimerElapsed,
    DisableRequested,
    ResetRequested,
}

/// Whether a failed worker should be retried or made terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExhaustionAction {
    Fail,
    Quarantine,
}

/// Backoff strategy declared by policy, expressed in logical ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackoffPolicy {
    Constant { ticks: u64 },
    Exponential { initial_ticks: u64, max_ticks: u64 },
}

impl BackoffPolicy {
    pub fn delay_for(self, attempt: u32) -> u64 {
        let attempt = attempt.max(1);
        match self {
            Self::Constant { ticks } => ticks,
            Self::Exponential {
                initial_ticks,
                max_ticks,
            } => {
                let shift = attempt.saturating_sub(1).min(63);
                initial_ticks
                    .checked_shl(shift)
                    .unwrap_or(u64::MAX)
                    .min(max_ticks)
            }
        }
    }
}

/// Declarative restart triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestartTriggers {
    pub crash: bool,
    pub startup_timeout: bool,
    pub heartbeat_timeout: bool,
    pub protocol_violation: bool,
    pub resource_exhausted: bool,
}

impl RestartTriggers {
    pub const fn conservative() -> Self {
        Self {
            crash: true,
            startup_timeout: true,
            heartbeat_timeout: true,
            protocol_violation: false,
            resource_exhausted: false,
        }
    }

    fn allows(self, reason: FailureReason) -> bool {
        match reason {
            FailureReason::Crash => self.crash,
            FailureReason::StartupTimeout => self.startup_timeout,
            FailureReason::HeartbeatTimeout => self.heartbeat_timeout,
            FailureReason::ProtocolViolation => self.protocol_violation,
            FailureReason::ResourceExhausted => self.resource_exhausted,
            FailureReason::ShutdownTimeout | FailureReason::Requested => false,
        }
    }
}

/// Restart behavior attached to a lifecycle declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub backoff: BackoffPolicy,
    pub triggers: RestartTriggers,
    pub on_exhausted: ExhaustionAction,
    pub reset_after_stable_ticks: Option<u64>,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            backoff: BackoffPolicy::Exponential {
                initial_ticks: 1,
                max_ticks: 32,
            },
            triggers: RestartTriggers::conservative(),
            on_exhausted: ExhaustionAction::Quarantine,
            reset_after_stable_ticks: Some(30),
        }
    }
}

/// Timeouts and restart behavior declared for one worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleSpec {
    pub desired: DesiredState,
    pub startup_timeout_ticks: u64,
    pub shutdown_timeout_ticks: u64,
    pub heartbeat_timeout_ticks: u64,
    pub restart: RestartPolicy,
}

impl Default for LifecycleSpec {
    fn default() -> Self {
        Self {
            desired: DesiredState::Running,
            startup_timeout_ticks: 30,
            shutdown_timeout_ticks: 10,
            heartbeat_timeout_ticks: 15,
            restart: RestartPolicy::default(),
        }
    }
}

/// Action selected after observing a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartDecision {
    Restart { attempt: u32, delay_ticks: u64 },
    Fail,
    Quarantine,
    NoRestart,
}

/// A successful transition, including the side-effect decision for a supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleTransition {
    pub from: LifecycleState,
    pub event: LifecycleEvent,
    pub to: LifecycleState,
    pub restart: RestartDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleMachine {
    spec: LifecycleSpec,
    state: LifecycleState,
    restart_attempts: u32,
    stable_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTransition {
    pub state: LifecycleState,
    pub event: LifecycleEvent,
}

impl fmt::Display for InvalidTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "event {:?} is invalid in state {:?}",
            self.event, self.state
        )
    }
}

impl std::error::Error for InvalidTransition {}

impl LifecycleMachine {
    pub fn new(spec: LifecycleSpec) -> Self {
        let state = match spec.desired {
            DesiredState::Running => LifecycleState::Declared,
            DesiredState::Stopped => LifecycleState::Stopped,
            DesiredState::Disabled => LifecycleState::Quarantined,
        };
        Self {
            spec,
            state,
            restart_attempts: 0,
            stable_ticks: 0,
        }
    }

    pub fn spec(&self) -> LifecycleSpec {
        self.spec
    }

    pub fn state(&self) -> LifecycleState {
        self.state
    }

    pub fn restart_attempts(&self) -> u32 {
        self.restart_attempts
    }

    pub fn observe_healthy(&mut self, ticks: u64) {
        if self.state != LifecycleState::Ready {
            return;
        }
        self.stable_ticks = self.stable_ticks.saturating_add(ticks);
        if self
            .spec
            .restart
            .reset_after_stable_ticks
            .is_some_and(|threshold| self.stable_ticks >= threshold)
        {
            self.restart_attempts = 0;
            self.stable_ticks = 0;
        }
    }

    pub fn apply(
        &mut self,
        event: LifecycleEvent,
    ) -> Result<LifecycleTransition, InvalidTransition> {
        let from = self.state;
        let (to, restart) = match (from, event) {
            (LifecycleState::Declared, LifecycleEvent::StartRequested)
            | (LifecycleState::Stopped, LifecycleEvent::StartRequested) => {
                (LifecycleState::Starting, RestartDecision::NoRestart)
            }
            (LifecycleState::Starting, LifecycleEvent::StartSucceeded) => {
                (LifecycleState::Ready, RestartDecision::NoRestart)
            }
            (LifecycleState::Declared, LifecycleEvent::StopRequested) => {
                (LifecycleState::Stopped, RestartDecision::NoRestart)
            }
            (
                LifecycleState::Starting
                | LifecycleState::Ready
                | LifecycleState::Restarting
                | LifecycleState::Failed,
                LifecycleEvent::StopRequested,
            ) => (LifecycleState::Draining, RestartDecision::NoRestart),
            (LifecycleState::Draining, LifecycleEvent::StopSucceeded) => {
                (LifecycleState::Stopped, RestartDecision::NoRestart)
            }
            (LifecycleState::Restarting, LifecycleEvent::RetryTimerElapsed) => {
                (LifecycleState::Starting, RestartDecision::NoRestart)
            }
            (
                LifecycleState::Failed | LifecycleState::Quarantined,
                LifecycleEvent::ResetRequested,
            ) => {
                self.restart_attempts = 0;
                self.stable_ticks = 0;
                (LifecycleState::Declared, RestartDecision::NoRestart)
            }
            (_, LifecycleEvent::DisableRequested) => {
                (LifecycleState::Quarantined, RestartDecision::Quarantine)
            }
            (LifecycleState::Starting | LifecycleState::Ready, LifecycleEvent::Failed(reason)) => {
                self.stable_ticks = 0;
                if reason == FailureReason::Requested || !self.spec.restart.triggers.allows(reason)
                {
                    (LifecycleState::Failed, RestartDecision::NoRestart)
                } else if self.restart_attempts < self.spec.restart.max_restarts {
                    self.restart_attempts = self.restart_attempts.saturating_add(1);
                    (
                        LifecycleState::Restarting,
                        RestartDecision::Restart {
                            attempt: self.restart_attempts,
                            delay_ticks: self.spec.restart.backoff.delay_for(self.restart_attempts),
                        },
                    )
                } else {
                    match self.spec.restart.on_exhausted {
                        ExhaustionAction::Fail => (LifecycleState::Failed, RestartDecision::Fail),
                        ExhaustionAction::Quarantine => {
                            (LifecycleState::Quarantined, RestartDecision::Quarantine)
                        }
                    }
                }
            }
            (LifecycleState::Draining, LifecycleEvent::Failed(_)) => {
                (LifecycleState::Failed, RestartDecision::NoRestart)
            }
            _ => return Err(InvalidTransition { state: from, event }),
        };

        self.state = to;
        Ok(LifecycleTransition {
            from,
            event,
            to,
            restart,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_machine() -> LifecycleMachine {
        LifecycleMachine::new(LifecycleSpec::default())
    }

    #[test]
    fn normal_start_and_stop_are_explicit() {
        let mut machine = running_machine();
        assert_eq!(machine.state(), LifecycleState::Declared);
        machine
            .apply(LifecycleEvent::StartRequested)
            .expect("start is valid");
        machine
            .apply(LifecycleEvent::StartSucceeded)
            .expect("ready is valid");
        assert_eq!(machine.state(), LifecycleState::Ready);
        machine
            .apply(LifecycleEvent::StopRequested)
            .expect("drain is valid");
        machine
            .apply(LifecycleEvent::StopSucceeded)
            .expect("stop is valid");
        assert_eq!(machine.state(), LifecycleState::Stopped);
    }

    #[test]
    fn crash_uses_exponential_restart_and_quarantines_after_exhaustion() {
        let mut machine = running_machine();
        machine.apply(LifecycleEvent::StartRequested).unwrap();
        machine.apply(LifecycleEvent::StartSucceeded).unwrap();

        for (attempt, delay) in [(1, 1), (2, 2), (3, 4)] {
            let transition = machine
                .apply(LifecycleEvent::Failed(FailureReason::Crash))
                .expect("crash is restartable");
            assert_eq!(transition.to, LifecycleState::Restarting);
            assert_eq!(
                transition.restart,
                RestartDecision::Restart {
                    attempt,
                    delay_ticks: delay
                }
            );
            machine.apply(LifecycleEvent::RetryTimerElapsed).unwrap();
            machine.apply(LifecycleEvent::StartSucceeded).unwrap();
        }

        let exhausted = machine
            .apply(LifecycleEvent::Failed(FailureReason::Crash))
            .expect("exhaustion is a valid transition");
        assert_eq!(exhausted.to, LifecycleState::Quarantined);
        assert_eq!(exhausted.restart, RestartDecision::Quarantine);
    }

    #[test]
    fn protocol_violation_is_not_restarted_by_conservative_policy() {
        let mut machine = running_machine();
        machine.apply(LifecycleEvent::StartRequested).unwrap();
        machine.apply(LifecycleEvent::StartSucceeded).unwrap();

        let transition = machine
            .apply(LifecycleEvent::Failed(FailureReason::ProtocolViolation))
            .expect("failure is a valid transition");
        assert_eq!(transition.to, LifecycleState::Failed);
        assert_eq!(transition.restart, RestartDecision::NoRestart);
    }

    #[test]
    fn stable_health_resets_restart_budget() {
        let mut machine = running_machine();
        machine.apply(LifecycleEvent::StartRequested).unwrap();
        machine.apply(LifecycleEvent::StartSucceeded).unwrap();
        machine
            .apply(LifecycleEvent::Failed(FailureReason::Crash))
            .unwrap();
        assert_eq!(machine.restart_attempts(), 1);
        machine.apply(LifecycleEvent::RetryTimerElapsed).unwrap();
        machine.apply(LifecycleEvent::StartSucceeded).unwrap();
        machine.observe_healthy(30);
        assert_eq!(machine.restart_attempts(), 0);
    }

    #[test]
    fn invalid_events_do_not_mutate_state() {
        let mut machine = running_machine();
        let error = machine
            .apply(LifecycleEvent::StartSucceeded)
            .expect_err("ready cannot precede start");
        assert_eq!(error.state, LifecycleState::Declared);
        assert_eq!(machine.state(), LifecycleState::Declared);
    }

    #[test]
    fn restart_requires_a_new_starting_transition_before_ready() {
        let mut machine = running_machine();
        machine.apply(LifecycleEvent::StartRequested).unwrap();
        machine.apply(LifecycleEvent::StartSucceeded).unwrap();
        machine
            .apply(LifecycleEvent::Failed(FailureReason::Crash))
            .unwrap();

        let error = machine
            .apply(LifecycleEvent::StartSucceeded)
            .expect_err("restarting worker cannot report ready before backoff");
        assert_eq!(error.state, LifecycleState::Restarting);
        assert_eq!(machine.state(), LifecycleState::Restarting);
    }
}
