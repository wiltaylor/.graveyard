# ADR-0010 — Idle-detection refactor: situation enum, scope filters, configurable prompts

**Status:** Proposed · **Date:** 2026-05-04 · **Deciders:** arch_consultant, dev_lead

## Context

`tick_idle_timers` at `crates/aciddog/src/tui/app.rs:3324` is the sole idle-detection entry point. Today it selects only top-level chats (`parent_idx.is_none()` at `:3315`), fires on a shared 5-second tick (interval at `:17146`), and hard-codes two actions: Nudge if the leader has at least one non-Hibernated direct child (`has_live_workers` at `:3291`) or Compact otherwise. Its helper predicates are `idle_timeout_for` at `:3277`, `agent_tree_active` at `:3298`, and `is_idle_eligible` at `:3311`; the original planning entry is `decisions/d7e59071` (per-agent `idle_timeout`, leader-vs-leaf split, `AgentStatus::Ready` predicate, refire floor `idle_timeout/2`, 5s tick, opt-in via WCL `idle_timeout` / `default_idle_timeout` at `config.rs:27` and `:82`).

The Foreman automaton (sibling ADR: `foreman-automaton`, source task `0f04ae5b`) needs worker-scoped idle supervision: detect when a Foreman-managed worker is stale and dispatch a configurable prompt. The current `tick_idle_timers` cannot be composed by an outside actor because it is hard-wired to top-level chats only, with no scope filter and no pluggable situation-to-action mapping. Giving Foreman its own idle loop would duplicate the tick infrastructure and split canonical idle behaviour into two parallel systems, the recurring failure mode this ADR explicitly forbids.

The manager-side managed-daemon activity tracking (`decisions/446356bd`, `decisions/8bec11fd`, ADR-0004 Phase 2) is a different idle subsystem that tracks IPC client activity on `crates/aciddog-manager/`. This ADR applies exclusively to the in-process `tick_idle_timers` in `crates/aciddog/src/tui/app.rs`. The two systems share no code and must not be conflated; this slice is documentation only and does not change production Rust code. Non-goals are production Rust changes in this slice and any second idle loop for Foreman.

## Decision

**A — IdleSituation module home**

**Decision (A):** Create a new `crates/aciddog/src/idle/` module. `IdleSituation` and `ConcreteAction` live there, with `pub use idle::IdleSituation` re-exported from `lib.rs` (or via a thin config-layer re-export) so they remain visible at the operator-facing config surface.

Rationale: `config.rs` already carries the WCL parse surface; adding a growing `IdleSituation` enum and its dispatcher logic there couples operator-facing WCL types with runtime dispatch. A dedicated `idle/` module keeps subsystem cohesion, matches the `daemon_runner/` and `automatons/` module conventions, and lets the implementation grow (future variant registrations, tests) without touching `config.rs`.

**B — v1 `IdleSituation` variant set + additive-only policy**

**Decision (B):** The v1 variant set is:
- `NoLiveWorkers` — the leader has zero non-Hibernated direct children (today's Compact branch).
- `LeaderWithLiveWorkers` — the leader has at least one non-Hibernated direct child (today's Nudge branch).
- `WorkerAwaitingResponse` — a Foreman-managed worker is awaiting a model response (no activity since last turn).
- `WorkerNoActivity` — a Foreman-managed worker has had no activity within its configured idle window.

Forward-compat policy: **additive only**. A variant may never be removed or renamed without a superseding ADR. WCL configs in the wild may key `idle_prompts` on any variant; silent removal would break operator configs without warning. New variants may be added in a future ADR; the default fallback (Decision C) handles situations with no configured prompt.

**C — Default-vs-required prompts**

**Decision (C):** Hardcoded per-variant defaults; operator override via WCL `idle_prompts`. If a Foreman (or any scope-filter owner) omits an `idle_prompts` entry for a situation, the built-in default fires. This matches the `decisions/d7e59071` opt-in philosophy and gives operators sensible day-one behaviour without requiring exhaustive configuration. Compile-time errors on missing prompts are operator-hostile for v1 and silently-skip behaviour violates the loud-failure ethos (`decisions/59ff479f`).

**D — Tick frequency**

**Decision (D):** Keep the shared 5-second tick at `app.rs:17146`. The scope filter is evaluated each tick; situation dispatch is per-scope. The effective fire window (5-10s per `decisions/d7e59071`) is adequate for human-time idle detection. Per-scope ticks add scheduler complexity for negligible benefit, and are rejected in Alternatives Considered.

**E — Single-loop invariant (load-bearing)**

**Decision (E):** Foreman (and any future worker-scoped idle consumer) MUST NOT run its own idle loop. It MUST register a `(scope_filter, dispatcher)` pair with the canonical `tick_idle_timers`. This is enforced by reviewer discipline, not by automated tooling; there is no compile-time gate. The Consequences section flags this explicitly.

The refactor shape is pure predicate to situation enum to scope filter. `idle_action_for` classifies an `AgentChat` into an `IdleSituation`, the dispatcher maps that situation to a `ConcreteAction`, and an `IdleScopeFilter` selects which chats each registered consumer owns.

**Refactored type signatures (verbatim — implementation has no design wiggle room):**

```rust
// Pure predicate — no side effects; idempotent given inputs.
pub fn idle_action_for(chat: &AgentChat, threshold: Duration, now: Instant) -> IdleSituation;

// Dispatcher — maps situation to concrete action; carries the operator-supplied prompt.
pub enum ConcreteAction {
    Nudge { prompt: String },
    Compact,
    KickWorker { prompt: String },
    // Extended per the v1 variant set locked above.
}

// Scope filter — pluggable selector ('top-level chats', 'workers of foreman F', ...).
pub trait IdleScopeFilter {
    fn includes(&self, chat: &AgentChat) -> bool;
}
// Alternative: `type IdleScopeFilter = fn(&AgentChat) -> bool;`
// Decision: use the trait form to allow stateful filters (e.g. Foreman holds its worker-id set).
```

**Registration surface:** `tick_idle_timers` accepts a slice of `(Box<dyn IdleScopeFilter>, Box<dyn Fn(IdleSituation) -> Option<ConcreteAction>>)` pairs. The default leader-scope behaviour is the first registered pair; Foreman registers its own pair at startup.

## Consequences

**Positive:**
- `IdleSituation` enum becomes the single source of truth for what "idle" means in aciddog's in-process runtime. Future consumers compose by registering a scope filter; no parallel loops.
- Hardcoded defaults (Decision C) mean operators can adopt Foreman with zero `idle_prompts` configuration and still get sensible behaviour.
- Additive-only variant policy gives operators a stable WCL key surface: `idle_prompts = { NoLiveWorkers = "...", WorkerNoActivity = "..." }` will not silently break across releases.

**Negative / risks:**
- **Developer-discipline gate (E):** There is no automated enforcement of the single-loop invariant. A future contributor could add a second idle loop inside Foreman or another automaton. Reviewer responsibility; the constraint must be called out in PR review checklists.
- **Additive-only policy is permanent.** A v1 variant that proves wrong cannot be renamed or removed without a superseding ADR. Authors must be conservative with variant names (prefer descriptive over terse).
- **`idle_prompts` is a new persisted operator surface.** It composes with the existing `idle_timeout` (config.rs:82) and `default_idle_timeout` (config.rs:27) fields; it does not replace or collide with them. Operators with existing WCL must not be surprised by silent defaults.
- **F9 (from Foreman ADR):** The `run_automaton_core` relocation into `automatons/` (per the `foreman-automaton` ADR) shifts the call site. The `tick_idle_timers` refactor and the `run_automaton_core` move must be coordinated in the implementation follow-up to avoid two concurrent restructurings of `tui/app.rs`.

**Follow-up implementation tasks (post-ADR acceptance):**
1. Create `crates/aciddog/src/idle/` module; define `IdleSituation`, `ConcreteAction`, `IdleScopeFilter`.
2. Refactor `tick_idle_timers` to accept `(scope_filter, dispatcher)` pairs; migrate leader-scope behaviour to the first registered pair.
3. Wire the four `idle_action_for` helper predicates (`idle_timeout_for`, `has_live_workers`, `agent_tree_active`, `is_idle_eligible`) into the new pure-function shape.
4. Implement hardcoded per-variant defaults for Decision C.
5. Add `idle_prompts` WCL block grammar to `config.rs`.
6. Wire Foreman's scope filter + dispatcher registration (depends on `foreman-automaton` ADR acceptance).

## Alternatives Considered

- **Per-scope tick frequency:** Give each scope filter its own tick interval. Rejected per Decision D: adds scheduler complexity (multiple timers, jitter coordination) for negligible benefit. The 5-second shared interval is adequate for human-time idle detection.
- **Compile-time error on missing `idle_prompts` entry:** Require every `IdleSituation` variant to be covered in WCL. Rejected per Decision C: operator-hostile for v1; the hardcoded-default approach gives sensible behaviour without exhaustive configuration.
- **`IdleSituation` in `config.rs`:** Keep the enum in the existing config module (operator-facing, already carries WCL parse surface). Rejected per Decision A: couples runtime dispatch logic to the WCL parse layer; `config.rs` is already large. Trade-off noted: a dedicated module adds one more import path for external consumers, but subsystem cohesion wins.
- **Leave `tick_idle_timers` hard-wired; give Foreman its own idle loop:** Avoid the refactor entirely; Foreman runs a parallel 5-second tick against its worker subset. Rejected per Decision E: the whole point of this ADR. Parallel loops duplicate tick infrastructure, risk clock skew between leader and worker idle checks, and create two canonical sites for "what does aciddog do when an agent is idle", exactly the divergence the single-loop invariant prevents.

## References

- `crates/aciddog/src/tui/app.rs:3277` — `idle_timeout_for`
- `crates/aciddog/src/tui/app.rs:3291` — `has_live_workers`
- `crates/aciddog/src/tui/app.rs:3298` — `agent_tree_active`
- `crates/aciddog/src/tui/app.rs:3311` — `is_idle_eligible`
- `crates/aciddog/src/tui/app.rs:3324` — `tick_idle_timers` (current monolith)
- `crates/aciddog/src/tui/app.rs:17146` — 5-second `idle_check_tick` interval
- `crates/aciddog/src/config.rs:27` — `team.default_idle_timeout`
- `crates/aciddog/src/config.rs:82` — `agent.idle_timeout`
- `decisions/d7e59071` — original `tick_idle_timers` planning entry (leader-vs-leaf split, refire floor, opt-in WCL surface)
- `decisions/446356bd`, `decisions/8bec11fd` — manager-side managed-daemon idle subsystem (ADR-0004 Phase 2; named only to disambiguate)
- ADR `foreman-automaton` (sibling ADR by slug; source task `0f04ae5b`) — worker-scoped idle consumer that motivates the scope-filter contract
- ADR-0007 (`docs/adr/0007-wake-on-request-transport.md`) — precedent shape (Michael Nygard skeleton, cold-reader test)
- `docs/adr/0000-template.md` — section shape source
- `docs/adr/README.md:6-9` — ADR immutability rule
- ADR numbering convention: `decisions/1a764bec`
