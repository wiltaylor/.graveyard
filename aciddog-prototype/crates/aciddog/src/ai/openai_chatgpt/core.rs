//! The `OpenAiChatgptCore` — an `AICore` that talks to the ChatGPT
//! subscription endpoint (`chatgpt.com/backend-api/codex/responses`).
//!
//! Structure mirrors `OpenAiApiCore` (builder shape, first-turn
//! preamble, native tool-use loop, tag-chain outer loop) but the wire
//! protocol is different: Responses API instead of Chat Completions,
//! OAuth tokens instead of `Authorization: Bearer <api-key>`, stateless
//! `store:false`, tools in the top-level shape, item IDs stripped
//! before send. See `responses.rs` for the request body, `sse.rs` for
//! the event parser.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::agent_ipc::{AgentSocketEvent, Request, RequestCmd, Response};
use crate::ai::claude_code::{PromptParts, MAX_TAG_CHAIN_ROUNDS};
use crate::ai::core::{AICore, AIError};
use crate::ai::event::{AIEvent, TurnUsage};
use crate::ai::tag_call;
use crate::ai::tools::{RegistryConfig, ToolRegistry};
use crate::openai_auth;

use super::responses::{self, new_item, ResponseItem};
use super::sse::ResponsesSseDecoder;

/// Cap on native tool-use rounds in one user turn. Mirrors the other
/// API cores so behaviour is consistent.
pub const MAX_NATIVE_TOOL_ROUNDS: usize = 16;

/// Production base URL for the ChatGPT-subscription Responses API.
const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

pub struct OpenAiChatgptCore {
    session_id: Uuid,
    credential: String,
    model: String,
    reasoning_effort: String,
    prompt_parts: PromptParts,
    allowed_tools: Vec<String>,
    system_prompt: Option<String>,
    env: Vec<(String, String)>,
    extra_tools: Vec<Arc<dyn crate::ai::tools::Tool>>,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    http: reqwest::Client,
    base_url: String,
    started: bool,
    history: Arc<Mutex<Vec<ResponseItem>>>,
    /// Test-only override: if set, `turn()` uses these tokens instead
    /// of calling `openai_auth::refresh_if_needed`, so integration
    /// tests can stand up a mock `/responses` server without a real
    /// `auth.json` on disk.
    tokens_override: Option<crate::openai_auth::AuthTokens>,
}

impl OpenAiChatgptCore {
    pub fn new(
        session_id: Uuid,
        credential: impl Into<String>,
        model: impl Into<String>,
        reasoning_effort: impl Into<String>,
        prompt_parts: PromptParts,
        allowed_tools: Vec<String>,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            session_id,
            credential: credential.into(),
            model: model.into(),
            reasoning_effort: reasoning_effort.into(),
            prompt_parts,
            allowed_tools,
            system_prompt,
            env: Vec::new(),
            extra_tools: Vec::new(),
            socket_tx: None,
            http: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            started: false,
            history: Arc::new(Mutex::new(Vec::new())),
            tokens_override: None,
        }
    }

    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    pub fn with_extra_tools(mut self, tools: Vec<Arc<dyn crate::ai::tools::Tool>>) -> Self {
        self.extra_tools = tools;
        self
    }

    pub fn with_socket_tx(mut self, tx: mpsc::Sender<AgentSocketEvent>) -> Self {
        self.socket_tx = Some(tx);
        self
    }

    pub fn mark_started(mut self) -> Self {
        self.started = true;
        self
    }

    /// Override the base URL. Used by tests to point the core at a
    /// `wiremock` server.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Inject a fixed set of tokens, bypassing `auth.json` and the
    /// `openai_auth::refresh_if_needed` call on every turn. Intended
    /// for integration tests that point the core at a `wiremock`
    /// server — not wired into any config path, so production agents
    /// never reach this.
    #[doc(hidden)]
    pub fn with_tokens_for_tests(mut self, tokens: crate::openai_auth::AuthTokens) -> Self {
        self.tokens_override = Some(tokens);
        self
    }

    pub fn first_turn_prompt(&self) -> String {
        crate::ai::openai_api::build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools)
    }
}

#[async_trait]
impl AICore for OpenAiChatgptCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        let user_text = if !self.started {
            self.started = true;
            let preamble = crate::ai::openai_api::build_first_turn_prompt(
                &self.prompt_parts,
                &self.allowed_tools,
            );
            if preamble.is_empty() {
                prompt.to_string()
            } else if prompt.is_empty() {
                preamble
            } else {
                format!("{preamble}\n\n{prompt}")
            }
        } else {
            prompt.to_string()
        };

        self.history
            .lock()
            .expect("history poisoned")
            .push(ResponseItem::Typed(new_item::user_message(&user_text)));

        let (registry, _resolved) = ToolRegistry::from_allowed_with(
            &self.allowed_tools,
            &RegistryConfig {
                bash_env: self.env.clone(),
                extra_tools: self.extra_tools.clone(),
            },
        );

        let (tx, rx) = mpsc::channel::<AIEvent>(64);

        let session_id = self.session_id;
        let credential = self.credential.clone();
        let model = self.model.clone();
        let reasoning_effort = self.reasoning_effort.clone();
        let system_prompt = self.system_prompt.clone();
        let http = self.http.clone();
        let base_url = self.base_url.clone();
        let history = self.history.clone();
        let socket_tx = self.socket_tx.clone();
        let registry = Arc::new(registry);
        let tokens_override = self.tokens_override.clone();

        tokio::spawn(async move {
            let _ = tx
                .send(AIEvent::SystemInit {
                    session_id,
                    tools: registry.names().iter().map(|s| s.to_string()).collect(),
                    model: Some(model.clone()),
                    raw: Value::Null,
                })
                .await;

            let buf = Arc::new(Mutex::new(String::new()));
            for _tag_round in 0..MAX_TAG_CHAIN_ROUNDS {
                let inner = InnerCtx {
                    http: http.clone(),
                    base_url: base_url.clone(),
                    credential: credential.clone(),
                    model: model.clone(),
                    reasoning_effort: reasoning_effort.clone(),
                    system_prompt: system_prompt.clone(),
                    session_id,
                    history: history.clone(),
                    registry: registry.clone(),
                    tx: tx.clone(),
                    collect_text: buf.clone(),
                    tokens_override: tokens_override.clone(),
                };
                let final_stop = match run_native_tool_loop(inner).await {
                    Ok(s) => s,
                    Err(()) => return,
                };

                if socket_tx.is_none() {
                    return;
                }
                if final_stop.as_deref() != Some("completed") {
                    return;
                }

                let assistant_text = buf.lock().expect("buf poisoned").clone();
                let calls = tag_call::extract_calls(&assistant_text);
                if calls.is_empty() {
                    return;
                }

                let mut responses_out: Vec<(usize, Response)> = Vec::with_capacity(calls.len());
                let socket_tx_ref = socket_tx.as_ref().expect("checked above");
                for (i, call) in calls.iter().enumerate() {
                    let input_json = match &call.parsed {
                        Ok(cmd) => serde_json::to_value(cmd).unwrap_or(Value::Null),
                        Err(_) => serde_json::from_str::<Value>(&call.raw_body)
                            .unwrap_or_else(|_| Value::String(call.raw_body.clone())),
                    };
                    let call_id = format!("aciddog-call-tag-{i}");
                    let _ = tx
                        .send(AIEvent::ToolUse {
                            id: call_id.clone(),
                            name: "aciddog-call".to_string(),
                            input: input_json.clone(),
                            raw: input_json,
                        })
                        .await;

                    let response = match &call.parsed {
                        Ok(cmd) => dispatch_call(cmd.clone(), session_id, socket_tx_ref).await,
                        Err(e) => Response::err(e.clone()),
                    };

                    let is_error = !response.ok;
                    let content_json = serde_json::to_value(&response).unwrap_or(Value::Null);
                    let _ = tx
                        .send(AIEvent::ToolResult {
                            tool_use_id: call_id,
                            content: content_json.clone(),
                            is_error,
                            raw: content_json,
                        })
                        .await;

                    responses_out.push((i, response));
                }

                let rendered = tag_call::render_responses(&responses_out);
                history
                    .lock()
                    .expect("history poisoned")
                    .push(ResponseItem::Typed(new_item::user_message(&rendered)));
                buf.lock().expect("buf poisoned").clear();
            }

            let _ = tx
                .send(AIEvent::Error {
                    message: format!(
                        "aciddog: tag-call chain exceeded {MAX_TAG_CHAIN_ROUNDS} rounds in a single \
                         turn; stopping. Wait for the next user prompt instead of emitting more calls."
                    ),
                    raw: None,
                })
                .await;
        });

        Ok(ReceiverStream::new(rx).boxed())
    }

    fn session_id(&self) -> Option<Uuid> {
        Some(self.session_id)
    }

    fn shell_tool_name(&self) -> Option<&'static str> {
        Some("Bash")
    }

    fn first_turn_prompt(&self) -> String {
        crate::ai::openai_api::build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools)
    }

    fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }
}

struct InnerCtx {
    http: reqwest::Client,
    base_url: String,
    credential: String,
    model: String,
    reasoning_effort: String,
    system_prompt: Option<String>,
    session_id: Uuid,
    history: Arc<Mutex<Vec<ResponseItem>>>,
    registry: Arc<ToolRegistry>,
    tx: mpsc::Sender<AIEvent>,
    collect_text: Arc<Mutex<String>>,
    tokens_override: Option<crate::openai_auth::AuthTokens>,
}

/// Native tool-use loop. Returns the terminal stream state
/// (`"completed"` / `"failed"` / …) so the outer tag-chain loop can
/// decide whether to re-scan for `<aciddog-call>` tags. Returns
/// `Err(())` if the consumer hung up.
async fn run_native_tool_loop(ctx: InnerCtx) -> Result<Option<String>, ()> {
    let endpoint = format!("{}/responses", ctx.base_url.trim_end_matches('/'));

    for _round in 0..MAX_NATIVE_TOOL_ROUNDS {
        // Refresh if needed, surface auth errors as AIEvent::Error.
        // Tests can short-circuit this via `with_tokens_for_tests`.
        let tokens = if let Some(t) = &ctx.tokens_override {
            t.clone()
        } else {
            match openai_auth::refresh_if_needed(&ctx.credential).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = ctx
                        .tx
                        .send(AIEvent::Error {
                            message: format!("openai_chatgpt: {e}"),
                            raw: None,
                        })
                        .await;
                    return Ok(Some("auth_error".into()));
                }
            }
        };

        let body = {
            let history = ctx.history.lock().expect("history poisoned");
            responses::build_request_body(
                &ctx.model,
                ctx.system_prompt.as_deref(),
                &history,
                &ctx.registry.responses_tools(),
                &ctx.reasoning_effort,
                &ctx.session_id.to_string(),
            )
        };

        let resp = match ctx
            .http
            .post(&endpoint)
            .header("authorization", format!("Bearer {}", tokens.access_token))
            .header("chatgpt-account-id", &tokens.account_id)
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", openai_auth::ORIGINATOR)
            .header("accept", "text/event-stream")
            .header("content-type", "application/json")
            .header("session_id", ctx.session_id.to_string())
            .header("conversation_id", ctx.session_id.to_string())
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = ctx
                    .tx
                    .send(AIEvent::Error {
                        message: format!("openai_chatgpt: request failed: {e}"),
                        raw: None,
                    })
                    .await;
                return Ok(None);
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let snippet: String = body_text.chars().take(512).collect();
            let _ = ctx
                .tx
                .send(AIEvent::Error {
                    message: format!("openai_chatgpt: HTTP {status}: {snippet}"),
                    raw: None,
                })
                .await;
            return Ok(None);
        }

        // Stream state. `text_buf` accumulates `output_text` deltas;
        // `pending_calls` captures finalised `function_call` items so
        // we can dispatch them after the stream closes.
        let mut text_buf = String::new();
        let mut pending_calls: Vec<PendingToolCall> = Vec::new();
        let mut pending_items: Vec<ResponseItem> = Vec::new();
        let mut turn_usage: Option<TurnUsage> = None;
        let mut terminal_state: Option<String> = None;

        let mut decoder = ResponsesSseDecoder::new();
        let mut stream = resp.bytes_stream();

        'stream: while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = ctx
                        .tx
                        .send(AIEvent::Error {
                            message: format!("openai_chatgpt: stream error: {e}"),
                            raw: None,
                        })
                        .await;
                    return Ok(None);
                }
            };
            decoder.push(&chunk);
            while let Some(ev) = decoder.next_event() {
                let ev = match ev {
                    Ok(e) => e,
                    Err(e) => {
                        warn!(error = %e, "openai_chatgpt: skipping malformed SSE event");
                        continue;
                    }
                };
                match ev.event_type.as_str() {
                    "response.output_text.delta" => {
                        if let Some(d) = ev.raw.get("delta").and_then(Value::as_str) {
                            text_buf.push_str(d);
                        }
                    }
                    "response.output_item.done" => {
                        let Some(item) = ev.raw.get("item").cloned() else {
                            continue;
                        };
                        let item_type = item
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if item_type == "function_call" {
                            let name = item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let call_id = item
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let args = item
                                .get("arguments")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            pending_calls.push(PendingToolCall {
                                name,
                                call_id,
                                arguments: args,
                                raw_item: item.clone(),
                            });
                        }
                        pending_items.push(ResponseItem::Raw(item));
                    }
                    "response.completed" => {
                        if let Some(usage) = ev
                            .raw
                            .pointer("/response/usage")
                            .and_then(parse_usage_responses)
                        {
                            turn_usage = Some(usage);
                        }
                        terminal_state = Some("completed".into());
                        break 'stream;
                    }
                    "response.failed" => {
                        let message = ev
                            .raw
                            .pointer("/response/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("response.failed with no message")
                            .to_string();
                        let code = ev
                            .raw
                            .pointer("/response/error/code")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown");
                        let _ = ctx
                            .tx
                            .send(AIEvent::Error {
                                message: format!(
                                    "openai_chatgpt: response.failed ({code}): {message}"
                                ),
                                raw: Some(ev.raw.clone()),
                            })
                            .await;
                        terminal_state = Some("failed".into());
                        break 'stream;
                    }
                    // Reasoning streams are informational; drop them
                    // in v1 rather than plumbing them through the
                    // AIEvent enum.
                    "response.reasoning_text.delta"
                    | "response.reasoning_summary_text.delta"
                    | "response.output_text.done"
                    | "response.created"
                    | "response.in_progress" => {}
                    other => {
                        debug!(event_type = %other, "openai_chatgpt: unhandled sse event");
                    }
                }
            }
        }

        // Flush assistant text as a single AssistantText event.
        if !text_buf.is_empty() {
            {
                let mut guard = ctx.collect_text.lock().expect("buf poisoned");
                guard.push_str(&text_buf);
                guard.push('\n');
            }
            if ctx
                .tx
                .send(AIEvent::AssistantText {
                    text: text_buf.clone(),
                    raw: Value::Null,
                })
                .await
                .is_err()
            {
                return Err(());
            }
        }

        // Emit a ToolUse event per finalised function_call. The raw
        // items get appended to history below so the next request
        // round-trips the function_call alongside its output.
        for call in &pending_calls {
            let input: Value = if call.arguments.is_empty() {
                Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(&call.arguments).unwrap_or(Value::Null)
            };
            if ctx
                .tx
                .send(AIEvent::ToolUse {
                    id: call.call_id.clone(),
                    name: call.name.clone(),
                    input: input.clone(),
                    raw: input,
                })
                .await
                .is_err()
            {
                return Err(());
            }
        }

        // Append every finalised item to history (assistant messages,
        // function_call items, reasoning items…). Stateless endpoint
        // requires the function_call to travel back alongside its
        // output on the next request.
        {
            let mut history = ctx.history.lock().expect("history poisoned");
            history.extend(pending_items.into_iter());
        }

        let _ = ctx
            .tx
            .send(AIEvent::TurnResult {
                stop_reason: terminal_state.clone(),
                total_cost_usd: None,
                usage: turn_usage,
                raw: Value::Null,
            })
            .await;

        if pending_calls.is_empty() {
            return Ok(terminal_state);
        }

        // Execute tools in parallel, then append function_call_output
        // items to history and loop for another round.
        let mut futures_vec = Vec::with_capacity(pending_calls.len());
        for call in pending_calls {
            let registry = ctx.registry.clone();
            futures_vec.push(async move {
                let input: Value = if call.arguments.is_empty() {
                    Value::Object(serde_json::Map::new())
                } else {
                    serde_json::from_str(&call.arguments).unwrap_or(Value::Null)
                };
                let output = match registry.get(&call.name) {
                    Some(tool) => tool.invoke(input).await,
                    None => {
                        crate::ai::tools::ToolOutput::error(format!("unknown tool `{}`", call.name))
                    }
                };
                (call.call_id, call.name, output)
            });
        }
        let outputs = futures::future::join_all(futures_vec).await;

        for (call_id, name, output) in outputs {
            let content_json = Value::String(output.text.clone());
            if ctx
                .tx
                .send(AIEvent::ToolResult {
                    tool_use_id: call_id.clone(),
                    content: content_json.clone(),
                    is_error: output.is_error,
                    raw: json!({
                        "type": "function_call_output",
                        "call_id": call_id,
                        "name": name,
                        "output": output.text,
                        "is_error": output.is_error,
                    }),
                })
                .await
                .is_err()
            {
                return Err(());
            }
            ctx.history
                .lock()
                .expect("history poisoned")
                .push(ResponseItem::Typed(new_item::function_call_output(
                    &call_id,
                    &output.text,
                )));
        }
    }

    let _ = ctx
        .tx
        .send(AIEvent::Error {
            message: format!(
                "openai_chatgpt: native tool-use loop exceeded {MAX_NATIVE_TOOL_ROUNDS} rounds; stopping."
            ),
            raw: None,
        })
        .await;
    Ok(Some("max_tool_rounds".into()))
}

struct PendingToolCall {
    name: String,
    call_id: String,
    arguments: String,
    #[allow(dead_code)]
    raw_item: Value,
}

/// Parse usage from the Responses API `response.completed` event.
/// Field names differ from Chat Completions (`input_tokens` /
/// `output_tokens` / `total_tokens` / `*_details.*_tokens`).
fn parse_usage_responses(usage: &Value) -> Option<TurnUsage> {
    let obj = usage.as_object()?;
    let mut u = TurnUsage::default();
    let mut any = false;
    if let Some(v) = obj.get("input_tokens").and_then(Value::as_u64) {
        u.input_tokens = v;
        any = true;
    }
    if let Some(v) = obj.get("output_tokens").and_then(Value::as_u64) {
        u.output_tokens = v;
        any = true;
    }
    if let Some(v) = usage
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
    {
        u.cache_read_input_tokens = v;
        u.input_tokens = u.input_tokens.saturating_sub(v);
        any = true;
    }
    if any {
        Some(u)
    } else {
        None
    }
}

async fn dispatch_call(
    cmd: RequestCmd,
    agent_id: Uuid,
    socket_tx: &mpsc::Sender<AgentSocketEvent>,
) -> Response {
    let (reply_tx, reply_rx) = oneshot::channel();
    let event = AgentSocketEvent {
        request: Request { agent_id, cmd },
        reply: reply_tx,
    };
    if socket_tx.send(event).await.is_err() {
        return Response::err("orchestrator channel closed");
    }
    match reply_rx.await {
        Ok(r) => r,
        Err(_) => Response::err("orchestrator dropped reply without responding"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_usage_populates_tokens_and_cached() {
        let usage = json!({
            "input_tokens": 120,
            "output_tokens": 40,
            "total_tokens": 160,
            "input_tokens_details": { "cached_tokens": 30 },
            "output_tokens_details": { "reasoning_tokens": 5 },
        });
        let u = parse_usage_responses(&usage).unwrap();
        assert_eq!(u.output_tokens, 40);
        // Cached subtracted from input to avoid double-counting.
        assert_eq!(u.input_tokens, 90);
        assert_eq!(u.cache_read_input_tokens, 30);
    }

    #[test]
    fn parse_usage_returns_none_when_no_fields() {
        assert!(parse_usage_responses(&json!({})).is_none());
    }

    #[test]
    fn parse_usage_returns_none_for_non_object() {
        assert!(parse_usage_responses(&json!("nope")).is_none());
    }
}
