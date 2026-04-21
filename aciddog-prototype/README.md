# aciddog

An agentic orchestrator TUI. Drop an `aciddog.wcl` file in a directory describing
the agent team you want, launch `aciddog`, and drive the team from a terminal UI.

> Phase 0 — only the TUI shell and config loader are implemented. Agent execution
> is deferred to a later phase.

## Requirements

- Rust (stable)
- [`just`](https://github.com/casey/just) — the sole user interface for this project

## Quick start

```sh
just            # list every available recipe
just run        # launch the TUI in demo/
just ci         # fmt check + clippy + tests
```

## Config

The TUI loads `aciddog.wcl` from the current working directory. See
[`demo/aciddog.wcl`](demo/aciddog.wcl) for a working example.

## Secrets

Put secrets like `ANTHROPIC_API_KEY` in a `.env` file alongside `aciddog.wcl`
(e.g. `demo/.env`). `.env` is gitignored and loaded at startup, so the vars
are inherited by spawned agents automatically.

A minimal config looks like:

```wcl
team default {
  description = "Default aciddog team"
}

agent planner {
  role = "coordinator"
  instructions = "Plan and delegate work."
}
```

## Workers

Agents can spawn ephemeral **workers** to handle sub-tasks. Workers are
declared in `aciddog.wcl` as `worker <id> { ... }` blocks and are
opt-in per agent via `can_create_workers = ["<worker-id>"]`. An agent
spawns one by running `aciddog worker create --template <id> --task
"<what to do>"`. The worker reports back with
`aciddog worker complete --body "<findings>"`, which delivers its
report to the parent and hibernates it. Hibernated workers keep their
pane and history — sending them another message resumes the session.
See [`demo/aciddog.wcl`](demo/aciddog.wcl) for a working example.

## Layout

- `src/` — Rust binary + library
- `demo/` — committed demo workspace used by `just run`
- `tests/` — integration tests
- `justfile` — build, test, run recipes

## Logging

Logs go to `aciddog.log` in the working directory (so `demo/aciddog.log` when
launched via `just run`). Adjust verbosity with `--log-level` or `RUST_LOG`:

```sh
aciddog --log-level aciddog=debug
```
