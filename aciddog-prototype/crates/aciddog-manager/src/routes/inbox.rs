//! User-inbox endpoints and the instance-wide SSE passthrough.

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
use crate::wire_inbox::WireInboxMessage;

#[derive(Debug, Clone, Serialize)]
pub struct InboxListResponse {
    pub ok: bool,
    pub messages: Vec<WireInboxMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MutationResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/inbox
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<InboxListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::InboxList).await?;
    if !resp.ok {
        return Ok(Json(InboxListResponse {
            ok: false,
            messages: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(InboxListResponse {
            ok: false,
            messages: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 InboxList support"
                    .into(),
            ),
        }));
    };
    let messages: Vec<WireInboxMessage> = serde_json::from_value(data).map_err(|e| {
        AppError::Remote(anyhow::anyhow!("decoding inbox list: {e}"))
    })?;
    Ok(Json(InboxListResponse {
        ok: true,
        messages,
        error: None,
    }))
}

/// DELETE /api/instances/:id/inbox/:index
pub async fn delete(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, usize)>,
) -> AppResult<Json<MutationResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::InboxDelete { index }).await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        error: if resp.ok {
            None
        } else {
            resp.error.or(resp.message)
        },
    }))
}

#[derive(Debug, Deserialize)]
pub struct ReplyRequest {
    pub body: String,
}

/// POST /api/instances/:id/inbox/:index/reply
///
/// Resolves the source message (to fetch the sender), issues
/// `AgentSendAsUser { to: sender, body }`, then `InboxDelete { index }`
/// on success. Mirrors the TUI's `r` → edit → send flow.
pub async fn reply(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, usize)>,
    Json(req): Json<ReplyRequest>,
) -> AppResult<Json<MutationResponse>> {
    let body = req.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::Validation("body is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    // Resolve the original sender from the inbox.
    let list_resp = client.send_rpc(&RequestCmd::InboxList).await?;
    if !list_resp.ok {
        return Ok(Json(MutationResponse {
            ok: false,
            error: list_resp.error.or(list_resp.message),
        }));
    }
    let messages: Vec<WireInboxMessage> = list_resp
        .data
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding inbox list: {e}")))?
        .unwrap_or_default();
    let Some(source) = messages.get(index) else {
        return Err(AppError::Validation(format!(
            "inbox index {index} out of range"
        )));
    };
    let to = source.from.clone();
    let send_resp = client
        .send_rpc(&RequestCmd::AgentSendAsUser { to, body })
        .await?;
    if !send_resp.ok {
        return Ok(Json(MutationResponse {
            ok: false,
            error: send_resp.error.or(send_resp.message),
        }));
    }
    let _ = client.send_rpc(&RequestCmd::InboxDelete { index }).await?;
    Ok(Json(MutationResponse {
        ok: true,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ForwardRequest {
    pub to: String,
    pub body: String,
}

/// POST /api/instances/:id/inbox/:index/forward
pub async fn forward(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, usize)>,
    Json(req): Json<ForwardRequest>,
) -> AppResult<Json<MutationResponse>> {
    let body = req.body.trim().to_string();
    let to = req.to.trim().to_string();
    if body.is_empty() {
        return Err(AppError::Validation("body is empty".into()));
    }
    if to.is_empty() {
        return Err(AppError::Validation("to is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let send_resp = client
        .send_rpc(&RequestCmd::AgentSendAsUser { to, body })
        .await?;
    if !send_resp.ok {
        return Ok(Json(MutationResponse {
            ok: false,
            error: send_resp.error.or(send_resp.message),
        }));
    }
    let _ = client.send_rpc(&RequestCmd::InboxDelete { index }).await?;
    Ok(Json(MutationResponse {
        ok: true,
        error: None,
    }))
}

/// GET /api/instances/:id/events
///
/// SSE passthrough for the webservice's instance-wide `/v1/events`
/// stream. Carries `UserMessage`, `AgentMessage`, and
/// `ChannelMessage` frames. The Inbox view subscribes and refetches
/// the inbox on any `user_message` event.
pub async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_instance_events().await?;
    if upstream_status != 200 {
        let status =
            StatusCode::from_u16(upstream_status).unwrap_or(StatusCode::BAD_GATEWAY);
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
    let byte_stream = stream.map(|res| match res {
        Ok(b) => Ok::<_, Infallible>(b),
        Err(e) => {
            tracing::debug!(error = %e, "upstream instance event stream ended with error");
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
