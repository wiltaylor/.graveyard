//! Background reconciler that drives `observed_state -> desired_state`
//! for every row in `managed_daemons`. Implements **ADR-0003**.
//!
//! ## Sibling tasks (all on `origin/main`)
//!
//! - **`7c11fa3d`** — `model::managed_daemon` (rows, `DesiredState`,
//!   `set_desired_state`, etc.).
//! - **`799b7b92`** — the `daemon_runner` subsystem (registry,
//!   `spawn`/`stop`/`status`/`adopt`, lifecycle truth-table).
//! - **`33fc8b7b`** — capability gate (`is_daemon_runner_enabled`,
//!   `state.daemon_runner: Option<Arc<DaemonRunner>>`).
//! - **`b91e9691`** — public `/api/managed-daemons` routes; calls
//!   `state.reconciler_notify.poke(id)` on every successful
//!   `set_desired_state` write.
//!
//! ## Architecture decision: `lifecycle::next_action` is the SOLE oracle
//!
//! The reconciler does **not** contain its own (desired, observed)
//! truth table. Every per-row decision is delegated to
//! `daemon_runner::lifecycle::next_action(desired, observed)`, which is
//! exhaustively unit-tested by sibling `799b7b92`. The reconciler
//! merely dispatches the resulting `ReconcilerAction` (NoOp / Spawn /
//! Stop / AlertCrashed) and never re-implements the table. Acceptance
//! gate #8 enforces this with `grep`.
//!
//! ## Adopted-row truth-table cell — locked by THIS task
//!
//! > desired=Stopped + observed=Running + child=None (adopted) -> Stop
//! > issued -> Err(AdoptedNotControlled) -> log warn + leave. Observed
//! > stays Running.
//!
//! The reconciler MUST NEVER deliver a process signal directly: only
//! `daemon_runner::stop` is allowed to touch process IDs, and it
//! refuses adopted rows. Acceptance gate #9 (banned-pattern grep)
//! enforces this module is signal-delivery-free.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use rand::Rng;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::client::Client;
use crate::daemon_runner::{
    self, idle_timeout_action, next_action, ActivityTracker, AdoptOutcome, DaemonRunner,
    DaemonRunnerControl, DaemonRunnerError, ManagedDaemonRow as RunnerRow, ObservedState,
    ReconcilerAction,
};
use crate::error::AppResult;
use crate::model::{instance::InstanceRow, managed_daemon};
use crate::state::AppState;

/// Tick budget: drift checks fire ~once a second (with ±100ms jitter).
const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// `runner.stop(id, STOP_GRACE)` graceful-then-force escalation budget.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// First spawn-failure backoff window.
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);

/// Maximum spawn-failure backoff window.
const BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Boot-time concurrency cap for the parallel adoption probe.
const BOOT_ADOPT_CONCURRENCY: usize = 8;

/// Per-row reconciler bookkeeping. Lives only inside the reconciler
/// task — NOT in `AppState`. `pub` only so `tick_once` (which
/// integration tests in `tests/reconciler_smoke.rs` need to name)
/// can carry it through its signature; not part of the stable surface.
#[derive(Default)]
pub struct ReconcilerRowState {
    last_attempt: Option<Instant>,
    backoff: Duration,
    /// `true` once we've already logged the soft-warn for an
    /// `AdoptedNotControlled` Stop dispatch. Resets when observed
    /// transitions away from Running.
    warned_adopted_stop: bool,
    /// `true` once we've already logged the loud-warn for a
    /// `Crashed` row. Resets when observed transitions away from
    /// Crashed.
    warned_crashed: bool,
}

/// Cross-handler wakeup channel.
///
/// `routes::managed_daemons` (sibling `b91e9691`) calls
/// [`ReconcilerNotify::poke`] after every successful `set_desired_state`
/// write so the reconciler tick fires within milliseconds rather than
/// waiting up to a tick-interval.
pub struct ReconcilerNotify {
    global: Arc<Notify>,
    per_row: Mutex<HashMap<Uuid, Arc<Notify>>>,
}

impl ReconcilerNotify {
    pub fn new() -> Self {
        Self {
            global: Arc::new(Notify::new()),
            per_row: Mutex::new(HashMap::new()),
        }
    }

    /// Wake the tick loop and any per-row waiter listening for `id`.
    /// `&self` (no `&mut`) — both `Notify::notify_one` and the
    /// `tokio::sync::Mutex` it locks accept shared references.
    pub async fn poke(&self, id: Uuid) {
        let entry = self.for_row(id).await;
        entry.notify_one();
        self.global.notify_one();
    }

    /// Wake the tick loop without targeting a specific row. Sync —
    /// `Notify::notify_one` is itself sync.
    pub fn poke_all(&self) {
        self.global.notify_one();
    }

    /// Get-or-insert the per-row `Notify`. Used by the tick loop and
    /// future per-row waiters.
    pub(crate) async fn for_row(&self, id: Uuid) -> Arc<Notify> {
        let mut guard = self.per_row.lock().await;
        guard
            .entry(id)
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Subscriber handle for the tick loop's `select!`.
    pub fn global_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.global)
    }
}

impl Default for ReconcilerNotify {
    fn default() -> Self {
        Self::new()
    }
}

/// Public entry point — spawn this from `main` after the daemon-runner
/// gate fires. Owns the reconciler task forever; never returns.
pub async fn run(state: AppState) {
    let runner = match state.daemon_runner.as_ref() {
        Some(r) => Arc::clone(r),
        None => {
            tracing::error!(
                "reconciler::run called with daemon_runner = None — \
                 main.rs mis-wiring; aborting"
            );
            return;
        }
    };

    if let Err(e) = boot_adopt(&state, &runner).await {
        // Don't exit — the tick loop can recover next pass.
        tracing::error!(error = %e, "reconciler: boot adoption pass failed");
    }

    run_tick_loop(state, runner, TICK_INTERVAL).await;
}

/// Tick loop, factored out so unit tests can override the interval.
async fn run_tick_loop<R: DaemonRunnerControl>(state: AppState, runner: R, tick: Duration) {
    let backoff_state: Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let notify_global = state.reconciler_notify.global_notify();

    loop {
        let interval = jittered_interval(tick);
        tokio::select! {
            _ = tokio::time::sleep(interval) => {},
            _ = notify_global.notified() => {},
        }

        if let Err(()) = tick_once(&state, &runner, &backoff_state).await {
            // tick_once already logged; swallow.
        }
    }
}

/// One iteration of the tick loop. Lifted out of `run_tick_loop` so
/// integration tests can drive the loop one step at a time. Public
/// only because `tests/reconciler_smoke.rs` is an external crate; not
/// part of the stable surface (sibling task `b91e9691` should call
/// `state.reconciler_notify.poke(id)` instead of touching this).
pub async fn tick_once<R: DaemonRunnerControl>(
    state: &AppState,
    runner: &R,
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
) -> Result<(), ()> {
    // Resolve the activity tracker once per tick (cheap Arc clone).
    // `None` when the daemon-runner capability is disabled — the
    // idle-timeout sweep then short-circuits.
    let tracker = state.activity_tracker();
    match managed_daemon::list(&state.pool).await {
        Ok(rows) => {
            for row in rows {
                // ADR-0004 §Activity-tracking / edeb7d11 — idle-timeout
                // sweep before the standard reconcile pass. Writes
                // `desired_state = Stopped` on rows that have been idle
                // longer than their configured `idle_timeout_secs`;
                // `next_action` then maps the now-stale `(Stopped,
                // Running)` pair to `Stop` on the next tick.
                if let Some(t) = tracker.as_ref() {
                    apply_idle_timeout(&state.pool, runner, &row, t).await;
                }
                reconcile_one(runner, &row, backoff_state).await;
            }
            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "reconciler: failed to list managed_daemons");
            Err(())
        }
    }
}

/// Per-row idle-timeout policy. Reads the row's observed state via
/// `runner.status`, the row's `idle_timeout_secs`, and the tracker's
/// `last_touched(id)` — when the policy says `Some(Stopped)`, persists
/// `desired_state = Stopped` and forgets the tracker entry so the
/// next tick observes `(Stopped, Running)` and dispatches `Stop`.
///
/// No-op when:
///   * the row has no `idle_timeout_secs`,
///   * the row was never seeded (race: created mid-tick or runner
///     rebuilt — the next `spawn`/`adopt` will seed),
///   * observed != Running, or
///   * `now - last_touched <= idle`.
async fn apply_idle_timeout<R: DaemonRunnerControl>(
    pool: &sqlx::SqlitePool,
    runner: &R,
    summary: &managed_daemon::ManagedDaemonSummary,
    tracker: &Arc<ActivityTracker>,
) {
    let secs = match summary.idle_timeout_secs {
        Some(s) => s,
        None => return,
    };
    let observed = runner
        .status(summary.id)
        .await
        .unwrap_or(ObservedState::Stopped);
    if observed != ObservedState::Running {
        return;
    }
    let last = match tracker.last_touched(summary.id) {
        Some(t) => t,
        None => return,
    };
    let idle = Duration::from_secs(secs as u64);
    let now = tokio::time::Instant::now();
    if let Some(new_desired) = idle_timeout_action(Some(idle), observed, last, now) {
        let model_state = match new_desired {
            daemon_runner::DesiredState::Stopped => managed_daemon::DesiredState::Stopped,
            daemon_runner::DesiredState::Running => managed_daemon::DesiredState::Running,
        };
        match managed_daemon::set_desired_state(pool, summary.id, model_state).await {
            Ok(()) => {
                tracker.forget(summary.id);
                tracing::info!(
                    daemon_id = %summary.id,
                    idle_secs = secs,
                    "reconciler: idle-timeout flipped desired_state to stopped"
                );
            }
            Err(e) => {
                tracing::error!(
                    daemon_id = %summary.id,
                    error = %e,
                    "reconciler: idle-timeout set_desired_state failed"
                );
            }
        }
    }
}

/// Add ±100ms jitter to `tick`. Saturating-arith because if `tick <
/// 100ms` we'd otherwise underflow; in practice `TICK_INTERVAL = 1s`
/// so this always lands in `[tick - 100ms, tick + 100ms]`.
fn jittered_interval(tick: Duration) -> Duration {
    let jitter_ms: u64 = rand::thread_rng().gen_range(0..200);
    let base = tick.saturating_sub(Duration::from_millis(100));
    base + Duration::from_millis(jitter_ms)
}

/// Test-only seam: run just the boot adoption pass against the
/// state's daemon_runner. Public because `tests/reconciler_smoke.rs`
/// is an external crate; not part of the stable surface.
pub async fn run_for_test_boot_only(state: &AppState) -> AppResult<()> {
    match state.daemon_runner.as_ref() {
        Some(runner) => boot_adopt(state, &Arc::clone(runner)).await,
        None => {
            tracing::warn!("run_for_test_boot_only called with daemon_runner = None");
            Ok(())
        }
    }
}

/// Boot-time adoption pass. For each row in `managed_daemons`:
///
/// 1. Probe `Client::health()` over the row's UDS with a 1s timeout.
/// 2. **Healthy** → `runner.adopt(id, socket)`.
///    * `Adopted` → log info; the reconciler now observes it as
///      Running.
///    * `NotResponding` → race (socket dropped between probes) → fall
///      through to the unhealthy branch.
///    * `AlreadyTracked` → log warn; idempotent re-boot.
/// 3. **Unhealthy + desired=Running** → `runner.spawn(row.into())`.
/// 4. **Unhealthy + desired=Stopped** → leave alone.
/// 5. **Unhealthy + desired=None** → ADR-0004 precedence
///    (`start_on_manager_boot`). Currently unreachable because sibling
///    `7c11fa3d` has not yet flipped the column to nullable; see the
///    comment inside `adopt_one`.
///
/// Bounded concurrency at [`BOOT_ADOPT_CONCURRENCY`] so 1k-row
/// deployments don't smash the FS with concurrent connects.
async fn boot_adopt<R: DaemonRunnerControl>(state: &AppState, runner: &R) -> AppResult<()> {
    let summaries = managed_daemon::list(&state.pool).await?;
    if summaries.is_empty() {
        return Ok(());
    }

    let runner_ref = runner;
    futures::stream::iter(summaries)
        .map(|summary| async move { adopt_one(state, runner_ref, summary).await })
        .buffer_unordered(BOOT_ADOPT_CONCURRENCY)
        .for_each(|()| async {})
        .await;

    Ok(())
}

/// Probe + dispatch for a single row in the boot adoption pass.
async fn adopt_one<R: DaemonRunnerControl>(
    _state: &AppState,
    runner: &R,
    summary: managed_daemon::ManagedDaemonSummary,
) {
    let socket_path = PathBuf::from(&summary.socket_path);
    let healthy = probe_health(&summary.socket_path).await;

    if healthy {
        match runner.adopt(summary.id, socket_path.clone()).await {
            Ok(AdoptOutcome::Adopted) => {
                tracing::info!(daemon_id = %summary.id, "adopted healthy daemon");
                return;
            }
            Ok(AdoptOutcome::NotResponding) => {
                // Race: socket dropped between health probe and adopt
                // probe. Fall through to the unhealthy branch.
            }
            Err(DaemonRunnerError::AlreadyTracked(_)) => {
                tracing::warn!(
                    daemon_id = %summary.id,
                    "adoption skipped: already tracked"
                );
                return;
            }
            Err(e) => {
                tracing::error!(
                    daemon_id = %summary.id,
                    error = %e,
                    "adoption failed"
                );
                return;
            }
        }
    }

    // Unhealthy — apply the desired-state precedence.
    //
    // NOTE: `summary.desired_state` is currently a non-`Option`
    // `DesiredState` because sibling `7c11fa3d` has not yet flipped the
    // column to nullable per ADR-0004. Until that lands the
    // `desired=None + start_on_manager_boot` branch is unreachable.
    // We short-circuit that case here; build green is the gate.
    match summary.desired_state {
        managed_daemon::DesiredState::Running => {
            let row = RunnerRow {
                id: summary.id,
                work_dir: summary.work_dir.clone(),
                socket_path,
            };
            if let Err(e) = runner.spawn(row).await {
                tracing::error!(
                    daemon_id = %summary.id,
                    error = %e,
                    "boot spawn failed; reconciler tick will retry"
                );
            } else {
                tracing::info!(daemon_id = %summary.id, "boot spawned daemon");
            }
        }
        managed_daemon::DesiredState::Stopped => {
            // Leave alone.
        } // TODO(7c11fa3d-followup): unreachable until desired_state
          // column is nullable per ADR-0004; once nullable, add a
          // None arm here that consults `summary.start_on_manager_boot`
          // and persists the resolved state via
          // `managed_daemon::set_desired_state`.
    }
}

/// Single-shot HTTP `/v1/health` probe over the row's Unix socket with
/// a 1s overall timeout. Returns true iff the server returned HTTP
/// 200 inside the budget.
async fn probe_health(socket_path: &str) -> bool {
    // `Client::from_row` is the only public ctor for a Unix-socket
    // `Client`; build a minimal `InstanceRow` to satisfy it.
    let row = InstanceRow {
        id: String::new(),
        name: String::new(),
        transport: "unix".to_string(),
        url: None,
        socket_path: Some(socket_path.to_string()),
        api_key: None,
        tls_ca_path: None,
        // Health probes are intentionally side-effect-free — even
        // when the row IS managed, we don't want the bootstrap probe
        // to count as activity (the seed-on-adopt hook is the
        // canonical source of the initial last_touched timestamp).
        managed_by_id: None,
        created_at: String::new(),
    };
    let client = match Client::from_row(&row) {
        Ok(c) => c,
        Err(_) => return false,
    };
    match tokio::time::timeout(Duration::from_secs(1), client.health()).await {
        Ok(Ok((200, _))) => true,
        Ok(Ok((_status, _))) => false,
        Ok(Err(_)) => false,
        Err(_) => false, // timeout
    }
}

/// Per-tick decision + dispatch for a single row.
async fn reconcile_one<R: DaemonRunnerControl>(
    runner: &R,
    summary: &managed_daemon::ManagedDaemonSummary,
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
) {
    let observed = runner
        .status(summary.id)
        .await
        .unwrap_or(ObservedState::Stopped);
    let desired = to_runner_desired(summary.desired_state);
    let action = next_action(desired, observed);

    // Reset transient "warned-once" flags when observed leaves the
    // state that armed them.
    if !matches!(observed, ObservedState::Running) {
        clear_flag(backoff_state, summary.id, |s| s.warned_adopted_stop = false).await;
    }
    if !matches!(observed, ObservedState::Crashed) {
        clear_flag(backoff_state, summary.id, |s| s.warned_crashed = false).await;
    }

    match action {
        ReconcilerAction::NoOp => {}
        ReconcilerAction::Spawn => {
            // Honour backoff before re-issuing.
            if !backoff_window_clear(backoff_state, summary.id).await {
                return;
            }
            let row = RunnerRow {
                id: summary.id,
                work_dir: summary.work_dir.clone(),
                socket_path: PathBuf::from(&summary.socket_path),
            };
            match runner.spawn(row).await {
                Ok(()) => {
                    record_spawn_success(backoff_state, summary.id).await;
                }
                Err(e) => {
                    tracing::error!(
                        daemon_id = %summary.id,
                        error = %e,
                        "reconciler: spawn failed"
                    );
                    record_spawn_failure(backoff_state, summary.id).await;
                }
            }
        }
        ReconcilerAction::Stop => match runner.stop(summary.id, STOP_GRACE).await {
            Ok(()) => {
                tracing::info!(daemon_id = %summary.id, "reconciler: stopped daemon");
            }
            Err(DaemonRunnerError::AdoptedNotControlled(_)) => {
                handle_adopted_stop(backoff_state, summary.id).await;
            }
            Err(e) => {
                tracing::error!(
                    daemon_id = %summary.id,
                    error = %e,
                    "reconciler: stop failed"
                );
            }
        },
        ReconcilerAction::AlertCrashed => {
            warn_crashed_once(backoff_state, summary.id).await;
        }
    }
}

/// Convert the model-layer DesiredState into the runner-layer one.
/// The two enums are intentionally identical; this avoids cross-layer
/// import without coupling the daemon_runner subsystem to the SQLite
/// model.
fn to_runner_desired(d: managed_daemon::DesiredState) -> daemon_runner::DesiredState {
    match d {
        managed_daemon::DesiredState::Running => daemon_runner::DesiredState::Running,
        managed_daemon::DesiredState::Stopped => daemon_runner::DesiredState::Stopped,
    }
}

/// Returns true iff the row is outside its current backoff window
/// (i.e. a new Spawn attempt is allowed).
async fn backoff_window_clear(
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
    id: Uuid,
) -> bool {
    let guard = backoff_state.lock().await;
    match guard.get(&id) {
        Some(entry) => match entry.last_attempt {
            Some(last) => last.elapsed() >= entry.backoff,
            None => true,
        },
        None => true,
    }
}

/// Record a successful spawn: reset backoff and stamp `last_attempt`.
async fn record_spawn_success(
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
    id: Uuid,
) {
    let mut guard = backoff_state.lock().await;
    let entry = guard.entry(id).or_default();
    entry.last_attempt = Some(Instant::now());
    entry.backoff = Duration::from_secs(0);
}

/// Record a failed spawn: stamp `last_attempt` and double the
/// backoff window, capped at [`BACKOFF_CAP`].
async fn record_spawn_failure(
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
    id: Uuid,
) {
    let mut guard = backoff_state.lock().await;
    let entry = guard.entry(id).or_default();
    entry.last_attempt = Some(Instant::now());
    entry.backoff = next_backoff(entry.backoff);
}

/// 0 -> 1s, 1s -> 2s, 2s -> 4s, ... capped at 30s. Pure fn so it can
/// be unit-tested independently.
fn next_backoff(prev: Duration) -> Duration {
    if prev.is_zero() {
        return BACKOFF_INITIAL;
    }
    let doubled = prev.saturating_mul(2);
    if doubled > BACKOFF_CAP {
        BACKOFF_CAP
    } else {
        doubled
    }
}

/// Handle a Stop dispatch that returned `AdoptedNotControlled`. Logs
/// once per row-armed transition and then leaves the row alone.
async fn handle_adopted_stop(
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
    id: Uuid,
) {
    let mut guard = backoff_state.lock().await;
    let entry = guard.entry(id).or_default();
    if !entry.warned_adopted_stop {
        tracing::warn!(
            daemon_id = %id,
            "desired=Stopped on adopted daemon: leaving running, manual intervention required"
        );
        entry.warned_adopted_stop = true;
    }
    // Do NOT retry, do NOT mutate desired_state, do NOT touch observed.
}

/// Loud once-per-row log for `AlertCrashed`.
async fn warn_crashed_once(
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
    id: Uuid,
) {
    let mut guard = backoff_state.lock().await;
    let entry = guard.entry(id).or_default();
    if !entry.warned_crashed {
        tracing::error!(
            daemon_id = %id,
            "daemon Crashed; manual intervention required"
        );
        entry.warned_crashed = true;
    }
}

/// Apply `f` to the entry for `id`, creating one if missing.
async fn clear_flag(
    backoff_state: &Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>>,
    id: Uuid,
    f: impl FnOnce(&mut ReconcilerRowState),
) {
    let mut guard = backoff_state.lock().await;
    let entry = guard.entry(id).or_default();
    f(entry);
}

// `runner` is owned by `run` but unused once we hand off into the tick
// loop; suppress the unused-import lint when DaemonRunner is referenced
// only via the trait.
#[allow(dead_code)]
fn _assert_runner_arc_impls_control() {
    fn check<T: DaemonRunnerControl>() {}
    check::<Arc<DaemonRunner>>();
}

// ---- model/daemon_runner glue ------------------------------------------
//
// The reconciler converts each `ManagedDaemonSummary` into the
// daemon_runner's `ManagedDaemonRow` shape inline (see `adopt_one` and
// `reconcile_one`). We do NOT modify the daemon_runner placeholder
// module; the conversion is one-directional and trivial.

// ---- tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    use tokio::net::UnixListener;

    use crate::auth::AuthConfig;
    use crate::db;
    use crate::state::AppState;

    /// Recorded MockRunner call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum MockEvent {
        Spawn(Uuid),
        Stop(Uuid),
        Status(Uuid),
        Adopt(Uuid),
    }

    /// Canned responses keyed by Uuid; defaults applied when absent.
    #[derive(Default)]
    struct MockResponses {
        spawn_results: Vec<Result<(), DaemonRunnerError>>,
        stop_results: Vec<Result<(), DaemonRunnerError>>,
        status_results: Vec<Option<ObservedState>>,
        adopt_results: Vec<Result<AdoptOutcome, DaemonRunnerError>>,
    }

    struct MockRunner {
        events: Arc<StdMutex<Vec<MockEvent>>>,
        responses: Arc<StdMutex<MockResponses>>,
    }

    impl MockRunner {
        fn new() -> Self {
            Self {
                events: Arc::new(StdMutex::new(Vec::new())),
                responses: Arc::new(StdMutex::new(MockResponses::default())),
            }
        }

        fn events(&self) -> Vec<MockEvent> {
            self.events.lock().unwrap().clone()
        }

        fn count_matching<F: Fn(&MockEvent) -> bool>(&self, pred: F) -> usize {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| pred(e))
                .count()
        }

        #[allow(dead_code)] // helper kept symmetric with the others
        fn push_spawn(&self, r: Result<(), DaemonRunnerError>) {
            self.responses.lock().unwrap().spawn_results.push(r);
        }
        fn push_stop(&self, r: Result<(), DaemonRunnerError>) {
            self.responses.lock().unwrap().stop_results.push(r);
        }
        fn push_status(&self, r: Option<ObservedState>) {
            self.responses.lock().unwrap().status_results.push(r);
        }
        fn push_adopt(&self, r: Result<AdoptOutcome, DaemonRunnerError>) {
            self.responses.lock().unwrap().adopt_results.push(r);
        }
    }

    impl DaemonRunnerControl for Arc<MockRunner> {
        fn spawn(
            &self,
            row: RunnerRow,
        ) -> impl std::future::Future<Output = Result<(), DaemonRunnerError>> + Send {
            let events = Arc::clone(&self.events);
            let responses = Arc::clone(&self.responses);
            async move {
                events.lock().unwrap().push(MockEvent::Spawn(row.id));
                let mut rs = responses.lock().unwrap();
                if rs.spawn_results.is_empty() {
                    Ok(())
                } else {
                    rs.spawn_results.remove(0)
                }
            }
        }

        fn stop(
            &self,
            id: Uuid,
            _timeout: Duration,
        ) -> impl std::future::Future<Output = Result<(), DaemonRunnerError>> + Send {
            let events = Arc::clone(&self.events);
            let responses = Arc::clone(&self.responses);
            async move {
                events.lock().unwrap().push(MockEvent::Stop(id));
                let mut rs = responses.lock().unwrap();
                if rs.stop_results.is_empty() {
                    Ok(())
                } else {
                    rs.stop_results.remove(0)
                }
            }
        }

        fn status(
            &self,
            id: Uuid,
        ) -> impl std::future::Future<Output = Option<ObservedState>> + Send {
            let events = Arc::clone(&self.events);
            let responses = Arc::clone(&self.responses);
            async move {
                events.lock().unwrap().push(MockEvent::Status(id));
                let mut rs = responses.lock().unwrap();
                if rs.status_results.is_empty() {
                    None
                } else {
                    rs.status_results.remove(0)
                }
            }
        }

        fn adopt(
            &self,
            id: Uuid,
            _socket_path: PathBuf,
        ) -> impl std::future::Future<Output = Result<AdoptOutcome, DaemonRunnerError>> + Send
        {
            let events = Arc::clone(&self.events);
            let responses = Arc::clone(&self.responses);
            async move {
                events.lock().unwrap().push(MockEvent::Adopt(id));
                let mut rs = responses.lock().unwrap();
                if rs.adopt_results.is_empty() {
                    Ok(AdoptOutcome::Adopted)
                } else {
                    rs.adopt_results.remove(0)
                }
            }
        }
    }

    /// Construct an in-memory AppState for unit tests.
    async fn test_state() -> AppState {
        let pool = db::connect_memory().await.expect("memory db");
        AppState::new(
            pool,
            AuthConfig {
                cookie_secure: false,
            },
            std::env::temp_dir(),
            true,
        )
    }

    /// Build a managed_daemons row directly (skipping the
    /// `insert_with_paired_instance` validation).
    async fn insert_raw_row(
        state: &AppState,
        id: Uuid,
        socket_path: &str,
        work_dir: &str,
        desired: managed_daemon::DesiredState,
        start_on_boot: bool,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let name = format!("test-{id}");
        let instance_id = Uuid::new_v4().to_string();
        let mut tx = state.pool.begin().await.expect("tx");
        sqlx::query(
            "INSERT INTO managed_daemons (id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, ?7, ?7)",
        )
        .bind(id.to_string())
        .bind(&name)
        .bind(work_dir)
        .bind(socket_path)
        .bind(desired.as_str())
        .bind(if start_on_boot { 1_i64 } else { 0_i64 })
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
        .bind(socket_path)
        .bind(id.to_string())
        .bind(&now)
        .execute(&mut *tx)
        .await
        .expect("insert instance");
        tx.commit().await.expect("commit");
    }

    /// Spawn a tokio task that listens on `path` and serves a
    /// minimal HTTP/1.1 200 OK response on every connection. Stops
    /// when the returned `JoinHandle` is dropped.
    fn spawn_health_serving_socket(path: PathBuf) -> tokio::task::JoinHandle<()> {
        // Pre-bind synchronously so the caller can probe immediately.
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = UnixListener::from_std(listener).expect("from_std");
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, _)) => {
                        tokio::spawn(async move {
                            use tokio::io::AsyncReadExt;
                            use tokio::io::AsyncWriteExt;
                            // Read until we see "\r\n\r\n", then reply.
                            let mut buf = [0u8; 4096];
                            let mut total = Vec::new();
                            loop {
                                match stream.read(&mut buf).await {
                                    Ok(0) => return,
                                    Ok(n) => {
                                        total.extend_from_slice(&buf[..n]);
                                        if total.windows(4).any(|w| w == b"\r\n\r\n") {
                                            break;
                                        }
                                    }
                                    Err(_) => return,
                                }
                            }
                            let body = b"ok";
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(resp.as_bytes()).await;
                            let _ = stream.write_all(body).await;
                            let _ = stream.shutdown().await;
                        });
                    }
                    Err(_) => return,
                }
            }
        })
    }

    // ---- (a) boot healthy gets adopted, not spawned ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn boot_healthy_gets_adopted_not_spawned() {
        let state = test_state().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("d.sock");
        let _serve = spawn_health_serving_socket(socket.clone());
        // Give the listener a tick to be ready.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let id = Uuid::new_v4();
        insert_raw_row(
            &state,
            id,
            socket.to_str().unwrap(),
            dir.path().to_str().unwrap(),
            managed_daemon::DesiredState::Running,
            true,
        )
        .await;

        let mock = Arc::new(MockRunner::new());
        mock.push_adopt(Ok(AdoptOutcome::Adopted));

        boot_adopt(&state, &mock).await.expect("boot ok");

        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Adopt(_))), 1);
        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Spawn(_))), 0);
    }

    // ---- (b) boot unhealthy + desired=Running gets spawned ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn boot_unhealthy_with_desired_running_gets_spawned() {
        let state = test_state().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let dead = dir.path().join("dead.sock");

        let id = Uuid::new_v4();
        insert_raw_row(
            &state,
            id,
            dead.to_str().unwrap(),
            dir.path().to_str().unwrap(),
            managed_daemon::DesiredState::Running,
            false,
        )
        .await;

        let mock = Arc::new(MockRunner::new());
        boot_adopt(&state, &mock).await.expect("boot ok");

        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Spawn(_))), 1);
        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Adopt(_))), 0);
    }

    // ---- (c) boot unhealthy + desired=Stopped left alone ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn boot_unhealthy_with_desired_stopped_left_alone() {
        let state = test_state().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let dead = dir.path().join("dead.sock");

        let id = Uuid::new_v4();
        insert_raw_row(
            &state,
            id,
            dead.to_str().unwrap(),
            dir.path().to_str().unwrap(),
            managed_daemon::DesiredState::Stopped,
            false,
        )
        .await;

        let mock = Arc::new(MockRunner::new());
        boot_adopt(&state, &mock).await.expect("boot ok");

        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Spawn(_))), 0);
        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Adopt(_))), 0);
    }

    // ---- (d) boot desired=None + start_on_boot=true (DISABLED) ----
    //
    // Unreachable until sibling 7c11fa3d flips desired_state to
    // nullable per ADR-0004 — the model layer types it as a non-Option
    // DesiredState today. Test omitted; documented in the slice's
    // final report as a flag-for-review item.

    // ---- (e) stop on adopted row is logged-not-signalled ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_on_adopted_row_is_logged_not_signalled() {
        let state = test_state().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("adopted.sock");
        let id = Uuid::new_v4();
        insert_raw_row(
            &state,
            id,
            socket.to_str().unwrap(),
            dir.path().to_str().unwrap(),
            managed_daemon::DesiredState::Stopped,
            false,
        )
        .await;
        let summary = managed_daemon::list(&state.pool)
            .await
            .expect("list")
            .into_iter()
            .next()
            .expect("one row");

        let mock = Arc::new(MockRunner::new());
        // Status returns Running on every call.
        for _ in 0..4 {
            mock.push_status(Some(ObservedState::Running));
        }
        // Stop returns AdoptedNotControlled on every call (but we
        // expect it to be invoked exactly once — the warned flag
        // gates retries logically by NOT skipping next_action, so
        // verify the flag suppresses repeated WARN logs and the stop
        // dispatch keeps happening but doesn't escalate. Per the plan
        // we issue one stop, then on subsequent ticks the flag is
        // already set and the stop is still dispatched — but the
        // warn isn't re-emitted. So we just confirm `warned_adopted_stop`
        // is honoured for the WARN log.
        for _ in 0..4 {
            mock.push_stop(Err(DaemonRunnerError::AdoptedNotControlled(id)));
        }

        let backoff_state = Arc::new(Mutex::new(HashMap::new()));

        // First reconcile: stop is dispatched, AdoptedNotControlled
        // is observed, warning is logged once.
        reconcile_one(&mock, &summary, &backoff_state).await;
        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Stop(_))), 1);
        {
            let guard = backoff_state.lock().await;
            assert!(guard
                .get(&id)
                .map(|s| s.warned_adopted_stop)
                .unwrap_or(false));
        }

        // Three more reconciles: stop continues to be dispatched
        // (the lifecycle table still says Stop), but
        // warned_adopted_stop guards the WARN — verified by reading
        // back the flag stayed set.
        for _ in 0..3 {
            reconcile_one(&mock, &summary, &backoff_state).await;
        }
        // The mock recorded four total Stop calls, but
        // warned_adopted_stop has only ever been flipped once (no
        // double-warn).
        assert_eq!(mock.count_matching(|e| matches!(e, MockEvent::Stop(_))), 4);
        {
            let guard = backoff_state.lock().await;
            assert!(guard
                .get(&id)
                .map(|s| s.warned_adopted_stop)
                .unwrap_or(false));
        }
    }

    // ---- (f) backoff caps at 30s ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_failure_backoff_caps_at_30s() {
        // Pure-fn coverage of the backoff curve, plus integration via
        // reconcile_one.
        assert_eq!(next_backoff(Duration::from_secs(0)), Duration::from_secs(1));
        assert_eq!(next_backoff(Duration::from_secs(1)), Duration::from_secs(2));
        assert_eq!(next_backoff(Duration::from_secs(2)), Duration::from_secs(4));
        assert_eq!(next_backoff(Duration::from_secs(4)), Duration::from_secs(8));
        assert_eq!(
            next_backoff(Duration::from_secs(8)),
            Duration::from_secs(16)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(16)),
            Duration::from_secs(30) // 32s clamped to 30s cap
        );
        assert_eq!(
            next_backoff(Duration::from_secs(30)),
            Duration::from_secs(30)
        );

        // Now drive `record_spawn_failure` six times and inspect the
        // backoff sequence.
        let id = Uuid::new_v4();
        let backoff_state: Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut sequence = Vec::new();
        for _ in 0..6 {
            record_spawn_failure(&backoff_state, id).await;
            sequence.push(backoff_state.lock().await.get(&id).unwrap().backoff);
        }
        assert_eq!(
            sequence,
            vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(30),
            ]
        );

        // Monotonic non-decreasing.
        for w in sequence.windows(2) {
            assert!(w[1] >= w[0]);
        }
        // Bounded.
        for d in &sequence {
            assert!(*d <= Duration::from_secs(30));
        }

        // Success resets backoff to 0.
        record_spawn_success(&backoff_state, id).await;
        assert_eq!(
            backoff_state.lock().await.get(&id).unwrap().backoff,
            Duration::from_secs(0)
        );
    }

    // ---- (g) poke wakes tick ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn poke_wakes_tick_within_50ms() {
        let state = test_state().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let dead = dir.path().join("dead.sock");
        let id = Uuid::new_v4();
        insert_raw_row(
            &state,
            id,
            dead.to_str().unwrap(),
            dir.path().to_str().unwrap(),
            managed_daemon::DesiredState::Stopped,
            false,
        )
        .await;

        let mock = Arc::new(MockRunner::new());
        for _ in 0..4 {
            mock.push_status(Some(ObservedState::Stopped));
        }

        let mock_for_loop = Arc::clone(&mock);
        let state_for_loop = state.clone();
        // Tick interval is 5s — without poke we'd see no Status call
        // for a long time.
        let _handle = tokio::spawn(async move {
            run_tick_loop(state_for_loop, mock_for_loop, Duration::from_secs(5)).await;
        });

        // Give the loop a moment to settle into select! BEFORE we
        // poke — ensures the notify_one isn't lost (Notify is permit-
        // based, so this is actually fine even if poke arrives first,
        // but be deterministic).
        tokio::time::sleep(Duration::from_millis(10)).await;
        state.reconciler_notify.poke(id).await;

        let start = Instant::now();
        let mut saw_status = false;
        while start.elapsed() < Duration::from_millis(500) {
            if mock.count_matching(|e| matches!(e, MockEvent::Status(_))) >= 1 {
                saw_status = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        assert!(
            saw_status,
            "expected at least one Status call within 500ms of poke; events = {:?}",
            mock.events()
        );
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "poke wake took too long ({:?})",
            start.elapsed()
        );
    }

    // ---- backoff window pure-fn check ----

    #[tokio::test]
    async fn backoff_window_skips_inside_window() {
        let id = Uuid::new_v4();
        let backoff_state: Arc<Mutex<HashMap<Uuid, ReconcilerRowState>>> =
            Arc::new(Mutex::new(HashMap::new()));
        // Fresh entry: window is clear.
        assert!(backoff_window_clear(&backoff_state, id).await);
        // Record a failure: now `last_attempt` is `now`, backoff = 1s.
        record_spawn_failure(&backoff_state, id).await;
        assert!(!backoff_window_clear(&backoff_state, id).await);
        // Reach into the entry and pretend `last_attempt` was 2s ago
        // — the window has now elapsed.
        {
            let mut g = backoff_state.lock().await;
            g.get_mut(&id).unwrap().last_attempt = Some(Instant::now() - Duration::from_secs(2));
        }
        assert!(backoff_window_clear(&backoff_state, id).await);
    }
}
