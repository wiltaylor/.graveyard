//! Cron endpoints — REST wrappers + SSE proxy over the aciddog
//! `Cron*` RPCs. Mirrors [`super::workflows`]: a JSON list endpoint,
//! CRUD mutations that round-trip a single RPC each, and an SSE
//! passthrough for live mutation events.

use std::convert::Infallible;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::RequestCmd;
use crate::routes::instances::load_row_for_routes;
use crate::routes::tasks::MutationResponse;
use crate::state::AppState;
use crate::wire_cron::WireCronJob;

#[derive(Debug, Clone, Serialize)]
pub struct CronListResponse {
    pub ok: bool,
    pub jobs: Vec<WireCronJob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/cron
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CronListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronListJson {
            target: None,
            include_disabled: true,
            mine: false,
        })
        .await?;
    if !resp.ok {
        return Ok(Json(CronListResponse {
            ok: false,
            jobs: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(CronListResponse {
            ok: false,
            jobs: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 CronListJson support"
                    .into(),
            ),
        }));
    };
    let jobs: Vec<WireCronJob> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding cron list: {e}")))?;
    Ok(Json(CronListResponse {
        ok: true,
        jobs,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateCronRequest {
    pub target: String,
    /// `"message"` or `"command"`.
    pub kind: String,
    pub payload: String,
    /// 5-field cron expression for recurring jobs; ISO wall-clock
    /// datetime (`YYYY-MM-DDTHH:MM[:SS]`) for one-off.
    pub schedule: String,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub oneoff: bool,
    #[serde(default)]
    pub catchup_on_startup: bool,
    #[serde(default)]
    pub disabled: bool,
}

/// POST /api/instances/:id/cron
pub async fn create(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateCronRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.target.trim().is_empty() {
        return Err(AppError::Validation("target is empty".into()));
    }
    if req.payload.trim().is_empty() {
        return Err(AppError::Validation("payload is empty".into()));
    }
    if req.schedule.trim().is_empty() {
        return Err(AppError::Validation("schedule is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronAdd {
            target: req.target,
            kind: req.kind,
            payload: req.payload,
            schedule: req.schedule,
            tz: req.tz.filter(|s| !s.trim().is_empty()),
            oneoff: req.oneoff,
            catchup_on_startup: req.catchup_on_startup,
            disabled: req.disabled,
        })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

#[derive(Debug, Deserialize)]
pub struct PatchCronRequest {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub oneoff: Option<bool>,
    #[serde(default)]
    pub catchup_on_startup: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// PATCH /api/instances/:id/cron/:job_id
pub async fn patch(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<PatchCronRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronUpdate {
            id: job_id,
            target: req.target,
            kind: req.kind,
            payload: req.payload,
            schedule: req.schedule,
            tz: req.tz,
            oneoff: req.oneoff,
            catchup_on_startup: req.catchup_on_startup,
            enabled: req.enabled,
        })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

/// DELETE /api/instances/:id/cron/:job_id
pub async fn delete(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::CronRemove { id: job_id })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

/// GET /api/instances/:id/cron/events
///
/// Opens an SSE stream against the webservice's `/v1/cron/events`
/// endpoint and forwards the raw byte stream to the browser.
pub async fn events(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_cron_events().await?;

    if upstream_status != 200 {
        let status = StatusCode::from_u16(upstream_status).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut collected = Vec::new();
        let mut s = stream;
        while let Some(chunk) = s.next().await {
            if let Ok(bytes) = chunk {
                collected.extend_from_slice(&bytes);
                if collected.len() > 4096 {
                    break;
                }
            } else {
                break;
            }
        }
        let body = String::from_utf8_lossy(&collected).into_owned();
        return Ok((status, body).into_response());
    }

    // Errors on the upstream stream close the SSE connection cleanly;
    // `EventSource` auto-reconnects.
    let byte_stream = stream.map(|res| match res {
        Ok(b) => Ok::<_, Infallible>(b),
        Err(e) => {
            tracing::debug!(error = %e, "upstream cron event stream ended with error");
            Ok(bytes::Bytes::new())
        }
    });

    let mut resp = Response::new(Body::from_stream(byte_stream));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    resp.headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(resp)
}
