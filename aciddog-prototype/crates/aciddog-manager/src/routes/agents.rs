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
/// `handle_admin_agent_list` (see `aciddog/src/tui/app.rs:6054`).
/// Header: `id\tkind\tstatus\tparent\tai_core`. Empty input yields an
/// empty Vec. Rows that don't have 5 columns are skipped.
pub(crate) fn parse_agent_table(raw: &str) -> Vec<AgentRow> {
    let mut lines = raw.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => return Vec::new(),
    };
    if header != "id\tkind\tstatus\tparent\tai_core" {
        // Unknown header — refuse to guess; the caller still has `raw`.
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 5 {
            continue;
        }
        let nullable = |s: &str| if s == "-" { None } else { Some(s.to_string()) };
        out.push(AgentRow {
            id: parts[0].to_string(),
            kind: parts[1].to_string(),
            status: parts[2].to_string(),
            parent: nullable(parts[3]),
            ai_core: nullable(parts[4]),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_table() {
        let raw = "id\tkind\tstatus\tparent\tai_core\n\
                   supervisor\tagent\tidle\t-\tclaude_code\n\
                   worker-1\tworker\tthinking\tsupervisor\tgemini_cli";
        let rows = parse_agent_table(raw);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "supervisor");
        assert_eq!(rows[0].parent, None);
        assert_eq!(rows[0].ai_core.as_deref(), Some("claude_code"));
        assert_eq!(rows[1].parent.as_deref(), Some("supervisor"));
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
}
