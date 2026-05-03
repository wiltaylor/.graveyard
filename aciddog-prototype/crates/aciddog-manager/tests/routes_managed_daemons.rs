//! ADR-0005 daemon-runner capability gate tests (foundation slice).
//!
//! Pins:
//! - Gate OFF: `/api/managed-daemons*` is literally absent → 404
//!   (NOT 403 — the capability isn't even registered).
//! - Gate ON, no session: `/api/managed-daemons` → 401
//!   (require_admin fails fast on missing AuthUser).
//! - whoami response echoes `is_daemon_runner_enabled` so the
//!   frontend can gate UI on bootstrap (login + whoami) without
//!   a separate capabilities probe.
//!
//! Concrete handlers under `/api/managed-daemons/*` are owned by
//! sibling b91e9691; this slice deliberately lands ZERO of them.

use std::path::PathBuf;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower::ServiceExt;

const ADMIN: &str = "admin";
const ADMIN_PW: &str = "admin-password";

async fn router_with_gate(is_daemon_runner_enabled: bool) -> axum::Router {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    build_router(pool, is_daemon_runner_enabled)
}

fn build_router(pool: SqlitePool, is_daemon_runner_enabled: bool) -> axum::Router {
    let state = AppState::new(
        pool,
        AuthConfig::default(),
        PathBuf::from("."),
        is_daemon_runner_enabled,
    );
    routes::build_router(state)
}

fn login_request(body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn cookie_from_set_cookie(raw: &str) -> String {
    raw.split(';').next().unwrap().trim().to_string()
}

async fn login_and_get_cookie(app: &axum::Router) -> String {
    let resp = app
        .clone()
        .oneshot(login_request(
            json!({"username": ADMIN, "password": ADMIN_PW}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "admin login should succeed");
    cookie_from_set_cookie(
        resp.headers()
            .get("set-cookie")
            .expect("set-cookie")
            .to_str()
            .unwrap(),
    )
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        panic!("expected JSON body, got: {text}");
    })
}

#[tokio::test]
async fn gate_off_returns_404_on_managed_daemons_route() {
    let app = router_with_gate(false).await;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/managed-daemons")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Capability literally absent — NOT 403, NOT 401. The /managed-daemons
    // subtree was not nested into /api, so axum's fallback (the static
    // assets handler) returns 404.
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn gate_on_with_no_session_returns_401() {
    let app = router_with_gate(true).await;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/managed-daemons")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // require_admin fails fast on missing AuthUser → 401 before any
    // admin check. There's no concrete handler here yet (sibling
    // b91e9691) but the layer fires regardless.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn whoami_reports_capability_off() {
    let app = router_with_gate(false).await;
    let cookie = login_and_get_cookie(&app).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/auth/whoami")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["username"], json!(ADMIN));
    assert_eq!(body["is_daemon_runner_enabled"], json!(false));
}

#[tokio::test]
async fn whoami_reports_capability_on() {
    let app = router_with_gate(true).await;
    let cookie = login_and_get_cookie(&app).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/auth/whoami")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["username"], json!(ADMIN));
    assert_eq!(body["is_daemon_runner_enabled"], json!(true));
}

#[tokio::test]
async fn login_response_echoes_capability_on() {
    let app = router_with_gate(true).await;
    let resp = app
        .oneshot(login_request(
            json!({"username": ADMIN, "password": ADMIN_PW}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["username"], json!(ADMIN));
    assert_eq!(body["is_daemon_runner_enabled"], json!(true));
}
