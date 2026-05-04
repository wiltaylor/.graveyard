//! NAMING — this is `daemon_runner`, NOT `supervisor`. WCL
//! `supervisor: bool` at `crates/aciddog/src/config.rs:90` is an
//! authorisation concept; this subsystem manages OS child processes.
//! See ADR-0001.
//!
//! Sibling **b91e9691** must rename `observed_state`/`signal`/
//! `stop_with_grace` to `status`/`reload`/`stop` at integration time.
//! The canonical names — fixed by THIS slice — are
//! `status(id) / reload(id) / stop(id, timeout) / spawn(row) /
//! adopt(id, socket_path)`.

mod activity;
mod lifecycle;
mod log_capture;
mod process;
mod registry;

use std::env;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;
use std::time::Instant;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::process::Child;
use uuid::Uuid;

use registry::{ChildHandle, Registry};

pub use activity::ActivityTracker;
pub use lifecycle::{idle_timeout_action, next_action, ReconcilerAction};
pub use log_capture::{
    CapturePolicy, CapturedLine, OptionalCapturePolicy, RingBufferHandle, Stream,
};
pub use process::ObservedState;

/// TEMPORARY: replace with `model::managed_daemon::*` once sibling
/// `7c11fa3d` lands. Internal to `daemon_runner` until then.
pub(crate) mod placeholder {
    use std::path::PathBuf;
    use uuid::Uuid;

    /// Stand-in for `model::managed_daemon::ManagedDaemonRow`.
    pub struct ManagedDaemonRow {
        pub id: Uuid,
        pub work_dir: PathBuf,
        pub socket_path: PathBuf,
    }

    /// Stand-in for `model::managed_daemon::DesiredState`.
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub enum DesiredState {
        Running,
        Stopped,
    }
}

pub use placeholder::{DesiredState, ManagedDaemonRow};

// Use `impl Future` in trait position (Rust 1.75+, repo has no MSRV
// floor and ships with cargo 1.92). Keeps the unit-test seam free of
// the `async-trait` macro and avoids an extra dependency.
/// Test seam for the reconciler: the four `DaemonRunner` methods that
/// the reconciler dispatches to. Production wiring uses the blanket
/// `impl DaemonRunnerControl for Arc<DaemonRunner>` below; unit tests
/// substitute a mock.
pub trait DaemonRunnerControl: Send + Sync + 'static {
    fn spawn(
        &self,
        row: ManagedDaemonRow,
    ) -> impl Future<Output = Result<(), DaemonRunnerError>> + Send;
    fn stop(
        &self,
        id: Uuid,
        timeout: Duration,
    ) -> impl Future<Output = Result<(), DaemonRunnerError>> + Send;
    fn status(&self, id: Uuid) -> impl Future<Output = Option<ObservedState>> + Send;
    fn adopt(
        &self,
        id: Uuid,
        socket_path: PathBuf,
    ) -> impl Future<Output = Result<AdoptOutcome, DaemonRunnerError>> + Send;
}

impl DaemonRunnerControl for Arc<DaemonRunner> {
    fn spawn(
        &self,
        row: ManagedDaemonRow,
    ) -> impl Future<Output = Result<(), DaemonRunnerError>> + Send {
        let this = Arc::clone(self);
        async move { DaemonRunner::spawn(&this, row).await }
    }

    fn stop(
        &self,
        id: Uuid,
        timeout: Duration,
    ) -> impl Future<Output = Result<(), DaemonRunnerError>> + Send {
        let this = Arc::clone(self);
        async move { DaemonRunner::stop(&this, id, timeout).await }
    }

    fn status(&self, id: Uuid) -> impl Future<Output = Option<ObservedState>> + Send {
        let this = Arc::clone(self);
        async move { DaemonRunner::status(&this, id).await }
    }

    fn adopt(
        &self,
        id: Uuid,
        socket_path: PathBuf,
    ) -> impl Future<Output = Result<AdoptOutcome, DaemonRunnerError>> + Send {
        let this = Arc::clone(self);
        async move { DaemonRunner::adopt(&this, id, socket_path).await }
    }
}

/// Outcome of an `adopt` call.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AdoptOutcome {
    /// Socket responded; entry inserted as observe-only.
    Adopted,
    /// Socket did not respond inside the probe budget; nothing was
    /// inserted.
    NotResponding,
}

/// Errors surfaced by the public `DaemonRunner` API.
///
/// Variants other than `BinaryNotFound` are copied verbatim from §N of
/// the implementation plan — sibling `b91e9691` maps each to a specific
/// HTTP status code.
#[derive(thiserror::Error, Debug)]
pub enum DaemonRunnerError {
    #[error("daemon {0} is already tracked")]
    AlreadyTracked(Uuid),
    #[error("daemon {0} is not tracked")]
    NotFound(Uuid),
    #[error("daemon {0} was adopted; control via aciddog CLI")]
    AdoptedNotControlled(Uuid),
    #[error("daemon {0} is not in Running state")]
    NotRunning(Uuid),
    #[error("daemon {0} did not exit after SIGKILL")]
    StillAlive(Uuid),
    #[error("spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("socket probe failed: {0}")]
    Probe(String),
    #[error("signal delivery failed: {0}")]
    Signal(#[from] nix::Error),
    /// Resolver could not locate an `aciddog` binary on disk.
    #[error("aciddog binary not found")]
    BinaryNotFound,
}

/// Public façade owning the registry and the cached path to the
/// `aciddog` binary used to spawn children.
pub struct DaemonRunner {
    aciddog_binary: PathBuf,
    capture_policy: CapturePolicy,
    registry: Registry,
    /// Per-managed-daemon `last_touched: Instant` map. ADR-0004
    /// §Activity-tracking — seeded on `spawn`/`adopt`, touched by the
    /// IPC client chokepoint, forgotten on successful `stop` and on
    /// reconciler-driven idle flips.
    activity: Arc<ActivityTracker>,
}

impl DaemonRunner {
    /// Construct a new runner. Wrap in `Arc` so background spawn-watch
    /// tasks can hold a `Weak` back-pointer without pinning the runner
    /// forever.
    ///
    /// The `capture_policy` drives the per-child stdio capture (sibling
    /// 3bc8a0d0). Tests and smoke paths can pass
    /// [`CapturePolicy::default`]; production wires the value from
    /// `--daemon-runner-log-*` CLI flags via `AppState::capture_policy`.
    pub fn new(aciddog_binary: PathBuf, capture_policy: CapturePolicy) -> Arc<Self> {
        Arc::new(Self {
            aciddog_binary,
            capture_policy,
            registry: Registry::new(),
            activity: ActivityTracker::new(),
        })
    }

    /// Hand out a clone of the `Arc<ActivityTracker>` so the
    /// reconciler tick and the IPC client chokepoint can share the
    /// same map.
    pub fn activity_tracker(&self) -> Arc<ActivityTracker> {
        Arc::clone(&self.activity)
    }

    /// Spawn a fresh `aciddog daemon` child for `row`.
    ///
    /// On success the entry is inserted as `Starting` and a background
    /// task races a 100 ms socket-probe loop (5 s overall budget)
    /// against `child.wait()`:
    ///
    /// - probe wins → state := `Running`.
    /// - budget elapses → state := `Crashed`.
    /// - `child.wait()` wins → state := `Crashed`, OR `Stopped` iff
    ///   observed was already `Stopping` (concurrent `stop` is draining).
    pub async fn spawn(self: &Arc<Self>, row: ManagedDaemonRow) -> Result<(), DaemonRunnerError> {
        // Defensive: reject up-front if work_dir is missing or not a
        // directory. Fail loudly here rather than in the spawned child.
        if !row.work_dir.is_dir() {
            return Err(DaemonRunnerError::Spawn(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("work_dir does not exist: {}", row.work_dir.display()),
            )));
        }

        if self.registry.contains(row.id).await {
            return Err(DaemonRunnerError::AlreadyTracked(row.id));
        }

        let child = process::spawn_child(&self.aciddog_binary, &row.work_dir)
            .map_err(DaemonRunnerError::Spawn)?;
        let pid = child.id();

        // Wire stdio capture: hand the child (with piped stdout/stderr)
        // to the per-child fan-out task. start_capture takes the pipes
        // by-value via child.stdout.take()/stderr.take() and returns
        // the (now pipe-stripped) Child for the registry to track.
        let (logs, logs_shutdown, child) =
            log_capture::start_capture(&row.work_dir, self.capture_policy.clone(), child);

        let handle = ChildHandle {
            child: Some(child),
            pid,
            state: ObservedState::Starting,
            work_dir: row.work_dir.clone(),
            socket_path: row.socket_path.clone(),
            spawned_at: Instant::now(),
            logs: Some(logs),
            logs_shutdown: Some(logs_shutdown),
        };

        if let Err(_rejected) = self.registry.insert(row.id, handle).await {
            // `_rejected.child` will be dropped here, killing the
            // freshly-spawned process via `kill_on_drop(true)`.
            return Err(DaemonRunnerError::AlreadyTracked(row.id));
        }

        // ADR-0004 §Activity-tracking / edeb7d11: seed last_touched so
        // the start itself counts as activity. Outside the registry
        // guard's closure (the tracker uses its own lock).
        self.activity.seed(row.id);

        // Spawn the background watcher with a Weak back-pointer so it
        // does not pin the runner in memory.
        let weak: Weak<Self> = Arc::downgrade(self);
        let id = row.id;
        let socket_path = row.socket_path.clone();
        tokio::spawn(async move {
            spawn_watcher(weak, id, socket_path).await;
        });

        Ok(())
    }

    /// Stop a managed daemon. Sends SIGTERM, races `child.wait()`
    /// against `timeout`; on expiry escalates to SIGKILL with a 1 s
    /// grace. See plan §L.
    pub async fn stop(
        self: &Arc<Self>,
        id: Uuid,
        timeout: Duration,
    ) -> Result<(), DaemonRunnerError> {
        // Read state + pid + child availability without holding the
        // lock across awaits.
        let (state, child_some, pid) = match self
            .registry
            .with_handle_mut(id, |h| (h.state, h.child.is_some(), h.pid))
            .await
        {
            Some(t) => t,
            None => return Err(DaemonRunnerError::NotFound(id)),
        };

        if matches!(state, ObservedState::Stopped) {
            // Idempotent: already stopped. ADR-0004 §Activity-tracking
            // / edeb7d11: drop the entry so a future re-spawn under
            // the same Uuid restarts the clock from scratch.
            self.activity.forget(id);
            return Ok(());
        }
        if !child_some {
            return Err(DaemonRunnerError::AdoptedNotControlled(id));
        }
        let pid = match pid {
            Some(p) => p,
            None => return Err(DaemonRunnerError::AdoptedNotControlled(id)),
        };

        // Mark Stopping so the background watcher knows the next
        // child.wait() result is expected.
        self.registry
            .with_handle_mut(id, |h| h.state = ObservedState::Stopping)
            .await;

        // Take the child OUT of the registry slot for the duration of
        // the wait. The slot itself stays so the public API can still
        // observe Stopping. Re-attach on the StillAlive failure path.
        let mut child: Child = match self
            .registry
            .with_handle_mut(id, |h| h.child.take())
            .await
            .flatten()
        {
            Some(c) => c,
            // Concurrent stop drained it first. Treat as success.
            None => {
                self.registry.remove(id).await;
                // ADR-0004 §Activity-tracking / edeb7d11: forget on
                // success so a future re-spawn restarts the clock.
                self.activity.forget(id);
                return Ok(());
            }
        };

        // SIGTERM
        signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)?;

        // Race wait vs timeout.
        let waited = tokio::select! {
            res = child.wait() => Some(res),
            _ = tokio::time::sleep(timeout) => None,
        };

        if waited.is_some() {
            self.registry
                .with_handle_mut(id, |h| h.state = ObservedState::Stopped)
                .await;
            self.registry.remove(id).await;
            // ADR-0004 §Activity-tracking / edeb7d11: forget on
            // SIGTERM-success path.
            self.activity.forget(id);
            return Ok(());
        }

        // Escalate.
        signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL)?;
        let waited2 = tokio::select! {
            res = child.wait() => Some(res),
            _ = tokio::time::sleep(Duration::from_secs(1)) => None,
        };

        if waited2.is_some() {
            self.registry
                .with_handle_mut(id, |h| h.state = ObservedState::Stopped)
                .await;
            self.registry.remove(id).await;
            // ADR-0004 §Activity-tracking / edeb7d11: forget on
            // SIGKILL-success path.
            self.activity.forget(id);
            return Ok(());
        }

        // SIGKILL also failed. Mark Crashed, leave entry for forensics,
        // re-attach the child so a future caller can still observe it.
        // INTENTIONALLY do NOT forget the activity entry: the row is
        // forensically interesting and forgetting would let the
        // reconciler hammer it via repeated idle flips
        // (last_touched=None => sweep skips => no harm, but a future
        // policy that touches the entry on observe would reset the
        // timer instead). Keep state stable on the failure path.
        self.registry
            .with_handle_mut(id, |h| {
                h.state = ObservedState::Crashed;
                h.child = Some(child);
            })
            .await;
        Err(DaemonRunnerError::StillAlive(id))
    }

    /// Send SIGHUP to a Running managed daemon. No state change — the
    /// daemon's own SIGHUP handler runs `app.reload()`.
    pub async fn reload(self: &Arc<Self>, id: Uuid) -> Result<(), DaemonRunnerError> {
        let (state, child_some, pid) = match self
            .registry
            .with_handle_mut(id, |h| (h.state, h.child.is_some(), h.pid))
            .await
        {
            Some(t) => t,
            None => return Err(DaemonRunnerError::NotFound(id)),
        };

        if !matches!(state, ObservedState::Running) {
            return Err(DaemonRunnerError::NotRunning(id));
        }
        if !child_some {
            return Err(DaemonRunnerError::AdoptedNotControlled(id));
        }
        let pid = match pid {
            Some(p) => p,
            None => return Err(DaemonRunnerError::AdoptedNotControlled(id)),
        };

        signal::kill(Pid::from_raw(pid as i32), Signal::SIGHUP)?;
        Ok(())
    }

    /// Best-effort current observed state, with a `Child::try_wait()`
    /// nudge so a surprise exit since the last poll is observable
    /// without waiting for the background watcher.
    pub async fn status(self: &Arc<Self>, id: Uuid) -> Option<ObservedState> {
        // Try to detect a surprise exit by peeking try_wait() while
        // briefly holding the lock. If try_wait reports Some(_) the
        // child has exited; promote state to Crashed (or Stopped iff
        // already Stopping).
        let _ = self
            .registry
            .with_handle_mut(id, |h| {
                if let Some(child) = h.child.as_mut() {
                    if let Ok(Some(_status)) = child.try_wait() {
                        h.state = match h.state {
                            ObservedState::Stopping => ObservedState::Stopped,
                            _ => ObservedState::Crashed,
                        };
                    }
                }
            })
            .await;
        self.registry.state_of(id).await
    }

    /// Adopt a daemon that the manager did not spawn. v1 is
    /// observe-only — the resulting entry has `child = None`, so
    /// `stop`/`reload` will return `AdoptedNotControlled`. See §H/§J.
    pub async fn adopt(
        self: &Arc<Self>,
        id: Uuid,
        socket_path: PathBuf,
    ) -> Result<AdoptOutcome, DaemonRunnerError> {
        if self.registry.contains(id).await {
            return Err(DaemonRunnerError::AlreadyTracked(id));
        }

        if process::probe_socket_once(&socket_path).await.is_err() {
            return Ok(AdoptOutcome::NotResponding);
        }

        // Derive work_dir from the socket path's parent — the daemon's
        // CWD is conventionally the directory holding the socket. v1
        // tolerates a missing parent by falling back to "." so that
        // adopt remains best-effort.
        let work_dir = socket_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let handle = ChildHandle {
            child: None,
            pid: None,
            state: ObservedState::Running,
            work_dir,
            socket_path,
            spawned_at: Instant::now(),
            // Adopted entries have no manager-side pipes; logs are
            // unavailable and `is_managed` returns false (the SSE route
            // 409s on adopted ids).
            logs: None,
            logs_shutdown: None,
        };
        if let Err(_rejected) = self.registry.insert(id, handle).await {
            return Err(DaemonRunnerError::AlreadyTracked(id));
        }
        // ADR-0004 §Activity-tracking / edeb7d11: seed last_touched
        // so adoption itself counts as activity. Outside the registry
        // guard's closure (the tracker uses its own lock).
        self.activity.seed(id);
        Ok(AdoptOutcome::Adopted)
    }

    /// Returns the per-child stdio capture handle for `id` if the
    /// runner spawned the process (i.e. it has manager-side pipes).
    /// Returns `None` for unknown ids AND for adopted entries
    /// (`child = None`, no pipe to read).
    pub async fn logs_handle(self: &Arc<Self>, id: Uuid) -> Option<RingBufferHandle> {
        self.registry
            .with_handle_mut(id, |h| {
                if h.child.is_some() {
                    h.logs.clone()
                } else {
                    None
                }
            })
            .await
            .flatten()
    }

    /// True iff the registry holds an entry for `id` AND the entry was
    /// spawned by this manager (has a `Child` handle). Used by the
    /// SSE logs route to disambiguate the `404 not-found` vs
    /// `409 adopted` outcome.
    pub async fn is_managed(self: &Arc<Self>, id: Uuid) -> bool {
        self.registry
            .with_handle_mut(id, |h| h.child.is_some())
            .await
            .unwrap_or(false)
    }
}

/// Background task spawned per-`spawn`. Runs until either the probe
/// succeeds, the budget elapses, or `child.wait()` returns. Holds a
/// `Weak<DaemonRunner>` so the runner can be dropped without leaking
/// this task forever — if the upgrade fails, exit silently.
async fn spawn_watcher(weak: Weak<DaemonRunner>, id: Uuid, socket_path: PathBuf) {
    // Race the probe loop against child.wait().
    //
    // We can't borrow the Child out of the registry for an indefinite
    // wait without holding the lock, so this loop polls instead: every
    // `PROBE_INTERVAL`, try one connect; in between, peek `try_wait()`
    // briefly under the lock to detect surprise exits.
    let deadline = tokio::time::Instant::now() + process::PROBE_BUDGET;
    loop {
        let runner = match weak.upgrade() {
            Some(r) => r,
            None => return,
        };

        // Single connect attempt.
        let connected = tokio::net::UnixStream::connect(&socket_path).await.is_ok();
        if connected {
            runner
                .registry
                .with_handle_mut(id, |h| {
                    // Don't clobber a concurrent Stopping/Stopped.
                    if matches!(h.state, ObservedState::Starting) {
                        h.state = ObservedState::Running;
                    }
                })
                .await;
            return;
        }

        // Peek try_wait under the lock. If the child has already
        // exited, mark Crashed (or Stopped iff we observed Stopping).
        let exited = runner
            .registry
            .with_handle_mut(id, |h| {
                if let Some(child) = h.child.as_mut() {
                    matches!(child.try_wait(), Ok(Some(_)))
                } else {
                    // Adopted or already taken — no child to wait on.
                    false
                }
            })
            .await
            .unwrap_or(false);

        if exited {
            runner
                .registry
                .with_handle_mut(id, |h| {
                    h.state = match h.state {
                        ObservedState::Stopping => ObservedState::Stopped,
                        _ => ObservedState::Crashed,
                    };
                })
                .await;
            return;
        }

        // Out of budget?
        let now = tokio::time::Instant::now();
        if now >= deadline {
            runner
                .registry
                .with_handle_mut(id, |h| {
                    if matches!(h.state, ObservedState::Starting) {
                        h.state = ObservedState::Crashed;
                    }
                })
                .await;
            return;
        }

        // Drop the Arc so the runner can be torn down between ticks.
        drop(runner);
        let remaining = deadline - now;
        let sleep = if remaining < process::PROBE_INTERVAL {
            remaining
        } else {
            process::PROBE_INTERVAL
        };
        tokio::time::sleep(sleep).await;
    }
}

/// Resolve the path to the `aciddog` binary in three tiers:
///
/// 1. `explicit` (CLI / env, set by the caller).
/// 2. Sibling-of-current-exe — i.e. an `aciddog` next to the running
///    `aciddog-manager` binary.
/// 3. PATH walk via `std::env::var_os("PATH")` and `env::split_paths`.
///
/// First match wins. Returns `BinaryNotFound` when nothing exists.
pub fn resolve_aciddog_binary(explicit: Option<PathBuf>) -> Result<PathBuf, DaemonRunnerError> {
    if let Some(path) = explicit {
        if is_executable_file(&path) {
            return Ok(path);
        }
        // Explicit path was specified but doesn't resolve — fall
        // through and try the other tiers rather than silently
        // accepting a bogus path.
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("aciddog");
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
        }
    }

    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join("aciddog");
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
        }
    }

    Err(DaemonRunnerError::BinaryNotFound)
}

/// True iff `path` exists, is a regular file, and (on Unix) is
/// executable by the current user. Pure-std implementation — we
/// deliberately avoid pulling in the `which` crate.
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => {
            if !meta.is_file() {
                return false;
            }
            let mode = meta.permissions().mode();
            // Any of user/group/other exec bits.
            mode & 0o111 != 0
        }
        Err(_) => false,
    }
}
