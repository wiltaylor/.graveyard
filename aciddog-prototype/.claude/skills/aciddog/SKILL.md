---
name: aciddog
description: Reference for working with aciddog — authoring/updating WCL team configs (agents, workers, automatons, ai_cores, channels, workflows, etc.), using the aciddog TUI (keybindings, slash commands, daemon lifecycle), and setting up the aciddog web manager (run commands, env vars, team editor, reload-config). Loads when the user asks about aciddog, aciddog.wcl, the TUI, the manager, agents/workers/automatons, or any aciddog-specific concept.
user-invocable: true
argument-hint: <topic — team | tui | manager>
allowed-tools:
  - Read
  - Write
  - Edit
  - Glob
  - Grep
  - Bash
---

<overview>
Aciddog is a multi-agent orchestrator (Rust workspace at the repo root) configured by a single WCL file (`aciddog.wcl`). It runs either as a TUI or a headless daemon, and ships with a SolidJS web manager that can edit and reload team configs remotely. This skill bundles every fact needed to author/update a team WCL, drive the TUI, and stand up the manager — sourced from the canonical schema at `wcl/aciddog.wcl`, the TUI command dispatcher, and the manager crate.
</overview>

<variables>
- `${CLAUDE_SKILL_DIR}`: this skill's directory (`.claude/skills/aciddog/`).
- `REPO_ROOT`: `/home/wil/dev/aciddog` — the aciddog workspace.
- `LIVE_SCHEMA`: `${REPO_ROOT}/wcl/aciddog.wcl` — source-of-truth schema. Always cross-check before writing real WCL.
- `DEMO_TEAM`: `${REPO_ROOT}/demo/aciddog.wcl` — the most complete worked example in the tree.
</variables>

<routing>
Read the reference file(s) that match the user's topic, then answer from them. Do not improvise WCL fields — every field name in this skill is verified against `${LIVE_SCHEMA}`.

| Topic                                                          | Read                                                                 |
|----------------------------------------------------------------|----------------------------------------------------------------------|
| Authoring or editing a team WCL (any block type)               | `reference/team-wcl.md` then `reference/wcl-fields.md` for field-level detail |
| Scaffolding a new team file from scratch                       | `reference/example-team.wcl` (annotated copy-paste starter)          |
| TUI usage — keybindings, slash commands, screens, daemon       | `reference/tui.md`                                                   |
| Web manager — setup, run commands, env vars, team editor flow  | `reference/manager.md`                                               |

When the user asks about a specific WCL field or block, also open `${LIVE_SCHEMA}` and confirm the field still exists there before quoting semantics — the schema is the source of truth and may have drifted from this skill's reference.
</routing>

<boundaries>
<always>
- Cross-check WCL fields against `${LIVE_SCHEMA}` before writing or editing real `.wcl` files.
- Use existing `ai_core`, `compaction`, and `memory_bank` ids declared elsewhere in the file rather than introducing duplicates.
- Validate skill names against `^[a-z0-9][a-z0-9_-]{0,63}$` before adding them to an agent's `skills = [...]`.
- Tell the user how to apply changes: `/reload` in the TUI, `kill -HUP <pid>` for the daemon, or the manager's "Reload config" button.
</always>

<ask>
- Before introducing a new `ai_core` variant when an existing one might fit.
- Before changing `supervisor` / `sysadmin` flags — these grant privileged RPCs.
- When the user's intent could map to either an `agent` (persistent) or a `worker` (ephemeral template).
</ask>

<never>
- Invent WCL fields that are not in `${LIVE_SCHEMA}`.
- Reference catalog/marketplace skills by URL or plugin id inside WCL — skills are disk-resident under `.aciddog/skills/<name>/` and referenced by directory name only.
- Use the `:harness` compaction strategy on any non-`claudeCode` core (parser rejects it).
- Add channel members that aren't declared `agent` blocks (workers and automatons cannot join channels).
</never>
</boundaries>
