//! Always-on daemon-runner route-handler tests for
//! `/api/managed-daemons`.
//!
//! The first half of this file covers route registration, auth, and the
//! one-release auth response compatibility field. The second half is
//! list / get_one / start / stop / reload / delete + create.
//!
//! ## daemon_runner injection in tests
//!
//! `DaemonRunner` is a CONCRETE type (no trait), so we cannot substitute
//! a mock. Two integration paths are used:
//!
//! - `adopt(id, socket_path)` against a pre-bound `tokio::net::UnixListener`
//!   gives us an entry whose `status(id)` returns `ObservedState::Running`
//!   without spawning any subprocess. Adopted entries return
//!   `AdoptedNotControlled` for `stop`/`reload` — useful for the
//!   delete-409 / reload-409 mapping branches.
//! - For tests that need `stop`/`reload` to actually succeed, we shell
//!   out to the `daemon_runner_shim` test binary via a small `sh`
//!   wrapper script (same pattern as `tests/daemon_runner_smoke.rs`):
//!   the wrapper exec's the shim with a stable `--socket <path>` so
//!   the runner's hard-coded `arg("daemon")` is harmless.
//!
//! ## Scope reduction vs the plan
//!
//! The plan listed `delete_500_when_stop_fails` (mock returns
//! `StillAlive`). `StillAlive` only fires when SIGKILL itself fails to
//! reap the child — there is no honest test fixture that simulates
//! that without a kernel-level injection. We instead ship
//! `delete_409_for_adopted_daemon` (a real branch we DO map) plus a
//! TODO breadcrumb. See `delete_409_for_adopted_daemon` below.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::daemon_runner::{CapturePolicy, DaemonRunner};
use aciddog_manager::model::managed_daemon::{self, CreateManagedDaemon};
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tempfile::{tempdir, TempDir};
use tokio::net::UnixListener;
use tower::ServiceExt;
use uuid::Uuid;

const ADMIN: &str = "admin";
const ADMIN_PW: &str = "admin-password";
const NON_ADMIN: &str = "alice";
const NON_ADMIN_PW: &str = "alice-password";

const ADMIN_PROTECTED_ROUTES: &[(Method, &str)] = &[
    (Method::GET, "/api/managed-daemons"),
    (Method::POST, "/api/managed-daemons"),
    (
        Method::GET,
        "/api/managed-daemons/00000000-0000-0000-0000-000000000001",
    ),
    (
        Method::DELETE,
        "/api/managed-daemons/00000000-0000-0000-0000-000000000001",
    ),
    (
        Method::POST,
        "/api/managed-daemons/00000000-0000-0000-0000-000000000001/start",
    ),
    (
        Method::POST,
        "/api/managed-daemons/00000000-0000-0000-0000-000000000001/stop",
    ),
    (
        Method::POST,
        "/api/managed-daemons/00000000-0000-0000-0000-000000000001/reload",
    ),
    (
        Method::GET,
        "/api/managed-daemons/00000000-0000-0000-0000-000000000001/logs",
    ),
];
// MIRROR of routes/managed_daemons.rs require_admin layer (lines 560-576).
// If you add a route there, add it here.
// require_admin coverage regression per security_consultant on ADR-0008 (TODO 85e34b7d).

// ---------------------------------------------------------------------
// Shared test harness (mirrors `tests/routes_instances.rs` shape).
//
// NOT extracted to `tests/common/mod.rs` per the brief — that refactor
// is a follow-up for the next slice that touches both files.
// ---------------------------------------------------------------------

async fn router_for_tests() -> axum::Router {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    let runner = DaemonRunner::new(PathBuf::from("/bin/true"), CapturePolicy::default());
    build_router(pool, runner)
}

fn build_router(pool: SqlitePool, daemon_runner: Arc<DaemonRunner>) -> axum::Router {
    let aciddog_binary = Arc::new(PathBuf::from("/bin/true"));
    let state = AppState::new(
        pool,
        AuthConfig::default(),
        PathBuf::from("."),
        aciddog_binary,
        daemon_runner,
        CapturePolicy::default(),
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
    login_and_get_cookie_for(app, ADMIN, ADMIN_PW).await
}

async fn login_and_get_cookie_for(app: &axum::Router, username: &str, password: &str) -> String {
    let resp = app
        .clone()
        .oneshot(login_request(
            json!({ "username": username, "password": password }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "login should succeed");
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

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn post_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
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

fn delete_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn request_without_cookie(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn request_with_cookie(method: Method, uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

// ---------------------------------------------------------------------
// daemon_runner test scaffolding
// ---------------------------------------------------------------------

/// Path to the `daemon_runner_shim` test binary, populated by Cargo at
/// build time (mirror of `tests/daemon_runner_smoke.rs`).
fn shim_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daemon_runner_shim"))
}

/// Write a `sh` wrapper script that exec's the shim with the given
/// trailing args. The runner spawns the binary with `arg("daemon")`,
/// which the shim ignores; the wrapper makes that harmless.
fn write_wrapper(wrapper_path: &Path, shim: &Path, extra_args: &[&str]) {
    let mut body = String::from("#!/bin/sh\n");
    body.push_str("# trailing args from caller (e.g. \"daemon\") are ignored\n");
    body.push_str(&format!("exec {:?}", shim));
    for arg in extra_args {
        body.push(' ');
        body.push_str(&format!("{:?}", arg));
    }
    body.push('\n');
    fs::write(wrapper_path, body).expect("write wrapper script");
    let mut perms = fs::metadata(wrapper_path)
        .expect("stat wrapper")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(wrapper_path, perms).expect("chmod wrapper");
}

/// Hold a `UnixListener` bound at `socket_path` and accept-loop in the
/// background so that `DaemonRunner::adopt`'s probe connects. The
/// listener is dropped (and the socket file removed) when this struct
/// goes out of scope.
struct AdoptedSocket {
    _accept_task: tokio::task::JoinHandle<()>,
    socket_path: PathBuf,
}

impl AdoptedSocket {
    fn new(socket_path: PathBuf) -> Self {
        // Best-effort cleanup of a stale socket.
        let _ = fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("bind unix listener");
        let task = tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => held.push(stream),
                    Err(_) => return,
                }
            }
        });
        Self {
            _accept_task: task,
            socket_path,
        }
    }
}

impl Drop for AdoptedSocket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}

/// Build a router + pool + a real `DaemonRunner`. The runner points at
/// the shim wrapper script at `<workdir>/shim-wrapper.sh`. `workdir`
/// MUST outlive every spawned child the test triggers.
async fn router_with_runner(workdir: &Path) -> (axum::Router, SqlitePool, Arc<DaemonRunner>) {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    let socket_path = workdir.join("shim.sock");
    let wrapper = workdir.join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &["--socket", socket_path.to_str().unwrap()],
    );
    let runner = DaemonRunner::new(wrapper.clone(), CapturePolicy::default());
    let router = build_router(pool.clone(), runner.clone());
    (router, pool, runner)
}

/// Insert a fresh managed-daemon row + paired instance row. Returns
/// `(daemon_id, instance_id, socket_path)`. The work_dir is a tempdir
/// owned by the returned `TempDir` — keep it alive for the test.
async fn insert_daemon_row(pool: &SqlitePool, name: &str) -> (Uuid, Uuid, PathBuf, TempDir) {
    let work_dir = tempdir().unwrap();
    let socket_path = work_dir.path().join(format!("{name}.sock"));
    let (daemon, instance) = managed_daemon::insert_with_paired_instance(
        pool,
        CreateManagedDaemon {
            name: name.into(),
            work_dir: work_dir.path().to_path_buf(),
            socket_path: socket_path.to_string_lossy().into_owned(),
            desired_state: None,
            start_on_manager_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
        },
    )
    .await
    .expect("insert managed daemon");
    (daemon.id, instance.id, socket_path, work_dir)
}

/// Poll `runner.status(id)` every 100 ms (up to 5 s) until target.
async fn wait_for_state(
    runner: &Arc<DaemonRunner>,
    id: Uuid,
    target: aciddog_manager::daemon_runner::ObservedState,
) -> Option<aciddog_manager::daemon_runner::ObservedState> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let state = runner.status(id).await;
        if state == Some(target) {
            return state;
        }
        if std::time::Instant::now() >= deadline {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ---------------------------------------------------------------------
// Always-on route smoke tests and auth compatibility assertions.
// ---------------------------------------------------------------------

#[tokio::test]
async fn routes_present_in_router_smoke() {
    let app = router_for_tests().await;
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
    assert_ne!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn no_session_returns_401() {
    let app = router_for_tests().await;
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
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn every_managed_daemon_route_requires_admin() {
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    user::insert(&pool, NON_ADMIN, NON_ADMIN_PW).await.unwrap();
    let runner = DaemonRunner::new(PathBuf::from("/bin/true"), CapturePolicy::default());
    let app = build_router(pool, runner);
    let non_admin_cookie = login_and_get_cookie_for(&app, NON_ADMIN, NON_ADMIN_PW).await;

    for (method, uri) in ADMIN_PROTECTED_ROUTES {
        let no_session = app
            .clone()
            .oneshot(request_without_cookie(method.clone(), uri))
            .await
            .unwrap();
        assert_eq!(
            no_session.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} without a session must return 401"
        );

        let non_admin = app
            .clone()
            .oneshot(request_with_cookie(method.clone(), uri, &non_admin_cookie))
            .await
            .unwrap();
        assert_eq!(
            non_admin.status(),
            StatusCode::FORBIDDEN,
            "{method} {uri} with a non-admin session must return 403"
        );
    }
}

#[tokio::test]
async fn whoami_reports_capability() {
    let app = router_for_tests().await;
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
    // Compatibility-window assertion per ADR-0008. Field is hardcoded true server-side.
    assert_eq!(body["is_daemon_runner_enabled"], json!(true));
}

#[tokio::test]
async fn login_response_echoes_capability() {
    let app = router_for_tests().await;
    let resp = app
        .oneshot(login_request(
            json!({"username": ADMIN, "password": ADMIN_PW}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["username"], json!(ADMIN));
    // Compatibility-window assertion per ADR-0008. Field is hardcoded true server-side.
    assert_eq!(body["is_daemon_runner_enabled"], json!(true));
}

// ---------------------------------------------------------------------
// b91e9691 route-handler tests (this slice).
// ---------------------------------------------------------------------

#[tokio::test]
async fn list_returns_empty_when_no_rows() {
    let workdir = tempdir().unwrap();
    let (app, _pool, _runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let resp = app
        .oneshot(get_with_cookie("/api/managed-daemons", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn list_includes_observed_state() {
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    // Seed a daemon row + paired instance.
    let (daemon_id, instance_id, socket_path, _work_dir) = insert_daemon_row(&pool, "alpha").await;

    // Adopt the daemon at a real bound socket so status(id) returns
    // ObservedState::Running without spawning a subprocess.
    let _adopted = AdoptedSocket::new(socket_path.clone());
    runner
        .adopt(daemon_id, socket_path.clone())
        .await
        .expect("adopt ok");

    let resp = app
        .oneshot(get_with_cookie("/api/managed-daemons", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let arr = body.as_array().expect("array body");
    assert_eq!(arr.len(), 1);
    let row = &arr[0];
    assert_eq!(row["id"], json!(daemon_id.to_string()));
    assert_eq!(row["name"], json!("alpha"));
    assert_eq!(row["observed_state"], json!("running"));
    assert_eq!(row["desired_state"], json!("stopped"));
    assert_eq!(
        row["paired_instance_id"],
        json!(instance_id.to_string()),
        "paired_instance_id must surface from instances.managed_by_id lookup"
    );
}

#[tokio::test]
async fn get_one_returns_view() {
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let (daemon_id, _instance_id, socket_path, _work_dir) = insert_daemon_row(&pool, "beta").await;
    let _adopted = AdoptedSocket::new(socket_path.clone());
    runner.adopt(daemon_id, socket_path).await.expect("adopt");

    let resp = app
        .oneshot(get_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["id"], json!(daemon_id.to_string()));
    assert_eq!(body["name"], json!("beta"));
    assert_eq!(body["observed_state"], json!("running"));
}

#[tokio::test]
async fn get_one_unknown_id_returns_404() {
    let workdir = tempdir().unwrap();
    let (app, _pool, _runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let resp = app
        .oneshot(get_with_cookie(
            &format!("/api/managed-daemons/{}", Uuid::new_v4()),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_blocks_until_stopped() {
    // Spawn a real shim child via the runner so stop() actually
    // succeeds. Wait for Running, then DELETE; assert 204 + DB rows
    // gone (both managed_daemons and the paired instances row).
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let (daemon_id, instance_id, socket_path, work_dir) =
        insert_daemon_row(&pool, "killable").await;
    // The shim's wrapper has a hard-coded socket path baked in by
    // router_with_runner. Override the socket_path the runner will
    // use to spawn this daemon by feeding it directly into the
    // ManagedDaemonRow we hand to spawn().
    let row = aciddog_manager::daemon_runner::ManagedDaemonRow {
        id: daemon_id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };
    // Rewrite the wrapper to point at THIS daemon's socket so the
    // shim binds where the runner is probing.
    let wrapper = workdir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &["--socket", socket_path.to_str().unwrap()],
    );
    runner.spawn(row).await.expect("spawn ok");
    let state = wait_for_state(
        &runner,
        daemon_id,
        aciddog_manager::daemon_runner::ObservedState::Running,
    )
    .await;
    assert_eq!(
        state,
        Some(aciddog_manager::daemon_runner::ObservedState::Running),
        "shim must reach Running before DELETE; got {state:?}"
    );

    let resp = app
        .oneshot(delete_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "delete should 204 when stop succeeds"
    );

    // Both rows must be gone (delete_with_paired_instance is atomic).
    let daemon_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM managed_daemons WHERE id = ?1")
            .bind(daemon_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(daemon_count, 0, "managed_daemons row must be deleted");
    let inst_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM instances WHERE id = ?1")
        .bind(instance_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(inst_count, 0, "paired instances row must be deleted");
}

/// Maps `DaemonRunnerError::AdoptedNotControlled` to a 409 with a
/// documented body. The DB rows STAY in place (operator must stop the
/// adopted daemon manually before the manager can drop it).
///
/// TODO(plan §12 (d)): the originally-listed `delete_500_when_stop_fails`
/// case requires a child that survives SIGKILL — there is no honest
/// fixture for that without kernel-level injection. We exercise the
/// AdoptedNotControlled branch here as a proxy for the failure-path
/// mapping; the StillAlive branch is covered by code review of
/// `routes/managed_daemons.rs::delete` (single match arm).
#[tokio::test]
async fn delete_409_for_adopted_daemon() {
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let (daemon_id, instance_id, socket_path, _work_dir) =
        insert_daemon_row(&pool, "adopted").await;
    let _adopted = AdoptedSocket::new(socket_path.clone());
    runner.adopt(daemon_id, socket_path).await.expect("adopt");

    let resp = app
        .oneshot(delete_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "delete on adopted entry should 409"
    );
    let body = body_json(resp).await;
    assert_eq!(body["ok"], json!(false));
    assert!(
        body["error"].as_str().unwrap_or("").contains("adopted"),
        "error body should mention adopted state, got: {body}"
    );

    // DB rows must STILL be present.
    let daemon_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM managed_daemons WHERE id = ?1")
            .bind(daemon_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(daemon_count, 1, "managed_daemons row must NOT be deleted");
    let inst_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM instances WHERE id = ?1")
        .bind(instance_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(inst_count, 1, "paired instances row must NOT be deleted");
}

#[tokio::test]
async fn delete_unknown_id_returns_404() {
    let workdir = tempdir().unwrap();
    let (app, _pool, _runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let resp = app
        .oneshot(delete_with_cookie(
            &format!("/api/managed-daemons/{}", Uuid::new_v4()),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn start_is_idempotent() {
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let (daemon_id, _instance_id, socket_path, _work_dir) =
        insert_daemon_row(&pool, "idem-start").await;
    let _adopted = AdoptedSocket::new(socket_path.clone());
    runner.adopt(daemon_id, socket_path).await.expect("adopt");

    // First call.
    let resp1 = app
        .clone()
        .oneshot(post_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}/start"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::ACCEPTED);
    let body1 = body_json(resp1).await;
    assert_eq!(body1["desired_state"], json!("running"));
    assert_eq!(body1["observed_state"], json!("running"));

    // Second call — still 202, same shape.
    let resp2 = app
        .clone()
        .oneshot(post_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}/start"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::ACCEPTED,
        "repeated start must be idempotent (still 202)"
    );
    let body2 = body_json(resp2).await;
    assert_eq!(body1["id"], body2["id"]);
    assert_eq!(body1["desired_state"], body2["desired_state"]);
    assert_eq!(body1["observed_state"], body2["observed_state"]);
}

#[tokio::test]
async fn stop_is_idempotent() {
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let (daemon_id, _instance_id, socket_path, _work_dir) =
        insert_daemon_row(&pool, "idem-stop").await;
    let _adopted = AdoptedSocket::new(socket_path.clone());
    runner.adopt(daemon_id, socket_path).await.expect("adopt");

    let resp1 = app
        .clone()
        .oneshot(post_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}/stop"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::ACCEPTED);
    let body1 = body_json(resp1).await;
    assert_eq!(body1["desired_state"], json!("stopped"));

    let resp2 = app
        .clone()
        .oneshot(post_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}/stop"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::ACCEPTED);
    let body2 = body_json(resp2).await;
    assert_eq!(body1["desired_state"], body2["desired_state"]);
}

#[tokio::test]
async fn reload_409_when_not_running() {
    let workdir = tempdir().unwrap();
    let (app, pool, _runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    // Seed a row but DO NOT adopt — runner.status returns None →
    // handler unwraps to ObservedState::Stopped → 409.
    let (daemon_id, _instance_id, _socket_path, _work_dir) =
        insert_daemon_row(&pool, "not-running").await;

    let resp = app
        .oneshot(post_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}/reload"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body = body_json(resp).await;
    let err = body["error"].as_str().unwrap_or("");
    assert!(
        err.contains("cannot reload daemon in observed_state"),
        "expected the documented 409 phrasing, got: {err}"
    );
    assert!(
        err.contains("expected running"),
        "expected the documented 409 phrasing, got: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reload_signals_when_running() {
    // Spawn a real shim child so reload (SIGHUP) actually delivers.
    // The shim has no SIGHUP handler installed, so the signal is a
    // no-op on the child's lifecycle — the route's job is to deliver
    // the signal and return 202.
    let workdir = tempdir().unwrap();
    let (app, pool, runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    let (daemon_id, _instance_id, socket_path, work_dir) =
        insert_daemon_row(&pool, "reload-target").await;
    let wrapper = workdir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &["--socket", socket_path.to_str().unwrap()],
    );
    let row = aciddog_manager::daemon_runner::ManagedDaemonRow {
        id: daemon_id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };
    runner.spawn(row).await.expect("spawn ok");
    let state = wait_for_state(
        &runner,
        daemon_id,
        aciddog_manager::daemon_runner::ObservedState::Running,
    )
    .await;
    assert_eq!(
        state,
        Some(aciddog_manager::daemon_runner::ObservedState::Running),
        "shim must reach Running before reload; got {state:?}"
    );

    let resp = app
        .oneshot(post_with_cookie(
            &format!("/api/managed-daemons/{daemon_id}/reload"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::ACCEPTED,
        "reload should 202 when observed=Running"
    );
    let body = body_json(resp).await;
    assert_eq!(body["id"], json!(daemon_id.to_string()));
    assert_eq!(body["observed_state"], json!("running"));
    // reload MUST NOT touch desired_state — it's still the seeded default.
    assert_eq!(
        body["desired_state"],
        json!("stopped"),
        "reload must not flip desired_state"
    );

    // Cleanup: stop the spawned child so the test does not leak a
    // process. Use a generous timeout because the SIGTERM path is
    // already covered by daemon_runner_smoke; here we just want to
    // not orphan the child.
    runner
        .stop(daemon_id, Duration::from_secs(5))
        .await
        .expect("cleanup stop ok");
}

/// The create handler is a documented 501 stub until sibling 580b77a1
/// (`scaffolding::create_managed_daemon`) lands. This test pins the
/// stub shape so a future implementer's grep finds the contract.
#[tokio::test]
async fn create_returns_501_until_scaffolding_lands() {
    let workdir = tempdir().unwrap();
    let (app, _pool, _runner) = router_with_runner(workdir.path()).await;
    let cookie = login_and_get_cookie(&app).await;

    // Body content does not matter — the handler ignores it. Send a
    // realistic-shaped payload so when 580b77a1 lands, this test only
    // needs its expected-status line flipped.
    let resp = app
        .oneshot(post_json_with_cookie(
            "/api/managed-daemons",
            json!({
                "name": "would-be-daemon",
                "work_dir": "/tmp/whatever",
                "socket_path": "/tmp/whatever.sock",
                "idle_timeout_secs": 300
            }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_IMPLEMENTED,
        "create handler must remain 501 until sibling 580b77a1 lands"
    );
    let body = body_json(resp).await;
    assert_eq!(body["ok"], json!(false));
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("580b77a1"),
        "error body must reference sibling 580b77a1 so a future grep finds the contract; got: {body}"
    );
}
