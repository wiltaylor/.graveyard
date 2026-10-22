# ADR-0009 — Foreman built-in automaton

**Status:** Proposed · **Date:** 2026-05-04 · **Deciders:** arch_consultant, dev_lead

## Context

Foreman is a built-in automaton that dispatches workers from a group's task queue, subject to a configurable concurrency cap (`max_concurrent`) and idle-supervision (`idle_prompts`). Unlike Script/Webservice/Remote automatons, Foreman is an in-process behaviour rather than a transport: it has no shell hop.

The arch-lens review by `code_reviewer#5099eb3b` on 2026-05-04 identified five findings (F1/F2/F5/F8/F9) that together define Foreman. This ADR locks all five. It is the foundational ADR of a three-ADR set; the sibling ADRs are `workflow-multi-owner` (workflow dispatch + per-owner cap) and `idle-detection-refactor` (`IdleSituation` enum + scope filter Foreman consumes for worker-scoped idle prompts). Cross-references use topic slugs, not numbers, because the numbering race is resolved at merge.

The existing code bounds the design. `AutomatonBackend` at `crates/aciddog/src/config.rs:236` currently has three transport variants (`Script`, `Webservice`, `Remote`). `handle_worker_create` at `crates/aciddog/src/tui/app.rs:4628` is the worker-creation entry point; it touches 10+ `App` fields directly: `self.chats`, `self.config` (sub-paths `.agents`, `.can_create`, `.worker()`, `.effective_ai_core`, `.effective_tools`, `.effective_model`, `.thinking_messages`), `self.ai_tx`, `self.aciddog_binary`, `self.tasks`, `self.socket_path`, `self.socket_events_tx`, `self.chat_block_tx`, `self.agent_status_tx`, `self.agent_ids`. Foreman cannot reach this surface cleanly from outside `App`. `Automaton.leader` at `crates/aciddog/src/config.rs:221-225` is currently documented as `"Same semantics as [Agent::leader] — pins the automaton to the top of the agent list and decorates it with 👑 in the TUI / web UI. Purely cosmetic."` `run_automaton_core` lives inside `crates/aciddog/src/tui/app.rs`, called from `crates/aciddog/src/tui/boot.rs:517` via `use super::app::{... run_automaton_core ...}`.

## Decision

**F1 — AutomatonBackend variant**

**Decision (F1):** Add `AutomatonBackend::Foreman { ... }` as a direct enum variant alongside the three existing transports.

Foreman is the first in-process built-in automaton; there is no second. Pre-collapsing into `Builtin { kind, params }` adds abstraction before abstraction is earned; it hurts reviewer comprehension now for a refactor that is one variant-rename away if/when a second built-in materialises. The inflection point is explicit:

Before: `AutomatonBackend::Script | ::Webservice | ::Remote`
After: `AutomatonBackend::Script | ::Webservice | ::Remote | ::Foreman { ... }`

**F2 — WorkerDispatcher mpsc extraction**

**Decision (F2):** Extract a `WorkerDispatcher` mpsc handle that Foreman sends spawn requests over, mirroring the existing `chat_block_tx` / `task_event_tx` / `agent_status_tx` pattern visible in `handle_worker_create`.

The extraction is required because the worker-creation surface touches `self.chats`, `self.config` (sub-paths `.agents`, `.can_create`, `.worker()`, `.effective_ai_core`, `.effective_tools`, `.effective_model`, `.thinking_messages`), `self.ai_tx`, `self.aciddog_binary`, `self.tasks`, `self.socket_path`, `self.socket_events_tx`, `self.chat_block_tx`, `self.agent_status_tx`, and `self.agent_ids`. Factor allowlist/cwd/template validation into a **free function** shared between the `WorkerDispatcher` consumer and the existing IPC `RequestCmd::WorkerCreate` handlers at `app.rs:3938` and `app.rs:5153` (and the test sites at `app.rs:21000+` and `agent_ipc/mod.rs:870+`).

> **IPC-handler sharing constraint (load-bearing):** The validation free function MUST enforce identical rules regardless of caller — Foreman over the channel, or an external process over IPC. Divergence between the in-process and over-IPC validation paths is the recurring failure mode this extraction must prevent. The implementation task MUST audit all four call sites before shipping.

**F5 — Leader semantics**

**Decision (F5):** Operationalise `Automaton.leader` for Foreman.

When any group member has a leader-aware backend (Foreman is the first), `leader = true` is required on that member and a second leader in the same group is forbidden. Update the rustdoc at `config.rs:221-225` from:

> `"Same semantics as [Agent::leader] — pins the automaton to the top of the agent list and decorates it with 👑 in the TUI / web UI. Purely cosmetic."`

To: `"backend-dependent — see Foreman ADR"`

**Backwards-compat note:** Existing teams whose group contains zero leader-aware automatons continue to see `leader` as cosmetic. The contract activates only on backend opt-in. No silent breakage of existing configs.

**F8 — Name**

**Decision (F8):** Name the automaton `Foreman`.

WCL symbol: `:foreman`. The name continues the team-of-people metaphor (`Agent → Worker → Automaton → Foreman`) and accurately describes the role: a site supervisor who dispatches workers, tracks their progress, and idles them when they stall.

Three alternatives were rejected: `TaskQueue` (collides with the existing `TaskStore`, names a data structure where an actor is needed), `Scheduler` (generic CS jargon, no metaphor coherence), `Dispatcher` (already overloaded: F2's mpsc handle is named `WorkerDispatcher`).

`:foreman` lands in operator WCL configs and is **permanent**.

**F9 — Module home**

**Decision (F9):** Create `crates/aciddog/src/automatons/` and land `foreman.rs` there.

Relocate `run_automaton_core` out of `tui/app.rs` into this module; it lives there today and is the natural fit for a module that owns built-in automaton runtimes. The import path at `tui/boot.rs:33,517` shifts; the implementation follow-up must update it. This sets the convention for future built-ins: the next built-in lands as `automatons/<name>.rs`.

**WCL surface (verbatim, locked by this ADR):**

```
automaton my_lead {
  type = :foreman,
  group = "engineering",
  workflow = "build_and_test",
  max_concurrent = 4,
  idle_prompts = { ... }
}
```

Field spellings: `type` and `group` already exist on `Automaton` in `config.rs`. `workflow`, `max_concurrent`, and `idle_prompts` are new and live inside the `Foreman` variant payload. The `idle_prompts` map keys are `IdleSituation` variants, defined by the sibling `idle-detection-refactor` ADR (cross-referenced by slug).

## Consequences

**Positive:**
- Built-in actor with no script-shell hop: Foreman runs in-process, with lower latency and tighter error semantics than a Script automaton.
- Clean Foreman-to-worker-spawn seam (`WorkerDispatcher`) reusable by any future in-process actor.
- Leader contract gives each group a single accountable supervisor; the group model gains structural clarity.

**Negative / risks:**
- F2 extraction is a non-trivial refactor. The free-function validation constraint must be maintained whenever either of the four call sites (Foreman channel, IPC `RequestCmd::WorkerCreate` at `app.rs:3938`, `app.rs:5153`, test sites) is modified. Divergence is the recurring failure mode; there is no automated enforcement, only reviewer responsibility.
- F5 is a backwards-compat shift that is silent for groups without a leader-aware backend. Operators adding Foreman to an existing group with `leader = false` will encounter a new validation error; this must be surfaced clearly in error messages.
- F9 relocation breaks the `run_automaton_core` import path at `tui/boot.rs:33`; mechanical, but the implementation follow-up must update it atomically with the move.

**Follow-up implementation tasks (post-ADR acceptance):**
1. Implement `AutomatonBackend::Foreman { workflow, max_concurrent, idle_prompts }` variant + WCL parse.
2. Extract `WorkerDispatcher` mpsc channel; factor validation free function; audit all four call sites.
3. Enforce leader contract at config-load: require `leader = true` on Foreman member, reject second leader in group.
4. Create `crates/aciddog/src/automatons/` module; move `run_automaton_core`; update `tui/boot.rs:33,517`.
5. Wire Foreman runtime: task-queue drain loop, `max_concurrent` cap, reconcile with idle-detection-refactor scope filter.
6. Implement `idle_prompts` dispatch (depends on `idle-detection-refactor` ADR acceptance).

**Non-goals for this ADR slice:**
- No production code changes land with this ADR; implementation follows only after ADR acceptance.
- No `Builtin { kind, params }` generic is introduced until a second built-in automaton materialises.

## Alternatives Considered

- **F1 — `Builtin { kind, params }` generic variant:** Pre-genericise `AutomatonBackend` before a second built-in exists. Rejected: no second built-in is in scope; the named variant reads cleaner, and the refactor to `Builtin` is one rename away if/when needed.
- **F2 — Pass `&mut App` into Foreman directly:** Skip the mpsc handle; give Foreman a mutable App reference. Rejected: Foreman would need a second mpsc seam for cap-respecting dispatch anyway (the cap state must outlive the App borrow); the handle pattern is already idiomatic in the codebase (`chat_block_tx`, `task_event_tx`, `agent_status_tx`).
- **F8 — `TaskQueue`:** Names a data structure, not an actor; collides conceptually with the existing `TaskStore`. Rejected.
- **F8 — `Scheduler`:** Generic CS jargon; breaks the team-of-people metaphor. Rejected.
- **F8 — `Dispatcher`:** Already overloaded by F2's `WorkerDispatcher` channel handle. Rejected.
- **F9 — Leave `run_automaton_core` in `tui/app.rs` and just add `automatons/foreman.rs`:** Avoids the import-path churn at boot.rs. Rejected: the convention only sets cleanly if the existing in-tree built-in moves to `automatons/` too; leaving it split creates two canonical homes for built-in automaton code.

## References

- Sibling ADR (by slug): `workflow-multi-owner` — workflow dispatch + per-owner cap (source task `3af2e235`).
- Sibling ADR (by slug): `idle-detection-refactor` — `IdleSituation` enum + scope filter Foreman consumes (source task `03f07242`).
- `crates/aciddog/src/config.rs:221-225` — `Automaton.leader` rustdoc (current text: `"Purely cosmetic"`).
- `crates/aciddog/src/config.rs:236` — `AutomatonBackend` enum (three transport variants).
- `crates/aciddog/src/tui/app.rs:4628` — `handle_worker_create` (10+ `App` field surface).
- `crates/aciddog/src/tui/app.rs:3938`, `:5153` — IPC `RequestCmd::WorkerCreate` handlers.
- `crates/aciddog/src/tui/boot.rs:33,517` — `run_automaton_core` caller (import path shifts on F9 move).
- Arch-lens review: `code_reviewer#5099eb3b`, 2026-05-04.
- ADR numbering convention: `decisions/1a764bec`.
- ADR immutability policy: `docs/adr/README.md:6-9`.
- `docs/adr/0000-template.md` — section shape source.
