# Using the aciddog TUI

The TUI and the daemon share the same orchestrator core; the daemon is just a headless variant. Same `aciddog.wcl`, same agents, same RPC surface.

## Launching

| Command                                | What it does                                                              |
|----------------------------------------|---------------------------------------------------------------------------|
| `just run`                             | TUI in `demo/`. Auto-runs `wcl-schema-install` first.                     |
| `just run-daemon`                      | Headless daemon in `demo/`. Drive via the manager or any `:webservice` automaton. |
| `just run-remote-server`               | Demo server in `demo-remote-server/` (exposes a `:webservice` over a Unix socket at `/tmp/aciddog-remote-demo.sock`). |
| `just run-remote-server-daemon`        | Same as above, headless.                                                  |
| `just run-remote-client`               | Demo client in `demo-remote-client/` that dials the server.               |
| `cargo run --manifest-path crates/aciddog/Cargo.toml -- [flags]` | Underlying invocation. |
| `cargo run --manifest-path crates/aciddog/Cargo.toml -- daemon`  | Headless variant.       |

CLI flags (apply to both modes):

| Flag                | Default       | Purpose |
|---------------------|---------------|---------|
| `--config <PATH>`   | `aciddog.wcl` | Path to the team WCL file. |
| `--log-level <LVL>` | from `RUST_LOG` | E.g. `info`, `aciddog=debug`. Overrides `RUST_LOG`. |

## Layout

The screen is split into a **main area** (left) and a stack of **right-column panels**. Tab cycles focus through them.

| Right panel  | Contents                                                                 |
|--------------|--------------------------------------------------------------------------|
| Agents       | All declared agents/automatons. Select to bring their chat into the main area. |
| Views        | Switches the main area to one of: Tasks, Memory, Cores, Inbox, Cron.     |
| Workflows    | Workflow templates and live instances.                                   |
| Channels     | Declared + ephemeral (`/meeting`-created) channels.                      |

Main-area views:

| View       | Purpose                                                              |
|------------|----------------------------------------------------------------------|
| Chat       | Selected agent/automaton/channel transcript with composer.           |
| Tasks      | In-TUI task board (add/edit/delete/toggle).                          |
| Memory     | Shared memory banks: search, browse, view detail.                    |
| Cores      | Configured `ai_core`s with usage/quota info from `usage_provider`.   |
| Inbox      | Personal messages addressed to the human operator.                   |
| Cron       | Scheduled payloads (`.aciddog/cron.json`). Add/edit/delete.          |
| Channel(N) | A channel's transcript and composer.                                 |
| Workflow   | A workflow's property-entry form, or a running instance.             |

## Slash commands

All entered in the chat composer. Verified against the dispatch table in `crates/aciddog/src/tui/commands.rs`.

### Lifecycle

| Command              | Effect |
|----------------------|--------|
| `/version`           | Prints aciddog version. |
| `/exit`, `/quit`     | Terminate TUI immediately. |
| `/shutdown`          | Graceful: sleep idle agents, let busy ones finish, then exit. |
| `/reload`            | Re-read `aciddog.wcl` and reconcile fleet (per-agent `on_reload` policy). Parse failure leaves old config active. |

### Agent control (acts on the focused agent)

| Command       | Effect |
|---------------|--------|
| `/compact`    | Run the agent's compaction strategy (or `:drop` if unconfigured). |
| `/reset`      | Drop session, re-seed with first-turn preamble. Ignores compaction. |
| `/stop`       | Cancel the current turn; kill in-flight tools. |
| `/sleep`      | Refuse mailbox/channel messages until `/awake`. |
| `/awake`, `/wake` | Resume receiving messages. |

### Workflows & meetings

| Command                 | Effect |
|-------------------------|--------|
| `/run <workflow_id>`    | Kick off a workflow with declared defaults. |
| `/workflow <workflow_id>` | Alias of `/run`. |
| `/meeting <id>`         | Create an ephemeral channel at runtime. |

### Channel ops (only meaningful from a channel pane)

| Command         | Effect |
|-----------------|--------|
| `/add <agent>`  | Add an agent to the current channel. |
| `/kick <agent>` | Remove an agent from the current channel. |
| `/leave`        | Destroy the current ephemeral channel. WCL-declared channels reject this. |

### Auth (ChatGPT credentials)

| Command                             | Effect |
|-------------------------------------|--------|
| `/auth login <provider> <name>`     | OAuth flow for `chatgpt`. Stores in `~/.config/aciddog/auth.json`. |
| `/auth logout <name>`               | Remove stored credential. |
| `/auth list`                        | List credentials (provider, account, expiry). |
| `/auth status [<name>]`             | Status for one or all credentials. |

## Daemon lifecycle (signals)

| Signal               | Effect |
|----------------------|--------|
| `SIGTERM` / `SIGINT` | Graceful shutdown (same as `/shutdown`). |
| Second SIGTERM/SIGINT| Force immediate exit. |
| `SIGHUP`             | Reload `aciddog.wcl` (same as TUI `/reload`). |

Run as daemon: `just run-daemon` or `cargo run --manifest-path crates/aciddog/Cargo.toml -- daemon`.

## Common workflows

| Want to…                          | How |
|-----------------------------------|-----|
| Switch between agents             | Tab to the **Agents** panel, j/k to navigate, Enter to focus their chat. |
| Send a message                    | Tab to the chat composer in the main area, type, Enter to send. |
| Apply WCL edits to a running fleet | TUI: type `/reload`. Daemon: `kill -HUP <pid>`. Manager: click "Reload config" on the Team screen. |
| Drop and restart an agent's session | `/reset` (always drops) or `/compact` (uses configured strategy). |
| Create an ad-hoc meeting          | `/meeting <id>`, then `/add <agent>` for each invitee. `/leave` when done. |
| Sleep an agent without killing it | `/sleep` on it. `/awake` to resume. Mailbox queues messages while sleeping. |
| View per-core usage/limits        | Views → Cores. Requires `usage_provider` set on the core. |
| Review messages addressed to you  | Views → Inbox. |
| Configure ChatGPT credential      | `/auth login chatgpt personal` (replace `personal` with any name you'll reference in `ai_core` `credential`). |

## Notes on TUI key handling

The right-column panel keys (j/k navigate, Enter activate, Esc back to Agents) are conventional. Detailed in-pane bindings vary by view (Tasks supports a/e/d/x for add/edit/delete/toggle; Cron mirrors that; Inbox uses Enter to open a reply). When in doubt, look at the relevant module under `/home/wil/dev/aciddog/crates/aciddog/src/tui/` (each major view has its own file).
