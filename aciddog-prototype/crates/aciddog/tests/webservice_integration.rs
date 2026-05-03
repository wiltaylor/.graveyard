//! Integration tests for the webservice router.
//!
//! These tests drive axum's `Router` directly via `tower::oneshot`,
//! skipping the actual HTTPS / Unix socket transport. The transport is
//! a thin layer on top of the router — these tests cover routing,
//! auth, webhook envelope shaping, and RPC forwarding.

use std::collections::BTreeMap;

use aciddog::agent_ipc::{AgentSocketEvent, RequestCmd, Response as IpcResponse};
use aciddog::config::{HmacAlgo, Webhook, WebhookAuth};
use aciddog::webservice::{http::build_router, WebserviceState};
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;
use uuid::Uuid;

fn fake_state(api_key: Option<&str>) -> (WebserviceState, mpsc::Receiver<AgentSocketEvent>) {
    let (state_pair, _, _) = fake_state_with(api_key, false);
    state_pair
}

/// Like `fake_state` but lets the test supply the `is_supervisor` flag
/// and returns the `chat_block_tx` and `agent_status_tx` sender clones
/// so tests can inject wire events / status transitions into the scoped
/// SSE stream.
fn fake_state_with(
    api_key: Option<&str>,
    is_supervisor: bool,
) -> (
    (WebserviceState, mpsc::Receiver<AgentSocketEvent>),
    broadcast::Sender<aciddog::wire_chat::ChatBlockEvent>,
    broadcast::Sender<aciddog::wire_chat::AgentStatusEvent>,
) {
    let (events_tx, events_rx) = mpsc::channel::<AgentSocketEvent>(16);
    let (broadcast_tx, _) = broadcast::channel(16);
    let (chat_block_tx, _) = broadcast::channel(64);
    let (channel_message_tx, _) = broadcast::channel(64);
    let (workflow_event_tx, _) = broadcast::channel(64);
    let (cron_event_tx, _) = broadcast::channel(64);
    let (agent_status_tx, _) = broadcast::channel(64);
    let state = WebserviceState {
        session_id: Uuid::new_v4(),
        events_tx,
        broadcast_tx,
        chat_block_tx: chat_block_tx.clone(),
        channel_message_tx,
        workflow_event_tx,
        cron_event_tx,
        agent_status_tx: agent_status_tx.clone(),
        is_supervisor,
        api_key: api_key.map(|s| std::sync::Arc::new(s.to_string())),
    };
    ((state, events_rx), chat_block_tx, agent_status_tx)
}

/// Spawn a tiny "orchestrator" that auto-replies `Response::ok("queued")`
/// to every incoming event. Returns a receiver that yields every
/// request seen, for assertions.
fn spawn_auto_responder(
    mut events_rx: mpsc::Receiver<AgentSocketEvent>,
) -> mpsc::UnboundedReceiver<RequestCmd> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(ev) = events_rx.recv().await {
            let _ = tx.send(ev.request.cmd.clone());
            let _ = ev.reply.send(IpcResponse::ok("queued"));
        }
    });
    rx
}

async fn body_bytes(resp: axum::response::Response) -> Vec<u8> {
    body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

#[tokio::test]
async fn rpc_forwards_and_replies() {
    let (state, events_rx) = fake_state(None);
    let mut seen = spawn_auto_responder(events_rx);
    let router = build_router(state, vec![]);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/rpc")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "cmd": "send_message", "to": "planner", "body": "hi" }).to_string(),
        ))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: IpcResponse = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    assert!(body.ok, "resp: {body:?}");

    let seen_cmd = seen.recv().await.expect("orchestrator saw the command");
    match seen_cmd {
        RequestCmd::SendMessage { to, body } => {
            assert_eq!(to, "planner");
            assert_eq!(body, "hi");
        }
        other => panic!("unexpected cmd: {other:?}"),
    }
}

/// Smoke: every new RPC variant added for the manager's Tasks / Inbox
/// views deserializes server-side and forwards to the orchestrator.
/// Guards against serde renames — any `cmd` string drift will show up
/// here as a 400 instead of being caught far downstream in a route
/// handler.
#[tokio::test]
async fn rpc_forwards_new_task_and_inbox_variants() {
    let (state, events_rx) = fake_state(None);
    let mut seen = spawn_auto_responder(events_rx);
    let router = build_router(state, vec![]);

    let probes = [
        serde_json::json!({ "cmd": "task_list_json", "mine": false, "unassigned": false }),
        serde_json::json!({ "cmd": "inbox_list" }),
        serde_json::json!({ "cmd": "inbox_delete", "index": 0 }),
        serde_json::json!({ "cmd": "team_info" }),
        serde_json::json!({ "cmd": "channel_list" }),
        serde_json::json!({ "cmd": "channel_history", "channel": "general" }),
        serde_json::json!({ "cmd": "channel_send_as_user", "channel": "general", "body": "hi" }),
        serde_json::json!({ "cmd": "memory_banks" }),
        serde_json::json!({ "cmd": "memory_search_json", "bank": "notes" }),
        serde_json::json!({
            "cmd": "memory_store_json", "bank": "notes", "body": "hi"
        }),
        serde_json::json!({ "cmd": "memory_graph", "bank": "notes" }),
    ];
    for probe in probes {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc")
            .header("content-type", "application/json")
            .body(Body::from(probe.to_string()))
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "probe {probe} rejected");
        let _seen = seen.recv().await.expect("orchestrator saw the cmd");
    }
}

#[tokio::test]
async fn rpc_rejects_missing_bearer_when_api_key_required() {
    let (state, _events_rx) = fake_state(Some("secret"));
    let router = build_router(state, vec![]);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/rpc")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({ "cmd": "test" }).to_string()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn chat_events_rejects_non_supervisor() {
    let ((state, _events_rx), _chat_tx, _status_tx) = fake_state_with(None, false);
    let router = build_router(state, vec![]);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/chat/events?agent=planner")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn chat_events_filters_on_agent_id() {
    use aciddog::wire_chat::{ChatBlockEvent, WireChatBlock};
    use tokio::time::{timeout, Duration};

    let ((state, _events_rx), chat_tx, _status_tx) = fake_state_with(None, true);
    let router = build_router(state, vec![]);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/chat/events?agent=planner")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Stream the body, publishing two events along the way — one for
    // a mismatched agent (should be filtered out) and one for the
    // target. We stop reading after the expected frame or 500ms.
    let mut body = resp.into_body().into_data_stream();
    use futures::StreamExt;

    // Producer task: push in a few events after a beat.
    let chat_tx2 = chat_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = chat_tx2.send(ChatBlockEvent {
            agent: "other".into(),
            block: WireChatBlock::UserText {
                text: "nope".into(),
            },
            ts: "2026-04-24T00:00:00Z".into(),
            seq: 1,
        });
        let _ = chat_tx2.send(ChatBlockEvent {
            agent: "planner".into(),
            block: WireChatBlock::UserText { text: "hi".into() },
            ts: "2026-04-24T00:00:01Z".into(),
            seq: 2,
        });
    });

    let mut collected = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_millis(200), body.next()).await {
            Ok(Some(Ok(chunk))) => {
                collected.extend_from_slice(&chunk);
                // Frames are `data: ...\n\n` separated. Stop once we've
                // seen the "hi" body.
                if collected
                    .windows(b"\"text\":\"hi\"".len())
                    .any(|w| w == b"\"text\":\"hi\"")
                {
                    break;
                }
            }
            _ => continue,
        }
    }
    let out = String::from_utf8_lossy(&collected);
    assert!(
        out.contains("\"agent\":\"planner\""),
        "expected planner event in stream, got:\n{out}"
    );
    assert!(
        !out.contains("\"text\":\"nope\""),
        "mismatched-agent event leaked into stream:\n{out}"
    );
}

#[tokio::test]
async fn chat_events_forwards_named_status_event_for_target_agent() {
    use aciddog::wire_chat::AgentStatusEvent;
    use tokio::time::{timeout, Duration};

    let ((state, _events_rx), _chat_tx, status_tx) = fake_state_with(None, true);
    let router = build_router(state, vec![]);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/chat/events?agent=planner")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let mut body = resp.into_body().into_data_stream();
    use futures::StreamExt;

    // Producer: push a foreign-agent status (must be filtered out) and
    // a target status (must arrive as a NAMED `status` SSE frame).
    let status_tx2 = status_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = status_tx2.send(AgentStatusEvent {
            agent: "other".into(),
            status: "working".into(),
        });
        let _ = status_tx2.send(AgentStatusEvent {
            agent: "planner".into(),
            status: "working".into(),
        });
    });

    let mut collected = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_millis(200), body.next()).await {
            Ok(Some(Ok(chunk))) => {
                collected.extend_from_slice(&chunk);
                // Stop once we've seen both the named-event line AND
                // the data payload for planner's working status.
                let s = String::from_utf8_lossy(&collected);
                if s.contains("event: status") && s.contains("\"status\":\"working\"") {
                    break;
                }
            }
            _ => continue,
        }
    }
    let out = String::from_utf8_lossy(&collected);
    assert!(
        out.contains("event: status"),
        "expected named `status` SSE event header in stream, got:\n{out}"
    );
    assert!(
        out.contains("\"status\":\"working\""),
        "expected planner's working payload in stream, got:\n{out}"
    );
    assert!(
        out.contains("\"agent\":\"planner\""),
        "expected planner agent id in payload, got:\n{out}"
    );
    // The foreign-agent status frame must NOT have leaked through —
    // the chat_events handler filters by agent server-side.
    let frames_with_other = out.matches("\"agent\":\"other\"").count();
    assert_eq!(
        frames_with_other, 0,
        "mismatched-agent status leaked into stream:\n{out}"
    );
}

#[tokio::test]
async fn rpc_accepts_valid_bearer() {
    let (state, events_rx) = fake_state(Some("secret"));
    let _seen = spawn_auto_responder(events_rx);
    let router = build_router(state, vec![]);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/rpc")
        .header("content-type", "application/json")
        .header("authorization", "Bearer secret")
        .body(Body::from(serde_json::json!({ "cmd": "test" }).to_string()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn webhook_bearer_auth_delivers_envelope() {
    std::env::set_var("ACID_TEST_BEARER_SECRET", "s3cr3t");
    let (state, events_rx) = fake_state(None);
    let mut seen = spawn_auto_responder(events_rx);

    let hook = Webhook {
        id: "hook_bearer".into(),
        mount: "api".into(),
        path: "/hooks/bearer".into(),
        deliver_to: "planner".into(),
        auth: WebhookAuth::Bearer {
            secret_env: "ACID_TEST_BEARER_SECRET".into(),
        },
        max_body_bytes: None,
    };
    let router = build_router(state, vec![hook]);

    // Wrong token → 401.
    let req = Request::builder()
        .method("POST")
        .uri("/hooks/bearer")
        .header("authorization", "Bearer wrong")
        .body(Body::from("{}"))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Right token → 200, envelope delivered as SendMessage.
    let req = Request::builder()
        .method("POST")
        .uri("/hooks/bearer?x=1")
        .header("authorization", "Bearer s3cr3t")
        .header("content-type", "application/json")
        .header("x-source", "tests")
        .body(Body::from("{\"ok\":true}"))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cmd = seen.recv().await.expect("webhook routed a SendMessage");
    match cmd {
        RequestCmd::SendMessage { to, body } => {
            assert_eq!(to, "planner");
            let env: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(env["kind"], "webhook");
            assert_eq!(env["hook"], "hook_bearer");
            assert_eq!(env["query"]["x"], "1");
            assert_eq!(env["body_json"]["ok"], true);
            // Auth header must NOT leak into the envelope.
            assert!(
                env["headers"].get("authorization").is_none(),
                "bearer header leaked: {env:?}"
            );
            assert_eq!(env["headers"]["x-source"], "tests");
        }
        other => panic!("unexpected cmd: {other:?}"),
    }
}

#[tokio::test]
async fn webhook_hmac_sha256_accepts_valid_signature() {
    std::env::set_var("ACID_TEST_HMAC_SECRET", "topsecret");
    let (state, events_rx) = fake_state(None);
    let mut seen = spawn_auto_responder(events_rx);
    let hook = Webhook {
        id: "gh".into(),
        mount: "api".into(),
        path: "/hooks/gh".into(),
        deliver_to: "coder".into(),
        auth: WebhookAuth::HmacHeader {
            header: "X-Hub-Signature-256".into(),
            secret_env: "ACID_TEST_HMAC_SECRET".into(),
            algo: HmacAlgo::Sha256,
        },
        max_body_bytes: None,
    };
    let router = build_router(state, vec![hook]);

    let body = br#"{"ref":"refs/heads/main"}"#;
    let mut mac: Hmac<Sha256> = Hmac::<Sha256>::new_from_slice(b"topsecret").unwrap();
    mac.update(body);
    let sig = hex::encode(mac.finalize().into_bytes());

    // Tampered body → 401.
    let req = Request::builder()
        .method("POST")
        .uri("/hooks/gh")
        .header("x-hub-signature-256", format!("sha256={sig}"))
        .body(Body::from(&body[..body.len() - 1]))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Correct signature → 200.
    let req = Request::builder()
        .method("POST")
        .uri("/hooks/gh")
        .header("x-hub-signature-256", format!("sha256={sig}"))
        .header("x-github-event", "push")
        .body(Body::from(&body[..]))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cmd = seen.recv().await.expect("hmac webhook delivered");
    match cmd {
        RequestCmd::SendMessage { to, body } => {
            assert_eq!(to, "coder");
            let env: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(env["headers"]["x-github-event"], "push");
            assert!(
                env["headers"].get("x-hub-signature-256").is_none(),
                "hmac header leaked"
            );
        }
        other => panic!("unexpected cmd: {other:?}"),
    }
}

#[tokio::test]
async fn webhook_json_field_auth_matches_nested_path() {
    std::env::set_var("ACID_TEST_JSON_SECRET", "token123");
    let (state, events_rx) = fake_state(None);
    let mut seen = spawn_auto_responder(events_rx);
    let hook = Webhook {
        id: "zap".into(),
        mount: "api".into(),
        path: "/hooks/zap".into(),
        deliver_to: "planner".into(),
        auth: WebhookAuth::JsonField {
            field: "auth.token".into(),
            secret_env: "ACID_TEST_JSON_SECRET".into(),
        },
        max_body_bytes: None,
    };
    let router = build_router(state, vec![hook]);

    // Wrong token in body → 401.
    let req = Request::builder()
        .method("POST")
        .uri("/hooks/zap")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "auth": { "token": "wrong" },
                "event": "x"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Matching token → delivery.
    let req = Request::builder()
        .method("POST")
        .uri("/hooks/zap")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "auth": { "token": "token123" },
                "event": "x"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cmd = seen.recv().await.unwrap();
    let RequestCmd::SendMessage { body, .. } = cmd else {
        panic!("expected SendMessage");
    };
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["body_json"]["event"], "x");
}

#[tokio::test]
async fn webhook_respects_max_body_bytes() {
    let (state, _events_rx) = fake_state(None);
    let hook = Webhook {
        id: "small".into(),
        mount: "api".into(),
        path: "/hooks/small".into(),
        deliver_to: "planner".into(),
        auth: WebhookAuth::None,
        max_body_bytes: Some(16),
    };
    let router = build_router(state, vec![hook]);

    let req = Request::builder()
        .method("POST")
        .uri("/hooks/small")
        .body(Body::from("x".repeat(32)))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn health_returns_ok() {
    let (state, _events_rx) = fake_state(None);
    let router = build_router(state, vec![]);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/health")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    assert_eq!(v["ok"], true);
}

/// Suppress unused-import warnings when the tests compile without
/// hitting every symbol (e.g. if someone comments out a test).
#[allow(dead_code)]
fn _used(_: BTreeMap<String, String>) {}
