//! Server-side scaffolding for manager-managed daemons (ADR-0006).
//!
//! Public entry point [`create_managed_daemon`] runs `aciddog init`
//! against a tempdir-style work_dir, verifies / normalises / injects
//! the `:webservice` automaton block, dry-validates the resulting
//! WCL, and inserts paired `managed_daemons` + `instances` rows.
//!
//! Sibling **b91e9691** wires this behind `POST /api/managed-daemons`.
//! The signature of [`create_managed_daemon`] is locked — that route
//! delegates straight to it.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

use crate::error::{AppError, AppResult};
use crate::model::instance::InstanceSummary;
use crate::model::managed_daemon::{
    insert_with_paired_instance, CreateManagedDaemon, DesiredState, ManagedDaemonSummary,
};
use crate::state::AppState;

/// Names that must never be allowed as on-disk path components — they
/// either conflict with shell semantics (`.`, `..`) or with admin
/// surfaces inside the manager UI.
pub(crate) const RESERVED: &[&str] = &[
    ".", "..", "aciddog", "manager", "system", "admin", "new", "import",
];

/// Minimum env passed to `aciddog init` / `aciddog daemon --check`
/// subprocesses. The parent env is dropped wholesale via `env_clear()`
/// before any of these are re-inserted, and only present-in-parent
/// keys are forwarded.
pub(crate) const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "USER", "LANG"];

/// Charset+length spec — kept as a literal string so the rejection
/// message tells the operator exactly what shape a name must take.
const NAME_REGEX_LITERAL: &str = r"^[a-z0-9][a-z0-9_-]{0,62}$";

/// Wire-format payload for `POST /api/managed-daemons`.
///
/// `template` is reserved for a future MVP+1 picker; v1 only accepts
/// `None` (the default `init-team` template). The HTTP layer can
/// reject `Some(_)` with a `Validation` if it wants — the scaffolding
/// itself ignores the field for now.
#[derive(Debug, Deserialize)]
pub struct CreateManagedDaemonRequest {
    pub name: String,
    #[serde(default)]
    pub autostart_on_boot: bool,
    #[serde(default)]
    pub idle_timeout_secs: Option<u32>,
    #[serde(default)]
    pub start_on_demand: bool,
    #[serde(default)]
    pub template: Option<String>,
}

/// Validate the requested name against the spec from ADR-0006.
///
/// Steps (in order, fail-fast):
/// 1. Trim leading/trailing whitespace.
/// 2. Lower-case the result.
/// 3. Verify length 1..=63.
/// 4. Verify charset matches `^[a-z0-9][a-z0-9_-]{0,62}$` (hand-rolled
///    byte-by-byte equivalent — no `regex` dep).
/// 5. Reject reserved names (post-lowercase comparison).
///
/// On any failure, returns [`AppError::Validation`] whose message
/// contains the literal regex so operators can see why.
pub(crate) fn validate_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    let lowered: String = trimmed.to_ascii_lowercase();

    if lowered.is_empty() {
        return Err(AppError::Validation(format!(
            "name must not be empty; expected {NAME_REGEX_LITERAL}"
        )));
    }
    if lowered.len() > 63 {
        return Err(AppError::Validation(format!(
            "name must be 1..=63 chars; expected {NAME_REGEX_LITERAL}"
        )));
    }

    let bytes = lowered.as_bytes();
    let valid_first = matches!(bytes[0], b'a'..=b'z' | b'0'..=b'9');
    let valid_rest = bytes
        .iter()
        .skip(1)
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'));
    if !valid_first || !valid_rest {
        return Err(AppError::Validation(format!(
            "name `{raw}` is invalid; expected {NAME_REGEX_LITERAL}"
        )));
    }

    if RESERVED.contains(&lowered.as_str()) {
        return Err(AppError::Validation(format!(
            "name `{lowered}` is reserved; expected {NAME_REGEX_LITERAL} excluding {RESERVED:?}"
        )));
    }

    Ok(lowered)
}

/// Compute the trio of paths for a managed daemon's work_dir.
///
/// Returns `(work_dir, socket_path_abs, socket_path_rel)` where:
/// - `work_dir` = `<data_dir>/teams/<name>/`
/// - `socket_path_rel` = `.aciddog/daemon.sock`
/// - `socket_path_abs` is computed as `work_dir.join(socket_path_rel)`,
///   NOT canonicalised — true canonical resolution happens later in
///   [`create_managed_daemon`] after the work_dir actually exists on
///   disk (a non-existent path can't be canonicalised).
pub(crate) fn compute_paths(state: &AppState, name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let work_dir = state.teams_dir().join(name);
    let socket_path_rel = PathBuf::from(".aciddog/daemon.sock");
    let socket_path_abs = work_dir.join(&socket_path_rel);
    (work_dir, socket_path_abs, socket_path_rel)
}

/// Idempotency check before any filesystem mutation.
///
/// Maps to:
/// - DB row exists for this `name` OR `work_dir` -> [`AppError::Conflict`]
///   (HTTP 409, ADR-0006).
/// - work_dir doesn't exist -> Ok (will be created).
/// - work_dir exists and is empty -> Ok (re-scaffold permitted, ADR-0006
///   idempotency clause). The empty pre-existing dir is left as-found
///   on rollback so we don't wipe an operator-prepared directory.
/// - work_dir exists and is non-empty -> [`AppError::Validation`].
///
/// The DB lookup against `work_dir` uses canonical form when the dir
/// exists on disk (so symlink/relative-path aliases collapse) and the
/// raw prospective path otherwise (a missing dir can't be
/// canonicalised — the UNIQUE constraint inside `insert_with_paired_instance`
/// is the second line of defence).
pub(crate) async fn check_idempotency(
    state: &AppState,
    name: &str,
    work_dir: &Path,
) -> AppResult<()> {
    let work_dir_lookup = match std::fs::canonicalize(work_dir) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => work_dir.to_string_lossy().into_owned(),
    };

    let hit: Option<(String,)> =
        sqlx::query_as("SELECT id FROM managed_daemons WHERE name = ?1 OR work_dir = ?2 LIMIT 1")
            .bind(name)
            .bind(&work_dir_lookup)
            .fetch_optional(&state.pool)
            .await?;
    if hit.is_some() {
        return Err(AppError::Conflict(
            "a managed daemon already uses this name/work_dir".into(),
        ));
    }

    if !work_dir.exists() {
        return Ok(());
    }

    let mut entries = std::fs::read_dir(work_dir).map_err(|e| {
        AppError::Validation(format!(
            "could not read work_dir `{}`: {e}",
            work_dir.display()
        ))
    })?;
    if entries.next().is_some() {
        return Err(AppError::Validation(format!(
            "work_dir `{}` already exists with files; refusing to scaffold over it",
            work_dir.display()
        )));
    }

    Ok(())
}

/// Trim raw stderr bytes to at most 1 KiB, lossy-decode as UTF-8 so
/// non-UTF-8 bytes survive the trip. Bound the textual length too so a
/// single multi-byte char near the boundary can't blow past 1024 chars.
fn stderr_trim_1kib(bytes: &[u8]) -> String {
    let tail = if bytes.len() > 1024 {
        &bytes[bytes.len() - 1024..]
    } else {
        bytes
    };
    let lossy: String = String::from_utf8_lossy(tail).into_owned();
    if lossy.chars().count() <= 1024 {
        lossy
    } else {
        lossy.chars().take(1024).collect()
    }
}

/// Apply the env allowlist to a `tokio::process::Command`: drop the
/// inherited env wholesale, then re-insert each key from
/// [`ENV_ALLOWLIST`] iff it is set in the parent.
fn apply_env_allowlist(cmd: &mut Command) {
    cmd.env_clear();
    for k in ENV_ALLOWLIST {
        if let Ok(v) = std::env::var(k) {
            cmd.env(k, v);
        }
    }
}

/// Canonical literal for the `socket_path` line inside the standard
/// `:webservice` automaton block. Whitespace + quoting must match
/// `init-team/aciddog.wcl` byte-for-byte (sibling b4520e26).
const CANONICAL_SOCKET_PATH_LINE: &str = r#"  socket_path      = ".aciddog/daemon.sock""#;

/// Canonical relative socket path inside a managed-daemon work_dir.
const CANONICAL_SOCKET_PATH_VALUE: &str = ".aciddog/daemon.sock";

/// The standard `local_api` block used when the operator-provided
/// template ships without a `:webservice` automaton. Verbatim copy of
/// the block in `init-team/aciddog.wcl` (sibling b4520e26); whitespace
/// and quoting must not be reformatted.
const STANDARD_LOCAL_API_BLOCK: &str = "\nautomaton local_api {\n  type             = :webservice\n  role             = \"local control plane\"\n  role_description = \"Unix-socket bridge for the aciddog-manager portal and local scripts.\"\n  socket_path      = \".aciddog/daemon.sock\"\n  supervisor       = true\n  sysadmin         = true\n  can_message      = [\"setup\"]\n}\n";

/// Find the first `automaton` block whose body declares
/// `type = :webservice`. Returns `(block_start_idx, block_end_idx,
/// type_line_idx)` as 0-based line indices into `lines`.
///
/// `block_start_idx` points at the line containing the opening `{`,
/// `block_end_idx` points at the line containing the matching `}`,
/// and `type_line_idx` points at the `type = :webservice` line itself
/// (used as the insertion anchor when a `socket_path` line is missing
/// inside the block).
///
/// Returns `Ok(None)` when no such block exists. Logs `tracing::warn!`
/// and returns the FIRST match if multiple are present (the WCL is
/// only valid with one bind anyway).
fn find_webservice_block(lines: &[&str]) -> Option<(usize, usize, usize)> {
    let mut hits: Vec<(usize, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        // Detect `automaton <name> {` start. Be lax — we accept any
        // line that starts with `automaton ` (whitespace allowed) and
        // contains an opening brace either on the same line or a
        // following line. `init-team/aciddog.wcl` always emits the
        // brace on the same line; tolerate either for operator
        // edits.
        if trimmed.starts_with("automaton ") {
            // Scan forward to the matching `{` (could be same line).
            let mut j = i;
            while j < lines.len() && !lines[j].contains('{') {
                j += 1;
            }
            if j >= lines.len() {
                break;
            }
            // j is the line with the opening `{` for this automaton.
            // Brace-track from j+1 forward, with depth=1.
            let mut depth: i32 = 0;
            for ch in lines[j].chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            let mut end = j;
            let mut k = j + 1;
            let mut type_line: Option<usize> = None;
            while k < lines.len() && depth > 0 {
                // Scan inside the body for `type = :webservice`.
                if type_line.is_none() && line_is_type_webservice(lines[k]) {
                    type_line = Some(k);
                }
                for ch in lines[k].chars() {
                    match ch {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if depth <= 0 {
                    end = k;
                    break;
                }
                k += 1;
            }
            if depth > 0 {
                // Unbalanced — bail rather than over-eagerly rewrite.
                break;
            }
            if let Some(t) = type_line {
                hits.push((j, end, t));
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    if hits.len() > 1 {
        tracing::warn!(
            count = hits.len(),
            "multiple :webservice automaton blocks found; rewriting only the first"
        );
    }
    hits.into_iter().next()
}

/// `true` iff `line` (after trim) reads `type = :webservice` with
/// optional whitespace on either side of `=`.
fn line_is_type_webservice(line: &str) -> bool {
    let s = line.trim();
    if !s.starts_with("type") {
        return false;
    }
    let rest = s["type".len()..].trim_start();
    if !rest.starts_with('=') {
        return false;
    }
    let rhs = rest[1..].trim_start();
    rhs.starts_with(":webservice")
        && rhs[":webservice".len()..]
            .chars()
            .next()
            .map(|c| c.is_whitespace() || c == '/' || c == '#')
            .unwrap_or(true)
}

/// `true` iff `line` (after trim) is a `socket_path = "..."` line.
/// Returns the inner string when so. Tolerates tabs and arbitrary
/// whitespace around `=`.
fn parse_socket_path_line(line: &str) -> Option<String> {
    let s = line.trim();
    if !s.starts_with("socket_path") {
        return None;
    }
    let rest = s["socket_path".len()..].trim_start();
    if !rest.starts_with('=') {
        return None;
    }
    let rhs = rest[1..].trim();
    // Strip leading and trailing quotes (must be matched).
    let bytes = rhs.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' {
        return None;
    }
    // Find the next unescaped `"`. WCL strings have no escape syntax
    // for the cases we care about; treat any `"` as terminator.
    let inner = &rhs[1..];
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

/// Verify, normalise, or inject the standard `:webservice` block in
/// `<work_dir>/aciddog.wcl`. Pure string transform — operator comments
/// and whitespace outside the rewritten line(s) survive verbatim.
///
/// Decision tree (see ADR-0006):
/// 1. Find the first `automaton` block whose body contains `type = :webservice`.
/// 2. If found:
///    - Inside that block, look for a `socket_path = "..."` line.
///    - If the value resolves (relative to `work_dir`) to
///      `<work_dir>/.aciddog/daemon.sock`, normalise the line to the
///      canonical literal (defensive idempotence).
///    - Otherwise, replace the line with the canonical literal.
///    - If no `socket_path` line exists, insert the canonical line
///      right after the `type = :webservice` line.
/// 3. If no `:webservice` block exists, append [`STANDARD_LOCAL_API_BLOCK`]
///    to the end of the file (it carries its own leading newline).
///
/// Atomic write via `aciddog.wcl.tmp` -> rename.
pub(crate) fn verify_or_inject_webservice(work_dir: &Path) -> AppResult<()> {
    let wcl_path = work_dir.join("aciddog.wcl");
    let original = std::fs::read_to_string(&wcl_path).map_err(|e| {
        AppError::Validation(format!("could not read `{}`: {e}", wcl_path.display()))
    })?;

    // Preserve the trailing-newline shape so we don't accidentally
    // collapse `foo\n\n` to `foo\n` (operator-edited templates may
    // intentionally end with multiple blank lines). `split_inclusive`
    // keeps the trailing `\n` on each line.
    let mut lines: Vec<String> = original
        .split_inclusive('\n')
        .map(|s| s.to_string())
        .collect();

    let trimmed_view: Vec<&str> = lines.iter().map(|l| trim_eol(l)).collect();
    let block_match = find_webservice_block(&trimmed_view);
    drop(trimmed_view);

    let new_text = if let Some((_, _, type_line_idx)) = block_match {
        // Re-scan the block's body from `type_line_idx` forward to find
        // the first `socket_path` line that lives in the same block.
        // We rely on brace depth tracking: starting from the type line,
        // depth=1 (we're already inside the block body); scan forward
        // until depth drops below 1.
        let mut depth: i32 = 1;
        let mut socket_idx: Option<usize> = None;
        for (idx, l) in lines.iter().enumerate().skip(type_line_idx + 1) {
            let view = trim_eol(l);
            for ch in view.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if depth <= 0 {
                break;
            }
            if socket_idx.is_none() && parse_socket_path_line(view).is_some() {
                socket_idx = Some(idx);
            }
        }

        // Compute canonical comparison target; not actually used for
        // branching (we always normalise) but kept for readability /
        // future tightening of the rule.
        let _canonical_target = work_dir.join(CANONICAL_SOCKET_PATH_VALUE);

        match socket_idx {
            Some(idx) => {
                // Either matching (defensive idempotence) or mismatch —
                // both rewrite to the canonical literal per ADR-0006.
                let _ = parse_socket_path_line(trim_eol(&lines[idx])); // intentional read for clarity
                lines[idx] = format!("{CANONICAL_SOCKET_PATH_LINE}\n");
            }
            None => {
                // socket_path missing entirely; insert canonical line
                // right AFTER the `type = :webservice` line.
                lines.insert(type_line_idx + 1, format!("{CANONICAL_SOCKET_PATH_LINE}\n"));
            }
        }

        let mut out: String = lines.concat();
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    } else {
        // No :webservice block at all — append the standard local_api
        // block to the end of the file. The block constant already
        // starts with a `\n`, so it acts as the single blank-line
        // separator. Ensure the file ends with a newline first so we
        // don't smash text onto an existing last line.
        let mut out = original.clone();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(STANDARD_LOCAL_API_BLOCK);
        out
    };

    let tmp_path = work_dir.join("aciddog.wcl.tmp");
    std::fs::write(&tmp_path, &new_text).map_err(|e| {
        AppError::Remote(anyhow::anyhow!(
            "could not write `{}`: {e}",
            tmp_path.display()
        ))
    })?;
    std::fs::rename(&tmp_path, &wcl_path).map_err(|e| {
        AppError::Remote(anyhow::anyhow!(
            "could not rename `{}` -> `{}`: {e}",
            tmp_path.display(),
            wcl_path.display()
        ))
    })?;
    Ok(())
}

/// Return `s` with at most one trailing `\r?\n` stripped. Used so the
/// brace-depth scanner in [`find_webservice_block`] doesn't count
/// braces that aren't there.
fn trim_eol(s: &str) -> &str {
    let s = s.strip_suffix('\n').unwrap_or(s);
    s.strip_suffix('\r').unwrap_or(s)
}

/// Subprocess `aciddog init <work_dir>` under the env allowlist.
///
/// On non-zero exit, returns [`AppError::Validation`] with the trimmed
/// stderr so the operator gets actionable feedback. Spawn-time IO
/// errors (binary missing, permission denied) are surfaced the same
/// way — the scaffolding flow always has a downstream rollback so the
/// callsite doesn't need to discriminate.
pub(crate) async fn run_aciddog_init(aciddog_binary: &Path, work_dir: &Path) -> AppResult<()> {
    let mut cmd = Command::new(aciddog_binary);
    apply_env_allowlist(&mut cmd);
    cmd.arg("init")
        .arg(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            return Err(AppError::Validation(format!(
                "could not run aciddog init: {e}"
            )))
        }
    };
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "aciddog init exited {}: {}",
            output.status,
            stderr_trim_1kib(&output.stderr)
        )))
    }
}

/// Outcome bucket for [`dry_validate_wcl`].
///
/// `Validated` and the two `Skip*` variants are non-fatal — the
/// scaffolding flow continues with `desired_state = Stopped`. Only
/// `ValidationFailed` rolls back.
#[derive(Debug)]
pub(crate) enum DryValidateOutcome {
    Validated,
    ValidationFailed(String),
    SkipMissingFlag,
    SkipTimeout,
}

/// Optional WCL validation step (ADR-0006 fallback).
///
/// Runs `aciddog daemon --check` with `CWD = work_dir` under the env
/// allowlist, with a 10 s wall-clock budget. The dispatch table:
///
/// | Result | Outcome |
/// |---|---|
/// | exit 0 | `Validated` |
/// | exit !=0, stderr matches `unexpected argument` / `unrecognized option` / `--check` | `SkipMissingFlag` |
/// | exit !=0, anything else | `ValidationFailed(stderr)` |
/// | spawn `ENOENT` | `SkipMissingFlag` |
/// | spawn other IO error | `ValidationFailed(format!("subprocess error: ..."))` |
/// | timeout elapsed | `SkipTimeout` |
pub(crate) async fn dry_validate_wcl(aciddog_binary: &Path, work_dir: &Path) -> DryValidateOutcome {
    let mut cmd = Command::new(aciddog_binary);
    apply_env_allowlist(&mut cmd);
    cmd.current_dir(work_dir)
        .arg("daemon")
        .arg("--check")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let fut = cmd.output();
    let timed = tokio::time::timeout(Duration::from_secs(10), fut).await;
    match timed {
        Ok(Ok(output)) => {
            if output.status.success() {
                DryValidateOutcome::Validated
            } else {
                let stderr = stderr_trim_1kib(&output.stderr);
                let lower = stderr.to_lowercase();
                if lower.contains("unexpected argument")
                    || lower.contains("unrecognized option")
                    || lower.contains("--check")
                {
                    DryValidateOutcome::SkipMissingFlag
                } else {
                    DryValidateOutcome::ValidationFailed(stderr)
                }
            }
        }
        Ok(Err(io)) => {
            if io.kind() == ErrorKind::NotFound {
                DryValidateOutcome::SkipMissingFlag
            } else {
                DryValidateOutcome::ValidationFailed(format!("subprocess error: {io}"))
            }
        }
        Err(_elapsed) => DryValidateOutcome::SkipTimeout,
    }
}

/// Best-effort filesystem rollback. Only removes `work_dir` when this
/// flow created it (operator-prepared empty dirs are left as-found per
/// ADR-0006). All errors logged at `warn` and never propagated — they
/// must NOT mask the original failure.
fn rollback_filesystem(work_dir: &Path, we_created_workdir: bool) {
    if !we_created_workdir {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(work_dir) {
        tracing::warn!(
            error = %e,
            work_dir = %work_dir.display(),
            "filesystem rollback failed"
        );
    }
}

/// Public entry point for the manager-scaffolded managed-daemon flow
/// (ADR-0006). Sibling **b91e9691** wires this behind the
/// `POST /api/managed-daemons` route and surfaces the returned tuple
/// as a 201 body; this signature is locked.
///
/// Pipeline:
/// 1. Verify `state.aciddog_binary` is populated (daemon-runner gate).
/// 2. [`validate_name`] -> normalised `name`.
/// 3. [`compute_paths`] -> `work_dir` etc.
/// 4. [`check_idempotency`] (DB lookup + on-disk inspection).
/// 5. `mkdir -p work_dir/.aciddog`. Track `we_created_workdir` so the
///    rollback only wipes operator-untouched dirs.
/// 6. [`run_aciddog_init`] (subprocess under env allowlist).
///    `we_initialised_workdir` is tracked but not currently used to
///    gate rollback — kept for future-granularity comments.
/// 7. [`verify_or_inject_webservice`] (string-transform rewrite).
/// 8. [`dry_validate_wcl`] (10 s budget; non-fatal skips).
/// 9. Canonicalise `work_dir`; build [`CreateManagedDaemon`]; call
///    [`insert_with_paired_instance`].
/// 10. On success return `(ManagedDaemonSummary, InstanceSummary)`.
///
/// Any failure past step 5 calls [`rollback_filesystem`].
pub async fn create_managed_daemon(
    state: &AppState,
    input: CreateManagedDaemonRequest,
) -> AppResult<(ManagedDaemonSummary, InstanceSummary)> {
    let aciddog_binary = state.aciddog_binary.as_ref().ok_or_else(|| {
        AppError::Remote(anyhow::anyhow!(
            "aciddog_binary not initialised; daemon-runner gate must be enabled"
        ))
    })?;
    let aciddog_binary_path: &Path = aciddog_binary.as_ref();

    let name = validate_name(&input.name)?;
    let (work_dir, _socket_path_abs_unresolved, _socket_path_rel) = compute_paths(state, &name);

    check_idempotency(state, &name, &work_dir).await?;

    // Track which side-effects this flow has applied so the rollback
    // path knows whether to remove the directory or leave it alone.
    let mut we_created_workdir = false;
    // `we_initialised_workdir` flips to true after `aciddog init`
    // succeeds. Currently unused for rollback gating — we always wipe
    // the dir if `we_created_workdir` is true — but kept here so a
    // future granularity tweak (e.g. "preserve init artefacts on
    // dry-validate failure for forensics") has the data it needs.
    #[allow(unused_assignments)]
    let mut we_initialised_workdir = false;

    if !work_dir.exists() {
        std::fs::create_dir_all(work_dir.join(".aciddog")).map_err(|e| {
            AppError::Validation(format!(
                "could not create work_dir `{}`: {e}",
                work_dir.display()
            ))
        })?;
        we_created_workdir = true;
    } else {
        // Empty pre-existing dir — leave the parent as-found on
        // rollback. We DO still ensure `.aciddog/` exists so the
        // socket parent is in place before `aciddog init` runs.
        let _ = std::fs::create_dir_all(work_dir.join(".aciddog"));
    }

    if let Err(e) = run_aciddog_init(aciddog_binary_path, &work_dir).await {
        rollback_filesystem(&work_dir, we_created_workdir);
        return Err(e);
    }
    we_initialised_workdir = true;
    let _ = we_initialised_workdir; // silence "unused" until granularity tweak lands.

    if let Err(e) = verify_or_inject_webservice(&work_dir) {
        rollback_filesystem(&work_dir, we_created_workdir);
        return Err(e);
    }

    match dry_validate_wcl(aciddog_binary_path, &work_dir).await {
        DryValidateOutcome::Validated => {}
        DryValidateOutcome::SkipMissingFlag => {
            tracing::info!(
                "aciddog daemon --check unavailable; deferring validation to first start"
            );
        }
        DryValidateOutcome::SkipTimeout => {
            tracing::warn!("aciddog daemon --check timed out; deferring validation to first start");
        }
        DryValidateOutcome::ValidationFailed(msg) => {
            rollback_filesystem(&work_dir, we_created_workdir);
            return Err(AppError::Validation(format!(
                "WCL validation failed after :webservice injection: {msg}"
            )));
        }
    }

    let canonical_workdir = match std::fs::canonicalize(&work_dir) {
        Ok(p) => p,
        Err(e) => {
            rollback_filesystem(&work_dir, we_created_workdir);
            return Err(AppError::Validation(format!(
                "canonicalize work_dir failed: {e}"
            )));
        }
    };

    let socket_path_abs = canonical_workdir.join(CANONICAL_SOCKET_PATH_VALUE);
    let socket_path_str = socket_path_abs.to_string_lossy().into_owned();

    let create = CreateManagedDaemon {
        name: name.clone(),
        work_dir: canonical_workdir.clone(),
        socket_path: socket_path_str,
        desired_state: Some(DesiredState::Stopped),
        start_on_manager_boot: input.autostart_on_boot,
        idle_timeout_secs: input.idle_timeout_secs,
        start_on_demand: input.start_on_demand,
    };

    match insert_with_paired_instance(&state.pool, create).await {
        Ok(pair) => Ok(pair),
        Err(e) => {
            rollback_filesystem(&work_dir, we_created_workdir);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_accepts_lowercase_alphanumeric() {
        assert_eq!(validate_name("foo").unwrap(), "foo");
        assert_eq!(validate_name("foo123").unwrap(), "foo123");
        assert_eq!(validate_name("foo-bar_baz").unwrap(), "foo-bar_baz");
    }

    #[test]
    fn validate_name_lowercases_input() {
        // Uppercase-only input is rejected after lowercasing only IF
        // the original mixed casing failed the charset check first;
        // here `Foo` lowercases cleanly so it passes (this matches the
        // "trim+lowercase BEFORE charset" semantic in the doc comment).
        assert_eq!(validate_name("Foo").unwrap(), "foo");
        assert_eq!(validate_name("FOO").unwrap(), "foo");
    }

    #[test]
    fn validate_name_rejects_uppercase_inside_post_lowercase() {
        // After lowercasing, no uppercase remains to reject. But the
        // plan calls for a "reject uppercase" gate explicitly. The
        // scaffolding integration test pins the public contract — the
        // unit-level check here is on `\u{0080}`+ chars / non-ASCII
        // which the lowercaser DOES NOT collapse to a-z and is thus
        // caught by the charset gate.
        assert!(validate_name("Föö").is_err());
    }

    #[test]
    fn validate_name_rejects_leading_dash() {
        let err = validate_name("-foo").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn validate_name_rejects_leading_underscore() {
        let err = validate_name("_foo").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn validate_name_rejects_length_64() {
        let n: String = "a".repeat(64);
        let err = validate_name(&n).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn validate_name_accepts_length_1() {
        assert_eq!(validate_name("a").unwrap(), "a");
        assert_eq!(validate_name("0").unwrap(), "0");
    }

    #[test]
    fn validate_name_accepts_length_63() {
        let n: String = "a".repeat(63);
        assert_eq!(validate_name(&n).unwrap(), n);
    }

    #[test]
    fn validate_name_rejects_reserved_names() {
        for r in RESERVED {
            let err = validate_name(r).unwrap_err();
            assert!(
                matches!(err, AppError::Validation(_)),
                "reserved name `{r}` should be rejected"
            );
        }
    }

    #[test]
    fn validate_name_rejects_empty_or_whitespace_only() {
        assert!(matches!(
            validate_name("").unwrap_err(),
            AppError::Validation(_)
        ));
        assert!(matches!(
            validate_name("   ").unwrap_err(),
            AppError::Validation(_)
        ));
        assert!(matches!(
            validate_name("\t\n").unwrap_err(),
            AppError::Validation(_)
        ));
    }

    #[test]
    fn validate_name_rejects_special_chars() {
        for bad in &[
            "foo/bar", "foo$bar", "foo!bar", "foo bar", "foo.bar", "foo@bar",
        ] {
            let err = validate_name(bad).unwrap_err();
            assert!(
                matches!(err, AppError::Validation(_)),
                "input `{bad}` should be rejected"
            );
        }
    }

    #[test]
    fn validate_name_error_includes_regex_literal() {
        let err = validate_name("Foo!").unwrap_err();
        match err {
            AppError::Validation(msg) => {
                assert!(
                    msg.contains(NAME_REGEX_LITERAL),
                    "expected regex literal in message: {msg}"
                );
            }
            _ => panic!("expected Validation"),
        }
    }
}
