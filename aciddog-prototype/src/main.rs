use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use aciddog::agent_ipc::{self, Request, RequestCmd};
use aciddog::cli::{AgentArgs, AgentCommand, Args};
use aciddog::config::load_config;
use aciddog::tui;

#[tokio::main]
async fn main() -> ExitCode {
    // Agent-mode dispatch: the parent aciddog process sets ACIDDOG_AGENT_ID
    // and ACIDDOG_AGENT_SOCKET on the claude child, and that child (and any
    // subprocess it spawns) inherits them. If either is set, run as the
    // agent-mode IPC client instead of the TUI.
    let id_env = std::env::var(agent_ipc::ENV_AGENT_ID).ok();
    let sock_env = std::env::var(agent_ipc::ENV_AGENT_SOCKET).ok();

    match (id_env, sock_env) {
        (Some(id), Some(sock)) => match run_agent_mode(id, sock).await {
            Ok(code) => code,
            Err(e) => {
                eprintln!("aciddog: {e:#}");
                ExitCode::from(1)
            }
        },
        (Some(_), None) | (None, Some(_)) => {
            eprintln!(
                "aciddog: {} and {} must either both be set or both be unset",
                agent_ipc::ENV_AGENT_ID,
                agent_ipc::ENV_AGENT_SOCKET
            );
            ExitCode::from(2)
        }
        (None, None) => match run_tui().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("aciddog: {e:#}");
                ExitCode::from(1)
            }
        },
    }
}

async fn run_tui() -> Result<()> {
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

async fn run_agent_mode(agent_id: String, socket_path: String) -> Result<ExitCode> {
    let args = AgentArgs::parse();
    let agent_id = Uuid::parse_str(&agent_id)
        .map_err(|e| anyhow!("invalid {}: {e}", agent_ipc::ENV_AGENT_ID))?;
    let socket_path = PathBuf::from(socket_path);

    let cmd = match args.cmd {
        AgentCommand::Test => RequestCmd::Test,
    };

    let response = agent_ipc::client::send(
        &socket_path,
        &Request {
            agent_id,
            cmd,
        },
    )
    .await
    .context("agent IPC request failed")?;

    if response.ok {
        if let Some(msg) = response.message {
            println!("{msg}");
        }
        Ok(ExitCode::SUCCESS)
    } else {
        let err = response.error.unwrap_or_else(|| "unknown error".into());
        eprintln!("aciddog: {err}");
        Ok(ExitCode::from(1))
    }
}
