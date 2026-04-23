use std::path::Path;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::{Request, Response};

/// Send a single agent-IPC request to the orchestrator and return its reply.
///
/// Wire format: one JSON line per message. The client writes the request,
/// reads one response line, and lets the connection close.
pub async fn send(socket: &Path, request: &Request) -> Result<Response> {
    let stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("connecting to agent socket {}", socket.display()))?;

    let (read_half, mut write_half) = stream.into_split();
    let mut payload = serde_json::to_string(request)?;
    payload.push('\n');
    write_half.write_all(payload.as_bytes()).await?;
    write_half.flush().await?;

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .context("reading response from agent socket")?;
    if line.is_empty() {
        anyhow::bail!("agent socket closed without sending a response");
    }
    let resp: Response = serde_json::from_str(line.trim())
        .with_context(|| format!("parsing agent socket response: {line:?}"))?;
    Ok(resp)
}
