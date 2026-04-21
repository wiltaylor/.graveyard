use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::{Tool, ToolOutput};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

pub struct BashTool {
    default_timeout: Duration,
    /// Extra env vars merged into every spawned shell (e.g.
    /// `ACIDDOG_AGENT_ID`/`ACIDDOG_AGENT_SOCKET`) so aciddog-CLI
    /// subprocesses the model shells out to can identify themselves.
    extra_env: Vec<(String, String)>,
}

impl Default for BashTool {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            extra_env: Vec::new(),
        }
    }
}

impl BashTool {
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.extra_env = env;
        self
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "Bash"
    }

    fn description(&self) -> &'static str {
        super::built_in_tool_doc("Bash")
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute via `/bin/sh -c`."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Optional timeout in milliseconds. Defaults to 120000.",
                    "minimum": 1
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory for the command."
                }
            },
            "required": ["command"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let command = match input.get("command").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => return ToolOutput::error("Bash: `command` is required"),
        };
        let tmo = input
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .map(Duration::from_millis)
            .unwrap_or(self.default_timeout);
        let cwd = input.get("cwd").and_then(Value::as_str);

        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg(&command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(format!("Bash: spawn failed: {e}")),
        };

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let run = async move {
            let mut out_buf = Vec::new();
            let mut err_buf = Vec::new();
            if let Some(ref mut h) = stdout_handle {
                let _ = h.read_to_end(&mut out_buf).await;
            }
            if let Some(ref mut h) = stderr_handle {
                let _ = h.read_to_end(&mut err_buf).await;
            }
            let status = child.wait().await;
            (status, out_buf, err_buf)
        };

        let (status, out_buf, err_buf) = match timeout(tmo, run).await {
            Ok(v) => v,
            Err(_) => {
                return ToolOutput::error(format!(
                    "Bash: timed out after {}ms",
                    tmo.as_millis()
                ))
            }
        };

        let code = match status {
            Ok(s) => s.code(),
            Err(e) => return ToolOutput::error(format!("Bash: wait failed: {e}")),
        };

        let mut stdout = String::from_utf8_lossy(&out_buf).into_owned();
        let mut stderr = String::from_utf8_lossy(&err_buf).into_owned();
        truncate(&mut stdout);
        truncate(&mut stderr);

        let mut text = String::new();
        if !stdout.is_empty() {
            text.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str("[stderr]\n");
            text.push_str(&stderr);
        }
        if text.is_empty() {
            text.push_str("(no output)\n");
        }
        match code {
            Some(0) => ToolOutput::ok(text),
            Some(c) => {
                text.push_str(&format!("\n[exit {c}]"));
                ToolOutput::error(text)
            }
            None => {
                text.push_str("\n[terminated by signal]");
                ToolOutput::error(text)
            }
        }
    }
}

fn truncate(s: &mut String) {
    if s.len() > MAX_OUTPUT_BYTES {
        s.truncate(MAX_OUTPUT_BYTES);
        s.push_str("\n... (truncated)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_stdout() {
        let out = BashTool::default()
            .invoke(json!({"command": "echo hello"}))
            .await;
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.contains("hello"));
    }

    #[tokio::test]
    async fn non_zero_exit_is_error() {
        let out = BashTool::default()
            .invoke(json!({"command": "false"}))
            .await;
        assert!(out.is_error);
        assert!(out.text.contains("[exit 1]"));
    }

    #[tokio::test]
    async fn timeout_kills_process() {
        let out = BashTool::default()
            .invoke(json!({"command": "sleep 2", "timeout_ms": 50}))
            .await;
        assert!(out.is_error);
        assert!(out.text.contains("timed out"), "got: {}", out.text);
    }
}
