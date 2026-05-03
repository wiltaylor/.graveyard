//! Integration smoke tests for `crate::reconciler`. Drives the real
//! `Arc<DaemonRunner>` against the existing `daemon_runner_shim`
//! binary (extended with `--serve-health` so `Client::health` returns
//! 200).
//!
//! Mirrors the layout of `daemon_runner_smoke.rs` — wrapper script
//! pattern, `tempfile::tempdir` per test, `wait_for_state` polling
//! helper.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::daemon_runner::{DaemonRunner, ObservedState};
use aciddog_manager::db;
use aciddog_manager::model::managed_daemon::DesiredState;
use aciddog_manager::reconciler;
use aciddog_manager::state::AppState;

use tempfile::tempdir;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Path to the `daemon_runner_shim` test binary.
fn shim_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daemon_runner_shim"))
}

/// Write a `sh` wrapper at `wrapper_path` that exec's `shim` with
/// `extra_args`, then chmod 0o755. Mirrors `daemon_runner_smoke.rs`.
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

/// Build an in-memory AppState whose daemon_runner spawns through
/// `wrapper`. The wrapper script is the path the runner uses as
/// "the aciddog binary"; it actually exec's the shim.
async fn build_state_with_runner(wrapper: PathBuf) -> (AppState, Arc<DaemonRunner>) {
    let pool = db::connect_memory().await.expect("memory db");
    let runner = DaemonRunner::new(wrapper);
    let mut state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: false,
        },
        std::env::temp_dir(),
        true,
    );
    state.daemon_runner = Some(Arc::clone(&runner));
    (state, runner)
}

/// Insert a managed_daemons row + paired instance row directly,
/// skipping `insert_with_paired_instance` validation (which insists
/// the work_dir already exists on disk; tempdir handles that for us
/// anyway, but we want to control the field shape exactly).
async fn insert_raw_row(
    state: &AppState,
    id: Uuid,
    socket_path: &Path,
    work_dir: &Path,
    desired: DesiredState,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let name = format!("smoke-{id}");
    let instance_id = Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await.expect("tx");
    sqlx::query(
        "INSERT INTO managed_daemons (id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, 0, ?6, ?6)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(work_dir.to_string_lossy().into_owned())
    .bind(socket_path.to_string_lossy().into_owned())
    .bind(desired.as_str())
    .bind(&now)
    .execute(&mut *tx)
    .await
    .expect("insert managed_daemon");
    sqlx::query(
        "INSERT INTO instances (id, name, transport, url, socket_path, api_key, tls_ca_path, managed_by_id, created_at)
         VALUES (?1, ?2, 'unix', NULL, ?3, NULL, NULL, ?4, ?5)",
    )
    .bind(&instance_id)
    .bind(&name)
    .bind(socket_path.to_string_lossy().into_owned())
    .bind(id.to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await
    .expect("insert instance");
    tx.commit().await.expect("commit");
}

/// Poll `runner.status(id)` every 100 ms (up to 5 s) until the
/// returned state equals `target`.
async fn wait_for_state(
    runner: &Arc<DaemonRunner>,
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

// --- (i) end-to-end_adopt: real shim, healthy socket, adopted ---

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_to_end_adopt() {
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("adopt.sock");

    // Pre-launch the shim manually (bypass DaemonRunner::spawn) so the
    // socket has a live HTTP-serving process bound and the runner has
    // no Child handle for it (true adoption scenario).
    let mut shim_proc = tokio::process::Command::new(shim_path())
        .arg("--socket")
        .arg(&socket_path)
        .arg("--serve-health")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn shim");

    // Wait for the shim to bind the socket — up to 2s.
    let bind_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !socket_path.exists() {
        if std::time::Instant::now() >= bind_deadline {
            panic!("shim never bound socket at {}", socket_path.display());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Give it a tick to be ready to accept.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // The runner's `aciddog_binary` is irrelevant for this test (we
    // never call spawn) — point it at the shim itself.
    let (state, runner) = build_state_with_runner(shim_path()).await;

    let id = Uuid::new_v4();
    insert_raw_row(
        &state,
        id,
        &socket_path,
        work_dir.path(),
        DesiredState::Running,
    )
    .await;

    // Drive boot adoption. After this, runner.status(id) should be
    // Some(Running) and the runner registry should reflect adopted
    // (i.e. no fresh Child was spawned).
    reconciler::run_for_test_boot_only(&state)
        .await
        .expect("boot ok");

    let observed = runner.status(id).await;
    assert_eq!(
        observed,
        Some(ObservedState::Running),
        "expected adopted=Running, got {observed:?}"
    );

    // Verify no fresh child was spawned: `runner.stop` on an adopted
    // entry must return AdoptedNotControlled.
    let err = runner
        .stop(id, Duration::from_secs(1))
        .await
        .expect_err("stop must reject adopted entry");
    assert!(
        matches!(
            err,
            aciddog_manager::daemon_runner::DaemonRunnerError::AdoptedNotControlled(got) if got == id
        ),
        "want AdoptedNotControlled, got {err:?}"
    );

    // Cleanup: SIGTERM the shim manually since the runner doesn't own
    // the child.
    let _ = shim_proc.start_kill();
    let _ = shim_proc.wait().await;
}

// --- (ii) end-to-end_spawn_then_detect_drift ---

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_to_end_spawn_then_detect_drift() {
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("drift.sock");

    // Wrapper exec's the shim with --socket <path> --serve-health,
    // ignoring the trailing "daemon" arg the runner injects.
    let wrapper = work_dir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &["--socket", socket_path.to_str().unwrap(), "--serve-health"],
    );

    let (state, runner) = build_state_with_runner(wrapper).await;

    let id = Uuid::new_v4();
    insert_raw_row(
        &state,
        id,
        &socket_path,
        work_dir.path(),
        DesiredState::Running,
    )
    .await;

    // Boot pass: socket is unbound, so health probe fails ⇒ runner
    // is asked to spawn a fresh child.
    reconciler::run_for_test_boot_only(&state)
        .await
        .expect("boot ok");

    // The spawn watcher races a 5s probe-budget; once the socket is
    // up and the watcher connects, observed flips to Running.
    let observed = wait_for_state(&runner, id, ObservedState::Running).await;
    assert_eq!(
        observed,
        Some(ObservedState::Running),
        "expected spawn->Running, got {observed:?}"
    );

    // Sanity: a tick_once after the boot pass should be a NoOp
    // (desired=Running, observed=Running).
    let backoff_state = Arc::new(Mutex::new(HashMap::new()));
    reconciler::tick_once(&state, &runner, &backoff_state)
        .await
        .expect("tick ok");
    assert_eq!(runner.status(id).await, Some(ObservedState::Running));

    // Cleanup the runner-owned child.
    runner
        .stop(id, Duration::from_secs(2))
        .await
        .expect("stop ok");
}
