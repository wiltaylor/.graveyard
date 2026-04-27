use std::path::PathBuf;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower::ServiceExt;

const ALICE: &str = "alice";
const ALICE_PW: &str = "alice-password";
const BOB: &str = "bob";
const BOB_PW: &str = "bob-password";

async fn router_with_pool(pool: SqlitePool) -> axum::Router {
    let state = AppState::new(pool, AuthConfig::default(), PathBuf::from("."));
    routes::build_router(state)
}

async fn seeded_pool_with_alice_and_bob() -> SqlitePool {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ALICE, ALICE_PW).await.unwrap();
    user::insert(&pool, BOB, BOB_PW).await.unwrap();
    pool
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

async fn login_and_get_cookie(app: &axum::Router, username: &str, password: &str) -> String {
    let resp = app
        .clone()
        .oneshot(login_request(
            json!({"username": username, "password": password}),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "login {username} should succeed"
    );
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

fn req(method: Method, uri: &str, cookie: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie)
}

#[tokio::test]
async fn list_users_returns_all_sorted_by_username() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;

    let resp = app
        .clone()
        .oneshot(
            req(Method::GET, "/api/users", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let arr = body.as_array().expect("array of users");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["username"], json!(ALICE));
    assert_eq!(arr[1]["username"], json!(BOB));
}

#[tokio::test]
async fn list_users_without_cookie_returns_401() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_user_succeeds_and_lets_them_log_in() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;

    let create = app
        .clone()
        .oneshot(
            req(Method::POST, "/api/users", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": "carol", "password": "carolpw"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let body = body_json(create).await;
    assert_eq!(body["username"], json!("carol"));

    // New user can immediately log in
    let login = app
        .oneshot(login_request(
            json!({"username": "carol", "password": "carolpw"}),
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_user_with_duplicate_username_returns_400() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;

    let resp = app
        .oneshot(
            req(Method::POST, "/api/users", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": BOB, "password": "anypw"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_user_with_empty_fields_returns_400() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;

    let resp = app
        .clone()
        .oneshot(
            req(Method::POST, "/api/users", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": "", "password": "pw"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .oneshot(
            req(Method::POST, "/api/users", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": "newish", "password": ""}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_user_removes_them_and_invalidates_their_sessions() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool.clone()).await;
    let alice_cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;
    let bob_cookie = login_and_get_cookie(&app, BOB, BOB_PW).await;

    // Look up bob's id
    let bob_row = user::find_by_username(&pool, BOB).await.unwrap().unwrap();

    let resp = app
        .clone()
        .oneshot(
            req(
                Method::DELETE,
                &format!("/api/users/{}", bob_row.id),
                &alice_cookie,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Bob's previously-issued cookie should now be invalid (cascade
    // delete removed his session row).
    let whoami = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/auth/whoami")
                .header("cookie", &bob_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(whoami.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_self_returns_409() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool.clone()).await;
    let alice_cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;
    let alice_row = user::find_by_username(&pool, ALICE).await.unwrap().unwrap();

    let resp = app
        .oneshot(
            req(
                Method::DELETE,
                &format!("/api/users/{}", alice_row.id),
                &alice_cookie,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn delete_last_remaining_user_returns_409() {
    // Seed only one user.
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ALICE, ALICE_PW).await.unwrap();
    let app = router_with_pool(pool.clone()).await;
    let alice_cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;
    let alice_row = user::find_by_username(&pool, ALICE).await.unwrap().unwrap();

    // Self-delete guard fires first; create a different user to test the
    // last-user guard cleanly. Insert a second row, log in as it, then
    // delete alice — bob is the only one left after, so try deleting bob
    // from his own session: hits self-delete first. To isolate the
    // last-user guard, we'd need a third user. Instead, hit the guard by
    // attempting to delete a *non-self* user when only one row exists,
    // which can't happen by construction. Verify the guard via a
    // different shape: log in as alice (sole user) and try to delete
    // herself — guards are checked in order and self-delete fires, but
    // after manually inserting a transient row we can hit last-user.
    // Simplest: insert bob, log in as alice, delete bob, then try to
    // delete a phantom UUID — that hits last-user before NotFound.
    user::insert(&pool, BOB, BOB_PW).await.unwrap();
    let bob_row = user::find_by_username(&pool, BOB).await.unwrap().unwrap();
    let resp = app
        .clone()
        .oneshot(
            req(
                Method::DELETE,
                &format!("/api/users/{}", bob_row.id),
                &alice_cookie,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Now alice is the sole user. Attempt to delete a fresh random UUID
    // (not alice's, so the self-guard doesn't fire). The last-user
    // guard runs before the not-found check.
    let phantom = uuid::Uuid::new_v4();
    assert_ne!(phantom.to_string(), alice_row.id);
    let resp = app
        .oneshot(
            req(
                Method::DELETE,
                &format!("/api/users/{phantom}"),
                &alice_cookie,
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn reset_password_lets_target_log_in_with_new_password() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool.clone()).await;
    let alice_cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;
    let bob_row = user::find_by_username(&pool, BOB).await.unwrap().unwrap();

    let resp = app
        .clone()
        .oneshot(
            req(
                Method::POST,
                &format!("/api/users/{}/password", bob_row.id),
                &alice_cookie,
            )
            .header("content-type", "application/json")
            .body(Body::from(json!({"password": "bob-new"}).to_string()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Old password fails, new one succeeds.
    let bad = app
        .clone()
        .oneshot(login_request(json!({"username": BOB, "password": BOB_PW})))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

    let good = app
        .oneshot(login_request(
            json!({"username": BOB, "password": "bob-new"}),
        ))
        .await
        .unwrap();
    assert_eq!(good.status(), StatusCode::OK);
}

#[tokio::test]
async fn change_my_password_requires_correct_current_password() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let alice_cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;

    let resp = app
        .oneshot(
            req(Method::POST, "/api/auth/password", &alice_cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"current_password": "wrong", "new_password": "anything"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn change_my_password_rotates_password() {
    let pool = seeded_pool_with_alice_and_bob().await;
    let app = router_with_pool(pool).await;
    let alice_cookie = login_and_get_cookie(&app, ALICE, ALICE_PW).await;

    let resp = app
        .clone()
        .oneshot(
            req(Method::POST, "/api/auth/password", &alice_cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"current_password": ALICE_PW, "new_password": "alice-new"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Old password no longer works.
    let bad = app
        .clone()
        .oneshot(login_request(
            json!({"username": ALICE, "password": ALICE_PW}),
        ))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

    // New password works.
    let good = app
        .oneshot(login_request(
            json!({"username": ALICE, "password": "alice-new"}),
        ))
        .await
        .unwrap();
    assert_eq!(good.status(), StatusCode::OK);
}
