//! Smoke tests for the `daemon_runner` subsystem.
//!
//! Two tiers:
//!   1. Pure-fn truth-table coverage of the `next_action` lifecycle map
//!      (one `#[test]` per row in §G of the implementation plan).
//!   2. Runtime behaviour of `DaemonRunner::{spawn, stop, reload, adopt,
//!      status}` exercised against the `daemon_runner_shim` test binary.
//!
//! The runner hard-codes `arg("daemon")` when spawning, so the shim
//! cannot be invoked directly with `--socket <path>` — instead each
//! runtime test writes a small `sh` wrapper into a tempdir that
//! `exec`s the shim with the right flags. The wrapper ignores any
//! trailing args (so the runner's `daemon` arg is harmless).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::tempdir;
use tokio::net::UnixListener;
use uuid::Uuid;

use aciddog_manager::daemon_runner::{
    next_action, AdoptOutcome, DaemonRunner, DaemonRunnerError, DesiredState, ManagedDaemonRow,
    ObservedState, ReconcilerAction,
};

// --- §A. Pure-fn lifecycle truth-table coverage --------------------------
//
// Each row in the truth table from §G of the plan gets its own
// `#[test]` so a regression in any single transition shows up as a
// named failure.

#[test]
fn truth_table_running_stopped_spawns() {
    assert_eq!(
        next_action(DesiredState::Running, ObservedState::Stopped),
        ReconcilerAction::Spawn
    );
}

#[test]
fn truth_table_running_crashed_alerts() {
    assert_eq!(
        next_action(DesiredState::Running, ObservedState::Crashed),
        ReconcilerAction::AlertCrashed
    );
}

#[test]
fn truth_table_running_running_noop() {
    assert_eq!(
        next_action(DesiredState::Running, ObservedState::Running),
        ReconcilerAction::NoOp
    );
}

#[test]
fn truth_table_running_starting_noop() {
    assert_eq!(
        next_action(DesiredState::Running, ObservedState::Starting),
        ReconcilerAction::NoOp
    );
}

#[test]
fn truth_table_running_stopping_noop() {
    assert_eq!(
        next_action(DesiredState::Running, ObservedState::Stopping),
        ReconcilerAction::NoOp
    );
}

#[test]
fn truth_table_stopped_running_stops() {
    assert_eq!(
        next_action(DesiredState::Stopped, ObservedState::Running),
        ReconcilerAction::Stop
    );
}

#[test]
fn truth_table_stopped_starting_stops() {
    assert_eq!(
        next_action(DesiredState::Stopped, ObservedState::Starting),
        ReconcilerAction::Stop
    );
}

#[test]
fn truth_table_stopped_stopping_noop() {
    assert_eq!(
        next_action(DesiredState::Stopped, ObservedState::Stopping),
        ReconcilerAction::NoOp
    );
}

#[test]
fn truth_table_stopped_stopped_noop() {
    assert_eq!(
        next_action(DesiredState::Stopped, ObservedState::Stopped),
        ReconcilerAction::NoOp
    );
}

#[test]
fn truth_table_stopped_crashed_noop() {
    assert_eq!(
        next_action(DesiredState::Stopped, ObservedState::Crashed),
        ReconcilerAction::NoOp
    );
}

// --- helpers for the runtime tests ---------------------------------------

/// Path to the `daemon_runner_shim` test binary, populated by Cargo at
/// build time.
fn shim_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daemon_runner_shim"))
}

/// Write a `sh` wrapper script at `wrapper_path` that exec's `shim`
/// with the given trailing args, then chmod it `0o755`.
fn write_wrapper(wrapper_path: &Path, shim: &Path, extra_args: &[&str]) {
    let mut body = String::from("#!/bin/sh\n");
    body.push_str("# trailing args from caller (e.g. \"daemon\") are ignored\n");
    body.push_str(&format!("exec {:?}", shim));
    for arg in extra_args {
        body.push(' ');
        body.push_str(&format!("{:?}", arg));
    }
    body.push('\n');
    fs::write(wrapper_path, body).expect("write wrapper script");
    let mut perms = fs::metadata(wrapper_path)
        .expect("stat wrapper")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(wrapper_path, perms).expect("chmod wrapper");
}

/// Poll `runner.status(id)` every 100 ms (up to 5 s) until the
/// returned state equals `target`. Returns the final observed state
/// (`None` if the entry vanished before timeout).
async fn wait_for_state(
    runner: &std::sync::Arc<DaemonRunner>,
    id: Uuid,
    target: ObservedState,
) -> Option<ObservedState> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let state = runner.status(id).await;
        if state == Some(target) {
            return state;
        }
        if std::time::Instant::now() >= deadline {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// --- §B. spawn → status::Running → stop → registry cleared ---------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_then_stop_drives_running_then_clears_registry() {
    // The runner shells out to `<binary> daemon`; the shim doesn't
    // know that flag, so the wrapper script translates the spawn into
    // `<shim> --socket <path>` and ignores trailing args.
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("daemon.sock");
    let wrapper = work_dir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &["--socket", socket_path.to_str().unwrap()],
    );

    let runner = DaemonRunner::new(wrapper);
    let id = Uuid::new_v4();
    let row = ManagedDaemonRow {
        id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };

    runner.spawn(row).await.expect("spawn ok");

    let state = wait_for_state(&runner, id, ObservedState::Running).await;
    assert_eq!(
        state,
        Some(ObservedState::Running),
        "expected Running, got {:?}",
        state
    );

    runner
        .stop(id, Duration::from_secs(2))
        .await
        .expect("stop ok");

    assert_eq!(
        runner.status(id).await,
        None,
        "registry should be cleared after stop"
    );
}

// --- §C. spawn → reload is a no-op on observed state ---------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_then_reload_is_noop_on_state() {
    // Same wrapper-script setup as §B; reload only delivers SIGHUP, so
    // the shim ignores it (no SIGHUP handler installed) and the
    // observed state should remain Running.
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("daemon.sock");
    let wrapper = work_dir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &["--socket", socket_path.to_str().unwrap()],
    );

    let runner = DaemonRunner::new(wrapper);
    let id = Uuid::new_v4();
    let row = ManagedDaemonRow {
        id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };

    runner.spawn(row).await.expect("spawn ok");
    let state = wait_for_state(&runner, id, ObservedState::Running).await;
    assert_eq!(state, Some(ObservedState::Running), "want Running");

    runner.reload(id).await.expect("reload ok");

    assert_eq!(
        runner.status(id).await,
        Some(ObservedState::Running),
        "reload must not flip observed state"
    );

    // Clean up so the test does not leak a child process.
    runner
        .stop(id, Duration::from_secs(2))
        .await
        .expect("stop ok");
}

// --- §D. adopt then stop/reload returns AdoptedNotControlled -------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adopt_then_stop_or_reload_returns_adopted_not_controlled() {
    // Pre-bind a UnixListener at `socket_path` and spawn an accept
    // loop so the adopt-time probe connects successfully. No shim is
    // spawned — the runner has no Child to control.
    let dir = tempdir().expect("tempdir");
    let socket_path = dir.path().join("adopted.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    // Hold accepted streams alive for the test lifetime.
    let _accept_task = tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            match listener.accept().await {
                Ok((stream, _)) => held.push(stream),
                Err(_) => return,
            }
        }
    });

    let runner = DaemonRunner::new(shim_path());
    let id = Uuid::new_v4();

    let outcome = runner
        .adopt(id, socket_path.clone())
        .await
        .expect("adopt ok");
    assert_eq!(outcome, AdoptOutcome::Adopted);

    assert_eq!(runner.status(id).await, Some(ObservedState::Running));

    let stop_err = runner
        .stop(id, Duration::from_secs(1))
        .await
        .expect_err("stop must reject adopted entry");
    assert!(
        matches!(stop_err, DaemonRunnerError::AdoptedNotControlled(got) if got == id),
        "want AdoptedNotControlled, got {:?}",
        stop_err
    );

    let reload_err = runner
        .reload(id)
        .await
        .expect_err("reload must reject adopted entry");
    assert!(
        matches!(reload_err, DaemonRunnerError::AdoptedNotControlled(got) if got == id),
        "want AdoptedNotControlled, got {:?}",
        reload_err
    );
}

// --- §E. adopt returns NotResponding when the socket is dead -------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adopt_returns_not_responding_when_socket_dead() {
    let dir = tempdir().expect("tempdir");
    let dead_path = dir.path().join("never-bound.sock");

    let runner = DaemonRunner::new(shim_path());
    let id = Uuid::new_v4();

    let outcome = runner.adopt(id, dead_path).await.expect("adopt ok");
    assert_eq!(outcome, AdoptOutcome::NotResponding);

    assert_eq!(
        runner.status(id).await,
        None,
        "registry must be empty when adopt fails to connect"
    );
}

// --- §F. crashing child transitions to Crashed ---------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_transitions_to_crashed() {
    // Wrapper script invokes the shim with `--exit-immediately`, so
    // the spawned child exits non-zero before binding the socket.
    // The watcher's try_wait peek should catch the early exit and
    // promote Starting → Crashed.
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("daemon.sock");
    let wrapper = work_dir.path().join("shim-fail.sh");
    write_wrapper(&wrapper, &shim_path(), &["--exit-immediately"]);

    let runner = DaemonRunner::new(wrapper);
    let id = Uuid::new_v4();
    let row = ManagedDaemonRow {
        id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };

    runner.spawn(row).await.expect("spawn ok");

    let state = wait_for_state(&runner, id, ObservedState::Crashed).await;
    assert_eq!(
        state,
        Some(ObservedState::Crashed),
        "expected Crashed after immediate exit, got {:?}",
        state
    );
}
