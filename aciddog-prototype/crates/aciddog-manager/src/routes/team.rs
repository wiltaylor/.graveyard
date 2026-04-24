//! Team-overview endpoint — single GET returning the team block,
//! fleet counts, and configured AI cores from the upstream
//! webservice.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
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
