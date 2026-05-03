# Architecture Decision Records

An Architecture Decision Record (ADR) captures a single architecturally
significant decision — its context, the choice made, the consequences, and
the alternatives weighed — at the moment it was taken. ADRs are **immutable
once accepted**: they are never edited in place to reflect a later change of
mind. When a decision is revisited, a new ADR is written and the old one is
marked `Superseded by ADR-NNNN` with a pointer; the historical reasoning
stays intact for future readers.

## Why `docs/adr/` and not `design/adr/`

`design/` is the Forge UI's asset and scratch directory; `docs/` is the
user- and contributor-facing documentation root (with `docs/wcl.md` as
precedent), so ADRs — which are public, in-tree, PR-reviewed records —
belong under `docs/`.

## File naming

ADR files are named `ADR-NNNN-kebab-case-title.md`, where `NNNN` is a
4-digit zero-padded sequence number. Numbering is sequential with no gaps.

## Status states

- **Proposed** — drafted, under review, not yet adopted.
- **Accepted** — adopted; the codebase is expected to reflect this decision.
- **Superseded by ADR-YYYY** — replaced by a later ADR; the pointer names the
  successor.

## Supersession protocol

Numbering races (two ADRs proposed concurrently against the same next
number) are resolved at PR-merge time by re-numbering the latecomer.
Superseded ADRs are **marked** with their successor pointer but **never
edited away**: their context, decision, and consequences remain readable
verbatim so that future readers can reconstruct why the project once chose
differently.

## Relationship to the `decisions` memory bank

ADRs and the `decisions` memory bank both record design rationale, but at
different grains and audiences. ADRs are **coarser-grained**, scoped to a
subsystem or cross-cutting concern, kept **in-tree**, **public**, and
**PR-reviewed**; they are the canonical reference for "why does the
codebase look like this?". The `decisions` memory bank captures
**finer-grained per-task design context** — the kind of trade-off note a
planner or dev_lead writes while shaping a single piece of work (see
e.g. `decisions/938fa631` worker restart-core, `decisions/b802148f`
restart-core permission policy). When a `decisions` entry grows large
enough that it establishes cross-subsystem policy, it is a candidate for
**promotion to an ADR**.

## Backfill

Backfilling pre-existing architectural choices (WCL block grammar, worker
restart-core, channel SSE, etc.) as ADRs is future work.

## Index

- [ADR-0001 — Local team supervision lives inside aciddog-manager](./0001-daemon-runner-in-aciddog-manager.md)
- [ADR-0002 — managed_daemons row + auto-managed instances row in one sqlx tx](./0002-managed-daemons-row-and-instances-fk.md)
- [ADR-0003 — Reconciler model with adoption-on-restart via health-probe](./0003-reconciler-adoption-via-health-probe.md)
- [ADR-0004 — Per-daemon lifecycle policy + sticky desired_state; wake mechanics deferred](./0004-per-daemon-lifecycle-policy.md)
- [ADR-0005 — Capability gate via CLI/env; routes return 404 when off](./0005-capability-gate-cli-env.md)
- [ADR-0006 — Scaffolding contract: manager-injected :webservice automaton](./0006-scaffolding-contract-webservice-injection.md)
