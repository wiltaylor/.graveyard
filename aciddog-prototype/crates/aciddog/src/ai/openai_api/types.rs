//! Wire types for the OpenAI chat-completions API. Deliberately small —
//! the core serialises history into JSON manually (see `build_request_body`
//! in `mod.rs`) so we can guarantee `"content": null` on assistant
//! messages that only emit tool calls. `serde`'s `skip_serializing_if`
//! would drop the field entirely, which some vendors reject.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One conversation turn as stored in `history`. Assistant messages may
/// carry `tool_calls` and no `content`; tool-result messages carry
/// `tool_call_id` and `content` and nothing else. User/system messages
/// carry just `content`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

/// Assistant-emitted call to one of the advertised function tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// Always `"function"` today; carried through verbatim so future
    /// variants (e.g. `"code_interpreter"`) round-trip cleanly.
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON-encoded string per the OpenAI contract — not an object.
    /// The core parses this into a `serde_json::Value` only when
    /// constructing `AIEvent::ToolUse.input`; history keeps the raw
    /// string so round-trips preserve the model's exact payload.
    pub arguments: String,
}
