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
    let (events_tx, events_rx) = mpsc::channel::<AgentSocketEvent>(16);
    let (broadcast_tx, _) = broadcast::channel(16);
    let state = WebserviceState {
        session_id: Uuid::new_v4(),
        events_tx,
        broadcast_tx,
        api_key: api_key.map(|s| std::sync::Arc::new(s.to_string())),
    };
    (state, events_rx)
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
