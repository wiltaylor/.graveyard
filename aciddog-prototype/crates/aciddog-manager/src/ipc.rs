//! Minimal subset of the aciddog agent-IPC wire types that the manager
//! needs to speak to a :webservice instance's `/v1/rpc` endpoint.
//!
//! We can't depend on the `aciddog` crate directly because it transitively
//! links `sqlite3` via cozo, which conflicts with sqlx-sqlite's
//! libsqlite3-sys (`links = "sqlite3"`). For v1 we only speak
//! `agent_list`, so duplicating these two tiny shapes is cheaper than
//! extracting a shared types crate. If future work needs richer RPC, the
//! right move is to split `aciddog::agent_ipc` out into
//! `aciddog-ipc-types` and depend on that from both sides.
//!
//! Shapes mirror `crates/aciddog/src/agent_ipc/mod.rs`.

use serde::{Deserialize, Serialize};

/// Wire-level request body for `POST /v1/rpc`. Variants are serialized
/// with `#[serde(tag = "cmd", rename_all = "snake_case")]`, matching
/// `aciddog::agent_ipc::RequestCmd`. The webservice stamps its own
/// `agent_id` server-side — we don't send one.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum RequestCmd {
    AgentList,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    }
}
