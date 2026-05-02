// Mounted under the protected router; inherits require_auth — same auth posture as wire_tasks.rs.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::RequestCmd;
use crate::routes::instances::load_row_for_routes;
use crate::state::AppState;
use crate::wire_repos::{WireDiffOutcome, WireRepository};

#[derive(Debug, Clone, Serialize)]
pub struct ReposListResponse {
    pub ok: bool,
    pub repos: Vec<WireRepository>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoDiffResponse {
    pub ok: bool,
    #[serde(flatten)]
    pub outcome: WireDiffOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RepoDiffRequest {
    pub branch: String,
}

/// GET /api/instances/:id/repos
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ReposListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::ReposListJson).await?;
    if !resp.ok {
        return Ok(Json(ReposListResponse {
            ok: false,
            repos: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(ReposListResponse {
            ok: false,
            repos: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 ReposListJson support"
                    .into(),
            ),
        }));
    };
    let repos: Vec<WireRepository> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding repos list: {e}")))?;
    Ok(Json(ReposListResponse {
        ok: true,
        repos,
        error: None,
    }))
}

/// POST /api/instances/:id/repos/:repo_id/diff
pub async fn diff(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(Uuid, String)>,
    Json(req): Json<RepoDiffRequest>,
) -> AppResult<Json<RepoDiffResponse>> {
    if req.branch.trim().is_empty() {
        return Err(AppError::Validation("branch is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::RepoDiffJson {
            repo_id,
            branch: req.branch,
        })
        .await?;
    if !resp.ok {
        let error = resp.error.or(resp.message);
        return Ok(Json(RepoDiffResponse {
            ok: false,
            outcome: WireDiffOutcome::Error {
                command: String::new(),
                stderr: error.clone().unwrap_or_default(),
            },
            error,
        }));
    }
    let Some(data) = resp.data else {
        let error = Some(
            "remote returned ok but no `data` — instance may predate \
             RepoDiffJson support"
                .into(),
        );
        return Ok(Json(RepoDiffResponse {
            ok: false,
            outcome: WireDiffOutcome::Error {
                command: String::new(),
                stderr: error.clone().unwrap_or_default(),
            },
            error,
        }));
    };
    let outcome: WireDiffOutcome = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding repo diff outcome: {e}")))?;
    Ok(Json(RepoDiffResponse {
        ok: true,
        outcome,
        error: None,
    }))
}
