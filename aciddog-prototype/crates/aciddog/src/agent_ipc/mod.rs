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
        properties: Option<serde_json::Value>,
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
    /// Return a detailed rendering of a single task — full
    /// description, dependencies, and the complete note log.
    TaskShow {
        id: Uuid,
    },
    /// Atomically claim the oldest unassigned task for the caller.
    TaskTake,
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
    /// Sysadmin: begin graceful shutdown of the orchestrator. Idle
    /// agents are put to sleep; working agents are allowed to finish
    /// their current turn before the process exits.
    ServerShutdown,
    /// Sysadmin: exit immediately. Raises `should_quit` without
    /// waiting for in-flight turns. Data loss is possible if a
    /// working agent is mid-response.
    ServerExit,
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
                properties: None,
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
                properties: Some(serde_json::json!({"repo":"acme/foo","branch":"main"})),
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
                properties: None,
            },
        };
        let back: Request = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        match back.cmd {
            RequestCmd::WorkerCreate {
                template,
                task,
                properties,
            } => {
                assert_eq!(template, "r");
                assert_eq!(task, "t");
                assert!(properties.is_none());
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
    fn task_add_roundtrips() {
        let dep = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::TaskAdd {
                title: "fix bug".into(),
                description: Some("details".into()),
                assignee: Some("tester".into()),
                blocked_by: vec![dep],
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
            } => {
                assert_eq!(title, "fix bug");
                assert_eq!(description.as_deref(), Some("details"));
                assert_eq!(assignee.as_deref(), Some("tester"));
                assert_eq!(blocked_by, vec![dep]);
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
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(
            !s.contains("blocked_by"),
            "empty list should be omitted: {s}"
        );
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
                add_blocked_by,
                remove_blocked_by,
                ..
            } => {
                assert_eq!(parsed, id);
                assert_eq!(status.as_deref(), Some("done"));
                assert_eq!(assignee.as_deref(), Some("coder"));
                assert_eq!(note.as_deref(), Some("ready"));
                assert!(!unassign);
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
