//! OpenAI **Responses API** request body + item types for the
//! ChatGPT-subscription endpoint. Non-obvious invariants enforced by
//! the backend (all 400-triggering if skipped):
//!
//!   - `store` must be literally `false` (stateless endpoint);
//!   - `stream` must be `true` (SSE only);
//!   - every `ResponseItem` in `input[]` must have its `id` stripped;
//!   - `item_reference` items must not appear;
//!   - every `function_call_output` must share a request with its
//!     originating `function_call` (never truncate history between).
//!
//! `build_request_body()` and `sanitise_for_stateless()` below handle
//! (1)-(4); the caller is responsible for (5) — the core's history
//! only grows within a turn, so as long as we don't drop items the
//! invariant holds.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::prompts;

/// One item in the `input[]` array. We keep the envelope loose so we
/// can round-trip items the server finalises for us (reasoning,
/// messages, custom_tool_call, …) without having to model every
/// variant ahead of time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseItem {
    /// A typed item we construct ourselves (developer prompts, user
    /// messages, function_call_output replies).
    Typed(Value),
    /// An item the server emitted (assistant messages, function_call,
    /// reasoning). We store the raw JSON and only sanitise on send.
    Raw(Value),
}

impl ResponseItem {
    pub fn as_value(&self) -> &Value {
        match self {
            ResponseItem::Typed(v) | ResponseItem::Raw(v) => v,
        }
    }
}

/// Build the top-level request body. `system_prompt` is the user's
/// aciddog-side system prompt + first-turn preamble; it goes into
/// `input[]` as a `role: "developer"` message, **not** into
/// `instructions` (which is always the bundled Codex prompt).
pub fn build_request_body(
    model: &str,
    system_prompt: Option<&str>,
    history: &[ResponseItem],
    tools: &[Value],
    reasoning_effort: &str,
    conversation_id: &str,
) -> Value {
    let mut input: Vec<Value> = Vec::with_capacity(history.len() + 1);
    if let Some(sp) = system_prompt.filter(|s| !s.trim().is_empty()) {
        input.push(json!({
            "type": "message",
            "role": "developer",
            "content": [
                { "type": "input_text", "text": sp }
            ],
        }));
    }
    for item in history {
        input.push(sanitise_for_stateless(item.as_value()));
    }

    let mut body = json!({
        "model": model,
        "instructions": prompts::instructions_for(model),
        "input": input,
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "reasoning": { "effort": reasoning_effort, "summary": "auto" },
        "store": false,
        "stream": true,
        "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": conversation_id,
    });
    if !tools.is_empty() {
        body.as_object_mut()
            .expect("object literal")
            .insert("tools".into(), Value::Array(tools.to_vec()));
    }
    body
}

/// Strip server-side `id` fields from a `ResponseItem` and drop
/// `item_reference` items by returning `Value::Null` (the caller
/// filters those out via `build_request_body` — we keep the signature
/// infallible to simplify call sites).
pub fn sanitise_for_stateless(item: &Value) -> Value {
    let mut clone = item.clone();
    if let Some(obj) = clone.as_object_mut() {
        // Drop item_reference entirely — replace with a no-op message
        // so we don't have to shuffle the outer array. Actually we
        // can't return null here because the caller inlines this, so
        // convert into a developer placeholder.
        if obj.get("type").and_then(Value::as_str) == Some("item_reference") {
            return json!({
                "type": "message",
                "role": "developer",
                "content": [{ "type": "input_text", "text": "" }],
            });
        }
        strip_ids(obj);
    }
    clone
}

fn strip_ids(obj: &mut serde_json::Map<String, Value>) {
    obj.remove("id");
    for v in obj.values_mut() {
        strip_ids_value(v);
    }
}

fn strip_ids_value(v: &mut Value) {
    match v {
        Value::Object(o) => strip_ids(o),
        Value::Array(arr) => {
            for el in arr.iter_mut() {
                strip_ids_value(el);
            }
        }
        _ => {}
    }
}

/// Helpers for constructing outgoing items — the only shapes the core
/// builds itself. Assistant / function_call / reasoning items come
/// from SSE `response.output_item.done` events and are stored as-is
/// in `Raw`.
pub mod new_item {
    use serde_json::{json, Value};

    pub fn user_message(text: &str) -> Value {
        json!({
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": text }],
        })
    }

    pub fn function_call_output(call_id: &str, output: &str) -> Value {
        json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": output,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitise_strips_top_level_ids() {
        let item = json!({
            "type": "message",
            "id": "msg_abc",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "hi" }],
        });
        let out = sanitise_for_stateless(&item);
        assert!(out.get("id").is_none());
        assert_eq!(out["type"], "message");
    }

    #[test]
    fn sanitise_strips_nested_ids_in_content_array() {
        let item = json!({
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "output_text", "id": "c_1", "text": "hi" },
                { "type": "output_text", "id": "c_2", "text": "there" },
            ],
        });
        let out = sanitise_for_stateless(&item);
        let arr = out["content"].as_array().unwrap();
        assert!(arr[0].get("id").is_none());
        assert!(arr[1].get("id").is_none());
    }

    #[test]
    fn sanitise_is_idempotent() {
        let item = json!({ "type": "message", "id": "x", "role": "user",
                            "content": [{ "type": "input_text", "text": "hi" }] });
        let once = sanitise_for_stateless(&item);
        let twice = sanitise_for_stateless(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn sanitise_replaces_item_reference() {
        let item = json!({ "type": "item_reference", "id": "ref_x" });
        let out = sanitise_for_stateless(&item);
        assert_eq!(out["type"], "message");
        assert_eq!(out["role"], "developer");
    }

    #[test]
    fn build_body_sets_required_invariants() {
        let body = build_request_body(
            "gpt-5.1-codex",
            Some("be helpful"),
            &[],
            &[],
            "medium",
            "conv-1",
        );
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
        assert_eq!(body["reasoning"]["effort"], "medium");
        assert_eq!(body["prompt_cache_key"], "conv-1");
        // Instructions is the bundled codex prompt, not the user text.
        assert!(body["instructions"]
            .as_str()
            .unwrap()
            .starts_with("You are Codex"));
        // User text lives in the first input item as a developer message.
        assert_eq!(body["input"][0]["role"], "developer");
        assert_eq!(
            body["input"][0]["content"][0]["text"].as_str().unwrap(),
            "be helpful"
        );
    }

    #[test]
    fn build_body_omits_empty_system_prompt() {
        let body = build_request_body(
            "gpt-5.1-codex",
            None,
            &[ResponseItem::Typed(new_item::user_message("hi"))],
            &[],
            "medium",
            "conv-1",
        );
        // First (and only) input is the user message, no developer slot.
        assert_eq!(body["input"][0]["role"], "user");
    }

    #[test]
    fn build_body_includes_tools_when_nonempty() {
        let tools = vec![json!({
            "type": "function",
            "name": "Read",
            "description": "read a file",
            "parameters": {},
            "strict": true,
        })];
        let body = build_request_body("gpt-5.1-codex", None, &[], &tools, "medium", "conv-1");
        assert_eq!(body["tools"], Value::Array(tools));
    }

    #[test]
    fn build_body_skips_tools_when_empty() {
        let body = build_request_body("gpt-5.1-codex", None, &[], &[], "medium", "conv-1");
        assert!(body.get("tools").is_none());
    }
}
