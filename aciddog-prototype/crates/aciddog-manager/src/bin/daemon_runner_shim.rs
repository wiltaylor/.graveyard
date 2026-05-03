//! Test shim used by the `daemon_runner_smoke` integration tests.
//!
//! Behaviour:
//!   * `--exit-immediately` exits non-zero straight away so the spawn
//!     pipeline observes a `Crashed` transition.
//!   * Otherwise binds a `tokio::net::UnixListener` at the path passed
//!     via `--socket <path>` (or the env fallback
//!     `DAEMON_RUNNER_SHIM_SOCKET`), sets perms to `0o660`, installs a
//!     SIGTERM handler, and serves accept-and-hold connections so the
//!     manager's socket probe can connect.
//!
//! Uses only deps already present in the crate (`tokio` + std). `nix`
//! is intentionally not used here — `tokio::signal::unix` is enough.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::ExitCode;

use tokio::net::UnixListener;
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() -> ExitCode {
    let mut exit_immediately = false;
    let mut socket_arg: Option<PathBuf> = None;

    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--exit-immediately" => exit_immediately = true,
            "--socket" => {
                socket_arg = args.next().map(PathBuf::from);
            }
            _ => {
                // Unknown flags are ignored so the test driver can pass
                // through arbitrary trailing args without breakage.
            }
        }
    }

    if exit_immediately {
        eprintln!("daemon_runner_shim: --exit-immediately, exiting non-zero");
        return ExitCode::from(2);
    }

    let socket_path =
        match socket_arg.or_else(|| env::var_os("DAEMON_RUNNER_SHIM_SOCKET").map(PathBuf::from)) {
            Some(p) => p,
            None => {
                eprintln!(
                    "daemon_runner_shim: missing --socket <path> or DAEMON_RUNNER_SHIM_SOCKET env"
                );
                return ExitCode::from(64);
            }
        };

    // Best-effort cleanup of a stale socket from a previous run.
    let _ = fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "daemon_runner_shim: bind {} failed: {e}",
                socket_path.display()
            );
            return ExitCode::from(74);
        }
    };

    if let Err(e) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660)) {
        eprintln!(
            "daemon_runner_shim: set_permissions on {} failed: {e}",
            socket_path.display()
        );
        // Non-fatal — the probe only needs to connect, not read perms.
    }

    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("daemon_runner_shim: signal install failed: {e}");
            return ExitCode::from(70);
        }
    };

    eprintln!("daemon_runner_shim: listening on {}", socket_path.display());

    // Hold accepted streams in a Vec so they stay alive for the
    // duration of the probe — dropping immediately would close before
    // the manager-side connect returns. The Vec drains on graceful
    // shutdown.
    let mut held = Vec::new();
    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                eprintln!("daemon_runner_shim: SIGTERM, shutting down");
                drop(held);
                let _ = fs::remove_file(&socket_path);
                return ExitCode::SUCCESS;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, _addr)) => held.push(stream),
                    Err(e) => {
                        eprintln!("daemon_runner_shim: accept failed: {e}");
                    }
                }
            }
        }
    }
}
