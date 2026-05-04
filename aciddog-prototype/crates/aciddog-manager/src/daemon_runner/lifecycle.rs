//! Pure-fn lifecycle state machine for the daemon_runner subsystem.
//!
//! Consumed by both the public `DaemonRunner` API and the future
//! reconciler tick (sibling task `d67a4d0c`). The function below is
//! intentionally pure — no I/O, no allocation, no async — so it can be
//! covered exhaustively by table-driven unit tests.

use std::time::Duration;

use tokio::time::Instant;

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

/// ADR-0004 §Activity-tracking — pure-fn idle-timeout policy.
///
/// Returns `Some(DesiredState::Stopped)` iff:
///   1. an idle timeout is configured (`idle.is_some()`),
///   2. the daemon's observed state is exactly `Running`, and
///   3. `now - last_touched > idle`  (strict greater-than — at-the-
///      timeout still counts as in-window so the test fixtures see
///      the boundary cleanly).
///
/// All other cases return `None` (no policy action).
///
/// Pure — no I/O, no allocation, no async. The reconciler tick is
/// the only caller; it consumes `Some(Stopped)` by writing
/// `set_desired_state(_, Stopped)` to the `managed_daemons` row,
/// then `next_action` (above) maps the now-stale `(Stopped,
/// Running)` pair to `Stop` on a subsequent tick.
pub fn idle_timeout_action(
    idle: Option<Duration>,
    observed: ObservedState,
    last_touched: Instant,
    now: Instant,
) -> Option<DesiredState> {
    let d = idle?;
    if observed != ObservedState::Running {
        return None;
    }
    if now.saturating_duration_since(last_touched) > d {
        Some(DesiredState::Stopped)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct the `(last_touched, now)` Instant pair via a base
    /// `tokio::time::Instant::now()` plus the offset `(now - last)`.
    /// Keeps the truth-table loop tight.
    fn instants_with_gap(gap: Duration) -> (Instant, Instant) {
        let last = Instant::now();
        let now = last + gap;
        (last, now)
    }

    /// Exhaustive truth table for `idle_timeout_action`:
    ///
    ///   idle      ∈ {None, Some(5s)}            (2 rows)
    ///   observed  ∈ {Stopped, Starting,         (5 rows)
    ///                Running, Stopping, Crashed}
    ///   gap       ∈ {0, 4s, 5s, 6s}             (4 rows)
    ///
    /// 2 × 5 × 4 = 40 cells. Only the cell
    /// `(Some(5s), Running, gap=6s)` returns `Some(Stopped)`; the
    /// other 39 cells return `None`. The strict greater-than at the
    /// 5s boundary is the only subtle case.
    #[tokio::test]
    async fn idle_timeout_action_truth_table() {
        let idles: [Option<Duration>; 2] = [None, Some(Duration::from_secs(5))];
        let observeds = [
            ObservedState::Stopped,
            ObservedState::Starting,
            ObservedState::Running,
            ObservedState::Stopping,
            ObservedState::Crashed,
        ];
        let gaps = [
            Duration::from_secs(0),
            Duration::from_secs(4),
            Duration::from_secs(5),
            Duration::from_secs(6),
        ];

        for &idle in &idles {
            for &observed in &observeds {
                for &gap in &gaps {
                    let (last, now) = instants_with_gap(gap);
                    let got = idle_timeout_action(idle, observed, last, now);
                    let expected = if matches!(idle, Some(d) if gap > d)
                        && observed == ObservedState::Running
                    {
                        Some(DesiredState::Stopped)
                    } else {
                        None
                    };
                    assert_eq!(
                        got, expected,
                        "idle_timeout_action(idle={idle:?}, observed={observed:?}, gap={gap:?}) \
                         expected {expected:?}, got {got:?}"
                    );
                }
            }
        }
    }

    /// Boundary check: `gap == idle` is in-window (returns None) per
    /// the strict greater-than. Pin this independently so a regression
    /// to `>=` is named.
    #[tokio::test]
    async fn idle_timeout_action_at_exact_boundary_is_in_window() {
        let (last, now) = instants_with_gap(Duration::from_secs(5));
        let got = idle_timeout_action(
            Some(Duration::from_secs(5)),
            ObservedState::Running,
            last,
            now,
        );
        assert_eq!(got, None, "gap == idle must be treated as in-window");
    }

    /// Just past the boundary flips to Stopped.
    #[tokio::test]
    async fn idle_timeout_action_just_past_boundary_flips_stopped() {
        let (last, now) = instants_with_gap(Duration::from_millis(5_001));
        let got = idle_timeout_action(
            Some(Duration::from_secs(5)),
            ObservedState::Running,
            last,
            now,
        );
        assert_eq!(got, Some(DesiredState::Stopped));
    }
}
