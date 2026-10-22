use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::model::instance::{self, CreateInstance, InstanceSummary};
use crate::state::AppState;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<InstanceSummary>>> {
    let rows = instance::list(&state.pool).await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<CreateInstance>,
) -> AppResult<(StatusCode, Json<InstanceSummary>)> {
    let summary = instance::insert(&state.pool, input).await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    instance::delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Helper used by `agents` and `health` routes to resolve a path param
/// into a stored row, mapping a malformed UUID to 404 (same as missing).
pub async fn load_row_for_routes(
    state: &AppState,
    id: Uuid,
) -> AppResult<crate::model::instance::InstanceRow> {
    instance::fetch(&state.pool, id).await.map_err(|e| match e {
        AppError::NotFound => AppError::NotFound,
        other => other,
    })
}
