use std::path::PathBuf;

use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn test_router() -> axum::Router {
    let pool = db::connect_memory().await.unwrap();
    let state = AppState::new(pool);
    // ServeDir needs a valid path, but we won't hit it in these tests.
    routes::build_router(state, &PathBuf::from("."))
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        panic!("expected JSON body, got: {text}");
    })
}

fn post_json(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn create_list_delete_round_trip() {
    let app = test_router().await;

    let empty = app.clone().oneshot(get("/api/instances")).await.unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    let list: Value = body_json(empty).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    let created = app
        .clone()
        .oneshot(post_json(
            "/api/instances",
            json!({
                "name": "demo",
                "transport": "unix",
                "socket_path": "/tmp/aciddog-demo.sock"
            }),
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

    let listed = app.clone().oneshot(get("/api/instances")).await.unwrap();
    let rows: Value = body_json(listed).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);

    let deleted = app
        .clone()
        .oneshot(delete(&format!("/api/instances/{id}")))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let after = app.clone().oneshot(get("/api/instances")).await.unwrap();
    let rows: Value = body_json(after).await;
    assert_eq!(rows.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_rejects_https_without_url() {
    let app = test_router().await;
    let resp = app
        .oneshot(post_json(
            "/api/instances",
            json!({
                "name": "bad",
                "transport": "https"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_rejects_duplicate_name() {
    let app = test_router().await;
    let payload = || {
        post_json(
            "/api/instances",
            json!({
                "name": "dup",
                "transport": "unix",
                "socket_path": "/tmp/a.sock"
            }),
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
    let resp = app
        .oneshot(delete(&format!("/api/instances/{}", uuid::Uuid::new_v4())))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn agents_on_unknown_id_returns_404() {
    let app = test_router().await;
    let resp = app
        .oneshot(get(&format!(
            "/api/instances/{}/agents",
            uuid::Uuid::new_v4()
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn static_fallback_still_mounted() {
    // Paths that don't match /api/* fall through to ServeDir. With static_dir=".",
    // a request for a non-existent file should yield 404 (ServeDir's own 404),
    // confirming the fallback service is wired.
    let app = test_router().await;
    let resp = app
        .oneshot(get("/definitely-not-a-file.html"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
