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
mod owner;
pub mod render;
pub mod value;

pub use event::{StepOutcome, WorkflowEvent};
pub use instance::{InstanceStatus, StepStatus, WorkflowInstance};
pub use owner::{OwnerHandle, OwnerId};
pub use render::{render_template, RenderError};
pub use value::PropertyValue;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::config::Workflow;
use owner::{OwnerSlot, OWNER_BROADCAST_CAPACITY};

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
    ownership_registry: HashMap<OwnerId, Arc<OwnerSlot>>,
    instance_owner: HashMap<u64, OwnerId>,
}

impl WorkflowEngine {
    pub fn new(
        config: Arc<crate::config::AciddogConfig>,
        event_tx: mpsc::UnboundedSender<WorkflowEvent>,
    ) -> Self {
        let mut ownership_registry = HashMap::new();
        let (events_tx, _) = tokio::sync::broadcast::channel(OWNER_BROADCAST_CAPACITY);
        ownership_registry.insert(
            OwnerId::default_legacy(),
            Arc::new(OwnerSlot {
                max_in_flight: None,
                events_tx,
            }),
        );
        Self {
            instances: HashMap::new(),
            event_tx,
            config,
            ownership_registry,
            instance_owner: HashMap::new(),
        }
    }

    /// Register an owner. Idempotent: if the id already exists, returns a
    /// handle to the existing slot without overwriting it.
    pub fn register_owner(&mut self, id: OwnerId, max_in_flight: Option<usize>) -> OwnerHandle {
        let slot = self
            .ownership_registry
            .entry(id.clone())
            .or_insert_with(|| {
                let (tx, _) = tokio::sync::broadcast::channel(OWNER_BROADCAST_CAPACITY);
                Arc::new(OwnerSlot {
                    max_in_flight,
                    events_tx: tx,
                })
            });
        OwnerHandle::new(id, slot.clone())
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
        // internal bookkeeping - sibling 92bfc4dc uses this for event routing
        self.instance_owner
            .insert(instance_id, OwnerId::default_legacy());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AciddogConfig, PropertyKind, StepKind, Team, WorkflowProperty, WorkflowStep,
    };
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::Duration;

    fn minimal_config() -> AciddogConfig {
        AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
                default_idle_timeout: None,
            },
            agents: vec![],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            repositories: vec![],
            cron_jobs: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        }
    }

    fn one_command_workflow(command: impl Into<String>) -> Workflow {
        Workflow {
            id: "w".into(),
            description: "d".into(),
            max_retries: 1,
            properties: vec![WorkflowProperty {
                id: "msg".into(),
                label: None,
                required: false,
                kind: PropertyKind::Text {
                    default: Some("hi".into()),
                },
            }],
            steps: vec![WorkflowStep {
                id: "only".into(),
                kind: StepKind::Command {
                    command: command.into(),
                    cwd: None,
                },
            }],
        }
    }

    fn engine_with_rx() -> (
        WorkflowEngine,
        tokio::sync::mpsc::UnboundedReceiver<WorkflowEvent>,
    ) {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            WorkflowEngine::new(Arc::new(minimal_config()), event_tx),
            event_rx,
        )
    }

    fn hash_owner_id(owner_id: &OwnerId) -> u64 {
        let mut hasher = DefaultHasher::new();
        owner_id.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn owner_id_round_trip() {
        let owner_id = OwnerId::from("x");
        let same = OwnerId::new("x");

        assert_eq!(owner_id.to_string(), "x");
        assert_eq!(owner_id.as_ref(), "x");
        assert_eq!(owner_id, same);
        assert_eq!(hash_owner_id(&owner_id), hash_owner_id(&same));
    }

    #[test]
    fn register_owner_returns_handle() {
        let (mut engine, _event_rx) = engine_with_rx();
        let owner_id = OwnerId::from("owner-a");

        let handle = engine.register_owner(owner_id.clone(), Some(2));
        let _receiver = handle.subscribe();

        assert_eq!(handle.owner_id(), &owner_id);
        assert_eq!(
            engine
                .ownership_registry
                .get(&owner_id)
                .expect("owner slot registered")
                .max_in_flight,
            Some(2)
        );
    }

    #[tokio::test]
    async fn register_owner_is_idempotent() {
        let (mut engine, _event_rx) = engine_with_rx();
        let owner_id = OwnerId::from("owner-a");
        let first = engine.register_owner(owner_id.clone(), Some(2));
        let second = engine.register_owner(owner_id.clone(), Some(5));
        let mut first_rx = first.subscribe();
        let mut second_rx = second.subscribe();

        let slot = engine
            .ownership_registry
            .get(&owner_id)
            .expect("owner slot registered");
        assert_eq!(slot.max_in_flight, Some(2));
        slot.events_tx
            .send(WorkflowEvent::Finished {
                instance_id: 9,
                ok: true,
                reason: None,
            })
            .expect("receivers subscribed");

        assert!(matches!(
            first_rx.recv().await.expect("first receiver event"),
            WorkflowEvent::Finished {
                instance_id: 9,
                ok: true,
                reason: None
            }
        ));
        assert!(matches!(
            second_rx.recv().await.expect("second receiver event"),
            WorkflowEvent::Finished {
                instance_id: 9,
                ok: true,
                reason: None
            }
        ));
    }

    #[test]
    fn default_legacy_owner_registered_on_new() {
        let (mut engine, _event_rx) = engine_with_rx();
        let legacy_id = OwnerId::default_legacy();
        let original = engine
            .ownership_registry
            .get(&legacy_id)
            .expect("legacy owner registered")
            .clone();

        let handle = engine.register_owner(legacy_id.clone(), Some(9));
        let current = engine
            .ownership_registry
            .get(&legacy_id)
            .expect("legacy owner still registered")
            .clone();

        assert_eq!(handle.owner_id(), &legacy_id);
        assert!(Arc::ptr_eq(&original, &current));
        assert_eq!(current.max_in_flight, None);
    }

    #[tokio::test]
    async fn start_instance_records_default_owner_mapping() {
        let (mut engine, mut event_rx) = engine_with_rx();
        let workflow = one_command_workflow("true");

        engine.start_instance(42, &workflow, HashMap::new());

        assert_eq!(
            engine.instance_owner.get(&42),
            Some(&OwnerId::default_legacy())
        );

        while let Ok(Some(WorkflowEvent::Finished { .. })) =
            tokio::time::timeout(Duration::from_secs(5), event_rx.recv()).await
        {
            break;
        }
    }

    #[tokio::test]
    async fn start_instance_legacy_event_stream_unchanged() {
        let (mut engine, mut event_rx) = engine_with_rx();
        let workflow = one_command_workflow("true");

        engine.start_instance(7, &workflow, HashMap::new());

        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
                .await
                .expect("workflow event timed out")
                .expect("workflow event channel closed");
            let finished = matches!(event, WorkflowEvent::Finished { .. });
            events.push(event);
            if finished {
                break;
            }
        }

        assert_eq!(events.len(), 4);
        assert!(matches!(
            &events[0],
            WorkflowEvent::Started {
                instance_id: 7,
                workflow_id,
                ..
            } if workflow_id == "w"
        ));
        assert!(matches!(
            events[1],
            WorkflowEvent::StepStarted {
                instance_id: 7,
                step_idx: 0
            }
        ));
        assert!(matches!(
            events[2],
            WorkflowEvent::StepFinished {
                instance_id: 7,
                step_idx: 0,
                outcome: StepOutcome::Ok
            }
        ));
        assert!(matches!(
            events[3],
            WorkflowEvent::Finished {
                instance_id: 7,
                ok: true,
                reason: None
            }
        ));
    }
}
