//! Optional local Unix socket that tunnels the full `RequestCmd`
//! surface through to the remote instance.
//!
//! Wire format is identical to the internal agent IPC
//! (`src/agent_ipc/server.rs`): one JSON line carrying a full
//! [`agent_ipc::Request`] in, one JSON line carrying a
//! [`agent_ipc::Response`] out. The `agent_id` field on the incoming
//! request is discarded — the remote webservice stamps its own.
//!
//! This is what lets local tools set `ACIDDOG_AGENT_SOCKET` to the
//! proxy path and use the `aciddog` CLI transparently against the
//! remote.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::warn;

use super::http_client::Client;
use crate::agent_ipc::{Request as IpcRequest, Response as IpcResponse};
use crate::ai::event::AIEvent;

pub async fn run_proxy(
    idx: usize,
    automaton_id: String,
    path: PathBuf,
    client: Client,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
    mut cancel_rx: mpsc::Receiver<()>,
) {
    let listener = match bind_listener(&path).await {
        Ok(l) => l,
        Err(e) => {
            let _ = ai_tx
                .send((
                    idx,
                    AIEvent::AutomatonStderr {
                        line: format!(
                            "remote `{automaton_id}` proxy failed to bind {}: {e}",
                            path.display()
                        ),
                    },
                ))
                .await;
            return;
        }
    };

    let _ = ai_tx
        .send((
            idx,
            AIEvent::AutomatonStdout {
                line: format!("remote `{automaton_id}` proxy listening on {}", path.display()),
            },
        ))
        .await;

    loop {
        tokio::select! {
            biased;
            _ = cancel_rx.recv() => break,
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _)) => {
                        let client = client.clone();
                        let ai_tx = ai_tx.clone();
                        let automaton_id = automaton_id.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, &client).await {
                                let _ = ai_tx
                                    .send((
                                        idx,
                                        AIEvent::AutomatonStderr {
                                            line: format!(
                                                "remote `{automaton_id}` proxy connection error: {e}"
                                            ),
                                        },
                                    ))
                                    .await;
                            }
                        });
                    }
                    Err(e) => {
                        warn!(automaton = %automaton_id, error = %e, "remote proxy accept error");
                    }
                }
            }
        }
    }

    let _ = tokio::fs::remove_file(&path).await;
}

async fn bind_listener(path: &PathBuf) -> Result<UnixListener> {
    if let Some(parent) = path.parent() {
        // Only chmod the parent if we created it — see the matching
        // comment in `webservice::http::serve_unix` for why.
        let existed = tokio::fs::metadata(parent).await.is_ok();
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating proxy socket parent {}", parent.display()))?;
        if !existed {
            let mut perms = tokio::fs::metadata(parent).await?.permissions();
            perms.set_mode(0o700);
            tokio::fs::set_permissions(parent, perms).await?;
        }
    }
    if path.exists() {
        if let Ok(_live) = UnixStream::connect(path).await {
            return Err(anyhow!(
                "proxy socket {} is already in use by a running listener",
                path.display()
            ));
        }
        tokio::fs::remove_file(path)
            .await
            .with_context(|| format!("removing stale proxy socket {}", path.display()))?;
    }
    let listener = UnixListener::bind(path)
        .with_context(|| format!("binding proxy UnixListener at {}", path.display()))?;
    let mut file_perms = tokio::fs::metadata(path).await?.permissions();
    file_perms.set_mode(0o660);
    tokio::fs::set_permissions(path, file_perms).await?;
    Ok(listener)
}

async fn handle_connection(stream: UnixStream, client: &Client) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .await
        .context("reading proxy request line")?;
    if n == 0 {
        return Ok(());
    }

    let request: IpcRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            let resp = IpcResponse::err(format!("malformed request: {e}"));
            write_response(&mut write_half, &resp).await?;
            return Ok(());
        }
    };

    // The `agent_id` is discarded — the remote webservice stamps its
    // own session id on inbound requests.
    let resp = match client.send_rpc(&request.cmd).await {
        Ok(r) => r,
        Err(e) => IpcResponse::err(format!("remote rpc failed: {e}")),
    };
    write_response(&mut write_half, &resp).await
}

async fn write_response(write: &mut tokio::net::unix::OwnedWriteHalf, resp: &IpcResponse) -> Result<()> {
    let mut text = serde_json::to_string(resp).context("serializing proxy response")?;
    text.push('\n');
    write.write_all(text.as_bytes()).await?;
    write.flush().await?;
    Ok(())
}
