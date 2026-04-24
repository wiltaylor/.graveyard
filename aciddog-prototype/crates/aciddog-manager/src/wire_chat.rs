//! Deserialize-capable mirror of `aciddog::wire_chat::WireChatBlock`.
//!
//! Same rationale as [`super::ipc`]: the manager can't depend on the
//! `aciddog` crate directly (sqlite3 link conflict with sqlx-sqlite),
//! so the wire shape is duplicated here. The canonical definition is
//! `crates/aciddog/src/wire_chat.rs` — any new `ChatBlock` variant
//! added there must be mirrored here too, or the manager will reject
//! the block with an unknown-variant error.
//!
//! Serde attrs match the canonical crate: outer tag `kind`, all-in
//! `snake_case`, inner fields that would collide with `kind` are
//! renamed (`System.level`, `TaskOp.op`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireChatBlock {
    AgentInfo {
        core: String,
        tools: Vec<String>,
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_output_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_cache_read_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_cache_creation_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_window: Option<u32>,
    },
    UserText {
        text: String,
    },
    AiMarkdown {
        raw: String,
    },
    BashTool {
        command: String,
        stdout: String,
        is_error: bool,
        pending: bool,
    },
    EditTool {
        file_path: String,
        diff: String,
        is_error: bool,
        pending: bool,
    },
    WriteTool {
        file_path: String,
        diff: String,
        is_error: bool,
        pending: bool,
    },
    ReadTool {
        file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<String>,
        is_error: bool,
        pending: bool,
    },
    GrepTool {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        matches: String,
        is_error: bool,
        pending: bool,
    },
    GlobTool {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        matches: String,
        is_error: bool,
        pending: bool,
    },
    GenericTool {
        name: String,
        input_pretty: String,
        result_pretty: String,
        is_error: bool,
        pending: bool,
    },
    System {
        text: String,
        #[serde(rename = "level")]
        kind: WireSystemKind,
    },
    Mail {
        direction: WireMailDirection,
        counterpart: String,
        body: String,
        is_error: bool,
        pending: bool,
    },
    WorkerSpawn {
        template: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<String>,
        task: String,
        is_error: bool,
        pending: bool,
    },
    WorkerComplete {
        body: String,
        is_error: bool,
        pending: bool,
    },
    ChannelMessage {
        sender: String,
        body: String,
        is_welcome: bool,
    },
    RemotePatch {
        summary: String,
        diff: String,
        is_error: bool,
        pending: bool,
    },
    TaskOp {
        #[serde(rename = "op")]
        kind: WireTaskOpKind,
        subject: String,
        body: String,
        reply: String,
        is_error: bool,
        pending: bool,
    },
    AutomatonStdout {
        line: String,
    },
    AutomatonStderr {
        line: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireSystemKind {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireMailDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireTaskOpKind {
    Add,
    List,
    Show,
    Update,
    Remove,
    Take,
    Note,
    TodoAdd,
    TodoList,
    TodoUpdate,
    TodoRemove,
    CommentAdd,
    CommentList,
    CommentUpdate,
    CommentRemove,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_round_trips() {
        let wire = r#"{"kind":"user_text","text":"hello"}"#;
        let parsed: WireChatBlock = serde_json::from_str(wire).unwrap();
        match parsed {
            WireChatBlock::UserText { text } => assert_eq!(text, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn bash_tool_round_trips() {
        let wire =
            r#"{"kind":"bash_tool","command":"ls","stdout":"a","is_error":false,"pending":false}"#;
        let parsed: WireChatBlock = serde_json::from_str(wire).unwrap();
        match parsed {
            WireChatBlock::BashTool {
                command, stdout, ..
            } => {
                assert_eq!(command, "ls");
                assert_eq!(stdout, "a");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn system_level_deserialises_from_renamed_field() {
        let wire = r#"{"kind":"system","text":"boom","level":"error"}"#;
        let parsed: WireChatBlock = serde_json::from_str(wire).unwrap();
        match parsed {
            WireChatBlock::System { text, kind } => {
                assert_eq!(text, "boom");
                assert_eq!(kind, WireSystemKind::Error);
            }
            _ => panic!("wrong variant"),
        }
    }
}
