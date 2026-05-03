use std::path::PathBuf;

use aciddog_manager::auth::{AuthConfig, COOKIE_NAME};
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

const USERNAME: &str = "tester";
const PASSWORD: &str = "tester-password";

async fn router() -> axum::Router {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, USERNAME, PASSWORD).await.unwrap();
    let state = AppState::new(pool, AuthConfig::default(), PathBuf::from("."), false);
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

fn cookie_header_from_set_cookie(raw: &str) -> String {
    raw.split(';').next().unwrap().trim().to_string()
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        panic!("expected JSON body, got: {text}");
    })
}

#[tokio::test]
async fn login_sets_cookie_and_whoami_returns_user() {
    let app = router().await;

    let resp = app
        .clone()
        .oneshot(login_request(
            json!({"username": USERNAME, "password": PASSWORD}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let raw = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(raw.contains("HttpOnly"));
    assert!(raw.contains("SameSite=Strict"));
    assert!(raw.contains(&format!("{COOKIE_NAME}=")));
    let cookie = cookie_header_from_set_cookie(&raw);

    // body returns the username
    let body: Value = body_json(resp).await;
    assert_eq!(body["username"], json!(USERNAME));

    // whoami with the cookie returns 200 + username
    let whoami_req = Request::builder()
        .method(Method::GET)
        .uri("/api/auth/whoami")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let whoami_resp = app.clone().oneshot(whoami_req).await.unwrap();
    assert_eq!(whoami_resp.status(), StatusCode::OK);
    let body: Value = body_json(whoami_resp).await;
    assert_eq!(body["username"], json!(USERNAME));
}

#[tokio::test]
async fn login_with_bad_password_returns_401() {
    let app = router().await;
    let resp = app
        .oneshot(login_request(
            json!({"username": USERNAME, "password": "wrong"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_with_unknown_user_returns_401() {
    let app = router().await;
    let resp = app
        .oneshot(login_request(
            json!({"username": "ghost", "password": "anything"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_without_cookie_returns_401() {
    let app = router().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/instances")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn whoami_without_cookie_returns_401() {
    let app = router().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/auth/whoami")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_invalidates_session() {
    let app = router().await;

    let login = app
        .clone()
        .oneshot(login_request(
            json!({"username": USERNAME, "password": PASSWORD}),
        ))
        .await
        .unwrap();
    let cookie =
        cookie_header_from_set_cookie(login.headers().get("set-cookie").unwrap().to_str().unwrap());

    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/auth/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert!(logout
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("Max-Age=0"))
        .unwrap_or(false));

    // Re-using the now-deleted session token should 401.
    let after = app
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
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}
