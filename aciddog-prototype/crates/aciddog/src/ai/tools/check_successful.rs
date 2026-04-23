//! `check_successful` tool. Injected into the tool registry of a
//! worker spawned for a `Conditional` workflow step. The worker calls
//! it exactly once — `pass=true` means the assertion succeeded,
//! `pass=false` means the workflow executor should jump back to the
//! step's declared `target`. Missing the call altogether is treated
//! as `pass=false` per the design.

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{Tool, ToolOutput};

/// Signal emitted by the tool and routed to the engine's per-step
/// check channel.
#[derive(Debug, Clone)]
pub struct CheckSuccessfulSignal {
    pub instance_id: u64,
    pub step_id: String,
    pub pass: bool,
    pub reason: Option<String>,
}

pub struct CheckSuccessfulTool {
    sink: mpsc::UnboundedSender<CheckSuccessfulSignal>,
    instance_id: u64,
    step_id: String,
}

impl CheckSuccessfulTool {
    pub fn new(
        sink: mpsc::UnboundedSender<CheckSuccessfulSignal>,
        instance_id: u64,
        step_id: String,
    ) -> Self {
        Self {
            sink,
            instance_id,
            step_id,
        }
    }
}

#[async_trait]
impl Tool for CheckSuccessfulTool {
    fn name(&self) -> &'static str {
        "check_successful"
    }

    fn description(&self) -> &'static str {
        "Signal whether this conditional workflow step's check passed. \
         Call exactly once before finishing your turn, with \
         `pass=true` iff the check succeeded. If you never call it, \
         or call with `pass=false`, the workflow executor jumps back \
         to the step's declared retry target (bounded by the \
         workflow's max_retries)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pass": {
                    "type": "boolean",
                    "description": "true iff the check succeeded; false will trigger a retry (or failure if the retry budget is exhausted)."
                },
                "reason": {
                    "type": "string",
                    "description": "Optional short message describing why the check passed or failed. Surfaced in the TUI's progress view."
                }
            },
            "required": ["pass"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let Some(pass) = input.get("pass").and_then(|v| v.as_bool()) else {
            return ToolOutput::error("`pass` must be a boolean");
        };
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let _ = self.sink.send(CheckSuccessfulSignal {
            instance_id: self.instance_id,
            step_id: self.step_id.clone(),
            pass,
            reason: reason.clone(),
        });
        ToolOutput::ok(format!(
            "recorded pass={pass}{}",
            match &reason {
                Some(r) => format!(" ({r})"),
                None => String::new(),
            }
        ))
    }
}
