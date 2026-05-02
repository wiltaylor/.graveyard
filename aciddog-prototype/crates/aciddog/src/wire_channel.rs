//! Serde-friendly projection of channel state — one info entry per
//! declared channel and a flat message list suitable for a web
//! transcript pane.
//!
//! Transcripts reuse `WireChatBlock::ChannelMessage` on the chat side;
//! this module only covers the metadata + standalone message event
//! used by the manager's Channels sidebar and live SSE stream.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireChannelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub welcome: Option<String>,
    pub members: Vec<String>,
    /// Count of non-welcome messages currently buffered in-memory —
    /// scope matches `ChannelState::blocks` minus any synthetic
    /// welcome banner.
    pub message_count: usize,
    /// 12-char hex truncation of `sha256(id || session_started)`.
    /// Identifies the on-disk JSONL file at
    /// `.aciddog/chat/<id>/<session_hash>.jsonl` and is rendered in
    /// the channel header. Empty for instances that predate session
    /// hashing (web client should treat empty as "unknown").
    #[serde(default)]
    pub session_hash: String,
    /// RFC3339 UTC timestamp of when the current session began.
    /// Empty for instances that predate session hashing.
    #[serde(default)]
    pub session_started: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireChannelMessage {
    /// `"user"` for human-originated posts, agent id otherwise.
    /// Matches `ChannelSender::label()` on the wire.
    pub sender: String,
    pub body: String,
    pub is_welcome: bool,
}

/// Fan-out envelope for the per-channel SSE stream at
/// `/v1/channel/events?channel=<id>`. Appended every time
/// `App::broadcast_to_channel` emits a new message, regardless of
/// whether the webservice automaton is a member of the channel —
/// supervisor scope lets the portal follow any channel it wants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessageEvent {
    pub channel: String,
    pub sender: String,
    pub body: String,
    pub ts: String,
    /// Zero-based index of this message in the channel's transcript
    /// at emit time (welcome banner excluded).
    pub seq: u64,
}
