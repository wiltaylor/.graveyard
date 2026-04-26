//! Minimal subset of the aciddog agent-IPC wire types that the manager
//! needs to speak to a :webservice instance's `/v1/rpc` endpoint.
//!
//! We can't depend on the `aciddog` crate directly because it transitively
//! links `sqlite3` via cozo, which conflicts with sqlx-sqlite's
//! libsqlite3-sys (`links = "sqlite3"`). Shapes duplicated from
//! `crates/aciddog/src/agent_ipc/mod.rs` must stay in lockstep by
//! hand. The proper fix is to split `aciddog::agent_ipc` out into
//! `aciddog-ipc-types` and depend on that from both sides.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wire-level request body for `POST /v1/rpc`. Variants are serialized
/// with `#[serde(tag = "cmd", rename_all = "snake_case")]`, matching
/// `aciddog::agent_ipc::RequestCmd`. The webservice stamps its own
/// `agent_id` server-side — we don't send one.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum RequestCmd {
    AgentList,
    /// Structured chat-history snapshot. Response carries a
    /// `Vec<WireChatBlock>` in `IpcResponse::data`.
    AgentHistoryJson {
        agent: String,
        #[serde(flatten)]
        selector: HistorySelector,
    },
    /// Deliver `body` to `to` as if the human operator typed it into
    /// the TUI pane. Supervisor-only on the remote side — fails with
    /// `error: "not a supervisor"` if the webservice automaton isn't
    /// flagged `supervisor = true`.
    AgentSendAsUser {
        to: String,
        body: String,
    },
    // ---- Workflows -------------------------------------------------
    /// Start a new instance of a declared workflow. `properties` is a
    /// map of property-id → JSON value; each is coerced against the
    /// declared `PropertyKind` on the daemon side.
    WorkflowRun {
        workflow: String,
        #[serde(default)]
        properties: std::collections::BTreeMap<String, serde_json::Value>,
    },
    /// Supervisor: declared workflow library with enough property
    /// metadata to render a portal form. Response carries
    /// `Vec<wire_workflow::WireWorkflowTemplate>` in `IpcResponse::data`.
    WorkflowTemplatesJson,
    /// Supervisor: live snapshot of the daemon's running + failed +
    /// lingering instances. Response carries
    /// `Vec<wire_workflow::WireWorkflowInstance>` in `IpcResponse::data`.
    WorkflowInstancesJson,
    /// Supervisor: drop a failed workflow instance. Running instances
    /// cannot be dismissed (the daemon rejects with an error).
    WorkflowDismiss {
        instance_id: u64,
    },
    // ---- Tasks -----------------------------------------------------
    /// Supervisor structured task list. Response carries
    /// `Vec<wire_tasks::Task>` in `IpcResponse::data`.
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
    TaskAdd {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        blocked_by: Vec<Uuid>,
    },
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
    TaskRemove {
        id: Uuid,
    },
    TaskNote {
        id: Uuid,
        body: String,
    },
    TaskTodoAdd {
        task: Uuid,
        body: String,
    },
    TaskTodoUpdate {
        task: Uuid,
        id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    TaskTodoRemove {
        task: Uuid,
        id: Uuid,
    },
    TaskCommentAdd {
        task: Uuid,
        body: String,
    },
    TaskCommentUpdate {
        task: Uuid,
        id: Uuid,
        body: String,
    },
    TaskCommentRemove {
        task: Uuid,
        id: Uuid,
    },
    // ---- Team ------------------------------------------------------
    /// Supervisor snapshot of the team + AI core config. Response
    /// carries `wire_team::WireTeamInfo` in `data`.
    TeamInfo,
    /// Sysadmin: re-read the WCL config and reconcile the running
    /// fleet. Response message carries the reload summary on success.
    /// Requires `sysadmin = true` on the manager-portal automaton.
    ServerReload,
    /// Sysadmin: read the raw WCL source from the daemon's config path.
    /// Response carries `{ wcl, path }` in `data`. Requires
    /// `sysadmin = true` on the manager-portal automaton.
    TeamRead,
    /// Sysadmin: replace the on-disk WCL config with the supplied source
    /// and reconcile the running fleet. The daemon validates the WCL
    /// before touching disk; the response carries the reload summary on
    /// success. Requires `sysadmin = true` on the manager-portal automaton.
    TeamWrite {
        wcl: String,
    },
    // ---- Memory ----------------------------------------------------
    MemoryBanks,
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
    MemoryStoreJson {
        bank: String,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    MemoryUpdate {
        bank: String,
        id: Uuid,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
    },
    MemoryRemove {
        bank: String,
        id: Uuid,
    },
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
    MemoryLink {
        bank: String,
        from: Uuid,
        to: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    MemoryUnlink {
        bank: String,
        from: Uuid,
        to: Uuid,
    },
    MemoryGraph {
        bank: String,
    },
    // ---- Channels --------------------------------------------------
    ChannelList,
    ChannelHistory {
        channel: String,
    },
    ChannelSendAsUser {
        channel: String,
        body: String,
    },
    // ---- Inbox -----------------------------------------------------
    /// Supervisor snapshot of the human-operator inbox. Response
    /// carries `Vec<wire_inbox::WireInboxMessage>` in `data`.
    InboxList,
    /// Supervisor delete by list index.
    InboxDelete {
        index: usize,
    },
    // ---- Cron ------------------------------------------------------
    /// Structured cron list. Response carries
    /// `Vec<wire_cron::WireCronJob>` in `IpcResponse::data`.
    CronListJson {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default)]
        include_disabled: bool,
        #[serde(default)]
        mine: bool,
    },
    /// Create a new cron job. Shape mirrors
    /// `aciddog::agent_ipc::RequestCmd::CronAdd` byte-for-byte.
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
    /// Partial update. Omitted fields are left unchanged.
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
    CronRemove {
        id: Uuid,
    },
    // ---- Skills ----------------------------------------------------
    /// Sysadmin: list every skill directory under the daemon's
    /// `.aciddog/skills/`. Distinct from the agent-scoped `SkillList`
    /// which returns the caller's allowlist. Response carries
    /// `Vec<SkillSummaryWire>` in `IpcResponse::data`.
    SkillsList,
    /// Sysadmin: read every file in a skill directory. Response carries
    /// a `SkillContentsWire { name, files }` in `IpcResponse::data`.
    SkillRead {
        name: String,
    },
    /// Sysadmin: atomically replace the named skill with the supplied
    /// files. Empty `files` is rejected by the daemon.
    SkillWrite {
        name: String,
        files: Vec<SkillFile>,
    },
    /// Sysadmin: delete a skill directory. Idempotent.
    SkillRemove {
        name: String,
    },
}

/// One file inside a skill directory. Matches
/// `aciddog::agent_ipc::SkillFile` byte-for-byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub rel_path: String,
    pub contents_b64: String,
}

/// Which slice of a target agent's chat log to return from
/// `AgentHistoryJson`. Serialized with an internally-tagged `select`
/// discriminator — mirrors `aciddog::agent_ipc::HistorySelector`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "select", rename_all = "snake_case")]
pub enum HistorySelector {
    All,
    Last { n: usize },
    LastMessage,
    Kind { kind: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Structured payload for commands whose reply doesn't fit neatly
    /// in `message`. Populated by `AgentHistoryJson`; `None` for every
    /// other variant in this crate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_list_serializes_as_expected() {
        let json = serde_json::to_value(&RequestCmd::AgentList).unwrap();
        assert_eq!(json, serde_json::json!({ "cmd": "agent_list" }));
    }

    #[test]
    fn response_round_trips() {
        let wire = r#"{"ok":true,"message":"hello"}"#;
        let parsed: IpcResponse = serde_json::from_str(wire).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.message.as_deref(), Some("hello"));
        assert!(parsed.error.is_none());
        assert!(parsed.data.is_none());
    }

    #[test]
    fn response_data_round_trips() {
        let wire = r#"{"ok":true,"data":[1,2,3]}"#;
        let parsed: IpcResponse = serde_json::from_str(wire).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.data, Some(serde_json::json!([1, 2, 3])));
    }

    #[test]
    fn agent_history_json_serializes_with_selector() {
        let cmd = RequestCmd::AgentHistoryJson {
            agent: "supervisor".into(),
            selector: HistorySelector::All,
        };
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v["cmd"], "agent_history_json");
        assert_eq!(v["agent"], "supervisor");
        assert_eq!(v["select"], "all");
    }

    #[test]
    fn cron_list_json_serializes_as_expected() {
        let json = serde_json::to_value(&RequestCmd::CronListJson {
            target: Some("reviewer".into()),
            include_disabled: true,
            mine: false,
        })
        .unwrap();
        assert_eq!(json["cmd"], "cron_list_json");
        assert_eq!(json["target"], "reviewer");
        assert_eq!(json["include_disabled"], true);
        assert_eq!(json["mine"], false);
    }

    #[test]
    fn cron_add_omits_optional_tz_when_none() {
        let cmd = RequestCmd::CronAdd {
            target: "reviewer".into(),
            kind: "message".into(),
            payload: "ping".into(),
            schedule: "*/5 * * * *".into(),
            tz: None,
            oneoff: false,
            catchup_on_startup: false,
            disabled: false,
        };
        let s = serde_json::to_string(&cmd).unwrap();
        assert!(s.contains("\"cmd\":\"cron_add\""), "got: {s}");
        assert!(!s.contains("\"tz\""), "tz should be omitted: {s}");
    }

    #[test]
    fn cron_remove_round_trips() {
        let id = Uuid::new_v4();
        let s = serde_json::to_string(&RequestCmd::CronRemove { id }).unwrap();
        assert!(s.contains("\"cmd\":\"cron_remove\""), "got: {s}");
        assert!(s.contains(&id.to_string()), "got: {s}");
    }

    #[test]
    fn agent_send_as_user_serializes_with_to_and_body() {
        let cmd = RequestCmd::AgentSendAsUser {
            to: "planner".into(),
            body: "ping".into(),
        };
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v["cmd"], "agent_send_as_user");
        assert_eq!(v["to"], "planner");
        assert_eq!(v["body"], "ping");
    }
}
