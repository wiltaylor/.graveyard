//! `workflow_get` tool. Reads a workflow variable and returns its
//! display form. Only constructed and handed to workers spawned by the
//! workflow engine for a `Worker` or `Conditional` step — not in
//! `BUILT_IN_TOOL_NAMES`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolOutput};
use crate::workflow::PropertyValue;

pub struct WorkflowGetTool {
    vars: Arc<Mutex<HashMap<String, PropertyValue>>>,
}

impl WorkflowGetTool {
    pub fn new(vars: Arc<Mutex<HashMap<String, PropertyValue>>>) -> Self {
        Self { vars }
    }
}

#[async_trait]
impl Tool for WorkflowGetTool {
    fn name(&self) -> &'static str {
        "workflow_get"
    }

    fn description(&self) -> &'static str {
        "Read the current value of a workflow variable by name. \
         Returns the value as a string (lists are comma-joined). \
         Errors if the variable has never been declared in this \
         workflow."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The workflow variable (property) name to read."
                }
            },
            "required": ["name"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let Some(name) = input.get("name").and_then(|v| v.as_str()) else {
            return ToolOutput::error("`name` is required");
        };
        let guard = self.vars.lock().expect("vars mutex poisoned");
        match guard.get(name) {
            Some(v) => ToolOutput::ok(v.as_display()),
            None => ToolOutput::error(format!("no variable `{name}` in this workflow")),
        }
    }
}
