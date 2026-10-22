//! OpenAI ChatGPT-subscription AI core. Speaks the Responses API
//! (`/responses`) against `chatgpt.com/backend-api/codex` using OAuth
//! tokens stored by `crate::openai_auth`.
//!
//! This is **not** the same wire format as the generic `OpenAiApi`
//! core: the endpoint, headers, request body, and SSE event shape all
//! differ. See the plan in
//! `/home/wil/.claude/plans/i-want-to-add-effervescent-moon.md` for
//! the full protocol notes.

pub mod prompts;
pub mod responses;
pub mod sse;
pub mod tools;

mod core;

pub use core::{OpenAiChatgptCore, MAX_NATIVE_TOOL_ROUNDS};
