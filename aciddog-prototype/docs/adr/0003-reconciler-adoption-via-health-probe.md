# ADR-0003 — Reconciler model with adoption-on-restart via health-probe

**Status:** Accepted
**Date:** 2026-05-03
**Deciders:** dev_lead, quality_consultant

## Context

The `daemon_runner` subsystem must survive manager restarts without killing running daemons or losing track of them. Two related but distinct values:

- `desired_state ∈ {Stopped, Running}` — user intent, persisted in SQLite.
- `observed_state ∈ {Stopped, Starting, Running, Stopping, Crashed}` — derived at runtime from a `connect()` + health probe on the recorded socket plus child-handle status when we own the child.

ADR-0002 excluded `pid_file` from the model because `aciddog daemon` does not write one. Adoption must therefore be socket-based, not PID-based. The manager's existing `Client::Unix` (`crates/aciddog-manager/src/client.rs:67-79`) and health probe (`client.rs:338-361`) are sufficient substrate — no cross-crate change.

## Decision

Run a reconciler task inside the manager that drives observed → desired on a 1 Hz tick and on event triggers (via a `tokio::sync::Notify` per row).

Boot sequence — runs once before the tick loop accepts traffic:

1. For each `managed_daemons` row:
   1. Call `Client::health()` against `socket_path`.
   2. If healthy → **adopt**: write a registry entry marking `observed=Running`, with no child handle. Subsequent shutdown of an adopted daemon uses RPC (an `aciddog` `:webservice` `/v1/shutdown` endpoint, **filed as a follow-up** if it does not already exist; if not available, surface adopted daemons as `externally_supervised=true` in the API and refuse Stop with a structured error pointing the user to `kill <pid>`).
   3. If unhealthy and `desired_state=Running` → call `daemon_runner::spawn()`.
   4. If unhealthy and `desired_state=Stopped` → leave stopped.
2. **Never kill orphans on boot.** Adoption only.

Tick loop — detects drift (a spawned child crashed, an adopted socket stopped responding), updates `observed_state`, optionally retries spawn with backoff if `desired_state=Running` and the last attempt failed. Persists `desired_state` mutations immediately so the next restart sees the intended goal.

`desired_state` is the source of truth. Policy fields (ADR-0004) feed back into `desired_state` mutations under specific conditions; they do not bypass it.

## Consequences

- Positive: manager restarts (cargo cycles, package upgrades, crashes) do not disturb running daemons.
- Positive: a single mental model — "the reconciler moves observed toward desired" — explains every state transition.
- Positive: no PID-file emission required from `aciddog`; this ADR is implementable as a manager-only change.
- Negative: shutdown of adopted daemons depends on an RPC the daemon may not yet expose. The adoption path documents the fallback (read-only adoption + manual kill) so this ADR is not blocked on `aciddog` core.
- Negative: a `connect()` succeeding without `/v1/health` succeeding is ambiguous (TCP/socket up, app not ready). The probe must use the health endpoint, not just `connect`.

## Alternatives Considered

- *Persist `observed_state` and trust it across restarts.* Rejected: it is a derived value; persisting it creates "lying database" bugs.
- *Kill orphans on boot for a clean slate.* Rejected: data-loss-by-default. Surfaces to users as "the manager just murdered my live agents because I restarted it".
- *PID-file-based adoption (originally proposed).* Rejected after red-team review: requires a cross-crate change to `aciddog` and is a worse signal than the health probe (a stale PID file is indistinguishable from "starting" without the probe anyway).

## References

- `crates/aciddog-manager/src/client.rs:67-79,338-361` (Unix dialler + health probe).
- `crates/aciddog/src/daemon.rs:1-139` (no PID file).
- ADR-0002 (model excludes `pid_file`), ADR-0004 (lifecycle policy precedence).
