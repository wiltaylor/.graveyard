//! Axum router + transport glue for the webservice.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Request as HttpRequest, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use futures::StreamExt;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use serde_json::Value;
use tokio::net::UnixListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::BroadcastStream;
use tower::util::ServiceExt;
use tracing::warn;

use crate::agent_ipc::{
    AgentSocketEvent, Request as IpcRequest, RequestCmd, Response as IpcResponse,
};
use crate::config::Webhook;

use super::{webhook, WebserviceState};

/// Build the axum router, including RPC, SSE, health, and one route per
/// webhook mounted on this webservice.
pub fn build_router(state: WebserviceState, webhooks: Vec<Webhook>) -> Router {
    let mut router = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/rpc", post(rpc))
        .route("/v1/events", get(events))
        .route("/v1/chat/events", get(chat_events))
        .route("/v1/channel/events", get(channel_events))
        .route("/v1/workflow/events", get(workflow_events));

    for hook in webhooks {
        let path = hook.path.clone();
        let handler = webhook::make_handler(hook, state.clone());
        router = router.route(&path, post(handler));
    }

    // Auth middleware only enforces when `state.api_key` is `Some`.
    router
        .layer(middleware::from_fn_with_state(state.clone(), auth_layer))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn rpc(State(state): State<WebserviceState>, Json(body): Json<Value>) -> impl IntoResponse {
    // The client sends the RequestCmd flattened (same shape the internal
    // Unix IPC accepts). We drop any client-supplied `agent_id` and
    // stamp the webservice's session id.
    let cmd: RequestCmd = match serde_json::from_value(body) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(IpcResponse::err(format!("malformed request: {e}"))),
            )
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    let request = IpcRequest {
        agent_id: state.session_id,
        cmd,
    };
    if state
        .events_tx
        .send(AgentSocketEvent {
            request,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(IpcResponse::err("orchestrator is shutting down")),
        );
    }

    match reply_rx.await {
        Ok(resp) => {
            let status = if resp.ok {
                StatusCode::OK
            } else {
                StatusCode::BAD_GATEWAY
            };
            (status, Json(resp))
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(IpcResponse::err("no reply from orchestrator")),
        ),
    }
}

async fn events(
    State(state): State<WebserviceState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.broadcast_tx.subscribe();
    let events = BroadcastStream::new(rx).filter_map(|msg| async move {
        match msg {
            Ok(ev) => {
                // `json_data` only errors if serde fails — our concrete
                // type is serializable, so this is effectively
                // infallible.
                Event::default().json_data(ev).ok().map(Ok)
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                let payload = serde_json::json!({ "kind": "lagged", "dropped": n });
                Event::default()
                    .event("lagged")
                    .json_data(payload)
                    .ok()
                    .map(Ok)
            }
        }
    });
    let stream = futures::stream::once(async { Ok::<_, Infallible>(ready_event()) }).chain(events);
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}

#[derive(Debug, serde::Deserialize)]
struct ChatEventsQuery {
    /// `display_name` of the target chat. Must match exactly (the same
    /// id used by `AgentSendAsUser`, `AgentHistory`, etc.).
    agent: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChannelEventsQuery {
    /// Channel id. Must match a declared or runtime-created channel.
    channel: String,
}

#[derive(Debug, serde::Deserialize)]
struct WorkflowEventsQuery {
    /// Optional instance id filter. When `None`, the stream returns
    /// events for every running instance.
    #[serde(default)]
    instance: Option<u64>,
}

/// Supervisor-only scoped SSE stream of `ChannelMessageEvent`s.
/// Filters the shared `App::channel_message_tx` broadcaster down to
/// events whose `channel` field matches the query string.
async fn channel_events(
    State(state): State<WebserviceState>,
    Query(q): Query<ChannelEventsQuery>,
) -> Response {
    if !state.is_supervisor {
        return (
            StatusCode::FORBIDDEN,
            Json(IpcResponse::err(
                "this webservice automaton is not flagged `supervisor = true`",
            )),
        )
            .into_response();
    }
    let wanted = q.channel;
    let rx = state.channel_message_tx.subscribe();
    let events = BroadcastStream::new(rx).filter_map(move |msg| {
        let wanted = wanted.clone();
        async move {
            match msg {
                Ok(ev) if ev.channel == wanted => Event::default().json_data(&ev).ok().map(Ok),
                Ok(_) => None,
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    let payload = serde_json::json!({ "kind": "lagged", "dropped": n });
                    Event::default()
                        .event("lagged")
                        .json_data(payload)
                        .ok()
                        .map(Ok::<_, Infallible>)
                }
            }
        }
    });
    let stream = futures::stream::once(async { Ok::<_, Infallible>(ready_event()) }).chain(events);
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

/// Supervisor-only SSE stream of workflow events. Optional `instance`
/// query parameter scopes the stream to a single running instance;
/// without it every event for every instance is forwarded.
///
/// The stream opens with a named `ready` frame so the browser's
/// `EventSource.onopen` fires immediately. Without it a client
/// connecting to an idle broadcaster (no declared workflows, no
/// running instances) would sit in CONNECTING for up to the keepalive
/// interval — the 15s of silence reads like a hang.
async fn workflow_events(
    State(state): State<WebserviceState>,
    Query(q): Query<WorkflowEventsQuery>,
) -> Response {
    if !state.is_supervisor {
        return (
            StatusCode::FORBIDDEN,
            Json(IpcResponse::err(
                "this webservice automaton is not flagged `supervisor = true`",
            )),
        )
            .into_response();
    }
    let wanted = q.instance;
    let rx = state.workflow_event_tx.subscribe();
    let events = BroadcastStream::new(rx).filter_map(move |msg| async move {
        match msg {
            Ok(ev) => {
                if let Some(id) = wanted {
                    if ev.instance_id() != id {
                        return None;
                    }
                }
                Event::default().json_data(&ev).ok().map(Ok)
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                let payload = serde_json::json!({ "kind": "lagged", "dropped": n });
                Event::default()
                    .event("lagged")
                    .json_data(payload)
                    .ok()
                    .map(Ok::<_, Infallible>)
            }
        }
    });
    let stream = futures::stream::once(async { Ok::<_, Infallible>(ready_event()) }).chain(events);
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

/// Zero-payload `ready` frame. Emitted as the first event on every
/// scoped SSE stream so `EventSource.onopen` fires immediately,
/// regardless of how long the broadcaster stays idle.
fn ready_event() -> Event {
    Event::default().event("ready").data("")
}

/// Supervisor-only scoped SSE stream of `ChatBlockEvent`s. Filters the
/// shared `App::chat_block_tx` broadcaster down to events whose
/// `agent` field matches the query string — so the manager portal
/// only sees the chat it's currently viewing.
async fn chat_events(
    State(state): State<WebserviceState>,
    Query(q): Query<ChatEventsQuery>,
) -> Response {
    if !state.is_supervisor {
        return (
            StatusCode::FORBIDDEN,
            Json(IpcResponse::err(
                "this webservice automaton is not flagged `supervisor = true`",
            )),
        )
            .into_response();
    }
    let wanted = q.agent;
    let rx = state.chat_block_tx.subscribe();
    let events = BroadcastStream::new(rx).filter_map(move |msg| {
        let wanted = wanted.clone();
        async move {
            match msg {
                Ok(ev) if ev.agent == wanted => Event::default().json_data(&ev).ok().map(Ok),
                Ok(_) => None,
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    let payload = serde_json::json!({ "kind": "lagged", "dropped": n });
                    Event::default()
                        .event("lagged")
                        .json_data(payload)
                        .ok()
                        .map(Ok::<_, Infallible>)
                }
            }
        }
    });
    let stream = futures::stream::once(async { Ok::<_, Infallible>(ready_event()) }).chain(events);
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

/// Verify `Authorization: Bearer <api_key>` when the webservice has an
/// api_key configured. Short-circuits for Unix-socket webservices where
/// `state.api_key` is `None` (filesystem perms are the trust boundary).
async fn auth_layer(
    State(state): State<WebserviceState>,
    headers: HeaderMap,
    req: HttpRequest<Body>,
    next: Next,
) -> Response {
    if let Some(expected) = state.api_key.as_deref() {
        let header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let got = header.strip_prefix("Bearer ").unwrap_or("");
        let ok = super::http_ct_eq(got.as_bytes(), expected.as_bytes());
        if !ok {
            return (StatusCode::UNAUTHORIZED, "").into_response();
        }
    }
    next.run(req).await
}

/// Bind an HTTPS listener with the given TLS material and serve the
/// router until `shutdown` fires.
pub async fn serve_https(
    address: String,
    tls_cert: PathBuf,
    tls_key: PathBuf,
    router: Router,
    shutdown: oneshot::Receiver<()>,
) -> Result<()> {
    let addr: SocketAddr = address
        .parse()
        .with_context(|| format!("parsing bind address {address}"))?;
    let tls_config = super::tls::load_tls_config(&tls_cert, &tls_key)
        .await
        .context("loading TLS material")?;
    let rustls = RustlsConfig::from_config(std::sync::Arc::new(tls_config));

    let handle = axum_server::Handle::new();
    let handle_for_shutdown = handle.clone();
    tokio::spawn(async move {
        if shutdown.await.is_ok() {
            handle_for_shutdown.graceful_shutdown(Some(Duration::from_secs(1)));
        }
    });

    axum_server::bind_rustls(addr, rustls)
        .handle(handle)
        .serve(router.into_make_service())
        .await
        .context("serving HTTPS")?;
    Ok(())
}

/// Bind a Unix socket listener and serve the router over per-connection
/// hyper tasks until `shutdown` fires.
pub async fn serve_unix(
    path: PathBuf,
    router: Router,
    mut shutdown: oneshot::Receiver<()>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        // Only chmod the parent if we created it. Otherwise a
        // user-chosen path like `/tmp/foo.sock` would try to restrict
        // `/tmp` itself, which fails with EPERM for non-root users.
        // The 0o660 mode on the socket file itself is the real trust
        // boundary; this extra 0o700 is defence-in-depth for
        // app-owned runtime directories.
        let existed = tokio::fs::metadata(parent).await.is_ok();
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating socket parent {}", parent.display()))?;
        if !existed {
            let mut perms = tokio::fs::metadata(parent).await?.permissions();
            perms.set_mode(0o700);
            tokio::fs::set_permissions(parent, perms).await?;
        }
    }
    if path.exists() {
        if let Ok(_stream) = tokio::net::UnixStream::connect(&path).await {
            return Err(anyhow!(
                "webservice unix socket {} is already in use by a running listener",
                path.display()
            ));
        }
        tokio::fs::remove_file(&path)
            .await
            .with_context(|| format!("removing stale socket {}", path.display()))?;
    }
    let listener = UnixListener::bind(&path)
        .with_context(|| format!("binding UnixListener at {}", path.display()))?;
    let mut file_perms = tokio::fs::metadata(&path).await?.permissions();
    file_perms.set_mode(0o660);
    tokio::fs::set_permissions(&path, file_perms).await?;

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accept = listener.accept() => {
                let (stream, _) = match accept {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e, "webservice unix accept error");
                        continue;
                    }
                };
                let router = router.clone();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req: HttpRequest<hyper::body::Incoming>| {
                        let router = router.clone();
                        async move {
                            // Convert the hyper body to an axum body, then run the
                            // router as a one-shot tower service.
                            let (parts, body) = req.into_parts();
                            let req = HttpRequest::from_parts(parts, Body::new(body));
                            router.oneshot(req).await
                        }
                    });
                    if let Err(e) = auto::Builder::new(TokioExecutor::new())
                        .serve_connection(io, svc)
                        .await
                    {
                        warn!(error = %e, "webservice unix connection error");
                    }
                });
            }
        }
    }

    let _ = tokio::fs::remove_file(&path).await;
    Ok(())
}
