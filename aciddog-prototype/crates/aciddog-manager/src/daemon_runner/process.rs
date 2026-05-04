//! Stdio posture — both stdout AND stderr are `Stdio::piped()` so
//! sibling 3bc8a0d0's `log_capture::start_capture` can fan-out to the
//! ring buffer + rotating on-disk file. The v1 `Stdio::null()` stub-out
//! has been removed.
//!
//! The daemon still writes its own `./aciddog.log` from inside the
//! process (`crates/aciddog/src/main.rs:222`); that log lands in CWD,
//! which is `row.work_dir`. The pipes captured here are PURELY the
//! manager-side mirror of whatever the child wrote to its inherited
//! stdio (e.g. panic backtraces, early-boot `eprintln!`s before the
//! daemon's logger is up).
//!
//! `kill_on_drop(true)` keeps a leaked `Child` from orphaning the OS
//! process. The pipes returned via the `Child` move with the `Child`
//! into `DaemonRunner::spawn`, which immediately hands the child to
//! `log_capture::start_capture` (that function `take()`s stdout +
//! stderr, returning the pipe-stripped `Child` so the registry's
//! lifecycle code keeps working without seeing the pipes).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::net::UnixStream;
use tokio::process::{Child, Command};

/// Observed lifecycle state of a managed daemon child process.
///
/// Lives here (rather than on the wire layer in sibling `b91e9691`) so
/// that the canonical wire shape is fixed by the runner that owns the
/// transitions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Crashed,
}

/// Spawn `<binary> daemon` with `work_dir` as CWD. Both stdio streams
/// are piped so [`crate::daemon_runner::log_capture::start_capture`]
/// can mirror them into the ring buffer + rotating writer.
/// `kill_on_drop(true)` ensures a leaked `Child` cannot orphan an OS
/// process.
pub(crate) fn spawn_child(binary: &Path, work_dir: &Path) -> std::io::Result<Child> {
    Command::new(binary)
        .arg("daemon")
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

/// Probe budget: every 100 ms try to connect, give up after 5 s.
pub(crate) const PROBE_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const PROBE_BUDGET: Duration = Duration::from_secs(5);

/// Adopt-time probe budget: a single attempt with a 1 s timeout (per
/// §J of the implementation plan).
pub(crate) const ADOPT_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Poll `path` every 100 ms until a `UnixStream::connect` succeeds, or
/// until the 5 s overall budget elapses.
///
/// Returns `Ok(())` on first successful connect, `Err(())` on budget
/// exhaustion. The caller decides whether budget exhaustion means
/// `Crashed` (spawn pipeline) or `NotResponding` (adopt pipeline).
///
/// This standalone helper is consumed by the smoke-test suite (sibling
/// TODO 11) — the in-runner probe loop is inlined into
/// `spawn_watcher` so it can race the child-wait peek under the same
/// budget.
#[allow(dead_code)]
pub(crate) async fn probe_socket(path: &Path) -> Result<(), ()> {
    let deadline = tokio::time::Instant::now() + PROBE_BUDGET;
    loop {
        if UnixStream::connect(path).await.is_ok() {
            return Ok(());
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(());
        }
        let remaining = deadline - now;
        let sleep = if remaining < PROBE_INTERVAL {
            remaining
        } else {
            PROBE_INTERVAL
        };
        tokio::time::sleep(sleep).await;
    }
}

/// One-shot connect probe with a fixed timeout — used by `adopt`.
///
/// Returns `Ok(())` if the connect lands inside `ADOPT_PROBE_TIMEOUT`,
/// `Err(())` on either timeout or connect failure.
pub(crate) async fn probe_socket_once(path: &Path) -> Result<(), ()> {
    match tokio::time::timeout(ADOPT_PROBE_TIMEOUT, UnixStream::connect(path)).await {
        Ok(Ok(_stream)) => Ok(()),
        _ => Err(()),
    }
}
