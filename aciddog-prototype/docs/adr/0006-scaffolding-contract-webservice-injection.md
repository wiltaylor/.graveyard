# ADR-0006 — Scaffolding contract: manager-injected :webservice automaton for managed daemons

**Status:** Accepted
**Date:** 2026-05-03
**Deciders:** dev_lead, quality_consultant

## Context

The socket the manager dials is bound by the daemon, not by the manager. In `aciddog`, a `:webservice` automaton's bind path is declared in the operator's WCL — `automaton X { type = :webservice ... bind { kind = :unix-socket; socket_path = "..." } }` — and the daemon parses and binds it at startup (`crates/aciddog/src/config.rs:295-305,1851-1951`; `crates/aciddog/src/webservice/mod.rs:209-225`).

This means `managed_daemons.socket_path` is **not** independently authoritative. If the operator's WCL says one thing and the row says another, the paired `instances` row dials a socket nothing is listening on.

Compounding this: the current `aciddog init` template (`init-team/aciddog.wcl:1-58`) ships **no** `:webservice` automaton at all. Freshly-scaffolded teams cannot be talked to by the manager until manually edited. (A separate intake task, "Add `:webservice` automaton to `init-team/aciddog.wcl` scaffold template", addresses the template; this ADR concerns the manager's contract on top of it.)

## Decision

`managed_daemons` are split into two provenance classes, with different contracts:

**Manager-scaffolded** (created via `POST /api/managed-daemons`):

1. Manager picks `work_dir = <teams_root>/<sanitised-name>/`.
2. Manager picks `socket_path = work_dir/.aciddog/daemon.sock`.
3. Manager invokes `aciddog init <work_dir>` as a subprocess (so the template stays the single source of truth for the skeleton).
4. Manager reads back the resulting `aciddog.wcl`. If the template's `:webservice` block already targets the standard `socket_path`, normalise/verify and finish. Otherwise, inject or rewrite the block to match `managed_daemons.socket_path`.
5. Manager dry-validates the resulting WCL. Mechanism is deferred to a spike ("Spike: aciddog-config library extraction vs `aciddog check` subcommand"). Until that spike resolves, validation falls back to first-start: the row is inserted with `desired_state=Stopped`; clicking Start surfaces parse errors via the daemon's exit. **This fallback is allowed for MVP and is not a blocker.**
6. Both rows (`managed_daemons` + paired `instances`) inserted in one sqlx transaction (ADR-0002).

For manager-scaffolded daemons, `managed_daemons.socket_path` **is** canonical because the manager wrote the WCL.

**Imported** (created via a future `POST /api/managed-daemons/import` or by registering an existing `work_dir`):

1. User supplies `work_dir` and `socket_path`.
2. Manager does not modify the WCL at all.
3. On first `start`, the reconciler waits for the socket, runs `Client::health()`, **and verifies that the daemon's reported `:webservice` bind path matches `managed_daemons.socket_path`** (mechanism: a new field on the health response, or an introspection RPC — to be specified during implementation; if neither exists, mark observed=Running on socket-connect and emit a warning for the imported case).
4. If the bind does not match, the reconciler refuses to mark `observed=Running` and surfaces a structured error pointing at the WCL.

For imported daemons, the WCL stays canonical; the row is verified against it.

## Consequences

- Positive: closes the silent-mismatch failure mode where a row dials a socket nothing serves.
- Positive: manager-scaffolded happy path remains a one-click create with no WCL editing required.
- Positive: imported path coexists with externally-edited WCL — operators retain full control.
- Negative: the import-time verification depends on either a daemon RPC that may not yet exist or a degraded "trust the row, warn loudly" fallback. Acceptable for MVP because the imported path is opt-in and rare.
- Negative: WCL editing in-browser (a future feature) must respect the canonical-source rule per provenance class. That is out of scope here but flagged.

## Alternatives Considered

- *Hand-syncing `socket_path` between WCL and the row, with no contract.* Rejected by red-team: footgun; produces silent dial-against-nothing failures.
- *Manager owns the listener socket and proxies to the daemon.* Rejected for this ADR: that is the wake-on-request question (ADR-0004 deferred clause). Conflating the two surfaces makes both harder to reason about.
- *Refusing imported daemons entirely.* Rejected: a non-trivial fraction of users will have hand-edited WCL they do not want the manager touching.

## References

- `crates/aciddog/src/config.rs:295-305,1851-1951` (`:webservice` block parsing).
- `crates/aciddog/src/webservice/mod.rs:209-225` (bind-path consumption).
- `init-team/aciddog.wcl:1-58` (current scaffold ships no `:webservice`).
- Intake task: "Add `:webservice` automaton to `init-team/aciddog.wcl` scaffold template".
- Intake task: "Spike: aciddog-config library extraction vs `aciddog check` subcommand".
- ADR-0002 (model invariant), ADR-0004 (relationship to wake mechanics).
