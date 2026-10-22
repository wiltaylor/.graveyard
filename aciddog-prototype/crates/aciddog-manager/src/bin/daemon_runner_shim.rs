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
//!   * `--serve-health` (added by sibling `d67a4d0c` for the reconciler
//!     smoke tests) flips the accept loop to read each request to
//!     `\r\n\r\n` and reply with `HTTP/1.1 200 OK` so `Client::health()`
//!     succeeds. Without the flag the loop just accept-and-holds, which
//!     is what the existing `daemon_runner_smoke` tests rely on.
//!   * `--emit-stdout-bytes <N>` / `--emit-stderr-bytes <N>` /
//!     `--hold-after-emit` (added by sibling 3bc8a0d0 for the
//!     log_capture integration test) write `N` bytes of `'A'`s on the
//!     respective stream in 80-byte newline-terminated chunks. After
//!     both emissions the shim exits 0, OR (with `--hold-after-emit`)
//!     falls through to the existing socket loop so the runner watcher
//!     can still observe `Running`.
//!
//! Uses only deps already present in the crate (`tokio` + std). `nix`
//! is intentionally not used here — `tokio::signal::unix` is enough.

use std::env;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::ExitCode;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() -> ExitCode {
    let mut exit_immediately = false;
    let mut serve_health = false;
    let mut socket_arg: Option<PathBuf> = None;
    let mut emit_stdout_bytes: u64 = 0;
    let mut emit_stderr_bytes: u64 = 0;
    let mut hold_after_emit = false;

    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--exit-immediately" => exit_immediately = true,
            "--serve-health" => serve_health = true,
            "--socket" => {
                socket_arg = args.next().map(PathBuf::from);
            }
            "--emit-stdout-bytes" => {
                emit_stdout_bytes = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "--emit-stderr-bytes" => {
                emit_stderr_bytes = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "--hold-after-emit" => hold_after_emit = true,
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

    // Sibling 3bc8a0d0: drive the log_capture path. Each emission is
    // chunked into 80-byte 'A' lines so the line-oriented reader on
    // the manager side has plenty of newlines to chew through.
    if emit_stdout_bytes > 0 {
        emit_stream(emit_stdout_bytes, false);
    }
    if emit_stderr_bytes > 0 {
        emit_stream(emit_stderr_bytes, true);
    }
    if (emit_stdout_bytes > 0 || emit_stderr_bytes > 0) && !hold_after_emit {
        // No socket loop requested: exit cleanly so tests that only
        // exercise the capture path don't have to teardown the listener.
        return ExitCode::SUCCESS;
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
                    Ok((mut stream, _addr)) => {
                        if serve_health {
                            // Drive each connection through a minimal
                            // HTTP/1.1 200 reply on its own task so we
                            // don't block the accept loop.
                            tokio::spawn(async move {
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
                                            if total.len() > 8192 {
                                                break;
                                            }
                                        }
                                        Err(_) => return,
                                    }
                                }
                                let body = b"ok";
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = stream.write_all(header.as_bytes()).await;
                                let _ = stream.write_all(body).await;
                                let _ = stream.shutdown().await;
                            });
                        } else {
                            held.push(stream);
                        }
                    }
                    Err(e) => {
                        eprintln!("daemon_runner_shim: accept failed: {e}");
                    }
                }
            }
        }
    }
}

/// Emit `total` bytes of `'A'`s onto the chosen stream, chunked into
/// 80-byte lines (79 'A's + '\n', so each line is exactly 80 bytes
/// of content + terminator) until the budget is met. The final partial
/// line (if any) is also newline-terminated so the manager-side line
/// reader sees a clean EOL boundary.
fn emit_stream(total: u64, stderr: bool) {
    const CHUNK: usize = 80;
    let mut remaining = total;
    let mut buf = Vec::with_capacity(CHUNK);
    if stderr {
        let mut out = std::io::stderr().lock();
        while remaining > 0 {
            buf.clear();
            let n = std::cmp::min(remaining, CHUNK as u64) as usize;
            let body_len = if n > 0 { n - 1 } else { 0 };
            buf.extend(std::iter::repeat_n(b'A', body_len));
            buf.push(b'\n');
            let _ = out.write_all(&buf);
            remaining = remaining.saturating_sub(n as u64);
        }
        let _ = out.flush();
    } else {
        let mut out = std::io::stdout().lock();
        while remaining > 0 {
            buf.clear();
            let n = std::cmp::min(remaining, CHUNK as u64) as usize;
            let body_len = if n > 0 { n - 1 } else { 0 };
            buf.extend(std::iter::repeat_n(b'A', body_len));
            buf.push(b'\n');
            let _ = out.write_all(&buf);
            remaining = remaining.saturating_sub(n as u64);
        }
        let _ = out.flush();
    }
}
