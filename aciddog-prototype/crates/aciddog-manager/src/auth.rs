use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::AppError;
use crate::model::session;
use crate::state::AppState;

pub const COOKIE_NAME: &str = "adm_session";

#[derive(Debug, Clone, Copy, Default)]
pub struct AuthConfig {
    pub cookie_secure: bool,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
}

/// Caller of a route protected by `require_user_or_instance`. Either a
/// logged-in human (session cookie) or a registered aciddog instance
/// presenting its `api_key` as a bearer token. Routes log the principal
/// so audit is straightforward.
#[derive(Debug, Clone)]
pub enum Principal {
    User { id: String, username: String },
    Instance { id: String, name: String },
}

impl Principal {
    pub fn label(&self) -> String {
        match self {
            Principal::User { username, .. } => format!("user:{username}"),
            Principal::Instance { name, .. } => format!("instance:{name}"),
        }
    }
}

/// Extract the session cookie from a `Cookie:` header without bringing
/// in `tower-cookies`. Cookies are `name=value; name2=value2; ...`.
pub fn extract_session_cookie(req: &Request) -> Option<String> {
    let header_value = req.headers().get(header::COOKIE)?;
    let raw = header_value.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(&format!("{COOKIE_NAME}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

pub fn build_session_cookie(token: &str, secure: bool) -> String {
    let max_age = session::SESSION_DURATION_DAYS * 24 * 60 * 60;
    let mut s =
        format!("{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}");
    if secure {
        s.push_str("; Secure");
    }
    s
}

pub fn build_clear_cookie(secure: bool) -> String {
    let mut s = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    if secure {
        s.push_str("; Secure");
    }
    s
}

/// Axum middleware: enforce a valid session cookie on every protected
/// request. On miss/expiry returns 401 with the standard error envelope.
pub async fn require_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let token = match extract_session_cookie(&req) {
        Some(t) => t,
        None => return AppError::Unauthenticated.into_response(),
    };
    let lookup = match session::lookup(&state.pool, &token).await {
        Ok(opt) => opt,
        Err(e) => return e.into_response(),
    };
    let Some((user, _)) = lookup else {
        return AppError::Unauthenticated.into_response();
    };
    req.extensions_mut().insert(AuthUser {
        user_id: user.id,
        username: user.username,
    });
    next.run(req).await
}

/// Extract `Authorization: Bearer <token>` from the request, if present.
fn extract_bearer_token(req: &Request) -> Option<String> {
    let raw = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let rest = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?;
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Look up an instance whose stored `api_key` matches the supplied
/// bearer token. We don't index `api_key` (multi-tenant manager
/// deployments are small — the existing `instances` table stays in
/// the low tens of rows). Returns `(id, name)` on match, `None`
/// otherwise.
async fn instance_by_api_key(
    pool: &sqlx::SqlitePool,
    token: &str,
) -> Result<Option<(String, String)>, AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM instances WHERE api_key = ?1 LIMIT 1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(row)
}

/// Axum middleware: accept either a logged-in user (session cookie) or a
/// registered instance (bearer matching `instances.api_key`). Used by
/// routes that the manager exposes both to the web UI and to
/// agent-side automation tooling on a daemon.
pub async fn require_user_or_instance(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(token) = extract_session_cookie(&req) {
        let lookup = match session::lookup(&state.pool, &token).await {
            Ok(opt) => opt,
            Err(e) => return e.into_response(),
        };
        if let Some((user, _)) = lookup {
            let principal = Principal::User {
                id: user.id.clone(),
                username: user.username.clone(),
            };
            req.extensions_mut().insert(AuthUser {
                user_id: user.id,
                username: user.username,
            });
            req.extensions_mut().insert(principal);
            return next.run(req).await;
        }
    }
    if let Some(bearer) = extract_bearer_token(&req) {
        match instance_by_api_key(&state.pool, &bearer).await {
            Ok(Some((id, name))) => {
                let principal = Principal::Instance {
                    id: id.clone(),
                    name: name.clone(),
                };
                req.extensions_mut().insert(principal);
                return next.run(req).await;
            }
            Ok(None) => {}
            Err(e) => return e.into_response(),
        }
    }
    AppError::Unauthenticated.into_response()
}
