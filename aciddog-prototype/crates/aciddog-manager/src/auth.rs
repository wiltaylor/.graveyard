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
