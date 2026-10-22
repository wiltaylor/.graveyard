# ADR-0002 — managed_daemons row plus auto-managed instances row, in one sqlx transaction

**Status:** Accepted
**Date:** 2026-05-03
**Deciders:** dev_lead, quality_consultant

## Context

The manager already has an `instances` model representing "a thing I talk to via Unix socket or HTTPS" (`crates/aciddog-manager/src/model/instance.rs`). The Unix transport variant explicitly forbids API keys, on the basis that **filesystem perms are the trust boundary** (`instance.rs:128-131`). A locally-managed daemon is, from the IPC client's point of view, exactly that: a Unix socket with filesystem-level trust. What it has additionally is lifecycle metadata (work directory, socket path, desired state, policy).

If lifecycle fields landed directly on `instances`, every `transport=https` row would carry NULL columns it can never use, and the frontend would have to learn an "is this row manageable?" predicate. If managed daemons used a separate connection abstraction, the IPC client and routes would learn a second connection type for no semantic gain.

## Decision

Introduce a new `managed_daemons` table carrying lifecycle fields only. Each `managed_daemons` row owns exactly one auto-managed `instances` row with `transport='unix'` and `managed_by_id` set as a FK back. Both rows are created and destroyed in a single sqlx transaction at the model layer; users do not pre-create an `instances` row for a managed daemon.

`managed_daemons` columns:

- `id` (TEXT PK, uuid)
- `name` (TEXT NOT NULL UNIQUE)
- `work_dir` (TEXT NOT NULL UNIQUE; canonicalised before insert)
- `socket_path` (TEXT NOT NULL)
- `desired_state` (TEXT NOT NULL, CHECK in `('stopped','running')`)
- `start_on_manager_boot` (INTEGER NOT NULL DEFAULT 0)
- `idle_timeout_secs` (INTEGER NULL)
- `start_on_demand` (INTEGER NOT NULL DEFAULT 0)
- `created_at`, `updated_at` (TEXT, RFC 3339)

`instances` migration: `ALTER TABLE instances ADD COLUMN managed_by_id TEXT NULL REFERENCES managed_daemons(id) ON DELETE CASCADE`.

Excluded from the schema deliberately:

- **`pid_file`** — `aciddog daemon` does not write one today (verified at `crates/aciddog/src/daemon.rs:1-139` and `main.rs:219-247`). Adoption uses the existing health-probe path on `socket_path` instead (see ADR-0003). No cross-crate change to `aciddog` is required.
- **`log_file`** — derived as `work_dir/aciddog.log`, written by the daemon's existing appender (`crates/aciddog/src/main.rs:222`). Manager-owned log capture is a Phase-2 decision, separate from the model.

Naming note: deliberately **not** `local_team`. "Team" already means three things in this repo — the WCL `team default { ... }` block, the manager's `/api/instances/:id/team` proxy route, and (in the original red-team feedback) the proposed managed-process row. Reserving "team" for the WCL/RPC meaning and using `managed_daemon` for the lifecycle row keeps grep unambiguous.

## Consequences

- Positive: no fork of the IPC client and no parallel routes for managed-vs-unmanaged daemons. The frontend's existing instance handling Just Works for managed ones.
- Positive: clean separation of concerns — `instances` = "where is it"; `managed_daemons` = "how does it live and die".
- Positive: a user can still register an externally-supervised local daemon by creating a plain `instances` row with `transport='unix'` and no `managed_by_id`; the existing flow stays available.
- Negative: two-row creation is non-atomic across logical operations; **both inserts must run inside a single sqlx transaction**, enforced in the model layer. Same for delete (`ON DELETE CASCADE` plus the model's wrapper sends SIGTERM first).
- Negative: a `managed_daemon` whose paired `instances` row was deleted manually leaves a dangling row. Schema cascade prevents the inverse; model code must refuse `instances` deletion when `managed_by_id` is set (return 409 — use the `managed_daemon` delete path instead).

## Alternatives Considered

- *Add lifecycle fields directly to `instances`.* Rejected: muddies the abstraction; NULL columns on every HTTPS row; complicates frontend dispatch.
- *Separate `managed_daemons` table that does not reuse `instances`.* Rejected: forces the IPC client and routes to learn a second connection type with no semantic gain.
- *Persist `managed_by_id` from the user-facing API at create time.* Rejected: lets the user sever the invariant by creating an `instances` row first and a `managed_daemons` row later; cleaner to make the model layer the only path.

## References

- `crates/aciddog-manager/src/model/instance.rs:34-77,128-131` (existing instances model + Unix-trust boundary).
- `crates/aciddog-manager/migrations/0001_initial.sql:1-10` (current instances schema).
- `crates/aciddog/src/daemon.rs:1-139`, `main.rs:219-247` (no PID file).
- `crates/aciddog/src/config.rs:13-28,986,1454-1456` (WCL `team` block).
- `crates/aciddog-manager/src/routes/team.rs:38-95` (existing /team route).
- ADR-0001 (subsystem placement), ADR-0003 (adoption mechanics).
