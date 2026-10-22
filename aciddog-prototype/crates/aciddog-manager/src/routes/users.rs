use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::model::user::{self, UserSummary};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub password: String,
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<UserSummary>>> {
    let users = user::list_summaries(&state.pool).await?;
    Ok(Json(users))
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<UserSummary>)> {
    let summary = user::insert(&state.pool, &input.username, &input.password).await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let id_str = id.to_string();
    if id_str == caller.user_id {
        return Err(AppError::Conflict("cannot delete your own account".into()));
    }
    if user::count(&state.pool).await? <= 1 {
        return Err(AppError::Conflict(
            "cannot delete the last remaining user".into(),
        ));
    }
    user::delete_by_id(&state.pool, &id_str).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset_password(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<ResetPasswordRequest>,
) -> AppResult<StatusCode> {
    user::update_password_by_id(&state.pool, &id.to_string(), &input.password).await?;
    Ok(StatusCode::NO_CONTENT)
}
