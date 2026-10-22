//! Per-managed-daemon `last_touched` timestamp map (ADR-0004 §Activity-tracking).
//!
//! Owned by [`crate::daemon_runner::DaemonRunner`] and exposed via
//! [`DaemonRunner::activity_tracker`] / [`AppState::activity_tracker`].
//! The IPC chokepoint inside `crate::client::Client::touch_if_managed`
//! is the SOLE mutation site on the request path; the runner itself
//! seeds entries on `spawn`/`adopt` and forgets them on `stop` (success
//! paths only — the `StillAlive` failure path is forensically
//! interesting and stays in the map so the reconciler does not hammer
//! it with repeated idle flips).
//!
//! Implementation notes:
//!
//! - `tokio::time::Instant` (NOT `std::time::Instant`) so
//!   `tokio::time::pause` / `tokio::time::advance` drives both the
//!   recorded timestamps and the reconciler's elapsed-time arithmetic
//!   in tests.
//! - `std::sync::RwLock` is fine — every critical section is an O(1)
//!   `HashMap` op. We never `.await` while holding the guard.
//! - `Arc<Self>` is the canonical handle (the single instance lives
//!   on `DaemonRunner` and is cloned out via `activity_tracker()`).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::time::Instant;
use uuid::Uuid;

/// In-memory `last_touched: Instant` map keyed by managed-daemon
/// `Uuid`. Restart resets the clock — the map is intentionally NOT
/// persisted to SQLite (idle-timeout is a soft policy, not a
/// correctness invariant).
pub struct ActivityTracker {
    last_touched: RwLock<HashMap<Uuid, Instant>>,
}

impl ActivityTracker {
    /// Construct a fresh tracker wrapped in `Arc` for cheap clones
    /// across handler/reconciler/runner.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            last_touched: RwLock::new(HashMap::new()),
        })
    }

    /// Record `id` as just-now. Equivalent to [`Self::touch`]; a
    /// distinct name is offered for the seed-on-spawn / seed-on-adopt
    /// hook points so call sites read intentionally.
    pub fn seed(&self, id: Uuid) {
        self.touch(id);
    }

    /// Update the `last_touched` timestamp for `id` to the current
    /// `tokio::time::Instant::now()`. Inserts the entry if absent.
    pub fn touch(&self, id: Uuid) {
        let mut g = self
            .last_touched
            .write()
            .expect("activity tracker poisoned");
        g.insert(id, Instant::now());
    }

    /// Read the most recent `last_touched` timestamp for `id`, or
    /// `None` if the entry has never been seeded / has been forgotten.
    pub fn last_touched(&self, id: Uuid) -> Option<Instant> {
        let g = self.last_touched.read().expect("activity tracker poisoned");
        g.get(&id).copied()
    }

    /// Drop the entry for `id`. Called from `DaemonRunner::stop` on
    /// successful completion paths and from the reconciler tick after
    /// a successful idle-timeout `set_desired_state(_, Stopped)`
    /// write so the next sweep does not re-fire on a stale entry.
    pub fn forget(&self, id: Uuid) {
        let mut g = self
            .last_touched
            .write()
            .expect("activity tracker poisoned");
        g.remove(&id);
    }

    /// Number of tracked entries. Exposed for integration tests
    /// (e.g. `tests/client_activity_chokepoint.rs`) so they can
    /// assert "exactly one key" / "tracker is empty" invariants
    /// without reaching past the public API. No mutating side
    /// effects — safe to call from production code, though no
    /// production caller exists today.
    pub fn len(&self) -> usize {
        self.last_touched
            .read()
            .expect("activity tracker poisoned")
            .len()
    }

    /// True iff the tracker has zero entries. See [`Self::len`] for
    /// the rationale on always-pub.
    pub fn is_empty(&self) -> bool {
        self.last_touched
            .read()
            .expect("activity tracker poisoned")
            .is_empty()
    }
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self {
            last_touched: RwLock::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn touch_records_now_and_advances_with_virtual_time() {
        let t = ActivityTracker::new();
        let id = Uuid::new_v4();
        t.touch(id);
        let first = t.last_touched(id).expect("seeded");
        tokio::time::advance(Duration::from_secs(1)).await;
        t.touch(id);
        let second = t.last_touched(id).expect("re-touched");
        assert!(second > first, "second touch must be later than first");
    }

    #[tokio::test(start_paused = true)]
    async fn seed_is_an_alias_for_touch() {
        let t = ActivityTracker::new();
        let id = Uuid::new_v4();
        t.seed(id);
        assert!(t.last_touched(id).is_some());
        assert_eq!(t.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn forget_removes_the_entry() {
        let t = ActivityTracker::new();
        let id = Uuid::new_v4();
        t.seed(id);
        assert_eq!(t.len(), 1);
        t.forget(id);
        assert!(t.last_touched(id).is_none());
        assert!(t.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn forget_unknown_id_is_a_noop() {
        let t = ActivityTracker::new();
        let id = Uuid::new_v4();
        t.forget(id);
        assert!(t.is_empty());
    }
}
