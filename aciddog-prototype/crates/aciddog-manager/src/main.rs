use std::net::SocketAddr;
use std::sync::Arc;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::config::{CliArgs, Command};
use aciddog_manager::daemon_runner;
use aciddog_manager::model::session;
use aciddog_manager::{db, reconciler, routes, seed, state::AppState};
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

    if let Some(cmd) = args.command.clone() {
        match cmd {
            Command::ResetPassword { username, password } => {
                seed::reset_password_command(&pool, &username, password.as_deref()).await?;
            }
        }
        return Ok(());
    }

    seed::ensure_admin_user(&pool).await?;
    let _ = session::purge_expired(&pool).await;

    let capture_policy = daemon_runner::CapturePolicy {
        max_size_bytes: args.daemon_runner_log_max_bytes,
        max_files: args.daemon_runner_log_max_files,
        ring_bytes: args.daemon_runner_log_ring_bytes,
    };
    let aciddog_binary = Arc::new(
        daemon_runner::resolve_aciddog_binary(args.aciddog_binary.clone()).with_context(|| {
            "resolving aciddog binary from --aciddog-binary/ACIDDOG_BINARY, sibling binary, or PATH"
        })?,
    );
    let daemon_runner =
        daemon_runner::DaemonRunner::new((*aciddog_binary).clone(), capture_policy.clone());

    let state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: args.cookie_secure,
        },
        args.data_dir.clone(),
        aciddog_binary,
        daemon_runner,
        capture_policy,
    );
    let router = routes::build_router(state.clone());

    tracing::info!("daemon-runner enabled");
    tokio::spawn(reconciler::run(state.clone()));

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
        "aciddog-manager listening"
    );
    axum::serve(listener, router).await.context("axum::serve")?;
    Ok(())
}
