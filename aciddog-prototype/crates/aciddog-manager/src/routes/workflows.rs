//! Workflow endpoints — REST wrappers + SSE proxy over the aciddog
//! `Workflow*` RPCs. Mirrors the Tasks pattern at
//! [`super::tasks`] with one addition: a live SSE passthrough for
//! workflow progress events.

use std::collections::BTreeMap;
use std::convert::Infallible;

use axum::body::Body;
use axum::extract::{Path, Query, State};
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
use crate::wire_workflow::{WireWorkflowInstance, WireWorkflowTemplate};

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTemplatesResponse {
    pub ok: bool,
    pub templates: Vec<WireWorkflowTemplate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowInstancesResponse {
    pub ok: bool,
    pub instances: Vec<WireWorkflowInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/workflows/templates
pub async fn list_templates(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<WorkflowTemplatesResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::WorkflowTemplatesJson).await?;
    if !resp.ok {
        return Ok(Json(WorkflowTemplatesResponse {
            ok: false,
            templates: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(WorkflowTemplatesResponse {
            ok: false,
            templates: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 WorkflowTemplatesJson support"
                    .into(),
            ),
        }));
    };
    let templates: Vec<WireWorkflowTemplate> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding workflow templates: {e}")))?;
    Ok(Json(WorkflowTemplatesResponse {
        ok: true,
        templates,
        error: None,
    }))
}

/// GET /api/instances/:id/workflows/instances
pub async fn list_instances(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<WorkflowInstancesResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::WorkflowInstancesJson).await?;
    if !resp.ok {
        return Ok(Json(WorkflowInstancesResponse {
            ok: false,
            instances: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(WorkflowInstancesResponse {
            ok: false,
            instances: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 WorkflowInstancesJson support"
                    .into(),
            ),
        }));
    };
    let instances: Vec<WireWorkflowInstance> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding workflow instances: {e}")))?;
    Ok(Json(WorkflowInstancesResponse {
        ok: true,
        instances,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct StartWorkflowRequest {
    pub workflow: String,
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
}

/// POST /api/instances/:id/workflows/instances
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<StartWorkflowRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.workflow.trim().is_empty() {
        return Err(AppError::Validation("workflow id is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::WorkflowRun {
            workflow: req.workflow,
            properties: req.properties,
        })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

/// DELETE /api/instances/:id/workflows/instances/:wf_id
pub async fn dismiss(
    State(state): State<AppState>,
    Path((id, wf_id)): Path<(Uuid, u64)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::WorkflowDismiss { instance_id: wf_id })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        message: resp.message,
        error: resp.error,
    }))
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Optional scope to a single running instance id.
    #[serde(default)]
    pub instance: Option<u64>,
}

/// GET /api/instances/:id/workflows/events[?instance=<wf_id>]
///
/// Opens an SSE stream against the webservice's
/// `/v1/workflow/events` endpoint and forwards the raw SSE byte stream
/// to the browser. Mirrors [`crate::routes::chat::events`].
pub async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<EventsQuery>,
) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_workflow_events(q.instance).await?;

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

    // Adapt anyhow error stream to an infallible stream — errors close
    // the SSE connection cleanly, and the browser's `EventSource`
    // auto-reconnects.
    let byte_stream = stream.map(|res| match res {
        Ok(b) => Ok::<_, Infallible>(b),
        Err(e) => {
            tracing::debug!(error = %e, "upstream workflow event stream ended with error");
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
