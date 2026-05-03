//! Orchestrator boot sequence, shared by the TUI (`tui::run`) and the
//! headless daemon mode (`crate::daemon::run`).
//!
//! Extracts the pre-event-loop startup steps that both entry points
//! need: task/cron/channel-log/memory registry loading, agent IPC
//! socket bind, per-agent and per-automaton core spawn. Neither TTY
//! nor ratatui state is touched here, so the same sequence is valid
//! for a daemon with no terminal attached.
//!
//! Callers own the returned [`OrchestratorHandles`] for the lifetime
//! of the event loop; dropping it closes all channels and aborts the
//! socket accept loop.
//!
//! Nothing outside the `tui` module should call this directly — the
//! daemon crosses the visibility line deliberately via
//! `crate::tui::boot::boot_orchestrator`.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::agent_ipc;
use crate::ai::{AICore, AIEvent, AciddogCliCtx, ChannelPromptCtx, PromptParts};
use crate::config::{AciddogConfig, Agent, Automaton};
use crate::memory::MemoryRegistry;
use crate::tasks::TaskStore;

use super::app::{
    build_core, build_memory_block, build_team_block, make_agent_chat, make_automaton_chat,
    run_automaton_core, run_core, uses_tag_ipc, App,
};
use super::blocks::ChatBlock;
use super::compaction::CompactionEvent;
use super::cores_view::UsageEvent;
use super::memory_view::MemoryEvent;

/// Everything the event loop needs, wired up and ready to drive.
///
/// Ownership notes:
/// - `app` carries the mutable orchestrator state; event-loop
///   callbacks mutate it in place.
/// - `socket` is the bound agent-IPC listener. Keep the whole struct
///   live in the caller; the event loop drains `socket.events`
///   through a `&mut` borrow. Dropping the [`Bound`] aborts the
///   accept loop and unlinks the socket file.
///
/// [`Bound`]: agent_ipc::server::Bound
pub(crate) struct OrchestratorHandles {
    pub app: App,
    pub ai_rx: mpsc::Receiver<(usize, AIEvent)>,
    pub socket: agent_ipc::server::Bound,
    pub compaction_rx: mpsc::Receiver<CompactionEvent>,
    pub memory_event_rx: mpsc::UnboundedReceiver<MemoryEvent>,
    pub workflow_event_rx: mpsc::UnboundedReceiver<crate::workflow::WorkflowEvent>,
    pub usage_event_rx: mpsc::UnboundedReceiver<UsageEvent>,
}

/// Run the full orchestrator boot sequence against a loaded config.
///
/// On success the returned handles can be driven by either the TUI
/// event loop or the daemon event loop. Errors here are treated as
/// fatal startup failures by the caller — the socket bind or the
/// first `build_core` call is the most likely failure point.
pub(crate) async fn boot_orchestrator(
    config: AciddogConfig,
    config_path: std::path::PathBuf,
) -> Result<OrchestratorHandles> {
    let repos_base_dir = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("repos");
    if !config.repositories.is_empty() {
        std::fs::create_dir_all(&repos_base_dir).map_err(|e| {
            anyhow!(
                "failed to create repository base directory {}: {e}",
                repos_base_dir.display()
            )
        })?;
    }
    let mut repos: Vec<crate::repos::RepoState> = Vec::new();
    for decl in &config.repositories {
        match crate::repos::ensure_repo(decl, &repos_base_dir) {
            Ok(mut state) => {
                if let Err(e) = crate::repos::refresh_worktrees(&mut state) {
                    warn!(
                        repository = %state.id,
                        error = %e,
                        "failed to refresh repository worktrees at boot"
                    );
                }
                repos.push(state);
            }
            Err(e) => return Err(anyhow!("{e}")),
        }
    }

    let mut app = App::new(config);
    app.config_path = config_path;
    app.repos = repos;

    // Load the disk-backed task store from `.aciddog/tasks.json` in the
    // cwd so tasks survive restarts. On I/O/parse error, warn and fall
    // back to an ephemeral store so aciddog still boots.
    let tasks_path = std::path::PathBuf::from(".aciddog").join("tasks.json");
    match TaskStore::load_or_init(tasks_path.clone()) {
        Ok(store) => {
            info!(path = %tasks_path.display(), count = store.len(), "loaded task store");
            app.tasks = store;
        }
        Err(e) => {
            warn!(error = %e, path = %tasks_path.display(), "failed to load task store; using ephemeral store");
        }
    }

    // Disk-backed cron store. Same fallback policy as TaskStore: a
    // load failure logs a warning and we boot with an ephemeral store
    // so an unparseable cron.json doesn't bring everything down.
    let cron_path = std::path::PathBuf::from(".aciddog").join("cron.json");
    match crate::cron::CronStore::load_or_init(cron_path.clone()) {
        Ok(mut store) => {
            match store.reconcile_on_startup(chrono::Utc::now()) {
                Ok(report) => {
                    info!(
                        path = %cron_path.display(),
                        count = store.len(),
                        removed_one_offs = report.removed_one_offs.len(),
                        queued_one_offs = report.queued_one_offs.len(),
                        recomputed_recurring = report.recomputed_recurring.len(),
                        disabled_unreachable = report.disabled_unreachable.len(),
                        "loaded cron store",
                    );
                }
                Err(e) => {
                    warn!(error = %e, path = %cron_path.display(), "cron startup reconciliation failed");
                }
            }
            app.cron = store;
        }
        Err(e) => {
            warn!(error = %e, path = %cron_path.display(), "failed to load cron store; using ephemeral store");
        }
    }

    // Open append-only JSONL transcripts for every declared channel,
    // then tail the last ~500 lines into each `ChannelState` for
    // display-only replay. The live messages are NEVER forwarded to
    // any agent's `channel_tx` here — agents start fresh on each boot.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    for ch in app.channels.iter_mut() {
        // 1. One-shot migration of any pre-session flat
        //    `.aciddog/chat/<id>.jsonl` into the new
        //    `.aciddog/chat/<id>/<hash>.jsonl` layout.
        if let Err(e) = super::channel_log::migrate_legacy(&cwd, &ch.id) {
            warn!(channel = %ch.id, error = %e, "failed to migrate legacy channel log");
        }
        // 2. Pick the most recent session file if one exists; else
        //    derive a fresh hash from `Utc::now()`. Either way, we end
        //    up with a `(start_time, hash)` pair to record on the
        //    state and a path to tail.
        let (started, hash) = match super::channel_log::latest_session(&cwd, &ch.id) {
            Some((_, h)) => (chrono::Utc::now(), h),
            None => {
                let now = chrono::Utc::now();
                (now, super::channel_log::session_hash(&ch.id, now))
            }
        };
        match super::channel_log::ChannelLog::open_session(&cwd, &ch.id, &hash) {
            Ok(log) => {
                let log_path = log.path().to_path_buf();
                ch.session_started = started;
                ch.session_hash = hash;
                ch.log = Some(log);
                match super::channel_log::tail(&log_path, 500) {
                    Ok(lines) => {
                        info!(
                            channel = %ch.id,
                            session = %ch.session_hash,
                            loaded = lines.len(),
                            "loaded channel transcript",
                        );
                        for line in lines {
                            ch.blocks
                                .push(ChatBlock::channel_message(line.sender, line.body));
                        }
                    }
                    Err(e) => {
                        warn!(channel = %ch.id, error = %e, "failed to tail channel transcript");
                    }
                }
            }
            Err(e) => {
                warn!(channel = %ch.id, error = %e, "failed to open channel log");
            }
        }
    }

    // Note: per-chat JSONL logs are opened later by the spawn paths
    // (`spawn_agent_core` / `spawn_automaton_core` / worker create)
    // — they need the AgentInfo block to carry the assembled
    // first-turn prompt, which only exists after the AI core is
    // built.

    // Construct the semantic-memory registry rooted at `.aciddog/memory/`.
    // Banks open lazily on first use, so this is just a handle creation —
    // no disk or model download cost unless an agent actually invokes
    // `memory search` / `memory store`.
    let memory_root = std::path::PathBuf::from(".aciddog").join("memory");
    let memory = Arc::new(MemoryRegistry::new(memory_root.clone()));
    info!(
        path = %memory_root.display(),
        banks = app.config.memory_banks.len(),
        "memory registry ready"
    );
    app.memory = Some(memory);

    // Channel for background memory ops (search/store/update/delete/link)
    // initiated by the Memory view. The receiver half is drained in the
    // main event loop; sends are non-blocking so the UI stays responsive.
    let (memory_event_tx, memory_event_rx) = mpsc::unbounded_channel::<MemoryEvent>();
    app.memory_event_tx = Some(memory_event_tx);

    // Channel for replies from background usage-fetch tasks (one per
    // core with a resolved usage provider, kicked off when the Cores
    // view is opened or refreshed).
    let (usage_event_tx, usage_event_rx) = mpsc::unbounded_channel::<UsageEvent>();
    app.usage_event_tx = Some(usage_event_tx);

    // Bind the agent IPC socket for this team before spawning cores, so its
    // path can be injected into each claude child's environment.
    let team_id = app.config.team.id.clone();
    let socket = agent_ipc::server::bind(&team_id)
        .await
        .with_context(|| format!("binding agent IPC socket for team {team_id}"))?;
    info!(path = %socket.path.display(), "agent IPC socket bound");
    let socket_path_str = socket.path.display().to_string();
    app.socket_path = socket_path_str.clone();
    // Clone the socket-events sender onto the App so tag-mode cores
    // spawned at runtime (worker create / resurrect) can push directly
    // into the same handler pipeline used by socket-borne requests.
    app.socket_events_tx = Some(socket.events_tx());

    // Resolve our own binary path so agents can invoke aciddog without it
    // being on PATH (cargo run, dev installs, nix store, etc).
    let aciddog_binary = std::env::current_exe()
        .context("resolving current aciddog binary path for agent preamble")?;
    let aciddog_binary_str = aciddog_binary.to_string_lossy().into_owned();
    info!(binary = %aciddog_binary_str, "agent CLI binary");
    app.aciddog_binary = aciddog_binary_str.clone();

    // One core per agent, each with its own session, user channel, and
    // agent-to-agent channel. User channel is unbounded (the TUI key
    // handler must never block or drop); agent channel is bounded so
    // the socket side has backpressure.
    let (ai_tx, ai_rx) = mpsc::channel::<(usize, AIEvent)>(256);
    app.ai_tx = Some(ai_tx.clone());
    let (compaction_tx, compaction_rx) = mpsc::channel::<CompactionEvent>(16);
    app.compaction_tx = Some(compaction_tx);
    // Workflow event plumbing. The engine owns the tx side; the TUI
    // drains the rx end in `event_loop`. Worker / Conditional steps
    // execute inline inside the engine using a cloned `AciddogConfig`,
    // so no further plumbing from App is needed.
    let (wf_event_tx, workflow_event_rx) =
        mpsc::unbounded_channel::<crate::workflow::WorkflowEvent>();
    app.workflow_engine = Some(crate::workflow::WorkflowEngine::new(
        Arc::new(app.config.clone()),
        wf_event_tx,
    ));

    for idx in 0..app.config.agents.len() {
        let agent = app.config.agents[idx].clone();
        spawn_agent_core(&mut app, idx, &agent)?;
    }

    // Automatons share the agent list but run a lightweight
    // per-backend loop: script-backed ones spawn a subprocess per
    // event; webservice-backed ones host a long-lived HTTP/Unix
    // listener. Each occupies its own `AgentChat` (appended after the
    // agents in `App::new`) and registers its session id in
    // `app.agent_ids` so the existing IPC / mail / ACL paths treat it
    // as any other addressable fleet member.
    let automaton_offset = app.config.agents.len();
    for auto_idx in 0..app.config.automatons.len() {
        let idx = automaton_offset + auto_idx;
        let automaton = app.config.automatons[auto_idx].clone();
        spawn_automaton_core(&mut app, idx, &automaton);
    }

    match app.reapply_wcl_crons() {
        Ok(()) => {
            // WCL reapply inserts only jobs that still represent a
            // future fire; startup reconciliation then handles the
            // persisted imperative cron set.
            if let Err(e) = app.cron.reconcile_on_startup(chrono::Utc::now()) {
                warn!(error = %e, "cron startup reconciliation after WCL reapply failed");
            }
        }
        Err(e) => warn!(error = %e, "failed to reapply WCL cron blocks at boot"),
    }

    drop(ai_tx);
    // `app.ai_tx` retains a sender clone so runtime worker spawns can
    // route their stream into the same loop. That clone is dropped
    // either when a new worker's `run_core` captures it, or when the
    // event loop exits and `App` is dropped.

    Ok(OrchestratorHandles {
        app,
        ai_rx,
        socket,
        compaction_rx,
        memory_event_rx,
        workflow_event_rx,
        usage_event_rx,
    })
}

/// Wire IPC channels onto the chat slot at `idx` and spawn the agent's
/// core task. Called once per agent during boot; reused by `/reload`
/// when the agent's effective config changes (`ReloadAction::Reset`)
/// or when a brand-new agent is added (via [`add_agent_chat`]).
///
/// Preconditions: `app.chats[idx]` exists (with display_name set),
/// `app.ai_tx`, `app.aciddog_binary`, `app.socket_path`, and (for
/// tag-mode cores) `app.socket_events_tx` are populated.
pub(super) fn spawn_agent_core(app: &mut App, idx: usize, agent: &Agent) -> Result<()> {
    let ai_tx = app
        .ai_tx
        .clone()
        .ok_or_else(|| anyhow!("ai_tx not initialised"))?;

    let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
    let (agent_tx, agent_rx) = mpsc::channel::<String>(256);
    let (channel_tx, channel_rx) = mpsc::channel::<String>(256);
    // Capacity 1: a single pending `/stop` is enough to cancel the
    // current turn; subsequent /stop spam is silently dropped by
    // `try_send` until the first signal is consumed.
    let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
    app.chats[idx].user_tx = Some(user_tx);
    app.chats[idx].agent_tx = Some(agent_tx);
    app.chats[idx].channel_tx = Some(channel_tx);
    app.chats[idx].cancel_tx = Some(cancel_tx);

    let ai_core = app
        .config
        .effective_ai_core(agent.ai_core.as_deref())
        .ok_or_else(|| {
            anyhow!(
                "agent `{}` has no ai_core (no override and no team default_ai_core)",
                agent.id
            )
        })?
        .clone();
    let tools = app
        .config
        .effective_tools(agent.tools.as_deref(), Some(&ai_core));
    let tag_mode = uses_tag_ipc(&tools, &ai_core);

    let session_id = Uuid::new_v4();
    app.chats[idx].session_id = Some(session_id);
    app.chats[idx].ai_core_id = Some(ai_core.id.clone());
    app.agent_ids.insert(session_id, agent.id.clone());

    let all_ids: Vec<String> = app.config.all_addressable_ids();
    let contactable: Vec<&str> = match &agent.can_message {
        Some(list) => list.iter().map(String::as_str).collect(),
        None => all_ids
            .iter()
            .filter(|id| id.as_str() != agent.id.as_str())
            .map(String::as_str)
            .collect(),
    };
    let creatable: Vec<&str> = agent
        .can_create_workers
        .as_deref()
        .map(|list| list.iter().map(String::as_str).collect())
        .unwrap_or_default();

    let team_block = build_team_block(&app.config, &contactable, &creatable, true);
    let memory_block = build_memory_block(&app.config, &agent.id);
    let memory_banks_for_agent: Vec<String> = agent
        .memory_banks
        .as_deref()
        .map(|list| list.to_vec())
        .unwrap_or_default();
    let skills_for_agent = app.config.skills_for(&agent.id);
    let channels_for_agent: Vec<ChannelPromptCtx> = app
        .config
        .channels_for_agent(&agent.id)
        .iter()
        .map(|c| ChannelPromptCtx {
            id: c.id.clone(),
            description: c.description.clone(),
            other_members: c
                .members
                .iter()
                .filter(|m| m.as_str() != agent.id.as_str())
                .cloned()
                .collect(),
        })
        .collect();

    let parts = PromptParts {
        role: agent.role_description.clone(),
        team: Some(team_block),
        memory: memory_block,
        instructions: agent.instructions.clone(),
        aciddog_cli: Some(AciddogCliCtx {
            binary: app.aciddog_binary.clone(),
            contactable: contactable.iter().map(|s| s.to_string()).collect(),
            creatable: creatable.iter().map(|s| s.to_string()).collect(),
            memory_banks: memory_banks_for_agent,
            skills: skills_for_agent,
            channels: channels_for_agent,
            worker: None,
            supervisor: agent.supervisor,
            sysadmin: agent.sysadmin,
        }),
    };

    let env = vec![
        (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
        (
            agent_ipc::ENV_AGENT_SOCKET.to_string(),
            app.socket_path.clone(),
        ),
    ];

    let socket_tx_for_core = if tag_mode {
        Some(
            app.socket_events_tx
                .clone()
                .ok_or_else(|| anyhow!("tag-mode core but socket_events_tx not initialised"))?,
        )
    } else {
        None
    };

    // Agents never carry `properties` (those only make sense for
    // ephemeral workers spawned on demand), so `None` is always
    // correct here. If an agent's core is a remote-worker variant
    // that requires properties (Jules), build_core will return an
    // error.
    let core: Box<dyn AICore> = build_core(
        &ai_core,
        session_id,
        parts,
        tools,
        env,
        socket_tx_for_core,
        false,
        agent.model.as_deref(),
        None,
        None,
    )
    .map_err(|e| anyhow!("agent `{}` core build failed: {e}", agent.id))?;

    // Reflect the assembled first-turn prompt in the top AgentInfo
    // block so the user can expand it and see exactly what the LLM
    // receives.
    let assembled = core.first_turn_prompt();
    let system_prompt_text = core.system_prompt().map(str::to_string);
    if let Some(ChatBlock::AgentInfo { prompt, .. }) = app.chats[idx].blocks.first_mut() {
        *prompt = assembled;
    }

    // Open the JSONL transcript now that AgentInfo is finalised. The
    // log starts with the AgentInfo header (assembled prompt + tools
    // + session hash) followed by an optional `system_prompt` line
    // when the core has one configured — that way every session file
    // is self-contained from the first byte.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    app.chats[idx].open_session_log(&cwd, system_prompt_text.as_deref());

    tokio::spawn(run_core(
        idx, core, user_rx, agent_rx, channel_rx, cancel_rx, ai_tx,
    ));
    Ok(())
}

/// Wire IPC channels onto the automaton chat slot at `idx` and spawn
/// the appropriate backend driver task (script-per-event subprocess,
/// webservice listener, or remote SSE proxy). Reused by `/reload` when
/// an automaton is added or its config (or webhook list) changes.
pub(super) fn spawn_automaton_core(app: &mut App, idx: usize, automaton: &Automaton) {
    let ai_tx = app
        .ai_tx
        .clone()
        .expect("ai_tx must be initialised before spawn_automaton_core");
    let socket_events_tx = app
        .socket_events_tx
        .clone()
        .expect("socket_events_tx must be initialised before spawn_automaton_core");

    let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
    let (agent_tx, agent_rx) = mpsc::channel::<String>(256);
    let (channel_tx, channel_rx) = mpsc::channel::<String>(256);
    let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
    app.chats[idx].user_tx = Some(user_tx);
    app.chats[idx].agent_tx = Some(agent_tx);
    app.chats[idx].channel_tx = Some(channel_tx);
    app.chats[idx].cancel_tx = Some(cancel_tx);

    let session_id = Uuid::new_v4();
    app.chats[idx].session_id = Some(session_id);
    app.agent_ids.insert(session_id, automaton.id.clone());

    // Open the JSONL transcript and write the AgentInfo header.
    // Automatons have no AI core, hence no `system_prompt` line.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    app.chats[idx].open_session_log(&cwd, None);

    match &automaton.backend {
        crate::config::AutomatonBackend::Script { .. } => {
            tokio::spawn(run_automaton_core(
                idx,
                automaton.clone(),
                None,
                session_id,
                app.socket_path.clone(),
                user_rx,
                agent_rx,
                channel_rx,
                cancel_rx,
                ai_tx,
            ));
        }
        crate::config::AutomatonBackend::Webservice { .. } => {
            let webhooks: Vec<crate::config::Webhook> = app
                .config
                .webhooks
                .iter()
                .filter(|w| w.mount == automaton.id)
                .cloned()
                .collect();
            let chat_block_tx = app.chat_block_tx.clone();
            let channel_message_tx = app.channel_message_tx.clone();
            let workflow_event_tx = app.workflow_event_tx.clone();
            let cron_event_tx = app.cron_event_tx.clone();
            let agent_status_tx = app.agent_status_tx.clone();
            tokio::spawn(crate::webservice::run_webservice_core(
                idx,
                automaton.clone(),
                session_id,
                webhooks,
                user_rx,
                agent_rx,
                channel_rx,
                cancel_rx,
                ai_tx,
                socket_events_tx,
                chat_block_tx,
                channel_message_tx,
                workflow_event_tx,
                cron_event_tx,
                agent_status_tx,
            ));
        }
        crate::config::AutomatonBackend::Remote { .. } => {
            tokio::spawn(crate::remote_automaton::run_remote_core(
                idx,
                automaton.clone(),
                session_id,
                user_rx,
                agent_rx,
                channel_rx,
                cancel_rx,
                ai_tx,
                socket_events_tx,
            ));
        }
    }
}

/// Append a fresh `AgentChat` for `agent`, attach the chat-block
/// broadcast sender, then spawn its core. Used by `/reload` when an
/// agent appears in the new config that wasn't in the old one.
pub(super) fn add_agent_chat(app: &mut App, agent: &Agent) -> Result<usize> {
    let mut chat = make_agent_chat(&app.config, agent);
    chat.chat_block_tx = Some(app.chat_block_tx.clone());
    chat.agent_status_tx = Some(app.agent_status_tx.clone());
    let idx = app.chats.len();
    app.chats.push(chat);
    // Log opening + header seeding happens inside `spawn_agent_core`
    // once the AI core has been built and the AgentInfo prompt is
    // finalised.
    spawn_agent_core(app, idx, agent)?;
    Ok(idx)
}

/// Append a fresh `AgentChat` for `automaton`, attach the chat-block
/// broadcast sender, then spawn its backend driver. Used by `/reload`
/// when an automaton appears in the new config that wasn't in the old
/// one (or replaces a removed-and-respawned slot).
pub(super) fn add_automaton_chat(app: &mut App, automaton: &Automaton) -> usize {
    let mut chat = make_automaton_chat(automaton);
    chat.chat_block_tx = Some(app.chat_block_tx.clone());
    chat.agent_status_tx = Some(app.agent_status_tx.clone());
    let idx = app.chats.len();
    app.chats.push(chat);
    // Log opening + header seeding happens inside
    // `spawn_automaton_core` once the chat slot is fully wired up.
    spawn_automaton_core(app, idx, automaton);
    idx
}
