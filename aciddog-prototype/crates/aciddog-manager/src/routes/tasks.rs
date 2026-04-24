//! Task board endpoints — REST wrappers over the aciddog
//! `Task*` RPCs. Every mutation round-trips a single RPC; the
//! caller expects to re-fetch (or refresh optimistically) after
//! the response.

use axum::extract::{Path, State};
use axum::Json;
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
            note: req.note,
            add_blocked_by: req.add_blocked_by,
            remove_blocked_by: req.remove_blocked_by,
        })
        .await?;
    Ok(Json(MutationResponse::from_ipc(resp)))
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
