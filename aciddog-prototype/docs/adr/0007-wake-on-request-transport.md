# ADR-0007 — Wake-on-request via daemon-canonical socket + reconciler-coordinated retry

**Status:** Proposed · **Date:** 2026-05-04 · **Deciders:** dev_lead, arch_consultant

## Context

When `start_on_demand=true` and `observed=Stopped`, an inbound request reaches the manager's `Client::Unix` (client.rs:67-79). The dial path (`uds_request` at client.rs:454-461) calls `UnixStream::connect(path)` with **no retry, no timeout, no backoff**; with no listener bound it returns `ENOENT` immediately wrapped in an anyhow context string. ADR-0004 deferred the wake-mechanics decision to this spike. Constraints inherited from prior ADRs: ADR-0001 (`daemon_runner` is in-process), ADR-0003 (socket-based adoption, no PID file), ADR-0004 (sticky `desired_state`; `idle_timeout` is the companion policy), and ADR-0006 (operators may run `aciddog daemon` against a manager-scaffolded WCL out-of-band; the imported provenance class is operator-controlled).

Four options were evaluated:
- **A.** Manager binds the listener and proxies to a daemon bound on a different path.
- **B.** Daemon stays canonical; manager pokes the reconciler and waits on a per-row `Notify` keyed by `managed_daemon_id`, then retries the dial.
- **C.** Hybrid: manager owns a separate "wake socket"; first connect triggers spawn, then the client retries against the real daemon socket.
- **D.** Punt; ship `start_on_demand` schema-only, return `503` always.

## Decision

Adopt **Option B**. The daemon's `:webservice` Unix socket remains the single canonical bind path, preserving the ADR-0003 adoption contract and the ADR-0006 imported-daemon path; the manager handles cold-dial by kicking the reconciler and waiting on a coalescing per-row `Notify`. **This decision supersedes the deferred wake clause in ADR-0004; ADR-0004 is not edited (immutability rule from `docs/adr/README.md`).**

| Axis | A — manager owns listener + proxy | B — daemon canonical + Notify-coordinated retry | C — hybrid wake socket | D — punt; manual-start only |
|---|---|---|---|---|
| Latency overhead per first request | 3/5 — proxy adds a second Unix-socket hop on every request once warm, on top of the existing `Client::Unix` dial at client.rs:67-79. | 3/5 — cold path adds reconciler `wake(id)` plus per-row `Notify` wait (≤ spawn + ADR-0003's 5 s health-probe budget); warm path is the single dial at client.rs:67-79. | 2/5 — wake-socket adds a connect-then-retry against a separate path on every cold request before reaching the daemon's `serve_unix` (http.rs:453-485). | 5/5 — `Client::Unix` returns the existing `503` immediately; no transport change at all. |
| Race-with-CLI-aciddog risk | 1/5 — both manager and operator-launched `aciddog daemon` follow http.rs:453-485 connect-then-bind, so bind ordering decides the loser and ADR-0006 imported-row verification breaks on mismatched canonical paths. | 5/5 — daemon remains the canonical bind owner; ADR-0006 imported-daemon flow is structurally untouched. | 4/5 — daemon-side bind path is unchanged so the wake-socket path cannot collide with `aciddog daemon`; only the wake socket itself is manager-owned. | 5/5 — no manager-owned listener and no spawn attempt, so there is no race surface against `aciddog daemon`. |
| Code complexity | 2/5 — adds a proxy hop, listener-management, and a second transport mode inside `Client::Unix` (client.rs:67-79). | 4/5 — adds one `wake(id)` reconciler entry-point, one per-row `Notify`, and a single retry seam in client.rs:67-79. | 2/5 — two listener paths plus drop-on-spawn semantics inside the manager, with client-side retry against a different path. | 5/5 — schema-only; no code paths to maintain. |
| Coupling to daemon_runner internals | 2/5 — manager would own the daemon's listener semantics, breaching ADR-0001's in-process boundary for `daemon_runner`. | 4/5 — reuses the `ReconcilerNotify` landing via d67a4d0c (decisions/a4b86852); only the new `wake(id)` entry-point crosses the seam. | 2/5 — wake-socket lifecycle has to know reconciler spawn timing to drop the wake socket and hand off to `serve_unix` (http.rs:453-485). | 5/5 — no implementation, so no coupling. |
| Future cost of removing if we swap | 2/5 — proxy hop and second-path semantics live deep in the dial path and are hard to unwind without re-architecting `Client::Unix`. | 5/5 — the retry seam in client.rs:67-79 plus the `wake()` call are deletable in a single diff if we ever swap transports. | 3/5 — wake-socket bind/drop is removable but the client-side retry-against-a-different-path also has to be reverted. | 5/5 — nothing was added, so nothing is removable. |
| **Total** | **10** | **21** | **13** | **25** |

Mechanism (≤6 bullets, scoping-only — implementation lives in d67a4d0c):
- On `ENOENT`/`ECONNREFUSED` in `Client::Unix`, look up the `managed_daemons` row by `socket_path`.
- If `start_on_demand=true`, call `reconciler.wake(managed_daemon_id)` — sets `desired=Running` and pokes the existing `ReconcilerNotify` (introduced by sibling d67a4d0c per decisions/a4b86852).
- Wait on a per-row `Notify` until either `observed=Running` or a configurable max-wait elapses; on timeout return `503` with the existing structured `desired_state=stopped` body.
- Reconciler signals the per-row `Notify` from its post-spawn health-probe success path; multiple in-flight requests share one wake.
- If `start_on_demand=false`, return `503` immediately — unchanged from ADR-0004.
- Subsequent requests during `observed=Starting` join the same wait — no proxy hop, no second socket, no new transport variant.

Non-goals:
- `503` behaviour when `start_on_demand=false` is unchanged from ADR-0004 (no `desired_state` mutation, no wake).
- Idle-timeout policy (the *companion* of `start_on_demand`) is owned by sibling task edeb7d11; this ADR does not define or modify timeout semantics.
- No PID-file emission — ADR-0003 stands; adoption remains health-probe based.

## Consequences

- Positive: zero changes to the daemon side — `serve_unix` (http.rs:453-485) and ADR-0006's bind contract are untouched; the imported provenance class works without modification.
- Positive: piggy-backs on `ReconcilerNotify` already landing via d67a4d0c; the additional surface is one `wake(id)` reconciler entry-point, one per-row `Notify`, and a retry seam inside `Client::Unix`.
- Negative: cold-dial latency = spawn time + 5 s health-probe budget (per ADR-0003) + caller's max-wait ceiling; this is the floor unless the daemon shortens its health-probe.
- Negative: residual race — if the operator runs `aciddog daemon` between `wake()` and the reconciler's spawn attempt, the operator's bind wins and the reconciler adopts on the next health probe; the request retries succeed but the registry briefly disagrees on provenance until the next adopt pass.
- Follow-up (file against d67a4d0c, not this ADR): reconciler must export `wake(managed_daemon_id) -> Result<(), WakeError>` and a `wait_for_running(id, max_wait)` future; `Client::Unix` grows a one-shot retry seam keyed on `managed_by_id`.

## Alternatives Considered

- **A — manager-owned listener + proxy.** Killed by race-with-CLI-aciddog: both manager and operator-launched `aciddog daemon` apply the same connect-then-bind logic (http.rs:453-485) so bind ordering decides the loser, and ADR-0006's imported-row verification breaks at canonical-path matching — see matrix above for per-axis breakdown.
- **C — hybrid wake socket.** Killed by code-complexity and latency: two listener paths plus drop-on-spawn semantics — strictly worse than B on every other axis while preserving the same canonical-socket invariant.
- **D — closure / punt.** Mechanically scored 25/25 but rejected on intent: ADR-0004 explicitly forecast successor mechanics for the `cold + lazy-wake` and `pinned-but-sleeps-overnight` deployment shapes, so punting renders those user-visible promises aspirational.

## References

- `crates/aciddog-manager/src/client.rs:67-79` (current `Transport::Unix` arm; no retry/timeout/backoff).
- `crates/aciddog/src/webservice/http.rs:453-485` (`serve_unix` connect-then-bind collision check).
- ADR-0001 (`daemon_runner` in-process), ADR-0003 (socket-based adoption), ADR-0004 (deferred wake clause, superseded by this ADR), ADR-0006 (manager-scaffolded vs imported provenance).
- Sibling tasks: 799b7b92 (`daemon_runner` spawn pipeline; decisions/434cdf5b), d67a4d0c (reconciler + `ReconcilerNotify`; decisions/a4b86852), edeb7d11 (idle-timeout companion policy); decisions/b59623f3 (this spike's planning entry).
