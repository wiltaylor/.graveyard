//! Serde-friendly projection of the workflow runtime types.
//!
//! Three concerns live here:
//!
//! - `WireWorkflowTemplate` / `WireWorkflowProperty` / `WirePropertyKind` —
//!   metadata used by the web portal to render the property-entry form.
//!   `Hidden` properties are filtered out at conversion time, matching
//!   `WorkflowFormState::new`'s user-visible filter.
//! - `WireWorkflowInstance` / `WireStepProgress` / `WireVarSnapshot` —
//!   the live-instance snapshot surfaced by `WorkflowInstancesJson`.
//! - `WireWorkflowEvent` — the SSE stream payload, a 1:1 mirror of
//!   `workflow::WorkflowEvent` with serde derived and `PropertyValue`
//!   flattened to its display string so the browser doesn't have to
//!   understand the variant. Conversion happens at the broadcast
//!   fan-out point in `App::on_workflow_event`, so the internal
//!   event type stays free of serde derives.
//!
//! Conversion helpers are intentionally infallible — this module is
//! "project backend state onto the wire", not validation.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::config::{PropertyKind, Workflow};
use crate::tui::workflow_view::WorkflowInstanceView;
use crate::workflow::{InstanceStatus, PropertyValue, StepOutcome, StepStatus, WorkflowEvent};

/// A declared workflow plus enough metadata to render its property form
/// and a structural diagram. `steps` carries the per-step retry target
/// (for `conditional` / `check`) so the portal can draw back-edges
/// without a second round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireWorkflowTemplate {
    pub id: String,
    pub description: String,
    pub max_retries: u32,
    pub properties: Vec<WireWorkflowProperty>,
    pub steps: Vec<WireTemplateStep>,
}

impl WireWorkflowTemplate {
    pub fn from_config(wf: &Workflow) -> Self {
        let properties = wf
            .properties
            .iter()
            .filter(|p| p.kind.is_user_visible())
            .map(|p| WireWorkflowProperty {
                id: p.id.clone(),
                label: p.label.clone(),
                required: p.required,
                kind: WirePropertyKind::from_kind(&p.kind),
            })
            .collect();
        Self {
            id: wf.id.clone(),
            description: wf.description.clone(),
            max_retries: wf.max_retries,
            properties,
            steps: wf.steps.iter().map(WireTemplateStep::from_step).collect(),
        }
    }
}

/// Static (declared) representation of a single step. Mirrors the
/// `kind` discriminator used by `WireStepProgress` so the frontend can
/// join template structure with instance runtime status by step id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTemplateStep {
    pub id: String,
    pub kind: String,
    /// Retry target step id — populated for `conditional` / `check`
    /// steps, `None` for `worker` / `command`. Identifies the step
    /// the engine jumps back to on failure (the retry edge in the
    /// diagram).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl WireTemplateStep {
    pub fn from_step(s: &crate::config::WorkflowStep) -> Self {
        let target = match &s.kind {
            crate::config::StepKind::Conditional { target, .. }
            | crate::config::StepKind::Check { target, .. } => Some(target.clone()),
            _ => None,
        };
        Self {
            id: s.id.clone(),
            kind: s.kind.name().to_string(),
            target,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireWorkflowProperty {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub required: bool,
    pub kind: WirePropertyKind,
}

/// Shape-compatible mirror of `config::PropertyKind` with `Hidden`
/// dropped (callers filter those out before constructing the template).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WirePropertyKind {
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Select {
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    MultiSelect {
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<Vec<String>>,
    },
}

impl WirePropertyKind {
    /// `Hidden` is projected onto `Text` so the callsite doesn't have
    /// to handle an `Option` return — but in practice every caller
    /// filters `Hidden` out first via `PropertyKind::is_user_visible`.
    pub fn from_kind(k: &PropertyKind) -> Self {
        match k {
            PropertyKind::Text { default } => Self::Text {
                default: default.clone(),
            },
            PropertyKind::Number { default, min, max } => Self::Number {
                default: *default,
                min: *min,
                max: *max,
            },
            PropertyKind::Select { options, default } => Self::Select {
                options: options.clone(),
                default: default.clone(),
            },
            PropertyKind::MultiSelect { options, default } => Self::MultiSelect {
                options: options.clone(),
                default: default.clone(),
            },
            PropertyKind::Hidden { default } => Self::Text {
                default: default.clone(),
            },
        }
    }
}

/// Live snapshot of one running / failed / lingering instance. Mirrors
/// `tui::workflow_view::WorkflowInstanceView` with everything stripped
/// to serializable primitives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireWorkflowInstance {
    pub id: u64,
    pub workflow_id: String,
    pub status: WireInstanceStatus,
    pub current_step: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub steps: Vec<WireStepProgress>,
    /// Tail of recent output lines (drained from the VecDeque in-order).
    pub tail_output: Vec<String>,
    /// Variables sorted by name for stable ordering in the UI.
    pub vars: Vec<WireVarSnapshot>,
    /// Milliseconds from *now* until the TUI auto-removes this succeeded
    /// instance. `None` for running / failed. Lets the browser mirror
    /// the 4-second linger without knowing the absolute deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_in_ms: Option<u64>,
}

impl WireWorkflowInstance {
    pub fn from_view(view: &WorkflowInstanceView) -> Self {
        let mut vars = WireVarSnapshot::sorted_from_map(&view.vars);
        vars.sort_by(|a, b| a.name.cmp(&b.name));
        let now = Instant::now();
        let remove_in_ms = view
            .remove_at
            .and_then(|t: Instant| t.checked_duration_since(now))
            .map(|d| d.as_millis() as u64);
        Self {
            id: view.id,
            workflow_id: view.workflow_id.clone(),
            status: WireInstanceStatus::from(view.status),
            current_step: view.current_step,
            failure_reason: view.failure_reason.clone(),
            steps: view.steps.iter().map(WireStepProgress::from_step).collect(),
            tail_output: view.tail_output.iter().cloned().collect(),
            vars,
            remove_in_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireStepProgress {
    pub id: String,
    pub kind: String,
    pub status: WireStepStatus,
    pub retry_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl WireStepProgress {
    pub fn from_step(s: &crate::tui::workflow_view::StepProgress) -> Self {
        Self {
            id: s.id.clone(),
            kind: s.kind_label.to_string(),
            status: WireStepStatus::from(s.status),
            retry_count: s.retry_count,
            reason: s.reason.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireVarSnapshot {
    pub name: String,
    pub display: String,
}

impl WireVarSnapshot {
    pub fn from_pair(name: &str, value: &PropertyValue) -> Self {
        Self {
            name: name.to_string(),
            display: value.as_display(),
        }
    }

    pub fn sorted_from_map(map: &HashMap<String, PropertyValue>) -> Vec<Self> {
        let mut out: Vec<Self> = map.iter().map(|(n, v)| Self::from_pair(n, v)).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireInstanceStatus {
    Running,
    Failed,
    Succeeded,
}

impl From<InstanceStatus> for WireInstanceStatus {
    fn from(s: InstanceStatus) -> Self {
        match s {
            InstanceStatus::Running => Self::Running,
            InstanceStatus::Failed => Self::Failed,
            InstanceStatus::Succeeded => Self::Succeeded,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireStepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Retrying,
}

impl From<StepStatus> for WireStepStatus {
    fn from(s: StepStatus) -> Self {
        match s {
            StepStatus::Pending => Self::Pending,
            StepStatus::Running => Self::Running,
            StepStatus::Succeeded => Self::Succeeded,
            StepStatus::Failed => Self::Failed,
            StepStatus::Retrying => Self::Retrying,
        }
    }
}

/// SSE stream payload. 1:1 mirror of `workflow::WorkflowEvent`, but
/// with `PropertyValue` flattened to display strings so the browser
/// doesn't need to understand runtime value variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WireWorkflowEvent {
    Started {
        instance_id: u64,
        workflow_id: String,
        vars: Vec<WireVarSnapshot>,
    },
    StepStarted {
        instance_id: u64,
        step_idx: usize,
    },
    StepOutput {
        instance_id: u64,
        line: String,
    },
    VarChanged {
        instance_id: u64,
        name: String,
        display: String,
    },
    StepFinished {
        instance_id: u64,
        step_idx: usize,
        outcome: WireStepOutcome,
    },
    Retrying {
        instance_id: u64,
        target_step_idx: usize,
        attempt: u32,
    },
    Finished {
        instance_id: u64,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl WireWorkflowEvent {
    pub fn from_event(ev: &WorkflowEvent) -> Self {
        match ev {
            WorkflowEvent::Started {
                instance_id,
                workflow_id,
                vars,
            } => Self::Started {
                instance_id: *instance_id,
                workflow_id: workflow_id.clone(),
                vars: vars
                    .iter()
                    .map(|(n, v)| WireVarSnapshot::from_pair(n, v))
                    .collect(),
            },
            WorkflowEvent::StepStarted {
                instance_id,
                step_idx,
            } => Self::StepStarted {
                instance_id: *instance_id,
                step_idx: *step_idx,
            },
            WorkflowEvent::StepOutput { instance_id, line } => Self::StepOutput {
                instance_id: *instance_id,
                line: line.clone(),
            },
            WorkflowEvent::VarChanged {
                instance_id,
                name,
                value,
            } => Self::VarChanged {
                instance_id: *instance_id,
                name: name.clone(),
                display: value.as_display(),
            },
            WorkflowEvent::StepFinished {
                instance_id,
                step_idx,
                outcome,
            } => Self::StepFinished {
                instance_id: *instance_id,
                step_idx: *step_idx,
                outcome: WireStepOutcome::from_outcome(outcome),
            },
            WorkflowEvent::Retrying {
                instance_id,
                target_step_idx,
                attempt,
            } => Self::Retrying {
                instance_id: *instance_id,
                target_step_idx: *target_step_idx,
                attempt: *attempt,
            },
            WorkflowEvent::Finished {
                instance_id,
                ok,
                reason,
            } => Self::Finished {
                instance_id: *instance_id,
                ok: *ok,
                reason: reason.clone(),
            },
        }
    }

    /// Instance id carried by every variant — used by the SSE handler
    /// to filter when a caller scopes the stream to one instance.
    pub fn instance_id(&self) -> u64 {
        match self {
            Self::Started { instance_id, .. }
            | Self::StepStarted { instance_id, .. }
            | Self::StepOutput { instance_id, .. }
            | Self::VarChanged { instance_id, .. }
            | Self::StepFinished { instance_id, .. }
            | Self::Retrying { instance_id, .. }
            | Self::Finished { instance_id, .. } => *instance_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WireStepOutcome {
    Ok,
    Retry { reason: String },
    Failed { reason: String },
}

impl WireStepOutcome {
    pub fn from_outcome(o: &StepOutcome) -> Self {
        match o {
            StepOutcome::Ok => Self::Ok,
            StepOutcome::Retry { reason } => Self::Retry {
                reason: reason.clone(),
            },
            StepOutcome::Failed { reason } => Self::Failed {
                reason: reason.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{WorkflowProperty, WorkflowStep};

    #[test]
    fn template_drops_hidden_properties() {
        let wf = Workflow {
            id: "w".into(),
            description: "d".into(),
            max_retries: 3,
            properties: vec![
                WorkflowProperty {
                    id: "visible".into(),
                    label: Some("Visible".into()),
                    required: true,
                    kind: PropertyKind::Text {
                        default: Some("hi".into()),
                    },
                },
                WorkflowProperty {
                    id: "secret".into(),
                    label: None,
                    required: false,
                    kind: PropertyKind::Hidden { default: None },
                },
            ],
            steps: vec![WorkflowStep {
                id: "s".into(),
                kind: crate::config::StepKind::Command {
                    command: "true".into(),
                    cwd: None,
                },
            }],
        };
        let t = WireWorkflowTemplate::from_config(&wf);
        assert_eq!(t.properties.len(), 1);
        assert_eq!(t.properties[0].id, "visible");
        assert_eq!(t.steps.len(), 1);
        assert_eq!(t.steps[0].id, "s");
        assert_eq!(t.steps[0].kind, "command");
        assert!(t.steps[0].target.is_none());
    }

    #[test]
    fn template_steps_carry_retry_targets() {
        use crate::config::CheckOp;
        let wf = Workflow {
            id: "w".into(),
            description: "d".into(),
            max_retries: 3,
            properties: vec![],
            steps: vec![
                WorkflowStep {
                    id: "echo".into(),
                    kind: crate::config::StepKind::Command {
                        command: "true".into(),
                        cwd: None,
                    },
                },
                WorkflowStep {
                    id: "gate".into(),
                    kind: crate::config::StepKind::Check {
                        variable: "ok".into(),
                        op: CheckOp::Equals,
                        value: Some("yes".into()),
                        target: "echo".into(),
                    },
                },
            ],
        };
        let t = WireWorkflowTemplate::from_config(&wf);
        assert_eq!(t.steps.len(), 2);
        assert!(t.steps[0].target.is_none());
        assert_eq!(t.steps[1].target.as_deref(), Some("echo"));
    }

    #[test]
    fn property_kind_select_serializes_with_kind_tag() {
        let k = WirePropertyKind::Select {
            options: vec!["a".into(), "b".into()],
            default: Some("b".into()),
        };
        let v = serde_json::to_value(&k).unwrap();
        assert_eq!(v["kind"], "select");
        assert_eq!(v["options"], serde_json::json!(["a", "b"]));
        assert_eq!(v["default"], "b");
    }

    #[test]
    fn event_started_includes_sorted_vars() {
        let ev = WorkflowEvent::Started {
            instance_id: 7,
            workflow_id: "deploy".into(),
            vars: vec![
                ("zzz".into(), PropertyValue::Text("z".into())),
                ("aaa".into(), PropertyValue::Number(1.0)),
            ],
        };
        let wire = WireWorkflowEvent::from_event(&ev);
        let v = serde_json::to_value(&wire).unwrap();
        assert_eq!(v["event"], "started");
        assert_eq!(v["instance_id"], 7);
        assert_eq!(v["workflow_id"], "deploy");
        // Order mirrors the source event (no re-sort at event time;
        // sorted order is a list-snapshot convention, not a stream one).
        assert_eq!(v["vars"][0]["name"], "zzz");
        assert_eq!(v["vars"][0]["display"], "z");
        assert_eq!(v["vars"][1]["name"], "aaa");
        assert_eq!(v["vars"][1]["display"], "1");
    }

    #[test]
    fn instance_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(WireInstanceStatus::Running).unwrap(),
            serde_json::json!("running")
        );
        assert_eq!(
            serde_json::to_value(WireInstanceStatus::Succeeded).unwrap(),
            serde_json::json!("succeeded")
        );
    }

    #[test]
    fn var_snapshot_sort_is_lexicographic() {
        let mut m: HashMap<String, PropertyValue> = HashMap::new();
        m.insert("zeta".into(), PropertyValue::Text("z".into()));
        m.insert("alpha".into(), PropertyValue::Text("a".into()));
        let sorted = WireVarSnapshot::sorted_from_map(&m);
        assert_eq!(sorted[0].name, "alpha");
        assert_eq!(sorted[1].name, "zeta");
    }
}
