//! Team-overview endpoint — single GET returning the team block,
//! fleet counts, and configured AI cores from the upstream
//! webservice.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::RequestCmd;
use crate::routes::instances::load_row_for_routes;
use crate::state::AppState;
use crate::wire_team::WireTeamInfo;

#[derive(Debug, Clone, Serialize)]
pub struct TeamInfoResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<WireTeamInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response shape for `POST /api/instances/:id/team/reload`. `summary`
/// echoes the daemon's reconciliation breakdown ("agents +N/-M/~K
/// (...)") for surfacing in the UI on success.
#[derive(Debug, Clone, Serialize)]
pub struct ReloadResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/team
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TeamInfoResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::TeamInfo).await?;
    if !resp.ok {
        return Ok(Json(TeamInfoResponse {
            ok: false,
            info: None,
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(TeamInfoResponse {
            ok: false,
            info: None,
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 TeamInfo support"
                    .into(),
            ),
        }));
    };
    let info: WireTeamInfo = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding team info: {e}")))?;
    Ok(Json(TeamInfoResponse {
        ok: true,
        info: Some(info),
        error: None,
    }))
}

/// POST /api/instances/:id/team/reload — re-read the upstream's WCL
/// config and reconcile its running fleet. Requires `sysadmin = true`
/// on the manager-portal automaton (the daemon enforces).
pub async fn reload(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ReloadResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::ServerReload).await?;
    if !resp.ok {
        return Ok(Json(ReloadResponse {
            ok: false,
            summary: None,
            error: resp.error.or(resp.message),
        }));
    }
    Ok(Json(ReloadResponse {
        ok: true,
        summary: resp.message,
        error: None,
    }))
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadWclResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wcl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WriteWclRequest {
    pub wcl: String,
}

/// GET /api/instances/:id/team/wcl — fetch the raw WCL source of the
/// upstream's config file. Used by the team editor to bootstrap.
pub async fn read_wcl(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ReadWclResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::TeamRead).await?;
    if !resp.ok {
        return Ok(Json(ReadWclResponse {
            ok: false,
            wcl: None,
            path: None,
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(ReadWclResponse {
            ok: false,
            wcl: None,
            path: None,
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 TeamRead support"
                    .into(),
            ),
        }));
    };
    #[derive(Deserialize)]
    struct Payload {
        wcl: String,
        path: String,
    }
    let payload: Payload = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding team WCL: {e}")))?;
    Ok(Json(ReadWclResponse {
        ok: true,
        wcl: Some(payload.wcl),
        path: Some(payload.path),
        error: None,
    }))
}

/// POST /api/instances/:id/team/wcl — replace the upstream's WCL config
/// with the supplied source and reconcile its fleet. Requires
/// `sysadmin = true` on the manager-portal automaton (the daemon enforces).
pub async fn write_wcl(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<WriteWclRequest>,
) -> AppResult<Json<ReloadResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TeamWrite { wcl: req.wcl })
        .await?;
    if !resp.ok {
        return Ok(Json(ReloadResponse {
            ok: false,
            summary: None,
            error: resp.error.or(resp.message),
        }));
    }
    Ok(Json(ReloadResponse {
        ok: true,
        summary: resp.message,
        error: None,
    }))
}
