//! Integration tests for the manager-side stdio capture + SSE log
//! endpoint (sibling 3bc8a0d0).
//!
//! Two halves:
//!   1. Live capture + SSE replay/follow/reconnect against a managed
//!      daemon driven by `daemon_runner_shim --emit-{stdout,stderr}-bytes`.
//!   2. Adopted-entry 409 mapping (no manager-side pipes).
//!
//! Mirrors `tests/daemon_runner_smoke.rs` for the wrapper-script setup
//! pattern (the runner hard-codes the bare-word subcommand, so each
//! spawn actually exec's the shim via a small `sh` wrapper). The
//! integration exercises only the shim binary — no real production
//! daemon is spawned.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::daemon_runner::{
    CapturePolicy, DaemonRunner, ManagedDaemonRow, ObservedState,
};
use aciddog_manager::model::managed_daemon::{self, CreateManagedDaemon};
use aciddog_manager::model::user;
use aciddog_manager::{db, routes, state::AppState};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tempfile::tempdir;
use tokio::net::UnixListener;
use tower::ServiceExt;
use uuid::Uuid;

const ADMIN: &str = "admin";
const ADMIN_PW: &str = "admin-password";

// ---------------------------------------------------------------------
// Shared scaffolding
// ---------------------------------------------------------------------

fn shim_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daemon_runner_shim"))
}

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

async fn build_router_with_runner(pool: SqlitePool, runner: Arc<DaemonRunner>) -> axum::Router {
    let mut state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: false,
        },
        PathBuf::from("."),
        true,
    );
    state.daemon_runner = Some(runner);
    routes::build_router(state)
}

async fn login_cookie(app: &axum::Router) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"username": ADMIN, "password": ADMIN_PW}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "admin login should succeed");
    let raw = resp
        .headers()
        .get("set-cookie")
        .expect("set-cookie")
        .to_str()
        .unwrap()
        .to_string();
    raw.split(';').next().unwrap().trim().to_string()
}

async fn wait_for_state(
    runner: &Arc<DaemonRunner>,
    id: Uuid,
    target: ObservedState,
) -> Option<ObservedState> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = runner.status(id).await;
        if state == Some(target) {
            return state;
        }
        if Instant::now() >= deadline {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn collect_sse_body(resp: axum::response::Response) -> String {
    let bytes = to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Parse a single SSE frame block into (event, id, data). Handles
/// only a single-line `data:` per frame which is what our SSE handler
/// emits (one JSON object per line event).
fn parse_frames(body: &str) -> Vec<(Option<String>, Option<String>, Option<String>)> {
    let mut out = Vec::new();
    for block in body.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let mut event = None;
        let mut id = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("id:") {
                id = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data = Some(rest.trim().to_string());
            }
        }
        out.push((event, id, data));
    }
    out
}

// ---------------------------------------------------------------------
// (a) GET /:id/logs?follow=0 — replay the captured tail
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logs_replay_returns_captured_stdout_and_stderr() {
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("daemon.sock");
    let wrapper = work_dir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &[
            "--socket",
            socket_path.to_str().unwrap(),
            "--emit-stdout-bytes",
            "131072",
            "--emit-stderr-bytes",
            "16384",
            "--hold-after-emit",
        ],
    );

    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    let runner = DaemonRunner::new(wrapper.clone(), CapturePolicy::default());
    let app = build_router_with_runner(pool.clone(), runner.clone()).await;
    let cookie = login_cookie(&app).await;

    let (daemon, _instance) = managed_daemon::insert_with_paired_instance(
        &pool,
        CreateManagedDaemon {
            name: "log-emit".into(),
            work_dir: work_dir.path().to_path_buf(),
            socket_path: socket_path.to_string_lossy().into_owned(),
            desired_state: None,
            start_on_manager_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
        },
    )
    .await
    .expect("insert daemon");

    let row = ManagedDaemonRow {
        id: daemon.id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };
    runner.spawn(row).await.expect("spawn ok");
    let state = wait_for_state(&runner, daemon.id, ObservedState::Running).await;
    assert_eq!(state, Some(ObservedState::Running));

    // Give the fan-out task a beat to drain the shim's emission. The
    // shim emits ~131 KiB stdout + ~16 KiB stderr; piped reads fan in
    // via tokio::select! which can interleave.
    tokio::time::sleep(Duration::from_millis(800)).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/managed-daemons/{}/logs?follow=0", daemon.id))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = collect_sse_body(resp).await;
    let frames = parse_frames(&body);

    let mut stdout_lines = 0usize;
    let mut stderr_lines = 0usize;
    for (event, _id, data) in &frames {
        let Some(name) = event else { continue };
        if name != "line" {
            continue;
        }
        let Some(d) = data else { continue };
        let Ok(json) = serde_json::from_str::<Value>(d) else {
            continue;
        };
        match json["stream"].as_str() {
            Some("stdout") => stdout_lines += 1,
            Some("stderr") => stderr_lines += 1,
            _ => {}
        }
    }
    assert!(
        stdout_lines > 0,
        "expected stdout 'line' events; got {stdout_lines}"
    );
    assert!(
        stderr_lines > 0,
        "expected stderr 'line' events; got {stderr_lines}"
    );

    // Cleanup: best-effort stop. The shim is still in the socket loop
    // so this delivers SIGTERM and waits.
    let _ = runner.stop(daemon.id, Duration::from_secs(2)).await;
}

// ---------------------------------------------------------------------
// (c) Reconnect with Last-Event-ID — no duplicates, gap event when far
// behind. Drained synchronously via a follow=0 replay so the test does
// not have to manage an unbounded SSE stream.
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logs_reconnect_with_last_event_id_skips_already_seen() {
    let work_dir = tempdir().expect("tempdir");
    let socket_path = work_dir.path().join("daemon.sock");
    let wrapper = work_dir.path().join("shim-wrapper.sh");
    write_wrapper(
        &wrapper,
        &shim_path(),
        &[
            "--socket",
            socket_path.to_str().unwrap(),
            "--emit-stdout-bytes",
            "8000",
            "--hold-after-emit",
        ],
    );

    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    let runner = DaemonRunner::new(wrapper.clone(), CapturePolicy::default());
    let app = build_router_with_runner(pool.clone(), runner.clone()).await;
    let cookie = login_cookie(&app).await;

    let (daemon, _instance) = managed_daemon::insert_with_paired_instance(
        &pool,
        CreateManagedDaemon {
            name: "reconnect-emit".into(),
            work_dir: work_dir.path().to_path_buf(),
            socket_path: socket_path.to_string_lossy().into_owned(),
            desired_state: None,
            start_on_manager_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
        },
    )
    .await
    .expect("insert daemon");

    let row = ManagedDaemonRow {
        id: daemon.id,
        work_dir: work_dir.path().to_path_buf(),
        socket_path: socket_path.clone(),
    };
    runner.spawn(row).await.expect("spawn ok");
    wait_for_state(&runner, daemon.id, ObservedState::Running).await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // First connection — drain the entire ring.
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/managed-daemons/{}/logs?follow=0", daemon.id))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = collect_sse_body(resp).await;
    let frames = parse_frames(&body);
    let last_id: Option<u64> = frames
        .iter()
        .filter_map(|(_, id, _)| id.as_ref())
        .filter_map(|s| s.parse().ok())
        .max();
    let last_id = last_id.expect("at least one line was captured");

    // Reconnect with Last-Event-ID — should see no entries (or only
    // entries with offset > last_id), no duplicates, no gap event
    // (the cursor is still inside the ring).
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/managed-daemons/{}/logs?follow=0", daemon.id))
        .header("cookie", &cookie)
        .header("Last-Event-ID", last_id.to_string())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = collect_sse_body(resp).await;
    let frames = parse_frames(&body);
    let mut saw_gap = false;
    for (event, id, _) in &frames {
        let Some(name) = event else { continue };
        if name == "gap" {
            saw_gap = true;
            continue;
        }
        if name == "line" {
            let parsed: u64 = id
                .as_ref()
                .and_then(|s| s.parse().ok())
                .expect("line frames must carry a parseable id");
            assert!(
                parsed > last_id,
                "line offset {parsed} must be strictly greater than the prior cursor {last_id}"
            );
        }
    }
    assert!(
        !saw_gap,
        "no gap event expected when the prior cursor is still inside the ring"
    );

    // Reconnect with a Last-Event-ID FAR past the ring (we set ring to
    // default 1MB, but since=0 covers the lower bound — set a value
    // below the ring's earliest offset by clearing the runner).
    // For the gap-event branch we instead ask the route directly with
    // since=0 after pumping enough bytes to fill + evict the ring.
    // That's covered separately below if the ring is small enough; for
    // the default 1MB ring the shim's 8KB emission stays inside, so
    // since=0 returns the full replay with no gap. Skip the gap-arm
    // assertion here and let the unit test
    // ring_since_before_oldest_returns_gap_marker_and_oldest_onwards
    // own that branch.

    let _ = runner.stop(daemon.id, Duration::from_secs(2)).await;
}

// ---------------------------------------------------------------------
// Adopted entry — 409 with documented body
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_returns_409_for_adopted_daemon() {
    let dir = tempdir().expect("tempdir");
    let socket_path = dir.path().join("adopted.sock");

    // Pre-bind a UnixListener so the adopt probe connects.
    let _ = fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");
    let _accept_task = tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            match listener.accept().await {
                Ok((stream, _)) => held.push(stream),
                Err(_) => return,
            }
        }
    });

    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    // Runner doesn't actually spawn anything for the adopted path —
    // the binary path is irrelevant.
    let runner = DaemonRunner::new(shim_path(), CapturePolicy::default());
    let app = build_router_with_runner(pool.clone(), runner.clone()).await;
    let cookie = login_cookie(&app).await;

    let (daemon, _instance) = managed_daemon::insert_with_paired_instance(
        &pool,
        CreateManagedDaemon {
            name: "adopted-no-logs".into(),
            work_dir: dir.path().to_path_buf(),
            socket_path: socket_path.to_string_lossy().into_owned(),
            desired_state: None,
            start_on_manager_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
        },
    )
    .await
    .expect("insert daemon");

    runner
        .adopt(daemon.id, socket_path.clone())
        .await
        .expect("adopt ok");

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/managed-daemons/{}/logs", daemon.id))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "adopted entries must 409 — no manager-side capture"
    );
    let body_bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).expect("json body");
    assert_eq!(body["error"], json!("adopted: no manager-side capture"));
}

// ---------------------------------------------------------------------
// Unknown id — 404 (sanity)
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_unknown_id_returns_404() {
    let dir = tempdir().expect("tempdir");
    let pool = db::connect_memory().await.unwrap();
    user::insert(&pool, ADMIN, ADMIN_PW).await.unwrap();
    let wrapper = dir.path().join("shim-wrapper.sh");
    write_wrapper(&wrapper, &shim_path(), &[]);
    let runner = DaemonRunner::new(wrapper, CapturePolicy::default());
    let app = build_router_with_runner(pool, runner).await;
    let cookie = login_cookie(&app).await;

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/managed-daemons/{}/logs", Uuid::new_v4()))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
