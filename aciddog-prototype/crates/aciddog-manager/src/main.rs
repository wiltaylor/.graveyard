use std::net::SocketAddr;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::config::{CliArgs, Command};
use aciddog_manager::model::session;
use aciddog_manager::{db, routes, seed, state::AppState};
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

    let state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: args.cookie_secure,
        },
    );
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
