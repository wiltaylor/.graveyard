//! Integration tests for `scaffolding::create_managed_daemon`.
//!
//! Each test wraps the `aciddog_init_shim` test binary in a `sh`
//! wrapper script (the same trick `daemon_runner_smoke.rs` uses) so
//! the per-test variant env vars survive the
//! `Command::env_clear()` call inside the scaffolding flow — the
//! shell sets them after env_clear has already happened to scaffolding's
//! `Command`.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

use aciddog_manager::auth::AuthConfig;
use aciddog_manager::db;
use aciddog_manager::error::AppError;
use aciddog_manager::scaffolding::{create_managed_daemon, CreateManagedDaemonRequest};
use aciddog_manager::state::AppState;

/// Path to the `aciddog_init_shim` test binary, populated by Cargo at
/// build time.
fn shim_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aciddog_init_shim"))
}

/// Write a `sh` wrapper script that `export`s each `(KEY, VAL)` pair
/// and then `exec`s the shim with the trailing args from the caller
/// (whatever the scaffolding flow passes — `init <target>` or
/// `daemon --check`). chmod 0o755.
fn write_wrapper(wrapper_path: &Path, shim: &Path, env: &[(&str, &str)]) {
    let mut body = String::from("#!/bin/sh\n");
    for (k, v) in env {
        body.push_str(&format!("export {}={}\n", k, shell_escape(v)));
    }
    body.push_str(&format!("exec {:?} \"$@\"\n", shim));
    fs::write(wrapper_path, body).expect("write wrapper script");
    let mut perms = fs::metadata(wrapper_path)
        .expect("stat wrapper")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(wrapper_path, perms).expect("chmod wrapper");
}

fn shell_escape(s: &str) -> String {
    // Single-quote escape: ' becomes '\''
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

struct Fixture {
    _data_dir: TempDir,
    state: AppState,
    work_dir_root: PathBuf,
}

impl Fixture {
    async fn build_with_env(env: &[(&str, &str)]) -> Self {
        // tempdir lives until the fixture drops; data_dir + the wrapper
        // script + the sqlite file all sit inside it.
        let data_dir = tempfile::tempdir().expect("tempdir");
        let db_path = data_dir.path().join("manager.db");
        let pool = db::connect(&db_path).await.expect("db connect");

        let wrapper_path = data_dir.path().join("aciddog-shim.sh");
        write_wrapper(&wrapper_path, &shim_path(), env);

        let mut state = AppState::new(
            pool,
            AuthConfig {
                cookie_secure: false,
            },
            data_dir.path().to_path_buf(),
            true,
        );
        state.aciddog_binary = Some(Arc::new(wrapper_path));

        let work_dir_root = data_dir.path().join("teams");

        Fixture {
            _data_dir: data_dir,
            state,
            work_dir_root,
        }
    }

    fn work_dir(&self, name: &str) -> PathBuf {
        self.work_dir_root.join(name)
    }
}

// 1. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn happy_path_template_already_has_webservice() {
    let fx = Fixture::build_with_env(&[]).await;
    let (daemon, instance) = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "foo".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("scaffold ok");

    assert_eq!(daemon.name, "foo");
    assert_eq!(instance.name, "foo");

    let wcl_path = fx.work_dir("foo").join("aciddog.wcl");
    let body = fs::read_to_string(&wcl_path).expect("read wcl");
    assert!(
        body.contains(r#"socket_path      = ".aciddog/daemon.sock""#),
        "canonical socket_path line missing in {}",
        body
    );
    assert!(
        fx.work_dir("foo").join(".aciddog").is_dir(),
        ".aciddog/ should exist"
    );
    let want_socket = fs::canonicalize(fx.work_dir("foo"))
        .unwrap()
        .join(".aciddog/daemon.sock");
    assert_eq!(
        instance.socket_path.as_deref(),
        Some(want_socket.to_str().unwrap())
    );
}

// 2. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injects_when_template_missing_webservice() {
    let fx = Fixture::build_with_env(&[("ACIDDOG_INIT_SHIM_VARIANT", "missing-webservice")]).await;
    create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "bar".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("scaffold ok");

    let wcl_path = fx.work_dir("bar").join("aciddog.wcl");
    let body = fs::read_to_string(&wcl_path).expect("read wcl");
    assert!(
        body.contains("automaton local_api {"),
        "appended local_api block missing: {body}"
    );
    assert!(
        body.contains(r#"socket_path      = ".aciddog/daemon.sock""#),
        "canonical socket_path missing: {body}"
    );
    assert!(
        body.contains("type             = :webservice"),
        "type line missing: {body}"
    );
}

// 3. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rewrites_when_socket_path_diverges() {
    let fx = Fixture::build_with_env(&[("ACIDDOG_INIT_SHIM_VARIANT", "wrong-socket")]).await;
    create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "baz".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("scaffold ok");

    let body = fs::read_to_string(fx.work_dir("baz").join("aciddog.wcl")).expect("read wcl");
    assert!(
        body.contains(r#"socket_path      = ".aciddog/daemon.sock""#),
        "canonical socket_path missing: {body}"
    );
    assert!(
        !body.contains("some/other.sock"),
        "old socket_path still present: {body}"
    );
}

// 4. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_invalid_name_uppercase_charset() {
    let fx = Fixture::build_with_env(&[]).await;
    // `Foo!` lowercases to `foo!` which the charset gate rejects.
    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "Foo!".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("must reject");
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
}

// 5. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_reserved_name() {
    let fx = Fixture::build_with_env(&[]).await;
    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "system".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("must reject");
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
}

// 6. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflict_when_db_row_exists() {
    let fx = Fixture::build_with_env(&[]).await;
    create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "first".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("first scaffold ok");

    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "first".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("must conflict");
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

// 7. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn re_scaffolds_when_dir_empty_and_no_db_row() {
    let fx = Fixture::build_with_env(&[]).await;
    fs::create_dir_all(fx.work_dir("rescaffold")).expect("mkdir");
    // Confirm dir is empty before the call.
    assert!(fs::read_dir(fx.work_dir("rescaffold"))
        .unwrap()
        .next()
        .is_none());

    create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "rescaffold".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("scaffold ok");

    assert!(fx.work_dir("rescaffold").join("aciddog.wcl").is_file());
}

// 8. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_when_dir_nonempty_and_no_db_row() {
    let fx = Fixture::build_with_env(&[]).await;
    let work = fx.work_dir("dirty");
    fs::create_dir_all(&work).expect("mkdir");
    fs::write(work.join("junk.txt"), "operator put this here\n").expect("write junk");

    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "dirty".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("must reject");
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");

    // Operator file must survive — we did not create the dir, we don't
    // wipe it.
    assert!(work.join("junk.txt").is_file());
}

// 9. ---------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dry_validation_skip_on_missing_flag() {
    let fx = Fixture::build_with_env(&[("ACIDDOG_INIT_SHIM_CHECK", "missing-flag")]).await;
    let (daemon, _) = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "skipper".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("scaffold ok despite check missing");
    assert_eq!(
        daemon.desired_state,
        aciddog_manager::model::managed_daemon::DesiredState::Stopped
    );
}

// 10. --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dry_validation_failure_rolls_back() {
    let fx = Fixture::build_with_env(&[("ACIDDOG_INIT_SHIM_CHECK", "parse-error")]).await;
    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "broken".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("must fail");
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
    assert!(
        !fx.work_dir("broken").exists(),
        "work_dir must be rolled back"
    );
}

// 11. --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn init_failure_rolls_back() {
    let fx = Fixture::build_with_env(&[("ACIDDOG_INIT_SHIM_VARIANT", "fail")]).await;
    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "stillborn".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("init must fail");
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
    assert!(
        !fx.work_dir("stillborn").exists(),
        "work_dir must be rolled back"
    );
}

// 12. --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn db_insert_failure_rolls_back_filesystem() {
    let fx = Fixture::build_with_env(&[]).await;
    // Pre-insert an unrelated `instances` row whose `name = "ghost"`
    // will collide with the paired-instance INSERT inside
    // `insert_with_paired_instance`. `check_idempotency` only looks at
    // `managed_daemons`, so we get past the pre-flight checks; the
    // DB-level UNIQUE on `instances.name` is what fires.
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO instances (id, name, transport, url, socket_path, api_key, tls_ca_path, managed_by_id, created_at)
         VALUES (?1, 'ghost', 'unix', NULL, '/tmp/ghost.sock', NULL, NULL, NULL, ?2)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&now)
    .execute(&fx.state.pool)
    .await
    .expect("pre-insert ghost row");

    let err = create_managed_daemon(
        &fx.state,
        CreateManagedDaemonRequest {
            name: "ghost".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect_err("must fail on instances.name UNIQUE");
    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
    assert!(
        !fx.work_dir("ghost").exists(),
        "work_dir must be rolled back"
    );

    // No `managed_daemons` row should have been committed (tx rolled back).
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM managed_daemons WHERE name = 'ghost'")
            .fetch_all(&fx.state.pool)
            .await
            .expect("query");
    assert!(rows.is_empty(), "managed_daemons row must not be committed");
}

// 13. --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn env_allowlist_holds() {
    // Bypass the sh wrapper for this test — `sh` itself injects SHLVL
    // / PWD / _ into the child env, which would muddy the allowlist
    // assertion. The shim is std-only and exec's directly without any
    // shell intermediary, so what scaffolding's env_clear+allowlist
    // hands it is exactly what `std::env::vars()` observes.
    let data_dir = tempfile::tempdir().expect("tempdir");
    let db_path = data_dir.path().join("manager.db");
    let pool = db::connect(&db_path).await.expect("db connect");
    let mut state = AppState::new(
        pool,
        AuthConfig {
            cookie_secure: false,
        },
        data_dir.path().to_path_buf(),
        true,
    );
    state.aciddog_binary = Some(Arc::new(shim_path()));
    let work_dir_root = data_dir.path().join("teams");

    // Setting an env var here is process-global; other tests don't
    // assert on it (only this test reads `observed-env.txt`), so the
    // worst case is that this var is visible inside the parent test
    // process for a while. Scaffolding's env_clear()+allowlist drops
    // it before subprocess spawn, so observed-env.txt MUST NOT carry
    // it.
    //
    // SAFETY: std::env::set_var is only unsafe on the rust 2024+ edition.
    // This crate is on edition 2021; the call is safe-by-default.
    std::env::set_var("ACIDDOG_TEST_LEAK", "1");

    create_managed_daemon(
        &state,
        CreateManagedDaemonRequest {
            name: "envtest".into(),
            autostart_on_boot: false,
            idle_timeout_secs: None,
            start_on_demand: false,
            template: None,
        },
    )
    .await
    .expect("scaffold ok");

    let observed_path = work_dir_root
        .join("envtest")
        .join(".aciddog")
        .join("observed-env.txt");
    let body = fs::read_to_string(&observed_path).expect("read observed-env");
    let keys: HashSet<&str> = body
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k))
        .collect();

    assert!(
        !keys.contains("ACIDDOG_TEST_LEAK"),
        "ACIDDOG_TEST_LEAK must not leak; got keys: {:?}",
        keys
    );
    let allowed: HashSet<&str> = ["PATH", "HOME", "USER", "LANG"].into_iter().collect();
    for k in &keys {
        assert!(
            allowed.contains(k),
            "key `{}` outside allowlist; got keys: {:?}",
            k,
            keys
        );
    }

    // Best-effort cleanup (other tests may already have observed it,
    // but unsetting limits the blast radius).
    std::env::remove_var("ACIDDOG_TEST_LEAK");

    // Keep the tempdir alive until after the assertions.
    drop(data_dir);
}
