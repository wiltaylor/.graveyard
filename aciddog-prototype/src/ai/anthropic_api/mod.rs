//! Anthropic Messages API core. Speaks the HTTP/SSE wire protocol
//! directly, so the same implementation works against
//! `api.anthropic.com`, Z.ai's Anthropic-compatible endpoint, or any
//! other compatible vendor. Everything vendor-specific (URL, env var
//! name, model id) comes in through the constructor.
//!
//! Event flow out of `turn()` mirrors `ClaudeCodeCore`:
//!   SystemInit → AssistantText/ToolUse/ToolResult* → TurnResult
//! so the TUI renders it with zero variant-specific code.
//!
//! Two-layer loop inside one `turn()`:
//!   - Native tool-use loop: POST, stream SSE, accumulate assistant
//!     content blocks, if stop_reason is `tool_use` execute the tools
//!     and append `tool_result` blocks, repeat. Cap 16 rounds.
//!   - Tag-protocol outer loop (only when `socket_tx` is set): after
//!     the inner loop settles with `end_turn`, scan buffered assistant
//!     text for `<aciddog-call>` tags, dispatch via the socket, render
//!     `<aciddog-response>` blocks, loop. Reuses `MAX_TAG_CHAIN_ROUNDS`
//!     from claude_code.

pub mod sse;
pub mod types;

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
use crate::ai::event::{parse_usage_anthropic, AIEvent, TurnUsage};
use crate::ai::tag_call;
use crate::ai::tools::{RegistryConfig, ToolRegistry};

use self::sse::SseDecoder;
use self::types::{ContentBlock, Message, Role};

/// Cap on native tool-use rounds inside one user turn. Mirrors the
/// spirit of `MAX_TAG_CHAIN_ROUNDS` — the model should eventually stop
/// asking for tools and finalise its answer.
pub const MAX_NATIVE_TOOL_ROUNDS: usize = 16;

/// Default `max_tokens` for the Anthropic request body. Anthropic
/// requires this field; 8192 is a generous ceiling for interactive
/// turns without being wasteful.
const DEFAULT_MAX_TOKENS: u32 = 8192;

/// Fixed API version header sent with every request. Expose in WCL
/// only if a vendor demands a different value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicApiCore {
    session_id: Uuid,
    api_url: String,
    api_key_env: String,
    model: String,
    prompt_parts: PromptParts,
    allowed_tools: Vec<String>,
    system_prompt: Option<String>,
    env: Vec<(String, String)>,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    http: reqwest::Client,
    started: bool,
    history: Arc<Mutex<Vec<Message>>>,
}

impl AnthropicApiCore {
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

    /// Full assembled first-turn prompt — mirrors `ClaudeCodeCore::first_turn_prompt`
    /// so the TUI's AgentInfo block can surface what the model will see.
    pub fn first_turn_prompt(&self) -> String {
        build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools)
    }
}

#[async_trait]
impl AICore for AnthropicApiCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        // Resolve the API key up front so a missing env var surfaces
        // as a single AIEvent::Error instead of a silent no-op.
        let api_key = match std::env::var(&self.api_key_env) {
            Ok(k) if !k.is_empty() => Some(k),
            _ => None,
        };

        // Prepend the XML preamble to the first user message only.
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
                content: vec![ContentBlock::Text { text: user_text }],
            });

        let (registry, _resolved) = ToolRegistry::from_allowed_with(
            &self.allowed_tools,
            &RegistryConfig {
                bash_env: self.env.clone(),
            },
        );

        let (tx, rx) = mpsc::channel::<AIEvent>(64);

        // Snapshot everything the spawned task needs. Shared refs via
        // Arc keep the `AICore` trait `&mut self` contract intact.
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
                        message: format!(
                            "anthropic_api: env var `{api_key_env}` is not set or empty"
                        ),
                        raw: None,
                    })
                    .await;
                return;
            };

            // Emit a SystemInit so the TUI can show tool list + model
            // in the top AgentInfo block, matching claude_code.
            let _ = tx
                .send(AIEvent::SystemInit {
                    session_id,
                    tools: registry.names().iter().map(|s| s.to_string()).collect(),
                    model: Some(model.clone()),
                    raw: Value::Null,
                })
                .await;

            // Run the full two-layer loop. Buffer assistant text
            // across the whole turn so the tag-protocol scan sees
            // every `<aciddog-call>` even if the model emits text
            // then a tool_use.
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
                if final_stop.as_deref() != Some("end_turn") {
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

                // Feed the rendered responses back as a user message
                // so the model can see what happened.
                let rendered = tag_call::render_responses(&responses);
                history.lock().expect("history poisoned").push(Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text { text: rendered }],
                });
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
        build_first_turn_prompt(&self.prompt_parts, &self.allowed_tools)
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

/// POST + stream loop. Returns the terminal `stop_reason` (or `None`
/// if the stream ended without one) or `Err(())` if the consumer hung
/// up. Executes tool calls and loops until `stop_reason != "tool_use"`
/// or the round cap is hit.
async fn run_native_tool_loop(ctx: InnerCtx) -> Result<Option<String>, ()> {
    let endpoint = format!("{}/v1/messages", ctx.api_url.trim_end_matches('/'));

    for _round in 0..MAX_NATIVE_TOOL_ROUNDS {
        let body = build_request_body(
            &ctx.model,
            &ctx.system_prompt,
            &ctx.history.lock().expect("history poisoned"),
            &ctx.registry.api_tools(),
        );

        let resp = match ctx
            .http
            .post(&endpoint)
            .header("x-api-key", &ctx.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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
                        message: format!("anthropic_api: request failed: {e}"),
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
                    message: format!("anthropic_api: HTTP {status}: {snippet}"),
                    raw: None,
                })
                .await;
            return Ok(None);
        }

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        let mut pending_text: String = String::new();
        let mut pending_tool: Option<PendingTool> = None;
        let mut current_block_type: Option<String> = None;
        let mut stop_reason: Option<String> = None;
        let mut final_raw: Value = Value::Null;
        // Anthropic reports input/cache counts in `message_start.usage`
        // and a final cumulative `output_tokens` in `message_delta.usage`.
        // We merge them here and emit one TurnUsage at end of round.
        let mut turn_usage: Option<TurnUsage> = None;

        let mut decoder = SseDecoder::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = ctx
                        .tx
                        .send(AIEvent::Error {
                            message: format!("anthropic_api: stream error: {e}"),
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
                        warn!(error = %e, "anthropic_api: skipping malformed SSE event");
                        continue;
                    }
                };
                match ev.event.as_str() {
                    "message_start" => {
                        if let Some(u) = ev
                            .data
                            .pointer("/message/usage")
                            .and_then(parse_usage_anthropic)
                        {
                            turn_usage = Some(merge_usage(turn_usage, u));
                        }
                    }
                    "content_block_start" => {
                        let kind = ev
                            .data
                            .pointer("/content_block/type")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        current_block_type = Some(kind.clone());
                        if kind == "tool_use" {
                            pending_tool = Some(PendingTool {
                                id: ev
                                    .data
                                    .pointer("/content_block/id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                name: ev
                                    .data
                                    .pointer("/content_block/name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                partial_json: String::new(),
                            });
                        }
                    }
                    "content_block_delta" => {
                        let delta_type = ev
                            .data
                            .pointer("/delta/type")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        match delta_type {
                            "text_delta" => {
                                if let Some(t) =
                                    ev.data.pointer("/delta/text").and_then(Value::as_str)
                                {
                                    pending_text.push_str(t);
                                }
                            }
                            "input_json_delta" => {
                                if let Some(t) = ev
                                    .data
                                    .pointer("/delta/partial_json")
                                    .and_then(Value::as_str)
                                {
                                    if let Some(p) = pending_tool.as_mut() {
                                        p.partial_json.push_str(t);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_stop" => {
                        match current_block_type.as_deref() {
                            Some("text") => {
                                let text = std::mem::take(&mut pending_text);
                                if !text.is_empty() {
                                    {
                                        let mut guard =
                                            ctx.collect_text.lock().expect("buf poisoned");
                                        guard.push_str(&text);
                                        guard.push('\n');
                                    }
                                    if ctx
                                        .tx
                                        .send(AIEvent::AssistantText {
                                            text: text.clone(),
                                            raw: Value::Null,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        return Err(());
                                    }
                                    assistant_blocks.push(ContentBlock::Text { text });
                                }
                            }
                            Some("tool_use") => {
                                if let Some(p) = pending_tool.take() {
                                    let input = if p.partial_json.is_empty() {
                                        Value::Object(serde_json::Map::new())
                                    } else {
                                        serde_json::from_str(&p.partial_json).unwrap_or(Value::Null)
                                    };
                                    if ctx
                                        .tx
                                        .send(AIEvent::ToolUse {
                                            id: p.id.clone(),
                                            name: p.name.clone(),
                                            input: input.clone(),
                                            raw: input.clone(),
                                        })
                                        .await
                                        .is_err()
                                    {
                                        return Err(());
                                    }
                                    assistant_blocks.push(ContentBlock::ToolUse {
                                        id: p.id,
                                        name: p.name,
                                        input,
                                    });
                                }
                            }
                            _ => {}
                        }
                        current_block_type = None;
                    }
                    "message_delta" => {
                        if let Some(sr) = ev
                            .data
                            .pointer("/delta/stop_reason")
                            .and_then(Value::as_str)
                        {
                            stop_reason = Some(sr.to_string());
                        }
                        // Anthropic only puts the final cumulative
                        // output_tokens count here (and sometimes
                        // updated cache numbers). Merge into the
                        // turn's running totals.
                        if let Some(u) = ev.data.pointer("/usage").and_then(parse_usage_anthropic) {
                            turn_usage = Some(merge_usage(turn_usage, u));
                        }
                    }
                    "message_stop" => {
                        final_raw = ev.data.clone();
                    }
                    "error" => {
                        let msg = ev
                            .data
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("anthropic_api: unspecified error")
                            .to_string();
                        let _ = ctx
                            .tx
                            .send(AIEvent::Error {
                                message: msg,
                                raw: Some(ev.data),
                            })
                            .await;
                        return Ok(stop_reason);
                    }
                    _ => {}
                }
            }
        }

        // Persist the assistant message if it had any content.
        if !assistant_blocks.is_empty() {
            ctx.history.lock().expect("history poisoned").push(Message {
                role: Role::Assistant,
                content: assistant_blocks.clone(),
            });
        }

        // Surface the per-round TurnResult so the TUI can close out the
        // block visually.
        let _ = ctx
            .tx
            .send(AIEvent::TurnResult {
                stop_reason: stop_reason.clone(),
                total_cost_usd: None,
                usage: turn_usage,
                raw: final_raw,
            })
            .await;

        match stop_reason.as_deref() {
            Some("tool_use") => {
                // Execute every tool_use block from this assistant
                // message, in parallel, and append the results as a
                // user message for the next round.
                let tool_calls: Vec<(String, String, Value)> = assistant_blocks
                    .into_iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => Some((id, name, input)),
                        _ => None,
                    })
                    .collect();

                if tool_calls.is_empty() {
                    return Ok(stop_reason);
                }

                let mut futures_vec = Vec::with_capacity(tool_calls.len());
                for (id, name, input) in &tool_calls {
                    let registry = ctx.registry.clone();
                    let name_c = name.clone();
                    let id_c = id.clone();
                    let input_c = input.clone();
                    futures_vec.push(async move {
                        let output = match registry.get(&name_c) {
                            Some(tool) => tool.invoke(input_c).await,
                            None => crate::ai::tools::ToolOutput::error(format!(
                                "unknown tool `{name_c}`"
                            )),
                        };
                        (id_c, output)
                    });
                }
                let outputs = futures::future::join_all(futures_vec).await;

                let mut result_blocks = Vec::with_capacity(outputs.len());
                for ((_, name, _), (id, output)) in tool_calls.iter().zip(outputs.into_iter()) {
                    // Mirror ToolResult events so the TUI can correlate.
                    let content_json = Value::String(output.text.clone());
                    if ctx
                        .tx
                        .send(AIEvent::ToolResult {
                            tool_use_id: id.clone(),
                            content: content_json.clone(),
                            is_error: output.is_error,
                            raw: json!({
                                "type": "tool_result",
                                "tool_use_id": id,
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

                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: id,
                        content: output.text,
                        is_error: if output.is_error { Some(true) } else { None },
                    });
                }

                ctx.history.lock().expect("history poisoned").push(Message {
                    role: Role::User,
                    content: result_blocks,
                });
                // loop for another native round
                continue;
            }
            _ => return Ok(stop_reason),
        }
    }

    // Round cap reached — treat as a soft stop with an error event so
    // the model can see something went sideways.
    let _ = ctx
        .tx
        .send(AIEvent::Error {
            message: format!(
                "anthropic_api: native tool-use loop exceeded {MAX_NATIVE_TOOL_ROUNDS} rounds; stopping."
            ),
            raw: None,
        })
        .await;
    Ok(Some("max_tool_rounds".to_string()))
}

struct PendingTool {
    id: String,
    name: String,
    partial_json: String,
}

fn build_request_body(
    model: &str,
    system_prompt: &Option<String>,
    history: &[Message],
    tools: &[Value],
) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "stream": true,
        "messages": history,
    });
    if let Some(sp) = system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        body["system"] = Value::String(sp.to_string());
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    body
}

fn build_first_turn_prompt(parts: &PromptParts, allowed_tools: &[String]) -> String {
    // Reuse the same XML layout as claude_code so agents see an
    // identical preamble regardless of core.
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
            ));
        } else {
            if let Some(w) = &cli.worker {
                body.push_str(&tag_worker_preamble(&w.parent_id, &w.task));
            }
            body.push_str(&tag_preamble(&contactable, &creatable, &memory_banks));
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

// Silence unused-import warnings when features differ.
#[allow(dead_code)]
fn _compiler_hint_debug(_: &dyn std::fmt::Debug) {
    debug!("noop");
}

/// Merge a freshly-parsed `TurnUsage` into the running totals for the
/// current turn. Anthropic emits `input_tokens` / cache numbers in
/// `message_start` and the final `output_tokens` in `message_delta`,
/// so we merge by taking the **maximum** of each field — the later
/// event always carries equal or higher cumulative counts.
fn merge_usage(prev: Option<TurnUsage>, next: TurnUsage) -> TurnUsage {
    let p = prev.unwrap_or_default();
    TurnUsage {
        input_tokens: p.input_tokens.max(next.input_tokens),
        output_tokens: p.output_tokens.max(next.output_tokens),
        cache_read_input_tokens: p.cache_read_input_tokens.max(next.cache_read_input_tokens),
        cache_creation_input_tokens: p
            .cache_creation_input_tokens
            .max(next.cache_creation_input_tokens),
    }
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
        let core = AnthropicApiCore::new(
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
    fn request_body_includes_system_when_present() {
        let body = build_request_body("m", &Some("you are x".into()), &[], &[]);
        assert_eq!(body["system"], "you are x");
    }

    #[test]
    fn request_body_serialises_history() {
        let hist = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: "hi".into() }],
        }];
        let body = build_request_body("m", &None, &hist, &[]);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][0]["text"], "hi");
    }

    #[tokio::test]
    async fn missing_env_var_surfaces_as_error_event() {
        // Use a name that's extraordinarily unlikely to exist.
        let env_name = "_ACIDDOG_TEST_MISSING_KEY_XYZ";
        std::env::remove_var(env_name);

        let mut core = AnthropicApiCore::new(
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
