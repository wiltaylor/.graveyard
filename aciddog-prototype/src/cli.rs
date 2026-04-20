use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Parser)]
#[command(name = "aciddog", version, about = "Agentic orchestrator TUI")]
pub struct Args {
    /// Path to the aciddog.wcl config file.
    #[arg(short, long, default_value = "aciddog.wcl")]
    pub config: PathBuf,

    /// Log level filter (e.g. `info`, `aciddog=debug`). Overrides `RUST_LOG`.
    #[arg(long)]
    pub log_level: Option<String>,
}

impl Args {
    pub fn effective_log_directive(&self) -> String {
        self.log_level
            .clone()
            .unwrap_or_else(|| std::env::var("RUST_LOG").unwrap_or_else(|_| "aciddog=info".into()))
    }
}

/// CLI surface exposed to AI agents running under an aciddog session.
///
/// Only parsed when `ACIDDOG_AGENT_ID` is set in the environment. Subcommands
/// here are callable by the agent via the standard shell and are routed over
/// the agent IPC socket to the parent aciddog process.
#[derive(Debug, Clone, Parser)]
#[command(name = "aciddog", version, about = "Agent-mode aciddog CLI")]
pub struct AgentArgs {
    #[command(subcommand)]
    pub cmd: AgentCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AgentCommand {
    /// Verify connectivity to the agent orchestrator by asking it to post a
    /// system message into this agent's chat.
    Test,
    /// Send a chat message to another agent on the team.
    Send {
        /// Config id of the recipient agent (e.g. `coder`).
        #[arg(long)]
        to: String,
        /// Message body to deliver. The orchestrator wraps this as
        /// `<Message from="your-id">body</Message>` on the recipient side.
        #[arg(long)]
        body: String,
    },
    /// Worker management subcommands. Agents with a non-empty
    /// `can_create_workers` allowlist in the config can spawn ephemeral
    /// workers. Workers themselves use `worker complete` to deliver their
    /// final report and hibernate.
    Worker {
        #[command(subcommand)]
        cmd: WorkerCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum WorkerCommand {
    /// Spawn a new worker from a declared worker template.
    Create {
        /// Worker template id as declared by a `worker <id> { ... }`
        /// block in the aciddog config.
        #[arg(long)]
        template: String,
        /// Initial task for the worker — becomes its first user prompt
        /// after the auto-injected worker preamble.
        #[arg(long)]
        task: String,
    },
    /// Deliver a final report to the parent and hibernate this worker.
    /// Only valid when invoked from inside a running worker.
    Complete {
        /// Final message body to deliver to the parent.
        #[arg(long)]
        body: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_args_parses_test_subcommand() {
        let args = AgentArgs::try_parse_from(["aciddog", "test"]).expect("parses");
        assert!(matches!(args.cmd, AgentCommand::Test));
    }

    #[test]
    fn agent_args_parses_send_subcommand() {
        let args =
            AgentArgs::try_parse_from(["aciddog", "send", "--to", "coder", "--body", "hi there"])
                .expect("parses");
        match args.cmd {
            AgentCommand::Send { to, body } => {
                assert_eq!(to, "coder");
                assert_eq!(body, "hi there");
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_send_requires_to_and_body() {
        assert!(AgentArgs::try_parse_from(["aciddog", "send"]).is_err());
        assert!(AgentArgs::try_parse_from(["aciddog", "send", "--to", "coder"]).is_err());
        assert!(AgentArgs::try_parse_from(["aciddog", "send", "--body", "hi"]).is_err());
    }

    #[test]
    fn agent_args_rejects_unknown_subcommand() {
        let err = AgentArgs::try_parse_from(["aciddog", "nope"]).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("subcommand")
                || err.to_string().contains("nope")
        );
    }

    #[test]
    fn agent_args_requires_subcommand() {
        let err = AgentArgs::try_parse_from(["aciddog"]).unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn agent_args_parses_worker_create() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "worker",
            "create",
            "--template",
            "researcher",
            "--task",
            "find FOO",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Worker {
                cmd: WorkerCommand::Create { template, task },
            } => {
                assert_eq!(template, "researcher");
                assert_eq!(task, "find FOO");
            }
            other => panic!("expected Worker::Create, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_worker_complete() {
        let args =
            AgentArgs::try_parse_from(["aciddog", "worker", "complete", "--body", "all done"])
                .expect("parses");
        match args.cmd {
            AgentCommand::Worker {
                cmd: WorkerCommand::Complete { body },
            } => {
                assert_eq!(body, "all done");
            }
            other => panic!("expected Worker::Complete, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_worker_create_requires_template_and_task() {
        assert!(AgentArgs::try_parse_from(["aciddog", "worker", "create"]).is_err());
        assert!(
            AgentArgs::try_parse_from(["aciddog", "worker", "create", "--template", "r"]).is_err()
        );
        assert!(AgentArgs::try_parse_from(["aciddog", "worker", "create", "--task", "t"]).is_err());
    }
}
