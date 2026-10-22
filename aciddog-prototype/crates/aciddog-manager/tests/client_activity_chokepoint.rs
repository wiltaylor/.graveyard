//! ADR-0004 §Activity-tracking — chokepoint coverage for
//! `Client::touch_if_managed`.
//!
//! Three cases (all run with `tokio::test(start_paused = true)` for
//! virtual-time control):
//!
//! - `managed_touches_advance_monotonically`: pre-bound managed row +
//!   tracker, three `client.health()` calls 1s apart, tracker records
//!   strictly-monotonic touches and exactly one Uuid keyed.
//! - `non_managed_no_mutation`: same fixture but `managed_by_id =
//!   None`; five `health()` calls leave the tracker empty.
//! - `legacy_from_row_produces_no_activity`: managed row constructed
//!   via legacy `from_row(&row)` (no tracker bound); five `health()`
//!   calls do not touch a sentinel tracker AND
//!   `Client::activity_is_some()` returns false.
//!
//! Uses `tokio::test(start_paused = true)` (gated behind tokio's
//! `test-util` feature, enabled in dev-deps) so `tokio::time::advance`
//! drives both `Instant::now()` calls inside the chokepoint and any
//! sleeps the client itself performs.

use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use uuid::Uuid;

use aciddog_manager::client::Client;
use aciddog_manager::daemon_runner::ActivityTracker;
use aciddog_manager::model::instance::InstanceRow;

/// Build an `InstanceRow` whose `transport=unix` points at
/// `socket_path`, optionally tagged with `managed_by_id`.
fn unix_row(socket_path: &std::path::Path, managed_by_id: Option<String>) -> InstanceRow {
    InstanceRow {
        id: Uuid::new_v4().to_string(),
        name: "stub".into(),
        transport: "unix".into(),
        url: None,
        socket_path: Some(socket_path.to_string_lossy().into_owned()),
        api_key: None,
        tls_ca_path: None,
        managed_by_id,
        created_at: "1970-01-01T00:00:00Z".into(),
    }
}

/// Spawn a tokio task that listens on `path`, accepts each
/// connection, drains the request, and replies with a minimal
/// `HTTP/1.1 200 OK` (Content-Length: 0). Stops when the listener is
/// dropped.
async fn spawn_health_listener(path: std::path::PathBuf) {
    let listener = UnixListener::bind(&path).expect("bind unix listener");
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    tokio::spawn(async move {
                        // Drain request bytes until the header
                        // terminator. We don't care about the path or
                        // method — the Client side is what's under
                        // test.
                        use tokio::io::AsyncReadExt;
                        let mut buf = [0u8; 4096];
                        let mut total = Vec::new();
                        loop {
                            match stream.read(&mut buf).await {
                                Ok(0) => return,
                                Ok(n) => {
                                    total.extend_from_slice(&buf[..n]);
                                    if total.windows(4).any(|w| w == b"\r\n\r\n") {
                                        break;
                                    }
                                }
                                Err(_) => return,
                            }
                        }
                        let resp =
                            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes()).await;
                        let _ = stream.shutdown().await;
                    });
                }
                Err(_) => return,
            }
        }
    });
    // Yield once so the spawned task gets a chance to enter the
    // accept() loop before the test issues its first connect.
    tokio::task::yield_now().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn managed_touches_advance_monotonically() {
    let dir = tempdir().expect("tempdir");
    let socket = dir.path().join("managed.sock");
    spawn_health_listener(socket.clone()).await;

    let tracker = ActivityTracker::new();
    let daemon_id = Uuid::new_v4();
    let row = unix_row(&socket, Some(daemon_id.to_string()));
    let client = Client::from_row_with_activity(&row, Some(Arc::clone(&tracker)))
        .expect("from_row_with_activity");
    assert!(
        client.activity_is_some(),
        "managed row + tracker => activity slot must be populated"
    );

    let mut samples = Vec::new();
    for _ in 0..3 {
        let (status, _body) = client.health().await.expect("health ok");
        assert_eq!(status, 200);
        let stamp = tracker
            .last_touched(daemon_id)
            .expect("tracker must record managed touches");
        samples.push(stamp);
        tokio::time::advance(Duration::from_secs(1)).await;
    }

    assert_eq!(samples.len(), 3);
    for w in samples.windows(2) {
        assert!(
            w[1] > w[0],
            "touches must advance under tokio::time control: {:?} -> {:?}",
            w[0],
            w[1]
        );
    }
    assert_eq!(
        tracker.len(),
        1,
        "exactly one Uuid keyed in the tracker (no nil ghost entries)"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn non_managed_no_mutation() {
    let dir = tempdir().expect("tempdir");
    let socket = dir.path().join("non-managed.sock");
    spawn_health_listener(socket.clone()).await;

    let tracker = ActivityTracker::new();
    let probe_id = Uuid::new_v4();
    // managed_by_id = None => activity slot stays None even though a
    // tracker is supplied.
    let row = unix_row(&socket, None);
    let client = Client::from_row_with_activity(&row, Some(Arc::clone(&tracker)))
        .expect("from_row_with_activity");
    assert!(
        !client.activity_is_some(),
        "managed_by_id=None must produce activity=None"
    );

    for _ in 0..5 {
        let (status, _body) = client.health().await.expect("health ok");
        assert_eq!(status, 200);
        tokio::time::advance(Duration::from_millis(250)).await;
    }

    assert!(
        tracker.last_touched(probe_id).is_none(),
        "non-managed dispatch must never insert into the tracker"
    );
    assert!(
        tracker.is_empty(),
        "tracker map must remain empty for non-managed clients"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn legacy_from_row_produces_no_activity() {
    let dir = tempdir().expect("tempdir");
    let socket = dir.path().join("legacy.sock");
    spawn_health_listener(socket.clone()).await;

    // Sentinel: an unrelated tracker that the client has no link to.
    // Must remain empty for the duration of the test.
    let sentinel_tracker = ActivityTracker::new();
    let daemon_id = Uuid::new_v4();
    let row = unix_row(&socket, Some(daemon_id.to_string()));
    // Legacy ctor: no tracker handed in. Even though the row IS
    // managed, the activity slot is None because the chokepoint
    // refuses to invent a tracker out of thin air.
    let client = Client::from_row(&row).expect("from_row");
    assert!(
        !client.activity_is_some(),
        "from_row(row) must not bind any tracker — the back-compat \
         path is intentionally inert"
    );

    for _ in 0..5 {
        let (status, _body) = client.health().await.expect("health ok");
        assert_eq!(status, 200);
        tokio::time::advance(Duration::from_millis(250)).await;
    }

    assert!(
        sentinel_tracker.last_touched(daemon_id).is_none(),
        "legacy from_row must never touch a tracker outside its scope"
    );
    assert!(
        sentinel_tracker.is_empty(),
        "sentinel tracker map must remain empty"
    );
}
