use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone)]
#[command(name = "aciddog-manager", version, about)]
pub struct CliArgs {
    #[arg(
        long,
        env = "ACIDDOG_MANAGER_BIND",
        default_value = "127.0.0.1:7000",
        global = true
    )]
    pub bind: String,

    #[arg(
        long,
        env = "ACIDDOG_MANAGER_DB",
        default_value = "./aciddog-manager.db",
        global = true
    )]
    pub db: PathBuf,

    /// Filesystem location for the skills catalog and marketplace clones.
    /// Created on demand the first time a skill is uploaded or a
    /// marketplace is added. Colocated with `--db` by default so the
    /// SQLite file and the blobs it indexes back up as one unit.
    #[arg(
        long,
        env = "ACIDDOG_MANAGER_DATA",
        default_value = "./aciddog-manager-data",
        global = true
    )]
    pub data_dir: PathBuf,

    /// Add `Secure` to the session cookie. Off by default because the
    /// portal binds to 127.0.0.1; turn on when fronted by TLS so
    /// browsers refuse to send the cookie over plain HTTP.
    #[arg(long, env = "ACIDDOG_MANAGER_COOKIE_SECURE", global = true)]
    pub cookie_secure: bool,

    /// Enable the daemon-runner capability (ADR-0005).
    ///
    /// Presence of either the CLI flag or the env var (any value, including empty)
    /// turns this ON; default is OFF. Mirrors the cookie_secure precedent — see
    /// the cookie_secure field in this file for the same bool+env shape.
    ///
    /// When OFF, the router skips /api/managed-daemons entirely (404, not 403 —
    /// the capability is literally absent) and the reconciler/daemon_runner
    /// subsystem is not started. When ON, /api/managed-daemons gets the
    /// require_admin layer at group root (concrete handlers land in sibling
    /// b91e9691).
    #[arg(long, env = "ACIDDOG_MANAGER_ENABLE_DAEMON_RUNNER", global = true)]
    pub enable_daemon_runner: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Set or reset a user's password. Creates the user if missing.
    /// Exits without starting the server.
    ResetPassword {
        #[arg(long, default_value = "admin")]
        username: String,
        /// If omitted, a random password is generated and printed.
        #[arg(long)]
        password: Option<String>,
    },
}
