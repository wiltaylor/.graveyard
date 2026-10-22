# aciddog web manager

The manager is a SolidJS + Axum web portal that registers one or more aciddog instances and edits their team WCL remotely. It's a separate binary (`aciddog-manager`) but talks to a daemon's `:webservice` automaton over the same RPC surface as any other client.

## Crate

- Path: `/home/wil/dev/aciddog/crates/aciddog-manager/`
- Binary: `aciddog-manager`
- Frontend: SolidJS + Vite + Tailwind under `crates/aciddog-manager/web/`. Bundled into static files by `npm run build` and served by the Rust binary.

## First-run setup

1. **Install frontend deps (once):**
   ```sh
   just manager-frontend-install
   ```
2. **Build frontend (every time the UI source changes):**
   ```sh
   just manager-frontend-build
   ```
3. **Build the manager binary (frontend included):**
   ```sh
   just manager-build
   ```
4. **Run with a deterministic test/test admin** (good for first try):
   ```sh
   just run-manager
   ```
   That recipe runs `reset-password --username test --password test` then starts the server. Replace with a real password before exposing the portal.
5. **Or run by hand:**
   ```sh
   # Set/reset an admin password (creates the user if missing):
   cargo run --manifest-path crates/aciddog-manager/Cargo.toml -- \
     reset-password --username admin --password '<pick-one>'
   # Start the server:
   cargo run --manifest-path crates/aciddog-manager/Cargo.toml
   ```
   Omit `--password` and a random one is generated and printed.

The SQLite database auto-migrates on startup. Migrations live under `crates/aciddog-manager/migrations/`.

## Defaults & environment

| Flag           | Env var                       | Default                                | Purpose |
|----------------|-------------------------------|----------------------------------------|---------|
| `--bind`       | `ACIDDOG_MANAGER_BIND`        | `127.0.0.1:7000`                       | Server bind address. |
| `--db`         | `ACIDDOG_MANAGER_DB`          | `./aciddog-manager.db`                 | SQLite path. |
| `--data-dir`   | `ACIDDOG_MANAGER_DATA`        | `./aciddog-manager-data`               | Skills catalog and marketplace clones. |
| `--cookie-secure` | `ACIDDOG_MANAGER_COOKIE_SECURE` | off                              | Add `Secure` to session cookie. Turn on when fronted by TLS. |

The daemon-runner subsystem is always enabled (ADR-0008). It has no on/off knob; only the three log-rotation knobs (`ACIDDOG_MANAGER_DAEMON_LOG_*`) and `--aciddog-binary` remain configurable.

Open `http://127.0.0.1:7000` once it's running; log in with the username/password set above.

## Manager ↔ daemon relationship

The manager is a **client** of the aciddog daemon. To connect a daemon, the daemon's WCL must declare a `:webservice` automaton (HTTPS or Unix socket) — typically with `supervisor = true` and `sysadmin = true` so the manager can read fleet state, write team WCL, and trigger reloads.

When the manager registers a new instance you supply either:
- An HTTPS URL + bearer token (env var `api_key_env` on the daemon side), or
- A Unix socket path (trust boundary is filesystem permissions, no token needed).

Each instance is stored in the `instances` table; the client is built from that row by `Client::from_row()` in `crates/aciddog-manager/src/client.rs`.

## Feature tour

### Team screen

- Reads the daemon's current `aciddog.wcl` and renders an info dashboard: team `description`, `default_ai_core`, `replace_thinking_messages`, plus KPI counts for agents, workers, automatons, channels, workflows.
- AI Cores section lists every `ai_core` with its model, credential/api-key env var, default tools, and which agents reference it.
- Header buttons: **Edit team**, **Refresh**, **Reload config**, **Remove team**.

### Team editor (catalog-aware modal)

- Tabbed editor for each block type: agents, automatons, workers, workflows, channels, memory banks, compactions, ai_cores.
- Each tab has an entity selector and a form. Workflows have reorderable step rows.
- Modal **chip picker** for selecting from the live daemon catalog when filling allowlists like `can_message`, `can_create_workers`, `members`. Picks come from the daemon's actual declared ids — no typos.
- On save, the editor serialises back to WCL and POSTs to `/api/instances/:id/team/wcl`. The daemon validates with `parse_config()`; on error the existing file is unchanged and the error is shown in a toast.

### Reload-config button

- On the Team screen header. Calls `POST /api/instances/:id/team/reload`, which sends `RequestCmd::ServerReload` over RPC.
- The daemon runs the same reload path as the TUI's `/reload` (per-agent `on_reload` policy applies). Result toast shows the reconciliation summary ("agents +N/-M/~K …").

### Skills catalog

- Stored under `--data-dir`/`skills/<source>/<name>/` on the manager host.
- Three sources tracked in SQLite: `uploaded` (manual upload), `imported` (pulled from a registered instance), `marketplace` (cloned from an Anthropic-format plugin repo).
- Migrations: `0003_skills.sql`, `0004_skill_marketplace_count.sql`.
- Note: WCL still references skills by directory name only — the manager stages catalog skills into a daemon's `.aciddog/skills/` rather than introducing catalog URIs into WCL.

## Data flow when editing a team

1. User edits in TeamEditor → component calls `api.writeTeam(instanceId, wcl)`.
2. Manager → daemon: `POST /v1/rpc` with `RequestCmd::team_write { wcl }`.
3. Daemon validates via `config::parse_config()`. Bad WCL → error returned, file untouched.
4. On success: write to `aciddog.wcl.tmp`, atomic `rename(2)` to `aciddog.wcl`.
5. (Optional) "Reload config" → `POST /api/instances/:id/team/reload` → daemon runs `tui::reload()` and reconciles. Reconciliation summary returned and toasted.

## Justfile recipes (manager)

| Recipe                       | Effect |
|------------------------------|--------|
| `just manager-frontend-install` | `npm install` in `crates/aciddog-manager/web`. |
| `just manager-frontend-build`   | `npm run build` → `web/dist/`. |
| `just manager-build`         | Build frontend then `cargo build` the binary. |
| `just manager-check`         | `cargo check` for the manager crate. |
| `just manager-lint`          | `cargo clippy -- -D warnings`. |
| `just manager-fmt` / `manager-fmt-check` | Format / check formatting. |
| `just manager-test`          | Run the manager's test suite. |
| `just run-manager`           | Reset password to `test/test` then start the server. |

`just build` and `just test` cover both the app and the manager. `just ci` runs the full pipeline.

## Source-of-truth pointers

- Binary entry: `crates/aciddog-manager/src/main.rs`
- CLI / env config: `crates/aciddog-manager/src/config.rs`
- Team routes: `crates/aciddog-manager/src/routes/team.rs`
- RPC layer: `crates/aciddog-manager/src/ipc.rs`
- Client builder: `crates/aciddog-manager/src/client.rs`
- TeamEditor component: `crates/aciddog-manager/web/src/components/team/TeamEditor.tsx`
- Migrations: `crates/aciddog-manager/migrations/`
