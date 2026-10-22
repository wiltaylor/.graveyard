# ADR-0011 — Multi-owner WorkflowEngine with concurrency caps and filtered events

**Status:** Proposed · **Date:** 2026-05-04 · **Deciders:** code_reviewer#5099eb3b, arch_consultant, dev_lead

## Context

Current engine shape: `WorkflowEngine` is an in-memory runtime, not a persistent workflow database:

```rust
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
```

The engine has a single shared event sink:

```rust
    pub event_tx: mpsc::UnboundedSender<WorkflowEvent>,
```

The current start API has no owner concept and starts work directly against that shared sink:

```rust
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
```

The completion event is the natural cap-decrement seam because the engine emits exactly one `Finished` per instance:

```rust
    Finished {
        instance_id: u64,
        ok: bool,
        reason: Option<String>,
    },
```

Findings from arch-lens review by `code_reviewer#5099eb3b`, 2026-05-04:

- **F3 (major):** `start_instance` has no owner concept, no per-owner concurrency cap, and a single shared `event_tx` — Foreman cannot filter its observations or enforce a cap without racing against `WorkflowEvent::Finished` emit ordering.
- **F7 (minor):** Concurrency cap belongs in the engine, not in the automaton/caller, for the same emit-ordering reason.

Three legacy call sites must keep working unchanged:

- `crates/aciddog/src/tui/app.rs:4538` — `handle_workflow_run` (IPC entry)
- `crates/aciddog/src/tui/app.rs:10401` — `start_workflow_with_defaults` (CLI `/run <wf>` + form Run button)
- `crates/aciddog/src/tui/app.rs:11000` — `submit_workflow_form` (TUI workflow form direct submission)

These become default unbounded owner pass-throughs after the additive extension.

## Decision

**Proposed additive API (no breaking changes):**

```rust
WorkflowEngine::register_owner(owner_id: OwnerId, max_in_flight: Option<usize>) -> OwnerHandle
OwnerHandle::try_start(workflow: &str, vars: Vars) -> Result<InstanceId, AtCap>
OwnerHandle::events() -> impl Stream<Item = WorkflowEvent>  // filtered to this owner
```

**A — Owner-id type: `OwnerId` newtype**

**Decision (A):** Use `pub struct OwnerId(String);` as the owner identifier type.

A bare `String` loses type-system separation between owner identity and other string parameters at the `register_owner` API boundary. Reusing `AgentId` conflates the workflow-engine concept with the agent-tree concept — these are distinct abstractions that happen to overlap for Foreman but will diverge for future consumers. A newtype is the cheapest available type-safety win. Note: `OwnerId` would be the first newtype-wrapping-`String` in this crate (no `pub struct *Id(String)` precedent found in a code search); this ADR accepts that cost and expects future PRs to follow the pattern where it adds value.

**B — At-cap behaviour: `Err(AtCap)` reject**

**Decision (B):** `OwnerHandle::try_start` returns `Err(AtCap)` when the owner's `max_in_flight` is reached.

Engine-managed queueing introduces unbounded memory growth and cancellation semantics that no current caller has requested. Leaving the policy to the caller via `Err(AtCap)` is the simplest predictable contract. The caller (Foreman or any future owner) implements its own back-off, retry, or drop policy. This provides a clean seam for a future "engine-managed bounded queue" ADR if demand materialises.

**C — Event subscription: fan-out via `tokio::sync::broadcast`**

**Decision (C):** `OwnerHandle::events()` returns a filtered stream backed by `tokio::sync::broadcast`.

Single-subscriber forces the owner to be the sole observer; TUI forms, automaton supervisors, and debugging surfaces all benefit from independent subscription without coordination. `tokio::sync::broadcast` is already used in `crates/aciddog/src/tui/app.rs:26745` for `AgentStatusEvent` fan-out, so this follows an existing in-crate pattern. Every subscriber must handle `RecvError::Lagged` — slow consumers may miss events between catch-up windows; broadcast channel capacity is an implementation-task decision.

**D — Engine topology: one engine, multiple owners**

**Decision (D):** Extend the single shared `WorkflowEngine` with an owner registry; do not create per-owner engine instances.

Per-owner engines fragment workflow definitions — every owner re-registers the same workflow set. The IPC handler at `app.rs:4538` already dispatches against a single engine reference; sharding it would require either a dispatch table or duplicated registration. One engine + multiple owners is the natural additive shape.

**Cap placement and emit ordering (F7 load-bearing rationale)**

The cap MUST live in the engine because the engine is the sole emitter of `WorkflowEvent::Finished`. If the cap lived in the automaton or any external caller, decrement logic would subscribe to events and race against the engine's emit ordering — a window exists between a running instance completing and the observer decrementing its counter, during which the cap could be overcounted. Workflow instances are **non-persistent** (see `mod.rs:1-11` quoted in Context) so there is no DB-locality argument for external cap placement. The `Finished` variant (one per instance, exactly) is the designated cap-decrement seam.

**Additive API and legacy-caller contract**

The three legacy callers (`handle_workflow_run` at `app.rs:4538`, `start_workflow_with_defaults` at `app.rs:10401`, `submit_workflow_form` at `app.rs:11000`) are preserved unchanged through a **default unbounded owner** (`max_in_flight: None`). Observable behaviour is identical post-change.

Event-routing mechanism (locked): the engine dual-emits to BOTH the existing legacy `event_tx: mpsc::UnboundedSender<WorkflowEvent>` (unchanged semantics) AND the per-owner broadcast channel. The legacy mpsc is NOT replaced or repurposed; it continues to receive all events as before. Per-owner broadcast channels receive only events tagged with their `OwnerId`. This forbids any change to the legacy `event_tx` semantics on the legacy path.

## Consequences

**Positive:**

- Foreman (and any future automaton) can enforce a per-workflow-owner cap without coordinating with the engine at the call site.
- Each owner gets a filtered event stream independent of other owners — no coordination or demultiplexing required at the subscriber.
- Legacy callers continue to work unchanged; this is a purely additive API extension.

**Negative / risks:**

- **At-cap caller responsibility.** Owners hitting `Err(AtCap)` must implement their own back-off, retry, or drop policy. There is no engine-side queue. If Foreman needs bounded queuing, that is a new ADR seam ("engine-managed bounded queue").
- **Lagged subscribers.** `tokio::sync::broadcast` subscribers may miss events if they fall behind channel capacity. Broadcast capacity sizing is an implementation-task decision — the ADR does not constrain it.
- **`OwnerId` is the first newtype-wrapping-`String` in the crate.** Future PRs may follow or diverge; this ADR only establishes the pattern at one boundary.
- **Dual-emit complexity.** The engine must emit to both the legacy mpsc and per-owner broadcast channels on every `WorkflowEvent`. The implementation must ensure neither channel blocks the other — the legacy mpsc is unbounded so it cannot block; broadcast sends are non-blocking (lagged subscribers are dropped, not blocked).
- **Implementation is deferred.** No production Rust lands in this slice. A follow-up task implements `OwnerId`, `OwnerHandle`, `AtCap`, the broadcast fan-out, and the dual-emit wiring.

## Alternatives Considered

- **Bare `String` owner-id:** Loses type-system separation between owner identity and other string parameters at the `register_owner` call site. Rejected in favour of the `OwnerId` newtype.
- **`AgentId` reuse:** Would conflate the workflow-engine owner concept with the agent-tree identifier concept. These overlap for Foreman but diverge for future non-agent workflow consumers. Rejected.
- **Engine-managed queueing on `AtCap`:** Introduces unbounded memory growth and cancellation semantics; no current caller has requested this behaviour. Rejected — `Err(AtCap)` + caller policy is simpler and provides a clean seam for a future bounded-queue ADR.
- **Single subscriber per owner:** Forces the owner to be the sole observer of its workflow events. TUI forms and debugging surfaces would require explicit delegation from the owner. Rejected in favour of broadcast fan-out.
- **Per-owner engine topology:** Each owner gets its own `WorkflowEngine` instance. Fragments workflow definitions — every owner re-registers the same workflow set. Complicates the IPC dispatch handler. Rejected — one engine + multiple owners is the natural additive shape.
- **Cap placement in the automaton/caller (F7):** The decrement would subscribe to events and race against the engine's `WorkflowEvent::Finished` emit ordering. No DB-locality argument applies (instances are non-persistent). Rejected — cap lives in the engine as the sole `Finished` emitter.

## References

- `crates/aciddog/src/workflow/mod.rs:1-11` — non-persistence module comment (quoted verbatim in Context)
- `crates/aciddog/src/workflow/mod.rs:55` — `event_tx: mpsc::UnboundedSender<WorkflowEvent>` (the F3 single-shared-sink)
- `crates/aciddog/src/workflow/mod.rs:110-138` — `start_instance` (current API being extended)
- `crates/aciddog/src/workflow/event.rs:17-66` — `WorkflowEvent` enum; `Finished` at `:61-65` (cap-decrement seam)
- `crates/aciddog/src/tui/app.rs:4538` — `handle_workflow_run` (legacy caller 1)
- `crates/aciddog/src/tui/app.rs:10401` — `start_workflow_with_defaults` (legacy caller 2)
- `crates/aciddog/src/tui/app.rs:11000` — `submit_workflow_form` (legacy caller 3)
- `crates/aciddog/src/tui/app.rs:26745` — existing `tokio::sync::broadcast` precedent for `AgentStatusEvent` fan-out
- Sibling ADR (by slug): `foreman-automaton` — motivating consumer of the owner API (source task `0f04ae5b`)
- Sibling ADR (by slug): `idle-detection-refactor` — sibling subsystem from the same arch-lens review (source task `03f07242`)
- Arch-lens review: `code_reviewer#5099eb3b`, 2026-05-04 (F3 major, F7 minor)
- ADR numbering convention: `decisions/1a764bec`
- ADR immutability policy: `docs/adr/README.md:5-9`
- `docs/adr/0000-template.md` — section shape source
