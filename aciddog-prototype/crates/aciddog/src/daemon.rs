//! Headless orchestrator entry point.
//!
//! Runs the same agent / automaton / cron / memory / IPC stack that the
//! TUI runs (see `tui::boot::boot_orchestrator`) but with no terminal
//! attached: no raw mode, no alternate screen, no keyboard events, no
//! `terminal.draw`. Management happens entirely through the webservice
//! automaton(s) declared in the user's `aciddog.wcl`; the aciddog-manager
//! web portal talks to those endpoints.
//!
//! Graceful shutdown: SIGTERM or SIGINT triggers
//! [`App::begin_shutdown`], which sleeps idle agents immediately and
//! lets busy ones finish their current turn. Once
//! [`App::should_quit`] flips, the event loop returns. A second signal
//! forces an immediate exit.

use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use crate::config::AciddogConfig;
use crate::tui::boot::{boot_orchestrator, OrchestratorHandles};

/// Run the orchestrator as a daemon until a shutdown signal drains
/// it, or a second signal forces it out.
pub async fn run(config: AciddogConfig, config_path: std::path::PathBuf) -> Result<()> {
    info!(
        team = %config.team.id,
        agents = config.agents.len(),
        automatons = config.automatons.len(),
        "starting daemon",
    );

    let mut handles = boot_orchestrator(config, config_path).await?;
    info!("aciddog daemon ready; send SIGTERM for graceful shutdown");

    daemon_event_loop(&mut handles).await?;

    info!("daemon exited");
    Ok(())
}

async fn daemon_event_loop(handles: &mut OrchestratorHandles) -> Result<()> {
    #[cfg(unix)]
    let (mut sigterm, mut sigint, mut sighup) = {
        use tokio::signal::unix::{signal, SignalKind};
        (
            signal(SignalKind::terminate())?,
            signal(SignalKind::interrupt())?,
            signal(SignalKind::hangup())?,
        )
    };

    let mut shutdown_requested = false;

    // Drive the workflow-instance pruning the TUI's spinner tick used
    // to handle, so completed instances whose linger timer has elapsed
    // don't accumulate on a long-running daemon. 1 Hz is coarse enough
    // to be invisible in logs and fine enough to match the TUI's UX.
    let mut workflow_prune_tick = tokio::time::interval(Duration::from_secs(1));
    workflow_prune_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        let next_cron_at = handles.app.cron.next_due_at();
        let cron_invalidate = handles.app.cron_invalidate.clone();

        tokio::select! {
            Some((idx, ev)) = handles.ai_rx.recv() => {
                handles.app.handle_ai_event(idx, ev);
            }
            Some(ev) = handles.socket.events.recv() => {
                handles.app.handle_agent_socket_event(ev);
            }
            Some(comp_ev) = handles.compaction_rx.recv() => {
                handles.app.handle_compaction_event(comp_ev);
            }
            Some(mem_ev) = handles.memory_event_rx.recv() => {
                handles.app.on_memory_event(mem_ev);
            }
            Some(wf_ev) = handles.workflow_event_rx.recv() => {
                handles.app.on_workflow_event(wf_ev);
            }
            Some(usage_ev) = handles.usage_event_rx.recv() => {
                handles.app.on_usage_event(usage_ev);
            }

            _ = workflow_prune_tick.tick() => {
                let _ = handles.app.prune_completed_workflows();
            }

            _ = async move {
                match next_cron_at {
                    Some(at) => {
                        let now = chrono::Utc::now();
                        let dur = (at - now).to_std().unwrap_or(Duration::from_millis(0));
                        tokio::time::sleep(dur).await
                    }
                    None => std::future::pending::<()>().await,
                }
            } => {
                handles.app.tick_cron(chrono::Utc::now());
            }
            _ = cron_invalidate.notified() => {}

            _ = sigterm.recv() => {
                if shutdown_requested {
                    warn!("second SIGTERM — forcing exit");
                    std::process::exit(130);
                }
                info!("SIGTERM received — graceful shutdown");
                shutdown_requested = true;
                handles.app.begin_shutdown(None);
            }
            _ = sigint.recv() => {
                if shutdown_requested {
                    warn!("second SIGINT — forcing exit");
                    std::process::exit(130);
                }
                info!("SIGINT received — graceful shutdown (Ctrl-C again to force)");
                shutdown_requested = true;
                handles.app.begin_shutdown(None);
            }
            _ = sighup.recv() => {
                info!("SIGHUP received — reloading WCL config");
                match handles.app.reload() {
                    Ok(summary) => info!(%summary, "reload complete"),
                    Err(e) => warn!(error = %e, "reload failed; old config still active"),
                }
            }
        }

        handles.app.check_shutdown_progress();
        if handles.app.should_quit {
            break;
        }
    }

    Ok(())
}
