//! OpenAI chat-completions API core. Speaks the HTTP/SSE wire protocol
//! directly, so the same implementation works against
//! `api.openai.com/v1`, OpenRouter, Ollama's OpenAI-compatible endpoint,
//! or any other vendor that implements `/chat/completions`. Everything
//! vendor-specific (base URL, env var name, model id) comes in through
//! the constructor.
//!
//! `apiUrl` is the **v1 base** — e.g. `https://api.openai.com/v1`,
//! `https://openrouter.ai/api/v1`, `http://localhost:11434/v1`. The
//! core appends `/chat/completions`. This matches how every
//! OpenAI-compatible vendor documents their endpoint.
//!
//! Event flow out of `turn()` mirrors `AnthropicApiCore`:
//!   SystemInit → AssistantText/ToolUse/ToolResult* → TurnResult
//! so the TUI renders it with zero variant-specific code.
//!
//! Two-layer loop inside one `turn()`:
//!   - Native tool-use loop: POST, stream SSE, accumulate assistant text
//!     and tool-call deltas (keyed by `index`, with `function.arguments`
//!     concatenated across chunks), and if `finish_reason == "tool_calls"`
//!     execute the tools and append one `role: "tool"` message per
//!     result. Cap 16 rounds.
//!   - Tag-protocol outer loop (only when `socket_tx` is set): after
//!     the inner loop settles with `finish_reason == "stop"`, scan the
//!     buffered assistant text for `<aciddog-call>` tags, dispatch via
//!     the socket, render `<aciddog-response>` blocks, loop. Reuses
//!     `MAX_TAG_CHAIN_ROUNDS` from claude_code.

pub mod sse;
pub mod types;

use std::collections::BTreeMap;
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
use crate::ai::event::{AIEvent, CodexFailureKind, TurnUsage};
use crate::ai::tag_call;
use crate::ai::tools::{RegistryConfig, ToolRegistry};

use self::sse::{OpenAiSseDecoder, OpenAiSseEvent};
use self::types::{FunctionCall, Message, Role, ToolCall};

/// Cap on native tool-use rounds inside one user turn. Mirrors the
/// Anthropic core's ceiling so behaviour is consistent across vendors.
pub const MAX_NATIVE_TOOL_ROUNDS: usize = 16;

pub struct OpenAiApiCore {
    session_id: Uuid,
    api_url: String,
    api_key_env: String,
    model: String,
    prompt_parts: PromptParts,
    allowed_tools: Vec<String>,
    system_prompt: Option<String>,
    env: Vec<(String, String)>,
    extra_tools: Vec<Arc<dyn crate::ai::tools::Tool>>,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    http: reqwest::Client,
    started: bool,
    history: Arc<Mutex<Vec<Message>>>,
}

impl OpenAiApiCore {
    pub fn new(
        session_id: Uuid,
        api_url: impl Into<String>,
        api_key_env: impl Into<String>,
        model: impl Into<String>,
        prompt_parts: PromptParts,
        allowed_tools: Vec<String>,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            session_id,
            api_url: api_url.into(),
            api_key_env: api_key_env.into(),
            model: model.into(),
            prompt_parts,
            allowed_tools,
            system_prompt,
            env: Vec::new(),
            extra_tools: Vec::new(),
            socket_tx: None,
            http: reqwest::Client::new(),
            started: false,
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// Inject additional tools into every turn's `ToolRegistry` on top
    /// of the `allowed_tools` allowlist. Mirrors `AnthropicApiCore`.
    pub fn with_extra_tools(mut self, tools: Vec<Arc<dyn crate::ai::tools::Tool>>) -> Self {
        self.extra_tools = tools;
        self
    }

    pub fn with_socket_tx(mut self, tx: mpsc::Sender<AgentSocketEvent>) -> Self {
        self.socket_tx = Some(tx);
        self
    }

    /// Mark the first-turn preamble as already delivered. Used by
    /// `resurrect_worker` so a resumed worker skips the XML preamble
    /// even though history is empty in-memory.
    pub fn mark_started(mut self) -> Self {
        self.started = true;
        self
    }

    /// Full assembled first-turn prompt — mirrors `AnthropicApiCore::first_turn_prompt`
    /// so the TUI's AgentInfo block can surface what the model will see.
    pub fn first_turn_prompt(&self) -> String {
        build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools)
    }
}

#[async_trait]
impl AICore for OpenAiApiCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        let api_key = match std::env::var(&self.api_key_env) {
            Ok(k) if !k.is_empty() => Some(k),
            _ => None,
        };

        let user_text = if !self.started {
            self.started = true;
            let preamble = build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools);
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
            .push(Message {
                role: Role::User,
                content: Some(user_text),
                tool_calls: None,
                tool_call_id: None,
            });

        let (registry, _resolved) = ToolRegistry::from_allowed_with(
            &self.allowed_tools,
            &RegistryConfig {
                bash_env: self.env.clone(),
                extra_tools: self.extra_tools.clone(),
            },
        );

        let (tx, rx) = mpsc::channel::<AIEvent>(64);

        let session_id = self.session_id;
        let api_url = self.api_url.clone();
        let api_key_env = self.api_key_env.clone();
        let model = self.model.clone();
        let system_prompt = self.system_prompt.clone();
        let http = self.http.clone();
        let history = self.history.clone();
        let socket_tx = self.socket_tx.clone();
        let registry = Arc::new(registry);

        tokio::spawn(async move {
            let Some(api_key) = api_key else {
                let _ = tx
                    .send(AIEvent::Error {
                        kind: CodexFailureKind::Unknown,
                        message: format!("openai_api: env var `{api_key_env}` is not set or empty"),
                        raw: None,
                    })
                    .await;
                return;
            };

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
                let inner_ctx = InnerCtx {
                    http: http.clone(),
                    api_url: api_url.clone(),
                    api_key: api_key.clone(),
                    model: model.clone(),
                    system_prompt: system_prompt.clone(),
                    history: history.clone(),
                    registry: registry.clone(),
                    tx: tx.clone(),
                    collect_text: buf.clone(),
                };
                let final_stop = match run_native_tool_loop(inner_ctx).await {
                    Ok(s) => s,
                    Err(()) => return, // consumer gone
                };

                if socket_tx.is_none() {
                    return;
                }
                if final_stop.as_deref() != Some("stop") {
                    return;
                }

                let assistant_text = buf.lock().expect("buf poisoned").clone();
                let calls = tag_call::extract_calls(&assistant_text);
                if calls.is_empty() {
                    return;
                }

                let mut responses: Vec<(usize, Response)> = Vec::with_capacity(calls.len());
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

                    responses.push((i, response));
                }

                let rendered = tag_call::render_responses(&responses);
                history.lock().expect("history poisoned").push(Message {
                    role: Role::User,
                    content: Some(rendered),
                    tool_calls: None,
                    tool_call_id: None,
                });
                buf.lock().expect("buf poisoned").clear();
            }

            let _ = tx
                .send(AIEvent::Error {
                    kind: CodexFailureKind::Unknown,
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
        build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools)
    }

    fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }
}

struct InnerCtx {
    http: reqwest::Client,
    api_url: String,
    api_key: String,
    model: String,
    system_prompt: Option<String>,
    history: Arc<Mutex<Vec<Message>>>,
    registry: Arc<ToolRegistry>,
    tx: mpsc::Sender<AIEvent>,
    collect_text: Arc<Mutex<String>>,
}

/// Per-stream accumulator for one tool call. OpenAI streams each call's
/// `function.arguments` as successive string fragments (keyed by
/// `index`); `id` and `name` only appear on the first chunk for a given
/// index. We concatenate arguments verbatim — the accumulated string is
/// itself JSON-encoded per the OpenAI contract.
struct AccumulatedToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// POST + stream loop. Returns the terminal `finish_reason` (or `None`
/// if the stream ended without one) or `Err(())` if the consumer hung
/// up. Executes tool calls and loops until `finish_reason != "tool_calls"`
/// or the round cap is hit.
async fn run_native_tool_loop(ctx: InnerCtx) -> Result<Option<String>, ()> {
    let endpoint = format!("{}/chat/completions", ctx.api_url.trim_end_matches('/'));

    for _round in 0..MAX_NATIVE_TOOL_ROUNDS {
        let body = build_request_body(
            &ctx.model,
            &ctx.system_prompt,
            &ctx.history.lock().expect("history poisoned"),
            &ctx.registry.openai_tools(),
        );

        let resp = match ctx
            .http
            .post(&endpoint)
            .header("authorization", format!("Bearer {}", ctx.api_key))
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = ctx
                    .tx
                    .send(AIEvent::Error {
                        kind: CodexFailureKind::Unknown,
                        message: format!("openai_api: request failed: {e}"),
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
                    kind: CodexFailureKind::Unknown,
                    message: format!("openai_api: HTTP {status}: {snippet}"),
                    raw: None,
                })
                .await;
            return Ok(None);
        }

        let mut text_buf: String = String::new();
        let mut tool_calls: BTreeMap<u32, AccumulatedToolCall> = BTreeMap::new();
        let mut finish_reason: Option<String> = None;
        let mut final_raw: Value = Value::Null;
        let mut saw_done = false;
        // OpenAI emits `usage` on the final chunk when
        // `stream_options.include_usage = true` is set on the request.
        let mut turn_usage: Option<TurnUsage> = None;

        let mut decoder = OpenAiSseDecoder::new();
        let mut stream = resp.bytes_stream();

        'stream: while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = ctx
                        .tx
                        .send(AIEvent::Error {
                            kind: CodexFailureKind::Unknown,
                            message: format!("openai_api: stream error: {e}"),
                            raw: None,
                        })
                        .await;
                    return Ok(None);
                }
            };
            let events = decoder.feed(&chunk);
            for ev in events {
                let ev = match ev {
                    Ok(e) => e,
                    Err(e) => {
                        warn!(error = %e, "openai_api: skipping malformed SSE event");
                        continue;
                    }
                };
                match ev {
                    OpenAiSseEvent::Done => {
                        saw_done = true;
                        break 'stream;
                    }
                    OpenAiSseEvent::Data(v) => {
                        // The terminal usage chunk (when include_usage
                        // is enabled) carries `choices: []` and a
                        // top-level `usage` object. Capture it before
                        // we skip the empty-choices early-return.
                        if let Some(usage) = v.pointer("/usage").and_then(parse_usage_openai) {
                            turn_usage = Some(usage);
                        }

                        // Some vendors (OpenRouter) send a leading
                        // metadata chunk with `choices: []`. Skip those.
                        let Some(choice) = v.pointer("/choices/0") else {
                            final_raw = v;
                            continue;
                        };

                        if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str)
                        {
                            if !text.is_empty() {
                                text_buf.push_str(text);
                            }
                        }

                        if let Some(calls) = choice
                            .pointer("/delta/tool_calls")
                            .and_then(Value::as_array)
                        {
                            for call in calls {
                                let index =
                                    call.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                                let entry = tool_calls.entry(index).or_insert_with(|| {
                                    AccumulatedToolCall {
                                        id: String::new(),
                                        name: String::new(),
                                        arguments: String::new(),
                                    }
                                });
                                if let Some(id) = call.get("id").and_then(Value::as_str) {
                                    if !id.is_empty() {
                                        entry.id = id.to_string();
                                    }
                                }
                                if let Some(name) =
                                    call.pointer("/function/name").and_then(Value::as_str)
                                {
                                    if !name.is_empty() {
                                        entry.name = name.to_string();
                                    }
                                }
                                if let Some(args) =
                                    call.pointer("/function/arguments").and_then(Value::as_str)
                                {
                                    entry.arguments.push_str(args);
                                }
                            }
                        }

                        if let Some(fr) = choice.get("finish_reason").and_then(Value::as_str) {
                            finish_reason = Some(fr.to_string());
                        }

                        final_raw = v;
                    }
                }
            }
        }

        let _ = saw_done; // informational only — some vendors close the stream without it

        // Flush the accumulated assistant text as a single AssistantText
        // event. Anthropic emits per content_block_stop; OpenAI's stream
        // has no analogous per-block boundary, so one event per stream
        // matches the TUI's expectations.
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

        // Materialise the streamed tool-call accumulators into stable
        // `ToolCall` values, emit a ToolUse event per call, and remember
        // them for the dispatch stage below.
        let finalised_calls: Vec<ToolCall> = tool_calls
            .into_values()
            .map(|c| ToolCall {
                id: c.id,
                kind: "function".to_string(),
                function: FunctionCall {
                    name: c.name,
                    arguments: c.arguments,
                },
            })
            .collect();

        for call in &finalised_calls {
            let input: Value = if call.function.arguments.is_empty() {
                Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null)
            };
            if ctx
                .tx
                .send(AIEvent::ToolUse {
                    id: call.id.clone(),
                    name: call.function.name.clone(),
                    input: input.clone(),
                    raw: input,
                })
                .await
                .is_err()
            {
                return Err(());
            }
        }

        // Persist the assistant message. `content: null` on tool-call-only
        // turns is crucial — some vendors reject the message if the
        // field is missing.
        if !text_buf.is_empty() || !finalised_calls.is_empty() {
            let tool_calls_field = if finalised_calls.is_empty() {
                None
            } else {
                Some(finalised_calls.clone())
            };
            ctx.history.lock().expect("history poisoned").push(Message {
                role: Role::Assistant,
                content: if text_buf.is_empty() {
                    None
                } else {
                    Some(text_buf)
                },
                tool_calls: tool_calls_field,
                tool_call_id: None,
            });
        }

        let _ = ctx
            .tx
            .send(AIEvent::TurnResult {
                stop_reason: finish_reason.clone(),
                total_cost_usd: None,
                usage: turn_usage,
                raw: final_raw,
            })
            .await;

        match finish_reason.as_deref() {
            Some("tool_calls") => {
                if finalised_calls.is_empty() {
                    return Ok(finish_reason);
                }

                let mut futures_vec = Vec::with_capacity(finalised_calls.len());
                for call in &finalised_calls {
                    let registry = ctx.registry.clone();
                    let name = call.function.name.clone();
                    let id = call.id.clone();
                    let args_str = call.function.arguments.clone();
                    futures_vec.push(async move {
                        let input = if args_str.is_empty() {
                            Value::Object(serde_json::Map::new())
                        } else {
                            serde_json::from_str(&args_str).unwrap_or(Value::Null)
                        };
                        let output = match registry.get(&name) {
                            Some(tool) => tool.invoke(input).await,
                            None => crate::ai::tools::ToolOutput::error(format!(
                                "unknown tool `{name}`"
                            )),
                        };
                        (id, name, output)
                    });
                }
                let outputs = futures::future::join_all(futures_vec).await;

                for (id, name, output) in outputs {
                    let content_json = Value::String(output.text.clone());
                    if ctx
                        .tx
                        .send(AIEvent::ToolResult {
                            tool_use_id: id.clone(),
                            content: content_json.clone(),
                            is_error: output.is_error,
                            raw: json!({
                                "role": "tool",
                                "tool_call_id": id,
                                "name": name,
                                "content": output.text,
                                "is_error": output.is_error,
                            }),
                        })
                        .await
                        .is_err()
                    {
                        return Err(());
                    }

                    // OpenAI wants ONE message per tool result, not a
                    // single user message with multiple blocks.
                    ctx.history.lock().expect("history poisoned").push(Message {
                        role: Role::Tool,
                        content: Some(output.text),
                        tool_calls: None,
                        tool_call_id: Some(id),
                    });
                }

                continue;
            }
            _ => return Ok(finish_reason),
        }
    }

    let _ = ctx
        .tx
        .send(AIEvent::Error {
            kind: CodexFailureKind::Unknown,
            message: format!(
                "openai_api: native tool-use loop exceeded {MAX_NATIVE_TOOL_ROUNDS} rounds; stopping."
            ),
            raw: None,
        })
        .await;
    Ok(Some("max_tool_rounds".to_string()))
}

/// Build the `/chat/completions` request body from our in-memory
/// history. The system prompt is prepended as a `role: "system"`
/// message rather than a separate field (that's the Anthropic shape).
/// Pull a `TurnUsage` out of an OpenAI-style `usage` object. OpenAI uses
/// `prompt_tokens` / `completion_tokens` / `total_tokens`; some vendors
/// also report `prompt_tokens_details.cached_tokens`. Returns `None`
/// when the value isn't an object or has no recognisable fields.
fn parse_usage_openai(usage: &Value) -> Option<TurnUsage> {
    let obj = usage.as_object()?;
    let mut u = TurnUsage::default();
    let mut any = false;
    if let Some(v) = obj.get("prompt_tokens").and_then(Value::as_u64) {
        u.input_tokens = v;
        any = true;
    }
    if let Some(v) = obj.get("completion_tokens").and_then(Value::as_u64) {
        u.output_tokens = v;
        any = true;
    }
    if let Some(v) = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
    {
        u.cache_read_input_tokens = v;
        // Cached tokens are reported as part of prompt_tokens — subtract
        // so we don't double-count when summing the four fields.
        u.input_tokens = u.input_tokens.saturating_sub(v);
        any = true;
    }
    if any {
        Some(u)
    } else {
        None
    }
}

fn build_request_body(
    model: &str,
    system_prompt: &Option<String>,
    history: &[Message],
    tools: &[Value],
) -> Value {
    let mut messages: Vec<Value> = Vec::with_capacity(history.len() + 1);
    if let Some(sp) = system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        messages.push(json!({ "role": "system", "content": sp }));
    }
    for m in history {
        messages.push(render_message(m));
    }

    let mut body = json!({
        "model": model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    body
}

/// Hand-built JSON so `content: null` is emitted on assistant messages
/// that only carry tool calls. Serde's `skip_serializing_if` would drop
/// the field, which some OpenAI-compatible backends reject.
fn render_message(m: &Message) -> Value {
    let role = match m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    let mut obj = serde_json::Map::new();
    obj.insert("role".into(), Value::String(role.to_string()));

    match m.role {
        Role::Tool => {
            obj.insert(
                "tool_call_id".into(),
                Value::String(m.tool_call_id.clone().unwrap_or_default()),
            );
            obj.insert(
                "content".into(),
                Value::String(m.content.clone().unwrap_or_default()),
            );
        }
        Role::Assistant => {
            match &m.content {
                Some(text) => obj.insert("content".into(), Value::String(text.clone())),
                None => obj.insert("content".into(), Value::Null),
            };
            if let Some(calls) = &m.tool_calls {
                let rendered: Vec<Value> = calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": tc.kind,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }
                        })
                    })
                    .collect();
                obj.insert("tool_calls".into(), Value::Array(rendered));
            }
        }
        Role::User | Role::System => {
            obj.insert(
                "content".into(),
                Value::String(m.content.clone().unwrap_or_default()),
            );
        }
    }
    Value::Object(obj)
}

pub(crate) fn build_first_turn_prompt(parts: &PromptParts, allowed_tools: &[String]) -> String {
    let mut sections: Vec<String> = Vec::new();
    if let Some(role) = parts
        .role
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sections.push(format!("<role>\n{role}\n</role>"));
    }
    if let Some(team) = parts
        .team
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sections.push(format!("<team>\n{team}\n</team>"));
    }
    if let Some(mem) = parts
        .memory
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sections.push(format!("<memory>\n{mem}\n</memory>"));
    }
    sections.push(build_tools_section(parts, allowed_tools));
    sections.push(format!(
        "<instructions>\n{}\n</instructions>",
        parts.instructions.trim()
    ));
    sections.join("\n\n")
}

fn build_tools_section(parts: &PromptParts, allowed_tools: &[String]) -> String {
    use crate::agent_ipc::preamble::{
        cli_preamble, tag_preamble, tag_worker_preamble, worker_preamble,
    };
    use crate::ai::tools::built_in_tool_doc;

    let mut body = String::new();
    body.push_str("## Built-in tools enabled\n\n");
    if allowed_tools.is_empty() {
        body.push_str("(none)\n");
    } else {
        for name in allowed_tools {
            body.push_str(&format!("- `{name}` — {}\n", built_in_tool_doc(name)));
        }
    }

    if let Some(cli) = &parts.aciddog_cli {
        let has_bash = allowed_tools.iter().any(|t| t == "Bash");
        let contactable: Vec<&str> = cli.contactable.iter().map(String::as_str).collect();
        let creatable: Vec<&str> = cli.creatable.iter().map(String::as_str).collect();
        let memory_banks: Vec<&str> = cli.memory_banks.iter().map(String::as_str).collect();
        let skills: Vec<&str> = cli.skills.iter().map(String::as_str).collect();
        let channel_infos: Vec<crate::agent_ipc::preamble::ChannelInfo<'_>> = cli
            .channels
            .iter()
            .map(|c| crate::agent_ipc::preamble::ChannelInfo {
                id: c.id.as_str(),
                description: c.description.as_deref(),
                other_members: c.other_members.iter().map(String::as_str).collect(),
            })
            .collect();

        // Workers may only message their parent — `user` is not
        // a valid recipient for them. Workers also never supervise.
        let include_user_recipient = cli.worker.is_none();
        let supervisor = cli.worker.is_none() && cli.supervisor;
        let sysadmin = cli.worker.is_none() && cli.sysadmin;
        body.push('\n');
        if has_bash {
            if let Some(w) = &cli.worker {
                body.push_str(&worker_preamble(&cli.binary, &w.parent_id, &w.task));
            }
            body.push_str(&cli_preamble(
                &cli.binary,
                &contactable,
                &creatable,
                &memory_banks,
                &skills,
                &channel_infos,
                include_user_recipient,
                supervisor,
                sysadmin,
            ));
        } else {
            if let Some(w) = &cli.worker {
                body.push_str(&tag_worker_preamble(&w.parent_id, &w.task));
            }
            body.push_str(&tag_preamble(
                &contactable,
                &creatable,
                &memory_banks,
                &skills,
                &channel_infos,
                include_user_recipient,
                supervisor,
                sysadmin,
            ));
        }
    }

    format!("<tools>\n{}\n</tools>", body.trim_end())
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

#[allow(dead_code)]
fn _compiler_hint_debug(_: &dyn std::fmt::Debug) {
    debug!("noop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::claude_code::PromptParts;

    fn parts(instructions: &str) -> PromptParts {
        PromptParts {
            instructions: instructions.into(),
            ..PromptParts::default()
        }
    }

    #[test]
    fn first_turn_prompt_wraps_sections() {
        let core = OpenAiApiCore::new(
            Uuid::new_v4(),
            "http://x".to_string(),
            "K".to_string(),
            "m".to_string(),
            PromptParts {
                role: Some("be terse".into()),
                instructions: "do the thing".into(),
                ..PromptParts::default()
            },
            vec!["Read".into()],
            None,
        );
        let p = core.first_turn_prompt();
        assert!(p.contains("<role>"), "role missing: {p}");
        assert!(p.contains("<tools>"));
        assert!(p.contains("<instructions>"));
        assert!(p.contains("- `Read` —"));
    }

    #[test]
    fn request_body_omits_tools_when_empty() {
        let body = build_request_body("m", &None, &[], &[]);
        assert!(body.get("tools").is_none());
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn request_body_opts_in_to_stream_usage() {
        let body = build_request_body("m", &None, &[], &[]);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn parse_usage_openai_basic_fields() {
        let v = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        });
        let u = parse_usage_openai(&v).expect("Some");
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
        assert_eq!(u.cache_read_input_tokens, 0);
    }

    #[test]
    fn parse_usage_openai_extracts_cached_tokens() {
        let v = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "prompt_tokens_details": { "cached_tokens": 30 }
        });
        let u = parse_usage_openai(&v).expect("Some");
        // Cached are split out; remainder is the actual fresh input.
        assert_eq!(u.input_tokens, 70);
        assert_eq!(u.output_tokens, 20);
        assert_eq!(u.cache_read_input_tokens, 30);
        assert_eq!(u.total_tokens(), 70 + 20 + 30);
    }

    #[test]
    fn parse_usage_openai_returns_none_for_empty() {
        assert!(parse_usage_openai(&serde_json::json!({})).is_none());
        assert!(parse_usage_openai(&serde_json::json!(null)).is_none());
    }

    #[test]
    fn request_body_prepends_system_as_first_message() {
        let body = build_request_body("m", &Some("you are x".into()), &[], &[]);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "you are x");
    }

    #[test]
    fn request_body_has_no_separate_system_field() {
        // System is always rendered as the first message. Separate
        // top-level `system` field would be the Anthropic shape.
        let body = build_request_body("m", &Some("sp".into()), &[], &[]);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn request_body_serialises_history_with_tool_call_shape() {
        let hist = vec![
            Message {
                role: Role::User,
                content: Some("hi".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::Assistant,
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".into(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: "Read".into(),
                        arguments: "{\"path\":\"x\"}".into(),
                    },
                }]),
                tool_call_id: None,
            },
            Message {
                role: Role::Tool,
                content: Some("file contents".into()),
                tool_calls: None,
                tool_call_id: Some("call_1".into()),
            },
        ];
        let body = build_request_body("m", &None, &hist, &[]);
        let msgs = body["messages"].as_array().expect("messages is array");
        assert_eq!(msgs.len(), 3);

        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "hi");

        assert_eq!(msgs[1]["role"], "assistant");
        // Assistant with only tool calls must carry explicit null content.
        assert!(msgs[1]["content"].is_null());
        let tcs = msgs[1]["tool_calls"].as_array().expect("tool_calls array");
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0]["id"], "call_1");
        assert_eq!(tcs[0]["type"], "function");
        assert_eq!(tcs[0]["function"]["name"], "Read");
        // arguments must be a JSON-encoded *string*, not an object.
        assert_eq!(tcs[0]["function"]["arguments"], "{\"path\":\"x\"}");

        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "call_1");
        assert_eq!(msgs[2]["content"], "file contents");
    }

    #[test]
    fn assistant_only_tool_calls_emits_null_content() {
        let m = Message {
            role: Role::Assistant,
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "c".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "Read".into(),
                    arguments: "{}".into(),
                },
            }]),
            tool_call_id: None,
        };
        let rendered = render_message(&m);
        assert!(rendered["content"].is_null());
        assert_eq!(rendered["role"], "assistant");
    }

    #[tokio::test]
    async fn missing_env_var_surfaces_as_error_event() {
        let env_name = "_ACIDDOG_TEST_OPENAI_MISSING_KEY_XYZ";
        std::env::remove_var(env_name);

        let mut core = OpenAiApiCore::new(
            Uuid::new_v4(),
            "http://127.0.0.1:1".to_string(),
            env_name.to_string(),
            "m".to_string(),
            parts("i"),
            vec![],
            None,
        );
        let mut stream = core.turn("hello").await.expect("turn");
        let mut saw_error = false;
        while let Some(ev) = stream.next().await {
            if let AIEvent::Error { message, .. } = ev {
                assert!(message.contains(env_name));
                saw_error = true;
                break;
            }
        }
        assert!(saw_error, "expected an AIEvent::Error for missing env var");
    }
}
