//! Workflow runtime. A `WorkflowEngine` spawns one tokio task per
//! running instance and publishes progress back to the TUI through an
//! mpsc channel of `WorkflowEvent`s. Instances are ephemeral: there is
//! no disk persistence today.
//!
//! The engine is constructed with a shared `AciddogConfig` snapshot so
//! per-instance workers (`Worker` / `Conditional` steps) can be spawned
//! through a caller-supplied sink (`SpawnRequest`) — the TUI owns the
//! actual worker-spawn machinery. `Check` and `Command` steps run
//! entirely inside the engine task, so those work without any external
//! plumbing.

pub mod engine;
pub mod event;
pub mod instance;
pub mod render;
pub mod value;

pub use event::{StepOutcome, WorkflowEvent};
pub use instance::{InstanceStatus, StepStatus, WorkflowInstance};
pub use render::{render_template, RenderError};
pub use value::PropertyValue;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::config::Workflow;

pub use crate::ai::tools::check_successful::CheckSuccessfulSignal;

/// Handle to a running instance owned by the engine. Stores what the
/// engine needs to route `check_successful` signals and to cancel the
/// task on dismissal.
pub struct InstanceHandle {
    pub workflow_id: String,
    pub cancel_tx: mpsc::Sender<()>,
    /// Live variable map shared with the per-instance tools so
    /// `workflow_get` / `workflow_set` read + mutate the same state
    /// the step executor sees.
    pub vars: Arc<Mutex<HashMap<String, PropertyValue>>>,
    /// Inbound signals from the conditional step's `CheckSuccessfulTool`.
    /// Scoped to this instance; the engine dispatches signals to the
    /// active step's channel.
    pub check_tx: mpsc::UnboundedSender<CheckSuccessfulSignal>,
}

/// Owner of all running workflow instances. Hands out handles by id and
/// multiplexes `CheckSuccessfulSignal`s into the right per-instance
/// channel.
pub struct WorkflowEngine {
    pub instances: HashMap<u64, InstanceHandle>,
    /// Outbound progress events drained by the TUI event loop.
    pub event_tx: mpsc::UnboundedSender<WorkflowEvent>,
    /// Shared config snapshot used by Worker / Conditional steps to
    /// resolve worker templates and their ai_core.
    pub config: Arc<crate::config::AciddogConfig>,
}

impl WorkflowEngine {
    pub fn new(
        config: Arc<crate::config::AciddogConfig>,
        event_tx: mpsc::UnboundedSender<WorkflowEvent>,
    ) -> Self {
        Self {
            instances: HashMap::new(),
            event_tx,
            config,
        }
    }

    /// Seed the initial variable map from supplied user inputs plus
    /// every property's declared default. User-visible inputs override
    /// defaults; hidden properties are seeded from their default only
    /// (callers have no way to supply them).
    pub fn seed_vars(
        workflow: &Workflow,
        user_inputs: HashMap<String, PropertyValue>,
    ) -> HashMap<String, PropertyValue> {
        let mut vars: HashMap<String, PropertyValue> = HashMap::new();
        for p in &workflow.properties {
            if let Some(v) = PropertyValue::default_from(&p.kind) {
                vars.insert(p.id.clone(), v);
            }
        }
        for (k, v) in user_inputs {
            vars.insert(k, v);
        }
        vars
    }

    /// Forward a `check_successful` signal from a tool to the matching
    /// instance's executor. No-op if the instance has already completed.
    pub fn dispatch_check(&self, sig: CheckSuccessfulSignal) {
        if let Some(handle) = self.instances.get(&sig.instance_id) {
            let _ = handle.check_tx.send(sig);
        }
    }

    /// Remove and drop an instance handle (used when the user dismisses
    /// a failed instance, or after an instance reports success).
    pub fn forget(&mut self, instance_id: u64) {
        self.instances.remove(&instance_id);
    }

    /// Start a new instance of `workflow` with the given initial
    /// variable map. Spawns a tokio task that drives the step loop and
    /// returns the handle immediately.
    pub fn start_instance(
        &mut self,
        instance_id: u64,
        workflow: &crate::config::Workflow,
        initial_vars: HashMap<String, PropertyValue>,
    ) {
        let vars = Arc::new(Mutex::new(initial_vars));
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
        let (check_tx, check_rx) = mpsc::unbounded_channel::<CheckSuccessfulSignal>();
        let handle = InstanceHandle {
            workflow_id: workflow.id.clone(),
            cancel_tx,
            vars: Arc::clone(&vars),
            check_tx,
        };
        self.instances.insert(instance_id, handle);
        let wf_clone = workflow.clone();
        let event_tx = self.event_tx.clone();
        let config = Arc::clone(&self.config);
        tokio::spawn(engine::run_instance(
            instance_id,
            wf_clone,
            vars,
            config,
            event_tx,
            check_rx,
            cancel_rx,
        ));
    }
}
