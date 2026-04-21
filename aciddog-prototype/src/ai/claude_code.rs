use std::io::Write as _;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use serde_json::Value;
use tempfile::NamedTempFile;
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

use super::core::{AICore, AIError};
use super::event::{parse_line, AIEvent};
use super::tag_call;
use super::tools::docs::built_in_tool_doc;

/// Structured first-turn prompt input for [`ClaudeCodeCore`]. The core
/// owns the assembly so it can build the `<tools>` section from its own
/// knowledge of which built-in tools are enabled and which aciddog-CLI
/// surface (CLI vs tag protocol) applies.
#[derive(Debug, Clone, Default)]
pub struct PromptParts {
    /// Long-form behavioural guidance. Emitted as `<role>...</role>`.
    /// `None` or whitespace-only → block omitted.
    pub role: Option<String>,
    /// Per-actor team listing (description + contactable peers +
    /// creatable workers). Emitted as `<team>...</team>`. `None` or
    /// whitespace-only → block omitted.
    pub team: Option<String>,
    /// Per-actor memory-bank listing (name + description). Emitted as
    /// `<memory>...</memory>`. `None` or whitespace-only → block
    /// omitted. This section is purely informational; the operational
    /// `memory search` / `memory store` docs go inside `<tools>`.
    pub memory: Option<String>,
    /// Per-actor skills block. Lists the skills pre-announced to this
    /// actor (name + short description) plus a discovery hint pointing
    /// at `.aciddog/skills/`. Emitted as `<skills>...</skills>`. `None`
    /// or whitespace-only → block omitted.
    pub skills: Option<String>,
    /// The WCL `instructions` string. Always emitted inside
    /// `<instructions>...</instructions>`.
    pub instructions: String,
    /// When `Some`, the `<tools>` section appends the aciddog CLI
    /// (or tag-protocol) docs after the built-in tool list.
    pub aciddog_cli: Option<AciddogCliCtx>,
}

/// Context the core needs to render the aciddog-CLI/tag section inside
/// `<tools>`. The core picks CLI vs tag based on whether `Bash` is in
/// `allowed_tools`.
#[derive(Debug, Clone)]
pub struct AciddogCliCtx {
    pub binary: String,
    pub contactable: Vec<String>,
    pub creatable: Vec<String>,
    /// Bank ids the actor may search/store against. Rendered as the
    /// `memory search` / `memory store` doc in the preamble. Empty ⇒
    /// those subcommands are shown as disabled.
    pub memory_banks: Vec<String>,
    /// `Some` for workers — adds the parent + task + `worker complete`
    /// preamble before the general CLI/tag commands.
    pub worker: Option<WorkerCtx>,
}

#[derive(Debug, Clone)]
pub struct WorkerCtx {
    pub parent_id: String,
    pub task: String,
}

const DEFAULT_BINARY: &str = "claude";

/// Maximum consecutive tag-protocol rounds the core will self-chain
/// inside a single user turn before emitting an error and stopping.
pub const MAX_TAG_CHAIN_ROUNDS: usize = 8;

pub struct ClaudeCodeCore {
    binary: String,
    session_id: Uuid,
    /// Structured first-turn prompt sources. Composed into the actual
    /// XML-tagged prompt by `build_first_turn_prompt` on the first turn.
    /// Ignored when `started` is true (resumed sessions).
    prompt_parts: PromptParts,
    allowed_tools: Vec<String>,
    /// Content passed via `--system-prompt-file` on every turn. When
    /// `None`, no system prompt file is created and claude uses its
    /// default system prompt.
    system_prompt: Option<String>,
    env: Vec<(String, String)>,
    /// Set to `Some` only when the agent lacks a shell tool and must
    /// coordinate through inline `<aciddog-call>` tags. When present,
    /// `turn` runs the tag-chain loop; otherwise it runs the child once
    /// and returns, matching the legacy behavior.
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    started: bool,
}

impl ClaudeCodeCore {
    /// Construct a core for a brand-new session. The caller owns the
    /// session id so it can be shared with the chat UI and the agent IPC
    /// layer before the first turn has run.
    pub fn new(
        session_id: Uuid,
        prompt_parts: PromptParts,
        allowed_tools: Vec<String>,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            session_id,
            prompt_parts,
            allowed_tools,
            system_prompt,
            env: Vec::new(),
            socket_tx: None,
            started: false,
        }
    }

    /// Construct a core that resumes a pre-existing session by id. The
    /// first-turn prompt has already been delivered to the previous
    /// session, so no `PromptParts` are needed — `started` starts true
    /// and every turn forwards the user prompt verbatim.
    pub fn with_session_id(
        session_id: Uuid,
        allowed_tools: Vec<String>,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            session_id,
            prompt_parts: PromptParts::default(),
            allowed_tools,
            system_prompt,
            env: Vec::new(),
            socket_tx: None,
            started: true,
        }
    }

    /// Override the binary path (useful for tests pointing at `/bin/false`, etc).
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    /// Additional environment variables to pass to the spawned claude process.
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
    /// `<memory>`, `<skills>`, `<tools>`, `<instructions>`. `<role>`,
    /// `<team>`, `<memory>`, and `<skills>` are skipped when their
    /// `PromptParts` field is `None` or whitespace-only; `<tools>` and
    /// `<instructions>` are always emitted (even when empty) so the
    /// model always sees its tool inventory and brief.
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
        if let Some(skills) = self
            .prompt_parts
            .skills
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            sections.push(format!("<skills>\n{skills}\n</skills>"));
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
                body.push_str(&format!("- `{name}` — {}\n", built_in_tool_doc(name)));
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
impl AICore for ClaudeCodeCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        self.validate_tools()?;

        // Create the system-prompt tempfile once per user turn. In tag
        // mode it is reused across every chain round.
        let system_prompt_file = if let Some(content) = &self.system_prompt {
            let mut tmp = NamedTempFile::new().map_err(AIError::SystemPromptIo)?;
            tmp.write_all(content.as_bytes())
                .map_err(AIError::SystemPromptIo)?;
            tmp.flush().map_err(AIError::SystemPromptIo)?;
            Some(tmp)
        } else {
            None
        };

        // First-turn flag: determines --session-id vs --resume on the
        // first child spawn of this user turn.
        let is_first = !self.started;
        let first_prompt = self.build_prompt(prompt);

        let ctx = Arc::new(RoundCtx {
            binary: self.binary.clone(),
            session_id: self.session_id,
            allowed_tools: self.allowed_tools.clone(),
            env: self.env.clone(),
            system_prompt_path: system_prompt_file.as_ref().map(|t| t.path().to_path_buf()),
        });

        let (tx, rx) = mpsc::channel::<AIEvent>(64);
        let socket_tx = self.socket_tx.clone();

        tokio::spawn(async move {
            // Hold the tempfile for the whole chain so every round can
            // read it. Dropped here, AFTER all rounds complete.
            let _tempfile_guard = system_prompt_file;

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
/// sidesteps the lifetime mismatch between `&mut self` and a task that
/// outlives the `turn()` call frame.
struct RoundCtx {
    binary: String,
    session_id: Uuid,
    allowed_tools: Vec<String>,
    env: Vec<(String, String)>,
    system_prompt_path: Option<std::path::PathBuf>,
}

/// Spawn one claude child, pipe the prompt in on stdin, forward every
/// parsed `AIEvent` from stdout to `tx`, await the child's exit. When
/// `collect_text` is `Some`, every `AssistantText` event's text is also
/// appended to the shared buffer so the caller can post-process for
/// tag calls.
///
/// Returns `Err` only if the `tx` channel closes early (consumer gone).
/// Child-spawn / stderr / non-zero-exit errors surface as
/// `AIEvent::Error` through `tx` and the function returns `Ok(())`.
async fn run_round(
    ctx: &RoundCtx,
    is_first: bool,
    prompt: String,
    tx: mpsc::Sender<AIEvent>,
    collect_text: Option<Arc<Mutex<String>>>,
) -> Result<(), ()> {
    let tools_arg = ctx.allowed_tools.join(",");
    let session_id = ctx.session_id.to_string();

    let mut cmd = Command::new(&ctx.binary);
    cmd.arg("-p")
        .arg("--dangerously-skip-permissions")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        // Block online MCP servers: empty inline config + strict mode
        // means only this (empty) MCP config is honored.
        .arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg(r#"{"mcpServers":{}}"#)
        // Skip CLAUDE.md auto-discovery and the auto-memory system so
        // spawned agents only see the prompt aciddog constructs.
        // Env-var form (not --bare) preserves OAuth/keychain auth.
        .env("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1")
        .env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1");

    if let Some(path) = &ctx.system_prompt_path {
        cmd.arg("--system-prompt-file").arg(path);
    }

    if is_first {
        cmd.arg("--session-id").arg(&session_id);
    } else {
        cmd.arg("--resume").arg(&session_id);
    }

    if !tools_arg.is_empty() {
        cmd.arg("--tools").arg(&tools_arg);
    }

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    debug!(binary = %ctx.binary, session_id = %session_id, "spawning claude");

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

    // Write the prompt and close stdin so claude sees EOF and starts
    // working.
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
            warn!(error = %e, "failed to write prompt to claude stdin");
        }
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

    // Stderr is drained in parallel; we await it at the end to include
    // its contents in any failure report.
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf).await;
        if !buf.trim().is_empty() {
            warn!(stderr = %buf, "claude stderr");
        }
        buf
    });

    // Stdout reader runs inline so this function only returns after
    // claude has finished emitting for this round.
    let mut reader = BufReader::new(stdout).lines();
    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                for ev in parse_line(&line) {
                    if let Some(buf) = collect_text.as_ref() {
                        if let AIEvent::AssistantText { text, .. } = &ev {
                            let mut guard = buf.lock().expect("collect_text poisoned");
                            guard.push_str(text);
                            guard.push('\n');
                        }
                    }
                    if tx.send(ev).await.is_err() {
                        let _ = child.start_kill();
                        // Still await to clean up zombies.
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
                "claude exited with status {code}; stderr: {}",
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
                    message: format!("failed to await claude child: {e}"),
                    raw: None,
                })
                .await;
        }
    }

    Ok(())
}

/// Run the tag-protocol chain: repeatedly spawn claude, parse any
/// `<aciddog-call>` tags out of its assistant text, dispatch them
/// through the orchestrator socket channel, and feed the results back
/// as the next round's prompt. Caps the loop at `MAX_TAG_CHAIN_ROUNDS`
/// per user turn.
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
            // Consumer gone.
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
            // Synthesize a ToolUse event so the TUI can render the
            // dispatched call as a first-class action block.
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

            // Mirror the response as a ToolResult so the TUI can
            // correlate it with the preceding ToolUse via `call_id`.
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

    // Chain cap reached. Emit a visible error so the model can see on
    // its next turn that aciddog stopped chaining.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(instructions: &str) -> PromptParts {
        PromptParts {
            instructions: instructions.into(),
            ..PromptParts::default()
        }
    }

    fn fresh(instructions: &str, tools: Vec<String>) -> ClaudeCodeCore {
        ClaudeCodeCore::new(Uuid::new_v4(), parts(instructions), tools, None)
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
    fn accepts_valid_tool_names() {
        let core = fresh("i", vec!["Bash".into(), "Edit".into()]);
        assert!(core.validate_tools().is_ok());
    }

    #[test]
    fn first_turn_wraps_instructions_in_xml_block() {
        let mut core = fresh("be a helper", vec![]);
        let p = core.build_prompt("hello");
        // Instructions section is always emitted, even with no other parts.
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
        let core = ClaudeCodeCore::new(Uuid::new_v4(), parts, vec!["Read".into()], None);
        let p = core.first_turn_prompt();
        let role_pos = p.find("<role>").expect("role");
        let team_pos = p.find("<team>").expect("team");
        let tools_pos = p.find("<tools>").expect("tools");
        let instr_pos = p.find("<instructions>").expect("instructions");
        assert!(role_pos < team_pos && team_pos < tools_pos && tools_pos < instr_pos);
        assert!(p.contains("- `Read` —"), "missing built-in Read line: {p}");
    }

    #[test]
    fn role_block_omitted_when_role_is_none_or_blank() {
        let core = ClaudeCodeCore::new(Uuid::new_v4(), parts("i"), vec![], None);
        let p = core.first_turn_prompt();
        assert!(!p.contains("<role>"), "no role block expected: {p}");

        let core = ClaudeCodeCore::new(
            Uuid::new_v4(),
            PromptParts {
                role: Some("   \n  ".into()),
                instructions: "i".into(),
                ..PromptParts::default()
            },
            vec![],
            None,
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
                worker: None,
            }),
            ..PromptParts::default()
        };
        let core = ClaudeCodeCore::new(
            Uuid::new_v4(),
            parts,
            vec!["Bash".into(), "Read".into()],
            None,
        );
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "missing tools open tag: {p}");
        assert!(p.contains("- `Bash` —"), "missing Bash built-in line: {p}");
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
        let core = ClaudeCodeCore::new(Uuid::new_v4(), parts, vec!["Read".into()], None);
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
        let core = ClaudeCodeCore::new(Uuid::new_v4(), parts, vec!["Bash".into()], None);
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
        let core = ClaudeCodeCore::new(Uuid::new_v4(), parts("i"), vec!["Read".into()], None);
        let p = core.first_turn_prompt();
        assert!(p.contains("<tools>"), "tools always emitted: {p}");
        assert!(!p.contains("aciddog"), "no aciddog docs expected: {p}");
    }

    #[test]
    fn new_uses_caller_provided_session_id() {
        let id = Uuid::new_v4();
        let core = ClaudeCodeCore::new(id, parts("i"), vec![], None);
        assert_eq!(core.session_id(), Some(id));
        assert!(!core.started, "new() sessions have not started yet");
    }

    #[test]
    fn with_session_id_starts_as_already_started() {
        let id = Uuid::new_v4();
        let mut core = ClaudeCodeCore::with_session_id(id, vec![], None);
        assert_eq!(core.session_id(), Some(id));
        let p = core.build_prompt("hi");
        assert_eq!(p, "hi");
    }

    #[test]
    fn system_prompt_is_stored_when_provided() {
        let core = ClaudeCodeCore::new(
            Uuid::new_v4(),
            parts("i"),
            vec![],
            Some("you are a helper".into()),
        );
        assert_eq!(core.system_prompt.as_deref(), Some("you are a helper"));
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
    async fn spawn_failure_surfaces_as_error_event_in_stream() {
        // Binary missing now fails inside the spawned task, which means
        // turn() itself returns Ok and the error arrives as an AIEvent
        // through the stream.
        let mut core =
            fresh("i", vec![]).with_binary("/this/path/definitely/does/not/exist/claude");
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

    #[tokio::test]
    async fn spawn_failure_with_system_prompt_still_surfaces_error() {
        // Exercises the tempfile path: even with a Some(system_prompt),
        // hitting a missing binary must emit an error event.
        let mut core =
            ClaudeCodeCore::new(Uuid::new_v4(), parts("i"), vec![], Some("SYSTEM".into()))
                .with_binary("/this/path/definitely/does/not/exist/claude");
        let mut stream = core.turn("hi").await.expect("turn returns ok");
        let mut saw_error = false;
        while let Some(ev) = stream.next().await {
            if matches!(ev, AIEvent::Error { .. }) {
                saw_error = true;
                break;
            }
        }
        assert!(saw_error, "expected AIEvent::Error in stream");
    }
}
