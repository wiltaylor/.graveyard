use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use aciddog::agent_ipc::{self, Request, RequestCmd};
use aciddog::cli::{
    AgentArgs, AgentCommand, Args, CommentCommand, CronCommand, MemoryCommand, SkillCommand,
    TaskCommand, TodoCommand, WorkerCommand,
};
use aciddog::config::load_config;
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
        AgentCommand::Send { to, body } => RequestCmd::SendMessage { to, body },
        AgentCommand::Worker {
            cmd: WorkerCommand::Create { template, task },
        } => RequestCmd::WorkerCreate { template, task },
        AgentCommand::Worker {
            cmd: WorkerCommand::Complete { body },
        } => RequestCmd::WorkerComplete { body },
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
