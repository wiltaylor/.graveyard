//! Deserialize mirror of `aciddog::wire_workflow`.
//!
//! Same rationale as [`super::wire_tasks`]: the manager crate cannot
//! depend on the `aciddog` crate because both transitively link
//! `sqlite3`. Shapes duplicated here must stay in lockstep with the
//! canonical definitions at `crates/aciddog/src/wire_workflow.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireWorkflowTemplate {
    pub id: String,
    pub description: String,
    pub max_retries: u32,
    pub properties: Vec<WireWorkflowProperty>,
    pub steps: Vec<WireTemplateStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTemplateStep {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireWorkflowProperty {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub required: bool,
    pub kind: WirePropertyKind,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireWorkflowInstance {
    pub id: u64,
    pub workflow_id: String,
    pub status: WireInstanceStatus,
    pub current_step: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub steps: Vec<WireStepProgress>,
    pub tail_output: Vec<String>,
    pub vars: Vec<WireVarSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_in_ms: Option<u64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireVarSnapshot {
    pub name: String,
    pub display: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireInstanceStatus {
    Running,
    Failed,
    Succeeded,
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
