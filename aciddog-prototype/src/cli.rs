use std::path::PathBuf;

use clap::{Parser, Subcommand};
use uuid::Uuid;

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
    /// Shared task-board subcommands. Tasks can be added, assigned to
    /// any agent or worker, picked up via `task take`, updated, and
    /// annotated with notes that travel across handoffs.
    Task {
        #[command(subcommand)]
        cmd: TaskCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TaskCommand {
    /// Add a new task to the shared board.
    Add {
        /// Short task title.
        #[arg(long)]
        title: String,
        /// Longer description / acceptance criteria.
        #[arg(long)]
        description: Option<String>,
        /// Display id of the agent or worker to assign this task to.
        #[arg(long)]
        assignee: Option<String>,
        /// Existing task ids this task depends on. Repeat the flag to
        /// list multiple deps (e.g. `--blocked-by <uuid> --blocked-by
        /// <uuid>`). `task take` skips tasks with any open dep.
        #[arg(long = "blocked-by")]
        blocked_by: Vec<Uuid>,
    },
    /// List tasks, optionally filtered.
    List {
        /// Only show tasks assigned to this display id.
        #[arg(long)]
        assignee: Option<String>,
        /// Only show tasks in this status
        /// (open|assigned|in_progress|done|cancelled).
        #[arg(long)]
        status: Option<String>,
        /// Only show tasks assigned to the calling agent/worker.
        #[arg(long)]
        mine: bool,
        /// Only show tasks with no assignee.
        #[arg(long)]
        unassigned: bool,
    },
    /// Update fields on an existing task. Omitted flags leave the
    /// field unchanged. Supply `--note` to append a handoff note.
    Update {
        /// Task id returned by `task add` / `task list`.
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        /// Clear the assignee (mutually exclusive with `--assignee`).
        #[arg(long, conflicts_with = "assignee")]
        unassign: bool,
        /// Append a note to the task log. Recommended when
        /// reassigning.
        #[arg(long)]
        note: Option<String>,
        /// Add a dependency on an existing task. Repeatable.
        #[arg(long = "add-blocked-by")]
        add_blocked_by: Vec<Uuid>,
        /// Remove an existing dependency. Repeatable.
        #[arg(long = "remove-blocked-by")]
        remove_blocked_by: Vec<Uuid>,
    },
    /// Remove a task from the board.
    Remove {
        #[arg(long)]
        id: Uuid,
    },
    /// Show the full body of a task: title, description, status,
    /// assignee, timestamps, dependencies, and the complete note log.
    Show {
        #[arg(long)]
        id: Uuid,
    },
    /// Atomically claim the oldest unassigned task and move it to
    /// `in_progress` with the calling agent as assignee.
    Take,
    /// Append a note to a task.
    Note {
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        body: String,
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

    #[test]
    fn agent_args_parses_task_add_minimal() {
        let args =
            AgentArgs::try_parse_from(["aciddog", "task", "add", "--title", "fix bug"]).unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Add {
                        title,
                        description,
                        assignee,
                        blocked_by,
                    },
            } => {
                assert_eq!(title, "fix bug");
                assert!(description.is_none());
                assert!(assignee.is_none());
                assert!(blocked_by.is_empty());
            }
            other => panic!("expected Task::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_add_full() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "add",
            "--title",
            "fix bug",
            "--description",
            "details here",
            "--assignee",
            "tester",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Add {
                        title,
                        description,
                        assignee,
                        blocked_by,
                    },
            } => {
                assert_eq!(title, "fix bug");
                assert_eq!(description.as_deref(), Some("details here"));
                assert_eq!(assignee.as_deref(), Some("tester"));
                assert!(blocked_by.is_empty());
            }
            other => panic!("expected Task::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_add_with_blocked_by() {
        let dep1 = Uuid::new_v4();
        let dep2 = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "add",
            "--title",
            "B",
            "--blocked-by",
            &dep1.to_string(),
            "--blocked-by",
            &dep2.to_string(),
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd: TaskCommand::Add { blocked_by, .. },
            } => {
                assert_eq!(blocked_by, vec![dep1, dep2]);
            }
            other => panic!("expected Task::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_list_all_filters() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "list",
            "--assignee",
            "coder",
            "--status",
            "in_progress",
            "--mine",
            "--unassigned",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::List {
                        assignee,
                        status,
                        mine,
                        unassigned,
                    },
            } => {
                assert_eq!(assignee.as_deref(), Some("coder"));
                assert_eq!(status.as_deref(), Some("in_progress"));
                assert!(mine);
                assert!(unassigned);
            }
            other => panic!("expected Task::List, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_list_no_filters() {
        let args = AgentArgs::try_parse_from(["aciddog", "task", "list"]).unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::List {
                        assignee,
                        status,
                        mine,
                        unassigned,
                    },
            } => {
                assert!(assignee.is_none());
                assert!(status.is_none());
                assert!(!mine);
                assert!(!unassigned);
            }
            other => panic!("expected Task::List, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_update() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "update",
            "--id",
            &id.to_string(),
            "--status",
            "done",
            "--note",
            "finished",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Update {
                        id: parsed,
                        status,
                        note,
                        unassign,
                        add_blocked_by,
                        remove_blocked_by,
                        ..
                    },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(status.as_deref(), Some("done"));
                assert_eq!(note.as_deref(), Some("finished"));
                assert!(!unassign);
                assert!(add_blocked_by.is_empty());
                assert!(remove_blocked_by.is_empty());
            }
            other => panic!("expected Task::Update, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_update_dep_edits() {
        let id = Uuid::new_v4();
        let add = Uuid::new_v4();
        let rm = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "update",
            "--id",
            &id.to_string(),
            "--add-blocked-by",
            &add.to_string(),
            "--remove-blocked-by",
            &rm.to_string(),
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Update {
                        add_blocked_by,
                        remove_blocked_by,
                        ..
                    },
            } => {
                assert_eq!(add_blocked_by, vec![add]);
                assert_eq!(remove_blocked_by, vec![rm]);
            }
            other => panic!("expected Task::Update, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_task_update_unassign_conflicts_with_assignee() {
        let id = Uuid::new_v4();
        let err = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "update",
            "--id",
            &id.to_string(),
            "--assignee",
            "bob",
            "--unassign",
        ])
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("cannot be used") || msg.contains("conflict"),
            "expected a conflict error, got: {msg}"
        );
    }

    #[test]
    fn agent_args_parses_task_remove() {
        let id = Uuid::new_v4();
        let args =
            AgentArgs::try_parse_from(["aciddog", "task", "remove", "--id", &id.to_string()])
                .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd: TaskCommand::Remove { id: parsed },
            } => {
                assert_eq!(parsed, id);
            }
            other => panic!("expected Task::Remove, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_show() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from(["aciddog", "task", "show", "--id", &id.to_string()])
            .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd: TaskCommand::Show { id: parsed },
            } => {
                assert_eq!(parsed, id);
            }
            other => panic!("expected Task::Show, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_task_show_requires_id() {
        assert!(AgentArgs::try_parse_from(["aciddog", "task", "show"]).is_err());
    }

    #[test]
    fn agent_args_parses_task_take() {
        let args = AgentArgs::try_parse_from(["aciddog", "task", "take"]).unwrap();
        assert!(matches!(
            args.cmd,
            AgentCommand::Task {
                cmd: TaskCommand::Take
            }
        ));
    }

    #[test]
    fn agent_args_parses_task_note() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "note",
            "--id",
            &id.to_string(),
            "--body",
            "see commit abc123",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd: TaskCommand::Note { id: parsed, body },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(body, "see commit abc123");
            }
            other => panic!("expected Task::Note, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_task_remove_requires_id() {
        assert!(AgentArgs::try_parse_from(["aciddog", "task", "remove"]).is_err());
    }

    #[test]
    fn agent_args_task_add_requires_title() {
        assert!(AgentArgs::try_parse_from(["aciddog", "task", "add"]).is_err());
    }
}
