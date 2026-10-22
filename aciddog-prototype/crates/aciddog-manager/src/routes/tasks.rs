//! Task board endpoints — REST wrappers over the aciddog
//! `Task*` RPCs. Every mutation round-trips a single RPC; the
//! caller expects to re-fetch (or refresh optimistically) after
//! the response.

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
use crate::state::AppState;
use crate::wire_tasks::Task;

#[derive(Debug, Clone, Serialize)]
pub struct TaskListResponse {
    pub ok: bool,
    pub tasks: Vec<Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MutationResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl MutationResponse {
    fn from_ipc(resp: crate::ipc::IpcResponse) -> Self {
        Self {
            ok: resp.ok,
            message: resp.message,
            error: resp.error,
        }
    }
}

/// GET /api/instances/:id/tasks
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TaskListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskListJson {
            assignee: None,
            status: None,
            mine: false,
            unassigned: false,
        })
        .await?;
    if !resp.ok {
        return Ok(Json(TaskListResponse {
            ok: false,
            tasks: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(TaskListResponse {
            ok: false,
            tasks: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 TaskListJson support"
                    .into(),
            ),
        }));
    };
    let tasks: Vec<Task> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding task list: {e}")))?;
    Ok(Json(TaskListResponse {
        ok: true,
        tasks,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub blocked_by: Vec<Uuid>,
    /// Optional initial status. The daemon currently only honors
    /// `"planning"` here (other statuses are auto-derived from the
    /// assignee).
    #[serde(default)]
    pub status: Option<String>,
    /// Pre-stored handoff target for a `planning` task; required
    /// when `status` is `"planning"`.
    #[serde(default)]
    pub planned_assignee: Option<String>,
}

/// POST /api/instances/:id/tasks
pub async fn create(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateTaskRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.title.trim().is_empty() {
        return Err(AppError::Validation("title is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskAdd {
            title: req.title,
            description: req.description.filter(|s| !s.trim().is_empty()),
            assignee: req.assignee.filter(|s| !s.trim().is_empty()),
            blocked_by: req.blocked_by,
            status: req.status.filter(|s| !s.trim().is_empty()),
            planned_assignee: req.planned_assignee.filter(|s| !s.trim().is_empty()),
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct PatchTaskRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub unassign: bool,
    /// Revise the pre-stored handoff target on a `planning` task.
    #[serde(default)]
    pub planned_assignee: Option<String>,
    /// Clear `planned_assignee`. Mutually exclusive with the above.
    #[serde(default)]
    pub clear_planned_assignee: bool,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub add_blocked_by: Vec<Uuid>,
    #[serde(default)]
    pub remove_blocked_by: Vec<Uuid>,
}

/// PATCH /api/instances/:id/tasks/:task_id
pub async fn patch(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<PatchTaskRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskUpdate {
            id: task_id,
            title: req.title,
            description: req.description,
            status: req.status,
            assignee: req.assignee,
            unassign: req.unassign,
            planned_assignee: req.planned_assignee,
            clear_planned_assignee: req.clear_planned_assignee,
            note: req.note,
            add_blocked_by: req.add_blocked_by,
            remove_blocked_by: req.remove_blocked_by,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// POST /api/instances/:id/tasks/:task_id/approve
///
/// Approve a `planning` task — transitions status to `approved`
/// and routes the assignee to the previously-stored
/// `planned_assignee`. Wraps a `task_update` RPC: status="approved",
/// assignee=current_planned_assignee, clear_planned_assignee=true.
/// The web manager fetches the task client-side to read the
/// `planned_assignee` value before issuing this call.
pub async fn approve(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<ApproveTaskRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.assignee.trim().is_empty() {
        return Err(AppError::Validation(
            "approve: missing assignee (the planned_assignee from the task)".into(),
        ));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskUpdate {
            id: task_id,
            title: None,
            description: None,
            status: Some("approved".into()),
            assignee: Some(req.assignee.clone()),
            unassign: false,
            planned_assignee: None,
            clear_planned_assignee: true,
            note: Some(format!("approved by user; routed to {}", req.assignee)),
            add_blocked_by: vec![],
            remove_blocked_by: vec![],
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct ApproveTaskRequest {
    /// The `planned_assignee` value from the task being approved.
    /// The client passes it explicitly so the daemon doesn't need
    /// a dedicated approve RPC.
    pub assignee: String,
}

/// POST /api/instances/:id/tasks/:task_id/reject
///
/// Reject a `planning` task. The reason becomes a `Rejected: ...`
/// comment and the task is reassigned back to its `created_by` for
/// revision. Status stays `planning`. Empty reasons are rejected.
pub async fn reject(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<RejectTaskRequest>,
) -> AppResult<Json<MutationResponse>> {
    if req.reason.trim().is_empty() {
        return Err(AppError::Validation("reject: reason is required".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskReject {
            id: task_id,
            reason: req.reason,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct RejectTaskRequest {
    pub reason: String,
}

/// DELETE /api/instances/:id/tasks/:task_id
pub async fn delete(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskRemove { id: task_id })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct BodyOnlyRequest {
    pub body: String,
}

/// POST /api/instances/:id/tasks/:task_id/notes
pub async fn add_note(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<BodyOnlyRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskNote {
            id: task_id,
            body: req.body,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// POST /api/instances/:id/tasks/:task_id/todos
pub async fn add_todo(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<BodyOnlyRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskTodoAdd {
            task: task_id,
            body: req.body,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

#[derive(Debug, Deserialize)]
pub struct PatchTodoRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

/// PATCH /api/instances/:id/tasks/:task_id/todos/:todo_id
pub async fn patch_todo(
    State(state): State<AppState>,
    Path((id, task_id, todo_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<PatchTodoRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskTodoUpdate {
            task: task_id,
            id: todo_id,
            status: req.status,
            body: req.body,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// DELETE /api/instances/:id/tasks/:task_id/todos/:todo_id
pub async fn delete_todo(
    State(state): State<AppState>,
    Path((id, task_id, todo_id)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskTodoRemove {
            task: task_id,
            id: todo_id,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// POST /api/instances/:id/tasks/:task_id/comments
pub async fn add_comment(
    State(state): State<AppState>,
    Path((id, task_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<BodyOnlyRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskCommentAdd {
            task: task_id,
            body: req.body,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// PATCH /api/instances/:id/tasks/:task_id/comments/:comment_id
pub async fn patch_comment(
    State(state): State<AppState>,
    Path((id, task_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<BodyOnlyRequest>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskCommentUpdate {
            task: task_id,
            id: comment_id,
            body: req.body,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// DELETE /api/instances/:id/tasks/:task_id/comments/:comment_id
pub async fn delete_comment(
    State(state): State<AppState>,
    Path((id, task_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::TaskCommentRemove {
            task: task_id,
            id: comment_id,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
}

/// GET /api/instances/:id/tasks/events
///
/// Opens an SSE stream against the webservice's `/v1/tasks/events`
/// endpoint and forwards the raw byte stream to the browser. Mirrors
/// `routes::cron::events` structurally — non-200 upstream statuses
/// passthrough as the same status with the (truncated) body, while
/// 200 wraps the bytes in a `text/event-stream` response with
/// no-cache headers.
pub async fn events(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_tasks_events().await?;

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
            tracing::debug!(error = %e, "upstream task event stream ended with error");
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
