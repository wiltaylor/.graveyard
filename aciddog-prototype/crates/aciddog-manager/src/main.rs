use std::net::SocketAddr;

use aciddog_manager::{config::CliArgs, db, routes, state::AppState};
use anyhow::{Context, Result};
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = CliArgs::parse();
    let pool = db::connect(&args.db).await?;
    let state = AppState::new(pool);
    let router = routes::build_router(state, &args.static_dir);

    let addr: SocketAddr = args
        .bind
        .parse()
        .with_context(|| format!("parsing bind address `{}`", args.bind))?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(
        %addr,
        db = %args.db.display(),
        static_dir = %args.static_dir.display(),
        "aciddog-manager listening"
    );
    axum::serve(listener, router).await.context("axum::serve")?;
    Ok(())
}
