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

Idle timeout is per-agent and opt-in. Set `idle_timeout = "<dur>"` on an
`agent` block, or `default_idle_timeout = "<dur>"` on the `team` block, to
auto-act on agents that have been Ready for that duration; durations parse via
`humantime` (`"30s"`, `"10m"`, `"2h"`), values below `5s` are rejected at
config load, and omitting both fields keeps the default disabled behavior.

- Leaders with at least one non-Hibernated direct child worker receive a
  synthetic user prompt (`NUDGE`) so their run loop can decide what to do.
- Leaf agents with no live workers get an automatic `/compact`.
- Cold inbox agents (`ever_prompted == false`) are never auto-nudged.
  Hibernated, Asleep, and Errored agents are skipped.

## Workers

Agents can spawn ephemeral **workers** to handle sub-tasks. Workers are
declared in `aciddog.wcl` as `worker <id> { ... }` blocks and are
opt-in per agent via `can_create_workers = ["<worker-id>"]`. An agent
spawns one by running `aciddog worker create --template <id> --task
"<what to do>"`; pass `--cwd <path>` to override a task worktree or
template cwd for that spawn. Tasks can bind a worker cwd with
`aciddog task add ... --worktree <path>` and revise or clear it with
`aciddog task update ... --worktree <path>` /
`--clear-worktree`. Cwd precedence is `--cwd > task.worktree >
template default > orchestrator current_dir`; explicit paths must
exist and be directories. The worker reports back with
`aciddog worker complete --body "<findings>"`, which delivers its
report to the parent and hibernates it. Hibernated workers keep their
pane and history — sending them another message resumes the session.
See [`demo/aciddog.wcl`](demo/aciddog.wcl) for a working example.

## Webservice & webhooks

An automaton declared with `type = :webservice` hosts an HTTP server
(HTTPS with an API key, or a Unix socket) instead of spawning a child
process per event. External clients can drive the fleet over it, and
third-party systems can POST webhook payloads that get delivered as
messages.

```wcl
automaton api {
  type         = :webservice
  role         = "control plane"
  bind         = "0.0.0.0:8443"
  tls_cert     = "./demo/tls/cert.pem"
  tls_key      = "./demo/tls/key.pem"
  api_key_env  = "ACIDDOG_API_KEY"
  can_message  = ["planner", "coder"]
}

webhook github_push {
  mount           = "api"
  path            = "/hooks/github/push"
  deliver_to      = "coder"
  auth_kind       = :hmac_header
  auth_header     = "X-Hub-Signature-256"
  auth_secret_env = "GITHUB_WEBHOOK_SECRET"
}
```

Endpoints on every webservice:

- `POST /v1/rpc` — same JSON shape the internal agent IPC accepts
  (`{"cmd":"send_message","to":"planner","body":"hi"}` etc). Requests
  are stamped with the automaton's identity and subject to its
  `can_message` ACL.
- `GET /v1/events` — Server-Sent Events stream of the mail / channel
  traffic this automaton receives.
- `GET /v1/health` — liveness probe.
- Each configured `webhook` mounts as an additional path.

Smoke test the RPC endpoint:

```sh
curl -k -H "Authorization: Bearer $ACIDDOG_API_KEY" \
     https://localhost:8443/v1/rpc \
     -d '{"cmd":"send_message","to":"planner","body":"hello"}'
```

Smoke test a GitHub-style webhook locally:

```sh
BODY='{"ref":"refs/heads/main"}'
SIG=$(printf '%s' "$BODY" | \
      openssl dgst -sha256 -hmac "$GITHUB_WEBHOOK_SECRET" -hex | \
      awk '{print $2}')
curl -k -H "X-Hub-Signature-256: sha256=$SIG" \
     -H "Content-Type: application/json" \
     https://localhost:8443/hooks/github/push \
     -d "$BODY"
```

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
