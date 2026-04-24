//! Integration tests for the `:remote` automaton client.
//!
//! These tests spin up a real Unix-socket webservice listener using
//! `webservice::http::serve_unix` and then drive a `remote_automaton`
//! `Client` against it over the loopback socket. They cover:
//!
//! * RPC round-trip — a `SendMessage` the client issues arrives at
//!   the fake orchestrator via the server's `/v1/rpc` handler.
//! * SSE round-trip — an event broadcast on the server is received
//!   by the client as a `data:` frame.
//! * Proxy socket — writing an agent-IPC JSON line to the local
//!   proxy socket tunnels the command through to the fake remote.

use std::path::PathBuf;
use std::time::Duration;

use aciddog::agent_ipc::{
    AgentSocketEvent, Request as IpcRequest, RequestCmd, Response as IpcResponse,
};
use aciddog::ai::event::AIEvent;
use aciddog::config::{Automaton, AutomatonBackend, RemoteTransport};
use aciddog::remote_automaton::{http_client, run_remote_core};
use aciddog::webservice::{http::build_router, http::serve_unix, InboundEvent, WebserviceState};

use futures::StreamExt;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

/// Spawn a webservice listener on a UDS inside `tempdir` and return
/// its path plus handles for poking the server state.
struct FakeRemote {
    _tempdir: TempDir,
    socket_path: PathBuf,
    broadcast_tx: broadcast::Sender<InboundEvent>,
    seen_rx: mpsc::UnboundedReceiver<RequestCmd>,
    _shutdown_tx: oneshot::Sender<()>,
}

async fn spawn_fake_remote() -> FakeRemote {
    let tempdir = TempDir::new().unwrap();
    let socket_path = tempdir.path().join("remote.sock");

    let (events_tx, mut events_rx) = mpsc::channel::<AgentSocketEvent>(32);
    let (broadcast_tx, _) = broadcast::channel::<InboundEvent>(32);
    let (chat_block_tx, _) = broadcast::channel(32);
    let state = WebserviceState {
        session_id: Uuid::new_v4(),
        events_tx,
        broadcast_tx: broadcast_tx.clone(),
        chat_block_tx,
        is_supervisor: false,
        api_key: None,
    };
    let router = build_router(state, vec![]);

    // Auto-responder: capture every inbound request and reply ok.
    let (seen_tx, seen_rx) = mpsc::unbounded_channel::<RequestCmd>();
    tokio::spawn(async move {
        while let Some(ev) = events_rx.recv().await {
            let _ = seen_tx.send(ev.request.cmd.clone());
            let _ = ev.reply.send(IpcResponse::ok("queued"));
        }
    });

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let socket_for_server = socket_path.clone();
    tokio::spawn(async move {
        let _ = serve_unix(socket_for_server, router, shutdown_rx).await;
    });

    // Wait briefly for the socket to appear.
    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        socket_path.exists(),
        "fake remote socket did not appear at {}",
        socket_path.display()
    );

    FakeRemote {
        _tempdir: tempdir,
        socket_path,
        broadcast_tx,
        seen_rx,
        _shutdown_tx: shutdown_tx,
    }
}

#[tokio::test]
async fn rpc_round_trip_over_uds() {
    let mut remote = spawn_fake_remote().await;

    let client = http_client::Client::build(&RemoteTransport::UnixSocket {
        path: remote.socket_path.clone(),
    })
    .unwrap();

    let resp = client
        .send_rpc(&RequestCmd::SendMessage {
            to: "planner".into(),
            body: "hello".into(),
        })
        .await
        .expect("rpc succeeds");
    assert!(resp.ok, "resp: {resp:?}");

    let seen = tokio::time::timeout(Duration::from_secs(1), remote.seen_rx.recv())
        .await
        .expect("orchestrator sees the RPC within 1s")
        .expect("fake remote delivered a cmd");
    match seen {
        RequestCmd::SendMessage { to, body } => {
            assert_eq!(to, "planner");
            assert_eq!(body, "hello");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test]
async fn sse_stream_delivers_broadcast_events() {
    let remote = spawn_fake_remote().await;

    let client = http_client::Client::build(&RemoteTransport::UnixSocket {
        path: remote.socket_path.clone(),
    })
    .unwrap();

    let mut stream = client.sse_stream().await.expect("sse subscribe");

    // Give the SSE handshake a moment to register the subscriber
    // before we broadcast.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let event = InboundEvent::AgentMessage {
        from: "bridge".into(),
        body: "ping".into(),
        ts: "2026-04-24T00:00:00Z".into(),
    };
    remote.broadcast_tx.send(event).unwrap();

    // Read bytes until we see a full `data:` frame carrying our body.
    let mut accum = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;
        let Ok(Some(chunk)) = next else { continue };
        let chunk = chunk.expect("stream chunk");
        accum.push_str(&String::from_utf8_lossy(&chunk));
        if accum.contains("\"body\":\"ping\"") {
            break;
        }
    }
    assert!(
        accum.contains("\"body\":\"ping\""),
        "expected broadcast body in SSE stream, got: {accum}"
    );
    assert!(
        accum.contains("\"from\":\"bridge\""),
        "expected from=bridge in SSE stream, got: {accum}"
    );
}

#[tokio::test]
async fn remote_core_forwards_outbound_user_message() {
    let mut remote = spawn_fake_remote().await;

    // Build a minimal :remote automaton wired directly to the fake
    // remote's UDS. No deliver_to validation runs here — we're
    // calling `run_remote_core` directly, not going through the full
    // config load.
    let automaton = Automaton {
        id: "peer".into(),
        role: "bridge".into(),
        role_description: None,
        can_message: None,
        backend: AutomatonBackend::Remote {
            transport: RemoteTransport::UnixSocket {
                path: remote.socket_path.clone(),
            },
            remote_target: "bridge".into(),
            deliver_to: "planner".into(),
            proxy_socket_path: None,
        },
        supervisor: false,
    };

    let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
    let (_agent_tx, agent_rx) = mpsc::channel::<String>(8);
    let (_channel_tx, channel_rx) = mpsc::channel::<String>(8);
    let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
    let (ai_tx, mut ai_rx) = mpsc::channel::<(usize, AIEvent)>(64);
    let (events_tx, _events_rx) = mpsc::channel::<AgentSocketEvent>(32);

    let session = Uuid::new_v4();
    let handle = tokio::spawn(run_remote_core(
        0, automaton, session, user_rx, agent_rx, channel_rx, cancel_rx, ai_tx, events_tx,
    ));

    // Wait for the startup stdout line ("remote → …"), which
    // confirms the task is past its config/client setup and has
    // entered the main event loop — the moment it's safe to send.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut started = false;
    while tokio::time::Instant::now() < deadline && !started {
        if let Ok(Some((_, ev))) =
            tokio::time::timeout(Duration::from_millis(100), ai_rx.recv()).await
        {
            if matches!(ev, AIEvent::AutomatonStdout { ref line } if line.starts_with("remote → "))
            {
                started = true;
            }
        }
    }
    assert!(started, "remote automaton never logged its startup line");

    // Inject a user message locally → should be forwarded as
    // SendMessage { to: "bridge", body: "hi" } to the remote.
    user_tx.send("hi".into()).unwrap();

    let seen = tokio::time::timeout(Duration::from_secs(2), remote.seen_rx.recv())
        .await
        .expect("remote orchestrator sees the forwarded RPC")
        .expect("fake remote delivered a cmd");
    match seen {
        RequestCmd::SendMessage { to, body } => {
            assert_eq!(to, "bridge");
            assert_eq!(body, "hi");
        }
        other => panic!("unexpected: {other:?}"),
    }

    let _ = cancel_tx.send(()).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

#[tokio::test]
async fn proxy_socket_tunnels_request_cmd() {
    let mut remote = spawn_fake_remote().await;
    let proxy_dir = TempDir::new().unwrap();
    let proxy_path = proxy_dir.path().join("proxy.sock");

    let automaton = Automaton {
        id: "peer".into(),
        role: "bridge".into(),
        role_description: None,
        can_message: None,
        backend: AutomatonBackend::Remote {
            transport: RemoteTransport::UnixSocket {
                path: remote.socket_path.clone(),
            },
            remote_target: "bridge".into(),
            deliver_to: "planner".into(),
            proxy_socket_path: Some(proxy_path.clone()),
        },
        supervisor: false,
    };

    let (_user_tx, user_rx) = mpsc::unbounded_channel::<String>();
    let (_agent_tx, agent_rx) = mpsc::channel::<String>(8);
    let (_channel_tx, channel_rx) = mpsc::channel::<String>(8);
    let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
    let (ai_tx, _ai_rx) = mpsc::channel::<(usize, AIEvent)>(64);
    let (events_tx, _events_rx) = mpsc::channel::<AgentSocketEvent>(32);

    let session = Uuid::new_v4();
    let handle = tokio::spawn(run_remote_core(
        0, automaton, session, user_rx, agent_rx, channel_rx, cancel_rx, ai_tx, events_tx,
    ));

    // Wait for the proxy socket to appear.
    for _ in 0..100 {
        if proxy_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(proxy_path.exists(), "proxy socket didn't appear");

    // Speak the agent-IPC wire format against the proxy.
    let stream = UnixStream::connect(&proxy_path).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let request = IpcRequest {
        agent_id: Uuid::nil(),
        cmd: RequestCmd::SendMessage {
            to: "coder".into(),
            body: "hello from proxy".into(),
        },
    };
    let mut line = serde_json::to_string(&request).unwrap();
    line.push('\n');
    write_half.write_all(line.as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();

    // Read the single-line reply.
    let mut reader = BufReader::new(read_half);
    let mut reply = String::new();
    tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut reply))
        .await
        .expect("proxy reply within 2s")
        .expect("proxy reply read");
    let resp: IpcResponse = serde_json::from_str(reply.trim()).unwrap();
    assert!(resp.ok, "proxy response: {resp:?}");

    // The fake remote should have seen the SendMessage.
    let seen = tokio::time::timeout(Duration::from_secs(2), remote.seen_rx.recv())
        .await
        .expect("remote orchestrator sees the tunnelled RPC")
        .expect("fake remote delivered a cmd");
    match seen {
        RequestCmd::SendMessage { to, body } => {
            assert_eq!(to, "coder");
            assert_eq!(body, "hello from proxy");
        }
        other => panic!("unexpected: {other:?}"),
    }

    let _ = cancel_tx.send(()).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}
