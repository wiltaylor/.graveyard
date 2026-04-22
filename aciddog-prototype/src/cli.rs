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
    /// Read and write semantic memory banks. Each bank is scoped by
    /// its declaration in the WCL config; an agent can only touch a
    /// bank that appears in its `memory_banks` allowlist.
    Memory {
        #[command(subcommand)]
        cmd: MemoryCommand,
    },
    /// Print the current local time, UTC time, and the orchestrator's
    /// timezone. Use this when computing a cron schedule rather than
    /// shelling out to `date`.
    Info,
    /// Schedule a payload to be delivered to a target agent on a
    /// recurring or one-off schedule.
    Cron {
        #[command(subcommand)]
        cmd: CronCommand,
    },
    /// Load a pre-announced skill's `SKILL.md` body. The orchestrator
    /// enforces the caller's skill allowlist and prepends a
    /// `{baseDir} = <abs-path>` header pointing at the skill's folder
    /// so the body can reference sibling files by that placeholder.
    Skill {
        #[command(subcommand)]
        cmd: SkillCommand,
    },
    /// Broadcast-style chat rooms. Any agent listed as a member of a
    /// channel in the WCL config may post there; every other member
    /// receives a `<ChannelMessage>` on their next turn at the lowest
    /// message priority. Channels never auto-notify — only send when
    /// you have something substantive to add.
    Channel {
        #[command(subcommand)]
        cmd: ChannelCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ChannelCommand {
    /// Broadcast a message to every other member of a channel you
    /// belong to. The orchestrator wraps the body as
    /// `<ChannelMessage channel="<id>" from="your-id">body</ChannelMessage>`
    /// on each recipient side. Sender does NOT receive their own echo.
    Send {
        /// Channel id (e.g. `planning`). Must appear in the config and
        /// have the calling agent listed in its `members`.
        #[arg(long)]
        id: String,
        /// Message body to broadcast.
        #[arg(long)]
        body: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum MemoryCommand {
    /// Search a memory bank. Combine `--query` (semantic), `--title-contains`
    /// (case-insensitive substring), and `--include-tag` / `--exclude-tag`
    /// (repeatable) freely; omit `--query` for a tag/title-only list
    /// ordered newest first.
    Search {
        /// Bank id as declared by a `memory_bank <id> { ... }` block.
        #[arg(long)]
        bank: String,
        /// Natural-language query text. Omit for a non-semantic list.
        #[arg(long)]
        query: Option<String>,
        /// Case-insensitive substring the entry title must contain.
        #[arg(long)]
        title_contains: Option<String>,
        /// Tag that every returned entry must carry. Repeatable;
        /// multiple flags intersect (entry must have all of them).
        #[arg(long = "include-tag")]
        include_tag: Vec<String>,
        /// Tag that disqualifies entries. Repeatable.
        #[arg(long = "exclude-tag")]
        exclude_tag: Vec<String>,
        /// Maximum number of hits to return (default 5).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Persist a new entry in a memory bank.
    Store {
        /// Bank id.
        #[arg(long)]
        bank: String,
        /// Entry body.
        #[arg(long)]
        body: String,
        /// Short title for the entry. Searchable via `--title-contains`.
        #[arg(long)]
        title: Option<String>,
        /// Tag to attach to the entry (repeatable). Tags are
        /// lower-cased and de-duplicated on store.
        #[arg(long = "tag")]
        tag: Vec<String>,
    },
    /// Create a directed link between two entries. Use `kind` to
    /// classify the relationship (e.g. "supersedes", "see_also").
    Link {
        #[arg(long)]
        bank: String,
        /// Source entry id (as returned by `memory store`).
        #[arg(long)]
        from: Uuid,
        /// Target entry id.
        #[arg(long)]
        to: Uuid,
        /// Optional free-text label describing the relationship.
        #[arg(long)]
        kind: Option<String>,
    },
    /// Remove a directed link between two entries.
    Unlink {
        #[arg(long)]
        bank: String,
        #[arg(long)]
        from: Uuid,
        #[arg(long)]
        to: Uuid,
    },
    /// Rewrite an existing entry's body, title, and tag set. The body
    /// is re-embedded so semantic search reflects the new text; the
    /// tag set is replaced entirely (omit `--tag` to drop all tags).
    Update {
        #[arg(long)]
        bank: String,
        /// Entry id as returned by `memory store` / `memory search`.
        #[arg(long)]
        id: Uuid,
        /// New body. Cannot be empty.
        #[arg(long)]
        body: String,
        /// New title. Pass an empty string to clear it.
        #[arg(long)]
        title: Option<String>,
        /// Tag to attach to the entry (repeatable). Replaces the
        /// current tag set.
        #[arg(long = "tag")]
        tag: Vec<String>,
    },
    /// Remove an entry. Cascades to its tags and every link where it
    /// appears as `from` or `to`.
    Remove {
        #[arg(long)]
        bank: String,
        #[arg(long)]
        id: Uuid,
    },
    /// Fetch a single entry by id. Prints `title`, tags, `created_at`,
    /// and the full body.
    Show {
        #[arg(long)]
        bank: String,
        #[arg(long)]
        id: Uuid,
    },
    /// List every link touching an entry. Each line is:
    /// `<-out|in-> <peer-uuid> [<kind>] <peer-body>`.
    Links {
        #[arg(long)]
        bank: String,
        /// Entry whose links you want to see.
        #[arg(long)]
        id: Uuid,
        /// Which direction to walk. One of `out`, `in`, `both`
        /// (default).
        #[arg(long)]
        direction: Option<String>,
        /// Tag the peer entry must carry (repeatable; intersection).
        #[arg(long = "include-tag")]
        include_tag: Vec<String>,
        /// Tag that disqualifies peer entries (repeatable).
        #[arg(long = "exclude-tag")]
        exclude_tag: Vec<String>,
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
    /// Manage the TODO checklist inside a task. Agents use this to
    /// break a task down into sub-items and track progress; the
    /// items show up in `task show` with glyphs for state.
    Todo {
        #[command(subcommand)]
        cmd: TodoCommand,
    },
    /// Manage agent-authored comments on a task. Comments are
    /// intentional messages left for the next agent to read; the
    /// system's event log lives under Notes (see `task note`).
    Comment {
        #[command(subcommand)]
        cmd: CommentCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TodoCommand {
    /// Append a TODO. Starts in the `pending` state.
    Add {
        #[arg(long)]
        task: Uuid,
        #[arg(long)]
        body: String,
    },
    /// List the TODOs on a task.
    List {
        #[arg(long)]
        task: Uuid,
    },
    /// Update a TODO's body, status, or both. Any omitted flag
    /// leaves that field alone.
    Update {
        #[arg(long)]
        task: Uuid,
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Remove a TODO.
    Remove {
        #[arg(long)]
        task: Uuid,
        #[arg(long)]
        id: Uuid,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum CommentCommand {
    /// Post a comment on a task. The author is the calling agent.
    Add {
        #[arg(long)]
        task: Uuid,
        #[arg(long)]
        body: String,
    },
    /// List the comments on a task.
    List {
        #[arg(long)]
        task: Uuid,
    },
    /// Edit a comment's body.
    Update {
        #[arg(long)]
        task: Uuid,
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        body: String,
    },
    /// Remove a comment.
    Remove {
        #[arg(long)]
        task: Uuid,
        #[arg(long)]
        id: Uuid,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum CronCommand {
    /// Schedule a new payload. `--kind message --payload "<text>"`
    /// sends a chat message; `--kind command --payload "/foo bar"`
    /// runs a slash command on the target as if the user typed it.
    /// `--schedule` is a 5-field POSIX cron expression for recurring
    /// jobs (default), or a `YYYY-MM-DDTHH:MM[:SS]` wall-clock
    /// instant when `--oneoff` is set. `--tz` is `utc` or `local`
    /// (default `local`, meaning the orchestrator's timezone).
    /// `--catchup-on-startup` only applies to `--oneoff` jobs whose
    /// `run_at` passed while aciddog was offline; without it those
    /// jobs are pruned on startup.
    Add {
        /// Config id of the agent to deliver the payload to.
        #[arg(long)]
        target: String,
        /// `message` or `command`.
        #[arg(long)]
        kind: String,
        /// The message body (for `message`) or the slash command (for
        /// `command`, with the leading `/`).
        #[arg(long)]
        payload: String,
        /// Cron expression (recurring) or `YYYY-MM-DDTHH:MM[:SS]`
        /// wall-clock time (one-off, with `--oneoff`).
        #[arg(long)]
        schedule: String,
        /// `utc` or `local`. Default `local` (the orchestrator's
        /// timezone). Use `aciddog info` to see the current local tz.
        #[arg(long)]
        tz: Option<String>,
        /// Treat `--schedule` as a one-off wall-clock datetime
        /// instead of a recurring cron expression.
        #[arg(long)]
        oneoff: bool,
        /// For one-off jobs only: if the scheduled instant has
        /// already passed by the time aciddog starts, fire it once
        /// on startup. Without this flag, missed one-off jobs are
        /// pruned during startup reconciliation.
        #[arg(long = "catchup-on-startup", requires = "oneoff")]
        catchup_on_startup: bool,
        /// Create the job in the disabled state (no `next_run` is
        /// computed; will not fire until enabled via `cron update`).
        #[arg(long)]
        disabled: bool,
    },
    /// List cron jobs, optionally filtered.
    List {
        /// Only show jobs whose target matches this id.
        #[arg(long)]
        target: Option<String>,
        /// Include disabled jobs in the output.
        #[arg(long = "include-disabled")]
        include_disabled: bool,
        /// Only show jobs created by the calling agent.
        #[arg(long)]
        mine: bool,
    },
    /// Show the full body of a cron job: target, payload, schedule,
    /// timezone, last/next run, and creator.
    Show {
        /// Job id from `cron add` / `cron list`.
        #[arg(long)]
        id: Uuid,
    },
    /// Update fields on an existing cron job. Omitted flags leave the
    /// field unchanged. Updating the schedule, timezone, or oneoff
    /// flag recomputes `next_run` from now.
    Update {
        /// Job id from `cron add` / `cron list`.
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        payload: Option<String>,
        #[arg(long)]
        schedule: Option<String>,
        #[arg(long)]
        tz: Option<String>,
        #[arg(long)]
        oneoff: Option<bool>,
        #[arg(long = "catchup-on-startup")]
        catchup_on_startup: Option<bool>,
        #[arg(long)]
        enabled: Option<bool>,
    },
    /// Remove a cron job by id.
    Remove {
        #[arg(long)]
        id: Uuid,
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

#[derive(Debug, Clone, Subcommand)]
pub enum SkillCommand {
    /// List every assigned skill as `id — description` lines. Output
    /// is scoped to the caller's allowlist.
    List,
    /// Filter assigned skills by case-insensitive substring match
    /// against id and description. Output matches the same shape as
    /// `skill list`.
    Search {
        /// Substring to match against skill id or description.
        #[arg(long)]
        query: String,
    },
    /// Print the instructions you need to follow when running a
    /// skill. Output is the skill's `SKILL.md` prefixed with
    /// `{baseDir} = <abs-path>` on its own line followed by a blank
    /// line — substitute that path wherever the instructions
    /// reference `{baseDir}`.
    Run {
        /// Skill id as declared in the caller's `skills = [...]` list
        /// and present on disk at `.aciddog/skills/<name>/SKILL.md`.
        #[arg(long)]
        name: String,
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

    #[test]
    fn agent_args_parses_task_todo_add() {
        let task_id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "todo",
            "add",
            "--task",
            &task_id.to_string(),
            "--body",
            "draft schema",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Todo {
                        cmd: TodoCommand::Add { task, body },
                    },
            } => {
                assert_eq!(task, task_id);
                assert_eq!(body, "draft schema");
            }
            other => panic!("expected Task::Todo::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_todo_update() {
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "todo",
            "update",
            "--task",
            &task_id.to_string(),
            "--id",
            &todo_id.to_string(),
            "--status",
            "in_progress",
        ])
        .unwrap();
        match args.cmd {
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
            } => {
                assert_eq!(task, task_id);
                assert_eq!(id, todo_id);
                assert_eq!(status.as_deref(), Some("in_progress"));
                assert!(body.is_none());
            }
            other => panic!("expected Task::Todo::Update, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_todo_list_and_remove() {
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        let list = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "todo",
            "list",
            "--task",
            &task_id.to_string(),
        ])
        .unwrap();
        assert!(matches!(
            list.cmd,
            AgentCommand::Task {
                cmd: TaskCommand::Todo {
                    cmd: TodoCommand::List { .. }
                }
            }
        ));
        let rm = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "todo",
            "remove",
            "--task",
            &task_id.to_string(),
            "--id",
            &todo_id.to_string(),
        ])
        .unwrap();
        assert!(matches!(
            rm.cmd,
            AgentCommand::Task {
                cmd: TaskCommand::Todo {
                    cmd: TodoCommand::Remove { .. }
                }
            }
        ));
    }

    #[test]
    fn agent_args_parses_task_comment_add() {
        let task_id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "comment",
            "add",
            "--task",
            &task_id.to_string(),
            "--body",
            "heads up",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Comment {
                        cmd: CommentCommand::Add { task, body },
                    },
            } => {
                assert_eq!(task, task_id);
                assert_eq!(body, "heads up");
            }
            other => panic!("expected Task::Comment::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_info_subcommand() {
        let args = AgentArgs::try_parse_from(["aciddog", "info"]).expect("parses");
        assert!(matches!(args.cmd, AgentCommand::Info));
    }

    #[test]
    fn agent_args_parses_cron_add_recurring_local_default() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "cron",
            "add",
            "--target",
            "reviewer",
            "--kind",
            "message",
            "--payload",
            "ping",
            "--schedule",
            "0 9 * * *",
        ])
        .expect("parses");
        match args.cmd {
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
            } => {
                assert_eq!(target, "reviewer");
                assert_eq!(kind, "message");
                assert_eq!(payload, "ping");
                assert_eq!(schedule, "0 9 * * *");
                assert!(tz.is_none());
                assert!(!oneoff);
                assert!(!catchup_on_startup);
                assert!(!disabled);
            }
            other => panic!("expected Cron::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_cron_add_with_tz_utc_explicit() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "cron",
            "add",
            "--target",
            "reviewer",
            "--kind",
            "message",
            "--payload",
            "ping",
            "--schedule",
            "0 9 * * *",
            "--tz",
            "utc",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Cron {
                cmd: CronCommand::Add { tz, .. },
            } => assert_eq!(tz.as_deref(), Some("utc")),
            other => panic!("expected Cron::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_cron_add_oneoff_with_catchup() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "cron",
            "add",
            "--target",
            "self",
            "--kind",
            "command",
            "--payload",
            "/version",
            "--schedule",
            "2026-04-22T14:35:00",
            "--oneoff",
            "--catchup-on-startup",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Cron {
                cmd:
                    CronCommand::Add {
                        oneoff,
                        catchup_on_startup,
                        ..
                    },
            } => {
                assert!(oneoff);
                assert!(catchup_on_startup);
            }
            other => panic!("expected Cron::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_cron_add_catchup_on_startup_requires_oneoff() {
        // `--catchup-on-startup` is gated on `--oneoff`; clap should
        // reject the combination without `--oneoff`.
        let err = AgentArgs::try_parse_from([
            "aciddog",
            "cron",
            "add",
            "--target",
            "reviewer",
            "--kind",
            "message",
            "--payload",
            "p",
            "--schedule",
            "0 9 * * *",
            "--catchup-on-startup",
        ])
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("required") || msg.contains("oneoff"),
            "expected requires-oneoff error: {msg}"
        );
    }

    #[test]
    fn agent_args_cron_add_requires_target_kind_payload_schedule() {
        assert!(AgentArgs::try_parse_from(["aciddog", "cron", "add"]).is_err());
        assert!(AgentArgs::try_parse_from([
            "aciddog", "cron", "add", "--target", "r", "--kind", "message", "--payload", "p"
        ])
        .is_err());
    }

    #[test]
    fn agent_args_parses_cron_list_with_filters() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "cron",
            "list",
            "--target",
            "reviewer",
            "--include-disabled",
            "--mine",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Cron {
                cmd:
                    CronCommand::List {
                        target,
                        include_disabled,
                        mine,
                    },
            } => {
                assert_eq!(target.as_deref(), Some("reviewer"));
                assert!(include_disabled);
                assert!(mine);
            }
            other => panic!("expected Cron::List, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_cron_show_remove() {
        let id = Uuid::new_v4();
        let show =
            AgentArgs::try_parse_from(["aciddog", "cron", "show", "--id", &id.to_string()])
                .unwrap();
        assert!(matches!(
            show.cmd,
            AgentCommand::Cron {
                cmd: CronCommand::Show { id: parsed }
            } if parsed == id
        ));
        let rm =
            AgentArgs::try_parse_from(["aciddog", "cron", "remove", "--id", &id.to_string()])
                .unwrap();
        assert!(matches!(
            rm.cmd,
            AgentCommand::Cron {
                cmd: CronCommand::Remove { id: parsed }
            } if parsed == id
        ));
    }

    #[test]
    fn agent_args_parses_cron_update_partial_fields() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "cron",
            "update",
            "--id",
            &id.to_string(),
            "--payload",
            "revised",
            "--enabled",
            "false",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Cron {
                cmd:
                    CronCommand::Update {
                        id: parsed,
                        payload,
                        enabled,
                        ..
                    },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(payload.as_deref(), Some("revised"));
                assert_eq!(enabled, Some(false));
            }
            other => panic!("expected Cron::Update, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_comment_update_list_remove() {
        let task_id = Uuid::new_v4();
        let c_id = Uuid::new_v4();
        let upd = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "comment",
            "update",
            "--task",
            &task_id.to_string(),
            "--id",
            &c_id.to_string(),
            "--body",
            "revised",
        ])
        .unwrap();
        match upd.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Comment {
                        cmd: CommentCommand::Update { task, id, body },
                    },
            } => {
                assert_eq!(task, task_id);
                assert_eq!(id, c_id);
                assert_eq!(body, "revised");
            }
            other => panic!("expected Task::Comment::Update, got {other:?}"),
        }
        let list = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "comment",
            "list",
            "--task",
            &task_id.to_string(),
        ])
        .unwrap();
        assert!(matches!(
            list.cmd,
            AgentCommand::Task {
                cmd: TaskCommand::Comment {
                    cmd: CommentCommand::List { .. }
                }
            }
        ));
        let rm = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "comment",
            "remove",
            "--task",
            &task_id.to_string(),
            "--id",
            &c_id.to_string(),
        ])
        .unwrap();
        assert!(matches!(
            rm.cmd,
            AgentCommand::Task {
                cmd: TaskCommand::Comment {
                    cmd: CommentCommand::Remove { .. }
                }
            }
        ));
    }
}
