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
