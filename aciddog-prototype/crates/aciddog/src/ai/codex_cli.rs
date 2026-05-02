//! OpenAI Codex CLI AI core.
//!
//! Drives the `codex` binary (install with `npm i -g @openai/codex`) the
//! same way [`GeminiCliCore`](super::gemini_cli::GeminiCliCore) drives
//! `gemini`. Structure, prompt assembly, and tag-chain loop are mirrored
//! so swapping cores in `aciddog.wcl` changes behaviour predictably. The
//! main divergences versus gemini:
//!
//! - **No tool allowlist flag**. Codex always grants the model shell
//!   access (`/usr/bin/bash -lc <cmd>`); there is no equivalent of
//!   gemini's `--policy` to hide built-ins. We still validate
//!   `default_tools` against [`CODEX_ALLOWED_TOOLS`] for parity with
//!   copilot/gemini and to keep the first-turn `<tools>` preamble
//!   accurate, but the constraint is soft — codex itself decides what
//!   to call.
//! - **Spawn shape**. We always pass
//!   `--json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox`
//!   so the daemon never stalls on approval prompts and codex runs even
//!   outside a git tree. The prompt is piped on stdin (`-` placeholder)
//!   so multi-line preambles don't have to be argv-escaped.
//! - **Session id ownership**. Codex generates its own thread UUID and
//!   exposes it via the `thread.started` JSON event; we capture that
//!   into `codex_session_id` and use `codex exec resume <uuid>` on every
//!   subsequent turn. Hibernated workers lose this in-memory value — on
//!   resurrection they spawn a fresh codex thread, matching the
//!   documented v1 limitation of [`super::gemini_cli::GeminiCliCore`].
//! - **Output format**. Codex's `--json` is a flat JSONL stream of
//!   `thread.started` / `turn.started` / `item.started` / `item.completed`
//!   / `turn.completed` events; assistant text and shell output are
//!   delivered whole rather than as deltas, so no accumulator is needed.
//! - **No `--system-prompt` flag**. Like copilot/gemini, role and
//!   instructions go into the `<role>`/`<instructions>` blocks of the
//!   first-turn prompt; the WCL variant has no `systemPrompt` field.
//! - **Two `SystemInit` events per first turn**: one synthesized using
//!   the aciddog-side session id (so the TUI can render per-agent
//!   chrome before codex's own init arrives), then a real one carrying
//!   codex's UUID once the stream produces it.

use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
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

const DEFAULT_BINARY: &str = "codex";

/// Aciddog-side tool names the `:codexCli` core accepts in `default_tools`.
/// Codex itself does not enforce a tool allowlist — the model always has
/// shell access — but rejecting unknown names at parse time keeps the
/// WCL surface aligned with copilot/gemini and prevents the first-turn
/// `<tools>` preamble from advertising tools the agent will not actually
/// invoke. The names here are the aciddog capitalised set; codex calls
/// shell directly and does not need translation.
pub const CODEX_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Write", "Edit", "Glob", "Grep"];

pub struct CodexCliCore {
    binary: String,
    /// Aciddog-side session id — used for IPC routing, display names,
    /// and the synthesized `SystemInit` event emitted at the top of
    /// every turn. Codex itself generates a different UUID per session;
    /// see `codex_session_id`.
    aciddog_session_id: Uuid,
    /// Codex's own thread id, captured from its `thread.started` stream
    /// event on the first turn of this core instance. Passed to
    /// `codex exec resume <uuid>` on every subsequent turn so the model
    /// keeps its history. `None` on a fresh session (first turn omits
    /// `resume`) and after resurrection (in-memory state is lost; same
    /// documented v1 limitation as the AnthropicApi / GeminiCli cores).
    ///
    /// Wrapped in `Arc<Mutex<_>>` because the stream-reading task
    /// spawned inside `turn()` needs to write here while the outer
    /// core keeps `&mut self` ownership.
    codex_session_id: Arc<Mutex<Option<Uuid>>>,
    /// Structured first-turn prompt sources. Composed into the actual
    /// XML-tagged prompt by `build_first_turn_prompt` on the first turn.
    /// Ignored when `started` is true (resumed sessions).
    prompt_parts: PromptParts,
    /// Aciddog-side tool names (capitalized). Used only to render the
    /// `<tools>` preamble — codex's binary takes no allowlist flag.
    allowed_tools: Vec<String>,
    /// When `Some`, passed as `--model <id>` on every turn. When `None`,
    /// codex picks whatever its own config / login defaults to.
    model: Option<String>,
    env: Vec<(String, String)>,
    /// Set to `Some` only when the agent lacks a shell tool and must
    /// coordinate through inline `<aciddog-call>` tags. When present,
    /// `turn` runs the tag-chain loop; otherwise it runs the child once
    /// and returns. Note: codex always has shell access regardless, so
    /// the tag path is a soft steering signal rather than a hard
    /// sandbox.
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    started: bool,
}

impl CodexCliCore {
    /// Construct a core for a brand-new session. The caller owns the
    /// aciddog-side session id (used for IPC and the synthesized
    /// SystemInit). Codex's own UUID is generated on the first spawn
    /// and captured from its `thread.started` stream event.
    pub fn new(session_id: Uuid, prompt_parts: PromptParts, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            aciddog_session_id: session_id,
            codex_session_id: Arc::new(Mutex::new(None)),
            prompt_parts,
            allowed_tools,
            model: None,
            env: Vec::new(),
            socket_tx: None,
            started: false,
        }
    }

    /// Construct a core that resumes a pre-existing session marker. The
    /// first-turn prompt has already been delivered to the previous
    /// session, so no `PromptParts` are needed — `started` starts true
    /// and every turn forwards the user prompt verbatim.
    ///
    /// Note: codex history is **not** recovered — this core instance has
    /// no record of codex's own UUID from the prior process, so the
    /// first turn after construction spawns a fresh codex thread under
    /// the covers. Same v1 limitation as AnthropicApi / GeminiCli.
    pub fn with_session_id(session_id: Uuid, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            aciddog_session_id: session_id,
            codex_session_id: Arc::new(Mutex::new(None)),
            prompt_parts: PromptParts::default(),
            allowed_tools,
            model: None,
            env: Vec::new(),
            socket_tx: None,
            started: true,
        }
    }

    /// Pin a specific codex model for every turn (`--model <id>`).
    /// Accepts codex's native model ids (e.g. `gpt-5.1-codex`,
    /// `gpt-5.4`, `o4-mini`). An unknown id surfaces as an
    /// `AIEvent::Error` when codex rejects it on spawn.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Override the binary path (useful for tests pointing at `/bin/false`, etc).
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Additional environment variables to pass to the spawned codex process.
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
        let allowed: std::collections::HashSet<&str> =
            CODEX_ALLOWED_TOOLS.iter().copied().collect();
        for tool in &self.allowed_tools {
            if tool.is_empty() || tool.contains(',') || tool.chars().any(char::is_whitespace) {
                return Err(AIError::InvalidToolName(tool.clone()));
            }
            if !allowed.contains(tool.as_str()) {
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
    /// / copilot / gemini cores so prompt-engineering insights transfer.
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
                // Codex calls tools by their natural English names
                // (shell, file ops) so we render the aciddog-side
                // capitalised name directly. Unknown names (shouldn't
                // happen after validate_tools) degrade gracefully to
                // "enabled".
                body.push_str(&format!("- `{name}` — {}\n", built_in_tool_doc(name)));
            }
        }

        if let Some(cli) = &self.prompt_parts.aciddog_cli {
            let has_bash = self.allowed_tools.iter().any(|t| t == "Bash");
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
}

#[async_trait]
impl AICore for CodexCliCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        self.validate_tools()?;

        let is_first = !self.started;
        let first_prompt = self.build_prompt(prompt);

        let ctx = Arc::new(RoundCtx {
            binary: self.binary.clone(),
            aciddog_session_id: self.aciddog_session_id,
            model: self.model.clone(),
            env: self.env.clone(),
            codex_session_id: self.codex_session_id.clone(),
        });

        let (tx, rx) = mpsc::channel::<AIEvent>(64);
        let socket_tx = self.socket_tx.clone();
        let aciddog_session_id = self.aciddog_session_id;
        let announced_tools = self.allowed_tools.clone();

        // Synthesize the fleet-side SystemInit so every turn starts with
        // a known session id before codex's own thread.started arrives.
        // A second SystemInit is emitted from run_round once codex
        // reports its real UUID — the TUI can treat either as
        // authoritative.
        let synth_init = AIEvent::SystemInit {
            session_id: aciddog_session_id,
            tools: announced_tools,
            model: None,
            raw: Value::Null,
        };
        if tx.send(synth_init).await.is_err() {
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
        Some(self.aciddog_session_id)
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
/// outlives the `turn()` call frame.
struct RoundCtx {
    binary: String,
    /// Aciddog-side id — used for IPC routing inside the tag-chain
    /// loop. Distinct from codex's own per-thread UUID.
    aciddog_session_id: Uuid,
    model: Option<String>,
    env: Vec<(String, String)>,
    /// Shared with the owning `CodexCliCore`. `run_round` writes here
    /// when it sees codex's `thread.started` event; subsequent rounds
    /// read it to build `exec resume <uuid>`.
    codex_session_id: Arc<Mutex<Option<Uuid>>>,
}

/// Spawn one codex child, forward every parsed `AIEvent` from stdout
/// to `tx`, await the child's exit. When `collect_text` is `Some`,
/// every `AssistantText` event's text is also appended to the shared
/// buffer so the caller can post-process for tag calls.
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
    // Snapshot codex's own thread id (from a previous round, if any).
    // First turn on a fresh core has `None` — invoke `codex exec` so
    // codex generates its UUID; subsequent rounds use `codex exec
    // resume <uuid>` to keep the conversation history.
    let resume_id: Option<Uuid> = {
        let guard = ctx
            .codex_session_id
            .lock()
            .expect("codex_session_id poisoned");
        *guard
    };

    let mut cmd = Command::new(&ctx.binary);
    cmd.arg("exec");
    if let Some(id) = resume_id {
        cmd.arg("resume").arg(id.to_string());
    }
    cmd.arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox");

    if let Some(model) = &ctx.model {
        cmd.arg("--model").arg(model);
    }

    // `-` tells codex to read the prompt from stdin so multi-line
    // preambles don't have to be argv-escaped.
    cmd.arg("-");

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    debug!(
        binary = %ctx.binary,
        resume = ?resume_id,
        "spawning codex",
    );

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

    // Pipe the prompt on stdin and close the handle so codex sees EOF.
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
            let _ = tx
                .send(AIEvent::Error {
                    message: format!("failed to write prompt to codex stdin: {e}"),
                    raw: None,
                })
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Ok(());
        }
        // Dropping stdin closes it; explicit shutdown surfaces I/O
        // errors that drop would silently swallow.
        let _ = stdin.shutdown().await;
        drop(stdin);
    }

    let stdout = child
        .stdout
        .take()
        .expect("stdout was piped, must be present");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was piped, must be present");

    // Codex writes noisy `tracing` lines to stderr (e.g. shell-snapshot
    // validation warnings, `failed to record rollout items` after
    // turn.completed). Drain in parallel and warn on non-empty rather
    // than treating any stderr output as fatal.
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf).await;
        if !buf.trim().is_empty() {
            warn!(stderr = %buf, "codex stderr");
        }
        buf
    });

    let mut reader = BufReader::new(stdout).lines();
    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                for ev in parse_codex_line(&line, &ctx.codex_session_id) {
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
                "codex exited with status {code}; stderr: {}",
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
                    message: format!("failed to await codex child: {e}"),
                    raw: None,
                })
                .await;
        }
    }

    Ok(())
}

/// Run the tag-protocol chain: repeatedly spawn codex, parse any
/// `<aciddog-call>` tags out of its assistant text, dispatch them
/// through the orchestrator socket channel, and feed the results back
/// as the next round's prompt. Caps the loop at `MAX_TAG_CHAIN_ROUNDS`
/// per user turn. Logic mirrors `claude_code::run_chain` /
/// `gemini_cli::run_chain`.
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
        // Tag-chain dispatch is routed by the aciddog-side id because
        // the orchestrator tracks agents by that id (ENV_AGENT_ID).
        // Codex's own thread UUID is only used for `exec resume`.
        let agent_id = ctx.aciddog_session_id;
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
                Ok(cmd) => dispatch_call(cmd.clone(), agent_id, &socket_tx).await,
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

/// Parse one line of codex's `--json` output into zero or more
/// [`AIEvent`]s.
///
/// `codex_session_id` is updated when the `thread.started` event
/// arrives so subsequent rounds can pass `exec resume <uuid>`.
///
/// Mapping (derived from an empirical trace against the real binary):
/// - `thread.started` → captures `thread_id` into `codex_session_id`
///   and emits a `SystemInit { session_id: thread_id, .. }`. This is
///   the **real** init, distinct from the synthesized one `turn()`
///   emits before the child has spawned.
/// - `turn.started` → `Other(raw)`.
/// - `item.started` with `item.type == "command_execution"` →
///   `ToolUse { id, name: "Bash", input: {"command": ..}, raw }`.
/// - `item.completed` with `item.type == "command_execution"` →
///   `ToolResult { tool_use_id: id, content: aggregated_output,
///   is_error: exit_code != 0, raw }`.
/// - `item.completed` with `item.type == "agent_message"` →
///   `AssistantText { text, raw }`.
/// - Any other `item.started` / `item.completed` (`reasoning`,
///   `file_edit`, `web_search`, etc.) → `Other(raw)`. New mappings can
///   be added if/when those item types start appearing in agent runs.
/// - `turn.completed` → `TurnResult { stop_reason: None,
///   total_cost_usd: None, usage: <parsed>, raw }`.
/// - Everything else → `Other(raw)`.
pub(crate) fn parse_codex_line(
    line: &str,
    codex_session_id: &Arc<Mutex<Option<Uuid>>>,
) -> Vec<AIEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return vec![AIEvent::Error {
                message: format!("failed to parse codex stream line: {e}"),
                raw: None,
            }];
        }
    };

    let ty = value.get("type").and_then(Value::as_str).unwrap_or("");

    match ty {
        "thread.started" => {
            let thread_id_str = value.get("thread_id").and_then(Value::as_str);
            let thread_id = thread_id_str.and_then(|s| Uuid::parse_str(s).ok());
            if let Some(id) = thread_id {
                let mut guard = codex_session_id.lock().expect("codex_session_id poisoned");
                *guard = Some(id);
                vec![AIEvent::SystemInit {
                    session_id: id,
                    tools: Vec::new(),
                    model: None,
                    raw: value,
                }]
            } else {
                // Unparseable thread id is a weird state; surface as
                // Other so the line is still visible in traces but
                // doesn't break the consumer's SystemInit contract.
                vec![AIEvent::Other(value)]
            }
        }
        "item.started" => parse_item(value, /* completed = */ false),
        "item.completed" => parse_item(value, /* completed = */ true),
        "turn.completed" => {
            let usage = parse_codex_usage(value.get("usage"));
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

/// Best-effort token-usage extraction from a codex `turn.completed`
/// event. Codex reports per-turn deltas with `input_tokens`,
/// `cached_input_tokens`, `output_tokens`, and `reasoning_output_tokens`;
/// reasoning tokens are folded into output for accounting since aciddog
/// has no separate slot. Returns `None` when no recognised numeric
/// field is present.
fn parse_codex_usage(usage: Option<&Value>) -> Option<TurnUsage> {
    let obj = usage?.as_object()?;
    let mut u = TurnUsage::default();
    let mut any = false;
    for (k, v) in obj {
        let Some(n) = v.as_u64() else { continue };
        match k.as_str() {
            "input_tokens" => {
                u.input_tokens = n;
                any = true;
            }
            "output_tokens" => {
                u.output_tokens = u.output_tokens.saturating_add(n);
                any = true;
            }
            "reasoning_output_tokens" => {
                u.output_tokens = u.output_tokens.saturating_add(n);
                any = true;
            }
            "cached_input_tokens" => {
                u.cache_read_input_tokens = n;
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

/// Map a codex `item.started` / `item.completed` event into a
/// `ToolUse` / `ToolResult` / `AssistantText`, or `Other` for item
/// types we don't model yet.
fn parse_item(raw: Value, completed: bool) -> Vec<AIEvent> {
    let item = match raw.get("item") {
        Some(v) => v,
        None => return vec![AIEvent::Other(raw)],
    };
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    match (item_type, completed) {
        ("command_execution", false) => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let input = serde_json::json!({ "command": command });
            vec![AIEvent::ToolUse {
                id,
                name: "Bash".to_string(),
                input,
                raw,
            }]
        }
        ("command_execution", true) => {
            let content = item
                .get("aggregated_output")
                .cloned()
                .unwrap_or(Value::Null);
            let exit_code = item.get("exit_code").and_then(Value::as_i64);
            let is_error = exit_code.map(|c| c != 0).unwrap_or(false);
            vec![AIEvent::ToolResult {
                tool_use_id: id,
                content,
                is_error,
                raw,
            }]
        }
        ("agent_message", true) => {
            let text = item
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                vec![AIEvent::Other(raw)]
            } else {
                vec![AIEvent::AssistantText { text, raw }]
            }
        }
        // `agent_message` only carries final text on completion; any
        // started event for it is bookkeeping. Other item types
        // (`reasoning`, `file_edit`, `web_search`, ...) aren't modelled
        // yet.
        _ => vec![AIEvent::Other(raw)],
    }
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

    fn fresh(instructions: &str, tools: Vec<String>) -> CodexCliCore {
        CodexCliCore::new(Uuid::new_v4(), parts(instructions), tools)
    }

    // ------------------ validate_tools -------------------

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

    // ------------------ first-turn prompt assembly -------------------

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
            instructions: "do the thing".into(),
            aciddog_cli: None,
        };
        let core = CodexCliCore::new(Uuid::new_v4(), parts, vec!["Read".into()]);
        let p = core.first_turn_prompt();
        let role_pos = p.find("<role>").expect("role");
        let team_pos = p.find("<team>").expect("team");
        let tools_pos = p.find("<tools>").expect("tools");
        let instr_pos = p.find("<instructions>").expect("instructions");
        assert!(role_pos < team_pos && team_pos < tools_pos && tools_pos < instr_pos);
        // Codex preamble uses the aciddog-side capitalised names
        // directly — there is no native renaming step.
        assert!(p.contains("- `Read` —"), "missing Read line: {p}");
    }

    #[test]
    fn role_block_omitted_when_role_is_none_or_blank() {
        let core = CodexCliCore::new(Uuid::new_v4(), parts("i"), vec![]);
        let p = core.first_turn_prompt();
        assert!(!p.contains("<role>"), "no role block expected: {p}");

        let core = CodexCliCore::new(
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
    fn tools_block_uses_cli_preamble_when_bash_present() {
        let parts = PromptParts {
            instructions: "i".into(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: "/usr/bin/aciddog".into(),
                contactable: vec!["bob".into()],
                creatable: vec![],
                memory_banks: vec![],
                skills: vec![],
                channels: vec![],
                supervisor: false,
                sysadmin: false,
                worker: None,
            }),
            ..PromptParts::default()
        };
        let core = CodexCliCore::new(Uuid::new_v4(), parts, vec!["Bash".into(), "Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "missing tools open tag: {p}");
        assert!(p.contains("- `Bash` —"), "missing Bash builtin line: {p}");
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
                skills: vec![],
                channels: vec![],
                supervisor: false,
                sysadmin: false,
                worker: None,
            }),
            ..PromptParts::default()
        };
        let core = CodexCliCore::new(Uuid::new_v4(), parts, vec!["Read".into()]);
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
                skills: vec![],
                channels: vec![],
                supervisor: false,
                sysadmin: false,
                worker: Some(WorkerCtx {
                    parent_id: "coder".into(),
                    task: "find FOO".into(),
                }),
            }),
            ..PromptParts::default()
        };
        let core = CodexCliCore::new(Uuid::new_v4(), parts, vec!["Bash".into()]);
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
        let core = CodexCliCore::new(Uuid::new_v4(), parts("i"), vec!["Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "tools always emitted: {p}");
        assert!(!p.contains("aciddog"), "no aciddog docs expected: {p}");
    }

    // ------------------ constructors -------------------

    #[test]
    fn new_uses_caller_provided_session_id() {
        let id = Uuid::new_v4();
        let core = CodexCliCore::new(id, parts("i"), vec![]);
        assert_eq!(core.session_id(), Some(id));
        assert!(!core.started, "new() sessions have not started yet");
    }

    #[test]
    fn with_session_id_starts_as_already_started() {
        let id = Uuid::new_v4();
        let mut core = CodexCliCore::with_session_id(id, vec![]);
        assert_eq!(core.session_id(), Some(id));
        let p = core.build_prompt("hi");
        assert_eq!(p, "hi");
    }

    #[test]
    fn with_model_stores_model_id() {
        let core = fresh("i", vec![]).with_model("gpt-5.1-codex");
        assert_eq!(core.model.as_deref(), Some("gpt-5.1-codex"));
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

    // ------------------ turn() + stream integration -------------------

    #[tokio::test]
    async fn system_init_is_synthesized_as_first_event() {
        let id = Uuid::new_v4();
        let mut core = CodexCliCore::new(id, parts("i"), vec!["Read".into()])
            .with_binary("/this/path/definitely/does/not/exist/codex");
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
        let mut core = fresh("i", vec![]).with_binary("/this/path/definitely/does/not/exist/codex");
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

    // ------------------ parse_codex_line tests -------------------

    fn fresh_session_slot() -> Arc<Mutex<Option<Uuid>>> {
        Arc::new(Mutex::new(None))
    }

    #[test]
    fn parse_empty_and_whitespace_lines_are_noop() {
        let id = fresh_session_slot();
        assert!(parse_codex_line("", &id).is_empty());
        assert!(parse_codex_line("   \n", &id).is_empty());
    }

    #[test]
    fn parse_malformed_json_becomes_error_event() {
        let id = fresh_session_slot();
        let evs = parse_codex_line("{not json", &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Error { .. }));
    }

    #[test]
    fn parse_thread_started_emits_systeminit_and_captures_session_id() {
        let id = fresh_session_slot();
        let line =
            r#"{"type":"thread.started","thread_id":"019de30c-c108-7b02-b492-63871c756f0a"}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::SystemInit { session_id, .. } => {
                assert_eq!(
                    session_id.to_string(),
                    "019de30c-c108-7b02-b492-63871c756f0a"
                );
            }
            other => panic!("expected SystemInit, got {other:?}"),
        }
        let stored = id.lock().unwrap();
        assert_eq!(
            stored.map(|u| u.to_string()).as_deref(),
            Some("019de30c-c108-7b02-b492-63871c756f0a")
        );
    }

    #[test]
    fn parse_thread_started_with_bad_id_becomes_other_event() {
        let id = fresh_session_slot();
        let line = r#"{"type":"thread.started","thread_id":"not-a-uuid"}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
        assert!(id.lock().unwrap().is_none(), "bad id should not be stored");
    }

    #[test]
    fn parse_turn_started_becomes_other() {
        let id = fresh_session_slot();
        let evs = parse_codex_line(r#"{"type":"turn.started"}"#, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn parse_command_execution_started_emits_tool_use() {
        let id = fresh_session_slot();
        let line = r#"{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"/usr/bin/bash -lc ls","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolUse {
                id, name, input, ..
            } => {
                assert_eq!(id, "item_0");
                assert_eq!(name, "Bash");
                assert_eq!(
                    input.get("command").and_then(Value::as_str),
                    Some("/usr/bin/bash -lc ls")
                );
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_command_execution_completed_success_emits_tool_result() {
        let id = fresh_session_slot();
        let line = r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"ls","aggregated_output":"a\nb\n","exit_code":0,"status":"completed"}}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
                ..
            } => {
                assert_eq!(tool_use_id, "item_0");
                assert!(!is_error);
                assert_eq!(content.as_str(), Some("a\nb\n"));
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_command_execution_completed_failure_sets_is_error() {
        let id = fresh_session_slot();
        let line = r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","aggregated_output":"boom","exit_code":1,"status":"completed"}}"#;
        let evs = parse_codex_line(line, &id);
        match &evs[0] {
            AIEvent::ToolResult { is_error, .. } => assert!(is_error),
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_agent_message_completed_emits_assistant_text() {
        let id = fresh_session_slot();
        let line = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Done!"}}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::AssistantText { text, .. } => assert_eq!(text, "Done!"),
            other => panic!("expected AssistantText, got {other:?}"),
        }
    }

    #[test]
    fn parse_empty_agent_message_becomes_other() {
        let id = fresh_session_slot();
        let line =
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":""}}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn parse_unknown_item_type_becomes_other() {
        let id = fresh_session_slot();
        let line = r#"{"type":"item.completed","item":{"id":"item_2","type":"reasoning","text":"thinking..."}}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn parse_turn_completed_emits_turn_result_with_usage() {
        let id = fresh_session_slot();
        let line = r#"{"type":"turn.completed","usage":{"input_tokens":13259,"cached_input_tokens":11648,"output_tokens":11,"reasoning_output_tokens":0}}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::TurnResult { usage, .. } => {
                let u = usage.as_ref().expect("usage parsed");
                assert_eq!(u.input_tokens, 13259);
                assert_eq!(u.output_tokens, 11);
                assert_eq!(u.cache_read_input_tokens, 11648);
            }
            other => panic!("expected TurnResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_turn_completed_folds_reasoning_into_output_tokens() {
        let id = fresh_session_slot();
        let line = r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":40,"reasoning_output_tokens":60}}"#;
        let evs = parse_codex_line(line, &id);
        match &evs[0] {
            AIEvent::TurnResult { usage, .. } => {
                let u = usage.as_ref().expect("usage parsed");
                assert_eq!(u.output_tokens, 100, "reasoning should be folded in");
            }
            other => panic!("expected TurnResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_type_becomes_other() {
        let id = fresh_session_slot();
        let line = r#"{"type":"mystery","data":1}"#;
        let evs = parse_codex_line(line, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }
}
