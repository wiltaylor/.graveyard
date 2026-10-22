/// Describes a detected idle condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdleSituation {
    /// The agent that appears idle.
    pub agent_id: String,
    /// How long the agent has been idle (wall-clock seconds).
    pub idle_seconds: u64,
    /// Optional free-form context tags (e.g. "last_status:waiting").
    pub tags: Vec<String>,
}
