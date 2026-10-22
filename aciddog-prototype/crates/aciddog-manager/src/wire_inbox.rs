//! Deserialize/serialize mirror of `aciddog::wire_inbox::WireInboxMessage`.
//!
//! Same duplication rationale as [`super::wire_chat`]. Canonical
//! definition: `crates/aciddog/src/wire_inbox.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireInboxMessage {
    pub from: String,
    pub body: String,
    /// Unix-epoch seconds with sub-second precision. JS turns this
    /// into a `Date` via `new Date(received_at * 1000)`.
    pub received_at: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_ts() {
        let wire = r#"{"from":"planner","body":"hi","received_at":1714000000.5}"#;
        let msg: WireInboxMessage = serde_json::from_str(wire).unwrap();
        assert_eq!(msg.from, "planner");
        assert!((msg.received_at - 1714000000.5).abs() < 1e-6);
    }
}
