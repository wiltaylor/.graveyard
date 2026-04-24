//! Serde-friendly projection of the TUI's [`ChatBlock`] enum.
//!
//! The in-memory `ChatBlock` at `tui/blocks.rs` carries rendering-layer
//! types (ratatui `Text`/`Line`/`Span` via `ParsedMd`, buffer caches,
//! click-hit `Rect`s) that have no business on a wire. This module is
//! the shape we serialize out over `/v1/rpc` (as `AgentHistoryJson`)
//! and over the `/v1/chat/events` SSE stream — a flat enum mirroring
//! every `ChatBlock` variant with only user-visible fields.
//!
//! The `From<&ChatBlock> for WireChatBlock` impl is compiler-enforced
//! exhaustive: any new `ChatBlock` variant will break this build
//! before it can silently drop off the wire.

use serde::{Deserialize, Serialize};

use crate::tui::{ChatBlock, MailDirection, SystemKind, TaskOpKind};

/// Mirror of `ChatBlock` with everything render-layer stripped.
///
/// Variants are tagged with `"kind"` and encoded in `snake_case` so the
/// JSON shape is self-describing on the SSE stream. Fields with
/// identical semantics keep identical names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireChatBlock {
    AgentInfo {
        core: String,
        tools: Vec<String>,
        prompt: String,
        /// `TurnUsage` flattened to plain counters so this wire type
        /// doesn't depend on `ai::TurnUsage` being serde-derived.
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

impl From<SystemKind> for WireSystemKind {
    fn from(k: SystemKind) -> Self {
        match k {
            SystemKind::Info => Self::Info,
            SystemKind::Warn => Self::Warn,
            SystemKind::Error => Self::Error,
        }
    }
}

impl From<MailDirection> for WireMailDirection {
    fn from(d: MailDirection) -> Self {
        match d {
            MailDirection::Sent => Self::Sent,
            MailDirection::Received => Self::Received,
        }
    }
}

impl From<TaskOpKind> for WireTaskOpKind {
    fn from(k: TaskOpKind) -> Self {
        match k {
            TaskOpKind::Add => Self::Add,
            TaskOpKind::List => Self::List,
            TaskOpKind::Show => Self::Show,
            TaskOpKind::Update => Self::Update,
            TaskOpKind::Remove => Self::Remove,
            TaskOpKind::Take => Self::Take,
            TaskOpKind::Note => Self::Note,
            TaskOpKind::TodoAdd => Self::TodoAdd,
            TaskOpKind::TodoList => Self::TodoList,
            TaskOpKind::TodoUpdate => Self::TodoUpdate,
            TaskOpKind::TodoRemove => Self::TodoRemove,
            TaskOpKind::CommentAdd => Self::CommentAdd,
            TaskOpKind::CommentList => Self::CommentList,
            TaskOpKind::CommentUpdate => Self::CommentUpdate,
            TaskOpKind::CommentRemove => Self::CommentRemove,
        }
    }
}

impl From<&ChatBlock> for WireChatBlock {
    fn from(b: &ChatBlock) -> Self {
        match b {
            ChatBlock::AgentInfo {
                core,
                tools,
                prompt,
                context_used,
                context_window,
                ..
            } => Self::AgentInfo {
                core: core.clone(),
                tools: tools.clone(),
                prompt: prompt.clone(),
                context_input_tokens: context_used.map(|u| u.input_tokens),
                context_output_tokens: context_used.map(|u| u.output_tokens),
                context_cache_read_tokens: context_used.map(|u| u.cache_read_input_tokens),
                context_cache_creation_tokens: context_used.map(|u| u.cache_creation_input_tokens),
                context_window: *context_window,
            },
            ChatBlock::UserText { text } => Self::UserText { text: text.clone() },
            ChatBlock::AiMarkdown { raw, .. } => Self::AiMarkdown { raw: raw.clone() },
            ChatBlock::BashTool {
                command,
                stdout,
                is_error,
                pending,
                ..
            } => Self::BashTool {
                command: command.clone(),
                stdout: stdout.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::EditTool {
                file_path,
                diff,
                is_error,
                pending,
                ..
            } => Self::EditTool {
                file_path: file_path.clone(),
                diff: diff.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::WriteTool {
                file_path,
                diff,
                is_error,
                pending,
                ..
            } => Self::WriteTool {
                file_path: file_path.clone(),
                diff: diff.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::ReadTool {
                file_path,
                range,
                is_error,
                pending,
            } => Self::ReadTool {
                file_path: file_path.clone(),
                range: range.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::GrepTool {
                pattern,
                path,
                matches,
                is_error,
                pending,
                ..
            } => Self::GrepTool {
                pattern: pattern.clone(),
                path: path.clone(),
                matches: matches.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::GlobTool {
                pattern,
                path,
                matches,
                is_error,
                pending,
                ..
            } => Self::GlobTool {
                pattern: pattern.clone(),
                path: path.clone(),
                matches: matches.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::GenericTool {
                name,
                input_pretty,
                result_pretty,
                is_error,
                pending,
                ..
            } => Self::GenericTool {
                name: name.clone(),
                input_pretty: input_pretty.clone(),
                result_pretty: result_pretty.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::System { text, kind } => Self::System {
                text: text.clone(),
                kind: (*kind).into(),
            },
            ChatBlock::Mail {
                direction,
                counterpart,
                body,
                is_error,
                pending,
                ..
            } => Self::Mail {
                direction: (*direction).into(),
                counterpart: counterpart.clone(),
                body: body.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::WorkerSpawn {
                template,
                worker_id,
                task,
                is_error,
                pending,
                ..
            } => Self::WorkerSpawn {
                template: template.clone(),
                worker_id: worker_id.clone(),
                task: task.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::WorkerComplete {
                body,
                is_error,
                pending,
                ..
            } => Self::WorkerComplete {
                body: body.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::ChannelMessage {
                sender,
                body,
                is_welcome,
            } => Self::ChannelMessage {
                sender: sender.clone(),
                body: body.clone(),
                is_welcome: *is_welcome,
            },
            ChatBlock::RemotePatch {
                summary,
                diff,
                is_error,
                pending,
                ..
            } => Self::RemotePatch {
                summary: summary.clone(),
                diff: diff.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::TaskOp {
                kind,
                subject,
                body,
                reply,
                is_error,
                pending,
                ..
            } => Self::TaskOp {
                kind: (*kind).into(),
                subject: subject.clone(),
                body: body.clone(),
                reply: reply.clone(),
                is_error: *is_error,
                pending: *pending,
            },
            ChatBlock::AutomatonStdout { line } => Self::AutomatonStdout { line: line.clone() },
            ChatBlock::AutomatonStderr { line } => Self::AutomatonStderr { line: line.clone() },
        }
    }
}

/// Envelope sent on the `/v1/chat/events?agent=<id>` SSE stream and
/// queued in the per-`App` block broadcaster. `seq` is the block's
/// zero-based index in the target chat at the time it was appended,
/// useful for detecting gaps on reconnect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatBlockEvent {
    pub agent: String,
    pub block: WireChatBlock,
    pub ts: String,
    pub seq: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::ChatBlock;

    #[test]
    fn user_text_round_trip() {
        let src = ChatBlock::UserText {
            text: "hello".into(),
        };
        let wire = WireChatBlock::from(&src);
        let json = serde_json::to_value(&wire).unwrap();
        assert_eq!(json["kind"], "user_text");
        assert_eq!(json["text"], "hello");
    }

    #[test]
    fn bash_tool_preserves_semantics_drops_collapsed() {
        let src = ChatBlock::BashTool {
            command: "ls".into(),
            stdout: "a\nb".into(),
            is_error: true,
            collapsed: true,
            pending: false,
        };
        let wire = WireChatBlock::from(&src);
        let json = serde_json::to_value(&wire).unwrap();
        assert_eq!(json["kind"], "bash_tool");
        assert_eq!(json["command"], "ls");
        assert_eq!(json["stdout"], "a\nb");
        assert_eq!(json["is_error"], true);
        assert_eq!(json["pending"], false);
        assert!(json.get("collapsed").is_none());
    }

    #[test]
    fn mail_direction_serializes_snake_case() {
        let src = ChatBlock::Mail {
            direction: MailDirection::Sent,
            counterpart: "worker-1".into(),
            body: "ping".into(),
            is_error: false,
            collapsed: false,
            pending: false,
        };
        let wire = WireChatBlock::from(&src);
        let json = serde_json::to_value(&wire).unwrap();
        assert_eq!(json["direction"], "sent");
    }

    #[test]
    fn system_kind_roundtrips() {
        let src = ChatBlock::System {
            text: "boom".into(),
            kind: SystemKind::Error,
        };
        let wire = WireChatBlock::from(&src);
        let json = serde_json::to_value(&wire).unwrap();
        // Outer discriminator wins the `kind` slot; inner renames to `level`.
        assert_eq!(json["kind"], "system");
        assert_eq!(json["level"], "error");
        assert_eq!(json["text"], "boom");
    }

    #[test]
    fn chat_block_event_serialises_compactly() {
        let ev = ChatBlockEvent {
            agent: "supervisor".into(),
            block: WireChatBlock::UserText {
                text: "hi".into(),
            },
            ts: "2026-04-24T12:34:56Z".into(),
            seq: 7,
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["agent"], "supervisor");
        assert_eq!(v["block"]["kind"], "user_text");
        assert_eq!(v["seq"], 7);
    }
}
