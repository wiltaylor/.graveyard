use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::client::Client;
use crate::error::AppResult;
use crate::routes::instances::load_row_for_routes;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub status: u16,
    pub body: String,
}

pub async fn probe(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<HealthResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (status, body) = client.health().await?;
    Ok(Json(HealthResponse {
        ok: status == 200,
        status,
        body,
    }))
}
