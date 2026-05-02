use std::path::PathBuf;

use aciddog_manager::auth::{AuthConfig, COOKIE_NAME};
use aciddog_manager::model::instance::{self, CreateInstance, Transport};
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::routing::post;
use axum::Json;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tower::ServiceExt;

const TEST_USERNAME: &str = "tester";
const TEST_PASSWORD: &str = "tester-password";

async fn router_with_pool(pool: SqlitePool) -> axum::Router {
    user::insert(&pool, TEST_USERNAME, TEST_PASSWORD)
        .await
        .unwrap();
    let state = AppState::new(pool, AuthConfig::default(), PathBuf::from("."));
    routes::build_router(state)
}

async fn login_cookie(app: &axum::Router) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"username": TEST_USERNAME, "password": TEST_PASSWORD}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "login should succeed");
    let raw = resp
        .headers()
        .get("set-cookie")
        .expect("login response carries Set-Cookie")
        .to_str()
        .unwrap()
        .to_string();
    let head = raw.split(';').next().unwrap().trim().to_string();
    assert!(head.starts_with(&format!("{COOKIE_NAME}=")));
    head
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        panic!("expected JSON body, got: {text}");
    })
}

fn post_json_with_cookie(uri: &str, body: Value, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .header("cookie", cookie)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[derive(Debug, Clone)]
struct StubDaemon {
    default_branch: &'static str,
}

async fn rpc_stub(
    State(stub): State<StubDaemon>,
    Json(req): Json<Value>,
) -> Json<Value> {
    assert_eq!(req["cmd"], "repo_diff_json");
    assert_eq!(req["repo_id"], "repo-a");

    let branch = req["branch"].as_str().unwrap_or_default();
    if branch == stub.default_branch {
        Json(json!({
            "ok": true,
            "data": { "outcome": "no_forward_changes" }
        }))
    } else {
        Json(json!({
            "ok": true,
            "data": {
                "outcome": "error",
                "command": format!("git -C /repo diff --no-color {}...{}", stub.default_branch, branch),
                "stderr": format!("fatal: ambiguous argument '{}'", branch)
            }
        }))
    }
}

async fn spawn_stub_daemon() -> String {
    let app = axum::Router::new()
        .route("/v1/rpc", post(rpc_stub))
        .with_state(StubDaemon {
            default_branch: "main",
        });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn app_with_stub_instance() -> (axum::Router, uuid::Uuid) {
    let pool = db::connect_memory().await.unwrap();
    let base_url = spawn_stub_daemon().await;
    let instance = instance::insert(
        &pool,
        CreateInstance {
            name: "stub".into(),
            transport: Transport::Https,
            url: Some(base_url),
            socket_path: None,
            api_key: Some("test-token".into()),
            tls_ca_path: None,
        },
    )
    .await
    .unwrap();
    let app = router_with_pool(pool).await;
    (app, instance.id)
}

#[tokio::test]
async fn repo_diff_default_branch_returns_no_forward_changes() {
    let (app, instance_id) = app_with_stub_instance().await;
    let cookie = login_cookie(&app).await;

    let resp = app
        .oneshot(post_json_with_cookie(
            &format!("/api/instances/{instance_id}/repos/repo-a/diff"),
            json!({ "branch": "main" }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["outcome"], "no_forward_changes");
}

#[tokio::test]
async fn repo_diff_loud_failure_preserves_command_and_stderr() {
    let (app, instance_id) = app_with_stub_instance().await;
    let cookie = login_cookie(&app).await;

    let resp = app
        .oneshot(post_json_with_cookie(
            &format!("/api/instances/{instance_id}/repos/repo-a/diff"),
            json!({ "branch": "missing-branch" }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["outcome"], "error");
    assert!(body["command"].as_str().unwrap().contains("missing-branch"));
    assert!(!body["stderr"].as_str().unwrap().is_empty());
}
