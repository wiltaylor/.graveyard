//! Managed-daemons HTTP surface (ADR-0005, foundation slice).
//!
//! This file deliberately exposes ZERO concrete handlers. Sibling
//! task b91e9691 will register the actual `/api/managed-daemons/*`
//! routes (list / create / start / stop / logs / etc). What lives
//! here is the gate-and-auth scaffolding so those handlers slot in
//! without re-deriving auth shape:
//!
//! - The router is only constructed when `AppState::is_daemon_runner_enabled`
//!   is true (see `routes/mod.rs`). When the capability is OFF, the
//!   nest is skipped entirely so `/api/managed-daemons` 404s — the
//!   capability is literally absent, not 403'd.
//! - When ON, the `require_admin` layer is baked in at the group
//!   root so every future handler under this prefix inherits both
//!   session auth (401 on missing/expired) and admin enforcement
//!   (403 on authenticated-but-not-admin) for free.

use axum::http::StatusCode;
use axum::routing::any;
use axum::Router;

use crate::state::AppState;

/// Build the (currently empty) `/managed-daemons` sub-router with the
/// admin layer baked in at the group root. Sibling b91e9691 layers
/// concrete handlers on top via `.route(...)` — the layer here will
/// inherit because axum applies group-level layers to every nested
/// route added to this router.
///
/// Returns a `Router<AppState>` (not `Router<()>`) because the parent
/// `/api` router is itself `Router<AppState>` until the final
/// `.with_state(state)` collapses it; nesting requires matching state
/// types.
///
/// Two placeholder routes — `/` and `/*rest` — answer 404 so that the
/// `require_admin` layer is exercised on every `/api/managed-daemons*`
/// path. The effect: any request to this prefix with NO session →
/// 401 (layer fail-fast); with an admin session but no concrete
/// handler → 404. Without these, axum's `nest` of an empty router
/// does not dispatch — requests fall through to the parent's
/// static-assets fallback, the layer never fires, and tests pin
/// 404 instead of 401. The placeholders close that gap.
///
/// SIBLING b91e9691 NOTE: when you register a real `/` handler
/// (e.g. `GET /api/managed-daemons` for list), DELETE the `/`
/// placeholder line below — axum errors at startup on duplicate
/// routes. The `/*rest` line can stay as a safety net (specific
/// routes win over the wildcard), or be removed once your handler
/// set is exhaustive.
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", any(|| async { StatusCode::NOT_FOUND }))
        .route("/*rest", any(|| async { StatusCode::NOT_FOUND }))
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::auth::require_admin,
        ))
}
