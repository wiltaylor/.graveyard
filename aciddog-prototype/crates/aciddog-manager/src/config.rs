use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "aciddog-manager", version, about)]
pub struct CliArgs {
    #[arg(long, env = "ACIDDOG_MANAGER_BIND", default_value = "127.0.0.1:7000")]
    pub bind: String,

    #[arg(
        long,
        env = "ACIDDOG_MANAGER_DB",
        default_value = "./aciddog-manager.db"
    )]
    pub db: PathBuf,

    #[arg(
        long,
        env = "ACIDDOG_MANAGER_STATIC",
        default_value = "./crates/aciddog-manager/web/dist"
    )]
    pub static_dir: PathBuf,
}
