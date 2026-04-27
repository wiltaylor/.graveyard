//! `workflow_set` tool. Updates a workflow variable with a new value,
//! coerced against its declared `PropertyKind`. Emits a `VarChanged`
//! event so the TUI's variables panel refreshes. Only constructed and
//! handed to workers spawned by the workflow engine — not in
//! `BUILT_IN_TOOL_NAMES`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{Tool, ToolOutput};
use crate::config::{PropertyKind, WorkflowProperty};
use crate::workflow::{PropertyValue, WorkflowEvent};

pub struct WorkflowSetTool {
    instance_id: u64,
    vars: Arc<Mutex<HashMap<String, PropertyValue>>>,
    properties: Arc<Vec<WorkflowProperty>>,
    event_tx: mpsc::UnboundedSender<WorkflowEvent>,
}

impl WorkflowSetTool {
    pub fn new(
        instance_id: u64,
        vars: Arc<Mutex<HashMap<String, PropertyValue>>>,
        properties: Arc<Vec<WorkflowProperty>>,
        event_tx: mpsc::UnboundedSender<WorkflowEvent>,
    ) -> Self {
        Self {
            instance_id,
            vars,
            properties,
            event_tx,
        }
    }

    pub fn coerce(kind: &PropertyKind, value: &Value) -> Result<PropertyValue, String> {
        match kind {
            PropertyKind::Text { .. } | PropertyKind::Hidden { .. } => {
                let s = value
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| value.as_i64().map(|i| i.to_string()))
                    .or_else(|| value.as_f64().map(|f| f.to_string()))
                    .or_else(|| value.as_bool().map(|b| b.to_string()))
                    .ok_or_else(|| "value must be a string".to_string())?;
                Ok(PropertyValue::Text(s))
            }
            PropertyKind::Number { min, max, .. } => {
                let n = if let Some(f) = value.as_f64() {
                    f
                } else if let Some(i) = value.as_i64() {
                    i as f64
                } else if let Some(s) = value.as_str() {
                    s.parse::<f64>()
                        .map_err(|_| format!("value `{s}` is not a number"))?
                } else {
                    return Err("value must be a number or numeric string".into());
                };
                if let Some(m) = min {
                    if n < *m {
                        return Err(format!("value {n} is below min {m}"));
                    }
                }
                if let Some(m) = max {
                    if n > *m {
                        return Err(format!("value {n} is above max {m}"));
                    }
                }
                Ok(PropertyValue::Number(n))
            }
            PropertyKind::Select { options, .. } => {
                let s = value
                    .as_str()
                    .ok_or_else(|| "select value must be a string".to_string())?;
                if !options.iter().any(|o| o == s) {
                    return Err(format!(
                        "value `{s}` not in allowed options [{}]",
                        options.join(", ")
                    ));
                }
                Ok(PropertyValue::Selection(s.to_string()))
            }
            PropertyKind::MultiSelect { options, .. } => {
                // Accept either a JSON array of strings or a comma-
                // separated string. Both forms are validated against
                // the declared option list.
                let parts: Vec<String> = if let Some(arr) = value.as_array() {
                    arr.iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_string)
                                .ok_or_else(|| "multiSelect items must be strings".to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else if let Some(s) = value.as_str() {
                    s.split(',').map(|x| x.trim().to_string()).collect()
                } else {
                    return Err("multiSelect value must be a list or comma-separated string".into());
                };
                for p in &parts {
                    if !options.iter().any(|o| o == p) {
                        return Err(format!(
                            "value `{p}` not in allowed options [{}]",
                            options.join(", ")
                        ));
                    }
                }
                Ok(PropertyValue::Multi(parts))
            }
        }
    }
}

#[async_trait]
impl Tool for WorkflowSetTool {
    fn name(&self) -> &'static str {
        "workflow_set"
    }

    fn description(&self) -> &'static str {
        "Update a workflow variable. Value is coerced against the \
         declared property kind (text/number/select/multiSelect/\
         hidden) and rejected if it fails validation. Later steps \
         will see the new value via `$$VAR$$` substitution or via \
         `workflow_get`."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The workflow variable (property) name to write."
                },
                "value": {
                    "description": "The new value. Strings, numbers, booleans, and arrays of strings are all accepted; the tool coerces to the property's declared kind.",
                }
            },
            "required": ["name", "value"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolOutput {
        let Some(name) = input.get("name").and_then(|v| v.as_str()) else {
            return ToolOutput::error("`name` is required");
        };
        let Some(value) = input.get("value") else {
            return ToolOutput::error("`value` is required");
        };
        let Some(prop) = self.properties.iter().find(|p| p.id == name) else {
            return ToolOutput::error(format!("no variable `{name}` in this workflow"));
        };
        let coerced = match Self::coerce(&prop.kind, value) {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(format!("`{name}`: {e}")),
        };

        {
            let mut guard = self.vars.lock().expect("vars mutex poisoned");
            guard.insert(name.to_string(), coerced.clone());
        }
        let _ = self.event_tx.send(WorkflowEvent::VarChanged {
            instance_id: self.instance_id,
            name: name.to_string(),
            value: coerced.clone(),
        });
        ToolOutput::ok(format!("set `{name}` = {}", coerced.as_display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(kind: PropertyKind) -> WorkflowProperty {
        WorkflowProperty {
            id: "x".into(),
            label: None,
            required: false,
            kind,
        }
    }

    #[test]
    fn coerce_text_from_number_json() {
        let v =
            WorkflowSetTool::coerce(&PropertyKind::Text { default: None }, &json!(42)).expect("ok");
        assert_eq!(v, PropertyValue::Text("42".into()));
    }

    #[test]
    fn coerce_number_from_string() {
        let v = WorkflowSetTool::coerce(
            &PropertyKind::Number {
                default: None,
                min: None,
                max: None,
            },
            &json!("3.14"),
        )
        .expect("ok");
        match v {
            PropertyValue::Number(n) => {
                #[allow(clippy::approx_constant)]
                let expected = 3.14_f64;
                assert!((n - expected).abs() < 1e-9);
            }
            other => panic!("expected Number, got {other:?}"),
        }
    }

    #[test]
    fn coerce_number_respects_min_max() {
        let p = PropertyKind::Number {
            default: None,
            min: Some(0.0),
            max: Some(10.0),
        };
        assert!(WorkflowSetTool::coerce(&p, &json!(5)).is_ok());
        assert!(WorkflowSetTool::coerce(&p, &json!(-1)).is_err());
        assert!(WorkflowSetTool::coerce(&p, &json!(11)).is_err());
    }

    #[test]
    fn coerce_select_rejects_unknown_option() {
        let p = PropertyKind::Select {
            options: vec!["a".into(), "b".into()],
            default: None,
        };
        assert!(WorkflowSetTool::coerce(&p, &json!("a")).is_ok());
        assert!(WorkflowSetTool::coerce(&p, &json!("c")).is_err());
    }

    #[test]
    fn coerce_multi_select_accepts_csv_or_array() {
        let p = PropertyKind::MultiSelect {
            options: vec!["a".into(), "b".into(), "c".into()],
            default: None,
        };
        let v1 = WorkflowSetTool::coerce(&p, &json!(["a", "c"])).expect("array ok");
        assert_eq!(v1, PropertyValue::Multi(vec!["a".into(), "c".into()]));
        let v2 = WorkflowSetTool::coerce(&p, &json!("a, c")).expect("csv ok");
        assert_eq!(v2, PropertyValue::Multi(vec!["a".into(), "c".into()]));
        assert!(WorkflowSetTool::coerce(&p, &json!(["d"])).is_err());
    }

    #[test]
    fn unused_instance_id_and_properties_keep_helper_api_stable() {
        // Silence dead-code on the prop-only helper when the full
        // WorkflowSetTool isn't exercised in unit tests.
        let _p = prop(PropertyKind::Text { default: None });
    }
}
