//! Client-side counterpart to the webservice-backed automaton.
//!
//! A `:remote`-backed automaton dials a remote aciddog instance's
//! `/v1/rpc` + `/v1/events` endpoints and proxies one identity on that
//! remote as a local fleet member. Outbound local messages become
//! `send_message(to=remote_target)` RPCs; inbound SSE is delivered to a
//! configured `deliver_to` local id. An optional `proxy_socket_path`
//! mounts a Unix socket that speaks the same JSON-per-line agent-IPC
//! wire format the rest of the fleet uses, so local tools can point
//! `ACIDDOG_AGENT_SOCKET` at it and drive the full `RequestCmd`
//! surface against the remote.

pub mod http_client;
pub mod proxy;
pub mod sse;

use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::agent_ipc::{AgentSocketEvent, Request as IpcRequest, RequestCmd};
use crate::ai::event::AIEvent;
use crate::config::{Automaton, AutomatonBackend};
use crate::webservice::InboundEvent;

/// Entry point spawned by the TUI startup loop for each `:remote`
/// automaton. Owns the SSE subscriber, the outbound RPC pump, and
/// (optionally) the local proxy socket listener.
#[allow(clippy::too_many_arguments)]
pub async fn run_remote_core(
    idx: usize,
    automaton: Automaton,
    session_id: Uuid,
    mut user_rx: mpsc::UnboundedReceiver<String>,
    mut agent_rx: mpsc::Receiver<String>,
    mut channel_rx: mpsc::Receiver<String>,
    mut cancel_rx: mpsc::Receiver<()>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
    events_tx: mpsc::Sender<AgentSocketEvent>,
) {
    let (transport, remote_target, deliver_to, proxy_socket_path) = match automaton.backend.clone()
    {
        AutomatonBackend::Remote {
            transport,
            remote_target,
            deliver_to,
            proxy_socket_path,
        } => (transport, remote_target, deliver_to, proxy_socket_path),
        AutomatonBackend::Script { .. } | AutomatonBackend::Webservice { .. } => {
            warn!(automaton = %automaton.id, "run_remote_core called with non-Remote backend; ignoring");
            return;
        }
    };

    // Resolve the bearer token once at startup. A missing env var is
    // fatal — mirrors the webservice backend's refusal to bind.
    let client = match http_client::Client::build(&transport) {
        Ok(c) => c,
        Err(e) => {
            let _ = ai_tx
                .send((
                    idx,
                    AIEvent::AutomatonStderr {
                        line: format!("remote `{}` config error: {e}", automaton.id),
                    },
                ))
                .await;
            return;
        }
    };

    let transport_desc = http_client::describe_transport(&transport);
    info!(automaton = %automaton.id, transport = %transport_desc, remote_target = %remote_target, "remote automaton starting");
    let _ = ai_tx
        .send((
            idx,
            AIEvent::AutomatonStdout {
                line: format!(
                    "remote → {transport_desc} (remote_target={remote_target}, deliver_to={deliver_to})"
                ),
            },
        ))
        .await;
    // Don't emit `AutomatonStarted` — same rationale as in
    // `run_webservice_core`. The listener is long-lived; flipping to
    // `Working` here would stick because there's no matching
    // per-request exit event.

    // SSE subscriber runs on its own task. Inbound events are passed
    // back here via an mpsc so the main select can prioritize cancel.
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundEvent>(128);
    let (sse_cancel_tx, sse_cancel_rx) = mpsc::channel::<()>(1);
    let sse_handle = tokio::spawn(sse::run_sse_loop(
        idx,
        automaton.id.clone(),
        client.clone(),
        inbound_tx,
        ai_tx.clone(),
        sse_cancel_rx,
    ));

    // Optional proxy socket. Listener task owns its own cancel signal
    // so we can tear it down cleanly on shutdown.
    let (proxy_cancel_tx, proxy_cancel_rx) = mpsc::channel::<()>(1);
    let proxy_handle = if let Some(path) = proxy_socket_path.clone() {
        Some(tokio::spawn(proxy::run_proxy(
            idx,
            automaton.id.clone(),
            path,
            client.clone(),
            ai_tx.clone(),
            proxy_cancel_rx,
        )))
    } else {
        None
    };

    // Main event loop. Outbound-from-local messages get forwarded as
    // RPCs; inbound-from-remote SSE events get delivered to
    // `deliver_to` via the orchestrator's agent-IPC injection channel.
    loop {
        tokio::select! {
            biased;
            _ = cancel_rx.recv() => break,
            Some(m) = user_rx.recv() => {
                // User messages never wear a wrapper — forward body as-is.
                forward_outbound(
                    idx,
                    &automaton.id,
                    &client,
                    &remote_target,
                    m,
                    &ai_tx,
                )
                .await;
            }
            Some(m) = agent_rx.recv() => {
                // Agent messages arrive wrapped as
                // `<Message from="...">body</Message>`. Strip the
                // wrapper so what crosses the wire is the plain body.
                let body = crate::webservice::strip_outer_tag(&m, "Message").to_string();
                forward_outbound(
                    idx,
                    &automaton.id,
                    &client,
                    &remote_target,
                    body,
                    &ai_tx,
                )
                .await;
            }
            Some(m) = channel_rx.recv() => {
                // Channel messages: unwrap and forward the body.
                let body = crate::webservice::strip_outer_tag(&m, "ChannelMessage").to_string();
                forward_outbound(
                    idx,
                    &automaton.id,
                    &client,
                    &remote_target,
                    body,
                    &ai_tx,
                )
                .await;
            }
            Some(ev) = inbound_rx.recv() => {
                handle_inbound(
                    idx,
                    &automaton.id,
                    session_id,
                    &remote_target,
                    &deliver_to,
                    ev,
                    &events_tx,
                    &ai_tx,
                )
                .await;
            }
            else => break,
        }
    }

    // Shutdown: signal SSE + proxy, wait briefly, then exit.
    let _ = sse_cancel_tx.send(()).await;
    let _ = proxy_cancel_tx.send(()).await;
    sse_handle.abort();
    if let Some(h) = proxy_handle {
        h.abort();
    }
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

/// Forward a single local-side message as an RPC. Honours the
/// `::rpc <json>` escape hatch: if the body starts with that prefix
/// the remainder is parsed as a JSON `RequestCmd` and sent verbatim;
/// otherwise it's sent as `SendMessage { to: remote_target, body }`.
async fn forward_outbound(
    idx: usize,
    automaton_id: &str,
    client: &http_client::Client,
    remote_target: &str,
    body: String,
    ai_tx: &mpsc::Sender<(usize, AIEvent)>,
) {
    let cmd = match parse_rpc_escape(&body) {
        Some(Ok(cmd)) => cmd,
        Some(Err(e)) => {
            let _ = ai_tx
                .send((
                    idx,
                    AIEvent::AutomatonStderr {
                        line: format!("remote `{automaton_id}` ::rpc parse error: {e}"),
                    },
                ))
                .await;
            return;
        }
        None => RequestCmd::SendMessage {
            to: remote_target.to_string(),
            body,
        },
    };

    match client.send_rpc(&cmd).await {
        Ok(resp) => {
            let line = if resp.ok {
                format!(
                    "→ {} : {}",
                    summarize_cmd(&cmd),
                    resp.message.unwrap_or_default()
                )
            } else {
                format!(
                    "→ {} : ERROR {}",
                    summarize_cmd(&cmd),
                    resp.error.unwrap_or_default()
                )
            };
            let _ = ai_tx
                .send((idx, AIEvent::AutomatonStdout { line }))
                .await;
        }
        Err(e) => {
            let _ = ai_tx
                .send((
                    idx,
                    AIEvent::AutomatonStderr {
                        line: format!("remote `{automaton_id}` RPC failed: {e}"),
                    },
                ))
                .await;
        }
    }
}

/// Translate an inbound SSE event into a local delivery via the
/// orchestrator's `events_tx` injection channel.
#[allow(clippy::too_many_arguments)]
async fn handle_inbound(
    idx: usize,
    automaton_id: &str,
    session_id: Uuid,
    remote_target: &str,
    deliver_to: &str,
    ev: InboundEvent,
    events_tx: &mpsc::Sender<AgentSocketEvent>,
    ai_tx: &mpsc::Sender<(usize, AIEvent)>,
) {
    // Label the remote-originated body so the local recipient sees
    // who it came from.
    let (from_label, body) = match ev {
        InboundEvent::AgentMessage { from, body, .. } => (from, body),
        InboundEvent::UserMessage { body, .. } => {
            (format!("{remote_target}:user"), body)
        }
        InboundEvent::ChannelMessage {
            channel,
            from,
            body,
            ..
        } => {
            // Channels are not mirrored locally — log and drop.
            let _ = ai_tx
                .send((
                    idx,
                    AIEvent::AutomatonStderr {
                        line: format!(
                            "remote `{automaton_id}` dropped channel #{channel} from {from}: {body}"
                        ),
                    },
                ))
                .await;
            return;
        }
    };

    // Only forward messages that came from the configured remote
    // target (or the remote's user). Messages from other remote
    // identities are logged and dropped — attributing them to the
    // target would lie.
    let target_user_label = format!("{remote_target}:user");
    if from_label != remote_target && from_label != target_user_label {
        let _ = ai_tx
            .send((
                idx,
                AIEvent::AutomatonStderr {
                    line: format!(
                        "remote `{automaton_id}` dropped message from {from_label} (expected {remote_target}): {body}"
                    ),
                },
            ))
            .await;
        return;
    }

    // Inject into the orchestrator as a SendMessage from this
    // automaton, stamped with its own session id (mirrors how
    // `webservice::webhook::handle` delivers inbound hooks).
    let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
    let request = IpcRequest {
        // Stamp with the remote automaton's own session id so the send
        // handler's `app.agent_ids` lookup resolves to this automaton
        // and the delivered `<Message from="...">` is attributed
        // correctly. Mirrors the webhook delivery path.
        agent_id: session_id,
        cmd: RequestCmd::SendMessage {
            to: deliver_to.to_string(),
            body,
        },
    };
    // Surface a one-line log so operators see the delivery.
    let _ = ai_tx
        .send((
            idx,
            AIEvent::AutomatonStdout {
                line: format!("← {from_label} → {deliver_to}"),
            },
        ))
        .await;
    let _ = events_tx
        .send(AgentSocketEvent {
            request,
            reply: reply_tx,
        })
        .await;
}

/// Recognize the `::rpc <json>` escape hatch. Returns:
/// * `None` — body is a regular message.
/// * `Some(Ok(cmd))` — parsed successfully.
/// * `Some(Err(e))` — prefix matched but the JSON was malformed.
fn parse_rpc_escape(body: &str) -> Option<Result<RequestCmd, String>> {
    let trimmed = body.trim_start();
    let rest = trimmed.strip_prefix("::rpc")?;
    // Require whitespace (or EOL) after the prefix so "::rpcXYZ" isn't
    // misinterpreted.
    let rest = match rest.chars().next() {
        None => return Some(Err("missing JSON payload after ::rpc".to_string())),
        Some(c) if c.is_whitespace() => rest.trim_start(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        return Some(Err("missing JSON payload after ::rpc".to_string()));
    }
    match serde_json::from_str::<RequestCmd>(rest) {
        Ok(cmd) => Some(Ok(cmd)),
        Err(e) => Some(Err(e.to_string())),
    }
}

/// One-line summary of an outbound command for pane logging.
fn summarize_cmd(cmd: &RequestCmd) -> String {
    match cmd {
        RequestCmd::SendMessage { to, body } => {
            let preview: String = body.chars().take(60).collect();
            format!("send_message to={to} body={preview:?}")
        }
        other => {
            // Fallback: print the variant name via serde.
            serde_json::to_value(other)
                .ok()
                .and_then(|v| v.get("cmd").and_then(|c| c.as_str()).map(str::to_string))
                .unwrap_or_else(|| "rpc".to_string())
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_escape_plain_message_is_none() {
        assert!(parse_rpc_escape("hello world").is_none());
    }

    #[test]
    fn rpc_escape_requires_whitespace_after_prefix() {
        assert!(parse_rpc_escape("::rpcblah").is_none());
    }

    #[test]
    fn rpc_escape_parses_valid_json() {
        let out = parse_rpc_escape("::rpc {\"cmd\":\"test\"}").unwrap().unwrap();
        assert!(matches!(out, RequestCmd::Test));
    }

    #[test]
    fn rpc_escape_accepts_leading_whitespace() {
        let out = parse_rpc_escape("   ::rpc {\"cmd\":\"test\"}")
            .unwrap()
            .unwrap();
        assert!(matches!(out, RequestCmd::Test));
    }

    #[test]
    fn rpc_escape_reports_parse_errors() {
        let err = parse_rpc_escape("::rpc not-json").unwrap();
        assert!(err.is_err());
    }

    #[test]
    fn rpc_escape_empty_payload_errors() {
        let err = parse_rpc_escape("::rpc ").unwrap();
        assert!(err.is_err());
        let err2 = parse_rpc_escape("::rpc").unwrap();
        assert!(err2.is_err());
    }

    #[test]
    fn rpc_escape_forwards_structured_command() {
        let out = parse_rpc_escape("::rpc {\"cmd\":\"send_message\",\"to\":\"x\",\"body\":\"y\"}")
            .unwrap()
            .unwrap();
        match out {
            RequestCmd::SendMessage { to, body } => {
                assert_eq!(to, "x");
                assert_eq!(body, "y");
            }
            other => panic!("unexpected cmd: {other:?}"),
        }
    }
}

