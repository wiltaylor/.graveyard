//! Per-instance runtime state the engine drives forward and the TUI
//! reflects. `WorkflowInstance` here is the backend bookkeeping struct;
//! the TUI has its own `WorkflowInstanceView` mirror it builds from
//! `WorkflowEvent`s.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceStatus {
    Running,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Retrying,
}

/// Engine-side per-instance state. The executor task owns this struct;
/// `App` never reads it directly — it only tracks the public
/// `WorkflowInstanceView` mirror.
#[derive(Debug, Clone)]
pub struct WorkflowInstance {
    pub id: u64,
    pub workflow_id: String,
    pub current_step: usize,
    pub retries_used: u32,
    pub status: InstanceStatus,
}

impl WorkflowInstance {
    pub fn new(id: u64, workflow_id: String) -> Self {
        Self {
            id,
            workflow_id,
            current_step: 0,
            retries_used: 0,
            status: InstanceStatus::Running,
        }
    }
}
