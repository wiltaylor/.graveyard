use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use super::{AgentSocketEvent, Request, Response};

/// A bound agent-IPC socket and the event stream it feeds.
///
/// Dropping the `Bound` aborts the accept loop and unlinks the socket file,
/// so ownership of this value keeps the server alive.
pub struct Bound {
    pub path: PathBuf,
    pub events: mpsc::Receiver<AgentSocketEvent>,
    /// Clone source for the same channel `events` drains from. Kept so
    /// in-process callers (e.g. the claude-code core's tag-dispatch
    /// path) can inject `AgentSocketEvent`s that land in the same
    /// handler pipeline as socket-borne requests.
    events_tx: mpsc::Sender<AgentSocketEvent>,
    accept_handle: JoinHandle<()>,
}

impl Bound {
    /// Clone of the channel sender feeding [`events`]. Use this to
    /// dispatch IPC requests from inside the process without going
    /// through the Unix socket.
    pub fn events_tx(&self) -> mpsc::Sender<AgentSocketEvent> {
        self.events_tx.clone()
    }
}

impl Drop for Bound {
    fn drop(&mut self) {
        self.accept_handle.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Bind the agent IPC socket for the given team.
///
/// Path: `$XDG_RUNTIME_DIR/aciddog/<team_id>/agent.socket`. The directory is
/// created with mode 0700 if missing.
///
/// Stale-socket policy: if the path already exists, probe it; if a live
/// listener answers, refuse to start (don't steal the socket). If the
/// connect is refused, unlink and bind anyway.
pub async fn bind(team_id: &str) -> Result<Bound> {
    let path = socket_path(team_id)?;
    bind_at(path).await
}

/// Variant of [`bind`] that accepts an explicit path — intended for tests
/// and for callers that have already resolved the path.
pub async fn bind_at(path: PathBuf) -> Result<Bound> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating socket parent dir {}", parent.display()))?;
        let mut perms = tokio::fs::metadata(parent).await?.permissions();
        perms.set_mode(0o700);
        tokio::fs::set_permissions(parent, perms).await?;
    }

    if path.exists() {
        match UnixStream::connect(&path).await {
            Ok(_) => {
                return Err(anyhow!(
                    "aciddog socket at {} is already in use by a running instance",
                    path.display()
                ));
            }
            Err(_) => {
                debug!(path = %path.display(), "removing stale socket file");
                tokio::fs::remove_file(&path)
                    .await
                    .with_context(|| format!("removing stale socket file {}", path.display()))?;
            }
        }
    }

    let listener = UnixListener::bind(&path)
        .with_context(|| format!("binding UnixListener at {}", path.display()))?;

    let (tx, events) = mpsc::channel::<AgentSocketEvent>(32);
    let events_tx = tx.clone();

    let accept_handle = tokio::spawn(async move {
        accept_loop(listener, tx).await;
    });

    Ok(Bound {
        path,
        events,
        events_tx,
        accept_handle,
    })
}

/// Resolve the socket path for a team id from `$XDG_RUNTIME_DIR`.
pub fn socket_path(team_id: &str) -> Result<PathBuf> {
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| anyhow!("XDG_RUNTIME_DIR is not set; required to locate the agent socket"))?;
    Ok(PathBuf::from(runtime)
        .join("aciddog")
        .join(team_id)
        .join("agent.socket"))
}

async fn accept_loop(listener: UnixListener, tx: mpsc::Sender<AgentSocketEvent>) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let conn_tx = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, conn_tx).await {
                        warn!(error = %e, "agent ipc connection error");
                    }
                });
            }
            Err(e) => {
                warn!(error = %e, "agent ipc accept error, stopping loop");
                return;
            }
        }
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    tx: mpsc::Sender<AgentSocketEvent>,
) -> Result<()> {
    let (read_half, mut write_half) = stream.split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(());
    }

    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::err(format!("malformed request: {e}"));
            write_response(&mut write_half, &resp).await?;
            return Ok(());
        }
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(AgentSocketEvent {
            request,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        let resp = Response::err("server shutting down");
        write_response(&mut write_half, &resp).await?;
        return Ok(());
    }

    let resp = match reply_rx.await {
        Ok(r) => r,
        Err(_) => Response::err("no reply from orchestrator"),
    };
    write_response(&mut write_half, &resp).await?;
    Ok(())
}

async fn write_response<W>(writer: &mut W, resp: &Response) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut payload = serde_json::to_string(resp)?;
    payload.push('\n');
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_ipc::{Request, RequestCmd};
    use uuid::Uuid;

    fn tmp_socket_path() -> PathBuf {
        let dir = tempfile::tempdir().unwrap().keep();
        dir.join("agent.socket")
    }

    #[tokio::test]
    async fn second_bind_on_live_socket_errors() {
        let path = tmp_socket_path();
        let _first = bind_at(path.clone()).await.expect("first bind ok");
        match bind_at(path.clone()).await {
            Ok(_) => panic!("second bind should have failed while first is live"),
            Err(e) => assert!(
                e.to_string().contains("already in use"),
                "unexpected error: {e}"
            ),
        }
    }

    #[tokio::test]
    async fn stale_socket_is_reclaimed() {
        let path = tmp_socket_path();
        // Create a dead socket file that nothing is listening on.
        {
            let listener = UnixListener::bind(&path).unwrap();
            drop(listener);
        }
        // File still present, but no listener. Bind should reclaim it.
        assert!(path.exists());
        let _bound = bind_at(path.clone())
            .await
            .expect("should reclaim stale socket");
    }

    #[tokio::test]
    async fn drop_unlinks_socket() {
        let path = tmp_socket_path();
        let bound = bind_at(path.clone()).await.unwrap();
        assert!(path.exists());
        drop(bound);
        // Small settle — drop is synchronous for the unlink.
        assert!(!path.exists(), "socket file should be unlinked on drop");
    }

    #[tokio::test]
    async fn roundtrip_request_response() {
        let path = tmp_socket_path();
        let mut bound = bind_at(path.clone()).await.unwrap();
        let id = Uuid::new_v4();

        // Spawn the client send.
        let client_path = path.clone();
        let client = tokio::spawn(async move {
            crate::agent_ipc::client::send(
                &client_path,
                &Request {
                    agent_id: id,
                    cmd: RequestCmd::Test,
                },
            )
            .await
        });

        // Receive the event on the server side and reply.
        let event = bound.events.recv().await.expect("event delivered");
        assert_eq!(event.request.agent_id, id);
        assert!(matches!(event.request.cmd, RequestCmd::Test));
        event.reply.send(Response::ok("pong")).unwrap();

        let resp = client.await.unwrap().expect("client send ok");
        assert!(resp.ok);
        assert_eq!(resp.message.as_deref(), Some("pong"));
    }
}
