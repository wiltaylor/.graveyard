use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::client::Client;
use crate::error::AppResult;
use crate::ipc::RequestCmd;
use crate::routes::instances::load_row_for_routes;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AgentRow {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub parent: Option<String>,
    pub ai_core: Option<String>,
    /// Top-of-list pin + 👑 decoration in the runtime UI. Mirrors the
    /// `leader` flag on the agent / automaton config block. False for
    /// every row when the upstream daemon speaks the legacy 5-column
    /// header (no `leader` column emitted).
    #[serde(default)]
    pub leader: bool,
    /// Optional `group` label from the agent / automaton / worker
    /// config block. `None` when the daemon speaks the 5- or 6-column
    /// legacy header (no `group` column emitted) or when the row's
    /// block has no `group` set.
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentListResponse {
    /// Parsed rows, if the remote replied ok and the table parsed.
    pub agents: Vec<AgentRow>,
    /// Whether the remote's reply was `ok=true`. If false, `error` is
    /// populated and `agents` is empty.
    pub ok: bool,
    /// Raw `message` string from the IPC reply — useful as a fallback if
    /// the plaintext table format at `aciddog/src/tui/app.rs` changes.
    pub raw: Option<String>,
    /// Populated when the remote returned `ok=false`.
    pub error: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AgentListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::AgentList).await?;

    if !resp.ok {
        return Ok(Json(AgentListResponse {
            agents: Vec::new(),
            ok: false,
            raw: resp.message,
            error: resp.error,
        }));
    }

    let raw = resp.message.unwrap_or_default();
    let agents = parse_agent_table(&raw);
    Ok(Json(AgentListResponse {
        agents,
        ok: true,
        raw: Some(raw),
        error: None,
    }))
}

/// Parse the tab-separated plaintext returned by
/// `handle_admin_agent_list` (see `aciddog/src/tui/app.rs`). Tolerant
/// of three header shapes for cross-version compatibility:
///   - Legacy 5-column: `id\tkind\tstatus\tparent\tai_core` — every
///     row gets `leader = false`, `group = None`.
///   - 6-column: `id\tkind\tstatus\tparent\tai_core\tleader` — the
///     trailing column is parsed as a `true` / `false` bool, `group`
///     defaults to `None`.
///   - 7-column: `id\tkind\tstatus\tparent\tai_core\tleader\tgroup` —
///     the trailing `group` cell uses the same `-`-means-None
///     convention as the `parent` and `ai_core` columns.
///
/// Empty input yields an empty Vec. Rows whose column count doesn't
/// match the chosen header shape are skipped.
pub(crate) fn parse_agent_table(raw: &str) -> Vec<AgentRow> {
    let mut lines = raw.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let expected_cols = match header {
        "id\tkind\tstatus\tparent\tai_core" => 5,
        "id\tkind\tstatus\tparent\tai_core\tleader" => 6,
        "id\tkind\tstatus\tparent\tai_core\tleader\tgroup" => 7,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != expected_cols {
            continue;
        }
        let nullable = |s: &str| if s == "-" { None } else { Some(s.to_string()) };
        let leader = if expected_cols >= 6 {
            parts[5] == "true"
        } else {
            false
        };
        let group = if expected_cols >= 7 {
            nullable(parts[6])
        } else {
            None
        };
        out.push(AgentRow {
            id: parts[0].to_string(),
            kind: parts[1].to_string(),
            status: parts[2].to_string(),
            parent: nullable(parts[3]),
            ai_core: nullable(parts[4]),
            leader,
            group,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_table() {
        // Legacy 5-column header (older daemon, no leader column).
        // Every row falls back to leader = false.
        let raw = "id\tkind\tstatus\tparent\tai_core\n\
                   supervisor\tagent\tidle\t-\tclaude_code\n\
                   worker-1\tworker\tthinking\tsupervisor\tgemini_cli";
        let rows = parse_agent_table(raw);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "supervisor");
        assert_eq!(rows[0].parent, None);
        assert_eq!(rows[0].ai_core.as_deref(), Some("claude_code"));
        assert!(!rows[0].leader);
        assert_eq!(rows[1].parent.as_deref(), Some("supervisor"));
        assert!(!rows[1].leader);
    }

    #[test]
    fn parses_canonical_table_with_leader() {
        // New 6-column header (modern daemon, leader column emitted).
        // Bool values "true" / "false" are parsed; the leader row
        // appears first because the daemon already sorts.
        let raw = "id\tkind\tstatus\tparent\tai_core\tleader\n\
                   admin\tagent\tidle\t-\tclaude_code\ttrue\n\
                   coder\tagent\tidle\t-\tclaude_code\tfalse\n\
                   worker-1\tworker\tthinking\tcoder\tgemini_cli\tfalse";
        let rows = parse_agent_table(raw);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, "admin");
        assert!(rows[0].leader, "explicit `true` parses to leader=true");
        assert_eq!(rows[1].id, "coder");
        assert!(!rows[1].leader);
        assert_eq!(rows[2].id, "worker-1");
        assert!(!rows[2].leader);
        assert_eq!(rows[2].parent.as_deref(), Some("coder"));
    }

    #[test]
    fn rejects_unknown_header() {
        let raw = "name\tstatus\nsomething\tok";
        assert!(parse_agent_table(raw).is_empty());
    }

    #[test]
    fn empty_input_is_ok() {
        assert!(parse_agent_table("").is_empty());
    }

    #[test]
    fn parses_canonical_table_with_group() {
        // Newest 7-column header (modern daemon, group column emitted).
        // The trailing `group` cell uses `-` for None; named groups
        // come through verbatim. Legacy headers leave group=None on
        // every row regardless of the row content.
        let raw = "id\tkind\tstatus\tparent\tai_core\tleader\tgroup\n\
                   admin\tagent\tidle\t-\tclaude_code\ttrue\tadmin\n\
                   coder\tagent\tidle\t-\tclaude_code\tfalse\tengineering\n\
                   shitposter\tagent\tidle\t-\tcopilot_haiku\tfalse\t-\n\
                   worker-1\tworker\tthinking\tcoder\tgemini_cli\tfalse\tengineering";
        let rows = parse_agent_table(raw);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].id, "admin");
        assert!(rows[0].leader);
        assert_eq!(rows[0].group.as_deref(), Some("admin"));
        assert_eq!(rows[1].group.as_deref(), Some("engineering"));
        assert!(rows[2].group.is_none(), "`-` cell must parse as None");
        assert_eq!(rows[3].group.as_deref(), Some("engineering"));
        assert_eq!(rows[3].parent.as_deref(), Some("coder"));
    }

    #[test]
    fn legacy_six_col_header_yields_no_group() {
        // 6-column legacy parses with group=None on every row even
        // when the data rows have a 7th column — those rows are
        // skipped because the column count mismatches the header.
        let raw = "id\tkind\tstatus\tparent\tai_core\tleader\n\
                   admin\tagent\tidle\t-\tclaude_code\ttrue";
        let rows = parse_agent_table(raw);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].leader);
        assert!(rows[0].group.is_none());
    }
}
