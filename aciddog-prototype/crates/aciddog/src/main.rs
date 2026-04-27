use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use aciddog::agent_ipc::{self, HistorySelector, Request, RequestCmd};
use aciddog::cli::{
    self, AgentAdminCommand, AgentArgs, AgentCommand, Args, AuthCommand, AuthProvider,
    ChannelCommand, CommentCommand, CronCommand, MemoryCommand, RootCommand, ServerCommand,
    SkillCommand, TaskCommand, TodoCommand, WorkerCommand,
};
use aciddog::config::load_config;
use aciddog::daemon;
use aciddog::init;
use aciddog::openai_auth;
use aciddog::tui;

#[tokio::main]
async fn main() -> ExitCode {
    // Load variables from a local `.env` if present. Absence is fine;
    // we only ignore the not-found case so real parse errors still surface.
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(e) if e.not_found() => {}
        Err(e) => {
            eprintln!("aciddog: failed to load .env: {e}");
            return ExitCode::from(1);
        }
    }

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
        (None, None) => {
            let args = Args::parse();
            // Daemon mode intercepts before the generic auth dispatch
            // because it needs `args` for `--config` / `--log-level`,
            // and its tracing setup differs (dual-writer: file + stdout).
            if let Some(RootCommand::Daemon) = &args.command {
                return match run_daemon(args).await {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("aciddog: {e:#}");
                        ExitCode::from(1)
                    }
                };
            }
            // Dispatch other top-level subcommands (auth login/logout/…)
            // before touching the TUI or the config file. They run to
            // completion and exit.
            if let Some(command) = &args.command {
                return match run_root_command(command.clone()).await {
                    Ok(code) => code,
                    Err(e) => {
                        eprintln!("aciddog: {e:#}");
                        ExitCode::from(1)
                    }
                };
            }
            match run_tui(args).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("aciddog: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
    }
}

async fn run_root_command(cmd: RootCommand) -> Result<ExitCode> {
    match cmd {
        RootCommand::Auth { cmd } => run_auth_command(cmd).await,
        // Intercepted in `main()` before this call. The arm exists so
        // the match stays exhaustive.
        RootCommand::Daemon => {
            unreachable!("RootCommand::Daemon is intercepted by main() before reaching here")
        }
        RootCommand::Init { path, force } => init::run_init(path, force).await,
    }
}

async fn run_auth_command(cmd: AuthCommand) -> Result<ExitCode> {
    match cmd {
        AuthCommand::Login {
            provider: AuthProvider::Chatgpt,
            name,
        } => match openai_auth::login_chatgpt(&name).await {
            Ok(tokens) => {
                println!(
                    "Signed in. credential={name} account_id={} expires_at={}",
                    tokens.account_id, tokens.expires_at
                );
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("aciddog: login failed: {e}");
                Ok(ExitCode::from(1))
            }
        },
        AuthCommand::Logout { name } => match openai_auth::remove(&name) {
            Ok(path) => {
                println!("Removed credential `{name}` from {}", path.display());
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("aciddog: logout failed: {e}");
                Ok(ExitCode::from(1))
            }
        },
        AuthCommand::List => match openai_auth::list() {
            Ok(entries) => {
                if entries.is_empty() {
                    println!("(no stored credentials)");
                } else {
                    for e in entries {
                        println!(
                            "{name:<20}  provider={prov:<20}  account={acc}  expires={exp}",
                            name = e.name,
                            prov = e.provider,
                            acc = e.account_id,
                            exp = e.expires_at
                        );
                    }
                }
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("aciddog: list failed: {e}");
                Ok(ExitCode::from(1))
            }
        },
        AuthCommand::Status { name } => match name {
            Some(n) => match openai_auth::get_chatgpt(&n) {
                Ok(t) => {
                    println!(
                        "{n:<20}  provider=openai_chatgpt  account={}  expires={}",
                        t.account_id, t.expires_at
                    );
                    Ok(ExitCode::SUCCESS)
                }
                Err(e) => {
                    eprintln!("aciddog: {e}");
                    Ok(ExitCode::from(1))
                }
            },
            None => match openai_auth::list() {
                Ok(entries) => {
                    if entries.is_empty() {
                        println!("(no stored credentials)");
                    } else {
                        let (path, _) = openai_auth::resolve_load_path();
                        if let Some(p) = path {
                            println!("auth file: {}", p.display());
                        }
                        for e in entries {
                            println!(
                                "  {name:<18}  provider={prov}  account={acc}  expires={exp}",
                                name = e.name,
                                prov = e.provider,
                                acc = e.account_id,
                                exp = e.expires_at
                            );
                        }
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Err(e) => {
                    eprintln!("aciddog: {e}");
                    Ok(ExitCode::from(1))
                }
            },
        },
    }
}

async fn run_tui(args: Args) -> Result<()> {
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

    tui::run(config, args.config).await
}

async fn run_daemon(args: Args) -> Result<()> {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let file_appender = tracing_appender::rolling::never(".", "aciddog.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard: the daemon's lifetime is the process lifetime,
    // and we want the non-blocking writer to keep flushing until the
    // very end — including the final `info!("daemon exited")` line
    // and any `warn!` on a forced second-signal exit.
    Box::leak(Box::new(guard));

    let filter = EnvFilter::try_new(args.effective_log_directive())
        .unwrap_or_else(|_| EnvFilter::new("aciddog=info"));

    // Layered subscriber: one fmt layer to ./aciddog.log (matches
    // `run_tui` behavior so operators keep the on-disk log) and one
    // fmt layer to stdout so systemd / journald / docker logs see
    // live output. Both ANSI-free — journald stamps its own
    // timestamps and disk grepping works better without escape codes.
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stdout).with_ansi(false))
        .init();

    let config = load_config(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;

    daemon::run(config, args.config).await
}

async fn run_agent_mode(agent_id: String, socket_path: String) -> Result<ExitCode> {
    let args = AgentArgs::parse();
    let agent_id = Uuid::parse_str(&agent_id)
        .map_err(|e| anyhow!("invalid {}: {e}", agent_ipc::ENV_AGENT_ID))?;
    let socket_path = PathBuf::from(socket_path);

    let cmd = match args.cmd {
        AgentCommand::Test => RequestCmd::Test,
        AgentCommand::Send { to, body } => RequestCmd::SendMessage { to, body },
        AgentCommand::Worker {
            cmd:
                WorkerCommand::Create {
                    template,
                    task,
                    properties,
                },
        } => {
            let parsed = match properties {
                None => None,
                Some(s) => Some(
                    serde_json::from_str::<serde_json::Value>(&s)
                        .map_err(|e| anyhow!("invalid --properties JSON: {e}"))?,
                ),
            };
            RequestCmd::WorkerCreate {
                template,
                task,
                properties: parsed,
            }
        }
        AgentCommand::Worker {
            cmd: WorkerCommand::Complete { body },
        } => RequestCmd::WorkerComplete { body },
        AgentCommand::Worker {
            cmd: WorkerCommand::Archive { id },
        } => RequestCmd::WorkerArchive { worker: id },
        AgentCommand::Task {
            cmd:
                TaskCommand::Add {
                    title,
                    description,
                    assignee,
                    blocked_by,
                },
        } => RequestCmd::TaskAdd {
            title,
            description,
            assignee,
            blocked_by,
        },
        AgentCommand::Task {
            cmd:
                TaskCommand::List {
                    assignee,
                    status,
                    mine,
                    unassigned,
                },
        } => RequestCmd::TaskList {
            assignee,
            status,
            mine,
            unassigned,
        },
        AgentCommand::Task {
            cmd:
                TaskCommand::Update {
                    id,
                    title,
                    description,
                    status,
                    assignee,
                    unassign,
                    note,
                    add_blocked_by,
                    remove_blocked_by,
                },
        } => RequestCmd::TaskUpdate {
            id,
            title,
            description,
            status,
            assignee,
            unassign,
            note,
            add_blocked_by,
            remove_blocked_by,
        },
        AgentCommand::Task {
            cmd: TaskCommand::Remove { id },
        } => RequestCmd::TaskRemove { id },
        AgentCommand::Task {
            cmd: TaskCommand::Show { id },
        } => RequestCmd::TaskShow { id },
        AgentCommand::Task {
            cmd: TaskCommand::Take,
        } => RequestCmd::TaskTake,
        AgentCommand::Task {
            cmd: TaskCommand::Note { id, body },
        } => RequestCmd::TaskNote { id, body },
        AgentCommand::Task {
            cmd:
                TaskCommand::Todo {
                    cmd: TodoCommand::Add { task, body },
                },
        } => RequestCmd::TaskTodoAdd { task, body },
        AgentCommand::Task {
            cmd: TaskCommand::Todo {
                cmd: TodoCommand::List { task },
            },
        } => RequestCmd::TaskTodoList { task },
        AgentCommand::Task {
            cmd:
                TaskCommand::Todo {
                    cmd:
                        TodoCommand::Update {
                            task,
                            id,
                            status,
                            body,
                        },
                },
        } => RequestCmd::TaskTodoUpdate {
            task,
            id,
            status,
            body,
        },
        AgentCommand::Task {
            cmd:
                TaskCommand::Todo {
                    cmd: TodoCommand::Remove { task, id },
                },
        } => RequestCmd::TaskTodoRemove { task, id },
        AgentCommand::Task {
            cmd:
                TaskCommand::Comment {
                    cmd: CommentCommand::Add { task, body },
                },
        } => RequestCmd::TaskCommentAdd { task, body },
        AgentCommand::Task {
            cmd:
                TaskCommand::Comment {
                    cmd: CommentCommand::List { task },
                },
        } => RequestCmd::TaskCommentList { task },
        AgentCommand::Task {
            cmd:
                TaskCommand::Comment {
                    cmd: CommentCommand::Update { task, id, body },
                },
        } => RequestCmd::TaskCommentUpdate { task, id, body },
        AgentCommand::Task {
            cmd:
                TaskCommand::Comment {
                    cmd: CommentCommand::Remove { task, id },
                },
        } => RequestCmd::TaskCommentRemove { task, id },
        AgentCommand::Memory {
            cmd:
                MemoryCommand::Search {
                    bank,
                    query,
                    title_contains,
                    include_tag,
                    exclude_tag,
                    limit,
                },
        } => RequestCmd::MemorySearch {
            bank,
            query,
            title_contains,
            include_tags: include_tag,
            exclude_tags: exclude_tag,
            limit,
        },
        AgentCommand::Memory {
            cmd:
                MemoryCommand::Store {
                    bank,
                    body,
                    title,
                    tag,
                },
        } => RequestCmd::MemoryStore {
            bank,
            body,
            title,
            tags: tag,
        },
        AgentCommand::Memory {
            cmd:
                MemoryCommand::Link {
                    bank,
                    from,
                    to,
                    kind,
                },
        } => RequestCmd::MemoryLink {
            bank,
            from,
            to,
            kind,
        },
        AgentCommand::Memory {
            cmd: MemoryCommand::Unlink { bank, from, to },
        } => RequestCmd::MemoryUnlink { bank, from, to },
        AgentCommand::Memory {
            cmd:
                MemoryCommand::Update {
                    bank,
                    id,
                    body,
                    title,
                    tag,
                },
        } => RequestCmd::MemoryUpdate {
            bank,
            id,
            body,
            title,
            tags: tag,
        },
        AgentCommand::Memory {
            cmd: MemoryCommand::Remove { bank, id },
        } => RequestCmd::MemoryRemove { bank, id },
        AgentCommand::Memory {
            cmd: MemoryCommand::Show { bank, id },
        } => RequestCmd::MemoryShow { bank, id },
        AgentCommand::Memory {
            cmd:
                MemoryCommand::Links {
                    bank,
                    id,
                    direction,
                    include_tag,
                    exclude_tag,
                },
        } => RequestCmd::MemoryLinks {
            bank,
            id,
            direction,
            include_tags: include_tag,
            exclude_tags: exclude_tag,
        },
        AgentCommand::Info => RequestCmd::Info,
        AgentCommand::Cron {
            cmd:
                CronCommand::Add {
                    target,
                    kind,
                    payload,
                    schedule,
                    tz,
                    oneoff,
                    catchup_on_startup,
                    disabled,
                },
        } => RequestCmd::CronAdd {
            target,
            kind,
            payload,
            schedule,
            tz,
            oneoff,
            catchup_on_startup,
            disabled,
        },
        AgentCommand::Cron {
            cmd:
                CronCommand::List {
                    target,
                    include_disabled,
                    mine,
                },
        } => RequestCmd::CronList {
            target,
            include_disabled,
            mine,
        },
        AgentCommand::Cron {
            cmd: CronCommand::Show { id },
        } => RequestCmd::CronShow { id },
        AgentCommand::Cron {
            cmd:
                CronCommand::Update {
                    id,
                    target,
                    kind,
                    payload,
                    schedule,
                    tz,
                    oneoff,
                    catchup_on_startup,
                    enabled,
                },
        } => RequestCmd::CronUpdate {
            id,
            target,
            kind,
            payload,
            schedule,
            tz,
            oneoff,
            catchup_on_startup,
            enabled,
        },
        AgentCommand::Cron {
            cmd: CronCommand::Remove { id },
        } => RequestCmd::CronRemove { id },
        AgentCommand::Skill {
            cmd: SkillCommand::List,
        } => RequestCmd::SkillList,
        AgentCommand::Skill {
            cmd: SkillCommand::Search { query },
        } => RequestCmd::SkillSearch { query },
        AgentCommand::Skill {
            cmd: SkillCommand::Run { name },
        } => RequestCmd::SkillRun { name },
        AgentCommand::Channel {
            cmd: ChannelCommand::Send { id, body },
        } => RequestCmd::ChannelSend { id, body },
        AgentCommand::Channel {
            cmd: ChannelCommand::Create { id },
        } => RequestCmd::ChannelCreate { id },
        AgentCommand::Channel {
            cmd: ChannelCommand::Join { id },
        } => RequestCmd::ChannelJoin { id },
        AgentCommand::Channel {
            cmd: ChannelCommand::Add { id, member },
        } => RequestCmd::ChannelAdd { id, member },
        AgentCommand::Channel {
            cmd: ChannelCommand::Kick { id, member },
        } => RequestCmd::ChannelKick { id, member },
        AgentCommand::Channel {
            cmd: ChannelCommand::End { id },
        } => RequestCmd::ChannelEnd { id },
        AgentCommand::Workflow {
            cmd: cli::WorkflowCommand::Run { id, properties },
        } => {
            // Translate `--property k=v` into JSON. The orchestrator's
            // WorkflowSetTool::coerce handles type-level conversion.
            let props: std::collections::BTreeMap<String, serde_json::Value> = properties
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect();
            RequestCmd::WorkflowRun {
                workflow: id,
                properties: props,
            }
        }
        AgentCommand::Agent {
            cmd: AgentAdminCommand::List,
        } => RequestCmd::AgentList,
        AgentCommand::Agent {
            cmd: AgentAdminCommand::Status { id },
        } => RequestCmd::AgentStatus { agent: id },
        AgentCommand::Agent {
            cmd:
                AgentAdminCommand::History {
                    id,
                    all,
                    last,
                    last_message,
                    kind,
                },
        } => {
            let selector = match (all, last, last_message, kind) {
                (true, None, false, None) => HistorySelector::All,
                (false, Some(n), false, None) => HistorySelector::Last { n },
                (false, None, true, None) => HistorySelector::LastMessage,
                (false, None, false, Some(k)) => HistorySelector::Kind { kind: k },
                _ => {
                    return Err(anyhow!(
                        "agent history: supply exactly one of --all / --last / --last-message / --kind"
                    ));
                }
            };
            RequestCmd::AgentHistory {
                agent: id,
                selector,
            }
        }
        AgentCommand::Agent {
            cmd: AgentAdminCommand::Stop { id, reason },
        } => RequestCmd::AgentStop { agent: id, reason },
        AgentCommand::Agent {
            cmd: AgentAdminCommand::Sleep { id },
        } => RequestCmd::AgentSleep { agent: id },
        AgentCommand::Agent {
            cmd: AgentAdminCommand::Wake { id },
        } => RequestCmd::AgentWake { agent: id },
        AgentCommand::Agent {
            cmd: AgentAdminCommand::SendAsUser { to, body },
        } => RequestCmd::AgentSendAsUser { to, body },
        AgentCommand::Worker {
            cmd: WorkerCommand::List { parent },
        } => RequestCmd::WorkerList { parent },
        AgentCommand::Worker {
            cmd: WorkerCommand::ForceArchive { id },
        } => RequestCmd::WorkerForceArchive { worker: id },
        AgentCommand::Server {
            cmd: ServerCommand::Shutdown,
        } => RequestCmd::ServerShutdown,
        AgentCommand::Server {
            cmd: ServerCommand::Exit,
        } => RequestCmd::ServerExit,
        AgentCommand::Server {
            cmd: ServerCommand::Reload,
        } => RequestCmd::ServerReload,
    };

    let response = agent_ipc::client::send(&socket_path, &Request { agent_id, cmd })
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
