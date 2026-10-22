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

    /// Path to the `aciddog` binary used to spawn managed daemon
    /// children. When unset the manager resolves it from a sibling
    /// binary, then PATH.
    #[arg(long, env = "ACIDDOG_BINARY", global = true)]
    pub aciddog_binary: Option<PathBuf>,

    /// Rotating-log threshold for per-child manager-side stdio capture
    /// (sibling 3bc8a0d0). Rotation happens on the next write that
    /// crosses this size. Default 10 MiB.
    #[arg(
        long,
        env = "ACIDDOG_MANAGER_DAEMON_LOG_MAX_BYTES",
        default_value_t = 10 * 1024 * 1024,
        global = true
    )]
    pub daemon_runner_log_max_bytes: u64,

    /// How many historical rotated log files to retain
    /// (`.log.1`..`.log.<max_files>`). Default 3.
    #[arg(
        long,
        env = "ACIDDOG_MANAGER_DAEMON_LOG_MAX_FILES",
        default_value_t = 3,
        global = true
    )]
    pub daemon_runner_log_max_files: u8,

    /// Per-child in-memory ring buffer cap (bytes). Drives the SSE
    /// replay window for `/api/managed-daemons/:id/logs`. Default 1 MiB.
    #[arg(
        long,
        env = "ACIDDOG_MANAGER_DAEMON_LOG_RING_BYTES",
        default_value_t = 1024 * 1024,
        global = true
    )]
    pub daemon_runner_log_ring_bytes: usize,

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
