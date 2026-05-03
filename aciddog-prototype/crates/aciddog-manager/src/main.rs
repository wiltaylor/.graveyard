use std::net::SocketAddr;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::config::{CliArgs, Command};
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

    let _aciddog_binary = args.aciddog_binary.clone();
    let mut state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: args.cookie_secure,
        },
        args.data_dir.clone(),
        args.enable_daemon_runner,
    );
    // TODO(33fc8b7b): construct DaemonRunner here when
    // is_daemon_runner_enabled is true; pass args.aciddog_binary to
    // resolve_aciddog_binary().
    state.daemon_runner = None;
    let router = routes::build_router(state.clone());

    if state.is_daemon_runner_enabled {
        tracing::info!("daemon-runner enabled");
        if state.daemon_runner.is_some() {
            tokio::spawn(reconciler::run(state.clone()));
        } else {
            tracing::warn!(
                "daemon-runner gate is on but state.daemon_runner is None; \
                 reconciler not spawned"
            );
        }
    }

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
