# ADR-0008 — Remove daemon-runner capability gate; always enabled

**Status:** Accepted
**Date:** 2026-05-04
**Deciders:** user, dev_lead

## Context

ADR-0005 added a runtime capability gate (`--enable-daemon-runner` /
`ACIDDOG_MANAGER_ENABLE_DAEMON_RUNNER`) so a manager deployment could refuse
to expose `/api/managed-daemons/*` and refuse to start the reconciler /
runner subsystem. Its rationale was to keep a single binary safe to ship
into both a self-hosted homelab (where local-process spawning is fine) and
a hypothetical hosted multi-tenant deployment (where it is catastrophic).

In practice the project is shipped only as a self-hosted operator tool. The
gate-off code path is unreached by every real deployment, the gate-on path
is the only one anyone exercises, and the off-by-default surfaced as a
day-one usability cliff: the Daemons UI was built and shipped but invisible
to operators because nobody passed the flag. The cost of carrying the gate
(two layers of enforcement, a wire-echoed bool, frontend prop plumbing,
gate-off router and `whoami` tests, ADR documentation, contributor mental
overhead) exceeds the benefit it currently delivers.

The project owner has decided that hosted multi-tenant deployment is not on
the roadmap and that the operator-facing usability cost of the gate is no
longer justified.

## Decision

Remove the daemon-runner capability gate. The subsystem is unconditionally
enabled.

Concretely:

- `--enable-daemon-runner` and `ACIDDOG_MANAGER_ENABLE_DAEMON_RUNNER` are
  removed from `CliArgs` (`crates/aciddog-manager/src/config.rs`).
- `AppState.is_daemon_runner_enabled` is removed; `state.daemon_runner`
  becomes non-`Option` and is always constructed at boot.
  `state.aciddog_binary` is always resolved at boot via the existing
  3-tier resolver (sibling-of-exe → PATH → explicit `--aciddog-binary`).
- The router unconditionally registers `/api/managed-daemons/*`
  (`src/routes/mod.rs`); the conditional `nest` is removed.
- The reconciler / runner tasks are unconditionally spawned in `main.rs`;
  the "gate-on but runner is None" warn-branch becomes unreachable and is
  deleted.
- `whoami` and `login` responses keep the existing `is_daemon_runner_enabled`
  field for one release, hardcoded to `true`, so existing clients do not
  break. A follow-up release removes the field; that removal will be its
  own ADR if it changes wire shape in a non-additive way.
- The frontend `App.tsx` capability signal, the `AppShell.tsx`
  `BASE_TABS`/`<Show>` gating, and the prop on `AppShell` are removed.
  The Daemons tab is always present in the tab strip.
- Gate-off tests (`tests/routes_managed_daemons.rs` 404-when-off, the
  `is_daemon_runner_enabled: false` body assertions, and the analogous
  frontend tests `mobile-drawer.test.tsx` / `DaemonsView.test.tsx` `false`
  arm) are removed. Gate-on tests stay and lose their boolean parameter.
- `--aciddog-binary`, `--daemon-runner-log-max-bytes`,
  `--daemon-runner-log-max-files`, and `--daemon-runner-log-ring-bytes`
  flags are kept — they are configuration knobs for the (now always-on)
  runner, not gates.
- `require_admin` on `/api/managed-daemons/*` is **kept**. It was a second
  line of defence behind the gate; with the gate removed it becomes the
  only line of defence and is non-negotiable.
- The router smoke test from ADR-0005 §Decision (assert zero
  `^/api/managed-daemons` routes when off) is deleted; a positive smoke
  test asserting the routes are *present* replaces it, so an accidental
  unconditional drop of the subtree is still caught.

## Consequences

- Positive: the Daemons UI is discoverable by default; no env-var folklore
  required to use a feature the binary advertises. Operator UX matches
  shipped capability.
- Positive: one less branch in router composition, one less wire-echoed
  bool, one less frontend prop, one less prop drilled through tests.
  Reduced contributor surface area.
- Positive: ADR-0005's stated negative ("two layers of enforcement means
  tests must cover both") is eliminated; the only auth surface is now
  `require_admin`, which is the surface every other admin route already
  uses.
- Negative: no in-binary way to ship a manager that does not expose
  `/api/managed-daemons/*`. Hosted/multi-tenant deployments would now
  require either (a) a build-time `daemon-runner` Cargo feature, or
  (b) a network-level reverse-proxy block, or (c) a deliberate fork.
  This is acceptable because hosted deployment is explicitly out of scope.
- Negative: `require_admin` is now the sole defence against
  local-process-spawning misuse. Any regression in admin auth — session
  hijack, role-check bug — exposes the full surface. Mitigation: existing
  admin tests must be expanded to cover this surface explicitly.
- Negative: Cargo feature `daemon-runner` deferred by ADR-0005 §Alternatives
  is now the *only* remaining knob if a future deployment ever needs to
  strip the surface entirely. The deferred work item still exists; we now
  rely on it more heavily if hosted ever returns.

## Alternatives Considered

- *Keep the runtime gate, default it to ON.* Rejected: solves the
  discoverability cliff but keeps the prop plumbing, the wire-echoed bool,
  the gate-off tests, and the contributor confusion. Cost stays, benefit
  shrinks.
- *Replace the runtime gate with a Cargo feature `daemon-runner`.*
  Rejected for this ADR: build-time stripping is strictly stronger than a
  runtime branch but only useful if a deployment actually wants the
  surface absent. No current deployment does. Tracked as a still-open
  follow-up from ADR-0005 §Alternatives if a hosted deployment appears.
- *Status quo (gate off by default, document the env var more loudly).*
  Rejected: documentation does not fix the operator UX of "I shipped a
  feature and cannot find it". Every operator hits the cliff once.

## References

- ADR-0005 (the gate this ADR removes; now marked Superseded by ADR-0008).
- ADR-0001 (the subsystem placement that ADR-0005 was protecting).
- `crates/aciddog-manager/src/config.rs` — fields to remove.
- `crates/aciddog-manager/src/state.rs` — `is_daemon_runner_enabled` and
  `Option<Arc<DaemonRunner>>` shape change.
- `crates/aciddog-manager/src/routes/mod.rs` — conditional nest to remove.
- `crates/aciddog-manager/src/main.rs` — unconditional runner construction.
- `crates/aciddog-manager/src/routes/auth.rs` — `whoami`/`login` field
  policy (hardcode `true` for one release; follow-up removal).
- `crates/aciddog-manager/web/src/App.tsx`, `AppShell.tsx` — frontend
  prop and `<Show>` removals.
- `crates/aciddog-manager/tests/routes_managed_daemons.rs` — gate-off tests
  to delete; replace with a positive presence smoke test.
- `crates/aciddog-manager/web/src/__tests__/mobile-drawer.test.tsx` and
  `web/src/components/__tests__/DaemonsView.test.tsx` — `false`-arm test
  cases to delete.
