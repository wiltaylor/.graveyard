//! NOTE: broadcast channel precedent: existing tui/app.rs AgentStatusEvent fan-out
//! at app.rs:27221-27307 uses tokio::sync::broadcast for multi-consumer events.
//! OwnerId follows the same pattern for per-owner WorkflowEvent fan-out.

/// Capacity for per-owner broadcast channels.
pub(super) const OWNER_BROADCAST_CAPACITY: usize = 64;

/// Typed identifier for a workflow owner.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OwnerId(pub String);

impl OwnerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub(crate) fn default_legacy() -> Self {
        Self("__legacy_default__".to_string())
    }
}

impl std::fmt::Display for OwnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for OwnerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for OwnerId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for OwnerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Minimal owner slot - max_in_flight is stored now for sibling 0456dbad;
/// events_tx exists so subscribe() works; actual fan-out wired by sibling 92bfc4dc.
pub(super) struct OwnerSlot {
    #[allow(dead_code)]
    pub(super) max_in_flight: Option<usize>,
    pub(super) events_tx: tokio::sync::broadcast::Sender<crate::workflow::WorkflowEvent>,
}

/// Handle returned to a caller who registered an owner.
/// NOTE: subscribers must handle RecvError::Lagged;
/// the formal contract + Stream adapter lands in sibling task 6ff9290c.
pub struct OwnerHandle {
    owner_id: OwnerId,
    slot: std::sync::Arc<OwnerSlot>,
}

impl OwnerHandle {
    pub(super) fn new(owner_id: OwnerId, slot: std::sync::Arc<OwnerSlot>) -> Self {
        Self { owner_id, slot }
    }

    pub fn owner_id(&self) -> &OwnerId {
        &self.owner_id
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<crate::workflow::WorkflowEvent> {
        self.slot.events_tx.subscribe()
    }
}
