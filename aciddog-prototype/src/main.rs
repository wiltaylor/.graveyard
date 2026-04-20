use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use aciddog::cli::Args;
use aciddog::config::load_config;
use aciddog::tui;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let file_appender = tracing_appender::rolling::never(".", "aciddog.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_new(args.effective_log_directive())
        .unwrap_or_else(|_| EnvFilter::new("aciddog=info"));

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(filter)
        .with_ansi(false)
        .init();

    let config = load_config(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;

    tui::run(config).await
}
