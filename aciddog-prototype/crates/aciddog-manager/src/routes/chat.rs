//! Per-agent chat endpoints: history snapshot, send-as-user, and
//! live SSE passthrough.
//!
//! All three talk to the upstream webservice via the [`crate::client`]
//! transport — nothing about chat state lives on the manager itself.

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
use crate::ipc::{HistorySelector, RequestCmd};
use crate::routes::instances::load_row_for_routes;
use crate::state::AppState;
use crate::wire_chat::WireChatBlock;

#[derive(Debug, Clone, Serialize)]
pub struct ChatHistoryResponse {
    pub ok: bool,
    pub blocks: Vec<WireChatBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/agents/:agent/history
pub async fn history(
    State(state): State<AppState>,
    Path((id, agent)): Path<(Uuid, String)>,
) -> AppResult<Json<ChatHistoryResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::AgentHistoryJson {
            agent: agent.clone(),
            selector: HistorySelector::All,
        })
        .await?;

    if !resp.ok {
        return Ok(Json(ChatHistoryResponse {
            ok: false,
            blocks: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }

    // Structured payload lives in `data`; older servers (pre
    // AgentHistoryJson) would leave it empty — surface that as an
    // actionable error rather than a silent empty chat.
    let Some(data) = resp.data else {
        return Ok(Json(ChatHistoryResponse {
            ok: false,
            blocks: Vec::new(),
            error: Some(
                "remote returned ok but no `data` payload — \
                 instance may predate AgentHistoryJson support"
                    .into(),
            ),
        }));
    };

    let blocks: Vec<WireChatBlock> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding chat history blocks: {e}")))?;

    Ok(Json(ChatHistoryResponse {
        ok: true,
        blocks,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct SendResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/instances/:id/agents/:agent/messages
pub async fn send(
    State(state): State<AppState>,
    Path((id, agent)): Path<(Uuid, String)>,
    Json(req): Json<SendRequest>,
) -> AppResult<Json<SendResponse>> {
    let body = req.body.trim();
    if body.is_empty() {
        return Err(AppError::Validation("body is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::AgentSendAsUser {
            to: agent,
            body: body.to_string(),
        })
        .await?;

    Ok(Json(SendResponse {
        ok: resp.ok,
        error: if resp.ok {
            None
        } else {
            resp.error.or(resp.message)
        },
    }))
}

/// GET /api/instances/:id/agents/:agent/events
///
/// Opens an SSE stream against the webservice's scoped
/// `/v1/chat/events?agent=<id>` endpoint and forwards the raw SSE
/// byte stream to the browser. Raw-byte passthrough keeps server-
/// generated framing (`keepalive` pings, named `lagged` events)
/// intact — the browser's `EventSource` parses it natively.
pub async fn events(
    State(state): State<AppState>,
    Path((id, agent)): Path<(Uuid, String)>,
) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_chat_events(&agent).await?;

    if upstream_status != 200 {
        // Surface upstream rejection (e.g. 403 when the webservice
        // automaton isn't a supervisor) with the same code to the
        // browser — gives the UI the signal it needs to show an
        // actionable message.
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

    // Adapt the anyhow::Error stream to the infallible shape axum's
    // `Body::from_stream` prefers. Errors become stream termination,
    // which closes the SSE connection — the browser will reconnect
    // and re-sync via history.
    let byte_stream = stream.map(|res| match res {
        Ok(b) => Ok::<_, Infallible>(b),
        Err(e) => {
            tracing::debug!(error = %e, "upstream chat event stream ended with error");
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
    // Hint to intermediary proxies (nginx's buffering is the common
    // SSE killer): don't buffer our event stream.
    resp.headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(resp)
}
