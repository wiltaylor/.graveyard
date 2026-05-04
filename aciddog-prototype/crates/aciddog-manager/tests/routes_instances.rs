use std::path::PathBuf;
use std::sync::Arc;

use aciddog_manager::auth::{AuthConfig, COOKIE_NAME};
use aciddog_manager::daemon_runner::{CapturePolicy, DaemonRunner};
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower::ServiceExt;

const TEST_USERNAME: &str = "tester";
const TEST_PASSWORD: &str = "tester-password";

async fn test_router_with_pool() -> (axum::Router, SqlitePool) {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, TEST_USERNAME, TEST_PASSWORD)
        .await
        .unwrap();
    let aciddog_binary = Arc::new(PathBuf::from("/bin/true"));
    let runner = DaemonRunner::new((*aciddog_binary).clone(), CapturePolicy::default());
    let state = AppState::new(
        pool.clone(),
        AuthConfig::default(),
        PathBuf::from("."),
        aciddog_binary,
        runner,
        CapturePolicy::default(),
    );
    let router = routes::build_router(state);
    (router, pool)
}

async fn test_router() -> axum::Router {
    let (router, _pool) = test_router_with_pool().await;
    router
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
    // `Set-Cookie: adm_session=<value>; Path=/; ...` — keep just `name=value`
    // for the request-side `Cookie:` header.
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

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn delete_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn create_list_delete_round_trip() {
    let app = test_router().await;
    let cookie = login_cookie(&app).await;

    let empty = app
        .clone()
        .oneshot(get_with_cookie("/api/instances", &cookie))
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    let list: Value = body_json(empty).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    let created = app
        .clone()
        .oneshot(post_json_with_cookie(
            "/api/instances",
            json!({
                "name": "demo",
                "transport": "unix",
                "socket_path": "/tmp/aciddog-demo.sock"
            }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let summary: Value = body_json(created).await;
    let id = summary["id"].as_str().expect("id is a string").to_string();
    assert_eq!(summary["has_api_key"], json!(false));
    assert!(
        summary.get("api_key").is_none(),
        "api_key must not be serialized"
    );

    let listed = app
        .clone()
        .oneshot(get_with_cookie("/api/instances", &cookie))
        .await
        .unwrap();
    let rows: Value = body_json(listed).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);

    let deleted = app
        .clone()
        .oneshot(delete_with_cookie(&format!("/api/instances/{id}"), &cookie))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let after = app
        .clone()
        .oneshot(get_with_cookie("/api/instances", &cookie))
        .await
        .unwrap();
    let rows: Value = body_json(after).await;
    assert_eq!(rows.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_rejects_https_without_url() {
    let app = test_router().await;
    let cookie = login_cookie(&app).await;
    let resp = app
        .oneshot(post_json_with_cookie(
            "/api/instances",
            json!({
                "name": "bad",
                "transport": "https"
            }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_rejects_duplicate_name() {
    let app = test_router().await;
    let cookie = login_cookie(&app).await;
    let payload = || {
        post_json_with_cookie(
            "/api/instances",
            json!({
                "name": "dup",
                "transport": "unix",
                "socket_path": "/tmp/a.sock"
            }),
            &cookie,
        )
    };
    let first = app.clone().oneshot(payload()).await.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let second = app.clone().oneshot(payload()).await.unwrap();
    assert_eq!(second.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_unknown_returns_404() {
    let app = test_router().await;
    let cookie = login_cookie(&app).await;
    let resp = app
        .oneshot(delete_with_cookie(
            &format!("/api/instances/{}", uuid::Uuid::new_v4()),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn agents_on_unknown_id_returns_404() {
    let app = test_router().await;
    let cookie = login_cookie(&app).await;
    let resp = app
        .oneshot(get_with_cookie(
            &format!("/api/instances/{}/agents", uuid::Uuid::new_v4()),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn static_fallback_still_mounted() {
    // Paths that don't match /api/* fall through to the embedded static
    // handler. A non-existent file should yield 404 from that handler,
    // confirming the fallback is wired and is NOT gated by auth.
    let app = test_router().await;
    let resp = app
        .oneshot(get("/definitely-not-a-file.html"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
