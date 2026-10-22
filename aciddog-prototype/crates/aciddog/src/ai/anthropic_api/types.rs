//! Wire types for the Anthropic Messages API. These are minimal — just
//! enough shape for the core to serialise conversation history into a
//! request and to deserialise the bits of the SSE event stream it
//! actually acts on. Anything the model sends that we don't recognise
//! rides through as `serde_json::Value` in the event layer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

/// Tagged union matching Anthropic's content-block schema. Every
/// variant the core reads from or writes to the wire has a
/// representation here; anything else the model emits (extended
/// thinking, redacted content, etc.) is dropped at the SSE decode
/// layer since we don't need it for this v1 scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}
