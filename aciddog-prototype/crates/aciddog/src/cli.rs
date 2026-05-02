use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use uuid::Uuid;

#[derive(Debug, Clone, Parser)]
#[command(name = "aciddog", version, about = "Agentic orchestrator TUI")]
pub struct Args {
    /// Path to the aciddog.wcl config file.
    #[arg(short, long, default_value = "aciddog.wcl", global = true)]
    pub config: PathBuf,

    /// Log level filter (e.g. `info`, `aciddog=debug`). Overrides `RUST_LOG`.
    #[arg(long, global = true)]
    pub log_level: Option<String>,

    /// Optional subcommand. When absent, aciddog launches the TUI.
    /// Subcommands like `auth login` run to completion and exit without
    /// opening the TUI.
    #[command(subcommand)]
    pub command: Option<RootCommand>,
}

/// Top-level subcommands that run outside the TUI (OAuth login flows,
/// credential management, future `config` / `doctor` helpers). Agent
/// subcommands live in `AgentArgs`, which is only parsed when the
/// `ACIDDOG_AGENT_ID` environment variable is set.
#[derive(Debug, Clone, Subcommand)]
pub enum RootCommand {
    /// Manage OAuth credentials for providers that don't use API
    /// keys. Currently: `chatgpt` (OpenAI ChatGPT subscription).
    Auth {
        #[command(subcommand)]
        cmd: AuthCommand,
    },
    /// Run the orchestrator without a terminal UI. All agents, cron
    /// jobs, webservice automatons, and the agent IPC socket come up
    /// exactly as in TUI mode — drive the instance via the
    /// `:webservice` automaton declared in your `aciddog.wcl` (the
    /// aciddog-manager web portal talks to that endpoint). SIGTERM /
    /// SIGINT trigger a graceful shutdown; a second signal forces an
    /// immediate exit.
    ///
    /// aciddog reads `aciddog.wcl`, `.env`, and `.aciddog/*` relative
    /// to the current working directory. Set `WorkingDirectory=` in
    /// your systemd unit, or `cd` before launching.
    Daemon,
    /// Scaffold a new aciddog team into PATH (default: current
    /// directory). Drops in a skeleton `aciddog.wcl` with a single
    /// supervisor+sysadmin agent and a copy of the bundled `aciddog`
    /// reference skill at `.aciddog/skills/aciddog/`. Refuses to
    /// overwrite an existing `aciddog.wcl` unless `--force` is given.
    Init {
        /// Target directory. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite existing files in the target.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthCommand {
    /// Open a browser to complete OAuth for the given provider and
    /// persist the tokens under `--name`. The same name is referenced
    /// by `ai_core ... { credential = "<name>" }` blocks in
    /// `aciddog.wcl`.
    Login {
        /// Provider to authenticate (currently only `chatgpt`).
        #[arg(value_enum)]
        provider: AuthProvider,
        /// Credential name to save the tokens under (e.g. `personal`,
        /// `work`). Must match `[a-z0-9_-]+`, length 1..=64.
        #[arg(long)]
        name: String,
    },
    /// Remove a stored credential.
    Logout {
        #[arg(long)]
        name: String,
    },
    /// List every stored credential with provider + expiry.
    List,
    /// Show the stored credential's provider, account id, and expiry
    /// without revealing tokens. `--name` filters to a single entry.
    Status {
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AuthProvider {
    /// OpenAI ChatGPT subscription (`chatgpt.com/backend-api/codex`).
    Chatgpt,
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
    /// Workflow management subcommands. Agents with a non-empty
    /// `can_run_workflows` allowlist in the config can trigger a new
    /// workflow instance and monitor its progress from the TUI.
    Workflow {
        #[command(subcommand)]
        cmd: WorkflowCommand,
    },
    /// Supervisor-only: observe and manage other agents. Requires
    /// `supervisor = true` on the calling agent's config; rejected
    /// with a permission error otherwise.
    Agent {
        #[command(subcommand)]
        cmd: AgentAdminCommand,
    },
    /// Sysadmin-only: orchestrator lifecycle (shutdown, exit).
    /// Requires `sysadmin = true` on the calling agent's config.
    Server {
        #[command(subcommand)]
        cmd: ServerCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum WorkflowCommand {
    /// Start a new instance of a declared workflow. Properties are
    /// supplied as `--property key=value` pairs (repeat for each). The
    /// value is forwarded as a JSON string; the orchestrator coerces
    /// it against the property's declared kind.
    Run {
        /// Id of the workflow to trigger (must match a `workflow
        /// <id> { ... }` block in the WCL config).
        #[arg(long)]
        id: String,
        /// `key=value` overrides. The value is treated as a bare
        /// string; the orchestrator coerces to the declared property
        /// kind (number/select/multiSelect). For multiSelect, supply
        /// a comma-separated list (`--property suites=unit,integ`).
        #[arg(long = "property", value_parser = parse_key_value)]
        properties: Vec<(String, String)>,
    },
}

fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected KEY=VALUE, got `{s}`"))?;
    Ok((k.to_string(), v.to_string()))
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
    /// Create an ephemeral meeting channel. You are auto-joined so
    /// you can `channel send --id <id>` to your own meeting
    /// immediately. The channel dies when `channel end`-ed or when
    /// aciddog exits — it is never written back to the config file.
    Create {
        /// New channel id. Must not collide with an existing channel;
        /// identifier rules: letters / digits / `_` / `-`, starting
        /// with a letter or `_`, ≤ 64 chars.
        #[arg(long)]
        id: String,
    },
    /// Add yourself to a channel (ephemeral or WCL-declared).
    /// Runtime-only — WCL membership is restored on restart.
    Join {
        /// Channel id to join.
        #[arg(long)]
        id: String,
    },
    /// Add another agent to a channel. You must be a member of `<id>`
    /// yourself. Target must be a declared `agent` (workers cannot be
    /// channel members). Runtime-only — not persisted.
    Add {
        /// Channel id.
        #[arg(long)]
        id: String,
        /// Agent id (not a worker template) to add.
        #[arg(long)]
        member: String,
    },
    /// Remove a member from a channel. You must be a member of `<id>`
    /// yourself. Works on ephemeral and WCL-declared channels
    /// (runtime-only — WCL state restored on restart).
    Kick {
        /// Channel id.
        #[arg(long)]
        id: String,
        /// Agent id to remove.
        #[arg(long)]
        member: String,
    },
    /// Destroy an ephemeral meeting channel. You must be a member.
    /// Rejects WCL-declared channels.
    End {
        /// Channel id to end.
        #[arg(long)]
        id: String,
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
        /// Initial status for the task. Currently only `planning` is
        /// settable here — that creates the task as a user-approval
        /// plan (see `--planned-assignee`). Omit for the default
        /// behaviour (auto-bumps to `assigned` if `--assignee` is
        /// supplied, otherwise `open`).
        #[arg(long)]
        status: Option<String>,
        /// Pre-stored handoff target for `--status planning` tasks.
        /// On user approval the task's assignee flips to this value
        /// and the status becomes `approved`. Required (and only
        /// meaningful) when `--status planning` is supplied.
        #[arg(long = "planned-assignee")]
        planned_assignee: Option<String>,
        /// Bind this task to a filesystem path used to set spawned
        /// workers' cwd. Must be an existing directory inside a git
        /// repo; validated at task-add time.
        #[arg(long)]
        worktree: Option<String>,
    },
    /// List tasks, optionally filtered.
    List {
        /// Only show tasks assigned to this display id.
        #[arg(long)]
        assignee: Option<String>,
        /// Only show tasks in this status
        /// (planning|open|assigned|approved|in_progress|review|done|cancelled).
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
        /// New status
        /// (planning|open|assigned|approved|in_progress|review|done|cancelled).
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        /// Clear the assignee (mutually exclusive with `--assignee`).
        #[arg(long, conflicts_with = "assignee")]
        unassign: bool,
        /// Revise the pre-stored handoff target on a `planning` task.
        /// Use this when the user has commented on the plan and the
        /// agent wants to route to a different agent / group on
        /// approval.
        #[arg(long = "planned-assignee")]
        planned_assignee: Option<String>,
        /// Bind this task to a filesystem path used to set spawned
        /// workers' cwd.
        #[arg(long)]
        worktree: Option<String>,
        /// Clear the planned assignee (mutually exclusive with
        /// `--planned-assignee`). Only meaningful on planning tasks.
        #[arg(long = "clear-planned-assignee", conflicts_with = "planned_assignee")]
        clear_planned_assignee: bool,
        /// Clear the worktree path (mutually exclusive with
        /// `--worktree`).
        #[arg(long = "clear-worktree", conflicts_with = "worktree")]
        clear_worktree: bool,
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
    /// Submit an in-progress task for reviewer sign-off.
    Review {
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Approve a task assigned to you for review.
    ReviewApprove {
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reject a task assigned to you for review and send it back to
    /// the implementer.
    ReviewReject {
        #[arg(long)]
        id: Uuid,
        #[arg(long)]
        reason: String,
    },
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
        /// Working directory for the spawned worker process. Highest
        /// precedence: overrides the task.worktree of the assigned task
        /// AND any per-template default. The path must exist and be a
        /// directory at spawn time; otherwise creation fails loudly.
        #[arg(long)]
        cwd: Option<String>,
        /// Optional JSON object of per-spawn properties, consumed by
        /// remote-worker cores that need extra context. Example for
        /// a Jules worker:
        /// `--properties '{"repo":"owner/repo","branch":"main"}'`.
        /// Ignored by local cores.
        #[arg(long)]
        properties: Option<String>,
    },
    /// Deliver a final report to the parent and hibernate this worker.
    /// Only valid when invoked from inside a running worker.
    Complete {
        /// Final message body to deliver to the parent.
        #[arg(long)]
        body: String,
    },
    /// Close out a worker you spawned. Drops its IPC channels,
    /// hibernates it, and — for remote-worker cores — tears down the
    /// backend session (e.g. DELETE the Jules session).
    Archive {
        /// Worker display id as reported by `worker create`.
        #[arg(long)]
        id: String,
    },
    /// Supervisor-only: list every running worker, optionally scoped
    /// to a single parent agent.
    List {
        /// Filter to workers spawned by this parent agent.
        #[arg(long)]
        parent: Option<String>,
    },
    /// Supervisor-only: archive any worker by display id, bypassing
    /// the usual parent-ownership check.
    ForceArchive {
        /// Worker display id.
        #[arg(long)]
        id: String,
    },
}

/// Supervisor-only observability + management of other agents.
#[derive(Debug, Clone, Subcommand)]
pub enum AgentAdminCommand {
    /// List every agent, worker, and automaton with status, kind, and
    /// ai_core. Tab-separated for easy parsing.
    List,
    /// Status snapshot for one agent.
    Status {
        /// Agent display id.
        #[arg(long)]
        id: String,
    },
    /// Return a slice of a target agent's chat history. Exactly one
    /// selector flag must be supplied.
    History {
        /// Agent display id.
        #[arg(long)]
        id: String,
        /// Return the whole transcript.
        #[arg(long, group = "history_selector")]
        all: bool,
        /// Return the last N blocks.
        #[arg(long, group = "history_selector")]
        last: Option<usize>,
        /// Return only the most recent AI reply.
        #[arg(long = "last-message", group = "history_selector")]
        last_message: bool,
        /// Return only blocks of the given kind (user, ai,
        /// mail_sent, mail_received, channel, tool_use, system_info,
        /// system_warn, system_error, worker_spawn, worker_complete,
        /// agent_info).
        #[arg(long, group = "history_selector")]
        kind: Option<String>,
    },
    /// Abort the target's in-flight turn.
    Stop {
        /// Agent display id.
        #[arg(long)]
        id: String,
        /// Reason recorded in the target's transcript.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Put the target to sleep. Mailbox/channel traffic is refused
    /// until `agent wake`.
    Sleep {
        /// Agent display id.
        #[arg(long)]
        id: String,
    },
    /// Wake a slept agent.
    Wake {
        /// Agent display id.
        #[arg(long)]
        id: String,
    },
    /// Deliver a message to the target's user-priority queue
    /// unwrapped. The target sees it as if the human operator typed
    /// it into the TUI pane.
    SendAsUser {
        /// Target agent id.
        #[arg(long)]
        to: String,
        /// Message body.
        #[arg(long)]
        body: String,
    },
}

/// Sysadmin-only: orchestrator lifecycle.
#[derive(Debug, Clone, Subcommand)]
pub enum ServerCommand {
    /// Begin graceful shutdown. Idle agents are slept; working
    /// agents finish their current turn before the process exits.
    Shutdown,
    /// Exit immediately without waiting for in-flight turns. Data
    /// loss is possible — prefer `server shutdown`.
    Exit,
    /// Re-read the WCL config and reconcile the running fleet
    /// (agents, channels, workflows, automatons + webhooks). Same
    /// effect as the TUI `/reload` command and daemon SIGHUP.
    Reload,
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
                cmd:
                    WorkerCommand::Create {
                        template,
                        task,
                        cwd,
                        properties,
                    },
            } => {
                assert_eq!(template, "researcher");
                assert_eq!(task, "find FOO");
                assert!(cwd.is_none());
                assert!(properties.is_none());
            }
            other => panic!("expected Worker::Create, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_worker_create_with_properties() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "worker",
            "create",
            "--template",
            "jules_refactor",
            "--task",
            "foo",
            "--properties",
            "{\"repo\":\"acme/foo\"}",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Worker {
                cmd: WorkerCommand::Create { properties, .. },
            } => {
                assert_eq!(properties.as_deref(), Some("{\"repo\":\"acme/foo\"}"));
            }
            other => panic!("expected Worker::Create, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_worker_archive() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "worker",
            "archive",
            "--id",
            "jules_refactor#abcd1234",
        ])
        .expect("parses");
        match args.cmd {
            AgentCommand::Worker {
                cmd: WorkerCommand::Archive { id },
            } => {
                assert_eq!(id, "jules_refactor#abcd1234");
            }
            other => panic!("expected Worker::Archive, got {other:?}"),
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
                        status,
                        planned_assignee,
                        worktree,
                    },
            } => {
                assert_eq!(title, "fix bug");
                assert!(description.is_none());
                assert!(assignee.is_none());
                assert!(blocked_by.is_empty());
                assert!(status.is_none());
                assert!(planned_assignee.is_none());
                assert!(worktree.is_none());
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
            "--worktree",
            "/repo",
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
                        status,
                        planned_assignee,
                        worktree,
                    },
            } => {
                assert_eq!(title, "fix bug");
                assert_eq!(description.as_deref(), Some("details here"));
                assert_eq!(assignee.as_deref(), Some("tester"));
                assert!(blocked_by.is_empty());
                assert!(status.is_none());
                assert!(planned_assignee.is_none());
                assert_eq!(worktree.as_deref(), Some("/repo"));
            }
            other => panic!("expected Task::Add, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_add_planning() {
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "add",
            "--title",
            "draft",
            "--assignee",
            "user",
            "--status",
            "planning",
            "--planned-assignee",
            "builders",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Add {
                        status,
                        planned_assignee,
                        worktree,
                        ..
                    },
            } => {
                assert_eq!(status.as_deref(), Some("planning"));
                assert_eq!(planned_assignee.as_deref(), Some("builders"));
                assert!(worktree.is_none());
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
            "--worktree",
            "/repo",
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
                        worktree,
                        clear_worktree,
                        add_blocked_by,
                        remove_blocked_by,
                        ..
                    },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(status.as_deref(), Some("done"));
                assert_eq!(note.as_deref(), Some("finished"));
                assert!(!unassign);
                assert_eq!(worktree.as_deref(), Some("/repo"));
                assert!(!clear_worktree);
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
    fn agent_args_parses_task_review() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "review",
            "--id",
            &id.to_string(),
            "--reviewer",
            "reviewer",
            "--note",
            "ready",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd:
                    TaskCommand::Review {
                        id: parsed,
                        reviewer,
                        note,
                    },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(reviewer, "reviewer");
                assert_eq!(note.as_deref(), Some("ready"));
            }
            other => panic!("expected Task::Review, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_review_approve() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "review-approve",
            "--id",
            &id.to_string(),
            "--note",
            "ship it",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd: TaskCommand::ReviewApprove { id: parsed, note },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(note.as_deref(), Some("ship it"));
            }
            other => panic!("expected Task::ReviewApprove, got {other:?}"),
        }
    }

    #[test]
    fn agent_args_parses_task_review_reject() {
        let id = Uuid::new_v4();
        let args = AgentArgs::try_parse_from([
            "aciddog",
            "task",
            "review-reject",
            "--id",
            &id.to_string(),
            "--reason",
            "needs tests",
        ])
        .unwrap();
        match args.cmd {
            AgentCommand::Task {
                cmd: TaskCommand::ReviewReject { id: parsed, reason },
            } => {
                assert_eq!(parsed, id);
                assert_eq!(reason, "needs tests");
            }
            other => panic!("expected Task::ReviewReject, got {other:?}"),
        }
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
            "aciddog",
            "cron",
            "add",
            "--target",
            "r",
            "--kind",
            "message",
            "--payload",
            "p"
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
        let show = AgentArgs::try_parse_from(["aciddog", "cron", "show", "--id", &id.to_string()])
            .unwrap();
        assert!(matches!(
            show.cmd,
            AgentCommand::Cron {
                cmd: CronCommand::Show { id: parsed }
            } if parsed == id
        ));
        let rm = AgentArgs::try_parse_from(["aciddog", "cron", "remove", "--id", &id.to_string()])
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
