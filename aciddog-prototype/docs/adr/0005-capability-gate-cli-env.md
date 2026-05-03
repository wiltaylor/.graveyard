# ADR-0005 — Capability gate via CLI/env; routes return 404 when off

**Status:** Accepted
**Date:** 2026-05-03
**Deciders:** dev_lead, security_consultant

## Context

A self-hosted homelab manager and a hosted multi-tenant manager have very different threat models. Local-process spawning is fine in the first and catastrophic in the second. The manager must be able to refuse to expose this capability at all.

The manager today has no file-based config — configuration is `clap`-derived CLI args plus environment variables (`crates/aciddog-manager/src/config.rs:1-58`, `main.rs:18-56`). Inventing a new file-config format for one feature would be disproportionate.

## Decision

Add to the existing `CliArgs`:

- `--enable-daemon-runner` (also `ACIDDOG_MANAGER_ENABLE_DAEMON_RUNNER=1`). Default off.
- `--aciddog-binary <path>` (default: `which("aciddog")`).
- `--daemon-runner-teams-root <path>` (default: `<data_dir>/teams`).

When the gate is **off**:

- The `/api/managed-daemons/*` routes are **not registered** in the axum router. They return `404 Not Found`, **not** `403 Forbidden` — the capability literally does not exist on this deployment, and `404` reveals nothing about whether the feature could be enabled.
- The `daemon_runner` subsystem and reconciler tasks are not started.
- The frontend `/api/bootstrap` payload reports `daemon_runner_enabled: false`; the Daemons tab is hidden.

When the gate is **on**:

- Routes are registered. Handlers additionally enforce an admin role check against the authenticated session.

Smoke test (required): with the gate off, the axum router contains zero routes matching `^/api/managed-daemons`. This catches the class of bug where a route is accidentally registered unconditionally.

## Consequences

- Positive: the most dangerous code path is dead-code-eliminable (or nearly so) in deployments that do not want it.
- Positive: `404` versus `403` is a deliberate signal — probing for the feature on a hardened deployment yields no information about whether it exists.
- Positive: matches the existing manager configuration posture; no new file-config infra to maintain.
- Negative: two layers of enforcement (route registration plus per-handler role check) means tests must cover both. The smoke test above plus a 401/403 test on each handler is the minimum.
- Negative: `which("aciddog")` is environment-dependent; misconfigured PATH on systemd units is a likely first-day issue. Documented in the manager README; explicit `--aciddog-binary` is the escape hatch.

## Alternatives Considered

- *New TOML / WCL config file for the manager.* Rejected: disproportionate for one knob; opens a bikeshed about format.
- *Cargo feature flag `daemon-runner`.* Considered as an add-on, not a replacement: useful to allow build-time stripping for hosted deployments. Both can coexist (`#[cfg(feature = "daemon-runner")]` around the module + runtime config check inside) but is not required for v1.
- *Single global `--unsafe-mode` flag covering this and any future host-touching features.* Rejected: too coarse; collapses unrelated capabilities into one switch.

## References

- `crates/aciddog-manager/src/config.rs:1-58`, `main.rs:18-56` (existing CLI/env config).
- `crates/aciddog-manager/src/routes/mod.rs` (router composition; gate is applied here).
- ADR-0001 (subsystem placement justifies why this gate is needed).
