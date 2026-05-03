use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::auth::AuthConfig;
use crate::reconciler::ReconcilerNotify;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub auth: AuthConfig,
    pub data_dir: Arc<PathBuf>,
    /// ADR-0005 daemon-runner capability gate. When false, the
    /// /api/managed-daemons router is not registered at all (404
    /// capability-literally-absent) and the reconciler/daemon_runner
    /// subsystem is not started. Driven by `--enable-daemon-runner`
    /// / `ACIDDOG_MANAGER_ENABLE_DAEMON_RUNNER`. Default OFF.
    pub is_daemon_runner_enabled: bool,
    /// Owns the OS child processes for managed daemons.
    ///
    /// Sibling 33fc8b7b will populate this conditional on
    /// `is_daemon_runner_enabled`. Until that lands, the field is
    /// always `None` and any consumer must treat the runner as
    /// optional.
    pub daemon_runner: Option<Arc<crate::daemon_runner::DaemonRunner>>,
    /// Cross-handler wakeup channel into the reconciler. Sibling
    /// `b91e9691` calls `state.reconciler_notify.poke(id)` after every
    /// successful `set_desired_state` write; the reconciler tick wakes
    /// within milliseconds rather than waiting up to a tick-interval.
    pub reconciler_notify: Arc<ReconcilerNotify>,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        auth: AuthConfig,
        data_dir: PathBuf,
        is_daemon_runner_enabled: bool,
    ) -> Self {
        Self {
            pool,
            auth,
            data_dir: Arc::new(data_dir),
            is_daemon_runner_enabled,
            daemon_runner: None,
            reconciler_notify: Arc::new(ReconcilerNotify::new()),
        }
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.data_dir.join("skills")
    }

    pub fn marketplaces_dir(&self) -> PathBuf {
        self.data_dir.join("marketplaces")
    }
}
