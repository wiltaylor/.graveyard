use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;
use uuid::Uuid;

use super::event::AIEvent;

#[async_trait]
pub trait AICore: Send + Sync {
    /// Run a single conversational turn. The returned stream yields events
    /// (text, tool calls, tool results, errors) until the turn completes.
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError>;

    /// The session id this core is bound to, if one has been assigned.
    fn session_id(&self) -> Option<Uuid>;

    /// Name of the built-in tool that gives this core shell access, so
    /// the TUI can decide whether the agent can invoke the aciddog CLI.
    /// `None` means the core has no shell concept — agents using it
    /// must always use tag-based IPC.
    fn shell_tool_name(&self) -> Option<&'static str> {
        None
    }

    /// Rendered first-turn prompt — used by the TUI to seed the
    /// AgentInfo block so the user can see exactly what the model
    /// receives. Default empty for cores that have no structured
    /// preamble.
    fn first_turn_prompt(&self) -> String {
        String::new()
    }

    /// Configured system-prompt content for cores that accept one
    /// (Claude Code's `--system-prompt-file`, Anthropic API's `system`
    /// field, OpenAI's `role: "system"` message, ChatGPT's developer
    /// preamble). Cores without a system-prompt concept return
    /// `None`. Used to record the prompt at the head of each
    /// session's on-disk transcript so the log is self-contained.
    fn system_prompt(&self) -> Option<&str> {
        None
    }
}

#[derive(Debug, Error)]
pub enum AIError {
    #[error("failed to spawn claude CLI: {0}")]
    Spawn(#[source] std::io::Error),

    #[error("claude CLI not found on PATH")]
    NotFound,

    #[error("invalid tool name `{0}` (must be non-empty, no commas or whitespace)")]
    InvalidToolName(String),

    #[error("failed to prepare system prompt file: {0}")]
    SystemPromptIo(#[source] std::io::Error),

    #[error("failed to write prompt to claude stdin: {0}")]
    StdinWrite(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[derive(Default)]
    struct MockCore {
        id: Option<Uuid>,
    }

    #[async_trait]
    impl AICore for MockCore {
        async fn turn(&mut self, _prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
            Ok(futures::stream::empty().boxed())
        }
        fn session_id(&self) -> Option<Uuid> {
            self.id
        }
    }

    #[test]
    fn ai_core_is_object_safe() {
        let _boxed: Box<dyn AICore> = Box::new(MockCore::default());
    }
}
