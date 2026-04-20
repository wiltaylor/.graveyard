use std::process::Stdio;

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};
use uuid::Uuid;

use super::core::{AICore, AIError};
use super::event::{parse_line, AIEvent};

const DEFAULT_BINARY: &str = "claude";

pub struct ClaudeCodeCore {
    binary: String,
    session_id: Uuid,
    initial_instructions: String,
    allowed_tools: Vec<String>,
    env: Vec<(String, String)>,
    started: bool,
}

impl ClaudeCodeCore {
    /// Construct a core for a brand-new session. The caller owns the
    /// session id so it can be shared with the chat UI and the agent IPC
    /// layer before the first turn has run.
    pub fn new(session_id: Uuid, initial_instructions: String, allowed_tools: Vec<String>) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            session_id,
            initial_instructions,
            allowed_tools,
            env: Vec::new(),
            started: false,
        }
    }

    /// Construct a core that resumes a pre-existing session by id.
    pub fn with_session_id(
        session_id: Uuid,
        initial_instructions: String,
        allowed_tools: Vec<String>,
    ) -> Self {
        Self {
            binary: DEFAULT_BINARY.to_string(),
            session_id,
            initial_instructions,
            allowed_tools,
            env: Vec::new(),
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
            if self.initial_instructions.is_empty() {
                user_prompt.to_string()
            } else if user_prompt.is_empty() {
                self.initial_instructions.clone()
            } else {
                format!("{}\n\n{}", self.initial_instructions, user_prompt)
            }
        }
    }
}

#[async_trait]
impl AICore for ClaudeCodeCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        self.validate_tools()?;

        // First turn creates the session with `--session-id`; subsequent
        // turns resume it with `--resume` (claude rejects reuse of
        // `--session-id` for an existing session).
        let is_first = !self.started;
        let full_prompt = self.build_prompt(prompt);
        let tools_arg = self.allowed_tools.join(",");
        let session_id = self.session_id.to_string();

        let mut cmd = Command::new(&self.binary);
        cmd.arg("-p")
            .arg("--dangerously-skip-permissions")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            // Block online MCP servers: empty inline config + strict mode
            // means only this (empty) MCP config is honored.
            .arg("--strict-mcp-config")
            .arg("--mcp-config")
            .arg(r#"{"mcpServers":{}}"#);

        if is_first {
            cmd.arg("--session-id").arg(&session_id);
        } else {
            cmd.arg("--resume").arg(&session_id);
        }

        if !tools_arg.is_empty() {
            cmd.arg("--tools").arg(&tools_arg);
        }

        // `--tools` takes a variadic list, so terminate options with `--`
        // before the positional prompt to prevent the prompt being parsed as
        // another tool name.
        cmd.arg("--").arg(&full_prompt);

        if !self.env.is_empty() {
            cmd.envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        debug!(binary = %self.binary, session_id = %session_id, "spawning claude");

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AIError::NotFound
            } else {
                AIError::Spawn(e)
            }
        })?;

        let stdout = child
            .stdout
            .take()
            .expect("stdout was piped, must be present");
        let stderr = child
            .stderr
            .take()
            .expect("stderr was piped, must be present");

        let (tx, rx) = mpsc::channel::<AIEvent>(64);

        // Stdout reader: parse each line and forward events.
        let stdout_tx = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            loop {
                match reader.next_line().await {
                    Ok(Some(line)) => {
                        for ev in parse_line(&line) {
                            if stdout_tx.send(ev).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(None) => return,
                    Err(e) => {
                        let _ = stdout_tx
                            .send(AIEvent::Error {
                                message: format!("stdout read error: {e}"),
                                raw: None,
                            })
                            .await;
                        return;
                    }
                }
            }
        });

        // Stderr reader: buffer into a String for the exit-watcher task.
        let (stderr_tx, stderr_rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            let _ = reader.read_to_string(&mut buf).await;
            if !buf.trim().is_empty() {
                warn!(stderr = %buf, "claude stderr");
            }
            let _ = stderr_tx.send(buf);
        });

        // Exit watcher: wait for child, synthesize an Error event on failure.
        let exit_tx = tx;
        tokio::spawn(async move {
            let status = child.wait().await;
            let stderr_buf = stderr_rx.await.unwrap_or_default();
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
                    let _ = exit_tx
                        .send(AIEvent::Error {
                            message: msg,
                            raw: None,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = exit_tx
                        .send(AIEvent::Error {
                            message: format!("failed to await claude child: {e}"),
                            raw: None,
                        })
                        .await;
                }
            }
        });

        Ok(ReceiverStream::new(rx).boxed())
    }

    fn session_id(&self) -> Option<Uuid> {
        Some(self.session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(instructions: &str, tools: Vec<String>) -> ClaudeCodeCore {
        ClaudeCodeCore::new(Uuid::new_v4(), instructions.into(), tools)
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
    fn first_turn_prepends_instructions() {
        let mut core = fresh("you are a helper", vec![]);
        let p = core.build_prompt("hello");
        assert_eq!(p, "you are a helper\n\nhello");
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
    fn new_uses_caller_provided_session_id() {
        let id = Uuid::new_v4();
        let core = ClaudeCodeCore::new(id, "i".into(), vec![]);
        assert_eq!(core.session_id(), Some(id));
        assert!(!core.started, "new() sessions have not started yet");
    }

    #[test]
    fn with_session_id_starts_as_already_started() {
        let id = Uuid::new_v4();
        let mut core = ClaudeCodeCore::with_session_id(id, "ignored".into(), vec![]);
        assert_eq!(core.session_id(), Some(id));
        let p = core.build_prompt("hi");
        assert_eq!(p, "hi");
    }

    #[test]
    fn with_env_stores_entries() {
        let core = fresh("i", vec![])
            .with_env(vec![("FOO".into(), "1".into()), ("BAR".into(), "2".into())]);
        assert_eq!(core.env.len(), 2);
        assert_eq!(core.env[0], ("FOO".into(), "1".into()));
    }

    #[tokio::test]
    async fn spawn_failure_surfaces_as_error() {
        let mut core =
            fresh("i", vec![]).with_binary("/this/path/definitely/does/not/exist/claude");
        let result = core.turn("hi").await;
        assert!(matches!(
            result,
            Err(AIError::NotFound) | Err(AIError::Spawn(_))
        ));
    }
}
