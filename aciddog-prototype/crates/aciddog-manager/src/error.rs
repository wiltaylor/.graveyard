use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,

    #[error("validation: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("unauthenticated")]
    Unauthenticated,

    #[error("forbidden")]
    Forbidden,

    #[error("invalid username or password")]
    BadCredentials,

    #[error("database: {0}")]
    Db(#[from] sqlx::Error),

    #[error("remote: {0}")]
    Remote(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::Unauthenticated => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::BadCredentials => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Db(e) => {
                tracing::error!(error = %e, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal database error".to_string(),
                )
            }
            AppError::Remote(e) => {
                tracing::warn!(error = %e, "remote error");
                (StatusCode::BAD_GATEWAY, e.to_string())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
