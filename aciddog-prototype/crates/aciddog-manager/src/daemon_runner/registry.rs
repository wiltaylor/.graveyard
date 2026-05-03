//! Lock discipline — hold the mutex briefly to read or insert; never
//! `.await` blocking I/O while holding. For `stop`, take the entry out
//! of the map (or extract the `Child`, drop the lock, await, reacquire).
//!
//! Single global `tokio::sync::Mutex<HashMap<Uuid, ChildHandle>>`. Every
//! method on `Registry` is intentionally short — long-running work
//! (signal delivery, `child.wait()`, socket probing) lives in the
//! `DaemonRunner` impl and explicitly drops the guard before awaiting.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use tokio::process::Child;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::process::ObservedState;

/// Per-daemon state held by the runner.
///
/// `child` is `Some` when the manager spawned the process and therefore
/// owns the OS handle, and `None` when the entry was adopted via
/// `DaemonRunner::adopt` (DECIDE-B v1 — observe-only, see plan §H).
///
/// Some fields (`work_dir`, `socket_path`, `spawned_at`) are stored for
/// the imminent reconciler tick (sibling `d67a4d0c`) and the per-daemon
/// status response (sibling `b91e9691`). Allow `dead_code` so this
/// slice can land without a sibling crate to consume them yet.
#[allow(dead_code)]
pub(crate) struct ChildHandle {
    /// `Some` if this manager spawned the process; `None` for adopted
    /// (observe-only) entries.
    pub(crate) child: Option<Child>,
    /// Mirrors `child.id()` for managed entries; `None` for adopted v1.
    pub(crate) pid: Option<u32>,
    pub(crate) state: ObservedState,
    pub(crate) work_dir: PathBuf,
    pub(crate) socket_path: PathBuf,
    pub(crate) spawned_at: Instant,
}

/// Thin wrapper around a `tokio::sync::Mutex<HashMap<Uuid, ChildHandle>>`.
pub(crate) struct Registry {
    inner: Mutex<HashMap<Uuid, ChildHandle>>,
}

impl Registry {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Insert a new entry. Returns `Err(handle)` (the rejected handle)
    /// when `id` is already tracked so the caller can recover the
    /// `Child` if needed.
    pub(crate) async fn insert(&self, id: Uuid, handle: ChildHandle) -> Result<(), ChildHandle> {
        let mut guard = self.inner.lock().await;
        if guard.contains_key(&id) {
            return Err(handle);
        }
        guard.insert(id, handle);
        Ok(())
    }

    /// Take the entry out of the map and return it.
    pub(crate) async fn remove(&self, id: Uuid) -> Option<ChildHandle> {
        let mut guard = self.inner.lock().await;
        guard.remove(&id)
    }

    /// Quick state read — copies the `ObservedState` and drops the guard.
    pub(crate) async fn state_of(&self, id: Uuid) -> Option<ObservedState> {
        let guard = self.inner.lock().await;
        guard.get(&id).map(|h| h.state)
    }

    /// Acquire the lock briefly, run `f` against a mutable handle, drop
    /// the lock. `f` must not perform blocking or async I/O.
    pub(crate) async fn with_handle_mut<F, R>(&self, id: Uuid, f: F) -> Option<R>
    where
        F: FnOnce(&mut ChildHandle) -> R,
    {
        let mut guard = self.inner.lock().await;
        guard.get_mut(&id).map(f)
    }

    /// Check whether an id is tracked, without copying the state.
    pub(crate) async fn contains(&self, id: Uuid) -> bool {
        let guard = self.inner.lock().await;
        guard.contains_key(&id)
    }
}
