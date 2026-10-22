use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::auth::AuthConfig;
use crate::daemon_runner::{CapturePolicy, DaemonRunner};
use crate::reconciler::ReconcilerNotify;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub auth: AuthConfig,
    pub data_dir: Arc<PathBuf>,
    /// Owns the OS child processes for managed daemons.
    pub daemon_runner: Arc<DaemonRunner>,
    /// Cross-handler wakeup channel into the reconciler. Sibling
    /// `b91e9691` calls `state.reconciler_notify.poke(id)` after every
    /// successful `set_desired_state` write; the reconciler tick wakes
    /// within milliseconds rather than waiting up to a tick-interval.
    pub reconciler_notify: Arc<ReconcilerNotify>,
    /// Resolved path to the `aciddog` binary used by both the
    /// daemon-runner (spawn children) and the scaffolding flow
    /// (`aciddog init`, `aciddog daemon --check`). Resolved at startup
    /// and wrapped in `Arc<PathBuf>` so cloning `AppState` stays cheap.
    pub aciddog_binary: Arc<PathBuf>,
    /// Per-child stdio capture policy (sibling 3bc8a0d0). Driven by
    /// `--daemon-runner-log-max-bytes / --max-files / --ring-bytes`
    /// (and the matching env vars). Defaulted at construction; main.rs
    /// overwrites with the CLI-derived value when the gate is on.
    /// Cheap to clone (three primitives).
    pub capture_policy: CapturePolicy,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        auth: AuthConfig,
        data_dir: PathBuf,
        aciddog_binary: Arc<PathBuf>,
        daemon_runner: Arc<DaemonRunner>,
        capture_policy: CapturePolicy,
    ) -> Self {
        Self {
            pool,
            auth,
            data_dir: Arc::new(data_dir),
            daemon_runner,
            reconciler_notify: Arc::new(ReconcilerNotify::new()),
            aciddog_binary,
            capture_policy,
        }
    }

    /// Hand out a clone of the runner's `Arc<ActivityTracker>`.
    /// ADR-0004 §Activity-tracking — used by the IPC client chokepoint
    /// and the reconciler tick to share a single `last_touched` map.
    pub fn activity_tracker(&self) -> Option<Arc<crate::daemon_runner::ActivityTracker>> {
        Some(self.daemon_runner.activity_tracker())
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.data_dir.join("skills")
    }

    pub fn marketplaces_dir(&self) -> PathBuf {
        self.data_dir.join("marketplaces")
    }

    /// Root directory holding per-team work_dirs scaffolded by
    /// `scaffolding::create_managed_daemon`. Each managed daemon owns
    /// `<teams_dir>/<name>/` with its `aciddog.wcl` + `.aciddog/` socket
    /// parent inside.
    pub fn teams_dir(&self) -> PathBuf {
        self.data_dir.join("teams")
    }
}
