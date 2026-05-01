# Field-by-field WCL reference

Every field in this document is sourced from `/home/wil/dev/aciddog/wcl/aciddog.wcl`. Cross-check before writing real WCL.

Notation: `req` = required, `opt` = optional, `default` shown when relevant.

---

## `team` (exactly one per file)

| Field                       | Type       | Status              | Description |
|-----------------------------|------------|---------------------|-------------|
| `id` (block name)           | identifier | req (`@inline(0)`)  | Team identifier. |
| `description`               | string     | req                 | Human description. |
| `replace_thinking_messages` | bool       | opt, default `false` | If `true`, user `thinking_messages` blocks replace built-in pool; else merge. |
| `default_ai_core`           | string     | opt                 | Fleet-wide fallback `ai_core` id. |

---

## `agent` (zero or more)

| Field                | Type            | Status               | Description |
|----------------------|-----------------|----------------------|-------------|
| `id` (block name)    | identifier      | req                  | Agent id. |
| `role`               | string          | req                  | Short role label (shown in `<team>` to peers). |
| `role_description`   | string          | opt                  | Multi-line role detail. |
| `model`              | string          | opt                  | Per-agent model override. Falls through to ai_core's `model` if unset. |
| `instructions`       | string          | req                  | Operational brief, the `<instructions>` block in first-turn prompt. |
| `can_message`        | list(string)    | opt                  | Allowlist of agent/automaton ids reachable via `send`. Omit = unrestricted; `[]` = none. |
| `can_create_workers` | list(string)    | opt                  | Worker template ids spawnable. Omit = none; `[]` = none. |
| `can_run_workflows`  | list(string)    | opt                  | Workflow ids triggerable. Omit = none. |
| `tools`              | list(string)    | opt                  | Per-agent tool allowlist. Falls through to ai_core's `default_tools`. |
| `ai_core`            | string          | opt                  | Per-agent core override. Falls through to team `default_ai_core`. |
| `memory_banks`       | list(string)    | opt                  | Banks this agent reads/writes. |
| `skills`             | list(string)    | opt                  | Skills pre-announced in first-turn prompt. Names: `^[a-z0-9][a-z0-9_-]{0,63}$`. |
| `compaction`         | string          | opt                  | Compaction policy id. Falls back to `:drop` when `/compact` fires unconfigured. |
| `supervisor`         | bool            | opt, default `false` | Grants observe/control RPCs and bypasses own `can_message`. |
| `sysadmin`           | bool            | opt, default `false` | Adds `server shutdown` / `server exit` / `server reload`. **Implies `supervisor = true`.** |
| `leader`             | bool            | opt, default `false` | Pin to the top of the agent's group with 👑. Cosmetic. |
| `group`              | string          | opt                  | Free-form section label. Rows sharing a value cluster under a `── {group} ──` header in the TUI sidebar / web manager. Leaders pin to the top of their own group; ungrouped rows fall under `(ungrouped)` (rendered last). |
| `on_reload`          | symbol          | opt                  | `:ignore` (default), `:compact`, or `:reset` — what `/reload` does to this agent's running session. |

---

## `worker` (zero or more — templates, not instances)

| Field                | Type            | Status | Description |
|----------------------|-----------------|--------|-------------|
| `id` (block name)    | identifier      | req    | Worker template id. |
| `role`               | string          | req    | Role label. |
| `role_description`   | string          | opt    | Multi-line role detail. |
| `model`              | string          | opt    | Per-template model override. |
| `instructions`       | string          | req    | Operational brief. |
| `tools`              | list(string)    | opt    | Per-template tool allowlist. |
| `can_create_workers` | list(string)    | opt    | Sub-worker templates spawnable. |
| `can_run_workflows`  | list(string)    | opt    | Workflow ids triggerable. |
| `ai_core`            | string          | opt    | Per-template core override. |
| `memory_banks`       | list(string)    | opt    | Banks accessible. |
| `skills`             | list(string)    | opt    | Pre-announced skills. |
| `compaction`         | string          | opt    | Compaction policy id. |
| `group`              | string          | opt    | Same as `agent.group`. Editor-metadata only at runtime — workers in the runtime sidebar always nest under their spawning parent regardless of `group`. |

Workers do **not** support `can_message`, `supervisor`, `sysadmin`, or `on_reload`.

---

## `automaton` (zero or more — `@tagged(field = "type")`)

Common fields (all variants):

| Field              | Type            | Status               | Description |
|--------------------|-----------------|----------------------|-------------|
| `id` (block name)  | identifier      | req                  | Automaton id. |
| `role`             | string          | req                  | Role label. |
| `role_description` | string          | opt                  | Detail. |
| `type`             | symbol          | opt                  | `:script` (default), `:webservice`, or `:remote`. |
| `can_message`      | list(string)    | opt                  | Targets reachable via `aciddog send --to`. |
| `supervisor`       | bool            | opt, default `false` | Supervisor RPCs (`:webservice` typically). |
| `sysadmin`         | bool            | opt, default `false` | Server-lifecycle RPCs. Implies `supervisor`. |
| `leader`           | bool            | opt, default `false` | Pin to top of group with 👑. Cosmetic. |
| `group`            | string          | opt                  | Same as `agent.group` — clusters this automaton under a `── {group} ──` header. |

### variant `:script` (default)

| Field             | Type            | Status | Description |
|-------------------|-----------------|--------|-------------|
| `command`         | string          | req    | Bare binary on PATH or absolute path. NOT a shell line. |
| `args`            | list(string)    | opt    | Extra argv entries. |
| `cwd`             | string          | opt    | Working directory. |
| `env`             | list(string)    | opt    | `KEY=VAL` strings (each must contain `=`). |
| `timeout_seconds` | number          | opt    | Kill child if exceeds. |

### variant `:webservice`

| Field         | Type   | Status | Description |
|---------------|--------|--------|-------------|
| `bind`        | string | opt    | `host:port` for HTTPS. Mutually exclusive with `socket_path`. |
| `tls_cert`    | string | opt    | PEM cert path. Required with `bind`. |
| `tls_key`     | string | opt    | PEM key path. Required with `bind`. |
| `api_key_env` | string | opt    | Env var holding API key. Required with `bind`; forbidden with `socket_path`. |
| `socket_path` | string | opt    | Unix socket path. Mutually exclusive with `bind`. |

### variant `:remote`

| Field               | Type   | Status | Description |
|---------------------|--------|--------|-------------|
| `url`               | string | opt    | HTTPS base URL. Mutually exclusive with `socket_path`. |
| `api_key_env`       | string | opt    | Bearer token env var. Required with `url`; forbidden with `socket_path`. |
| `tls_ca`            | string | opt    | PEM CA bundle for self-signed verification. |
| `socket_path`       | string | opt    | Unix socket path on the remote. |
| `remote_target`     | string | req    | Id of the fleet member on the remote to proxy. |
| `deliver_to`        | string | req    | Local agent/automaton id receiving inbound SSE events. Validated. |
| `proxy_socket_path` | string | opt    | Local socket exposing full RPC tunnel through this automaton. |

---

## `webhook` (zero or more)

| Field             | Type   | Status            | Description |
|-------------------|--------|-------------------|-------------|
| `id` (block name) | identifier | req           | Webhook id. |
| `mount`           | string | req               | Id of a `:webservice` automaton. |
| `path`            | string | req               | HTTP path; must start with `/`; no collision with `/v1/rpc`, `/v1/events`, `/v1/health`. |
| `deliver_to`      | string | req               | Agent or automaton id receiving the delivery. |
| `max_body_bytes`  | number | opt, default 1 MiB | Reject larger bodies with HTTP 413. |
| `auth_kind`       | symbol | opt               | `:none`, `:bearer`, `:header`, `:json_field`, `:hmac_header`. |
| `auth_header`     | string | opt               | Header name for `:header` / `:hmac_header`. |
| `auth_field`      | string | opt               | Dot-path into JSON body for `:json_field`. |
| `auth_secret_env` | string | opt               | Env var with expected value. **Required for every non-`:none` variant.** |
| `auth_algo`       | symbol | opt, default `:sha256` | `:sha256` or `:sha1` for `:hmac_header`. |

---

## `memory_bank` (zero or more)

| Field             | Type       | Status                            | Description |
|-------------------|------------|-----------------------------------|-------------|
| `id` (block name) | identifier | req                               | Bank id. On disk: `.aciddog/memory/<id>/`. |
| `description`     | string     | req                               | Description. |
| `embed_model`     | string     | opt, default `BGE-small-en-v1.5`  | fastembed model id. |

---

## `channel` (zero or more)

| Field             | Type         | Status | Description |
|-------------------|--------------|--------|-------------|
| `id` (block name) | identifier   | req    | Channel id. |
| `description`     | string       | opt    | Description. |
| `welcome`         | string       | opt    | Cosmetic banner shown in TUI. **Never injected into any agent's context.** |
| `members`         | list(string) | req    | Agent ids only — workers and automatons rejected. |

---

## `compaction` (zero or more — `@tagged(field = "strategy")`)

Common fields:

| Field             | Type       | Status | Description |
|-------------------|------------|--------|-------------|
| `id` (block name) | identifier | req    | Policy id. |
| `strategy`        | symbol     | req    | `:harness`, `:drop`, `:promptAndDrop`, or `:summarise`. |

### variant `:harness` — no extra fields. Only valid with `:claudeCode` cores.
### variant `:drop` — no extra fields.
### variant `:promptAndDrop`

| Field    | Type   | Status | Description |
|----------|--------|--------|-------------|
| `prompt` | string | req    | Final-turn prompt before drop + re-seed. |

### variant `:summarise`

| Field     | Type   | Status | Description |
|-----------|--------|--------|-------------|
| `prompt`  | string | req    | Prompt for the throwaway summariser. |
| `ai_core` | string | opt    | Core to use for summarisation. Defaults to the agent's own. |

---

## `ai_core` (one or more — `@tagged(field = "type")`)

Common fields:

| Field             | Type       | Status | Description |
|-------------------|------------|--------|-------------|
| `id` (block name) | identifier | req    | Core id. |
| `type`            | symbol     | req    | Variant discriminator. |
| `usage_provider`  | symbol     | opt    | `:zai`, `:chatgpt_oauth`, `:openrouter`, `:github_copilot`. Omit to disable usage tracking. |

### variant `:claudeCode`

| Field           | Type         | Status | Description |
|-----------------|--------------|--------|-------------|
| `systemPrompt`  | string       | opt    | Written to temp file, passed via `--system-prompt-file`. |
| `model`         | string       | opt    | `--model <id>` for `claude` CLI. |
| `default_tools` | list(string) | opt    | Fallback tool allowlist. |

### variant `:anthropicApi`

| Field           | Type         | Status | Description |
|-----------------|--------------|--------|-------------|
| `apiUrl`        | string       | req    | Base URL; `/v1/messages` is appended. |
| `apiKeyEnv`     | string       | req    | Env var with API key. |
| `model`         | string       | req    | Model id sent in `model` field. |
| `systemPrompt`  | string       | opt    | Top-level `system` field. |
| `default_tools` | list(string) | opt    | Built-ins: Bash, Read, Write, Edit, Glob, Grep, WebFetch. |

### variant `:openAiApi`

| Field           | Type         | Status | Description |
|-----------------|--------------|--------|-------------|
| `apiUrl`        | string       | req    | **v1 base** (e.g. `https://api.openai.com/v1`). `/chat/completions` appended. |
| `apiKeyEnv`     | string       | req    | Env var with API key. |
| `model`         | string       | req    | Model id. |
| `systemPrompt`  | string       | opt    | Prepended as `role: "system"` message. |
| `default_tools` | list(string) | opt    | Built-ins: Bash, Read, Write, Edit, Glob, Grep, WebFetch. |

### variant `:openAiChatgpt`

| Field             | Type         | Status | Description |
|-------------------|--------------|--------|-------------|
| `credential`      | string       | req    | Name in `~/.config/aciddog/auth.json`. Create via `aciddog auth login chatgpt --name <n>`. |
| `model`           | string       | req    | e.g. `gpt-5.4`, `gpt-5.1-codex`. |
| `systemPrompt`    | string       | opt    | Delivered as `role: "developer"` head of `input[]`. |
| `default_tools`   | list(string) | opt    | Built-ins as above. |
| `reasoningEffort` | string       | opt, default `medium` | `low`, `medium`, `high`. **`minimal` rejected at parse.** |

### variant `:copilotCli`

| Field           | Type         | Status | Description |
|-----------------|--------------|--------|-------------|
| `model`         | string       | opt    | `--model <id>` for `copilot` CLI (e.g. `claude-haiku-4.5`). |
| `default_tools` | list(string) | opt    | Capitalised aciddog names — only Bash, Read, Write, Edit, Glob, Grep map. |

### variant `:geminiCli`

| Field           | Type         | Status | Description |
|-----------------|--------------|--------|-------------|
| `model`         | string       | opt    | `--model <id>` for `gemini` CLI. |
| `default_tools` | list(string) | opt    | Capitalised aciddog names — only the same six map. |

### variant `:jules` (worker-only)

| Field                  | Type    | Status               | Description |
|------------------------|---------|----------------------|-------------|
| `apiKeyEnv`            | string  | req                  | Env var with Jules API key. |
| `defaultBranch`        | string  | opt                  | Fallback `startingBranch` when parent omits `branch` in spawn properties. |
| `pollIntervalSeconds`  | number  | opt, default 10      | Clamped to `3..=120`. |
| `requirePlanApproval`  | boolean | opt, default `true`  | Pause for parent's `approve` reply before execution. |
| `automationMode`       | string  | opt                  | `AUTOMATION_MODE_UNSPECIFIED` or `AUTO_CREATE_PR`. |

---

## `workflow` (zero or more)

Workflows have no schema — their nested `property` and `step` blocks are validated in Rust by `validate_workflow_refs` in `crates/aciddog/src/config.rs`.

Workflow-level fields:

| Field         | Type   | Status | Description |
|---------------|--------|--------|-------------|
| `id` (block)  | identifier | req | Workflow id. |
| `description` | string | opt    | Description. |
| `max_retries` | number | opt    | Per-step retry count. |

Property kinds (`kind = …`):

| Kind          | Extra fields                       | Description |
|---------------|-----------------------------------|-------------|
| `:text`       | `default = "..."`                 | Free-form text. |
| `:select`     | `options = [...]`, `default`      | Single-choice dropdown. |
| `:multiSelect`| `options = [...]`, `default = [...]` | Multi-choice. |
| `:hidden`     | `default = "..."`                 | Not shown in form; mutable at runtime via `workflow_set`. |

Step kinds (`kind = …`):

| Kind            | Extra fields                                         | Description |
|-----------------|-----------------------------------------------------|-------------|
| `:command`      | `command = "..."`                                   | Shell command (with `$$VAR$$` substitution). |
| `:check`        | `variable`, `op` (e.g. `:equals`), `value`, `target` | Variable gate; on fail jumps to `target`. |
| `:worker`       | (template ref + task — wired in follow-up)          | Spawns a worker step. |
| `:conditional`  | (branching — wired in follow-up)                    | Conditional branch. |

Variable substitution: `$$varname$$` in any string field is replaced at step start with the live variable map.

---

## `thinking_messages` (zero or more)

| Field      | Type         | Status | Description |
|------------|--------------|--------|-------------|
| `messages` | list(string) | req    | Indicator strings shown while an agent is working. Multiple blocks concatenate. |
