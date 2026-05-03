pub mod client;
pub mod preamble;
pub mod server;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

/// Env var name holding the agent session UUID for child processes.
pub const ENV_AGENT_ID: &str = "ACIDDOG_AGENT_ID";

/// Env var name holding the full path to the agent IPC socket for child processes.
pub const ENV_AGENT_SOCKET: &str = "ACIDDOG_AGENT_SOCKET";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub agent_id: Uuid,
    #[serde(flatten)]
    pub cmd: RequestCmd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum RequestCmd {
    Test,
    /// Send a chat message to another agent on the team by its config id.
    /// The orchestrator wraps the body as `<Message from="sender">body</Message>`
    /// before delivering it, and enforces the sender's `can_message` allowlist.
    SendMessage {
        to: String,
        body: String,
    },
    /// Broadcast a message to every other member of a channel. The
    /// orchestrator wraps the body as
    /// `<ChannelMessage channel="<id>" from="<sender>">body</ChannelMessage>`
    /// and enforces that the sender is a declared member of the channel.
    /// Delivered at the lowest priority — below user input and direct
    /// mailbox messages.
    ChannelSend {
        id: String,
        body: String,
    },
    /// Create an ephemeral runtime channel. The caller is auto-joined
    /// so they can `channel send` to their own meeting immediately.
    /// Errors if the id collides with an existing channel (WCL or
    /// runtime) or violates identifier rules.
    ChannelCreate {
        id: String,
    },
    /// Add the calling agent to an existing channel. Works for both
    /// ephemeral and WCL-declared channels — runtime-only membership
    /// that does not persist across restarts.
    ChannelJoin {
        id: String,
    },
    /// Add another agent to a channel. The caller must themselves be
    /// a member of the channel. Works for ephemeral and WCL-declared
    /// channels (runtime-only — not persisted).
    ChannelAdd {
        id: String,
        member: String,
    },
    /// Remove a member from a channel. The caller must be a member.
    /// Works for ephemeral and WCL-declared channels (runtime-only —
    /// WCL membership restored on restart).
    ChannelKick {
        id: String,
        member: String,
    },
    /// Destroy an ephemeral channel. The caller must be a member.
    /// Rejects WCL-declared channels.
    ChannelEnd {
        id: String,
    },
    /// Spawn an ephemeral worker from a declared worker template.
    /// Enforces the caller's `can_create_workers` allowlist. Reply
    /// message carries the new worker's display id and session UUID.
    ///
    /// `properties` is a free-form JSON object passed through to the
    /// worker's core at build time — required for remote-worker cores
    /// that need per-spawn context (e.g. Jules requires
    /// `{"repo":"owner/repo"}`). Ignored by local cores.
    WorkerCreate {
        template: String,
        task: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        properties: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ai_core_override: Option<String>,
    },
    /// Deliver a final report to the parent and hibernate the calling
    /// worker. Only valid when the caller is itself a worker.
    WorkerComplete {
        body: String,
    },
    /// Parent-initiated close of a worker. Drops the worker's IPC
    /// channels, hibernates it, and — for remote-worker cores — tears
    /// down the backend session (e.g. DELETE the Jules session).
    /// Validated to be called by the worker's parent.
    WorkerArchive {
        worker: String,
    },
    /// Parent-initiated restart of a worker's AI core. Drops the
    /// worker's IPC channels, hibernates it, and composes a fresh
    /// worker that resumes from the archived worker's log.
    /// Validated to be called by the worker's parent.
    WorkerRestartCore {
        worker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ai_core_override: Option<String>,
    },
    /// Internal: remote-worker cores use this to register their
    /// backend session identifier with the orchestrator once the
    /// remote session has been successfully created. Stored on the
    /// worker's `AgentChat` so `worker archive` can issue an explicit
    /// remote DELETE at teardown time. Not part of the documented
    /// agent CLI — callers never emit this.
    RemoteSessionBound {
        name: String,
    },
    /// Start a new instance of a declared workflow. Enforces the
    /// caller's `can_run_workflows` allowlist. `properties` is a map
    /// of property-id → JSON value; each is coerced against the
    /// declared `PropertyKind` (unsupplied properties fall back to
    /// declared defaults; required-with-no-value fails the request).
    /// Reply message carries the new instance id.
    WorkflowRun {
        workflow: String,
        #[serde(default)]
        properties: std::collections::BTreeMap<String, serde_json::Value>,
    },
    /// Supervisor: structured list of every declared workflow, with
    /// enough property metadata to render a portal form. Reply carries
    /// `Vec<wire_workflow::WireWorkflowTemplate>` in `Response::data`.
    WorkflowTemplatesJson,
    /// Supervisor: live snapshot of the TUI's `workflow_instances`
    /// mirror — running, failed, and lingering-after-success instances.
    /// Reply carries `Vec<wire_workflow::WireWorkflowInstance>` in
    /// `Response::data`.
    WorkflowInstancesJson,
    /// Supervisor: drop a failed workflow instance (parity with the
    /// TUI's `d` keybind). Running instances can't be dismissed; the
    /// handler replies with `error` in that case.
    WorkflowDismiss {
        instance_id: u64,
    },
    /// Add a new task to the shared team board.
    TaskAdd {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        /// Ids of existing tasks that must be `Done` before this one
        /// can be claimed via `task take`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        blocked_by: Vec<Uuid>,
        /// Initial status. Currently only `planning` is settable
        /// here; omit for default behaviour (auto-bumps to `assigned`
        /// when `assignee` is set, else `open`).
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        /// Pre-stored handoff target for a `planning` task. Required
        /// (and only meaningful) when `status = "planning"`.
        #[serde(skip_serializing_if = "Option::is_none")]
        planned_assignee: Option<String>,
        /// Filesystem worktree path used as spawned workers' cwd.
        #[serde(skip_serializing_if = "Option::is_none")]
        worktree: Option<String>,
    },
    /// List tasks, optionally filtered.
    TaskList {
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(default)]
        mine: bool,
        #[serde(default)]
        unassigned: bool,
    },
    /// Supervisor: structured variant of `TaskList`. Returns a
    /// serialized `Vec<Task>` in `Response::data` (full nested shape
    /// including todos, comments, notes, blocked_by). Same filter
    /// arguments as `TaskList`. Intended for the manager web portal's
    /// Kanban view — plaintext parsing is too lossy for rich UIs.
    TaskListJson {
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(default)]
        mine: bool,
        #[serde(default)]
        unassigned: bool,
    },
    /// Supervisor: structured repository list for the manager web UI.
    /// Reply carries `Vec<wire_repos::WireRepository>` in `Response::data`.
    ReposListJson,
    /// Supervisor: structured branch diff for a repository worktree.
    /// Reply carries `wire_repos::WireDiffOutcome` in `Response::data`.
    RepoDiffJson {
        repo_id: String,
        branch: String,
    },
    /// Update fields on an existing task. Fields set to `None` are
    /// left unchanged. `unassign` clears the assignee (mutually
    /// exclusive with `assignee`). `add_blocked_by` / `remove_blocked_by`
    /// edit the dependency list.
    TaskUpdate {
        id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        #[serde(default)]
        unassign: bool,
        /// Revise the pre-stored handoff target on a planning task.
        #[serde(skip_serializing_if = "Option::is_none")]
        planned_assignee: Option<String>,
        /// Set the filesystem worktree path for spawned workers.
        #[serde(skip_serializing_if = "Option::is_none")]
        worktree: Option<String>,
        /// Clear `planned_assignee` (mutually exclusive with
        /// `planned_assignee`).
        #[serde(default)]
        clear_planned_assignee: bool,
        /// Clear `worktree` (mutually exclusive with `worktree`).
        #[serde(default)]
        clear_worktree: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        add_blocked_by: Vec<Uuid>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        remove_blocked_by: Vec<Uuid>,
    },
    /// Remove a task by id.
    TaskRemove {
        id: Uuid,
    },
    /// Reject a `planning` task (human-only flow). Atomically
    /// appends `reason` as a `Rejected: ...` comment, reassigns the
    /// task back to its `created_by` for revision, and pings the
    /// creator. Status stays `planning`. Empty reason is rejected.
    TaskReject {
        id: Uuid,
        reason: String,
    },
    /// Return a detailed rendering of a single task — full
    /// description, dependencies, and the complete note log.
    TaskShow {
        id: Uuid,
    },
    /// Atomically claim the oldest unassigned task for the caller.
    TaskTake,
    /// Submit an in-progress task for reviewer sign-off.
    TaskReview {
        id: Uuid,
        reviewer: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// Approve a task assigned to the caller for review.
    TaskReviewApprove {
        id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// Reject a task assigned to the caller for review.
    TaskReviewReject {
        id: Uuid,
        reason: String,
    },
    /// Append a standalone note to a task.
    TaskNote {
        id: Uuid,
        body: String,
    },
    /// Append a TODO to a task (starts `pending`).
    TaskTodoAdd {
        task: Uuid,
        body: String,
    },
    /// List TODOs on a task (reply body is a plaintext table).
    TaskTodoList {
        task: Uuid,
    },
    /// Update a TODO's body and/or status.
    TaskTodoUpdate {
        task: Uuid,
        id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    /// Remove a TODO from a task.
    TaskTodoRemove {
        task: Uuid,
        id: Uuid,
    },
    /// Post a comment on a task (intentional agent-to-agent note).
    TaskCommentAdd {
        task: Uuid,
        body: String,
    },
    /// List comments on a task.
    TaskCommentList {
        task: Uuid,
    },
    /// Edit a comment's body.
    TaskCommentUpdate {
        task: Uuid,
        id: Uuid,
        body: String,
    },
    /// Remove a comment from a task.
    TaskCommentRemove {
        task: Uuid,
        id: Uuid,
    },
    /// Search a memory bank. When `query` is `Some`, the search runs
    /// through the HNSW index; when `None`, results come from a pure
    /// relational list (newest first). Tag and title filters apply in
    /// both modes; `limit` is the top-k cap (default 5).
    MemorySearch {
        bank: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title_contains: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        include_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },
    /// Persist a new entry in a memory bank.
    MemoryStore {
        bank: String,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    /// Create a directed link between two entries in a memory bank.
    /// Both ids must name existing entries.
    MemoryLink {
        bank: String,
        from: Uuid,
        to: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    /// Remove a directed link between two entries.
    MemoryUnlink {
        bank: String,
        from: Uuid,
        to: Uuid,
    },
    /// Rewrite an existing entry. Re-embeds `body`; replaces the tag
    /// set entirely. `title` = `None` clears the title.
    MemoryUpdate {
        bank: String,
        id: Uuid,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    /// Delete an entry. Cascades to its tags and any link that
    /// references it.
    MemoryRemove {
        bank: String,
        id: Uuid,
    },
    /// Fetch a single entry by id.
    MemoryShow {
        bank: String,
        id: Uuid,
    },
    /// List every link touching an entry. `direction` is one of
    /// `out` / `in` / `both` (default `both`). `include_tags` /
    /// `exclude_tags` filter the peers.
    MemoryLinks {
        bank: String,
        id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        include_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude_tags: Vec<String>,
    },
    /// Supervisor: list every declared memory bank + its entry count.
    /// Returns `Vec<WireBankInfo>` in `Response::data`.
    MemoryBanks,
    /// Supervisor structured memory search. Same selector shape as
    /// `MemorySearch`; response carries `Vec<WireHit>` in `data`.
    MemorySearchJson {
        bank: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title_contains: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        include_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },
    /// Like `MemoryStore` but returns `{ "id": "<uuid>" }` in `data`
    /// so the caller can link/display the new entry without parsing
    /// the plaintext reply.
    MemoryStoreJson {
        bank: String,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    /// Supervisor structured `MemoryLinks`. Response carries
    /// `Vec<WireLink>` in `data`.
    MemoryLinksJson {
        bank: String,
        id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        include_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude_tags: Vec<String>,
    },
    /// Supervisor: full graph snapshot of a bank. Returns
    /// `WireGraph` (nodes + edges) in `data`. Nodes project only
    /// title/tags/timestamps to keep the payload small; callers
    /// fetch bodies on demand via `MemoryShow` /
    /// `MemorySearchJson`.
    MemoryGraph {
        bank: String,
    },
    /// Return current time (local + UTC) and the orchestrator's
    /// timezone. Cheap, in-process, the canonical way for an agent to
    /// discover "now" before scheduling a cron job.
    Info,
    /// Schedule a payload to be delivered to a target agent on a
    /// recurring or one-off schedule.
    ///
    /// `kind` is `"message"` or `"command"` (slash command). For
    /// recurring jobs `schedule` is a 5-field POSIX cron expression
    /// (`min hour dom mon dow`); for one-off (`oneoff = true`) it is
    /// a wall-clock datetime `YYYY-MM-DDTHH:MM[:SS]`. `tz` is
    /// `"utc"` or `"local"` (default `"local"` if omitted).
    /// `catchup_on_startup` only applies to one-off jobs.
    CronAdd {
        target: String,
        kind: String,
        payload: String,
        schedule: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
        #[serde(default)]
        oneoff: bool,
        #[serde(default)]
        catchup_on_startup: bool,
        #[serde(default)]
        disabled: bool,
    },
    /// List cron jobs, optionally filtered.
    CronList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default)]
        include_disabled: bool,
        /// Only show jobs created by the calling agent.
        #[serde(default)]
        mine: bool,
    },
    /// Show the full body of a cron job.
    CronShow {
        id: Uuid,
    },
    /// Update fields on an existing cron job. Omitted fields are
    /// left unchanged. Updating the schedule or timezone recomputes
    /// `next_run` from now.
    CronUpdate {
        id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schedule: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        oneoff: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        catchup_on_startup: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
    },
    /// Remove a cron job by id.
    CronRemove {
        id: Uuid,
    },
    /// Structured `cron list` — same filtering as [`RequestCmd::CronList`]
    /// but the reply carries a `Vec<WireCronJob>` in `Response::data`
    /// instead of a preformatted text block in `message`. Intended for
    /// programmatic consumers (e.g. the manager web portal's Cron tab).
    CronListJson {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default)]
        include_disabled: bool,
        /// Only show jobs created by the calling agent.
        #[serde(default)]
        mine: bool,
    },
    /// List every skill in the caller's allowlist as `id — description`
    /// lines. Scoped to the caller's assigned skills.
    SkillList,
    /// Filter the caller's assigned skills by case-insensitive
    /// substring match against id + description. `query` is treated
    /// as a single substring; returns matching entries as
    /// `id — description` lines.
    SkillSearch {
        query: String,
    },
    /// Load an assigned skill's `SKILL.md` body. The orchestrator
    /// enforces the caller's skill allowlist and prepends a
    /// `{baseDir} = <abs-path>` header pointing at the skill folder.
    SkillRun {
        name: String,
    },
    /// Supervisor: list every agent, worker, and automaton with a
    /// status summary (kind, status, asleep, hibernated, parent,
    /// ai_core). Reply is a plaintext table.
    AgentList,
    /// Supervisor: status snapshot for a single agent/worker/automaton.
    AgentStatus {
        agent: String,
    },
    /// Supervisor: selected chat history for an agent/worker/automaton.
    /// The selector picks a subset of the in-memory `ChatBlock` log
    /// (user prompts, AI replies, mail, channel, tool-use summaries,
    /// system notices) — see [`HistorySelector`].
    AgentHistory {
        agent: String,
        #[serde(flatten)]
        selector: HistorySelector,
    },
    /// Supervisor: structured chat history for an agent/worker/automaton.
    /// Like [`RequestCmd::AgentHistory`] but returns a serialized
    /// `Vec<WireChatBlock>` in `Response::data` instead of a flat
    /// plaintext summary in `message`. Intended for programmatic
    /// consumers (e.g. the manager web portal's chat pane).
    AgentHistoryJson {
        agent: String,
        #[serde(flatten)]
        selector: HistorySelector,
    },
    /// Supervisor: abort the target agent's in-flight turn (if any).
    /// Equivalent to the human running `/stop` on the target pane.
    AgentStop {
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Supervisor: put the target agent to sleep. Mailbox/channel
    /// deliveries are refused until the next `AgentWake`.
    AgentSleep {
        agent: String,
    },
    /// Supervisor: wake a previously-slept agent.
    AgentWake {
        agent: String,
    },
    /// Supervisor: deliver a message to the target's user-priority
    /// queue unwrapped. The target sees it indistinguishable from the
    /// human typing the body into the TUI pane.
    AgentSendAsUser {
        to: String,
        body: String,
    },
    /// Supervisor: snapshot of the running team + AI cores config.
    /// Returns a serialized `WireTeamInfo` in `Response::data`.
    /// Intended for the manager web portal's Team tab.
    TeamInfo,
    /// Supervisor: list every declared channel with its membership +
    /// in-memory message count. Returns `Vec<WireChannelInfo>` in
    /// `Response::data`.
    ChannelList,
    /// Supervisor: message transcript for a single channel, oldest
    /// first. The welcome banner (if any) is included as the first
    /// entry with `is_welcome = true`. Returns
    /// `Vec<WireChannelMessage>` in `Response::data`.
    ChannelHistory {
        channel: String,
    },
    /// Supervisor: post a message into `channel` as if the human
    /// operator had typed it into the TUI's channel pane
    /// (`ChannelSender::User`). Same path as the TUI's
    /// `submit_channel_input` — validates the channel, appends a
    /// `ChannelMessage` block to the transcript + JSONL log,
    /// broadcasts to every member's channel_tx, and fans out on the
    /// channel SSE broadcaster.
    ChannelSendAsUser {
        channel: String,
        body: String,
    },
    /// Supervisor: snapshot of the human operator's inbox. Returns a
    /// `Vec<WireInboxMessage>` in `Response::data`. Order matches the
    /// TUI's list (oldest first, same as `App::inbox`); the `index`
    /// used by `InboxDelete` refers to this order.
    InboxList,
    /// Supervisor: delete one inbox entry by its list index. Mirrors
    /// the TUI's `d` key — purely local, no wire event emitted. A
    /// bogus index (out of range) fails with `ok: false`.
    InboxDelete {
        index: usize,
    },
    /// Supervisor: list every running worker across the fleet,
    /// optionally filtered to workers spawned by a specific parent.
    WorkerList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
    },
    /// Supervisor: archive any worker by display id, bypassing the
    /// parent-ownership check that `WorkerArchive` enforces.
    WorkerForceArchive {
        worker: String,
    },
    /// Supervisor: restart any worker's AI core by display id,
    /// bypassing the parent-ownership check that `WorkerRestartCore`
    /// enforces.
    WorkerForceRestartCore {
        worker: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ai_core_override: Option<String>,
    },
    /// Sysadmin: begin graceful shutdown of the orchestrator. Idle
    /// agents are put to sleep; working agents are allowed to finish
    /// their current turn before the process exits.
    ServerShutdown,
    /// Sysadmin: exit immediately. Raises `should_quit` without
    /// waiting for in-flight turns. Data loss is possible if a
    /// working agent is mid-response.
    ServerExit,
    /// Sysadmin: re-read the WCL config and reconcile the running
    /// fleet. See `tui::reload` for the per-subsystem semantics.
    /// Same code path as the TUI `/reload` slash command and the
    /// daemon SIGHUP handler.
    ServerReload,
    /// Sysadmin: read the raw WCL source the daemon was started with
    /// (the file at `App::config_path`). Returns
    /// `{ wcl: String, path: String }` in `Response::data`. The
    /// manager uses this to bootstrap the team editor with the exact
    /// on-disk contents (including any comments / formatting).
    TeamRead,
    /// Sysadmin: replace the on-disk WCL config with the supplied
    /// source string and reconcile the running fleet. The daemon
    /// validates the WCL via `config::parse_config` *before* touching
    /// disk; if parsing fails the existing file is left intact.
    /// On success the new file is written via `<path>.tmp` + `rename(2)`
    /// and `tui::reload` is invoked (same code path as `ServerReload`).
    TeamWrite {
        wcl: String,
    },
    /// Sysadmin: list every skill directory under `.aciddog/skills/`.
    /// Distinct from the agent-scoped `SkillList`, which only returns
    /// the caller's allowlist. Returns
    /// `Vec<wire_skills::WireSkillSummary>` in `Response::data`.
    SkillsList,
    /// Sysadmin: read every file in a skill directory. Returns a
    /// `WireSkillContents { name, files }` in `Response::data` where
    /// each file carries `rel_path` plus base64-encoded `contents_b64`.
    SkillRead {
        name: String,
    },
    /// Sysadmin: atomically replace the named skill directory with the
    /// supplied files. Files are written into a sibling temp dir and
    /// `rename(2)`d into place. Caller's responsibility to include
    /// `SKILL.md`. Empty `files` is rejected.
    SkillWrite {
        name: String,
        files: Vec<SkillFile>,
    },
    /// Sysadmin: delete a skill directory under `.aciddog/skills/`.
    /// Idempotent — succeeds with `ok: true` even if the skill doesn't
    /// exist.
    SkillRemove {
        name: String,
    },
}

/// One file inside a skill directory. `rel_path` is interpreted relative
/// to the skill root (`.aciddog/skills/<name>/`); validated to disallow
/// `..`, leading `/`, and NUL bytes. `contents_b64` is the file body
/// encoded as standard base64.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub rel_path: String,
    pub contents_b64: String,
}

/// Which slice of a target agent's chat log to return from
/// `AgentHistory`. Serialized with an internally-tagged `select`
/// discriminator so the JSON shape is ergonomic to emit from the tag
/// IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "select", rename_all = "snake_case")]
pub enum HistorySelector {
    /// Every block in the transcript.
    All,
    /// The most recent `n` blocks (any kind).
    Last { n: usize },
    /// Only the most recent AI markdown reply (skips tool-use and
    /// system blocks). Empty reply if the agent hasn't spoken yet.
    LastMessage,
    /// Only blocks of the given kind — one of `user`, `ai`,
    /// `mail_sent`, `mail_received`, `channel`, `tool_use`,
    /// `system_info`, `system_warn`, `system_error`, `worker_spawn`,
    /// `worker_complete`.
    Kind { kind: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Optional structured payload. Used by commands whose reply is
    /// easier to consume as JSON than as plaintext embedded in
    /// `message` (e.g. `AgentHistoryJson`). Older callers that only
    /// inspect `message`/`error` ignore this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Response {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: Some(message.into()),
            error: None,
            data: None,
        }
    }

    /// Successful reply carrying a structured JSON payload. Sets
    /// `ok=true` and leaves `message` empty — the caller is expected
    /// to read `data`.
    pub fn ok_data(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            message: None,
            error: None,
            data: Some(data),
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: None,
            error: Some(error.into()),
            data: None,
        }
    }
}

/// An inbound request routed from the socket server to the application,
/// together with a oneshot channel for the handler to deliver a reply.
#[derive(Debug)]
pub struct AgentSocketEvent {
    pub request: Request,
    pub reply: oneshot::Sender<Response>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_flat_cmd_tag() {
        let id = Uuid::nil();
        let req = Request {
            agent_id: id,
            cmd: RequestCmd::Test,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"test\""), "got: {s}");
        assert!(s.contains("\"agent_id\""), "got: {s}");
    }

    #[test]
    fn request_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: id,
            cmd: RequestCmd::Test,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back.agent_id, id);
        assert!(matches!(back.cmd, RequestCmd::Test));
    }

    #[test]
    fn send_message_serializes_with_to_and_body() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::SendMessage {
                to: "coder".into(),
                body: "hi".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"send_message\""), "got: {s}");
        assert!(s.contains("\"to\":\"coder\""), "got: {s}");
        assert!(s.contains("\"body\":\"hi\""), "got: {s}");
    }

    #[test]
    fn send_message_roundtrips() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::SendMessage {
                to: "coder".into(),
                body: "hello world".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::SendMessage { to, body } => {
                assert_eq!(to, "coder");
                assert_eq!(body, "hello world");
            }
            other => panic!("expected SendMessage, got {other:?}"),
        }
    }

    #[test]
    fn worker_create_serializes_with_template_and_task() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::WorkerCreate {
                template: "researcher".into(),
                task: "find FOO".into(),
                cwd: None,
                properties: None,
                ai_core_override: None,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_create\""), "got: {s}");
        assert!(s.contains("\"template\":\"researcher\""), "got: {s}");
        assert!(s.contains("\"task\":\"find FOO\""), "got: {s}");
        assert!(
            !s.contains("\"properties\""),
            "properties should be omitted when None: {s}"
        );
    }

    #[test]
    fn worker_create_serializes_with_properties() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::WorkerCreate {
                template: "jules_refactor".into(),
                task: "refactor foo".into(),
                cwd: None,
                properties: Some(serde_json::json!({"repo":"acme/foo","branch":"main"})),
                ai_core_override: None,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"properties\""), "got: {s}");
        assert!(s.contains("\"repo\":\"acme/foo\""), "got: {s}");
    }

    #[test]
    fn worker_complete_serializes_with_body() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::WorkerComplete {
                body: "all done".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_complete\""), "got: {s}");
        assert!(s.contains("\"body\":\"all done\""), "got: {s}");
    }

    #[test]
    fn worker_create_roundtrips() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerCreate {
                template: "r".into(),
                task: "t".into(),
                cwd: None,
                properties: None,
                ai_core_override: None,
            },
        };
        let back: Request = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        match back.cmd {
            RequestCmd::WorkerCreate {
                template,
                task,
                cwd,
                properties,
                ai_core_override,
            } => {
                assert_eq!(template, "r");
                assert_eq!(task, "t");
                assert!(cwd.is_none());
                assert!(properties.is_none());
                assert!(ai_core_override.is_none());
            }
            other => panic!("expected WorkerCreate, got {other:?}"),
        }
    }

    #[test]
    fn worker_create_roundtrips_with_ai_core_override() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerCreate {
                template: "r".into(),
                task: "t".into(),
                cwd: None,
                properties: None,
                ai_core_override: Some("claude-sonnet-fake".into()),
            },
        };
        let back: Request = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        match back.cmd {
            RequestCmd::WorkerCreate {
                ai_core_override, ..
            } => {
                assert_eq!(ai_core_override.as_deref(), Some("claude-sonnet-fake"));
            }
            other => panic!("expected WorkerCreate, got {other:?}"),
        }
    }

    #[test]
    fn worker_create_deserializes_old_payload_without_ai_core_override() {
        let raw = r#"{
            "agent_id":"00000000-0000-0000-0000-000000000000",
            "cmd":"worker_create",
            "template":"r",
            "task":"t"
        }"#;
        let back: Request = serde_json::from_str(raw).unwrap();
        match back.cmd {
            RequestCmd::WorkerCreate {
                ai_core_override, ..
            } => {
                assert!(ai_core_override.is_none());
            }
            other => panic!("expected WorkerCreate, got {other:?}"),
        }
    }

    #[test]
    fn worker_archive_roundtrips() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerArchive {
                worker: "jules_refactor#abcd1234".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_archive\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::WorkerArchive { worker } => {
                assert_eq!(worker, "jules_refactor#abcd1234");
            }
            other => panic!("expected WorkerArchive, got {other:?}"),
        }
    }

    #[test]
    fn worker_restart_core_roundtrips_with_ai_core_override() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerRestartCore {
                worker: "jules_refactor#abcd1234".into(),
                ai_core_override: Some("claude-sonnet-fake".into()),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_restart_core\""), "got: {s}");
        assert!(
            s.contains("\"ai_core_override\":\"claude-sonnet-fake\""),
            "got: {s}"
        );
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::WorkerRestartCore {
                worker,
                ai_core_override,
            } => {
                assert_eq!(worker, "jules_refactor#abcd1234");
                assert_eq!(ai_core_override.as_deref(), Some("claude-sonnet-fake"));
            }
            other => panic!("expected WorkerRestartCore, got {other:?}"),
        }
    }

    #[test]
    fn worker_restart_core_roundtrips_without_ai_core_override() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerRestartCore {
                worker: "jules_refactor#abcd1234".into(),
                ai_core_override: None,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_restart_core\""), "got: {s}");
        assert!(!s.contains("ai_core_override"), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::WorkerRestartCore {
                worker,
                ai_core_override,
            } => {
                assert_eq!(worker, "jules_refactor#abcd1234");
                assert!(ai_core_override.is_none());
            }
            other => panic!("expected WorkerRestartCore, got {other:?}"),
        }
    }

    #[test]
    fn worker_force_restart_core_roundtrips_with_ai_core_override() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerForceRestartCore {
                worker: "jules_refactor#abcd1234".into(),
                ai_core_override: Some("claude-sonnet-fake".into()),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(
            s.contains("\"cmd\":\"worker_force_restart_core\""),
            "got: {s}"
        );
        assert!(
            s.contains("\"ai_core_override\":\"claude-sonnet-fake\""),
            "got: {s}"
        );
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::WorkerForceRestartCore {
                worker,
                ai_core_override,
            } => {
                assert_eq!(worker, "jules_refactor#abcd1234");
                assert_eq!(ai_core_override.as_deref(), Some("claude-sonnet-fake"));
            }
            other => panic!("expected WorkerForceRestartCore, got {other:?}"),
        }
    }

    #[test]
    fn worker_force_restart_core_roundtrips_without_ai_core_override() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerForceRestartCore {
                worker: "jules_refactor#abcd1234".into(),
                ai_core_override: None,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(
            s.contains("\"cmd\":\"worker_force_restart_core\""),
            "got: {s}"
        );
        assert!(!s.contains("ai_core_override"), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::WorkerForceRestartCore {
                worker,
                ai_core_override,
            } => {
                assert_eq!(worker, "jules_refactor#abcd1234");
                assert!(ai_core_override.is_none());
            }
            other => panic!("expected WorkerForceRestartCore, got {other:?}"),
        }
    }

    #[test]
    fn task_add_roundtrips() {
        let dep = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::TaskAdd {
                title: "fix bug".into(),
                description: Some("details".into()),
                assignee: Some("tester".into()),
                blocked_by: vec![dep],
                status: None,
                planned_assignee: None,
                worktree: Some("/repo".into()),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_add\""), "got: {s}");
        assert!(s.contains("\"title\":\"fix bug\""), "got: {s}");
        assert!(s.contains("\"blocked_by\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskAdd {
                title,
                description,
                assignee,
                blocked_by,
                status,
                planned_assignee,
                worktree,
            } => {
                assert_eq!(title, "fix bug");
                assert_eq!(description.as_deref(), Some("details"));
                assert_eq!(assignee.as_deref(), Some("tester"));
                assert_eq!(blocked_by, vec![dep]);
                assert!(status.is_none());
                assert!(planned_assignee.is_none());
                assert_eq!(worktree.as_deref(), Some("/repo"));
            }
            other => panic!("expected TaskAdd, got {other:?}"),
        }
    }

    #[test]
    fn task_add_omits_blocked_by_when_empty() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskAdd {
                title: "t".into(),
                description: None,
                assignee: None,
                blocked_by: vec![],
                status: None,
                planned_assignee: None,
                worktree: None,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(
            !s.contains("blocked_by"),
            "empty list should be omitted: {s}"
        );
    }

    #[test]
    fn task_add_carries_planning_fields() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskAdd {
                title: "draft".into(),
                description: Some("plan body".into()),
                assignee: Some("user".into()),
                blocked_by: vec![],
                status: Some("planning".into()),
                planned_assignee: Some("builders".into()),
                worktree: None,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"status\":\"planning\""), "got: {s}");
        assert!(s.contains("\"planned_assignee\":\"builders\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskAdd {
                status,
                planned_assignee,
                worktree,
                ..
            } => {
                assert_eq!(status.as_deref(), Some("planning"));
                assert_eq!(planned_assignee.as_deref(), Some("builders"));
                assert!(worktree.is_none());
            }
            other => panic!("expected TaskAdd, got {other:?}"),
        }
    }

    #[test]
    fn task_list_roundtrips() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskList {
                assignee: Some("coder".into()),
                status: Some("in_progress".into()),
                mine: true,
                unassigned: false,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_list\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::TaskList {
                mine: true,
                unassigned: false,
                ..
            }
        ));
    }

    #[test]
    fn repos_list_json_roundtrips_with_literal_cmd_tag() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::ReposListJson,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert_eq!(
            s,
            r#"{"agent_id":"00000000-0000-0000-0000-000000000000","cmd":"repos_list_json"}"#
        );
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(back.cmd, RequestCmd::ReposListJson));
    }

    #[test]
    fn repo_diff_json_roundtrips_with_literal_cmd_tag_and_fields() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::RepoDiffJson {
                repo_id: "aciddog".into(),
                branch: "feature/repos".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert_eq!(
            s,
            r#"{"agent_id":"00000000-0000-0000-0000-000000000000","cmd":"repo_diff_json","repo_id":"aciddog","branch":"feature/repos"}"#
        );
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::RepoDiffJson { repo_id, branch } => {
                assert_eq!(repo_id, "aciddog");
                assert_eq!(branch, "feature/repos");
            }
            other => panic!("expected RepoDiffJson, got {other:?}"),
        }
    }

    #[test]
    fn task_update_roundtrips() {
        let id = Uuid::new_v4();
        let dep = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskUpdate {
                id,
                title: None,
                description: None,
                status: Some("done".into()),
                assignee: Some("coder".into()),
                unassign: false,
                planned_assignee: None,
                worktree: Some("/repo".into()),
                clear_planned_assignee: false,
                clear_worktree: false,
                note: Some("ready".into()),
                add_blocked_by: vec![dep],
                remove_blocked_by: vec![],
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_update\""), "got: {s}");
        assert!(s.contains("\"add_blocked_by\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskUpdate {
                id: parsed,
                status,
                assignee,
                note,
                unassign,
                worktree,
                clear_worktree,
                add_blocked_by,
                remove_blocked_by,
                ..
            } => {
                assert_eq!(parsed, id);
                assert_eq!(status.as_deref(), Some("done"));
                assert_eq!(assignee.as_deref(), Some("coder"));
                assert_eq!(note.as_deref(), Some("ready"));
                assert!(!unassign);
                assert_eq!(worktree.as_deref(), Some("/repo"));
                assert!(!clear_worktree);
                assert_eq!(add_blocked_by, vec![dep]);
                assert!(remove_blocked_by.is_empty());
            }
            other => panic!("expected TaskUpdate, got {other:?}"),
        }
    }

    #[test]
    fn task_remove_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskRemove { id },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_remove\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::TaskRemove { id: parsed } if parsed == id
        ));
    }

    #[test]
    fn task_show_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskShow { id },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_show\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::TaskShow { id: parsed } if parsed == id
        ));
    }

    #[test]
    fn task_take_roundtrips() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskTake,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_take\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(back.cmd, RequestCmd::TaskTake));
    }

    #[test]
    fn task_review_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskReview {
                id,
                reviewer: "reviewer".into(),
                note: Some("ready".into()),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_review\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskReview {
                id: parsed,
                reviewer,
                note,
            } => {
                assert_eq!(parsed, id);
                assert_eq!(reviewer, "reviewer");
                assert_eq!(note.as_deref(), Some("ready"));
            }
            other => panic!("expected TaskReview, got {other:?}"),
        }
    }

    #[test]
    fn task_review_approve_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskReviewApprove {
                id,
                note: Some("ship it".into()),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_review_approve\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskReviewApprove { id: parsed, note } => {
                assert_eq!(parsed, id);
                assert_eq!(note.as_deref(), Some("ship it"));
            }
            other => panic!("expected TaskReviewApprove, got {other:?}"),
        }
    }

    #[test]
    fn task_review_reject_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskReviewReject {
                id,
                reason: "needs tests".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_review_reject\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskReviewReject { id: parsed, reason } => {
                assert_eq!(parsed, id);
                assert_eq!(reason, "needs tests");
            }
            other => panic!("expected TaskReviewReject, got {other:?}"),
        }
    }

    #[test]
    fn task_note_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskNote {
                id,
                body: "see commit abc123".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_note\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskNote { id: parsed, body } => {
                assert_eq!(parsed, id);
                assert_eq!(body, "see commit abc123");
            }
            other => panic!("expected TaskNote, got {other:?}"),
        }
    }

    #[test]
    fn task_todo_roundtrips() {
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        for (variant, tag) in [
            (
                RequestCmd::TaskTodoAdd {
                    task: task_id,
                    body: "b".into(),
                },
                "task_todo_add",
            ),
            (RequestCmd::TaskTodoList { task: task_id }, "task_todo_list"),
            (
                RequestCmd::TaskTodoUpdate {
                    task: task_id,
                    id: todo_id,
                    status: Some("in_progress".into()),
                    body: None,
                },
                "task_todo_update",
            ),
            (
                RequestCmd::TaskTodoRemove {
                    task: task_id,
                    id: todo_id,
                },
                "task_todo_remove",
            ),
        ] {
            let req = Request {
                agent_id: Uuid::nil(),
                cmd: variant.clone(),
            };
            let s = serde_json::to_string(&req).unwrap();
            assert!(
                s.contains(&format!("\"cmd\":\"{tag}\"")),
                "missing tag {tag} in {s}"
            );
            let back: Request = serde_json::from_str(&s).unwrap();
            match (variant, back.cmd) {
                (
                    RequestCmd::TaskTodoAdd { task: a, .. },
                    RequestCmd::TaskTodoAdd { task: b, .. },
                ) => assert_eq!(a, b),
                (RequestCmd::TaskTodoList { task: a }, RequestCmd::TaskTodoList { task: b }) => {
                    assert_eq!(a, b)
                }
                (
                    RequestCmd::TaskTodoUpdate { id: a, .. },
                    RequestCmd::TaskTodoUpdate { id: b, .. },
                ) => assert_eq!(a, b),
                (
                    RequestCmd::TaskTodoRemove { id: a, .. },
                    RequestCmd::TaskTodoRemove { id: b, .. },
                ) => assert_eq!(a, b),
                (expected, got) => panic!("mismatch: {expected:?} vs {got:?}"),
            }
        }
    }

    #[test]
    fn task_comment_roundtrips() {
        let task_id = Uuid::new_v4();
        let c_id = Uuid::new_v4();
        for (variant, tag) in [
            (
                RequestCmd::TaskCommentAdd {
                    task: task_id,
                    body: "b".into(),
                },
                "task_comment_add",
            ),
            (
                RequestCmd::TaskCommentList { task: task_id },
                "task_comment_list",
            ),
            (
                RequestCmd::TaskCommentUpdate {
                    task: task_id,
                    id: c_id,
                    body: "b".into(),
                },
                "task_comment_update",
            ),
            (
                RequestCmd::TaskCommentRemove {
                    task: task_id,
                    id: c_id,
                },
                "task_comment_remove",
            ),
        ] {
            let req = Request {
                agent_id: Uuid::nil(),
                cmd: variant.clone(),
            };
            let s = serde_json::to_string(&req).unwrap();
            assert!(
                s.contains(&format!("\"cmd\":\"{tag}\"")),
                "missing tag {tag} in {s}"
            );
            let back: Request = serde_json::from_str(&s).unwrap();
            // Sanity: id/task survive.
            match (variant, back.cmd) {
                (
                    RequestCmd::TaskCommentAdd { task: a, .. },
                    RequestCmd::TaskCommentAdd { task: b, .. },
                ) => assert_eq!(a, b),
                (
                    RequestCmd::TaskCommentList { task: a },
                    RequestCmd::TaskCommentList { task: b },
                ) => assert_eq!(a, b),
                (
                    RequestCmd::TaskCommentUpdate { id: a, .. },
                    RequestCmd::TaskCommentUpdate { id: b, .. },
                ) => assert_eq!(a, b),
                (
                    RequestCmd::TaskCommentRemove { id: a, .. },
                    RequestCmd::TaskCommentRemove { id: b, .. },
                ) => assert_eq!(a, b),
                (expected, got) => panic!("mismatch: {expected:?} vs {got:?}"),
            }
        }
    }

    #[test]
    fn info_serializes_with_cmd_tag() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::Info,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"info\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(back.cmd, RequestCmd::Info));
    }

    #[test]
    fn cron_add_roundtrips() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::CronAdd {
                target: "reviewer".into(),
                kind: "message".into(),
                payload: "ping".into(),
                schedule: "0 9 * * *".into(),
                tz: Some("local".into()),
                oneoff: false,
                catchup_on_startup: false,
                disabled: false,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"cron_add\""), "got: {s}");
        assert!(s.contains("\"target\":\"reviewer\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::CronAdd {
                target,
                kind,
                payload,
                schedule,
                tz,
                oneoff,
                catchup_on_startup,
                disabled,
            } => {
                assert_eq!(target, "reviewer");
                assert_eq!(kind, "message");
                assert_eq!(payload, "ping");
                assert_eq!(schedule, "0 9 * * *");
                assert_eq!(tz.as_deref(), Some("local"));
                assert!(!oneoff);
                assert!(!catchup_on_startup);
                assert!(!disabled);
            }
            other => panic!("expected CronAdd, got {other:?}"),
        }
    }

    #[test]
    fn cron_add_omits_optional_tz_when_none() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::CronAdd {
                target: "a".into(),
                kind: "message".into(),
                payload: "b".into(),
                schedule: "0 9 * * *".into(),
                tz: None,
                oneoff: false,
                catchup_on_startup: false,
                disabled: false,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(!s.contains("\"tz\""), "tz should be omitted: {s}");
    }

    #[test]
    fn cron_list_roundtrips() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::CronList {
                target: Some("reviewer".into()),
                include_disabled: true,
                mine: false,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"cron_list\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::CronList {
                include_disabled: true,
                mine: false,
                ..
            }
        ));
    }

    #[test]
    fn cron_list_json_roundtrips() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::CronListJson {
                target: None,
                include_disabled: false,
                mine: true,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"cron_list_json\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::CronListJson {
                target: None,
                include_disabled: false,
                mine: true,
            }
        ));
    }

    #[test]
    fn cron_show_remove_roundtrip() {
        let id = Uuid::new_v4();
        for (variant, tag) in [
            (RequestCmd::CronShow { id }, "cron_show"),
            (RequestCmd::CronRemove { id }, "cron_remove"),
        ] {
            let req = Request {
                agent_id: Uuid::nil(),
                cmd: variant.clone(),
            };
            let s = serde_json::to_string(&req).unwrap();
            assert!(
                s.contains(&format!("\"cmd\":\"{tag}\"")),
                "missing tag {tag} in {s}"
            );
            let back: Request = serde_json::from_str(&s).unwrap();
            match (variant, back.cmd) {
                (RequestCmd::CronShow { id: a }, RequestCmd::CronShow { id: b }) => {
                    assert_eq!(a, b)
                }
                (RequestCmd::CronRemove { id: a }, RequestCmd::CronRemove { id: b }) => {
                    assert_eq!(a, b)
                }
                (e, g) => panic!("mismatch: {e:?} vs {g:?}"),
            }
        }
    }

    #[test]
    fn cron_update_roundtrips_with_partial_fields() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::CronUpdate {
                id,
                target: None,
                kind: None,
                payload: Some("revised".into()),
                schedule: Some("*/5 * * * *".into()),
                tz: None,
                oneoff: None,
                catchup_on_startup: None,
                enabled: Some(false),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"cron_update\""), "got: {s}");
        assert!(s.contains("\"payload\":\"revised\""), "got: {s}");
        assert!(
            !s.contains("\"target\""),
            "None target should be omitted: {s}"
        );
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::CronUpdate {
                id: parsed,
                payload,
                schedule,
                enabled,
                ..
            } => {
                assert_eq!(parsed, id);
                assert_eq!(payload.as_deref(), Some("revised"));
                assert_eq!(schedule.as_deref(), Some("*/5 * * * *"));
                assert_eq!(enabled, Some(false));
            }
            other => panic!("expected CronUpdate, got {other:?}"),
        }
    }

    #[test]
    fn skill_run_roundtrips_with_snake_case_tag() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::SkillRun {
                name: "code-review".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"skill_run\""), "got: {s}");
        assert!(s.contains("\"name\":\"code-review\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::SkillRun { name } => assert_eq!(name, "code-review"),
            other => panic!("expected SkillRun, got {other:?}"),
        }
    }

    #[test]
    fn skill_list_roundtrips_with_snake_case_tag() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::SkillList,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"skill_list\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(back.cmd, RequestCmd::SkillList));
    }

    #[test]
    fn skill_search_roundtrips_with_snake_case_tag() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::SkillSearch {
                query: "review".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"skill_search\""), "got: {s}");
        assert!(s.contains("\"query\":\"review\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::SkillSearch { query } => assert_eq!(query, "review"),
            other => panic!("expected SkillSearch, got {other:?}"),
        }
    }

    #[test]
    fn response_helpers() {
        let ok = Response::ok("hello");
        assert!(ok.ok);
        assert_eq!(ok.message.as_deref(), Some("hello"));
        assert!(ok.error.is_none());

        let err = Response::err("nope");
        assert!(!err.ok);
        assert_eq!(err.error.as_deref(), Some("nope"));
        assert!(err.message.is_none());
    }
}
