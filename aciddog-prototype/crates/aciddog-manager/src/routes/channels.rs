//! Channel endpoints — list, transcript, supervisor-as-user post,
//! and the scoped SSE passthrough.

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
use crate::wire_channel::{WireChannelInfo, WireChannelMessage};

#[derive(Debug, Clone, Serialize)]
pub struct ChannelListResponse {
    pub ok: bool,
    pub channels: Vec<WireChannelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelHistoryResponse {
    pub ok: bool,
    pub messages: Vec<WireChannelMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MutationResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// GET /api/instances/:id/channels
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ChannelListResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::ChannelList).await?;
    if !resp.ok {
        return Ok(Json(ChannelListResponse {
            ok: false,
            channels: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(ChannelListResponse {
            ok: false,
            channels: Vec::new(),
            error: Some(
                "remote returned ok but no `data` — instance may predate \
                 ChannelList support"
                    .into(),
            ),
        }));
    };
    let channels: Vec<WireChannelInfo> = serde_json::from_value(data)
        .map_err(|e| AppError::Remote(anyhow::anyhow!("decoding channel list: {e}")))?;
    Ok(Json(ChannelListResponse {
        ok: true,
        channels,
        error: None,
    }))
}

/// GET /api/instances/:id/channels/:channel/history
pub async fn history(
    State(state): State<AppState>,
    Path((id, channel)): Path<(Uuid, String)>,
) -> AppResult<Json<ChannelHistoryResponse>> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::ChannelHistory {
            channel: channel.clone(),
        })
        .await?;
    if !resp.ok {
        return Ok(Json(ChannelHistoryResponse {
            ok: false,
            messages: Vec::new(),
            error: resp.error.or(resp.message),
        }));
    }
    let Some(data) = resp.data else {
        return Ok(Json(ChannelHistoryResponse {
            ok: false,
            messages: Vec::new(),
            error: Some("remote returned ok but no `data`".into()),
        }));
    };
    let messages: Vec<WireChannelMessage> = serde_json::from_value(data).map_err(|e| {
        AppError::Remote(anyhow::anyhow!("decoding channel history: {e}"))
    })?;
    Ok(Json(ChannelHistoryResponse {
        ok: true,
        messages,
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    pub body: String,
}

/// POST /api/instances/:id/channels/:channel/messages
pub async fn send(
    State(state): State<AppState>,
    Path((id, channel)): Path<(Uuid, String)>,
    Json(req): Json<SendRequest>,
) -> AppResult<Json<MutationResponse>> {
    let body = req.body.trim();
    if body.is_empty() {
        return Err(AppError::Validation("body is empty".into()));
    }
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::ChannelSendAsUser {
            channel,
            body: body.to_string(),
        })
        .await?;
    Ok(Json(MutationResponse {
        ok: resp.ok,
        error: if resp.ok {
            None
        } else {
            resp.error.or(resp.message)
        },
    }))
}

/// GET /api/instances/:id/channels/:channel/events
pub async fn events(
    State(state): State<AppState>,
    Path((id, channel)): Path<(Uuid, String)>,
) -> AppResult<Response> {
    let row = load_row_for_routes(&state, id).await?;
    let client = Client::from_row(&row)?;
    let (upstream_status, stream) = client.open_channel_events(&channel).await?;
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
            tracing::debug!(error = %e, "upstream channel event stream ended with error");
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
