//! Google Gemini CLI AI core.
//!
//! Drives the `gemini` binary the same way [`CopilotCliCore`](super::copilot_cli::CopilotCliCore)
//! drives `copilot`. Structure, prompt assembly, and tag-chain loop are
//! mirrored so swapping cores in `aciddog.wcl` changes behaviour
//! predictably. The main divergences versus copilot:
//!
//! - **Session id ownership**. Gemini generates its own session UUID
//!   and rejects seeded ones (`--resume <uuid>` only works for a UUID
//!   the binary already knows about). We cannot hand it the aciddog-
//!   side id. Instead the core captures gemini's UUID from its `init`
//!   stream event and uses that for `--resume` on every subsequent
//!   turn. Hibernated workers lose this in-memory value — on
//!   resurrection they spawn a fresh gemini session, matching the
//!   documented v1 limitation of the AnthropicApi core.
//! - **Tool restriction** goes through a per-turn policy-engine TOML
//!   file passed via `--policy`. The deprecated `--allowed-tools`
//!   doesn't actually restrict anything in recent gemini versions.
//! - **Assistant text is streamed as deltas**. Each `message` event
//!   with `role: assistant` and `delta: true` carries a partial
//!   fragment; we accumulate into a local buffer and flush a single
//!   `AssistantText` when the stream hits a different event type or
//!   EOF. Surfacing one event per delta would shatter a sentence
//!   across many TUI chat blocks.
//! - Two `SystemInit` events are emitted per turn: one synthesized
//!   using the aciddog-side session id (so the TUI can render per-
//!   agent chrome before gemini's own init arrives), then a real one
//!   carrying gemini's UUID once the stream produces it. Callers that
//!   only care about the first-init behaviour see the synthetic event;
//!   callers that care about the backing CLI's own session can read
//!   the second one.

use std::io::Write as _;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use serde_json::Value;
use tempfile::NamedTempFile;
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

const DEFAULT_BINARY: &str = "gemini";

/// Aciddog-side tool names that have a gemini built-in equivalent.
/// The config validator uses this for the `GeminiCli` variant so
/// unknown or unmappable names are rejected at parse time.
pub const GEMINI_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Write", "Edit", "Glob", "Grep"];

/// Gemini built-ins that never have an aciddog equivalent. They are
/// always denied by the per-turn policy file so the model cannot reach
/// for capabilities aciddog doesn't know how to audit. `list_directory`
/// overlaps with `Glob` so we route agents through `glob` instead; the
/// background-process tools have no aciddog counterpart at all.
const GEMINI_UNMAPPED_BUILTINS: &[&str] = &[
    "list_directory",
    "list_background_processes",
    "read_background_output",
];

/// Translate an aciddog tool name to gemini's native name. Returns
/// `None` when the name has no gemini equivalent.
pub fn translate_tool(name: &str) -> Option<&'static str> {
    match name {
        "Bash" => Some("run_shell_command"),
        "Read" => Some("read_file"),
        "Write" => Some("write_file"),
        "Edit" => Some("replace"),
        "Glob" => Some("glob"),
        "Grep" => Some("grep_search"),
        _ => None,
    }
}

/// One-line description used inside the first-turn `<tools>` preamble.
/// Reuses [`built_in_tool_doc`] so the English stays in one place; the
/// gemini-native name is mapped back to the aciddog key before
/// delegation.
fn gemini_tool_doc(gemini_name: &str) -> &'static str {
    let aciddog = match gemini_name {
        "run_shell_command" => "Bash",
        "read_file" => "Read",
        "write_file" => "Write",
        "replace" => "Edit",
        "glob" => "Glob",
        "grep_search" => "Grep",
        _ => return "enabled",
    };
    built_in_tool_doc(aciddog)
}

pub struct GeminiCliCore {
    binary: String,
    /// Aciddog-side session id — used for IPC routing, display names,
    /// and the synthesized `SystemInit` event emitted at the top of
    /// every turn. Gemini itself generates a different UUID per
    /// session; see `gemini_session_id`.
    aciddog_session_id: Uuid,
    /// Gemini's own session id, captured from its `init` stream event
    /// on the first turn of this core instance. Passed as
    /// `--resume <uuid>` on every subsequent turn so the model keeps
    /// its history. `None` on a fresh session (first turn omits
    /// `--resume`) and after resurrection (in-memory state is lost;
    /// same documented v1 limitation as the AnthropicApi core).
    ///
    /// Wrapped in `Arc<Mutex<_>>` because the stream-reading task
    /// spawned inside `turn()` needs to write here while the outer
    /// core keeps `&mut self` ownership.
    gemini_session_id: Arc<Mutex<Option<Uuid>>>,
    /// Structured first-turn prompt sources. Composed into the actual
    /// XML-tagged prompt by `build_first_turn_prompt` on the first turn.
    /// Ignored when `started` is true (resumed sessions).
    prompt_parts: PromptParts,
    /// Aciddog-side tool names (capitalized). Translated to gemini's
    /// native names at turn time for the policy file.
    allowed_tools: Vec<String>,
    /// When `Some`, passed as `--model <id>` on every turn. When `None`,
    /// gemini picks its own default.
    model: Option<String>,
    env: Vec<(String, String)>,
    /// Set to `Some` only when the agent lacks a shell tool and must
    /// coordinate through inline `<aciddog-call>` tags. When present,
    /// `turn` runs the tag-chain loop; otherwise it runs the child once
    /// and returns.
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    started: bool,
}

impl GeminiCliCore {
    /// Construct a core for a brand-new session. The caller owns the
    /// aciddog-side session id (used for IPC and the synthesized
    /// SystemInit). Gemini's own UUID is generated on the first spawn
    /// and captured from its `init` stream event.
    pub fn new(session_id: Uuid, prompt_parts: PromptParts, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            aciddog_session_id: session_id,
            gemini_session_id: Arc::new(Mutex::new(None)),
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
    /// Note: gemini history is **not** recovered — this core instance
    /// has no record of gemini's own UUID from the prior process, so
    /// the first turn after construction spawns a fresh gemini session
    /// under the covers. This matches the v1 limitation documented on
    /// the AnthropicApi core.
    pub fn with_session_id(session_id: Uuid, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            aciddog_session_id: session_id,
            gemini_session_id: Arc::new(Mutex::new(None)),
            prompt_parts: PromptParts::default(),
            allowed_tools,
            model: None,
            env: Vec::new(),
            socket_tx: None,
            started: true,
        }
    }

    /// Pin a specific gemini model for every turn (`--model <id>`).
    /// Accepts gemini's native model ids. An unknown id surfaces as an
    /// `AIEvent::Error` when gemini rejects it on spawn.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Override the binary path (useful for tests pointing at `/bin/false`, etc).
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Additional environment variables to pass to the spawned gemini process.
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
    /// and copilot cores so prompt-engineering insights transfer.
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
                // Show the gemini-native name so the model sees the
                // exact identifier it will use to invoke the tool.
                // Unknown names (shouldn't happen after validate_tools)
                // degrade gracefully.
                let display = translate_tool(name).unwrap_or(name.as_str());
                body.push_str(&format!("- `{display}` — {}\n", gemini_tool_doc(display)));
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
impl AICore for GeminiCliCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        self.validate_tools()?;

        // Build the per-turn policy tempfile. Lifetime parks on the
        // spawned task so the file stays alive for the whole chain in
        // tag mode (same pattern as claude_code's system-prompt file).
        let policy_file =
            build_policy_tempfile(&self.allowed_tools).map_err(AIError::SystemPromptIo)?;

        // Suppress GEMINI.md auto-loading (global, workspace, JIT) via
        // a system-settings JSON pointing at a sentinel context
        // filename. Using system settings keeps the user's
        // `~/.gemini/settings.json` (and its OAuth token) untouched.
        let settings_file = build_bare_settings_tempfile().map_err(AIError::SystemPromptIo)?;

        let is_first = !self.started;
        let first_prompt = self.build_prompt(prompt);

        let ctx = Arc::new(RoundCtx {
            binary: self.binary.clone(),
            aciddog_session_id: self.aciddog_session_id,
            model: self.model.clone(),
            env: self.env.clone(),
            policy_path: policy_file.as_ref().map(|t| t.path().to_path_buf()),
            system_settings_path: settings_file.path().to_path_buf(),
            gemini_session_id: self.gemini_session_id.clone(),
        });

        let (tx, rx) = mpsc::channel::<AIEvent>(64);
        let socket_tx = self.socket_tx.clone();
        let aciddog_session_id = self.aciddog_session_id;
        let announced_tools = self.allowed_tools.clone();

        // Synthesize the fleet-side SystemInit so every turn starts with
        // a known session id before gemini's own init arrives. A second
        // SystemInit is emitted from run_round once gemini reports its
        // real UUID — the TUI can treat either as authoritative.
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
            // Hold the tempfiles for the whole chain so every round can
            // read them.
            let _policy_guard = policy_file;
            let _settings_guard = settings_file;

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
    /// loop. Distinct from gemini's own per-session UUID.
    aciddog_session_id: Uuid,
    model: Option<String>,
    env: Vec<(String, String)>,
    /// Path to the per-turn policy TOML file passed via `--policy`.
    /// `None` means the caller chose not to emit one (today only
    /// happens when `allowed_tools` is empty *and* no unmapped
    /// built-ins need denying — practically never).
    policy_path: Option<std::path::PathBuf>,
    /// Path to a per-turn system-settings JSON, surfaced to gemini via
    /// `GEMINI_CLI_SYSTEM_SETTINGS_PATH`. The file sets
    /// `context.fileName` to a sentinel that never matches any real
    /// file, which suppresses GEMINI.md auto-discovery without touching
    /// the user's `~/.gemini/` (so OAuth stays intact).
    system_settings_path: std::path::PathBuf,
    /// Shared with the owning `GeminiCliCore`. `run_round` writes here
    /// when it sees gemini's `init` event; subsequent rounds read it to
    /// build `--resume <uuid>`.
    gemini_session_id: Arc<Mutex<Option<Uuid>>>,
}

/// Build a per-turn `NamedTempFile` containing policy rules that deny
/// every gemini built-in not in `allowed_tools`. Returns `Ok(None)`
/// only when there's nothing to deny (degenerate case: an agent with
/// every aciddog-mappable tool enabled and no unmapped built-ins to
/// block — shouldn't happen since `GEMINI_UNMAPPED_BUILTINS` is never
/// empty). Returns `Err` on I/O failure.
fn build_policy_tempfile(allowed: &[String]) -> std::io::Result<Option<NamedTempFile>> {
    let allowed_native: std::collections::HashSet<&'static str> =
        allowed.iter().filter_map(|n| translate_tool(n)).collect();

    let mut denies: Vec<&'static str> = Vec::new();
    for name in GEMINI_ALLOWED_TOOLS {
        let native = translate_tool(name).expect("GEMINI_ALLOWED_TOOLS must translate");
        if !allowed_native.contains(native) {
            denies.push(native);
        }
    }
    denies.extend(GEMINI_UNMAPPED_BUILTINS.iter().copied());

    if denies.is_empty() {
        return Ok(None);
    }

    let mut body = String::new();
    body.push_str("# Generated by aciddog per-turn — denies gemini built-ins the\n");
    body.push_str("# current agent is not permitted to use.\n\n");
    for name in denies {
        body.push_str("[[rule]]\n");
        body.push_str(&format!("toolName = \"{name}\"\n"));
        body.push_str("decision = \"deny\"\n");
        body.push_str("priority = 100\n\n");
    }

    // `.toml` suffix so gemini's policy loader accepts the file — the
    // docs require `.toml` files under the policy directory; passing a
    // bare tempfile path works the same way.
    let mut tmp = tempfile::Builder::new()
        .prefix("aciddog-gemini-policy-")
        .suffix(".toml")
        .tempfile()?;
    tmp.write_all(body.as_bytes())?;
    tmp.flush()?;
    Ok(Some(tmp))
}

/// Build a minimal gemini system-settings JSON that points
/// `context.fileName` at a sentinel name. Gemini iterates this name
/// for global, workspace, and JIT GEMINI.md discovery, so pinning it
/// to a value that never matches any real file disables every branch
/// in one shot. Routing through `GEMINI_CLI_SYSTEM_SETTINGS_PATH`
/// keeps the user's `~/.gemini/settings.json` (and its OAuth token)
/// untouched.
fn build_bare_settings_tempfile() -> std::io::Result<NamedTempFile> {
    let body = r#"{"context":{"fileName":"__aciddog_disabled_gemini_md__"}}"#;
    let mut tmp = tempfile::Builder::new()
        .prefix("aciddog-gemini-settings-")
        .suffix(".json")
        .tempfile()?;
    tmp.write_all(body.as_bytes())?;
    tmp.flush()?;
    Ok(tmp)
}

/// Spawn one gemini child, forward every parsed `AIEvent` from stdout
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
    // Snapshot gemini's own session id (from a previous round, if any).
    // First turn on a fresh core has `None` — omit `--resume` and let
    // gemini generate its UUID.
    let resume_id: Option<Uuid> = {
        let guard = ctx
            .gemini_session_id
            .lock()
            .expect("gemini_session_id poisoned");
        *guard
    };

    let mut cmd = Command::new(&ctx.binary);
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--yolo")
        .arg("--output-format")
        .arg("stream-json");

    if let Some(id) = resume_id {
        cmd.arg("--resume").arg(id.to_string());
    }

    if let Some(model) = &ctx.model {
        cmd.arg("--model").arg(model);
    }

    if let Some(path) = &ctx.policy_path {
        cmd.arg("--policy").arg(path);
    }

    // Point gemini at our bare system-settings file so GEMINI.md
    // auto-loading is suppressed for this spawn only.
    cmd.env("GEMINI_CLI_SYSTEM_SETTINGS_PATH", &ctx.system_settings_path);

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    debug!(
        binary = %ctx.binary,
        resume = ?resume_id,
        "spawning gemini",
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

    let stdout = child
        .stdout
        .take()
        .expect("stdout was piped, must be present");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was piped, must be present");

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf).await;
        if !buf.trim().is_empty() {
            warn!(stderr = %buf, "gemini stderr");
        }
        buf
    });

    // Delta accumulator: gemini streams assistant text as fragments.
    // We flush on any non-delta event or EOF.
    let mut pending_text = String::new();
    let mut pending_raw: Option<Value> = None;

    let mut reader = BufReader::new(stdout).lines();
    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                for ev in parse_gemini_line(
                    &line,
                    &mut pending_text,
                    &mut pending_raw,
                    &ctx.gemini_session_id,
                ) {
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

    // Flush any trailing assistant text that arrived without a
    // terminating non-delta event.
    if !pending_text.is_empty() {
        let raw = pending_raw.take().unwrap_or(Value::Null);
        let text = std::mem::take(&mut pending_text);
        if let Some(buf) = collect_text.as_ref() {
            let mut guard = buf.lock().expect("collect_text poisoned");
            guard.push_str(&text);
        }
        let _ = tx.send(AIEvent::AssistantText { text, raw }).await;
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
                "gemini exited with status {code}; stderr: {}",
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
                    message: format!("failed to await gemini child: {e}"),
                    raw: None,
                })
                .await;
        }
    }

    Ok(())
}

/// Run the tag-protocol chain: repeatedly spawn gemini, parse any
/// `<aciddog-call>` tags out of its assistant text, dispatch them
/// through the orchestrator socket channel, and feed the results back
/// as the next round's prompt. Caps the loop at `MAX_TAG_CHAIN_ROUNDS`
/// per user turn. Logic mirrors `claude_code::run_chain` /
/// `copilot_cli::run_chain`.
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
        // Gemini's own session UUID is only used for `--resume`.
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

/// Parse one line of gemini's `--output-format stream-json` output into
/// zero or more [`AIEvent`]s.
///
/// `pending_text` / `pending_raw` are the per-round delta accumulators
/// owned by `run_round`. They are drained into an `AssistantText` event
/// the first time we see a non-delta event (tool_use / tool_result /
/// result) or an assistant message without `delta: true`.
///
/// `gemini_session_id` is updated when the `init` event arrives so
/// subsequent rounds can pass `--resume <uuid>`.
///
/// Mapping (derived from an empirical trace against the real binary):
/// - `init` → `SystemInit { session_id: <from payload>, tools: [], model }`
///   *and* stores the session id in `gemini_session_id` for future
///   rounds. This is the **real** init, distinct from the synthesized
///   one `turn()` emits before the child has spawned.
/// - `message` with `role: assistant` and `delta: true` → accumulate
///   `content` into `pending_text` and stash the raw value. No event
///   emitted yet.
/// - `message` with `role: assistant` without `delta: true` → flush
///   accumulator (if any), then emit an `AssistantText` for this
///   message's content.
/// - `message` with `role: user` → echo of our own prompt; swallow as
///   `Other`.
/// - `tool_use` → flush accumulator, then `ToolUse { id: tool_id,
///   name: tool_name, input: parameters }`.
/// - `tool_result` → flush accumulator, then `ToolResult { tool_use_id:
///   tool_id, content: result, is_error: status != "success" }`.
/// - `result` → flush accumulator, then `TurnResult { stop_reason:
///   None, total_cost_usd: None }` (gemini reports token usage, not
///   USD).
/// - Everything else → `Other(raw)`.
pub(crate) fn parse_gemini_line(
    line: &str,
    pending_text: &mut String,
    pending_raw: &mut Option<Value>,
    gemini_session_id: &Arc<Mutex<Option<Uuid>>>,
) -> Vec<AIEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return vec![AIEvent::Error {
                message: format!("failed to parse gemini stream line: {e}"),
                raw: None,
            }];
        }
    };

    let ty = value.get("type").and_then(Value::as_str).unwrap_or("");

    match ty {
        "init" => {
            let mut events = Vec::new();
            // Any accumulated assistant text from a previous line
            // should flush before the init (shouldn't happen in
            // practice — init is always first — but stay robust).
            flush_pending(pending_text, pending_raw, &mut events);

            let session_id_str = value.get("session_id").and_then(Value::as_str);
            let session_id = session_id_str.and_then(|s| Uuid::parse_str(s).ok());
            if let Some(id) = session_id {
                let mut guard = gemini_session_id
                    .lock()
                    .expect("gemini_session_id poisoned");
                *guard = Some(id);
            }
            let tools = value
                .get("tools")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(id) = session_id {
                events.push(AIEvent::SystemInit {
                    session_id: id,
                    tools,
                    model,
                    raw: value,
                });
            } else {
                // Unparseable session id is a weird state; surface as
                // Other so the line is still visible in traces but
                // doesn't break the consumer's SystemInit contract.
                events.push(AIEvent::Other(value));
            }
            events
        }
        "message" => parse_message(value, pending_text, pending_raw),
        "tool_use" => {
            let mut events = Vec::new();
            flush_pending(pending_text, pending_raw, &mut events);
            let id = value
                .get("tool_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = value
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let input = value.get("parameters").cloned().unwrap_or(Value::Null);
            events.push(AIEvent::ToolUse {
                id,
                name,
                input,
                raw: value,
            });
            events
        }
        "tool_result" => {
            let mut events = Vec::new();
            flush_pending(pending_text, pending_raw, &mut events);
            let tool_use_id = value
                .get("tool_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let content = value
                .get("result")
                .cloned()
                .or_else(|| value.get("output").cloned())
                .unwrap_or(Value::Null);
            let status = value.get("status").and_then(Value::as_str).unwrap_or("");
            let is_error = status != "success" && !status.is_empty();
            events.push(AIEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
                raw: value,
            });
            events
        }
        "result" => {
            let mut events = Vec::new();
            flush_pending(pending_text, pending_raw, &mut events);
            // Gemini reports cumulative token usage in its result event;
            // best-effort extraction tolerates several known shapes.
            let usage = parse_gemini_usage(&value);
            events.push(AIEvent::TurnResult {
                stop_reason: None,
                total_cost_usd: None,
                usage,
                raw: value,
            });
            events
        }
        _ => vec![AIEvent::Other(value)],
    }
}

/// Best-effort token-usage extraction from a gemini `result` event.
/// Tolerates several shapes — gemini's CLI has used `stats.total_tokens`,
/// nested `usage`/`tokens` objects, and Anthropic-style flat keys across
/// versions. Returns `None` if no recognised numeric field is present.
fn parse_gemini_usage(value: &Value) -> Option<TurnUsage> {
    let mut u = TurnUsage::default();
    let mut any = false;

    // Probe a handful of plausible container locations.
    let containers: [Option<&Value>; 4] = [
        value.get("usage"),
        value.get("stats"),
        value.get("tokens"),
        value.pointer("/data/usage"),
    ];

    for cont in containers.iter().flatten() {
        let Some(obj) = cont.as_object() else { continue };
        for (k, v) in obj {
            let Some(n) = v.as_u64() else { continue };
            match k.as_str() {
                "prompt_tokens" | "input_tokens" | "tokens_in" | "input" => {
                    u.input_tokens = u.input_tokens.max(n);
                    any = true;
                }
                "completion_tokens" | "output_tokens" | "tokens_out" | "output" => {
                    u.output_tokens = u.output_tokens.max(n);
                    any = true;
                }
                "cache_read_input_tokens" | "cached_tokens" | "cached_input_tokens" => {
                    u.cache_read_input_tokens = u.cache_read_input_tokens.max(n);
                    any = true;
                }
                "cache_creation_input_tokens" => {
                    u.cache_creation_input_tokens = u.cache_creation_input_tokens.max(n);
                    any = true;
                }
                "total_tokens" | "total" => {
                    // Only fall back to `total` when we have no other
                    // signal; otherwise input+output is more accurate.
                    if u.input_tokens == 0 && u.output_tokens == 0 {
                        u.input_tokens = n;
                        any = true;
                    }
                }
                _ => {}
            }
        }
    }

    if any {
        Some(u)
    } else {
        None
    }
}

fn parse_message(
    raw: Value,
    pending_text: &mut String,
    pending_raw: &mut Option<Value>,
) -> Vec<AIEvent> {
    let role = raw.get("role").and_then(Value::as_str).unwrap_or("");
    match role {
        "assistant" => {
            let content = raw
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let is_delta = raw.get("delta").and_then(Value::as_bool).unwrap_or(false);
            if is_delta {
                pending_text.push_str(&content);
                // Last delta's raw value wins — good enough for
                // traceability. The accumulated text is what matters
                // downstream.
                *pending_raw = Some(raw);
                Vec::new()
            } else {
                let mut events = Vec::new();
                flush_pending(pending_text, pending_raw, &mut events);
                events.push(AIEvent::AssistantText { text: content, raw });
                events
            }
        }
        _ => vec![AIEvent::Other(raw)],
    }
}

/// Drain the per-round delta accumulator into a single `AssistantText`
/// event. No-op when the buffer is empty.
fn flush_pending(
    pending_text: &mut String,
    pending_raw: &mut Option<Value>,
    out: &mut Vec<AIEvent>,
) {
    if pending_text.is_empty() {
        return;
    }
    let text = std::mem::take(pending_text);
    let raw = pending_raw.take().unwrap_or(Value::Null);
    out.push(AIEvent::AssistantText { text, raw });
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

    fn fresh(instructions: &str, tools: Vec<String>) -> GeminiCliCore {
        GeminiCliCore::new(Uuid::new_v4(), parts(instructions), tools)
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

    #[test]
    fn translator_maps_every_allowed_name() {
        assert_eq!(translate_tool("Bash"), Some("run_shell_command"));
        assert_eq!(translate_tool("Read"), Some("read_file"));
        assert_eq!(translate_tool("Write"), Some("write_file"));
        assert_eq!(translate_tool("Edit"), Some("replace"));
        assert_eq!(translate_tool("Glob"), Some("glob"));
        assert_eq!(translate_tool("Grep"), Some("grep_search"));
        assert_eq!(translate_tool("WebFetch"), None);
        assert_eq!(translate_tool("Nonsense"), None);
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
        let core = GeminiCliCore::new(Uuid::new_v4(), parts, vec!["Read".into()]);
        let p = core.first_turn_prompt();
        let role_pos = p.find("<role>").expect("role");
        let team_pos = p.find("<team>").expect("team");
        let tools_pos = p.find("<tools>").expect("tools");
        let instr_pos = p.find("<instructions>").expect("instructions");
        assert!(role_pos < team_pos && team_pos < tools_pos && tools_pos < instr_pos);
        // Native gemini name appears in the preamble, not the aciddog
        // "Read" label.
        assert!(
            p.contains("- `read_file` —"),
            "missing native Read→read_file: {p}"
        );
        assert!(!p.contains("- `Read` —"), "aciddog name leaked: {p}");
    }

    #[test]
    fn role_block_omitted_when_role_is_none_or_blank() {
        let core = GeminiCliCore::new(Uuid::new_v4(), parts("i"), vec![]);
        let p = core.first_turn_prompt();
        assert!(!p.contains("<role>"), "no role block expected: {p}");

        let core = GeminiCliCore::new(
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
    fn tools_block_shows_gemini_native_names() {
        let core = GeminiCliCore::new(
            Uuid::new_v4(),
            parts("i"),
            vec!["Bash".into(), "Read".into(), "Write".into()],
        );
        let p = core.first_turn_prompt();
        assert!(p.contains("- `run_shell_command` —"), "missing bash: {p}");
        assert!(p.contains("- `read_file` —"), "missing read_file: {p}");
        assert!(p.contains("- `write_file` —"), "missing write_file: {p}");
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
        let core = GeminiCliCore::new(Uuid::new_v4(), parts, vec!["Bash".into(), "Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "missing tools open tag: {p}");
        assert!(
            p.contains("- `run_shell_command` —"),
            "missing bash builtin line: {p}"
        );
        assert!(
            p.contains("/usr/bin/aciddog send --to"),
            "missing CLI preamble: {p}"
        );
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
        let core = GeminiCliCore::new(Uuid::new_v4(), parts, vec!["Read".into()]);
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
        let core = GeminiCliCore::new(Uuid::new_v4(), parts, vec!["Bash".into()]);
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
        let core = GeminiCliCore::new(Uuid::new_v4(), parts("i"), vec!["Read".into()]);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "tools always emitted: {p}");
        assert!(!p.contains("aciddog"), "no aciddog docs expected: {p}");
    }

    // ------------------ constructors -------------------

    #[test]
    fn new_uses_caller_provided_session_id() {
        let id = Uuid::new_v4();
        let core = GeminiCliCore::new(id, parts("i"), vec![]);
        assert_eq!(core.session_id(), Some(id));
        assert!(!core.started, "new() sessions have not started yet");
    }

    #[test]
    fn with_session_id_starts_as_already_started() {
        let id = Uuid::new_v4();
        let mut core = GeminiCliCore::with_session_id(id, vec![]);
        assert_eq!(core.session_id(), Some(id));
        let p = core.build_prompt("hi");
        assert_eq!(p, "hi");
    }

    #[test]
    fn with_model_stores_model_id() {
        let core = fresh("i", vec![]).with_model("auto-gemini-3");
        assert_eq!(core.model.as_deref(), Some("auto-gemini-3"));
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
        let mut core = GeminiCliCore::new(id, parts("i"), vec!["Read".into()])
            .with_binary("/this/path/definitely/does/not/exist/gemini");
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
            fresh("i", vec![]).with_binary("/this/path/definitely/does/not/exist/gemini");
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

    // ------------------ policy-file generation -------------------

    #[test]
    fn policy_file_denies_unmapped_builtins_always() {
        let tmp = build_policy_tempfile(&[
            "Bash".into(),
            "Read".into(),
            "Write".into(),
            "Edit".into(),
            "Glob".into(),
            "Grep".into(),
        ])
        .expect("io ok")
        .expect("all-allowed still denies the unmapped builtins");
        let body = std::fs::read_to_string(tmp.path()).unwrap();
        for name in GEMINI_UNMAPPED_BUILTINS {
            assert!(body.contains(name), "policy missing {name}: {body}");
        }
        assert!(
            !body.contains("run_shell_command"),
            "allowed tool should not be denied: {body}"
        );
    }

    #[test]
    fn policy_file_denies_mapped_builtins_not_in_allowlist() {
        let tmp = build_policy_tempfile(&["Read".into()])
            .expect("io ok")
            .unwrap();
        let body = std::fs::read_to_string(tmp.path()).unwrap();
        assert!(
            body.contains("run_shell_command"),
            "bash should be denied: {body}"
        );
        assert!(
            body.contains("write_file"),
            "write should be denied: {body}"
        );
        assert!(body.contains("replace"), "edit should be denied: {body}");
        assert!(
            !body.contains("toolName = \"read_file\""),
            "read_file should not be denied: {body}"
        );
    }

    // ------------------ parse_gemini_line tests -------------------

    fn fresh_parser_state() -> (String, Option<Value>, Arc<Mutex<Option<Uuid>>>) {
        (String::new(), None, Arc::new(Mutex::new(None)))
    }

    #[test]
    fn parse_empty_and_whitespace_lines_are_noop() {
        let (mut text, mut raw, id) = fresh_parser_state();
        assert!(parse_gemini_line("", &mut text, &mut raw, &id).is_empty());
        assert!(parse_gemini_line("   \n", &mut text, &mut raw, &id).is_empty());
    }

    #[test]
    fn parse_malformed_json_becomes_error_event() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let evs = parse_gemini_line("{not json", &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Error { .. }));
    }

    #[test]
    fn parse_init_emits_systeminit_and_captures_session_id() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line = r#"{"type":"init","session_id":"cff2fdd6-12b1-451c-a670-b62c90170cb0","model":"auto-gemini-3"}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::SystemInit {
                session_id, model, ..
            } => {
                assert_eq!(
                    session_id.to_string(),
                    "cff2fdd6-12b1-451c-a670-b62c90170cb0"
                );
                assert_eq!(model.as_deref(), Some("auto-gemini-3"));
            }
            other => panic!("expected SystemInit, got {other:?}"),
        }
        // Captured into the shared slot so run_round can pass --resume.
        let stored = id.lock().unwrap();
        assert_eq!(
            stored.map(|u| u.to_string()).as_deref(),
            Some("cff2fdd6-12b1-451c-a670-b62c90170cb0")
        );
    }

    #[test]
    fn parse_init_with_bad_session_id_becomes_other_event() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line = r#"{"type":"init","session_id":"not-a-uuid"}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
        assert!(id.lock().unwrap().is_none(), "bad id should not be stored");
    }

    #[test]
    fn parse_assistant_delta_accumulates_without_emitting() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line = r#"{"type":"message","role":"assistant","content":"Hello ","delta":true}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert!(evs.is_empty(), "delta should not emit an event");
        assert_eq!(text, "Hello ");
        assert!(raw.is_some());
    }

    #[test]
    fn parse_multiple_deltas_concatenate() {
        let (mut text, mut raw, id) = fresh_parser_state();
        parse_gemini_line(
            r#"{"type":"message","role":"assistant","content":"Hel","delta":true}"#,
            &mut text,
            &mut raw,
            &id,
        );
        parse_gemini_line(
            r#"{"type":"message","role":"assistant","content":"lo","delta":true}"#,
            &mut text,
            &mut raw,
            &id,
        );
        assert_eq!(text, "Hello");
    }

    #[test]
    fn parse_non_delta_assistant_flushes_and_emits_standalone() {
        let (mut text, mut raw, id) = fresh_parser_state();
        parse_gemini_line(
            r#"{"type":"message","role":"assistant","content":"earlier ","delta":true}"#,
            &mut text,
            &mut raw,
            &id,
        );
        let line = r#"{"type":"message","role":"assistant","content":"final"}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 2);
        match &evs[0] {
            AIEvent::AssistantText { text, .. } => assert_eq!(text, "earlier "),
            other => panic!("expected flushed AssistantText first, got {other:?}"),
        }
        match &evs[1] {
            AIEvent::AssistantText { text, .. } => assert_eq!(text, "final"),
            other => panic!("expected standalone AssistantText, got {other:?}"),
        }
    }

    #[test]
    fn parse_user_message_becomes_other() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line = r#"{"type":"message","role":"user","content":"hi"}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn parse_tool_use_flushes_accumulator_and_emits_tool_use() {
        let (mut text, mut raw, id) = fresh_parser_state();
        parse_gemini_line(
            r#"{"type":"message","role":"assistant","content":"running it","delta":true}"#,
            &mut text,
            &mut raw,
            &id,
        );
        let line = r#"{"type":"tool_use","tool_name":"run_shell_command","tool_id":"t1","parameters":{"command":"ls"}}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], AIEvent::AssistantText { .. }));
        match &evs[1] {
            AIEvent::ToolUse {
                id, name, input, ..
            } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "run_shell_command");
                assert_eq!(input.get("command").and_then(Value::as_str), Some("ls"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
        assert!(text.is_empty(), "accumulator should have been drained");
    }

    #[test]
    fn parse_tool_result_success_emits_tool_result_without_error() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line =
            r#"{"type":"tool_result","tool_id":"t1","status":"success","result":{"output":"ok"}}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolResult {
                tool_use_id,
                is_error,
                ..
            } => {
                assert_eq!(tool_use_id, "t1");
                assert!(!is_error);
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_result_failure_sets_is_error() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line =
            r#"{"type":"tool_result","tool_id":"t1","status":"error","result":{"output":"boom"}}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        match &evs[0] {
            AIEvent::ToolResult { is_error, .. } => assert!(is_error),
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_result_flushes_and_emits_turn_result() {
        let (mut text, mut raw, id) = fresh_parser_state();
        parse_gemini_line(
            r#"{"type":"message","role":"assistant","content":"wrap","delta":true}"#,
            &mut text,
            &mut raw,
            &id,
        );
        let line = r#"{"type":"result","status":"success","stats":{"total_tokens":100}}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], AIEvent::AssistantText { .. }));
        assert!(matches!(evs[1], AIEvent::TurnResult { .. }));
    }

    #[test]
    fn parse_unknown_type_becomes_other() {
        let (mut text, mut raw, id) = fresh_parser_state();
        let line = r#"{"type":"mystery","data":1}"#;
        let evs = parse_gemini_line(line, &mut text, &mut raw, &id);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }
}
