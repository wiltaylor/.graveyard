//! Deserialize/serialize mirror of
//! `aciddog::wire_channel::{WireChannelInfo, WireChannelMessage, ChannelMessageEvent}`.
//!
//! Same duplication rationale as the other `wire_*.rs` modules —
//! canonical shape is in `crates/aciddog/src/wire_channel.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireChannelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub welcome: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
    pub message_count: usize,
    #[serde(default)]
    pub session_hash: String,
    #[serde(default)]
    pub session_started: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireChannelMessage {
    pub sender: String,
    pub body: String,
    pub is_welcome: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_round_trips() {
        let wire = r#"{"id":"general","members":["planner","coder"],"message_count":3}"#;
        let info: WireChannelInfo = serde_json::from_str(wire).unwrap();
        assert_eq!(info.id, "general");
        assert_eq!(info.members, vec!["planner", "coder"]);
        assert_eq!(info.message_count, 3);
    }

    #[test]
    fn message_round_trips_welcome_flag() {
        let wire = r#"{"sender":"user","body":"hi","is_welcome":false}"#;
        let msg: WireChannelMessage = serde_json::from_str(wire).unwrap();
        assert_eq!(msg.sender, "user");
        assert!(!msg.is_welcome);
    }
}
