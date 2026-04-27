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
