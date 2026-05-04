//! HTTP surface for `/api/managed-daemons` (ADR-0005, sibling b91e9691).
//!
//! Replaces the empty stub from sibling 33fc8b7b with seven thin
//! handlers. The handlers own ONLY HTTP shape, status-code mapping,
//! and the `ManagedDaemonView` wire shape. All business logic flows
//! through sibling-owned APIs:
//!
//! Sibling APIs imported (verify drift in one glance):
//!
//! - `crate::model::managed_daemon::{list, fetch, fetch_summary,
//!     set_desired_state, delete_with_paired_instance,
//!     ManagedDaemonRow, ManagedDaemonSummary, DesiredState}` from
//!   sibling **7c11fa3d** (sqlx model + transactional delete helper).
//!   `DesiredState` serialises with `rename_all = "lowercase"`.
//! - `state.daemon_runner.{status, stop, reload}` (canonical names
//!   locked by sibling **799b7b92** — used to be
//!   `observed_state` / `signal(SIGHUP)` / `stop_with_grace`). Returns
//!   `DaemonRunnerError` whose `StillAlive` and `AdoptedNotControlled`
//!   variants are mapped to specific HTTP statuses below.
//!   `ObservedState` serialises with `rename_all = "snake_case"`.
//! - `crate::auth::require_admin` baked in at the group root by
//!   sibling **33fc8b7b**; we PRESERVE it on the new router below
//!   rather than re-applying per-handler.
//!
//! Sibling **580b77a1** (`scaffolding::create_managed_daemon`) is in
//! planning, so the POST `/` handler ships as a documented 501 Not
//! Implemented — see `create()` below for the breadcrumb.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::daemon_runner::{DaemonRunner, DaemonRunnerError, ObservedState};
use crate::error::{AppError, AppResult};
use crate::model::managed_daemon::{self, DesiredState, ManagedDaemonSummary};
use crate::state::AppState;

/// DELETE grace timeout. Sibling 799b7b92 enforces an additional
/// post-SIGKILL 1s tail-wait inside `DaemonRunner::stop`, so total
/// worst-case latency before a 500 is ≤7s.
const DELETE_GRACE: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------

/// Outward-facing managed-daemon row (joins persisted `ManagedDaemonSummary`
/// with the runtime `ObservedState` from `daemon_runner`).
#[derive(Debug, Clone, Serialize)]
pub struct ManagedDaemonView {
    pub id: Uuid,
    /// Best-effort: we read this via direct sqlx on `instances.managed_by_id`.
    /// `None` means the paired instance row is missing — should not happen
    /// in steady state because `insert_with_paired_instance` is atomic, but
    /// the lookup is best-effort so a failed lookup never blocks the response.
    /// TODO(7c11fa3d follow-up): expose a typed `model::managed_daemon::paired_instance_id`
    /// helper so this layer stops issuing raw SQL.
    pub paired_instance_id: Option<Uuid>,
    pub name: String,
    pub desired_state: DesiredState,
    pub observed_state: ObservedState,
    pub socket_path: String,
    pub created_at: DateTime<Utc>,
}

impl ManagedDaemonView {
    pub fn from_parts(
        summary: ManagedDaemonSummary,
        observed: ObservedState,
        paired: Option<Uuid>,
    ) -> Self {
        Self {
            id: summary.id,
            paired_instance_id: paired,
            name: summary.name,
            desired_state: summary.desired_state,
            observed_state: observed,
            socket_path: summary.socket_path,
            created_at: summary.created_at,
        }
    }
}

/// POST `/` request body — placeholder shape until sibling 580b77a1
/// (`scaffolding::create_managed_daemon`) lands.
///
/// All fields are optional so a request with `{}` still parses; the
/// handler ignores them and returns 501 regardless.
///
/// TODO(580b77a1): replace stub with thin wrapper around
/// `scaffolding::create_managed_daemon`; promote the fields to required +
/// validated.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CreateManagedDaemonRequest {
    pub name: Option<String>,
    pub work_dir: Option<String>,
    pub socket_path: Option<String>,
    pub idle_timeout_secs: Option<u32>,
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Resolve the runtime daemon-runner or fail with a 500.
///
/// This branch should be unreachable in production: the gate at
/// `routes/mod.rs` only nests `/managed-daemons` when
/// `is_daemon_runner_enabled = true`, and `main.rs` is responsible for
/// constructing the runner under the same gate. If we get here with a
/// `None` runner, the binary was misconfigured.
fn require_runner(state: &AppState) -> AppResult<&std::sync::Arc<DaemonRunner>> {
    state
        .daemon_runner
        .as_ref()
        .ok_or_else(|| AppError::Internal("daemon_runner not initialised".into()))
}

/// Best-effort lookup of the paired `instances.id` for a managed daemon.
/// Returns `None` on any error or miss — the view is allowed to lack
/// this field rather than fail the whole response.
async fn paired_instance_id(state: &AppState, daemon_id: Uuid) -> Option<Uuid> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT id FROM instances WHERE managed_by_id = ?1 LIMIT 1")
            .bind(daemon_id.to_string())
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    row.and_then(|(id_str,)| Uuid::parse_str(&id_str).ok())
}

// ---------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------

/// GET `/api/managed-daemons` — list all managed daemons with their
/// current observed state.
pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<ManagedDaemonView>>> {
    let summaries = managed_daemon::list(&state.pool).await?;
    let runner = require_runner(&state)?;
    let mut views = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let observed = runner
            .status(summary.id)
            .await
            .unwrap_or(ObservedState::Stopped);
        let paired = paired_instance_id(&state, summary.id).await;
        views.push(ManagedDaemonView::from_parts(summary, observed, paired));
    }
    Ok(Json(views))
}

/// GET `/api/managed-daemons/:id` — fetch one with observed state.
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ManagedDaemonView>> {
    let summary = managed_daemon::fetch_summary(&state.pool, id).await?;
    let runner = require_runner(&state)?;
    let observed = runner.status(id).await.unwrap_or(ObservedState::Stopped);
    let paired = paired_instance_id(&state, id).await;
    Ok(Json(ManagedDaemonView::from_parts(
        summary, observed, paired,
    )))
}

/// POST `/api/managed-daemons` — STUBBED at 501.
///
/// Sibling **580b77a1** (`scaffolding::create_managed_daemon`) owns
/// the implementation; until it lands the route slot is reserved but
/// the body returns Not Implemented.
///
/// TODO(580b77a1): replace stub with thin wrapper around
/// `scaffolding::create_managed_daemon` (5–10 line delegation per
/// the original plan §5).
pub async fn create(
    State(_state): State<AppState>,
    Json(_req): Json<CreateManagedDaemonRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    Ok((
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "ok": false,
            "error": "create handler is stubbed pending sibling 580b77a1 (scaffolding::create_managed_daemon); routes table reserves the slot but the body is unimplemented in this slice"
        })),
    ))
}

/// DELETE `/api/managed-daemons/:id` — stop with grace, then drop both
/// the `managed_daemons` row and its paired `instances` row in a single
/// transaction.
///
/// Sequence per plan §6:
///   a. fetch — 404 on miss.
///   b. `runner.stop(id, 5s)` — canonical name (was `stop_with_grace`).
///      Sibling 799b7b92 enforces the post-SIGKILL 1s tail-wait, so
///      total worst-case latency before a 500 is ≤7s.
///   c. on Ok → `delete_with_paired_instance` → 204.
///   d. on Err(StillAlive) → 500 + json body; DB rows STAY in place
///      for forensics.
///   e. on Err(AdoptedNotControlled) → 409 (operator must stop the
///      adopted daemon manually before it can be deleted by the
///      manager).
///   f. on Err(other) → mapped via AppError::Internal (or whatever
///      sqlx surfaces, via `?`).
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    // (a) Resolve 404 separately so AppError::NotFound's response shape
    // (`{"error": "not found"}`) keeps matching the rest of the API.
    if let Err(e) = managed_daemon::fetch(&state.pool, id).await {
        let resp = e.into_response_parts();
        return Err(resp);
    }

    let runner = match state.daemon_runner.as_ref() {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "ok": false,
                    "error": "daemon_runner not initialised"
                })),
            ));
        }
    };

    match runner.stop(id, DELETE_GRACE).await {
        Ok(()) => match managed_daemon::delete_with_paired_instance(&state.pool, id).await {
            Ok(()) => Ok(StatusCode::NO_CONTENT),
            Err(e) => Err(e.into_response_parts()),
        },
        Err(DaemonRunnerError::StillAlive(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "ok": false,
                "error": "daemon failed to stop within grace; manual intervention required"
            })),
        )),
        Err(DaemonRunnerError::AdoptedNotControlled(_)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "error": "cannot delete adopted daemon: stop it manually first"
            })),
        )),
        Err(DaemonRunnerError::NotFound(_)) => {
            // The daemon-runner has no entry for this id — that just
            // means it was never spawned (or already cleaned up).
            // Proceed with the DB delete; this is the common case
            // when desired_state was Stopped and no child was ever
            // spawned by the manager.
            match managed_daemon::delete_with_paired_instance(&state.pool, id).await {
                Ok(()) => Ok(StatusCode::NO_CONTENT),
                Err(e) => Err(e.into_response_parts()),
            }
        }
        Err(other) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "ok": false,
                "error": format!("daemon_runner stop failed: {other}")
            })),
        )),
    }
}

/// POST `/api/managed-daemons/:id/start` — flip desired_state to
/// `Running` and return the current view. Idempotent: a repeated call
/// against an already-Running daemon still returns 202 with the
/// current view (never 409).
///
/// The reconciler in sibling 799b7b92 picks up the desired_state flip
/// on its next tick and issues the actual spawn.
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<ManagedDaemonView>)> {
    let _ = managed_daemon::fetch(&state.pool, id).await?;
    managed_daemon::set_desired_state(&state.pool, id, DesiredState::Running).await?;
    let summary = managed_daemon::fetch_summary(&state.pool, id).await?;
    let runner = require_runner(&state)?;
    let observed = runner.status(id).await.unwrap_or(ObservedState::Stopped);
    let paired = paired_instance_id(&state, id).await;
    Ok((
        StatusCode::ACCEPTED,
        Json(ManagedDaemonView::from_parts(summary, observed, paired)),
    ))
}

/// POST `/api/managed-daemons/:id/stop` — symmetric to `start` with
/// `DesiredState::Stopped`. The reconciler in 799b7b92 owns the
/// actual SIGTERM dispatch. Idempotent.
pub async fn stop(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<ManagedDaemonView>)> {
    let _ = managed_daemon::fetch(&state.pool, id).await?;
    managed_daemon::set_desired_state(&state.pool, id, DesiredState::Stopped).await?;
    let summary = managed_daemon::fetch_summary(&state.pool, id).await?;
    let runner = require_runner(&state)?;
    let observed = runner.status(id).await.unwrap_or(ObservedState::Stopped);
    let paired = paired_instance_id(&state, id).await;
    Ok((
        StatusCode::ACCEPTED,
        Json(ManagedDaemonView::from_parts(summary, observed, paired)),
    ))
}

/// POST `/api/managed-daemons/:id/reload` — SIGHUP a Running daemon.
/// Returns 409 when observed != Running. Does NOT touch
/// `desired_state`.
pub async fn reload(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<ManagedDaemonView>)> {
    let _ = managed_daemon::fetch(&state.pool, id).await?;
    let runner = require_runner(&state)?;
    let observed = runner.status(id).await.unwrap_or(ObservedState::Stopped);
    if observed != ObservedState::Running {
        return Err(AppError::Conflict(format!(
            "cannot reload daemon in observed_state '{:?}'; expected running",
            observed
        )));
    }
    match runner.reload(id).await {
        Ok(()) => {}
        Err(DaemonRunnerError::AdoptedNotControlled(_)) => {
            return Err(AppError::Conflict(format!(
                "cannot reload adopted daemon {id}: stop it manually first"
            )));
        }
        Err(DaemonRunnerError::NotRunning(_)) => {
            // Race: status said Running, but the runner state machine
            // disagreed. Treat as a 409 — same as the pre-check.
            return Err(AppError::Conflict(format!(
                "cannot reload daemon {id}: not in Running state"
            )));
        }
        Err(other) => {
            return Err(AppError::Internal(format!(
                "daemon_runner reload failed: {other}"
            )));
        }
    }
    let summary = managed_daemon::fetch_summary(&state.pool, id).await?;
    let paired = paired_instance_id(&state, id).await;
    Ok((
        StatusCode::ACCEPTED,
        Json(ManagedDaemonView::from_parts(summary, observed, paired)),
    ))
}

/// Query params for `GET /api/managed-daemons/:id/logs`.
///
/// `follow=1` keeps the stream open and tails new lines via the
/// per-child broadcaster; `follow=0` (default) drains the in-memory
/// ring buffer once and closes. `since=<offset>` is a byte-anchored
/// cursor — typically populated from the prior connection's last
/// `id:` field via the browser's automatic `Last-Event-ID` header,
/// but explicit `?since=` overrides the header.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LogsQuery {
    pub follow: Option<u8>,
    pub since: Option<u64>,
}

/// `GET /api/managed-daemons/:id/logs[?follow=0|1&since=<u64>]` —
/// stream stdout/stderr captured from the managed child as SSE.
///
/// Behaviour (sibling 3bc8a0d0):
///   * `404` if the runner has no entry for `id`.
///   * `409 {"error": "adopted: no manager-side capture"}` for
///     adopted entries (`child = None`, no pipe to read).
///   * Replay phase: drain `RingBufferHandle::since(<since>)`. If the
///     cursor is older than the oldest retained line, emit one
///     `event: gap\ndata: {"dropped_until": <offset>}` first.
///   * Each replayed line is one
///     `id: <offset>\nevent: line\ndata: {"ts","stream","line"}`.
///   * `follow=0` (default): the stream ends after the replay; no
///     broadcast subscription, no keepalive.
///   * `follow=1`: AFTER the replay, attach a `broadcast::Receiver`
///     and stream indefinitely. `BroadcastStreamRecvError::Lagged(n)`
///     becomes a named `lagged` event so the UI can render a
///     `[lagged: N events dropped]` marker.
///   * Headers: `Cache-Control: no-cache, no-store, must-revalidate`
///     and `x-accel-buffering: no` so reverse proxies (nginx, Caddy)
///     don't coalesce SSE frames.
pub async fn logs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<LogsQuery>,
    headers: HeaderMap,
) -> Response {
    // Resolve `since` from the query string, falling back to the
    // browser's auto-supplied Last-Event-ID, then 0.
    let since = query
        .since
        .or_else(|| {
            headers
                .get("Last-Event-ID")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(0);

    // Resolve the per-child handle. None can mean either:
    //   * the runner doesn't track this id (404), or
    //   * the entry is adopted and has no pipes (409).
    let runner = match state.daemon_runner.as_ref() {
        Some(r) => r,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let handle = runner.logs_handle(id).await;
    if handle.is_none() {
        // Distinguish adopted from missing.
        if runner.is_managed(id).await {
            // Should not happen — managed entries always have logs —
            // but be defensive and surface as a 500 so a future
            // regression is visible.
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "managed entry without logs handle"})),
            )
                .into_response();
        }
        // Either id is unknown OR entry is adopted (no pipe).
        // The brief asks for 404 unknown vs 409 adopted.
        // `is_managed(id) == false` covers BOTH the missing AND adopted
        // cases; we need a separate "exists" check.
        let exists_adopted = registry_has_id(runner, id).await;
        return if exists_adopted {
            (
                StatusCode::CONFLICT,
                Json(json!({"error": "adopted: no manager-side capture"})),
            )
                .into_response()
        } else {
            StatusCode::NOT_FOUND.into_response()
        };
    }
    let handle = handle.expect("checked Some above");

    // Replay snapshot.
    let (replay_entries, gap) = handle.since(since).await;
    let follow = query.follow == Some(1);

    // Build the replay stream.
    let mut replay: Vec<Result<Event, Infallible>> = Vec::with_capacity(replay_entries.len() + 1);
    if let Some(offset) = gap {
        if let Ok(ev) = Event::default()
            .event("gap")
            .json_data(json!({"dropped_until": offset}))
        {
            replay.push(Ok(ev));
        }
    }
    for entry in replay_entries {
        if let Some(ev) = line_event(&entry) {
            replay.push(Ok(ev));
        }
    }
    let replay_stream = futures::stream::iter(replay);

    if follow {
        let rx = handle.subscribe();
        let live = BroadcastStream::new(rx).filter_map(|msg| async move {
            match msg {
                Ok(line) => line_event(&line).map(Ok::<_, Infallible>),
                Err(BroadcastStreamRecvError::Lagged(n)) => Event::default()
                    .event("lagged")
                    .json_data(json!({"dropped": n}))
                    .ok()
                    .map(Ok::<_, Infallible>),
            }
        });
        let combined = replay_stream.chain(live);
        let sse = Sse::new(combined).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        );
        let mut resp = sse.into_response();
        attach_sse_headers(resp.headers_mut());
        resp
    } else {
        let sse = Sse::new(replay_stream);
        let mut resp = sse.into_response();
        attach_sse_headers(resp.headers_mut());
        resp
    }
}

fn attach_sse_headers(headers: &mut HeaderMap) {
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
}

fn line_event(line: &crate::daemon_runner::CapturedLine) -> Option<Event> {
    let body = String::from_utf8_lossy(&line.bytes);
    Event::default()
        .id(line.offset.to_string())
        .event("line")
        .json_data(json!({
            "ts": line.ts,
            "stream": line.stream,
            "line": body,
        }))
        .ok()
}

/// True iff the runner tracks `id` regardless of whether it owns the
/// child. Used to distinguish "not found (404)" from "adopted (409)"
/// in the SSE logs handler.
async fn registry_has_id(runner: &std::sync::Arc<DaemonRunner>, id: Uuid) -> bool {
    runner.status(id).await.is_some()
}

// ---------------------------------------------------------------------
// Wiring
// ---------------------------------------------------------------------

/// Build the `/managed-daemons` sub-router with the admin layer baked
/// in at the group root.
///
/// Replaces the empty stub from sibling 33fc8b7b: the `/` placeholder
/// is removed (axum errors at startup on duplicate routes once we
/// register the real `list` + `create` handlers). The `/*rest`
/// placeholder is also dropped — axum 0.7 rejects a `/*wildcard` that
/// overlaps with concrete dynamic segments like `/:id/start`, and our
/// concrete routes exhaustively cover the documented surface, so any
/// unknown sub-path falls through to the parent's static-asset
/// fallback and 404s anyway.
///
/// PRESERVE 33fc8b7b's `require_admin` layer — single application at
/// the group root. Every handler below inherits 401 (no session) and
/// 403 (authenticated-but-not-admin) for free.
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).delete(delete))
        .route("/:id/start", post(start))
        .route("/:id/stop", post(stop))
        .route("/:id/reload", post(reload))
        // Sibling 3bc8a0d0 — SSE tail of stdout/stderr captured by
        // log_capture::start_capture. Concrete route, no /*rest catch-all
        // (b91e9691's merge dropped it), so any unknown sub-path falls
        // through to the parent's 404.
        .route("/:id/logs", get(logs))
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::auth::require_admin,
        ))
}

// ---------------------------------------------------------------------
// AppError → (StatusCode, Json) helper
// ---------------------------------------------------------------------

trait AppErrorParts {
    fn into_response_parts(self) -> (StatusCode, Json<Value>);
}

impl AppErrorParts for AppError {
    /// Convert an `AppError` into the same shape `IntoResponse for
    /// AppError` produces — used by handlers that return a tuple type
    /// (e.g. `delete`) where we cannot rely on the blanket `?` mapping.
    fn into_response_parts(self) -> (StatusCode, Json<Value>) {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::Unauthenticated => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::BadCredentials => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Db(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal database error".to_string(),
            ),
            AppError::Remote(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
        };
        (status, Json(json!({ "error": message })))
    }
}
