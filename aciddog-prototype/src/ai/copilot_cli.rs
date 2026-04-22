//! GitHub Copilot CLI AI core.
//!
//! Drives the `copilot` binary the same way [`ClaudeCodeCore`](super::claude_code::ClaudeCodeCore)
//! drives `claude`. Structure and prompt assembly are deliberately
//! mirrored so swapping cores in aciddog.wcl changes behaviour
//! predictably. The main divergences are:
//!
//! - Copilot has no system-prompt flag; the config variant omits it.
//! - Prompt is passed via `-p <text>` argv, not piped on stdin.
//! - Copilot's native tool names are lowercase (`bash`, `view`, `create`,
//!   `edit`, `glob`, `grep`). The core translates from the aciddog-side
//!   capitalized names at spawn time so the rest of the codebase
//!   (`shell_tool_name`, `uses_tag_ipc`, config) stays uniform.
//! - Session id is managed with `--resume=<uuid>` on every turn; the
//!   flag acts as create-or-resume when given a UUID we generated.
//! - Output is copilot's own JSONL format — parsed by
//!   [`parse_copilot_line`], which maps the observed event types to the
//!   shared [`AIEvent`] enum.
//! - Copilot never emits a claude-style `system.init` event, so the
//!   core synthesizes an [`AIEvent::SystemInit`] itself at the start of
//!   each turn using the UUID it already owns.

use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::agent_ipc::preamble::{
    cli_preamble, tag_preamble, tag_worker_preamble, worker_preamble,
};
use crate::agent_ipc::{AgentSocketEvent, Request, RequestCmd, Response};

use super::claude_code::{PromptParts, MAX_TAG_CHAIN_ROUNDS};
use super::core::{AICore, AIError};
use super::event::{AIEvent, TurnUsage};
use super::tag_call;
use super::tools::docs::built_in_tool_doc;

const DEFAULT_BINARY: &str = "copilot";

/// Aciddog-side tool names that have a copilot built-in equivalent.
/// The config validator uses this for the `CopilotCli` variant so
/// unknown or unmappable names are rejected at parse time.
pub const COPILOT_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Write", "Edit", "Glob", "Grep"];

/// Translate an aciddog tool name to copilot's native (lowercase) name.
/// Returns `None` when the name has no copilot equivalent.
pub fn translate_tool(name: &str) -> Option<&'static str> {
    match name {
        "Bash" => Some("bash"),
        "Read" => Some("view"),
        "Write" => Some("create"),
        "Edit" => Some("edit"),
        "Glob" => Some("glob"),
        "Grep" => Some("grep"),
        _ => None,
    }
}

/// One-line description used inside the first-turn `<tools>` preamble.
/// Reuses [`built_in_tool_doc`] so the English stays in one place; the
/// lowercase copilot name is mapped back to the aciddog key before
/// delegation.
fn copilot_tool_doc(copilot_name: &str) -> &'static str {
    let aciddog = match copilot_name {
        "bash" => "Bash",
        "view" => "Read",
        "create" => "Write",
        "edit" => "Edit",
        "glob" => "Glob",
        "grep" => "Grep",
        _ => return "enabled",
    };
    built_in_tool_doc(aciddog)
}

pub struct CopilotCliCore {
    binary: String,
    session_id: Uuid,
    /// Structured first-turn prompt sources. Composed into the actual
    /// XML-tagged prompt by `build_first_turn_prompt` on the first turn.
    /// Ignored when `started` is true (resumed sessions).
    prompt_parts: PromptParts,
    /// Aciddog-side tool names (capitalized). Translated to copilot's
    /// lowercase names at spawn time for `--available-tools`.
    allowed_tools: Vec<String>,
    /// When `Some`, passed as `--model <id>` on every turn. When `None`,
    /// copilot picks its own default (currently `claude-sonnet-4.5`).
    /// Must match a model copilot actually offers; otherwise the child
    /// exits with an "is not available" error.
    model: Option<String>,
    env: Vec<(String, String)>,
    /// Set to `Some` only when the agent lacks a shell tool and must
    /// coordinate through inline `<aciddog-call>` tags. When present,
    /// `turn` runs the tag-chain loop; otherwise it runs the child once
    /// and returns.
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    started: bool,
}

impl CopilotCliCore {
    /// Construct a core for a brand-new session. The caller owns the
    /// session id so it can be shared with the chat UI and the agent IPC
    /// layer before the first turn has run. Copilot treats
    /// `--resume=<uuid>` as create-or-resume, so the same UUID works
    /// for the first and every subsequent spawn.
    pub fn new(session_id: Uuid, prompt_parts: PromptParts, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            session_id,
            prompt_parts,
            allowed_tools,
            model: None,
            env: Vec::new(),
            socket_tx: None,
            started: false,
        }
    }

    /// Construct a core that resumes a pre-existing session by id. The
    /// first-turn prompt has already been delivered to the previous
    /// session, so no `PromptParts` are needed — `started` starts true
    /// and every turn forwards the user prompt verbatim.
    pub fn with_session_id(session_id: Uuid, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            session_id,
            prompt_parts: PromptParts::default(),
            allowed_tools,
            model: None,
            env: Vec::new(),
            socket_tx: None,
            started: true,
        }
    }

    /// Pin a specific copilot model for every turn (`--model <id>`).
    /// Accepts copilot's native model ids — e.g. `claude-haiku-4.5`,
    /// `gpt-5.2`. An unknown id surfaces as an `AIEvent::Error` when
    /// copilot rejects it on spawn.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Override the binary path (useful for tests pointing at `/bin/false`, etc).
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Additional environment variables to pass to the spawned copilot process.
    /// Used to inject `ACIDDOG_AGENT_ID` / `ACIDDOG_AGENT_SOCKET` so the
    /// agent can call back into aciddog via the IPC socket.
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// Opt into the tag-protocol chain: the core intercepts
    /// `<aciddog-call>` tags in assistant output and dispatches them
    /// through the provided channel, injecting the responses as the
    /// next turn's prompt. Only set this when the agent has no shell
    /// tool available.
    pub fn with_socket_tx(mut self, socket_tx: mpsc::Sender<AgentSocketEvent>) -> Self {
        self.socket_tx = Some(socket_tx);
        self
    }

    fn validate_tools(&self) -> Result<(), AIError> {
        for tool in &self.allowed_tools {
            if tool.is_empty() || tool.contains(',') || tool.chars().any(char::is_whitespace) {
                return Err(AIError::InvalidToolName(tool.clone()));
            }
            if translate_tool(tool).is_none() {
                return Err(AIError::InvalidToolName(tool.clone()));
            }
        }
        Ok(())
    }

    fn build_prompt(&mut self, user_prompt: &str) -> String {
        if self.started {
            user_prompt.to_string()
        } else {
            self.started = true;
            let preamble = self.build_first_turn_prompt();
            if preamble.is_empty() {
                user_prompt.to_string()
            } else if user_prompt.is_empty() {
                preamble
            } else {
                format!("{preamble}\n\n{user_prompt}")
            }
        }
    }

    /// Assemble the structured first-turn prompt: `<role>`, `<team>`,
    /// `<memory>`, `<tools>`, `<instructions>`. Same shape as the claude
    /// core's preamble so prompt-engineering insights transfer.
    pub fn first_turn_prompt(&self) -> String {
        self.build_first_turn_prompt()
    }

    fn build_first_turn_prompt(&self) -> String {
        let mut sections: Vec<String> = Vec::new();
        if let Some(role) = self
            .prompt_parts
            .role
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            sections.push(format!("<role>\n{role}\n</role>"));
        }
        if let Some(team) = self
            .prompt_parts
            .team
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            sections.push(format!("<team>\n{team}\n</team>"));
        }
        if let Some(memory) = self
            .prompt_parts
            .memory
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            sections.push(format!("<memory>\n{memory}\n</memory>"));
        }
        sections.push(self.build_tools_section());
        sections.push(format!(
            "<instructions>\n{}\n</instructions>",
            self.prompt_parts.instructions.trim()
        ));
        sections.join("\n\n")
    }

    fn build_tools_section(&self) -> String {
        let mut body = String::new();
        body.push_str("## Built-in tools enabled\n\n");
        if self.allowed_tools.is_empty() {
            body.push_str("(none)\n");
        } else {
            for name in &self.allowed_tools {
                // Show the copilot-native (lowercase) name so the model
                // sees the exact identifier it will use to invoke the
                // tool. Unknown names (shouldn't happen after
                // validate_tools) degrade gracefully.
                let display = translate_tool(name).unwrap_or(name.as_str());
                body.push_str(&format!("- `{display}` — {}\n", copilot_tool_doc(display)));
            }
        }

        if let Some(cli) = &self.prompt_parts.aciddog_cli {
            let has_bash = self.allowed_tools.iter().any(|t| t == "Bash");
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
}

#[async_trait]
impl AICore for CopilotCliCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        self.validate_tools()?;

        let is_first = !self.started;
        let first_prompt = self.build_prompt(prompt);

        let ctx = Arc::new(RoundCtx {
            binary: self.binary.clone(),
            session_id: self.session_id,
            allowed_tools: self.allowed_tools.clone(),
            model: self.model.clone(),
            env: self.env.clone(),
        });

        let (tx, rx) = mpsc::channel::<AIEvent>(64);
        let socket_tx = self.socket_tx.clone();

        // Synthesize SystemInit ourselves. Copilot never emits a
        // claude-style system.init; downstream consumers (TUI chat
        // metadata, integration tests) still want one event that pins
        // the session id and advertises the enabled tools before any
        // assistant output arrives. Re-emit on every turn for parity
        // with claude_code's behaviour.
        let synth_init = AIEvent::SystemInit {
            session_id: self.session_id,
            tools: self.allowed_tools.clone(),
            model: None,
            raw: Value::Null,
        };
        if tx.send(synth_init).await.is_err() {
            // Consumer dropped before we even started; nothing to do.
            return Ok(ReceiverStream::new(rx).boxed());
        }

        tokio::spawn(async move {
            match socket_tx {
                None => {
                    let _ = run_round(&ctx, is_first, first_prompt, tx, None).await;
                }
                Some(socket_tx) => {
                    run_chain(&ctx, is_first, first_prompt, tx, socket_tx).await;
                }
            }
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
        self.build_first_turn_prompt()
    }
}

/// Immutable per-turn snapshot used by the spawn task. Owning copies
/// sidestep the lifetime mismatch between `&mut self` and a task that
/// outlives the `turn()` call frame. No system-prompt path because
/// copilot doesn't support one.
struct RoundCtx {
    binary: String,
    session_id: Uuid,
    allowed_tools: Vec<String>,
    /// See `CopilotCliCore::model`.
    model: Option<String>,
    env: Vec<(String, String)>,
}

/// Spawn one copilot child, forward every parsed `AIEvent` from stdout
/// to `tx`, await the child's exit. When `collect_text` is `Some`, every
/// `AssistantText` event's text is also appended to the shared buffer
/// so the caller can post-process for tag calls.
///
/// Returns `Err` only if the `tx` channel closes early (consumer gone).
/// Child-spawn / stderr / non-zero-exit errors surface as
/// `AIEvent::Error` through `tx` and the function returns `Ok(())`.
async fn run_round(
    ctx: &RoundCtx,
    _is_first: bool,
    prompt: String,
    tx: mpsc::Sender<AIEvent>,
    collect_text: Option<Arc<Mutex<String>>>,
) -> Result<(), ()> {
    // Translate aciddog names → copilot's lowercase names. Validated
    // upstream, but a None here would silently drop the tool — fail
    // loudly instead.
    let mut translated: Vec<&'static str> = Vec::with_capacity(ctx.allowed_tools.len());
    for name in &ctx.allowed_tools {
        match translate_tool(name) {
            Some(n) => translated.push(n),
            None => {
                let _ = tx
                    .send(AIEvent::Error {
                        message: format!("copilot: unmappable tool name `{name}`"),
                        raw: None,
                    })
                    .await;
                return Ok(());
            }
        }
    }
    let tools_arg = translated.join(",");
    let session_id = ctx.session_id.to_string();

    let mut cmd = Command::new(&ctx.binary);
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--yolo")
        .arg("--output-format")
        .arg("json")
        .arg("--disable-builtin-mcps")
        .arg("--no-custom-instructions")
        .arg(format!("--resume={session_id}"));

    if !tools_arg.is_empty() {
        cmd.arg("--available-tools").arg(&tools_arg);
    }

    if let Some(model) = &ctx.model {
        cmd.arg("--model").arg(model);
    }

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    debug!(binary = %ctx.binary, session_id = %session_id, "spawning copilot");

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let err = if e.kind() == std::io::ErrorKind::NotFound {
                AIError::NotFound
            } else {
                AIError::Spawn(e)
            };
            let _ = tx
                .send(AIEvent::Error {
                    message: err.to_string(),
                    raw: None,
                })
                .await;
            return Ok(());
        }
    };

    let stdout = child
        .stdout
        .take()
        .expect("stdout was piped, must be present");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was piped, must be present");

    // Stderr is drained in parallel; we await it at the end to include
    // its contents in any failure report.
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf).await;
        if !buf.trim().is_empty() {
            warn!(stderr = %buf, "copilot stderr");
        }
        buf
    });

    let mut reader = BufReader::new(stdout).lines();
    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                for ev in parse_copilot_line(&line) {
                    if let Some(buf) = collect_text.as_ref() {
                        if let AIEvent::AssistantText { text, .. } = &ev {
                            let mut guard = buf.lock().expect("collect_text poisoned");
                            guard.push_str(text);
                        }
                    }
                    if tx.send(ev).await.is_err() {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        let _ = stderr_handle.await;
                        return Err(());
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                let _ = tx
                    .send(AIEvent::Error {
                        message: format!("stdout read error: {e}"),
                        raw: None,
                    })
                    .await;
                break;
            }
        }
    }

    let status = child.wait().await;
    let stderr_buf = stderr_handle.await.unwrap_or_default();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let code = s
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into());
            let msg = format!(
                "copilot exited with status {code}; stderr: {}",
                stderr_buf.trim()
            );
            let _ = tx
                .send(AIEvent::Error {
                    message: msg,
                    raw: None,
                })
                .await;
        }
        Err(e) => {
            let _ = tx
                .send(AIEvent::Error {
                    message: format!("failed to await copilot child: {e}"),
                    raw: None,
                })
                .await;
        }
    }

    Ok(())
}

/// Run the tag-protocol chain: repeatedly spawn copilot, parse any
/// `<aciddog-call>` tags out of its assistant text, dispatch them
/// through the orchestrator socket channel, and feed the results back
/// as the next round's prompt. Caps the loop at `MAX_TAG_CHAIN_ROUNDS`
/// per user turn. Logic mirrors `claude_code::run_chain`.
async fn run_chain(
    ctx: &RoundCtx,
    mut is_first: bool,
    first_prompt: String,
    tx: mpsc::Sender<AIEvent>,
    socket_tx: mpsc::Sender<AgentSocketEvent>,
) {
    let mut current_prompt = first_prompt;
    for round in 0..MAX_TAG_CHAIN_ROUNDS {
        let buf = Arc::new(Mutex::new(String::new()));
        if run_round(
            ctx,
            is_first,
            std::mem::take(&mut current_prompt),
            tx.clone(),
            Some(buf.clone()),
        )
        .await
        .is_err()
        {
            return;
        }
        is_first = false;

        let assistant_text = {
            let guard = buf.lock().expect("collect_text poisoned");
            guard.clone()
        };
        let calls = tag_call::extract_calls(&assistant_text);
        if calls.is_empty() {
            return;
        }

        let mut responses: Vec<(usize, Response)> = Vec::with_capacity(calls.len());
        for (i, call) in calls.iter().enumerate() {
            let input_json = match &call.parsed {
                Ok(cmd) => serde_json::to_value(cmd).unwrap_or(Value::Null),
                Err(_) => serde_json::from_str::<Value>(&call.raw_body)
                    .unwrap_or_else(|_| Value::String(call.raw_body.clone())),
            };
            let call_id = format!("aciddog-call-{round}-{i}");
            let _ = tx
                .send(AIEvent::ToolUse {
                    id: call_id.clone(),
                    name: "aciddog-call".to_string(),
                    input: input_json.clone(),
                    raw: input_json,
                })
                .await;

            let response = match &call.parsed {
                Ok(cmd) => dispatch_call(cmd.clone(), ctx.session_id, &socket_tx).await,
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

        current_prompt = tag_call::render_responses(&responses);
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
}

/// Build an `AgentSocketEvent`, push it onto the orchestrator channel,
/// and await the reply. Channel closure or dropped reply surface as
/// synthesized error responses.
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

/// Parse one line of copilot's `--output-format json` stream into zero
/// or more [`AIEvent`]s.
///
/// Mapping (derived from an empirical trace against the real binary):
/// - `assistant.message_delta` → `Other` (streamed word-by-word; the
///   TUI renders one chat block per `AssistantText`, so surfacing
///   deltas shatters a sentence across many balloons — emit only the
///   consolidated message instead)
/// - `assistant.message` → if `data.content` is non-empty, emit one
///   `AssistantText { text: content }`; then, for each entry in
///   `data.toolRequests`, emit `ToolUse { id: toolCallId, name, input:
///   arguments }`. A single message can carry both.
/// - `tool.execution_complete` → `ToolResult { tool_use_id: toolCallId,
///   content: data.result, is_error: !data.success }`
/// - `result` → `TurnResult { stop_reason: None, total_cost_usd: None }`
///   (copilot reports `premiumRequests`, not a USD cost)
/// - Everything else (`session.*`, `user.message`,
///   `assistant.turn_*`, `assistant.reasoning*`,
///   `tool.execution_start`, unknown types) → `Other(raw)`.
pub(crate) fn parse_copilot_line(line: &str) -> Vec<AIEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return vec![AIEvent::Error {
                message: format!("failed to parse copilot stream line: {e}"),
                raw: None,
            }];
        }
    };

    let ty = value.get("type").and_then(Value::as_str).unwrap_or("");

    match ty {
        "assistant.message" => parse_assistant_message(value),
        "tool.execution_complete" => parse_tool_complete(value),
        "result" => {
            // Copilot's `usage` field is currently emitted as `{}`
            // (see test fixture). Try to parse anyway in case the
            // shape changes; an empty object yields `None`.
            let usage = parse_copilot_usage(value.get("usage"));
            vec![AIEvent::TurnResult {
                stop_reason: None,
                total_cost_usd: None,
                usage,
                raw: value,
            }]
        }
        _ => vec![AIEvent::Other(value)],
    }
}

/// Best-effort extraction of token usage from copilot's `result.usage`.
/// Copilot currently ships an empty object, but if/when it starts
/// reporting we try OpenAI-style names first (`prompt_tokens` /
/// `completion_tokens`) and fall back to Anthropic-style
/// (`input_tokens` / `output_tokens`).
fn parse_copilot_usage(usage: Option<&Value>) -> Option<TurnUsage> {
    let obj = usage?.as_object()?;
    let mut u = TurnUsage::default();
    let mut any = false;
    for (k, v) in obj {
        let Some(n) = v.as_u64() else { continue };
        match k.as_str() {
            "prompt_tokens" | "input_tokens" => {
                u.input_tokens = n;
                any = true;
            }
            "completion_tokens" | "output_tokens" => {
                u.output_tokens = n;
                any = true;
            }
            "cache_read_input_tokens" | "cached_tokens" => {
                u.cache_read_input_tokens = n;
                any = true;
            }
            "cache_creation_input_tokens" => {
                u.cache_creation_input_tokens = n;
                any = true;
            }
            _ => {}
        }
    }
    if any {
        Some(u)
    } else {
        None
    }
}

fn parse_assistant_message(raw: Value) -> Vec<AIEvent> {
    let content = raw
        .pointer("/data/content")
        .and_then(Value::as_str)
        .unwrap_or("");
    let requests = raw
        .pointer("/data/toolRequests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if content.is_empty() && requests.is_empty() {
        return vec![AIEvent::Other(raw)];
    }

    let mut events = Vec::with_capacity(requests.len() + 1);
    if !content.is_empty() {
        events.push(AIEvent::AssistantText {
            text: content.to_string(),
            raw: raw.clone(),
        });
    }
    for req in requests {
        let id = req
            .get("toolCallId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = req
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let input = req.get("arguments").cloned().unwrap_or(Value::Null);
        events.push(AIEvent::ToolUse {
            id,
            name,
            input,
            raw: req.clone(),
        });
    }
    events
}

fn parse_tool_complete(raw: Value) -> Vec<AIEvent> {
    let tool_use_id = raw
        .pointer("/data/toolCallId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // Prefer the string content for the TUI; fall back to the whole
    // result object when copilot stops putting a plain string there.
    let content = raw
        .pointer("/data/result/content")
        .cloned()
        .or_else(|| raw.pointer("/data/result").cloned())
        .unwrap_or(Value::Null);
    let is_error = !raw
        .pointer("/data/success")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    vec![AIEvent::ToolResult {
        tool_use_id,
        content,
        is_error,
        raw,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::claude_code::{AciddogCliCtx, WorkerCtx};

    fn parts(instructions: &str) -> PromptParts {
        PromptParts {
            instructions: instructions.into(),
            ..PromptParts::default()
        }
    }

    fn fresh(instructions: &str, tools: Vec<String>) -> CopilotCliCore {
        CopilotCliCore::new(Uuid::new_v4(), parts(instructions), tools)
    }

    #[test]
    fn rejects_empty_tool_name() {
        let core = fresh("i", vec!["".into()]);
        assert!(matches!(
            core.validate_tools(),
            Err(AIError::InvalidToolName(s)) if s.is_empty()
        ));
    }

    #[test]
    fn rejects_tool_name_with_comma() {
        let core = fresh("i", vec!["Ba,sh".into()]);
        assert!(matches!(
            core.validate_tools(),
            Err(AIError::InvalidToolName(_))
        ));
    }

    #[test]
    fn rejects_tool_name_with_whitespace() {
        let core = fresh("i", vec!["Bash Edit".into()]);
        assert!(matches!(
            core.validate_tools(),
            Err(AIError::InvalidToolName(_))
        ));
    }

    #[test]
    fn rejects_unknown_tool_name() {
        let core = fresh("i", vec!["WebFetch".into()]);
        assert!(matches!(
            core.validate_tools(),
            Err(AIError::InvalidToolName(s)) if s == "WebFetch"
        ));
    }

    #[test]
    fn accepts_valid_tool_names() {
        let core = fresh("i", vec!["Bash".into(), "Edit".into(), "Read".into()]);
        assert!(core.validate_tools().is_ok());
    }

    #[test]
    fn translator_maps_every_allowed_name() {
        assert_eq!(translate_tool("Bash"), Some("bash"));
        assert_eq!(translate_tool("Read"), Some("view"));
        assert_eq!(translate_tool("Write"), Some("create"));
        assert_eq!(translate_tool("Edit"), Some("edit"));
        assert_eq!(translate_tool("Glob"), Some("glob"));
        assert_eq!(translate_tool("Grep"), Some("grep"));
        assert_eq!(translate_tool("WebFetch"), None);
        assert_eq!(translate_tool("Nonsense"), None);
    }

    #[test]
    fn first_turn_wraps_instructions_in_xml_block() {
        let mut core = fresh("be a helper", vec![]);
        let p = core.build_prompt("hello");
        assert!(
            p.contains("<instructions>\nbe a helper\n</instructions>"),
            "missing instructions block: {p}"
        );
        assert!(p.ends_with("\n\nhello"), "user prompt missing: {p}");
        assert!(core.started);
    }

    #[test]
    fn second_turn_does_not_prepend_instructions() {
        let mut core = fresh("you are a helper", vec![]);
        let _ = core.build_prompt("hello");
        let p = core.build_prompt("another question");
        assert_eq!(p, "another question");
    }

    #[test]
    fn first_turn_prompt_emits_role_team_tools_instructions_in_order() {
        let parts = PromptParts {
            role: Some("be terse".into()),
            team: Some("you can message: bob".into()),
            memory: None,
            skills: None,
            instructions: "do the thing".into(),
            aciddog_cli: None,
        };
        let core = CopilotCliCore::new(Uuid::new_v4(), parts, vec!["Read".into()]);
        let p = core.first_turn_prompt();
        let role_pos = p.find("<role>").expect("role");
        let team_pos = p.find("<team>").expect("team");
        let tools_pos = p.find("<tools>").expect("tools");
        let instr_pos = p.find("<instructions>").expect("instructions");
        assert!(role_pos < team_pos && team_pos < tools_pos && tools_pos < instr_pos);
        // Lowercase copilot name appears in the preamble, not the
        // aciddog-side "Read".
        assert!(p.contains("- `view` —"), "missing lowercase Read→view: {p}");
        assert!(!p.contains("- `Read` —"), "aciddog name leaked: {p}");
    }

    #[test]
    fn role_block_omitted_when_role_is_none_or_blank() {
        let core = CopilotCliCore::new(Uuid::new_v4(), parts("i"), vec![]);
        let p = core.first_turn_prompt();
        assert!(!p.contains("<role>"), "no role block expected: {p}");

        let core = CopilotCliCore::new(
            Uuid::new_v4(),
            PromptParts {
                role: Some("   \n  ".into()),
                instructions: "i".into(),
                ..PromptParts::default()
            },
            vec![],
        );
        assert!(!core.first_turn_prompt().contains("<role>"));
    }

    #[test]
    fn tools_block_shows_copilot_lowercase_names() {
        let core = CopilotCliCore::new(
            Uuid::new_v4(),
            parts("i"),
            vec!["Bash".into(), "Read".into(), "Write".into()],
        );
        let p = core.first_turn_prompt();
        assert!(p.contains("- `bash` —"), "missing bash: {p}");
        assert!(p.contains("- `view` —"), "missing view: {p}");
        assert!(p.contains("- `create` —"), "missing create: {p}");
        assert!(!p.contains("- `Bash` —"), "Bash leaked: {p}");
    }

    #[test]
    fn tools_block_uses_cli_preamble_when_bash_present() {
        let parts = PromptParts {
            instructions: "i".into(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: "/usr/bin/aciddog".into(),
                contactable: vec!["bob".into()],
                creatable: vec![],
                memory_banks: vec![],
                worker: None,
            }),
            ..PromptParts::default()
        };
        let core = CopilotCliCore::new(Uuid::new_v4(), parts, vec!["Bash".into(), "Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "missing tools open tag: {p}");
        assert!(p.contains("- `bash` —"), "missing bash builtin line: {p}");
        assert!(
            p.contains("/usr/bin/aciddog test"),
            "missing CLI preamble header (binary path): {p}"
        );
        assert!(p.contains("| send |"), "missing CLI send row: {p}");
        assert!(
            !p.contains("<aciddog-call>"),
            "should not include tag preamble: {p}"
        );
    }

    #[test]
    fn tools_block_uses_tag_preamble_when_bash_absent() {
        let parts = PromptParts {
            instructions: "i".into(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: "/usr/bin/aciddog".into(),
                contactable: vec!["bob".into()],
                creatable: vec![],
                memory_banks: vec![],
                worker: None,
            }),
            ..PromptParts::default()
        };
        let core = CopilotCliCore::new(Uuid::new_v4(), parts, vec!["Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<aciddog-call>"), "missing tag preamble: {p}");
        assert!(
            !p.contains("/usr/bin/aciddog send"),
            "CLI preamble leaked into tag mode: {p}"
        );
    }

    #[test]
    fn tools_block_includes_worker_preamble_when_worker_ctx_present() {
        let parts = PromptParts {
            instructions: "i".into(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: "/a".into(),
                contactable: vec!["coder".into()],
                creatable: vec![],
                memory_banks: vec![],
                worker: Some(WorkerCtx {
                    parent_id: "coder".into(),
                    task: "find FOO".into(),
                }),
            }),
            ..PromptParts::default()
        };
        let core = CopilotCliCore::new(Uuid::new_v4(), parts, vec!["Bash".into()]);
        let p = core.first_turn_prompt();
        assert!(
            p.contains("spawned by `coder`"),
            "missing worker preamble: {p}"
        );
        assert!(p.contains("find FOO"), "missing task line: {p}");
        assert!(
            p.contains("/a worker complete --body"),
            "missing worker complete: {p}"
        );
    }

    #[test]
    fn tools_block_omits_aciddog_section_when_cli_ctx_none() {
        let core = CopilotCliCore::new(Uuid::new_v4(), parts("i"), vec!["Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "tools always emitted: {p}");
        assert!(!p.contains("aciddog"), "no aciddog docs expected: {p}");
    }

    #[test]
    fn new_uses_caller_provided_session_id() {
        let id = Uuid::new_v4();
        let core = CopilotCliCore::new(id, parts("i"), vec![]);
        assert_eq!(core.session_id(), Some(id));
        assert!(!core.started, "new() sessions have not started yet");
    }

    #[test]
    fn with_session_id_starts_as_already_started() {
        let id = Uuid::new_v4();
        let mut core = CopilotCliCore::with_session_id(id, vec![]);
        assert_eq!(core.session_id(), Some(id));
        let p = core.build_prompt("hi");
        assert_eq!(p, "hi");
    }

    #[test]
    fn with_model_stores_model_id() {
        let core = fresh("i", vec![]).with_model("claude-haiku-4.5");
        assert_eq!(core.model.as_deref(), Some("claude-haiku-4.5"));
    }

    #[test]
    fn default_core_has_no_model_pinned() {
        let core = fresh("i", vec![]);
        assert!(core.model.is_none());
    }

    #[test]
    fn with_env_stores_entries() {
        let core = fresh("i", vec![])
            .with_env(vec![("FOO".into(), "1".into()), ("BAR".into(), "2".into())]);
        assert_eq!(core.env.len(), 2);
        assert_eq!(core.env[0], ("FOO".into(), "1".into()));
    }

    #[test]
    fn shell_tool_name_is_bash() {
        let core = fresh("i", vec![]);
        assert_eq!(core.shell_tool_name(), Some("Bash"));
    }

    #[test]
    fn with_socket_tx_enables_tag_mode() {
        let (tx, _rx) = mpsc::channel::<AgentSocketEvent>(1);
        let core = fresh("i", vec![]).with_socket_tx(tx);
        assert!(core.socket_tx.is_some());
    }

    #[tokio::test]
    async fn system_init_is_synthesized_as_first_event() {
        let id = Uuid::new_v4();
        let mut core = CopilotCliCore::new(id, parts("i"), vec!["Read".into()])
            .with_binary("/this/path/definitely/does/not/exist/copilot");
        let mut stream = core.turn("hi").await.expect("turn returns ok");
        let first = stream.next().await.expect("at least one event");
        match first {
            AIEvent::SystemInit {
                session_id, tools, ..
            } => {
                assert_eq!(session_id, id);
                assert_eq!(tools, vec!["Read".to_string()]);
            }
            other => panic!("expected SystemInit as first event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn spawn_failure_surfaces_as_error_event_in_stream() {
        let mut core =
            fresh("i", vec![]).with_binary("/this/path/definitely/does/not/exist/copilot");
        let mut stream = core.turn("hi").await.expect("turn returns ok");
        let mut saw_error = false;
        while let Some(ev) = stream.next().await {
            if matches!(ev, AIEvent::Error { .. }) {
                saw_error = true;
                break;
            }
        }
        assert!(saw_error, "expected an AIEvent::Error in the stream");
    }

    // ------------------ parse_copilot_line tests ------------------

    #[test]
    fn parse_empty_and_whitespace_lines_are_noop() {
        assert!(parse_copilot_line("").is_empty());
        assert!(parse_copilot_line("   \n").is_empty());
    }

    #[test]
    fn parse_malformed_json_becomes_error_event() {
        let evs = parse_copilot_line("{not json");
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Error { .. }));
    }

    #[test]
    fn parse_message_delta_is_swallowed() {
        // Deltas are streamed word-by-word; surfacing each one as an
        // AssistantText shatters a single logical message across many
        // TUI chat blocks. We wait for `assistant.message` instead.
        let line = r#"{"type":"assistant.message_delta","data":{"messageId":"m1","deltaContent":"hello "}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        assert!(
            matches!(evs[0], AIEvent::Other(_)),
            "deltas should be swallowed as Other; got {:?}",
            evs[0]
        );
    }

    #[test]
    fn parse_assistant_message_with_tool_request_emits_tool_use() {
        let line = r#"{"type":"assistant.message","data":{"messageId":"m1","content":"","toolRequests":[{"toolCallId":"t1","name":"bash","arguments":{"command":"ls"},"type":"function"}]}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolUse {
                id, name, input, ..
            } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "bash");
                assert_eq!(input.get("command").and_then(Value::as_str), Some("ls"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_message_with_content_emits_single_assistant_text() {
        let line = r#"{"type":"assistant.message","data":{"messageId":"m1","content":"Done! It worked.","toolRequests":[]}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::AssistantText { text, .. } => assert_eq!(text, "Done! It worked."),
            other => panic!("expected single AssistantText, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_message_with_content_and_tool_requests_emits_both() {
        let line = r#"{"type":"assistant.message","data":{"messageId":"m1","content":"Running the command.","toolRequests":[{"toolCallId":"t1","name":"bash","arguments":{"command":"ls"},"type":"function"}]}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], AIEvent::AssistantText { .. }));
        assert!(matches!(evs[1], AIEvent::ToolUse { .. }));
    }

    #[test]
    fn parse_empty_assistant_message_becomes_other() {
        let line = r#"{"type":"assistant.message","data":{"messageId":"m1","content":"","toolRequests":[]}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        assert!(
            matches!(evs[0], AIEvent::Other(_)),
            "empty message should be Other; got {:?}",
            evs[0]
        );
    }

    #[test]
    fn parse_tool_complete_success_emits_tool_result() {
        let line = r#"{"type":"tool.execution_complete","data":{"toolCallId":"t1","success":true,"result":{"content":"ok\n"}}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
                ..
            } => {
                assert_eq!(tool_use_id, "t1");
                assert!(!is_error);
                assert_eq!(content.as_str(), Some("ok\n"));
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_complete_failure_sets_is_error() {
        let line = r#"{"type":"tool.execution_complete","data":{"toolCallId":"t1","success":false,"result":{"content":"boom"}}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolResult { is_error, .. } => assert!(is_error),
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_result_becomes_turn_result() {
        let line = r#"{"type":"result","sessionId":"81a24150-917f-47d5-bf60-469e64cb38d2","exitCode":0,"usage":{}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::TurnResult { .. }));
    }

    #[test]
    fn parse_unknown_type_becomes_other() {
        let line = r#"{"type":"session.tools_updated","data":{"model":"claude-haiku-4.5"}}"#;
        let evs = parse_copilot_line(line);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }
}
