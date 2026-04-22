//! Events emitted by a running workflow instance. The TUI drains these
//! from an mpsc channel and mutates its `WorkflowInstanceView` list.

use super::value::PropertyValue;

/// Outcome of a single step. `Retry` means a `conditional` or `check`
/// step failed but the instance's retry budget is not yet exhausted —
/// the executor follows up with `Retrying` + `StepStarted` for the
/// target step.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    Ok,
    Retry { reason: String },
    Failed { reason: String },
}

#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    Started {
        instance_id: u64,
        workflow_id: String,
        /// Snapshot of every variable's initial value (after merging
        /// user inputs with defaults). Used by the TUI's Variables
        /// panel before any `VarChanged` arrives.
        vars: Vec<(String, PropertyValue)>,
    },
    StepStarted {
        instance_id: u64,
        step_idx: usize,
    },
    /// One line of live output from a `Command` step, or a tool-result
    /// snippet from a `Worker` step. Append-only; the TUI keeps only a
    /// tail.
    StepOutput {
        instance_id: u64,
        line: String,
    },
    /// A variable was mutated by a worker via `workflow_set` (or by the
    /// engine seeding a default). Lets the TUI update its Variables
    /// panel without polling the shared mutex.
    VarChanged {
        instance_id: u64,
        name: String,
        value: PropertyValue,
    },
    StepFinished {
        instance_id: u64,
        step_idx: usize,
        outcome: StepOutcome,
    },
    /// A `conditional` / `check` step's retry budget decremented and
    /// the executor is about to re-enter the target step.
    Retrying {
        instance_id: u64,
        target_step_idx: usize,
        attempt: u32,
    },
    /// Instance left the event loop. `ok=true` ⇒ TUI removes the
    /// instance from its list; `ok=false` ⇒ TUI keeps it with a red
    /// icon and `reason` populated.
    Finished {
        instance_id: u64,
        ok: bool,
        reason: Option<String>,
    },
}
