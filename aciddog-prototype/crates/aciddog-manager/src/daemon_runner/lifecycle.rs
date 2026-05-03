//! Pure-fn lifecycle state machine for the daemon_runner subsystem.
//!
//! Consumed by both the public `DaemonRunner` API and the future
//! reconciler tick (sibling task `d67a4d0c`). The function below is
//! intentionally pure — no I/O, no allocation, no async — so it can be
//! covered exhaustively by table-driven unit tests.

use super::placeholder::DesiredState;
use super::process::ObservedState;

/// The action a reconciler should take to drive `observed` toward
/// `desired`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReconcilerAction {
    /// No action required; observed is already aligned (or in a
    /// transient state that will resolve itself).
    NoOp,
    /// Spawn a fresh child process via `DaemonRunner::spawn`.
    Spawn,
    /// Stop the running child via `DaemonRunner::stop`.
    Stop,
    /// The child crashed while desired=Running. v1 surfaces this loudly
    /// for an operator to act on rather than auto-respawning, in line
    /// with the loud-failure ethos recorded in `decisions/59ff479f`.
    ///
    /// Auto-recover-with-backoff is a deliberate v2 follow-up — it
    /// requires per-handle backoff state and a circuit breaker which
    /// is out of scope for this slice.
    AlertCrashed,
}

/// Map a `(desired, observed)` pair to a `ReconcilerAction` per §G of
/// the implementation plan.
///
/// Truth table (rows are exhaustive):
///
/// | desired | observed | action       |
/// |---------|----------|--------------|
/// | Running | Stopped  | Spawn        |
/// | Running | Crashed  | AlertCrashed |
/// | Running | Running  | NoOp         |
/// | Running | Starting | NoOp         |
/// | Running | Stopping | NoOp         |
/// | Stopped | Running  | Stop         |
/// | Stopped | Starting | Stop         |
/// | Stopped | Stopping | NoOp         |
/// | Stopped | Stopped  | NoOp         |
/// | Stopped | Crashed  | NoOp         |
pub fn next_action(desired: DesiredState, observed: ObservedState) -> ReconcilerAction {
    match (desired, observed) {
        (DesiredState::Running, ObservedState::Stopped) => ReconcilerAction::Spawn,
        (DesiredState::Running, ObservedState::Crashed) => ReconcilerAction::AlertCrashed,
        (DesiredState::Running, ObservedState::Running) => ReconcilerAction::NoOp,
        (DesiredState::Running, ObservedState::Starting) => ReconcilerAction::NoOp,
        (DesiredState::Running, ObservedState::Stopping) => ReconcilerAction::NoOp,
        (DesiredState::Stopped, ObservedState::Running) => ReconcilerAction::Stop,
        (DesiredState::Stopped, ObservedState::Starting) => ReconcilerAction::Stop,
        (DesiredState::Stopped, ObservedState::Stopping) => ReconcilerAction::NoOp,
        (DesiredState::Stopped, ObservedState::Stopped) => ReconcilerAction::NoOp,
        (DesiredState::Stopped, ObservedState::Crashed) => ReconcilerAction::NoOp,
    }
}
