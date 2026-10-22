# aciddog-ipc-types extraction — feasibility assessment

Spike (investigation only) for task `80e9d334-aa5f-45f5-9280-90a55a061ff0`.
No code, no `Cargo.toml` edits, no module moves. The deliverable is this
single document. Actual extraction is scheduled as a follow-up task —
see Migration sketch and Open questions.

## 1. Summary & recommendation

**Recommendation: DEFER.** The locked entanglement rubric returns
`E = 0.50 = 50.00 %`, comfortably inside the `≥ 30 %` *defer* bucket.

The extraction's prize is real but small: 4 of 8 public types in
`crates/aciddog/src/agent_ipc/` (`RequestCmd`, `Response`, `SkillFile`,
`HistorySelector`) are pure-serde *and* mirrored hand-for-hand in
`aciddog-manager/src/ipc.rs`. Together they account for **454 LOC**
(11.35 %) of the 3 999 LOC the module spans. The rest of the module is
either runtime-entangled (`AgentSocketEvent` holds a
`oneshot::Sender<Response>`; `Bound` owns a `JoinHandle` and `mpsc`
endpoints), an envelope the manager intentionally skips (`Request`
exists only to stamp `agent_id` server-side, which the manager already
does), or non-wire helper code (`preamble.rs` is 1 822 LOC of tooling
preamble text generation and contributes one borrow-shaped struct,
`ChannelInfo<'a>`, that has no `Serialize`/`Deserialize` derives).

The grep-shaped cycle audit is *clean* — `agent_ipc/` has zero
`use crate::…` lines, so it is already a leaf module. Nothing in the
intra-crate import graph blocks extraction.

The reason to defer is not an architectural blocker; it is a cost/
benefit one. Extracting four types removes ~84 LOC of hand-mirrored
shapes from `aciddog-manager/src/ipc.rs` (`RequestCmd` mirror is the
bulk; `IpcResponse`, `SkillFile`, `HistorySelector` are tens of lines
each). That is worth doing eventually, but the rubric was deliberately
locked before measurement to keep this honest: at 50 % E the extraction
is more carrying-cost than payoff today, and the manager's lockstep
comment can wait. Re-run the spike when the manager surface grows
materially (e.g. supervisor-side daemon-runner adds new wire variants,
which the planning brief flagged as the trigger) — the *additions* will
pile up on the manager-relevant side of the rubric and may push E below
30 %.

The 5-phase migration sketch in §8 below is recorded so the follow-up
ticket can pick it up unchanged on a re-run.

## 2. Audit set

Re-globbed at audit time (do not trust the plan's snapshot list).

`crates/aciddog/src/agent_ipc/**/*.rs`:

- `crates/aciddog/src/agent_ipc/client.rs` (36 LOC)
- `crates/aciddog/src/agent_ipc/mod.rs` (1 876 LOC)
- `crates/aciddog/src/agent_ipc/preamble.rs` (1 822 LOC)
- `crates/aciddog/src/agent_ipc/server.rs` (265 LOC)

Total: **3 999 LOC** (`wc -l`).

Manager reference set (read-only, for the manager-relevance filter):

- `crates/aciddog-manager/src/ipc.rs` (480 LOC)
- `crates/aciddog-manager/src/wire_channel.rs` (52 LOC)
- `crates/aciddog-manager/src/wire_chat.rs` (224 LOC)
- `crates/aciddog-manager/src/wire_cron.rs` (97 LOC)
- `crates/aciddog-manager/src/wire_inbox.rs` (28 LOC)
- `crates/aciddog-manager/src/wire_memory.rs` (113 LOC)
- `crates/aciddog-manager/src/wire_repos.rs` (125 LOC)
- `crates/aciddog-manager/src/wire_tasks.rs` (200 LOC)
- `crates/aciddog-manager/src/wire_team.rs` (92 LOC)
- `crates/aciddog-manager/src/wire_workflow.rs` (110 LOC)

The manager's `wire_*.rs` files are mirrors of payload types living at
`crates/aciddog/src/wire_*.rs` (siblings of `agent_ipc/`, **not**
children of it) and so are out of scope for this spike. They are noted
in §11 as the natural follow-up extraction target once `agent_ipc`'s
own envelope types are sorted.

## 3. Per-file classification

Each `.rs` file under `crates/aciddog/src/agent_ipc/` is enumerated;
every public `struct`/`enum` gets a row. Constants and private but
load-bearing items are recorded as notes on the parent file. Public
functions are *items* but not *types*; they appear in notes only and
are excluded from the rubric denominator (per task TODO 3 / §7 below).

| file | type / item | serde-pure | manager-relevant | runtime entanglement / note |
|---|---|---|---|---|
| `agent_ipc/mod.rs` | `Request` (struct, line 16) | y | n | Plain serde envelope: `agent_id: Uuid` + `#[serde(flatten)] cmd: RequestCmd`. The manager does **not** mirror this struct: `aciddog-manager/src/ipc.rs:17` explicitly notes "the webservice stamps its own `agent_id` server-side — we don't send one", so the manager serializes `RequestCmd` directly into the `/v1/rpc` body. Extraction-safe in isolation, but unused on the manager side. |
| `agent_ipc/mod.rs` | `RequestCmd` (enum, line 24) | y | y | Hand-mirrored byte-for-byte at `aciddog-manager/src/ipc.rs:20-331` (subset of the 60-odd variants the daemon understands; manager keeps only what `/v1/rpc` consumers call). Pure data: primitives, `String`, `Uuid`, `Vec<Uuid>`, `Option<…>`, `BTreeMap<String, serde_json::Value>`, plus a flattened `HistorySelector` and a `Vec<SkillFile>`. The serde tag attribute (`#[serde(tag = "cmd", rename_all = "snake_case")]`) is the load-bearing wire contract. |
| `agent_ipc/mod.rs` | `SkillFile` (struct, line 724) | y | y | Two-string struct (`rel_path`, `contents_b64`). Mirrored byte-for-byte at `aciddog-manager/src/ipc.rs:336-339`. Used by `RequestCmd::SkillWrite` and the daemon's `SkillsList` reply payload. |
| `agent_ipc/mod.rs` | `HistorySelector` (enum, line 735) | y | y | Internally-tagged enum (`#[serde(tag = "select")]`) with All / Last / LastMessage / Kind variants, all primitive-or-`String`. Mirrored at `aciddog-manager/src/ipc.rs:346-351`. Flattened into `RequestCmd::AgentHistory{,Json}` as the selector field. |
| `agent_ipc/mod.rs` | `Response` (struct, line 751) | y | y | Plain serde reply envelope (`ok: bool`, `message`, `error`, `data: Option<serde_json::Value>`). Mirrored as `IpcResponse` at `aciddog-manager/src/ipc.rs:354-365` (renamed in the manager to disambiguate from `axum::response::Response`; structurally identical). |
| `agent_ipc/mod.rs` | `AgentSocketEvent` (struct, line 800) | n | n/a | Holds `request: Request` + `reply: oneshot::Sender<Response>`. The `oneshot` field is a Tokio in-process channel handle — pure runtime, never serialized. Whole point of the type is the in-process handoff between the socket reader (`server::handle_connection`) and the request handler in `tui::app`. Stays in the daemon crate. |
| `agent_ipc/mod.rs` | `ENV_AGENT_ID` (`pub const &str`, line 10) | n/a (not a type) | n/a (not a type) | Note: `&'static str` constant `"ACIDDOG_AGENT_ID"`. Trivially extractable but unused by the manager (it is a daemon-side env var name read by spawned agent processes). Excluded from rubric. |
| `agent_ipc/mod.rs` | `ENV_AGENT_SOCKET` (`pub const &str`, line 13) | n/a (not a type) | n/a (not a type) | Note: `&'static str` constant `"ACIDDOG_AGENT_SOCKET"`. Same posture as `ENV_AGENT_ID`. Excluded from rubric. |
| `agent_ipc/preamble.rs` | `ChannelInfo<'a>` (struct, line 28) | y | n | Per the rubric we ignore the `&'a str` borrow-form question and treat it as serde-pure-shaped. **However**, the type has no `Serialize`/`Deserialize` derives and is never written to the socket — it is an in-process argument shape used by `cli_preamble`/`tag_preamble` to render the system-generated `<tools>` block shown to LLMs. Not on the wire; manager has no equivalent. |
| `agent_ipc/preamble.rs` | `Row<'a>` (private, line 1124) | n/a | n/a | Note: private helper for the preamble table renderer; not load-bearing for any wire envelope. Excluded from rubric. |
| `agent_ipc/preamble.rs` | `USER_RECIPIENT_ID` (private const, line 39) | n/a | n/a | Note: `"user"` literal, used only to inject the operator inbox into the messaging recipients list inside `cli_preamble`/`tag_preamble`. Not load-bearing for any wire type. Excluded from rubric. |
| `agent_ipc/preamble.rs` | `cli_preamble` / `tag_preamble` / `worker_preamble` / `tag_worker_preamble` (public fns) | n/a | n/a | Note: text generators, not types. They build LLM tool-documentation strings; entirely orthogonal to the wire crate. Stay in the daemon crate (they need access to runtime-team metadata, ai-core flags, etc.). Excluded from rubric. |
| `agent_ipc/client.rs` | (no public `struct`/`enum`) | n/a | n/a | Note: the only public surface is `pub async fn send(&Path, &Request) -> anyhow::Result<Response>`. Pure runtime: opens a `UnixStream`, writes one JSON line, reads one JSON line. Stays in the daemon crate (Tokio + `anyhow` dependencies). |
| `agent_ipc/server.rs` | `Bound` (struct, line 17) | n | n/a | Holds `events: mpsc::Receiver<AgentSocketEvent>`, `events_tx: mpsc::Sender<AgentSocketEvent>`, `accept_handle: JoinHandle<()>`. Drop impl aborts the listener task and unlinks the socket file. Pure runtime — exists only to keep the listener alive and expose its event stream. Stays in the daemon crate. The whole `server.rs` module (Tokio listener, `accept_loop`, `handle_connection`) is non-extractable for the same reason. |

**Total public `struct`/`enum` types in audit**: **8**
(`Request`, `RequestCmd`, `SkillFile`, `HistorySelector`, `Response`,
`AgentSocketEvent`, `ChannelInfo<'a>`, `Bound`).

## 4. Manager-relevance filter

Cross-reference of the **serde-pure = y** rows from §3 against
`aciddog-manager/src/ipc.rs` and the manager's `wire_*.rs` set.

| serde-pure type | manager-relevant | mirrored at | how the manager uses it |
|---|---|---|---|
| `Request` | n | (no mirror) | Manager skips the envelope entirely. `aciddog-manager/src/ipc.rs:1-9` declares the manager mirrors `aciddog::agent_ipc::RequestCmd` only; the daemon's `/v1/rpc` handler stamps `agent_id` server-side, so the manager never constructs a `Request`. |
| `RequestCmd` | y | `aciddog-manager/src/ipc.rs:20-331` | Manager defines a same-named enum with `#[serde(tag = "cmd", rename_all = "snake_case")]` covering the supervisor-relevant variant subset (~45 variants). Posted as the JSON body of `POST /v1/rpc`. Drift between the two enums is the canonical tech-debt the spike addresses. |
| `SkillFile` | y | `aciddog-manager/src/ipc.rs:336-339` | Mirrored byte-for-byte. Used as the inner element of `RequestCmd::SkillWrite { name, files: Vec<SkillFile> }`. |
| `HistorySelector` | y | `aciddog-manager/src/ipc.rs:346-351` | Mirrored byte-for-byte. Flattened into `RequestCmd::AgentHistoryJson { agent, selector }`. |
| `Response` | y | `aciddog-manager/src/ipc.rs:354-365` (named `IpcResponse`) | Identical struct layout — just renamed in the manager to avoid colliding with `axum::response::Response`. Decodes every reply from `/v1/rpc`. |
| `ChannelInfo<'a>` | n | (no mirror) | In-process preamble helper. Never appears on the wire. |

**Manager-relevant + serde-pure subset**: `RequestCmd`, `SkillFile`,
`HistorySelector`, `Response` — **4 types**.

The manager's `wire_*.rs` files mirror sibling files at
`crates/aciddog/src/wire_*.rs`, **not** anything under `agent_ipc/`.
They were read to confirm none of them transitively pull a type back
out of `agent_ipc/` — they don't. Out of scope here; see §11.

## 5. Surface-area metrics

**Method**: per-type manual count using `awk 'NR>=$start && NR<=$end' …
| grep -cv '^\s*///'`. The `start`/`end` window covers each type's
`#[derive(...)]` line, any `#[serde(...)]` attribute lines on the type
or its fields, and the entire body (open brace through closing brace
inclusive). Doc comments (`///`) are excluded per the rubric. Impl
blocks (e.g. `impl Response { ok / ok_data / err }`), `mod tests`, and
standalone helper functions are excluded.

| type | line range | non-doc LOC |
|---|---|---|
| `RequestCmd` (mod.rs) | 22-717 (incl. 2 attribute lines) | 431 |
| `SkillFile` (mod.rs) | 723-727 | 5 |
| `HistorySelector` (mod.rs) | 733-748 (incl. 2 attribute lines) | 8 |
| `Response` (mod.rs) | 750-763 (incl. 1 attribute line) | 10 |
| **Total extractable subset** | | **454** |

**Total `agent_ipc/` LOC** (all four files, `wc -l`): **3 999**.

**Ratio (extractable / total)**: 454 / 3 999 = **11.35 %**.

Reading: ~89 % of `agent_ipc/`'s LOC budget is *not* on the wire — it
is preamble text generation (`preamble.rs` alone is 1 822 LOC), the
Tokio listener / connection plumbing in `server.rs`, the doc-comment
explanations on every variant of `RequestCmd`, and the `impl Response
{ ok, ok_data, err }` helpers. The wire types themselves are a
relatively thin slice that *would* be cheap to move if the rubric
landed in the proceed bucket.

## 6. Cyclic-dep audit

**Outbound** — `^use crate::` across `crates/aciddog/src/agent_ipc/**/*.rs`:

> **No matches.** `agent_ipc/` is a leaf module with zero in-crate
> couplings to its siblings. (Within-subtree imports use
> `use super::…` — `client.rs:7` and `server.rs:11` pull `Request`,
> `Response`, and `AgentSocketEvent` from the parent `mod.rs`, which
> is internal coupling and irrelevant to extraction.) Outbound
> external imports are limited to `serde`, `tokio::*`, `uuid::Uuid`,
> `anyhow`, `tracing`, `std::*`, plus `serde_json::Value` — nothing
> from elsewhere in `aciddog`.

**Inbound** — `^use crate::agent_ipc` across `crates/aciddog/src/**/*.rs`
(excluding `agent_ipc/` itself):

| consumer module | usage |
|---|---|
| `crates/aciddog/src/webservice/mod.rs:28` | `use crate::agent_ipc::AgentSocketEvent;` |
| `crates/aciddog/src/webservice/webhook.rs:32` | `use crate::agent_ipc::{AgentSocketEvent, Request as IpcRequest, RequestCmd};` |
| `crates/aciddog/src/webservice/http.rs:30` | `use crate::agent_ipc::{ … };` (multi-line — pulls `Request`, `Response`, `RequestCmd`, etc.) |
| `crates/aciddog/src/tui/app.rs:25` | `use crate::agent_ipc::{self, AgentSocketEvent, HistorySelector, RequestCmd, Response};` |
| `crates/aciddog/src/tui/boot.rs:25` | `use crate::agent_ipc;` |
| `crates/aciddog/src/tui/compaction.rs:16` | `use crate::agent_ipc;` |
| `crates/aciddog/src/skills.rs:19` | `use crate::agent_ipc::SkillFile;` |
| `crates/aciddog/src/ai/tag_call.rs:8` | `use crate::agent_ipc::{RequestCmd, Response};` |
| `crates/aciddog/src/ai/copilot_cli.rs:36,39` | preamble functions + `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/codex_cli.rs:93,96` | preamble functions + `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/claude_code.rs:16,19` | preamble functions + `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/openai_chatgpt/core.rs:22` | `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/openai_api/mod.rs:43` | `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/anthropic_api/mod.rs:34` | `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/gemini_cli.rs:48,51` | preamble functions + `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/ai/jules/mod.rs:35,36` | preamble functions + `AgentSocketEvent`, `Request`, `RequestCmd`, `Response` |
| `crates/aciddog/src/remote_automaton/mod.rs:21` | `AgentSocketEvent`, `Request as IpcRequest`, `RequestCmd` |
| `crates/aciddog/src/remote_automaton/proxy.rs:24` | `Request as IpcRequest`, `Response as IpcResponse` |
| `crates/aciddog/src/remote_automaton/http_client.rs:25` | `RequestCmd`, `Response as IpcResponse` |

**Edge graph**:

```
agent_ipc/  ──(no outbound use crate::*)──▶  ∅
   ▲
   │
   ├─ webservice (mod, webhook, http)
   ├─ tui (app, boot, compaction)
   ├─ skills
   ├─ ai (tag_call, copilot_cli, codex_cli, claude_code,
   │      openai_chatgpt, openai_api, anthropic_api,
   │      gemini_cli, jules)
   └─ remote_automaton (mod, proxy, http_client)
```

A blocking cycle would require *both* directions: a sibling module
importing `crate::agent_ipc::…` **and** `agent_ipc/` importing back
from that sibling. The outbound side is empty — there are no
candidates.

**Conclusion: no cycles that block extraction.**

Notes for the follow-up extraction task:

- Consumers mostly want `RequestCmd` + `Response` + a handful import
  `AgentSocketEvent` (in-process channel-event shape) and the preamble
  functions. After Phase 2 (re-export stubs), every one of these
  consumer call sites compiles unchanged because `aciddog::agent_ipc::*`
  still resolves; after Phase 4 the runtime-only consumers (webservice,
  ai cores, remote_automaton, tui) keep importing
  `crate::agent_ipc::AgentSocketEvent` from in-tree, while wire-type
  consumers (`skills`, `ai/tag_call`, the `Request as IpcRequest`
  aliases in webhook/proxy/http_client) move to
  `aciddog_ipc_types::…`.
- The preamble functions stay in-tree even after the wire-type
  extraction; the `ai::*` modules import them from
  `crate::agent_ipc::preamble::…` exactly as today.

## 7. Entanglement rubric & calculation

Locked thresholds (recorded in decisions/`b7995eb0` before measurement):

- `E ≥ 30 %` → **defer**
- `E < 10 %` → **proceed**
- `10 % ≤ E < 30 %` → **partial** (extract only the manager-relevant subset)

Numerator = count of types from §3 that are runtime-entangled **OR**
not manager-relevant.

| type | counted in numerator? | reason |
|---|---|---|
| `Request` | ✓ | not manager-relevant (manager skips the envelope) |
| `RequestCmd` | — | serde-pure + manager-relevant |
| `SkillFile` | — | serde-pure + manager-relevant |
| `HistorySelector` | — | serde-pure + manager-relevant |
| `Response` | — | serde-pure + manager-relevant |
| `AgentSocketEvent` | ✓ | runtime-entangled (`oneshot::Sender<Response>`) |
| `ChannelInfo<'a>` | ✓ | not manager-relevant (preamble helper, no derives) |
| `Bound` | ✓ | runtime-entangled (`mpsc` + `JoinHandle`) |

Denominator = total public `struct`/`enum` types in `agent_ipc/` = **8**.

Numerator = **4**.

`E = 4 / 8 = 0.5000 = **50.00 %**.`

Bucket: **`E ≥ 30 %` → DEFER.**

Recommendation matches the bucket.

Honest commentary (does not change the recommendation): the high E is
driven less by *technical* blockers and more by what counts as a
"type". Two of the four numerator entries (`Request`, `ChannelInfo`)
are tiny and serde-pure-ish; they bloat the numerator because they
live in `agent_ipc/` but aren't *wire shapes the manager consumes*.
Were the manager to start mirroring `Request` (it deliberately does
not — see §4), or the rubric to scope itself to "wire types" rather
than "all public types in `agent_ipc/`", the calculation would land in
the *partial* or even *proceed* bucket. The rubric was locked
up-front specifically to prevent that re-scoping after measurement, so
the recommendation here is **defer** as written.

## 8. Migration sketch (phases 0-4 with gates)

This sketch is provisional and recorded for the eventual follow-up
ticket. Today's recommendation is to *defer*; revisit when the manager
mirror grows further (e.g. supervisor-side daemon-runner adds wire
variants) and the rubric returns *proceed* / *partial*.

- **Phase 0 — Spike** (this task): assessment + sketch + decisions
  memory entry.
  Gate: `docs/spikes/aciddog-ipc-types-extraction.md` landed on `main`
  and a decisions-bank entry stored.

- **Phase 1 — Empty crate**: create `crates/aciddog-ipc-types/` as a
  workspace member (`Cargo.toml`, `src/lib.rs`); declare `serde`,
  `serde_json`, `uuid` from `[workspace.dependencies]`; add to root
  `[workspace.members]`. No type moves yet.
  Gate: `cargo check -p aciddog-ipc-types` passes on an otherwise-empty
  lib + the new workspace member is enumerated by `cargo metadata`.

- **Phase 2 — Move pure-serde, manager-relevant types**: relocate
  `RequestCmd`, `SkillFile`, `HistorySelector`, `Response` from
  `aciddog::agent_ipc` into `aciddog_ipc_types::*`. Leave
  `pub use aciddog_ipc_types::{RequestCmd, SkillFile, HistorySelector,
  Response};` re-export stubs in `crates/aciddog/src/agent_ipc/mod.rs`
  so every existing consumer call site compiles unchanged.
  Gate: `cargo check --workspace` clean; aciddog still re-exports the
  moved types.

- **Phase 3 — Manager swap**: rewrite `aciddog-manager/src/ipc.rs` to
  `use aciddog_ipc_types::{RequestCmd, SkillFile, HistorySelector,
  IpcResponse}` (or keep the rename `pub use Response as IpcResponse`),
  delete the hand-mirrored `RequestCmd`/`SkillFile`/`HistorySelector`/
  `IpcResponse` definitions, and remove the standing 5-9 line lockstep
  comment from every `wire_*.rs` and from `ipc.rs`.
  Gate: `aciddog-manager/src/ipc.rs` no longer declares wire types and
  the lockstep comment is gone; `cargo check --workspace` and
  `cargo test -p aciddog-manager` are green.

- **Phase 4 — Drop the re-export stubs**: delete the
  `pub use aciddog_ipc_types::*` re-exports from
  `aciddog/src/agent_ipc/mod.rs`; update each in-tree consumer
  (`skills`, `ai::tag_call`, the `Request as IpcRequest` aliases in
  `webservice::webhook`/`http`, `remote_automaton::*`) to import from
  `aciddog_ipc_types`. `aciddog`'s `Cargo.toml` adds
  `aciddog-ipc-types = { workspace = true }`. `agent_ipc/` retains only
  the runtime-entangled bits: `Request` envelope, `AgentSocketEvent`,
  `ENV_AGENT_*`, `client.rs`, `server.rs`, `preamble.rs`.
  Gate: no `pub use aciddog_ipc_types::*` re-export survives in
  `aciddog/src/agent_ipc/`; `cargo check --workspace` and
  `cargo test --workspace` are green.

## 9. Out of scope

Explicitly excluded from this spike — and from any follow-up extraction
task that adopts this sketch verbatim:

- **Doing the extraction.** Phases 1-4 are sketched above but executed
  under a separate, follow-up ticket once the rubric returns *proceed*
  or *partial*.
- **Moving runtime types out of `aciddog::agent_ipc`.** Specifically:
  `AgentSocketEvent` (uses `oneshot::Sender<Response>`), the entirety
  of `client.rs` (Tokio `UnixStream` + `anyhow`), and the entirety of
  `server.rs` (Tokio listener / `mpsc` / `JoinHandle`) stay in the
  daemon crate. They are **not** wire types — they are the in-process
  socket plumbing.
- **Extracting `manager-relevant = n` types.** `Request` (the envelope
  the manager skips), `ChannelInfo<'a>` (preamble helper), and the
  `ENV_AGENT_*` constants stay in `aciddog::agent_ipc` even after a
  proceed-bucket extraction — the manager has no use for them.
- **Touching `aciddog-manager/src/wire_*.rs` or `ipc.rs` in this
  spike.** Read-only reference set.
- **Redesigning the cozo / sqlx-sqlite `links = "sqlite3"` collision.**
  The whole point of the proposed extraction is to side-step it; this
  spike does not redesign storage.
- **Extracting the daemon-side `crates/aciddog/src/wire_*.rs` payload
  types** (`wire_chat`, `wire_workflow`, `wire_inbox`, …). Those are
  siblings of `agent_ipc/`, not children, and the manager mirrors them
  in its own `wire_*.rs` set. They are the *next* extraction target —
  see §11 — but explicitly out of scope here.

## 10. Prior decisions

Searched the `decisions` memory bank for prior context against the
queries below. Each query returned one match; the matches all resolve
to the same single planning entry for this spike (the planner's
write-up that locked the rubric and out-of-scope list before
measurement).

Searched queries:

- `agent_ipc workspace extraction`
- `ipc-types crate split`
- `sqlite cozo links collision`
- `aciddog-manager wire types mirror`

Match (single entry, top hit on every query):

- **`b7995eb0-3126-4f34-b423-b58245be7341`** — *aciddog-ipc-types
  extraction spike — investigation-only with up-front rubric (E≥30%
  defer, <10% proceed, 10-30% partial); deliverable
  docs/spikes/aciddog-ipc-types-extraction.md; per-file classification
  table is the load-bearing artifact*. Tags:
  `aciddog-manager, agent_ipc, planning, spike, workspace`. The
  planning record this spike implements; locks the rubric thresholds
  used in §7.

The `sqlite cozo links collision` query also surfaced
`18de93c9-…-d8e264daae2469` (`aciddog-manager managed_daemons sqlx
model + migration (ADR-0002)`), which notes that
`PRAGMA foreign_keys = ON` is *not* set and `links = "sqlite3"`
collisions are managed at the call-site level today — context, but not
a prior decision *about* the agent_ipc extraction.

No prior decisions about the extraction itself beyond the planning
entry. The standing tech-debt comment at
`crates/aciddog-manager/src/ipc.rs:5-9` (re-cited in
`wire_chat.rs`, `wire_tasks.rs`, `wire_workflow.rs`, `wire_inbox.rs`)
is the originating signal.

## 11. Open questions for follow-up

1. **Re-run trigger**: the planning brief flagged "supervisor-side
   daemon-runner adds new wire variants" as the natural moment to
   re-run this spike. Add an explicit "wire variants added" counter or
   a dated tickler so the follow-up surfaces when the manager mirror
   has materially grown rather than when someone happens to re-read the
   tech-debt comment.

2. **Daemon-side `wire_*.rs` extraction**: the daemon's
   `crates/aciddog/src/wire_{channel,chat,inbox,memory,repos,skills,
   team,workflow}.rs` are siblings of `agent_ipc/`, not children, and
   are likewise hand-mirrored in `aciddog-manager/src/wire_*.rs`. Once
   `aciddog-ipc-types` exists, those payload types are the obvious
   next move — file as a sibling spike (`aciddog-ipc-types extraction:
   wire payload types`) at the same time the envelope extraction is
   approved.

3. **`Request` envelope policy**: should the manager start sending the
   `Request { agent_id, cmd }` envelope instead of bare `RequestCmd`?
   That would unify the wire shape on both sides, push `Request` into
   the manager-relevant column, and lower E below the *defer*
   threshold by one type. Scope decision for the next extraction
   spike.

4. **`Response` rename in the manager**: if/when the extraction lands,
   does `aciddog-manager` keep the `IpcResponse` rename to dodge the
   `axum::response::Response` collision, or import the original name
   under a local alias? Stylistic; flag for the implementer.

5. **`preamble.rs` separation**: the file is 1 822 LOC of LLM
   tooling-text generation that has nothing to do with IPC types. It
   sits in `agent_ipc/` only because it documents the IPC surface. A
   cosmetic follow-up could move it to `crates/aciddog/src/preamble/`
   (sibling of `agent_ipc/`), which would also tighten the rubric on a
   re-run by removing `ChannelInfo<'a>` from the type-count denominator
   and numerator. Independent of the extraction.

6. **`SkillFile` collision risk**: `aciddog-manager/src/ipc.rs:336`
   already declares `SkillFile`, identical layout. Picking a different
   (or the same) name in the extracted crate is straightforward, but
   note that the manager's `wire_skills`-shaped types
   (`SkillSummaryWire`, `SkillContentsWire`) are inline in `ipc.rs`
   rather than in a `wire_skills.rs` file — the manager has *no*
   `wire_skills.rs` even though the daemon does
   (`crates/aciddog/src/wire_skills.rs`). Worth deciding whether to
   extract those alongside `SkillFile`.
