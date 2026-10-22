# Spike — `aciddog-config` library extraction (A) vs `aciddog check` subcommand (B)

**Task:** d143b7a2 (this spike) · **Sibling consumer:** 580b77a1 (manager scaffolding step 5; has its own fallback) · **Status:** Recommendation only — investigation, no code.

## Problem

Task `d143b7a2` asks whether `aciddog-manager` should dry-validate a WCL document by linking a hypothetical `aciddog-config` library (path **A**) or by shelling out to a new `aciddog check <path>` subcommand (path **B**). The same parse surface is also wanted by sibling task `580b77a1` step 5 (which carries a fallback so this spike is non-blocking) and by a future in-browser editor. The whole question exists because the workspace `Cargo.toml` excludes `crates/aciddog-manager` today: the `aciddog` crate transitively links the C `sqlite3` library via `cozo`'s sqlite storage backend, while `aciddog-manager` links the same `sqlite3` via `sqlx-sqlite`, and cargo refuses two `links = "sqlite3"` packages in one resolver output. Path A is only viable if the parse surface can be carved out of `aciddog` without dragging cozo (or any of `rusqlite` / `libsqlite3-sys` / `sqlx-sqlite` / `fastembed`) along with it.

## Findings

### Parse-path imports inventory (`crates/aciddog/src/config.rs`)

`use` lines (entire file, lines 1–6):

```
config.rs:1 use std::path::{Path, PathBuf};
config.rs:2 use std::time::Duration;
config.rs:4 use serde::{Deserialize, Serialize};
config.rs:5 use thiserror::Error;
config.rs:6 use wcl_lang::{parse, ParseOptions, Value};
```

Fully-qualified paths reached from `load_config` / `parse_config` / private `parse_*` helpers / `impl AciddogConfig`:

- `std::fs::read_to_string` (config.rs:1406, in `load_config`)
- `std::path::{Path, PathBuf}`, `std::time::Duration`, `std::collections::{HashSet, BTreeMap}` (throughout)
- `wcl_lang::{parse, ParseOptions, Value, BlockRef}` (throughout `parse_*` helpers)
- `serde::{Deserialize, Serialize}` (derive macros on every config struct)
- `thiserror::Error` (derive macro on `ConfigError`)
- `crate::cron::CronTz` and its `::Local`, `::parse` items (config.rs:449, :458, :459, :1821, :3573, :3586) plus `crate::cron::parse_payload` / `crate::cron::parse_schedule` (config.rs:2380, :2386)
- `crate::ai::tools::BUILT_IN_TOOL_NAMES`, `crate::ai::copilot_cli::COPILOT_ALLOWED_TOOLS`, `crate::ai::gemini_cli::GEMINI_ALLOWED_TOOLS`, `crate::ai::codex_cli::CODEX_ALLOWED_TOOLS` (config.rs:2700, :2705, :2709, :2713 — read as `&[&'static str]` constants only)
- `crate::skills::is_valid_skill_name` (config.rs:2793 — pure regex check)

Grep absences confirmed (with `multiline=true`):

- `std::env` — none
- `dotenvy` — none
- `cozo` — none
- `rusqlite` — none
- `fastembed` — only as a doc-comment word at config.rs:471 describing a config field; no import, no call
- `tokio` — none (no async, no `tokio::*`)
- `reqwest` — none
- `dirs` — none
- `crate::skill` — only `crate::skills::is_valid_skill_name` (single pure-validator call)
- `crate::memory` — none
- No other `crate::` import beyond the `cron` / `ai` / `skills` constants enumerated above

### `cargo tree` summary

`cargo tree -e=normal --no-default-features -p aciddog` and `cargo tree -e=normal -p aciddog` produced byte-identical 1157-line trees (the workspace member exposes no features, so `--no-default-features` is a no-op). Filter `'^[├└]── (cozo|sqlx|rusqlite|libsqlite3-sys|fastembed|ort) '` shows only `cozo v0.7.6` and `fastembed v5.13.4` as direct deps; `ort v2.0.0-rc.12` appears once, transitively under `fastembed`. Plain `grep` of the entire tree returns **zero** hits for `rusqlite`, `libsqlite3-sys`, or `sqlx`. The only sqlite presence is via cozo: `cozo → sqlite v0.32.0 → sqlite3-sys v0.15.2 → sqlite3-src v0.5.1` (the `stainless` sqlite stack, not `libsqlite3-sys`, but it links the same native `sqlite3` C library — that is the source of the workspace-level `links = "sqlite3"` collision with `sqlx-sqlite`). Verbatim subtrees in the Appendix.

### Per-crate dep audit table

One row per entry in `crates/aciddog/Cargo.toml [dependencies]` (51 entries, lines 9–59).

| crate | reason it appears in `aciddog` | reachable from `parse_config` → `AciddogConfig`? (y/n) | notes |
|---|---|---|---|
| `ratatui` | TUI rendering | n | UI layer; never imported by `config.rs` |
| `crossterm` | terminal events | n | UI layer; never imported by `config.rs` |
| `wcl_lang` | WCL parser | y | `config.rs:6 use wcl_lang::{parse, ParseOptions, Value};` — required by extraction |
| `tokio` | async runtime (daemon, IPC, HTTP) | n | no `tokio::*` anywhere in `config.rs` |
| `futures` | stream combinators | n | runtime; never imported by `config.rs` |
| `tracing` | structured logging | n | runtime; never imported by `config.rs` |
| `tracing-subscriber` | tracing setup | n | runtime; never imported by `config.rs` |
| `tracing-appender` | log file rotation | n | runtime; never imported by `config.rs` |
| `clap` | CLI parsing | n | lives in `cli.rs` / `main.rs`; never imported by `config.rs` |
| `anyhow` | binary-side error helper | n | never imported by `config.rs` (which uses `thiserror` only) |
| `thiserror` | `ConfigError` derive | y | `config.rs:5 use thiserror::Error;` |
| `serde` | derive on every config struct | y | `config.rs:4 use serde::{Deserialize, Serialize};` |
| `serde_json` | wire-protocol JSON | n | never imported by `config.rs` |
| `uuid` | session/run ids | n | never imported by `config.rs`; cron module uses it but config touches only `cron::CronTz` |
| `async-trait` | AI-core trait macros | n | never imported by `config.rs` |
| `tokio-stream` | async streams | n | never imported by `config.rs` |
| `pulldown-cmark` | markdown render | n | TUI/web; never imported by `config.rs` |
| `unicode-width` | display width | n | TUI; never imported by `config.rs` |
| `similar` | diff rendering | n | TUI; never imported by `config.rs` |
| `base64` | wire encoding | n | never imported by `config.rs`; skills.rs uses it but config touches only `is_valid_skill_name` |
| `shlex` | shell quoting | n | runtime; never imported by `config.rs` |
| `tempfile` | temp dirs (tests, scratch) | n | never imported by `config.rs` |
| `dotenvy` | `.env` loader | n | runtime env; **never imported by `config.rs`** (a key sqlite-clean property) |
| `cozo` | memory store backend | n | **sqlite-link source** behind `crate::memory`, which `config.rs` does not call |
| `fastembed` | embedding loader | n | mentioned only in doc-comment at `config.rs:471`; no import, no call (also pulls `ort`) |
| `chrono` | time types | n | runtime/cron; never imported by `config.rs` directly |
| `cron` | cron schedule parser | n | wrapped behind `crate::cron::*`; `config.rs` calls only that module's `parse_*` helpers |
| `iana-time-zone` | tz lookup for cron | n | runtime; never imported by `config.rs` |
| `reqwest` | HTTP client | n | runtime; never imported by `config.rs` |
| `glob` | file globs | n | runtime/skills; never imported by `config.rs` |
| `include_dir` | static asset embed | n | TUI/web; never imported by `config.rs` |
| `grep-searcher` | grep tool impl | n | tool layer; never imported by `config.rs` |
| `grep-regex` | grep tool regex | n | tool layer; never imported by `config.rs` |
| `grep-matcher` | grep tool matcher | n | tool layer; never imported by `config.rs` |
| `bytes` | byte buffers | n | wire layer; never imported by `config.rs` |
| `sha2` | hashing | n | wire/skills; never imported by `config.rs` |
| `sha1` | hashing | n | wire; never imported by `config.rs` |
| `hmac` | webhook signing | n | webhook server; never imported by `config.rs` |
| `subtle` | constant-time compare | n | webhook server; never imported by `config.rs` |
| `hex` | encoding | n | webhook server; never imported by `config.rs` |
| `dirs` | XDG paths | n | runtime; never imported by `config.rs` |
| `urlencoding` | url encoding | n | webhook server; never imported by `config.rs` |
| `humantime` | duration parsing | n | runtime/cron; never imported by `config.rs` |
| `axum` | web framework | n | server; never imported by `config.rs` |
| `axum-server` | HTTPS server | n | server; never imported by `config.rs` |
| `hyper` | HTTP plumbing | n | server; never imported by `config.rs` |
| `hyper-util` | hyper helpers | n | server; never imported by `config.rs` |
| `http-body-util` | http body helpers | n | server; never imported by `config.rs` |
| `tower` | middleware | n | server; never imported by `config.rs` |
| `rustls` | TLS | n | server; never imported by `config.rs` |
| `rustls-pemfile` | PEM loader | n | server; never imported by `config.rs` |

Bottom line: **3 y rows** (`wcl_lang`, `serde`, `thiserror`) + an implicit fourth (`serde_derive`, pulled by `serde`'s `derive` feature — same row in the table since the manifest declares it via `serde = { features = ["derive"] }`). **48 n rows.** None of the y rows transitively reaches `cozo`, `rusqlite`, `libsqlite3-sys`, `sqlx-sqlite`, `fastembed`, or `ort` (verified against the `wcl_lang` subtree at lines 1092–1157 of the cargo tree: pulls `base64`, `csv`, `glob`, `hcl-rs`, `indexmap`, `miette`, `nom`, `quick-xml`, `regex`, `rmp-serde`, `serde`, `serde_json`, `serde_yaml_ng`, `sha2`, `thiserror`, `toml` — all sqlite-free).

A small intra-`aciddog` bridge does exist on the parse path: `config.rs` reads four `&[&'static str]` constants from `crate::ai::*` (lines 2700–2713), one regex validator from `crate::skills::is_valid_skill_name` (line 2793), and the `crate::cron::CronTz` type plus `parse` / `parse_payload` / `parse_schedule` helpers (lines 449, 458, 459, 1821, 2380, 2386, 3573, 3586). These are pure data + small validators with sqlite-clean own deps (`chrono`, `cron`, `uuid`, `serde`, `thiserror` for cron; `base64`, `thiserror` for skills; nothing for the AI-core constant arrays). They are a migration cost for path A — see Migration sketch — not a sqlite-clean failure.

## Sqlite-clean verdict

**Parse path is sqlite-clean — recommend (A).** The three deps reachable from `parse_config` → `AciddogConfig` (`wcl_lang`, `serde` + `serde_derive`, `thiserror`) all transitively avoid `cozo`, `rusqlite`, `libsqlite3-sys`, `sqlx-sqlite`, and `fastembed` / `ort` (sole sqlite path in `aciddog`'s tree is `cozo v0.7.6 → sqlite v0.32.0 → sqlite3-sys v0.15.2 → sqlite3-src v0.5.1` at tree lines 449–454, and cozo is unreachable from `config.rs` per the audit table).

## Rubric

| axis | (A) library extraction | (B) `aciddog check` subcommand |
|---|---|---|
| build-time cost added to `aciddog-manager` | one extra small crate compile: `serde` + `serde_derive` + `thiserror` + `wcl_lang` (~17 transitive crates per the wcl_lang subtree, all already in workspace caches); ≪10 s incremental on a warm target dir | zero |
| runtime cost per validate | direct function call: parse one WCL file in microseconds; no syscalls beyond `read_to_string` | fork+exec the `aciddog` binary (≈10–30 ms cold on Linux), plus binary cold-load (the daemon binary is multi-MB with `cozo` + `fastembed` linked in); loses warm caches between calls |
| ergonomics for sibling 580b77a1 step 5 | typed `aciddog_config::parse_and_validate(&Path) -> Result<AciddogConfig, ConfigError>`; matches `ConfigError` variants directly; no string parsing | parse stderr text + interpret exit code (0 / 1); `ConfigError` variants degrade to a single error string; no machine-readable error stream |
| ergonomics for future browser editor | same `aciddog_config` crate compiles to WASM (`wcl_lang` is pure-rust + `serde`/`thiserror`; no `tokio`, no FFI); editor calls `parse_and_validate` in-page and renders typed errors | impossible without a backend round-trip — the browser cannot fork+exec `aciddog`; would force a server endpoint just to validate |
| CI surface | one new crate to build/test in the workspace (`cargo test -p aciddog-config`); existing `aciddog` tests keep passing because aciddog re-exports the new crate | no new crate, but a new clap subcommand to test (parse-arg cases, exit-code matrix, stderr-format snapshot); one new integration test per `ConfigError` variant the manager wants to react to |
| risk if parse path later grows env coupling | the lib API forces the discipline: any new `std::env::var` or `dotenvy::from_path` in `aciddog-config` is a compile-time visible diff; manager links pick it up immediately and any sqlite re-introduction breaks the workspace build | hidden — subcommand keeps working even if `parse_config` quietly grows env reads or pulls in new crates; manager just shells out and never sees the regression |
| failure mode if duplicate-`links` constraint ever resurfaces | extraction breaks at workspace-resolve time; revisit by quarantining the offending dep behind a feature flag, or fall back to (B) at that point | irrelevant — manager and aciddog stay in separate resolver outputs (today's status quo); the conflict cannot trigger |

## Recommendation

**Path A — extract `crates/aciddog-config`.** The audit confirms the parse path is sqlite-clean (3 y-rows, 48 n-rows; zero edges to cozo/sqlx/rusqlite/libsqlite3-sys/fastembed/ort from any y-row), so the dependency reason for excluding `aciddog-manager` from the workspace does **not** carry into the extracted crate. Five of the seven rubric axes favour A (build cost is negligible-vs-zero, but A wins runtime cost, ergonomics for both consumers, env-coupling discipline; B only wins on raw build cost and on the irrelevant-today `links` failure mode); the deciding axis is **ergonomics for the future browser editor**, where B is structurally impossible (no fork+exec in a browser tab) while A compiles to WASM directly. The intra-`aciddog` bridge (3 small constants/validators across `crate::cron`, `crate::ai::*`, `crate::skills`) is a one-time migration cost, not a sqlite-clean failure, and is covered in the Migration sketch.

## Migration sketch (path A)

A separate follow-up task implements the steps below. Shape only — no code in this spike.

- **`crates/aciddog-config/Cargo.toml`** — new crate, `name = "aciddog-config"`, `edition = "2021"`. `[dependencies]`: `serde = { version = "1", features = ["derive"] }`, `thiserror = "1"`, `wcl_lang = "0.9.0-alpha"`. No other deps. Inherits `[lints] workspace = true`.
- **`crates/aciddog-config/src/lib.rs`** — public surface: `pub use self::types::{AciddogConfig, Team, Agent, Worker, Automaton, Webhook, Workflow, AiCore, /* … all current pub items from aciddog::config */};` plus `pub use self::error::ConfigError;` and the entry point `pub fn parse_and_validate(path: &std::path::Path) -> Result<AciddogConfig, ConfigError>` (calls today's `load_config` body unchanged). The body of `crates/aciddog/src/config.rs` (types, parsers, validators, `ConfigError`, `USER_INBOX_ID`) moves verbatim into this crate.
- **Intra-`aciddog` bridge fixups** (one-time, all sqlite-clean): inline the four `&[&'static str]` AI-core tool-name constants (`BUILT_IN_TOOL_NAMES`, `COPILOT_ALLOWED_TOOLS`, `GEMINI_ALLOWED_TOOLS`, `CODEX_ALLOWED_TOOLS`) into `aciddog-config` (or move them into a tiny `aciddog-tool-names` shared crate if the daemon-side `ai::*` modules want to keep ownership); inline `is_valid_skill_name` (single regex) into `aciddog-config`; move `CronTz` definition + `parse` + `parse_payload` + `parse_schedule` from `crate::cron` into `aciddog-config` (their deps `chrono` / `cron` / `uuid` / `serde` / `thiserror` are all sqlite-clean) — the daemon-side `crate::cron` re-exports from `aciddog_config::cron` so the rest of `aciddog` keeps compiling unchanged.
- **`crates/aciddog/src/lib.rs`** — replace the `pub mod config;` line with `pub use aciddog_config as config;` so every existing `aciddog::config::*` consumer (CLI dispatch, daemon, tests) keeps working with no further edits.
- **Workspace `Cargo.toml`** — add `crates/aciddog-config` to `members`. The exclusion of `crates/aciddog-manager` stays in place (cozo is still in `aciddog`); the new crate is a sibling of both, depended on by both.
- **`crates/aciddog-manager/Cargo.toml`** — add `aciddog-config = { path = "../aciddog-config" }` to `[dependencies]`.
- **Sibling 580b77a1 step 5** — replace its fallback with `aciddog_config::parse_and_validate(path)` and match on `ConfigError` variants directly (no stderr/exit-code parsing).

## Out-of-scope

This spike does not perform the extraction or add the subcommand. A follow-up task implements the chosen path.

## Appendix — raw `cargo tree` excerpts

Tree captured with `cargo tree -e=normal -p aciddog` from the spike worktree on 2026-05-04 (591 packages locked; output identical to `--no-default-features` per a `diff -q` of both runs, since the `aciddog` workspace member exposes no features).

### Search hits for the deciding crates

```
$ cargo tree -e=normal -p aciddog 2>&1 | grep -nE '^├── (cozo|sqlx|rusqlite|libsqlite3-sys|fastembed|ort) '
249:├── cozo v0.7.6
511:├── fastembed v5.13.4
```

```
$ grep -nE '(rusqlite|libsqlite3-sys|sqlx)' aciddog-tree.txt
(no matches)
```

```
$ grep -nE 'sqlite' aciddog-tree.txt
449:│   ├── sqlite v0.32.0
451:│   │   └── sqlite3-sys v0.15.2
453:│   │       └── sqlite3-src v0.5.1
454:│   ├── sqlite3-src v0.5.1
```

### `cozo v0.7.6` subtree (direct children only — full subtree spans tree lines 249–484)

```
├── cozo v0.7.6
│   ├── aho-corasick v1.1.4
│   ├── approx v0.5.1
│   ├── base64 v0.21.7
│   ├── byteorder v1.5.0
│   ├── casey v0.4.2 (proc-macro)
│   ├── chrono v0.4.44 (*)
│   ├── chrono-tz v0.8.6
│   ├── crossbeam v0.8.4
│   ├── csv v1.4.0
│   ├── document-features v0.2.12 (proc-macro)
│   ├── either v1.15.0
│   ├── env_logger v0.10.2
│   ├── fast2s v0.3.1
│   ├── itertools v0.12.1
│   ├── jieba-rs v0.6.8
│   ├── lazy_static v1.5.0
│   ├── log v0.4.29
│   ├── miette v5.10.0
│   ├── minreq v2.14.1
│   ├── ndarray v0.15.6
│   ├── num-traits v0.2.19
│   ├── ordered-float v4.6.0
│   ├── pest v2.8.6
│   ├── pest_derive v2.8.6 (proc-macro)
│   ├── priority-queue v1.4.0
│   ├── quadrature v0.1.2
│   ├── rand v0.8.6
│   ├── rayon v1.12.0
│   ├── regex v1.12.3 (*)
│   ├── rmp v0.8.15
│   ├── rmp-serde v1.3.1
│   ├── rmpv v1.3.1
│   ├── rust-stemmers v1.2.0
│   ├── rustc-hash v1.1.0
│   ├── serde v1.0.228 (*)
│   ├── serde_bytes v0.11.19
│   ├── serde_derive v1.0.228 (proc-macro) (*)
│   ├── serde_json v1.0.149 (*)
│   ├── sha2 v0.10.9
│   ├── smallvec v1.15.1 (*)
│   ├── smartstring v1.0.1
│   ├── sqlite v0.32.0
│   │   ├── libc v0.2.186
│   │   └── sqlite3-sys v0.15.2
│   │       ├── libc v0.2.186
│   │       └── sqlite3-src v0.5.1
│   ├── sqlite3-src v0.5.1
│   ├── swapvec v0.3.0
│   ├── thiserror v1.0.69 (*)
│   ├── twox-hash v1.6.3 (*)
│   ├── unicode-normalization v0.1.25
│   └── uuid v1.23.1
```

The `cozo v0.7` features used here are `["minimal", "rayon", "requests"]` (per `crates/aciddog/Cargo.toml:32`, `default-features = false`). The native `links = "sqlite3"` declaration arrives via `sqlite3-src v0.5.1` (the `stainless` sqlite stack), which is the source of the workspace-level conflict with `aciddog-manager`'s `sqlx = { features = [..., "sqlite", ...] }` (`crates/aciddog-manager/Cargo.toml:26`).

### `fastembed v5.13.4` subtree — `ort` extract (full subtree spans tree lines 511–939)

```
├── fastembed v5.13.4
│   ├── anyhow v1.0.102
│   ├── hf-hub v0.5.0          # http hub fetcher (HTTPS, dirs, native-tls/openssl, …)
│   ├── image v0.25.8
│   ├── ndarray v0.17.2
│   ├── ort v2.0.0-rc.12
│   │   ├── ndarray v0.17.2 (*)
│   │   ├── ort-sys v2.0.0-rc.12
│   │   └── smallvec v1.15.1 (*)
│   ├── safetensors v0.7.0
│   ├── serde v1.0.228 (*)
│   ├── serde_json v1.0.149 (*)
│   └── tokenizers v0.22.2     # ahash, compact_str, derive_builder, esaxx-rs, onig, rayon-cond, …
```

`fastembed` is the ONNX runtime path (`ort` + `ort-sys`). It is a top-level dep of `aciddog` only because the daemon-side `crate::memory` embedding loader uses it; it is **not** reachable from `crate::config`.

### `sqlx-*`, `rusqlite`, `libsqlite3-sys` subtrees

Not present in the `aciddog` tree (`grep -E '(rusqlite|libsqlite3-sys|sqlx)' aciddog-tree.txt` returns zero matches across all 1157 lines). The duplicate-`links` conflict that excludes `aciddog-manager` from the workspace exists at the *workspace* level (`aciddog` brings sqlite via cozo's `sqlite3-src`; `aciddog-manager` brings sqlite via `sqlx-sqlite`); the extracted `aciddog-config` crate brings neither.

### `wcl_lang v0.9.0-alpha` subtree (the only y-row external dep beyond `serde` and `thiserror`)

```
└── wcl_lang v0.9.0-alpha
    ├── base64 v0.22.1
    ├── csv v1.4.0 (*)
    ├── glob v0.3.3
    ├── hcl-rs v0.18.7
    ├── indexmap v2.14.0 (*)
    ├── miette v7.6.0
    ├── nom v8.0.0 (*)
    ├── quick-xml v0.37.5
    ├── regex v1.12.3 (*)
    ├── rmp-serde v1.3.1 (*)
    ├── serde v1.0.228 (*)
    ├── serde_json v1.0.149 (*)
    ├── serde_yaml_ng v0.10.0
    ├── sha2 v0.10.9 (*)
    ├── thiserror v2.0.18 (*)
    └── toml v1.1.2+spec-1.1.0
```

No `cozo`, no `sqlx`, no `rusqlite`, no `libsqlite3-sys`, no `fastembed`, no `ort` — confirms the sqlite-clean verdict.
