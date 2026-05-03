# ADR-0004 — Per-daemon lifecycle policy: start_on_manager_boot, idle_timeout, start_on_demand; sticky desired_state precedence; wake mechanics deferred

**Status:** Accepted
**Date:** 2026-05-03
**Deciders:** dev_lead, quality_consultant

## Context

Not every managed daemon is always-on. Real deployment shapes span "always-up production team", "dev team I spin up by hand", and "cold team that wakes on first request". A single `autostart` flag conflates three independent concerns: boot behaviour, idle behaviour, and request behaviour.

Red-team review also surfaced that the wake-on-request mechanics are a transport-layer change (`Client::Unix` dials an existing socket; nothing routes connect-attempts during boot), not a policy-layer change. The policy field is cheap; the implementation is its own design problem.

## Decision

Three orthogonal policy fields per `managed_daemons` row:

- `start_on_manager_boot: bool` — on manager startup, if `observed=Stopped` and no prior `desired_state` is recorded for this row (first boot for the row, or row was created since last manager start), set `desired=Running`.
- `idle_timeout: Option<Duration>` — when `Some(d)`, the reconciler stops the daemon after `d` of zero observed activity. `None` = never auto-stop.
- `start_on_demand: bool` — when `true`, an inbound IPC request hitting an `observed=Stopped` daemon sets `desired=Running` and waits for the socket up to a configurable max-wait, then forwards. When `false`, returns `503` with a structured `desired_state=stopped` body so the UI can prompt.

**Precedence rule (sticky desired_state):** `desired_state` is sticky. Once a user clicks Stop, subsequent manager restarts honour Stop and ignore `start_on_manager_boot`. `start_on_manager_boot` only fires when `desired_state` is null (first boot for that row) **or** when the last persisted `desired_state` was `Running`. This prevents boot-autostart from fighting explicit user intent.

**Activity definition (for `idle_timeout`):** any inbound IPC request the manager forwards to that daemon's socket. Tracked at one chokepoint in `crates/aciddog-manager/src/client.rs`, **not** scattered across routes. No daemon-side cooperation required. Schema is forward-compatible if daemon-emitted activity pulses are added later.

**Wake-on-request mechanics: deferred.** This ADR locks in the policy field `start_on_demand: bool` and its `503` behaviour when off. The transport story when on — does the manager own the listener socket, retry against the daemon's socket, or run a hybrid wake-socket — is **not** decided here; it is the subject of a separate spike (intake task: "Spike: wake-on-request prototype for managed daemons"). MVP ships with `start_on_demand=true` returning the same `503` until that spike resolves and a successor ADR (likely ADR-0007) supersedes this clause.

## Consequences

- Positive: each deployment shape maps cleanly:
  - Always-on production team: `boot=true, idle=None, demand=false`.
  - Dev team: `boot=false, idle=Some(30m), demand=false`.
  - Cold + lazy-wake: `boot=false, idle=Some(15m), demand=true`.
  - Pinned during a session, sleeps overnight: `boot=true, idle=Some(8h), demand=true`.
- Positive: policy and mechanics are decoupled — we can ship the field today and the listener strategy when we are sure of it.
- Positive: idle-timeout activity tracking lives in one instrumented chokepoint; cannot rot via duplication.
- Negative: three knobs is a documentation surface. Mitigated by sensible defaults (`true / None / false` for the everyday case) and by the matrix above.
- Negative: until the wake spike lands, `start_on_demand=true` and `start_on_demand=false` behave identically (both `503`). Acceptable for MVP; UI must not promise wake-on-request before the spike closes.

## Alternatives Considered

- *Single `autostart` enum `{Off | OnBoot | OnDemand | Always}`.* Rejected: muddles boot-vs-demand (they are independent — a daemon may have both) and offers nowhere to attach `idle_timeout`.
- *Daemon-side idle detection (the daemon shuts itself down).* Rejected for v1: requires reverse coupling, and makes the manager's lifecycle DB lie about who owns the decision.
- *Implementing wake mechanics inline with the policy field.* Rejected after red-team: the transport refactor is bigger than the policy plumbing; conflating them risks shipping a half-implemented surface.

## References

- `crates/aciddog-manager/src/client.rs:67-79,107-118` (current Unix dialler — does not own a listener).
- ADR-0003 (reconciler is what consumes these fields).
- Spike intake task: "Spike: wake-on-request prototype for managed daemons".
