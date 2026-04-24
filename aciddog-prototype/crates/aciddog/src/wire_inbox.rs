//! Serde-friendly projection of [`crate::tui::InboxMessage`].
//!
//! `InboxMessage::received_at` is a `std::time::Instant`, which is
//! opaque and explicitly non-serializable. For the wire we convert it
//! to a Unix-epoch seconds float at serialization time — the browser
//! turns it back into a `Date` via `new Date(received_at * 1000)`.
//!
//! Canonical definition of the shape is here; the manager crate
//! duplicates it as `aciddog-manager/src/wire_inbox.rs` (same drift
//! caveat as `wire_chat.rs`).

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::tui::InboxMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireInboxMessage {
    pub from: String,
    pub body: String,
    /// Unix-epoch seconds with sub-second precision. Derived from the
    /// `Instant` at serialize time — there's no stable "Instant → wall
    /// time" conversion, so we reconstruct by subtracting the
    /// elapsed duration from `SystemTime::now()`.
    pub received_at: f64,
}

impl WireInboxMessage {
    pub fn project(msg: &InboxMessage, now: Instant) -> Self {
        let elapsed = now.saturating_duration_since(msg.received_at);
        let wall_now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let received = wall_now.saturating_sub(elapsed);
        Self {
            from: msg.from.clone(),
            body: msg.body.clone(),
            received_at: received.as_secs_f64(),
        }
    }
}

impl From<&InboxMessage> for WireInboxMessage {
    fn from(msg: &InboxMessage) -> Self {
        Self::project(msg, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_body_and_from() {
        let src = InboxMessage::new("planner", "hello operator");
        let wire = WireInboxMessage::from(&src);
        let json = serde_json::to_value(&wire).unwrap();
        assert_eq!(json["from"], "planner");
        assert_eq!(json["body"], "hello operator");
        let ts = json["received_at"].as_f64().unwrap();
        assert!(ts > 0.0, "received_at should be a positive unix timestamp");
    }
}
