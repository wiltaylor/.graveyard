use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::{self, AuthUser};
use crate::error::{AppError, AppResult};
use crate::model::{session, user};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub username: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> AppResult<Response> {
    let row = user::find_by_username(&state.pool, input.username.trim()).await?;
    let Some(row) = row else {
        // Constant-ish delay so the response time doesn't differ between
        // unknown users and bad passwords.
        tokio::time::sleep(Duration::from_millis(250)).await;
        return Err(AppError::BadCredentials);
    };
    if !user::verify_password(&row.password_hash, &input.password) {
        tokio::time::sleep(Duration::from_millis(250)).await;
        return Err(AppError::BadCredentials);
    }
    let session = session::create(&state.pool, &row.id).await?;
    user::mark_logged_in(&state.pool, &row.id).await?;

    let cookie = auth::build_session_cookie(&session.token, state.auth.cookie_secure);
    let mut resp = (
        StatusCode::OK,
        Json(UserResponse {
            username: row.username,
        }),
    )
        .into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|e| AppError::Validation(format!("cookie header: {e}")))?,
    );
    Ok(resp)
}

pub async fn logout(State(state): State<AppState>, req: Request) -> AppResult<Response> {
    if let Some(token) = auth::extract_session_cookie(&req) {
        let _ = session::delete(&state.pool, &token).await;
    }
    let cookie = auth::build_clear_cookie(state.auth.cookie_secure);
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|e| AppError::Validation(format!("cookie header: {e}")))?,
    );
    Ok(resp)
}

pub async fn whoami(State(state): State<AppState>, req: Request) -> AppResult<Json<UserResponse>> {
    let token = auth::extract_session_cookie(&req).ok_or(AppError::Unauthenticated)?;
    let lookup = session::lookup(&state.pool, &token).await?;
    let Some((user, _)) = lookup else {
        return Err(AppError::Unauthenticated);
    };
    Ok(Json(UserResponse {
        username: user.username,
    }))
}

/// Optional handler-side extractor for routes that want to know which
/// user is making the request. Currently unused — the middleware injects
/// `AuthUser` into request extensions but no protected handler reads it.
#[allow(dead_code)]
pub fn current_user(req: &Request) -> Option<&AuthUser> {
    req.extensions().get::<AuthUser>()
}
