//! Per-webhook request handler factory.
//!
//! Each configured [`Webhook`] is turned into an axum-compatible
//! handler closure. The handler:
//!
//! 1. Buffers the request body (up to `max_body_bytes`, default 1 MiB).
//! 2. Runs the declared auth check (bearer / header / json-field /
//!    HMAC-signed header). Unauthorised requests return `401` with an
//!    empty body.
//! 3. Wraps the request (method, path, query, headers, body) into a
//!    JSON envelope.
//! 4. Sends an [`AgentSocketEvent`] carrying a
//!    [`RequestCmd::SendMessage`] to the shared `events_tx`, stamped
//!    with the webservice automaton's `session_id`. The mail routes
//!    through the same handler that processes script-backed mail, so
//!    the mount automaton's `can_message` ACL is the trust boundary
//!    for which agents a webhook can reach.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::Sha256;
use tokio::sync::oneshot;

use crate::agent_ipc::{AgentSocketEvent, Request as IpcRequest, RequestCmd};
use crate::config::{HmacAlgo, Webhook, WebhookAuth};

use super::WebserviceState;

const DEFAULT_MAX_BODY_BYTES: usize = 1 << 20; // 1 MiB

/// Produce an axum handler closure for a single webhook. The returned
/// closure borrows the webhook by `Arc` so it survives the router
/// lifetime, and captures the state for forwarding.
#[allow(clippy::type_complexity)]
pub fn make_handler(
    hook: Webhook,
    _state: WebserviceState,
) -> impl Fn(
    State<WebserviceState>,
    Query<BTreeMap<String, String>>,
    HeaderMap,
    Bytes,
) -> futures::future::BoxFuture<'static, Response>
       + Clone
       + Send
       + 'static {
    let hook = Arc::new(hook);
    move |State(state): State<WebserviceState>,
          Query(query): Query<BTreeMap<String, String>>,
          headers: HeaderMap,
          body: Bytes| {
        let hook = hook.clone();
        Box::pin(async move { handle(state, hook, query, headers, body).await })
    }
}

async fn handle(
    state: WebserviceState,
    hook: Arc<Webhook>,
    query: BTreeMap<String, String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let max = hook.max_body_bytes.unwrap_or(DEFAULT_MAX_BODY_BYTES);
    if body.len() > max {
        return (StatusCode::PAYLOAD_TOO_LARGE, "").into_response();
    }

    let auth_header_name = declared_auth_header(&hook.auth);
    if let Err(status) = check_auth(&hook.auth, &headers, &body) {
        return (status, "").into_response();
    }

    let envelope = build_envelope(&hook, &headers, auth_header_name, &query, &body);
    let envelope_text = match serde_json::to_string(&envelope) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("envelope serialization failed: {e}"),
            )
                .into_response()
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    let req = IpcRequest {
        agent_id: state.session_id,
        cmd: RequestCmd::SendMessage {
            to: hook.deliver_to.clone(),
            body: envelope_text,
        },
    };
    if state
        .events_tx
        .send(AgentSocketEvent {
            request: req,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "delivered": false,
                "error": "orchestrator shutting down",
            })),
        )
            .into_response();
    }
    match reply_rx.await {
        Ok(resp) if resp.ok => (
            StatusCode::OK,
            Json(serde_json::json!({ "delivered": true })),
        )
            .into_response(),
        Ok(resp) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "delivered": false,
                "error": resp.error.unwrap_or_default(),
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "delivered": false,
                "error": "no reply from orchestrator",
            })),
        )
            .into_response(),
    }
}

fn declared_auth_header(auth: &WebhookAuth) -> Option<String> {
    match auth {
        WebhookAuth::None => None,
        WebhookAuth::Bearer { .. } => Some("authorization".to_string()),
        WebhookAuth::Header { header, .. } | WebhookAuth::HmacHeader { header, .. } => {
            Some(header.to_ascii_lowercase())
        }
        WebhookAuth::JsonField { .. } => None,
    }
}

fn check_auth(auth: &WebhookAuth, headers: &HeaderMap, body: &[u8]) -> Result<(), StatusCode> {
    match auth {
        WebhookAuth::None => Ok(()),
        WebhookAuth::Bearer { secret_env } => {
            let expected = env_nonempty(secret_env).ok_or(StatusCode::UNAUTHORIZED)?;
            let got = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .unwrap_or("");
            if super::http_ct_eq(got.as_bytes(), expected.as_bytes()) {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        WebhookAuth::Header { header, secret_env } => {
            let expected = env_nonempty(secret_env).ok_or(StatusCode::UNAUTHORIZED)?;
            let got = headers
                .get(header.as_str())
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if super::http_ct_eq(got.as_bytes(), expected.as_bytes()) {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        WebhookAuth::JsonField { field, secret_env } => {
            let expected = env_nonempty(secret_env).ok_or(StatusCode::UNAUTHORIZED)?;
            let parsed: serde_json::Value =
                serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)?;
            let got = walk_json_path(&parsed, field)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if super::http_ct_eq(got.as_bytes(), expected.as_bytes()) {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        WebhookAuth::HmacHeader {
            header,
            secret_env,
            algo,
        } => {
            let secret = env_nonempty(secret_env).ok_or(StatusCode::UNAUTHORIZED)?;
            let raw = headers
                .get(header.as_str())
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            // Strip optional `sha256=` / `sha1=` prefix (GitHub style).
            let hex_sig = raw
                .strip_prefix("sha256=")
                .or_else(|| raw.strip_prefix("sha1="))
                .unwrap_or(raw);
            let sig_bytes = match hex::decode(hex_sig) {
                Ok(v) => v,
                Err(_) => return Err(StatusCode::UNAUTHORIZED),
            };
            let expected = match algo {
                HmacAlgo::Sha256 => {
                    let mut mac: Hmac<Sha256> = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    mac.update(body);
                    mac.finalize().into_bytes().to_vec()
                }
                HmacAlgo::Sha1 => {
                    let mut mac: Hmac<Sha1> = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    mac.update(body);
                    mac.finalize().into_bytes().to_vec()
                }
            };
            if super::http_ct_eq(&sig_bytes, &expected) {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn walk_json_path<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = v;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

#[derive(serde::Serialize)]
struct Envelope<'a> {
    kind: &'static str,
    hook: &'a str,
    path: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    query: &'a BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
    body_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_json: Option<serde_json::Value>,
}

fn build_envelope(
    hook: &Webhook,
    headers: &HeaderMap,
    suppress_header: Option<String>,
    query: &BTreeMap<String, String>,
    body: &[u8],
) -> serde_json::Value {
    let mut hdrs: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in headers.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if suppress_header.as_deref() == Some(lower.as_str()) {
            continue;
        }
        if let Ok(s) = value.to_str() {
            hdrs.insert(lower, s.to_string());
        }
    }

    let (body_text, body_json) = match std::str::from_utf8(body) {
        Ok(text) => {
            let parsed: Option<serde_json::Value> = serde_json::from_str(text).ok();
            (text.to_string(), parsed)
        }
        Err(_) => {
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            (format!("b64:{}", STANDARD.encode(body)), None)
        }
    };

    let envelope = Envelope {
        kind: "webhook",
        hook: &hook.id,
        path: &hook.path,
        query,
        headers: hdrs,
        body_text,
        body_json,
    };
    serde_json::to_value(envelope).unwrap_or(serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_nested_json_paths() {
        let v = serde_json::json!({ "data": { "token": "xyz" } });
        assert_eq!(walk_json_path(&v, "data.token").unwrap(), "xyz");
        assert!(walk_json_path(&v, "data.missing").is_none());
    }
}
