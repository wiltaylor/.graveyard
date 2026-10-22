use std::path::PathBuf;
use std::time::Duration;

use aciddog_manager::db;
use aciddog_manager::error::AppError;
use aciddog_manager::model::instance::{self, CreateInstance, Transport};
use aciddog_manager::model::managed_daemon::{
    self, CreateManagedDaemon, DesiredState, ManagedDaemonRow,
};
use sqlx::Row;
use tempfile::tempdir;
use uuid::Uuid;

fn make_input(name: &str, work_dir: PathBuf, socket: &str) -> CreateManagedDaemon {
    CreateManagedDaemon {
        name: name.into(),
        work_dir,
        socket_path: socket.into(),
        desired_state: None,
        start_on_manager_boot: false,
        idle_timeout_secs: None,
        start_on_demand: false,
    }
}

#[tokio::test]
async fn insert_creates_both_rows_atomically() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    let (daemon, instance_summary) = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("alpha", tmp.path().to_path_buf(), "/tmp/alpha.sock"),
    )
    .await
    .expect("insert ok");

    assert_eq!(daemon.name, "alpha");
    assert_eq!(daemon.desired_state, DesiredState::Stopped);
    assert_eq!(instance_summary.name, "alpha");
    assert_eq!(instance_summary.transport, Transport::Unix);

    // Daemon row is present.
    let daemon_row = managed_daemon::fetch(&pool, daemon.id)
        .await
        .expect("fetch ok");
    assert_eq!(daemon_row.name, "alpha");

    // Paired instance row: transport='unix', managed_by_id=daemon.id, NULL url/api_key/tls_ca_path.
    let row = sqlx::query(
        "SELECT transport, url, api_key, tls_ca_path, managed_by_id, socket_path
         FROM instances WHERE id = ?1",
    )
    .bind(instance_summary.id.to_string())
    .fetch_one(&pool)
    .await
    .expect("instance row");

    let transport: String = row.get("transport");
    let url: Option<String> = row.get("url");
    let api_key: Option<String> = row.get("api_key");
    let tls_ca_path: Option<String> = row.get("tls_ca_path");
    let managed_by_id: Option<String> = row.get("managed_by_id");
    let socket_path: Option<String> = row.get("socket_path");

    assert_eq!(transport, "unix");
    assert!(url.is_none());
    assert!(api_key.is_none());
    assert!(tls_ca_path.is_none());
    assert_eq!(
        managed_by_id.as_deref(),
        Some(daemon.id.to_string().as_str())
    );
    assert_eq!(socket_path.as_deref(), Some("/tmp/alpha.sock"));
}

#[tokio::test]
async fn insert_atomic_rollback_on_paired_failure() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    // Pre-insert a manual instances row that collides with the paired
    // instance INSERT (instances.name UNIQUE).
    instance::insert(
        &pool,
        CreateInstance {
            name: "collide".into(),
            transport: Transport::Unix,
            url: None,
            socket_path: Some("/tmp/already.sock".into()),
            api_key: None,
            tls_ca_path: None,
        },
    )
    .await
    .expect("manual insert ok");

    let err = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("collide", tmp.path().to_path_buf(), "/tmp/collide.sock"),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, AppError::Validation(_)),
        "expected Validation, got {err:?}"
    );

    // No managed_daemons row should remain.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM managed_daemons")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "tx should have rolled back");
}

#[tokio::test]
async fn delete_removes_both_rows_atomically() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    let (daemon, _instance) = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("beta", tmp.path().to_path_buf(), "/tmp/beta.sock"),
    )
    .await
    .expect("insert ok");

    managed_daemon::delete_with_paired_instance(&pool, daemon.id)
        .await
        .expect("delete ok");

    let daemon_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM managed_daemons")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(daemon_count, 0);

    let inst_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM instances WHERE managed_by_id = ?1")
            .bind(daemon.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inst_count, 0);
}

#[tokio::test]
async fn delete_unknown_id_returns_not_found() {
    let pool = db::connect_memory().await.expect("pool");
    let err = managed_daemon::delete_with_paired_instance(&pool, Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound), "got {err:?}");
}

#[tokio::test]
async fn unique_name_rejected() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp_a = tempdir().unwrap();
    let tmp_b = tempdir().unwrap();

    managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("dup", tmp_a.path().to_path_buf(), "/tmp/a.sock"),
    )
    .await
    .expect("first ok");

    let err = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("dup", tmp_b.path().to_path_buf(), "/tmp/b.sock"),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, AppError::Validation(_)),
        "expected Validation, got {err:?}"
    );
}

#[tokio::test]
async fn unique_work_dir_rejected() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("first", tmp.path().to_path_buf(), "/tmp/first.sock"),
    )
    .await
    .expect("first ok");

    // Same physical dir, different syntactic path. canonicalize() on
    // tmp.path().join(".") collapses to the same canonical path so the
    // UNIQUE work_dir constraint should fire.
    let alias = tmp.path().join(".");
    let err = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("second", alias, "/tmp/second.sock"),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, AppError::Validation(_)),
        "expected Validation, got {err:?}"
    );
}

#[tokio::test]
async fn desired_state_default_is_stopped() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    let (daemon, _) = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("default-state", tmp.path().to_path_buf(), "/tmp/d.sock"),
    )
    .await
    .expect("ok");

    let summary = managed_daemon::fetch_summary(&pool, daemon.id)
        .await
        .expect("fetch_summary ok");
    assert_eq!(summary.desired_state, DesiredState::Stopped);
}

#[tokio::test]
async fn set_desired_state_updates_field_and_timestamp() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    let (daemon, _) = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("flip", tmp.path().to_path_buf(), "/tmp/flip.sock"),
    )
    .await
    .expect("ok");

    let initial = managed_daemon::fetch_summary(&pool, daemon.id)
        .await
        .expect("fetch ok");
    assert_eq!(initial.desired_state, DesiredState::Stopped);

    // Sleep a touch so the rfc3339 timestamp advances.
    tokio::time::sleep(Duration::from_millis(10)).await;

    managed_daemon::set_desired_state(&pool, daemon.id, DesiredState::Running)
        .await
        .expect("set ok");

    let after = managed_daemon::fetch_summary(&pool, daemon.id)
        .await
        .expect("fetch ok");
    assert_eq!(after.desired_state, DesiredState::Running);
    assert!(
        after.updated_at > initial.updated_at,
        "expected updated_at to advance: before={:?} after={:?}",
        initial.updated_at,
        after.updated_at
    );
}

#[tokio::test]
async fn idle_timeout_zero_rejected() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp = tempdir().unwrap();

    let mut input = make_input("zerotimeout", tmp.path().to_path_buf(), "/tmp/zt.sock");
    input.idle_timeout_secs = Some(0);
    let err = managed_daemon::insert_with_paired_instance(&pool, input)
        .await
        .unwrap_err();
    match err {
        AppError::Validation(msg) => assert!(
            msg.contains("idle_timeout_secs"),
            "expected idle_timeout_secs in message, got: {msg}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn list_start_on_boot_returns_only_flagged() {
    let pool = db::connect_memory().await.expect("pool");
    let tmp_a = tempdir().unwrap();
    let tmp_b = tempdir().unwrap();

    let mut flagged = make_input("flagged", tmp_a.path().to_path_buf(), "/tmp/f.sock");
    flagged.start_on_manager_boot = true;
    managed_daemon::insert_with_paired_instance(&pool, flagged)
        .await
        .expect("flagged ok");

    let unflagged = make_input("normal", tmp_b.path().to_path_buf(), "/tmp/n.sock");
    managed_daemon::insert_with_paired_instance(&pool, unflagged)
        .await
        .expect("normal ok");

    let rows: Vec<ManagedDaemonRow> = managed_daemon::list_start_on_boot(&pool)
        .await
        .expect("list ok");
    assert_eq!(rows.len(), 1, "got: {rows:?}");
    assert_eq!(rows[0].name, "flagged");
    assert_eq!(rows[0].start_on_manager_boot, 1);
}

#[tokio::test]
async fn manual_instance_with_managed_by_null_unaffected() {
    let pool = db::connect_memory().await.expect("pool");

    // Manual instance via existing instance::insert flow — managed_by_id is NULL.
    let manual = instance::insert(
        &pool,
        CreateInstance {
            name: "manual".into(),
            transport: Transport::Unix,
            url: None,
            socket_path: Some("/tmp/manual.sock".into()),
            api_key: None,
            tls_ca_path: None,
        },
    )
    .await
    .expect("manual ok");

    // Insert+delete cycle of a managed daemon.
    let tmp = tempdir().unwrap();
    let (daemon, _) = managed_daemon::insert_with_paired_instance(
        &pool,
        make_input("managed-cycle", tmp.path().to_path_buf(), "/tmp/mc.sock"),
    )
    .await
    .expect("daemon insert ok");
    managed_daemon::delete_with_paired_instance(&pool, daemon.id)
        .await
        .expect("daemon delete ok");

    // Manual row is still there with managed_by_id=NULL.
    let row = sqlx::query("SELECT managed_by_id FROM instances WHERE id = ?1")
        .bind(manual.id.to_string())
        .fetch_optional(&pool)
        .await
        .expect("query ok");
    let row = row.expect("manual row should still exist");
    let managed_by_id: Option<String> = row.get("managed_by_id");
    assert!(
        managed_by_id.is_none(),
        "manual row managed_by_id should remain NULL, got {managed_by_id:?}"
    );
}
