# Authoring an aciddog team WCL

This is the narrative guide. For exhaustive per-field tables read `wcl-fields.md`. For a copy-paste starter read `example-team.wcl`. For the source of truth read the schema at `/home/wil/dev/aciddog/wcl/aciddog.wcl`.

## File layout

Every team WCL is a single file (`aciddog.wcl`) that begins with the schema import and contains exactly **one** `team { ... }` block plus declarations for every other entity the team uses.

```wcl
import <aciddog.wcl>

ai_core default_core { type = :claudeCode … }   // 1+ ai_cores

team default {                                   // exactly 1 team block
  description = "..."
  default_ai_core = "default_core"
}

agent planner { … }                              // 0+ agents
worker researcher { … }                          // 0+ worker templates
automaton quokka { … }                           // 0+ automatons
webhook github_push { … }                        // 0+ webhooks (mounted on :webservice automatons)
memory_bank project_context { … }                // 0+ memory banks
channel planning { … }                           // 0+ channels
compaction last_word { … }                       // 0+ compaction policies
workflow smoke { … }                             // 0+ workflows
thinking_messages { messages = [...] }           // 0+ thinking-indicator pools
```

The schema library must be installed once before launch:

```sh
just wcl-schema-install   # copies wcl/aciddog.wcl into ${XDG_DATA_HOME:-$HOME/.local/share}/wcl/lib/
```

The `just run-*` recipes do this automatically. The `import <aciddog.wcl>` line resolves to that installed file.

## The `team` block (required, exactly one)

```wcl
team default {
  description = "Default aciddog team"
  default_ai_core = "default_core"          // optional fleet-wide fallback
  replace_thinking_messages = false         // optional; default false (merge with built-ins)
}
```

- `description` — required string.
- `default_ai_core` — optional. Agents/workers without their own `ai_core` field inherit this. If omitted, every agent/worker MUST declare its own `ai_core`.
- `replace_thinking_messages` — when `true`, user-declared `thinking_messages` blocks replace the built-in pool entirely; when `false` (default), they merge.

## Block taxonomy

| Block               | Purpose                                                                                 |
|---------------------|-----------------------------------------------------------------------------------------|
| `ai_core`           | Named runtime (claudeCode, anthropicApi, openAiApi, openAiChatgpt, copilotCli, geminiCli, jules). |
| `agent`             | Persistent AI actor. Joins channels, has a mailbox, optionally supervisor/sysadmin.     |
| `worker`            | **Template** for ephemeral actors spawned on demand by an agent's `worker create`.      |
| `automaton`         | Script-backed (`:script`), webservice-backed (`:webservice`), or remote-bridge (`:remote`) pseudo-agent — no AI core. |
| `webhook`           | Inbound HTTP route mounted on a `:webservice` automaton.                                |
| `memory_bank`       | Embedding-indexed shared store under `.aciddog/memory/<id>/`.                           |
| `channel`           | Multi-agent chat room. Agents only — workers cannot be members.                         |
| `compaction`        | Named `/compact` policy: `:harness`, `:drop`, `:promptAndDrop`, or `:summarise`.        |
| `workflow`          | Reusable typed-input procedure with sequential steps.                                   |
| `thinking_messages` | Pool of indicator strings shown while an agent works (merge or replace).                |

## AI core variants (seven)

| `type`            | Backend                                              | Auth model                          | `model` field |
|-------------------|------------------------------------------------------|-------------------------------------|---------------|
| `:claudeCode`     | Local `claude` CLI                                   | Whatever `claude` is logged into    | optional (`--model`) |
| `:anthropicApi`   | Anthropic Messages API or compatible (Z.ai, etc.)    | `apiKeyEnv` → env var               | required      |
| `:openAiApi`      | OpenAI chat-completions or compatible (OpenRouter)   | `apiKeyEnv` → env var               | required      |
| `:openAiChatgpt`  | ChatGPT subscription via OAuth (Codex `/responses`)  | `credential` (auth.json entry)      | required      |
| `:copilotCli`     | Local `copilot` CLI                                  | `copilot auth`                      | optional (`--model`) |
| `:geminiCli`      | Local `gemini` CLI                                   | `gemini` builtin auth               | optional (`--model`) |
| `:jules`          | Google Jules remote worker (worker-only)             | `apiKeyEnv` → env var               | n/a (Jules picks) |

Direct-API cores (`:anthropicApi`, `:openAiApi`, `:openAiChatgpt`) require a `model` either on the core or on every agent/worker that uses it. CLI cores fall through to the child binary's default if neither is set. The `:jules` core is **only** valid for `worker` templates — never for an `agent`.

## Agents vs workers

- **Agent**: declared at config-load time, runs forever, has a mailbox + chat pane, can join channels, can be a supervisor or sysadmin, has an `on_reload` policy.
- **Worker**: a *template* — instances are spawned by `aciddog worker create --template <id> --task "..."` from within an agent that lists this template in `can_create_workers`. Workers cannot join channels, cannot be supervisors, and have no `on_reload` field.

Field overlap: both accept `role`, `role_description`, `model`, `instructions`, `tools`, `ai_core`, `memory_banks`, `skills`, `compaction`, `can_create_workers`, `can_run_workflows`, `group`. Only agents accept `can_message`, `supervisor`, `sysadmin`, `leader`, `on_reload`.

## Grouping (`group`)

Optional free-form section label on `agent`, `automaton`, and `worker` blocks. Rows sharing a value cluster under a `── {group} ──` header in the TUI sidebar and the web manager (and as `Group` column buckets in the manager's instance detail view).

- **Free-form**: no pre-declared group block needed; the union of every `group` value used becomes the bucket set.
- **Section order**: first-declaration order across `agent` → `automaton` → `worker` blocks. The `(ungrouped)` section always renders last.
- **Leaders**: pin to the top of *their own group* (not the global top). A leader without a `group` floats to the top of `(ungrouped)`.
- **Workers**: at runtime, workers nest under their spawning parent regardless of `group` — the field is editor-metadata only there. The team-editor's worker tab does honour it for visual consistency.
- **Empty string** (`group = ""`) is normalised to "no group" at parse time.

## Allowlist semantics (`can_message`, `can_create_workers`, `can_run_workflows`)

WCL distinguishes *omitted* from *empty list*:

- **Omitted** → unrestricted for `can_message`; **disabled** for `can_create_workers` and `can_run_workflows`.
- **Empty list `[]`** → none allowed (zero agents reachable / zero workers spawnable).
- **Non-empty list** → exactly those ids permitted.

`supervisor = true` bypasses the agent's own `can_message` allowlist.

## Skills

Skills are prompt fragments at `.aciddog/skills/<name>/SKILL.md`. The first non-blank line (after optional YAML frontmatter) is the description shown to agents.

- Reference them by directory name only — there are no catalog URIs in WCL today.
- Names must match `^[a-z0-9][a-z0-9_-]{0,63}$` (lowercase alphanumeric, dash, underscore; max 64 chars; no leading dash).
- Listing a skill in an agent's `skills = [...]` pre-announces it in the first-turn prompt. Unlisted skills remain discoverable via the on-disk listing.
- The manager's central catalog (commits `ad28c37`, `6df7829`) is a future enhancement — today it stages skills, but agents still read them by directory name from `.aciddog/skills/`.

## Compaction strategies

| `strategy`         | Behaviour                                                                                |
|--------------------|------------------------------------------------------------------------------------------|
| `:harness`         | Forward `/compact` literally to the backing CLI. **Only valid with `:claudeCode`.**     |
| `:drop`            | Discard session, re-seed with first-turn preamble.                                       |
| `:promptAndDrop`   | Send one final `prompt` to the existing session, capture reply, then drop + re-seed.     |
| `:summarise`       | Serialise transcript through a throwaway core (`ai_core` field, else the agent's own), then drop + re-seed and inject the digest as the first user message. |

Agents without a `compaction` reference fall back to `:drop` when `/compact` fires. `/reset` always drops regardless.

## Reload behaviour

Triggered by `/reload` in the TUI, `SIGHUP` to the daemon, or the manager's "Reload config" button.

1. Re-read `aciddog.wcl` and re-validate.
2. **Parse failure → old config stays active**, system error block pushed to the chat. Fix the WCL and try again. This is intentional — `/reload` is safe-by-default.
3. **Parse success →** swap config and reconcile:
   - **Agents**: deep-compare against resolved core/compaction/memory/skills/channel-membership. Per-agent `on_reload`:
     - `:ignore` (default) — running session unaffected; new prompt picks up at next `/compact` or `/reset`.
     - `:compact` — run the agent's compaction strategy now.
     - `:reset` — drop and re-seed now.
   - **Channels**: members list is source of truth. Ephemeral channels created via `/meeting` are preserved.
   - **Workflows**: engine reinitialised.
   - **Automatons + webhooks**: old instances cancelled, new ones respawned.
   - **Workers**: no reconciliation (template-only; new spawns pick up new config).
   - **Cron**: NOT reloaded — cron lives in `.aciddog/cron.json`, not WCL.

## Validation rules enforced at parse time

1. Exactly one `team` block.
2. Every `ai_core` reference must resolve (agents, workers, compactions, etc.).
3. `default_ai_core` on the team block must resolve.
4. `compaction = :harness` only allowed on `:claudeCode` cores.
5. Skill names match `^[a-z0-9][a-z0-9_-]{0,63}$`.
6. Memory bank references must resolve.
7. `supervisor` / `sysadmin` only on `agent` and (limited) `automaton` blocks — workers reject them.
8. `sysadmin = true` automatically sets `supervisor = true`.
9. Webhook `mount` must reference a declared `:webservice` automaton.
10. Webhook `path` must start with `/` and not collide with `/v1/rpc`, `/v1/events`, `/v1/health`.
11. `:webservice` automatons must use **either** `bind` (HTTPS, requires `tls_cert`/`tls_key`/`api_key_env`) **or** `socket_path` (Unix socket, no api key) — not both.
12. `:remote` automatons must use **either** `url` (HTTPS, requires `api_key_env`) **or** `socket_path` (no api key).
13. `automaton.env` entries must contain at least one `=`.
14. `:openAiChatgpt` `reasoningEffort` accepts only `low` / `medium` / `high` (NOT `minimal`).
15. `:jules` cores are valid only on `worker` templates.

## Gotchas (frequent footguns)

1. **Skill names are literal directory names.** Typos silently fail at runtime — the agent shows "skill not found" in its first-turn prompt rather than at parse time.
2. **`on_reload` is per-agent only.** Workers/automatons don't have it.
3. **`:harness` compaction is `claudeCode`-only.** Direct-API cores reject it at parse.
4. **`sysadmin` implies `supervisor`.** No need to set both; setting just `sysadmin = true` is enough.
5. **`default_ai_core` is fleet-wide.** Omit it only if every agent/worker declares its own.
6. **Webhooks need a `:webservice` automaton with a matching `mount` id.** No automaton, no webhook.
7. **Workers are templates, not instances.** Listing them in `can_create_workers` is what makes spawning possible — they don't run on their own.
8. **Channels can only contain agents.** Workers and automatons are silently rejected from `members`.
9. **Memory banks aren't auto-created.** Declaring one just registers it. Wipe a bank by deleting `.aciddog/memory/<id>/`.
10. **Reload is parse-failure-safe.** A bad edit doesn't crash the running fleet — the old config stays active until the next clean `/reload`.

## Source-of-truth pointers

- Schema: `/home/wil/dev/aciddog/wcl/aciddog.wcl`
- Worked example: `/home/wil/dev/aciddog/demo/aciddog.wcl`
- Parser & cross-block validation: `/home/wil/dev/aciddog/crates/aciddog/src/config.rs`
- Reload reconciliation: `/home/wil/dev/aciddog/crates/aciddog/src/tui/reload.rs`
- Skill name validation: `/home/wil/dev/aciddog/crates/aciddog/src/skills.rs` (`is_valid_skill_name`)
- Slash command dispatcher: `/home/wil/dev/aciddog/crates/aciddog/src/tui/commands.rs`
