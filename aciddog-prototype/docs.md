# aciddog documentation plan

Working notes for the eventual public docs site. **Not built yet** — architecture is
still settling. This file is the source of truth until we're ready to commit to a
site layout.

Target stack (when we build): Hugo + wdoc theme, Cloudflare Pages, sibling site to
the wcl docs. Forge brand kit (`design/`) supplies the visual identity.

---

## Goals

- Replace "the AI Skill is the only doc" with a navigable site for humans (and
  ideally agents).
- Keep documentation close enough to code that it can't silently rot.
- Match the operational feel of wcl's docs — same hosting, same release cadence,
  same review pattern — without forcing visual sameness if Forge fits aciddog
  better.
- Ship something useful at Phase 0 that scales as Phase 1+ lands.

## Non-goals (for now)

- No site build, no theme work, no CI wiring until the runtime architecture
  stabilises.
- No marketing landing page in v1 — docs first, marketing later.
- No i18n in v1.

---

## Information architecture (working draft — task-shaped)

1. **Get started** — install, first `aciddog.wcl`, `just run`, expected output.
2. **Concepts** — team, agent, worker, automaton, webservice, channel, cron,
   task board, memory. One short page each.
3. **Configuration reference** — exhaustive WCL block-by-block.
4. **Operating aciddog** — secrets / `.env`, idle timeouts, logging, hibernation,
   worker cwd precedence.
5. **Webservice & webhooks** — RPC, SSE events, health, webhook auth kinds,
   smoke-test recipes.
6. **Manager UI** — what it is, login, the seven tabs (lift from `DESIGN.md`).
7. **CLI reference** — auto-generated from `--help`.
8. **Recipes / how-tos** — spawn a Jules worker, auto-PR vs gated, wire a GitHub
   webhook, set up a review channel.
9. **Architecture** — runtime model, IPC, SSE channels, agent lifecycle states.
10. **FAQ + troubleshooting**.
11. **Changelog / roadmap** — Phase 0 → 1 → ….

Alternative shapes considered:
- Audience-shaped (Operators / Agent authors / Contributors) — cleaner for
  discovery, harder to keep DRY.
- Mirror wcl's IA verbatim — lowest cognitive load for users bouncing between
  the two sites.

---

## Source-of-truth strategy

The fastest way for docs to rot is to retype things that already live in code.
Rules of thumb for when we build:

- **Single-source the README** — Hugo includes `README.md` into `/get-started/`;
  do not maintain two copies.
- **Auto-publish rustdoc** for `aciddog` and `aciddog-manager` to `/api/`.
  `cargo doc --no-deps` in CI.
- **CLI reference from `--help`** — `just docs-cli` walks subcommands and emits
  Markdown into `content/cli/`. Commit the output so PRs surface drift.
- **WCL reference from the schema** — derive a JSON Schema from the serde
  config model and render it as the reference page (à la `cargo-deny`). If the
  serde model is the wrong shape for that, treat schema export as its own task.
- **Embed `demo/aciddog.wcl` literally** via Hugo `readFile` so the example is
  always the file CI exercises.
- **Round-trip code blocks** — tag executable blocks ` ```sh,run-in-ci` and run
  them from `scripts/test-docs.sh` so stale curl/openssl invocations break CI.

---

## Branding

Forge (`design/`) is a complete kit and matches the product's tone. When we
build:

- Use `design/colors_and_type.css` tokens as wdoc CSS overrides.
- IBM Plex Sans + JetBrains Mono.
- Dark default, honor `prefers-color-scheme: light`.
- `assets/logo-mark.svg` / `logo-wordmark.svg` for nav and favicon.
- Lucide icons for callouts. No emoji.
- 14px body, sentence case, dense — same vibe as the TUI/manager.

Open question: does the aciddog site visually match the wcl docs (cross-product
consistency) or use Forge (distinct product identity)? Lean Forge unless wcl's
theme is already strong.

---

## Generated / live content

- **Rustdoc** at `/api/`.
- **WCL config schema** at `/reference/wcl/`.
- **CLI tree** at `/reference/cli/`.
- **Mermaid diagrams**: agent state machine, worker lifecycle, webservice
  request path.
- **SSE event catalogue** — table of every event kind the manager subscribes
  to (lift from `DESIGN.md` §8).
- **Asciinema casts** for `just run`, spawning a worker, hitting a webhook.

---

## Hosting & CI (when ready)

- Cloudflare Pages, mirroring wcl's deploy pattern.
- Repo location — undecided:
  - **In-tree `docs/`** — docs ship with the code that defines them; bloats the
    runtime repo.
  - **Sister `aciddog-docs` repo** — clean separation; needs a sync step for
    generated content.
- GitHub Actions → `hugo` build → Cloudflare Pages. PR previews on by default.
- Versioned docs: at minimum `latest` and `next` so Phase-0 → Phase-1 changes
  don't silently rewrite history.
- Stable URL pattern matching wcl.

---

## Content gaps to fill (review notes against current README/DESIGN/SKILL)

The site needs to cover these — they are not in `README.md` today:

1. **Concept glossary** — `automaton`, `worker template`, `webservice`,
   `channel`, `memory` are name-dropped without definitions.
2. **Agent lifecycle states** — `Ready`, `Hibernated`, `Asleep`, `Errored`,
   `ever_prompted` referenced in the idle-timeout section without a diagram.
3. **Task board model** — `aciddog task add ... --worktree` is mentioned, but
   nothing explains status transitions, planning workflow, or
   `--planned-assignee` semantics.
4. **Cron** — exists in the tool, missing from the README entirely.
5. **Channels** — core to how the fleet talks; not in the README.
6. **Memory** — `DESIGN.md` mentions the Memory tab; no user-facing doc.
7. **Manager web UI** — exists, has a whole `DESIGN.md`, no user-facing doc.
8. **Worker remote backends (Jules)** — `autoCreatePr`, plan approval flow,
   properties JSON. Currently tribal knowledge.
9. **Security** — webservice TLS expectations, API-key rotation, webhook
   secret handling, secrets-loading order, what `.env` won't catch.
10. **Roadmap / phase status** — README says "Phase 0"; nothing tells a reader
    what Phase 1 looks like.

---

## Decisions still needed before we build

- **Docs-in-repo vs sister repo** — affects review workflow and CI time.
- **wdoc vs another Hugo theme** — does wdoc handle multi-section sidebar,
  versioning, and reference auto-include cleanly? Default to it if wcl is
  happy.
- **AI Skill survival** — keep `design/SKILL.md` and future agent skills as
  the canonical machine-readable surface; have the site include them rather
  than rephrase. Otherwise humans and agents drift.
- **Search** — Pagefind vs Algolia DocSearch. Pick early so content is
  authored search-friendly.
- **Feedback channel** — link to issues, or embed Giscus-style widget? Mirror
  wcl's choice.
- **Architecture lockdown** — the trigger for starting build work is the
  runtime architecture being stable enough that the Concepts and Architecture
  sections won't need rewrites every sprint.

---

## Status

- 2026-05-03 — File created. Site build deferred until architecture settles.
