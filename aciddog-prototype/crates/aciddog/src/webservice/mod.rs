//! HTTP/Unix-socket webservice backend for automatons.
//!
//! A webservice-backed automaton (configured via `automaton { type = :webservice ... }`
//! in WCL) is a fleet member whose identity is represented by a long-lived
//! HTTP server rather than a per-event subprocess. The server exposes:
//!
//! * `POST /v1/rpc` — mirrors the internal agent IPC JSON protocol.
//!   Requests are stamped with the automaton's `session_id` and flow into
//!   the same handler pipeline used by the Unix-socket agent IPC.
//! * `GET /v1/events` — Server-Sent Events stream delivering the mail /
//!   channel traffic this automaton would otherwise receive on stdin.
//! * `GET /v1/health` — liveness probe.
//! * Any number of configured webhook routes that mount on this same
//!   listener, auth-gate incoming requests, and deliver the request
//!   envelope as a `SendMessage` to a configured target.

pub mod http;
pub mod tls;
pub mod webhook;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};
use uuid::Uuid;

use crate::agent_ipc::AgentSocketEvent;
use crate::ai::event::AIEvent;
use crate::config::{Automaton, AutomatonBackend, Webhook, WebserviceBind};

/// Capacity of the SSE broadcaster. A slow/disconnected subscriber may
/// lose events past this backlog — `BroadcastStream` reports a Lagged
/// error which the handler converts into a `data: {"lagged":N}` frame.
const BROADCAST_CAPACITY: usize = 256;

/// Event pushed to every SSE subscriber when this automaton receives
/// mail. Shape mirrors the JSON envelope a script-backed automaton
/// sees on stdin. Also re-used by the `:remote` client automaton
/// when deserializing the peer's SSE stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboundEvent {
    UserMessage {
        body: String,
        ts: String,
    },
    AgentMessage {
        from: String,
        body: String,
        ts: String,
    },
    ChannelMessage {
        channel: String,
        from: String,
        body: String,
        ts: String,
    },
}

/// Shared state handed to every axum handler.
#[derive(Clone)]
pub struct WebserviceState {
    pub session_id: Uuid,
    pub events_tx: mpsc::Sender<AgentSocketEvent>,
    pub broadcast_tx: broadcast::Sender<InboundEvent>,
    /// `Some(key)` when the webservice is HTTPS-bound with an API key
    /// resolved from the env at startup. `None` for Unix-socket binds —
    /// auth middleware short-circuits in that case.
    pub api_key: Option<Arc<String>>,
}

/// Entry point spawned by the TUI startup loop for each
/// webservice-backed automaton. Owns the accept loop, the mail-reception
/// loop, and the SSE broadcaster.
#[allow(clippy::too_many_arguments)]
pub async fn run_webservice_core(
    idx: usize,
    automaton: Automaton,
    session_id: Uuid,
    webhooks: Vec<Webhook>,
    mut user_rx: mpsc::UnboundedReceiver<String>,
    mut agent_rx: mpsc::Receiver<String>,
    mut channel_rx: mpsc::Receiver<String>,
    mut cancel_rx: mpsc::Receiver<()>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
    events_tx: mpsc::Sender<AgentSocketEvent>,
) {
    let (bind, api_key_env) = match automaton.backend {
        AutomatonBackend::Webservice { bind, api_key_env } => (bind, api_key_env),
        AutomatonBackend::Script { .. } | AutomatonBackend::Remote { .. } => {
            warn!(automaton = %automaton.id, "run_webservice_core called with non-Webservice backend; ignoring");
            return;
        }
    };

    let api_key = match (&bind, api_key_env.as_deref()) {
        (WebserviceBind::Https { .. }, Some(env_name)) => match std::env::var(env_name) {
            Ok(v) if !v.is_empty() => Some(Arc::new(v)),
            _ => {
                let _ = ai_tx
                    .send((
                        idx,
                        AIEvent::AutomatonStderr {
                            line: format!(
                                "webservice `{}` api_key_env `{env_name}` is unset or empty; refusing to bind",
                                automaton.id
                            ),
                        },
                    ))
                    .await;
                return;
            }
        },
        (WebserviceBind::UnixSocket { .. }, _) => None,
        (WebserviceBind::Https { .. }, None) => None,
    };

    let (broadcast_tx, _) = broadcast::channel::<InboundEvent>(BROADCAST_CAPACITY);

    let state = WebserviceState {
        session_id,
        events_tx,
        broadcast_tx: broadcast_tx.clone(),
        api_key,
    };

    let router = http::build_router(state, webhooks);

    // Listener task. We spawn it so cancellation + mail-reception can
    // run independently. A oneshot is used to signal shutdown.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let bind_desc = describe_bind(&bind);
    let listener_handle = tokio::spawn({
        let automaton_id = automaton.id.clone();
        let ai_tx = ai_tx.clone();
        async move {
            match &bind {
                WebserviceBind::Https {
                    address,
                    tls_cert,
                    tls_key,
                } => {
                    if let Err(e) = http::serve_https(
                        address.clone(),
                        tls_cert.clone(),
                        tls_key.clone(),
                        router,
                        shutdown_rx,
                    )
                    .await
                    {
                        let _ = ai_tx
                            .send((
                                idx,
                                AIEvent::AutomatonStderr {
                                    line: format!(
                                        "webservice `{automaton_id}` https listener exited: {e}"
                                    ),
                                },
                            ))
                            .await;
                    }
                }
                WebserviceBind::UnixSocket { path } => {
                    if let Err(e) = http::serve_unix(path.clone(), router, shutdown_rx).await {
                        let _ = ai_tx
                            .send((
                                idx,
                                AIEvent::AutomatonStderr {
                                    line: format!(
                                        "webservice `{automaton_id}` unix listener exited: {e}"
                                    ),
                                },
                            ))
                            .await;
                    }
                }
            }
        }
    });

    info!(automaton = %automaton.id, bind = %bind_desc, "webservice listener bound");
    let _ = ai_tx
        .send((
            idx,
            AIEvent::AutomatonStdout {
                line: format!("webservice listening on {bind_desc}"),
            },
        ))
        .await;
    // Don't emit `AutomatonStarted` — that event is for script-backed
    // automatons to mark a per-event child spawn, and flips the pane
    // to `Working` until a matching `AutomatonExit` arrives. A
    // webservice listener is long-lived and has no matching exit on
    // each request, so the pane would stay stuck at `Working`.
    // Default `Ready` is the right status for a listening webservice.

    // Mail-reception loop: forward each incoming message to the SSE
    // broadcaster and also log a single stdout line in the TUI pane so
    // the operator can see traffic without subscribing.
    loop {
        let ev = tokio::select! {
            biased;
            _ = cancel_rx.recv() => break,
            Some(m) = user_rx.recv() => InboundEvent::UserMessage {
                body: m,
                ts: chrono::Utc::now().to_rfc3339(),
            },
            Some(m) = agent_rx.recv() => {
                let from = extract_attr(&m, "from").unwrap_or_default();
                let body = strip_outer_tag(&m, "Message").to_string();
                InboundEvent::AgentMessage {
                    from,
                    body,
                    ts: chrono::Utc::now().to_rfc3339(),
                }
            }
            Some(m) = channel_rx.recv() => {
                let from = extract_attr(&m, "from").unwrap_or_default();
                let channel = extract_attr(&m, "channel").unwrap_or_default();
                let body = strip_outer_tag(&m, "ChannelMessage").to_string();
                InboundEvent::ChannelMessage {
                    channel,
                    from,
                    body,
                    ts: chrono::Utc::now().to_rfc3339(),
                }
            }
            else => break,
        };

        let label = match &ev {
            InboundEvent::UserMessage { body, .. } => format!("user → {body}"),
            InboundEvent::AgentMessage { from, body, .. } => format!("{from} → {body}"),
            InboundEvent::ChannelMessage {
                channel,
                from,
                body,
                ..
            } => format!("#{channel} ({from}) → {body}"),
        };
        let _ = ai_tx
            .send((idx, AIEvent::AutomatonStdout { line: label }))
            .await;

        // `send` only fails when there are zero subscribers — that's
        // fine, the message is simply dropped.
        let _ = broadcast_tx.send(ev);
    }

    let _ = shutdown_tx.send(());
    listener_handle.abort();
    let _ = ai_tx
        .send((
            idx,
            AIEvent::AutomatonExit {
                code: Some(0),
                duration_ms: 0,
                timed_out: false,
            },
        ))
        .await;
}

/// Constant-time byte-slice comparison. Returns `false` on length
/// mismatch (no short-circuit on content).
pub(crate) fn http_ct_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).unwrap_u8() == 1
}

fn describe_bind(bind: &WebserviceBind) -> String {
    match bind {
        WebserviceBind::Https { address, .. } => format!("https://{address}"),
        WebserviceBind::UnixSocket { path } => format!("unix:{}", path.display()),
    }
}

/// Extract the `attr="value"` inner value out of a `<Message>` /
/// `<ChannelMessage>` wrapper. Returns `None` on any structural
/// mismatch.
fn extract_attr(wrapper: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = wrapper.find(&needle)? + needle.len();
    let rest = &wrapper[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Strip `<Tag …>body</Tag>` wrappers. Returns the original string on
/// any structural mismatch so we still emit *something*.
pub(crate) fn strip_outer_tag<'a>(wrapper: &'a str, tag: &str) -> &'a str {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    if let Some(open_end) = wrapper.find('>').map(|i| i + 1) {
        if wrapper.starts_with(&open) && wrapper.ends_with(&close) {
            let body_end = wrapper.len() - close.len();
            if body_end > open_end {
                return &wrapper[open_end..body_end];
            }
        }
    }
    wrapper
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_outer_tag_extracts_inner_body() {
        assert_eq!(
            strip_outer_tag("<Message from=\"a\">hello</Message>", "Message"),
            "hello"
        );
        assert_eq!(
            strip_outer_tag(
                "<ChannelMessage channel=\"c\" from=\"a\">body</ChannelMessage>",
                "ChannelMessage"
            ),
            "body"
        );
    }

    #[test]
    fn extract_attr_reads_wrapper_attributes() {
        let wrapper = "<Message from=\"planner\">hi</Message>";
        assert_eq!(extract_attr(wrapper, "from").as_deref(), Some("planner"));
        assert!(extract_attr(wrapper, "missing").is_none());
    }

    #[test]
    fn inbound_event_serializes_with_kind_tag() {
        let ev = InboundEvent::UserMessage {
            body: "hi".into(),
            ts: "2026-04-24T12:00:00Z".into(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"kind\":\"user_message\""), "got: {s}");
        assert!(s.contains("\"body\":\"hi\""), "got: {s}");
    }
}
