//! ADR-0004 §Activity-tracking — integration coverage for the
//! reconciler tick's idle-timeout sweep (sibling task `edeb7d11`,
//! Phase 2).
//!
//! Three cases:
//!
//! 1. `idle_timeout_flips_desired_after_window` — managed row with
//!    `idle_timeout_secs = Some(5)`, observed=Running, no traffic;
//!    after `advance(6s)` and a single `tick_once`, the row's
//!    `desired_state` is flipped to `'stopped'` and the tracker
//!    entry has been forgotten.
//! 2. `idle_timeout_keeps_running_inside_window` — same fixture but
//!    `advance(4s)`; one tick leaves `desired_state = 'running'` and
//!    the tracker entry intact.
//! 3. `traffic_resets_idle_clock` — same fixture; at virtual t=4s
//!    issue a `tracker.touch(id)` (simulating a forwarded RPC),
//!    advance to t=8s; one tick leaves `desired_state = 'running'`
//!    because the timer was reset by the touch (now-last_touched =
//!    4s, still inside the 5s window).
//!
//! Time control: SQLite's connection pool uses real-time-driven
//! timeouts internally, which deadlock under `start_paused = true`
//! (the in-memory connect call hangs waiting on a paused timer
//! wheel). The pattern here is therefore the pause-advance-resume
//! sandwich:
//!
//!   1. Run setup (`db::connect_memory`, `runner.adopt`, etc.)
//!      under real time.
//!   2. `tokio::time::pause()` to freeze the clock at the seed
//!      timestamp.
//!   3. `tokio::time::advance(Duration)` to push virtual time
//!      forward (so `Instant::now()` in `apply_idle_timeout`
//!      observes a stale `last_touched`).
//!   4. `tokio::time::resume()` so the subsequent SQL-driven
//!      reconciler tick sees a working real-time pool.
//!   5. `tick_once(...).await` — `Instant::now()` continues from
//!      the advanced point; the elapsed window is still observed
//!      as `> idle`.
//!   6. Assert `desired_state` via raw sqlx.
//!
//! Real wall-clock waits are NEVER used between steps 3 and 5.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::daemon_runner::{CapturePolicy, DaemonRunner, ObservedState};
use aciddog_manager::db;
use aciddog_manager::reconciler;
use aciddog_manager::state::AppState;
use sqlx::SqlitePool;
use tempfile::tempdir;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Build an in-memory `AppState` whose `daemon_runner` is set to a
/// fresh `DaemonRunner`. The runner's `aciddog_binary` is irrelevant
/// for these tests (we never call `spawn`); we point it at
/// `/bin/true` (or `/usr/bin/true` — `DaemonRunner::new` accepts a
/// PathBuf without validating).
async fn build_state() -> (AppState, Arc<DaemonRunner>) {
    let pool = db::connect_memory().await.expect("memory db");
    // /bin/true is universally present on Linux test runners.
    let runner = DaemonRunner::new(
        std::path::PathBuf::from("/bin/true"),
        CapturePolicy::default(),
    );
    let aciddog_binary = Arc::new(std::path::PathBuf::from("/bin/true"));
    let state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: false,
        },
        std::env::temp_dir(),
        aciddog_binary,
        Arc::clone(&runner),
        CapturePolicy::default(),
    );
    (state, runner)
}

/// Insert a managed_daemons row with the given `idle_timeout_secs`
/// and a paired `instances` row carrying `managed_by_id =
/// <that uuid>`. Returns the daemon `Uuid`.
async fn insert_row_with_idle(
    pool: &SqlitePool,
    socket_path: &Path,
    work_dir: &Path,
    idle_timeout_secs: Option<i64>,
) -> Uuid {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let name = format!("idle-{id}");
    let instance_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await.expect("tx");
    sqlx::query(
        "INSERT INTO managed_daemons (id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'running', 0, ?5, 0, ?6, ?6)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(work_dir.to_string_lossy().into_owned())
    .bind(socket_path.to_string_lossy().into_owned())
    .bind(idle_timeout_secs)
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
    id
}

/// Spawn a `UnixListener` at `path` that accepts connections and
/// drops them immediately — sufficient for `probe_socket_once` (it
/// only checks `connect()`-ability).
async fn spawn_drop_socket(path: std::path::PathBuf) {
    let listener = UnixListener::bind(&path).expect("bind unix listener");
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((_stream, _)) => {
                    // Drop immediately.
                }
                Err(_) => return,
            }
        }
    });
    // Yield once so the accept loop is ready before adopt's probe.
    tokio::task::yield_now().await;
}

/// Read the row's `desired_state` column directly via raw sqlx so the
/// test doesn't depend on the model layer's `parse` helpers.
async fn fetch_desired_state(pool: &SqlitePool, id: Uuid) -> String {
    let row: (String,) = sqlx::query_as("SELECT desired_state FROM managed_daemons WHERE id = ?1")
        .bind(id.to_string())
        .fetch_one(pool)
        .await
        .expect("row");
    row.0
}

#[tokio::test(flavor = "current_thread")]
async fn idle_timeout_flips_desired_after_window() {
    let (state, runner) = build_state().await;
    let dir = tempdir().expect("tempdir");
    let socket = dir.path().join("idle.sock");
    spawn_drop_socket(socket.clone()).await;

    let id = insert_row_with_idle(&state.pool, &socket, dir.path(), Some(5)).await;
    runner.adopt(id, socket.clone()).await.expect("adopt ok");
    assert_eq!(runner.status(id).await, Some(ObservedState::Running));
    assert_eq!(fetch_desired_state(&state.pool, id).await, "running");
    assert!(
        runner.activity_tracker().last_touched(id).is_some(),
        "adopt must seed last_touched"
    );

    // Pause AFTER setup, advance virtual time past the timeout, then
    // resume so SQL pool acquires (which use real-time-driven
    // timeouts internally) work cleanly during the tick. The
    // tokio::time::Instant captured during pause/advance survives
    // resume — `Instant::now()` continues from the advanced point.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(6)).await;
    tokio::time::resume();

    // One reconciler tick.
    let backoff_state = Arc::new(Mutex::new(HashMap::new()));
    reconciler::tick_once(&state, &runner, &backoff_state)
        .await
        .expect("tick ok");

    assert_eq!(
        fetch_desired_state(&state.pool, id).await,
        "stopped",
        "idle window elapsed => desired_state must flip to 'stopped'"
    );
    assert!(
        runner.activity_tracker().last_touched(id).is_none(),
        "tracker entry must be forgotten after the idle flip so the \
         reconciler does not hammer the row on subsequent ticks"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn idle_timeout_keeps_running_inside_window() {
    let (state, runner) = build_state().await;
    let dir = tempdir().expect("tempdir");
    let socket = dir.path().join("inside.sock");
    spawn_drop_socket(socket.clone()).await;

    let id = insert_row_with_idle(&state.pool, &socket, dir.path(), Some(5)).await;
    runner.adopt(id, socket.clone()).await.expect("adopt ok");
    assert_eq!(runner.status(id).await, Some(ObservedState::Running));

    // Pause AFTER setup (see module-level rationale), advance, then
    // resume before issuing SQL-driven work.
    tokio::time::pause();
    // Inside the 5s window.
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::time::resume();

    let backoff_state = Arc::new(Mutex::new(HashMap::new()));
    reconciler::tick_once(&state, &runner, &backoff_state)
        .await
        .expect("tick ok");

    assert_eq!(
        fetch_desired_state(&state.pool, id).await,
        "running",
        "still inside the idle window => desired_state must stay 'running'"
    );
    assert!(
        runner.activity_tracker().last_touched(id).is_some(),
        "in-window tick must NOT forget the tracker entry"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn traffic_resets_idle_clock() {
    let (state, runner) = build_state().await;
    let dir = tempdir().expect("tempdir");
    let socket = dir.path().join("traffic.sock");
    spawn_drop_socket(socket.clone()).await;

    let id = insert_row_with_idle(&state.pool, &socket, dir.path(), Some(5)).await;
    runner.adopt(id, socket.clone()).await.expect("adopt ok");
    assert_eq!(runner.status(id).await, Some(ObservedState::Running));

    // Pause AFTER setup (see module-level rationale).
    tokio::time::pause();

    // At t=4s simulate a forwarded RPC by touching the tracker
    // directly (the chokepoint's behaviour is covered by
    // tests/client_activity_chokepoint.rs; here we just need the
    // timestamp reset).
    tokio::time::advance(Duration::from_secs(4)).await;
    runner.activity_tracker().touch(id);

    // Advance another 4s — total wall-time = 8s, but only 4s since
    // the last touch (still inside the 5s window). Resume before the
    // SQL-driven tick.
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::time::resume();

    let backoff_state = Arc::new(Mutex::new(HashMap::new()));
    reconciler::tick_once(&state, &runner, &backoff_state)
        .await
        .expect("tick ok");

    assert_eq!(
        fetch_desired_state(&state.pool, id).await,
        "running",
        "mid-window touch must reset the idle clock; desired_state \
         must stay 'running'"
    );
    assert!(
        runner.activity_tracker().last_touched(id).is_some(),
        "tracker entry must be intact after a touch-reset tick"
    );
}
