/// A resolved, ready-to-execute action targeting an idle agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcreteAction {
    /// Send the agent a wake-up message.
    SendMessage { agent_id: String, body: String },
    /// Log the situation and take no further action.
    LogOnly { reason: String },
    /// Escalate to a named escalation target.
    Escalate { target: String, summary: String },
}
