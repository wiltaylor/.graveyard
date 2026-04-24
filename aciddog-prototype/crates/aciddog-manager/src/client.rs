//! Thin HTTPS / Unix-socket client for talking to an aciddog :webservice.
//!
//! Mirrors `aciddog::remote_automaton::http_client::Client` but takes a
//! literal bearer token (stored in our SQLite `instances` table) and an
//! optional CA path instead of env-var lookups.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes as HyperBytes;
use hyper::{Request as HyperRequest, StatusCode as HyperStatus};
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

use crate::ipc::{IpcResponse, RequestCmd};
use crate::model::instance::{InstanceRow, Transport};

#[derive(Clone)]
pub struct Client {
    inner: Inner,
}

#[derive(Clone)]
enum Inner {
    Https {
        client: reqwest::Client,
        base_url: String,
        bearer: Option<Arc<String>>,
    },
    Unix {
        path: Arc<PathBuf>,
    },
}

impl Client {
    /// Build a client from a stored instance row.
    pub fn from_row(row: &InstanceRow) -> Result<Self> {
        let transport = parse_transport(&row.transport)?;
        match transport {
            Transport::Https => {
                let url = row
                    .url
                    .as_deref()
                    .ok_or_else(|| anyhow!("instance has transport=https but no url"))?
                    .trim_end_matches('/')
                    .to_string();
                let mut builder = reqwest::Client::builder();
                if let Some(ca_path) = row.tls_ca_path.as_deref() {
                    let pem = std::fs::read(ca_path)
                        .with_context(|| format!("reading tls_ca file {ca_path}"))?;
                    let cert = reqwest::Certificate::from_pem(&pem)
                        .with_context(|| format!("parsing tls_ca PEM at {ca_path}"))?;
                    builder = builder.add_root_certificate(cert);
                }
                let client = builder.build().context("building reqwest client")?;
                Ok(Self {
                    inner: Inner::Https {
                        client,
                        base_url: url,
                        bearer: row.api_key.clone().map(Arc::new),
                    },
                })
            }
            Transport::Unix => {
                let path = row
                    .socket_path
                    .as_deref()
                    .ok_or_else(|| anyhow!("instance has transport=unix but no socket_path"))?;
                Ok(Self {
                    inner: Inner::Unix {
                        path: Arc::new(PathBuf::from(path)),
                    },
                })
            }
        }
    }

    /// POST a single `RequestCmd` to `/v1/rpc` and return the parsed reply.
    pub async fn send_rpc(&self, cmd: &RequestCmd) -> Result<IpcResponse> {
        let body = serde_json::to_vec(cmd).context("serializing RequestCmd")?;
        match &self.inner {
            Inner::Https {
                client,
                base_url,
                bearer,
            } => {
                let url = format!("{base_url}/v1/rpc");
                let mut req = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(body);
                if let Some(token) = bearer.as_deref() {
                    req = req.bearer_auth(token);
                }
                let resp = req.send().await.with_context(|| format!("POST {url}"))?;
                let status = resp.status().as_u16();
                let bytes = resp
                    .bytes()
                    .await
                    .context("reading /v1/rpc response body")?;
                parse_rpc_response(status, &bytes)
            }
            Inner::Unix { path } => {
                let (status, bytes) = uds_request(
                    path,
                    "POST",
                    "/v1/rpc",
                    &[("content-type", "application/json")],
                    Some(body),
                )
                .await?;
                parse_rpc_response(status, &bytes)
            }
        }
    }

    /// Open an SSE byte stream against the webservice's scoped
    /// chat-events endpoint. The returned stream yields the raw SSE
    /// frame bytes (already chunked by the server) — the manager's
    /// axum handler forwards them verbatim to the browser, which
    /// parses them via `EventSource`.
    ///
    /// Both transports stream incrementally: the HTTPS branch uses
    /// reqwest's `bytes_stream()`; the Unix branch uses hyper's
    /// frame-at-a-time body so long-running SSE connections don't
    /// pre-buffer everything (unlike [`send_rpc`], which can afford
    /// to `.collect()` a bounded reply body).
    pub async fn open_chat_events(
        &self,
        agent: &str,
    ) -> Result<(
        u16,
        std::pin::Pin<Box<dyn Stream<Item = Result<Bytes, anyhow::Error>> + Send>>,
    )> {
        // Percent-encode the agent id. We hand-roll a minimal encoder
        // for the characters likely to appear in worker ids
        // (`#`, ` `) rather than pulling in `urlencoding` just for
        // this one site.
        let encoded_agent = encode_query(agent);
        match &self.inner {
            Inner::Https {
                client,
                base_url,
                bearer,
            } => {
                let url = format!("{base_url}/v1/chat/events?agent={encoded_agent}");
                let mut req = client.get(&url).header("accept", "text/event-stream");
                if let Some(token) = bearer.as_deref() {
                    req = req.bearer_auth(token);
                }
                let resp = req.send().await.with_context(|| format!("GET {url}"))?;
                let status = resp.status().as_u16();
                let stream = resp
                    .bytes_stream()
                    .map(|r| r.map_err(|e| anyhow!("sse chunk: {e}")));
                Ok((status, Box::pin(stream)))
            }
            Inner::Unix { path } => {
                let target = format!("/v1/chat/events?agent={encoded_agent}");
                let (status, body_stream) =
                    uds_stream_request(path, "GET", &target, &[("accept", "text/event-stream")])
                        .await?;
                Ok((status, body_stream))
            }
        }
    }

    /// GET /v1/health. Returns the raw status code and the response body.
    pub async fn health(&self) -> Result<(u16, String)> {
        match &self.inner {
            Inner::Https {
                client,
                base_url,
                bearer,
            } => {
                let url = format!("{base_url}/v1/health");
                let mut req = client.get(&url);
                if let Some(token) = bearer.as_deref() {
                    req = req.bearer_auth(token);
                }
                let resp = req.send().await.with_context(|| format!("GET {url}"))?;
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                Ok((status, body))
            }
            Inner::Unix { path } => {
                let (status, bytes) = uds_request(path, "GET", "/v1/health", &[], None).await?;
                let body = String::from_utf8_lossy(&bytes).into_owned();
                Ok((status, body))
            }
        }
    }
}

fn parse_rpc_response(status: u16, bytes: &[u8]) -> Result<IpcResponse> {
    if status == 200 {
        let resp: IpcResponse =
            serde_json::from_slice(bytes).context("decoding /v1/rpc response as JSON")?;
        Ok(resp)
    } else if status == 502 {
        // Orchestrator returned a structured error.
        let resp: IpcResponse =
            serde_json::from_slice(bytes).context("decoding /v1/rpc error as JSON")?;
        Ok(resp)
    } else {
        let body = String::from_utf8_lossy(bytes);
        bail!("remote returned HTTP {status}: {body}")
    }
}

/// Open an HTTP request over a Unix socket and return the response
/// body as an incremental byte stream. Unlike [`uds_request`], which
/// `.collect()`s the whole body before returning, this is safe for
/// long-running responses like SSE.
async fn uds_stream_request(
    path: &Path,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
) -> Result<(
    u16,
    std::pin::Pin<Box<dyn Stream<Item = Result<Bytes, anyhow::Error>> + Send>>,
)> {
    let stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("connecting to unix socket {}", path.display()))?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("hyper handshake on unix socket")?;
    tokio::spawn(async move {
        if let Err(err) = conn.await {
            tracing::debug!(%err, "unix-socket connection closed");
        }
    });
    let mut builder = HyperRequest::builder()
        .method(method)
        .uri(target)
        .header("host", "localhost");
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    let req = builder
        .body(Full::<HyperBytes>::from(Vec::new()))
        .context("building hyper request")?;
    let resp = sender
        .send_request(req)
        .await
        .context("sending hyper request on unix socket")?;
    let status = resp.status().as_u16();
    let body = resp.into_body();
    let frame_stream = futures::stream::unfold(body, |mut body| async move {
        loop {
            match body.frame().await {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(bytes) => return Some((Ok(bytes), body)),
                    // Trailers frame — skip and read the next one.
                    Err(_non_data_frame) => continue,
                },
                Some(Err(e)) => {
                    return Some((Err(anyhow!("unix sse frame: {e}")), body));
                }
                None => return None,
            }
        }
    });
    Ok((status, Box::pin(frame_stream)))
}

/// Minimal percent-encoder for query values. Escapes every character
/// outside the set we know is safe in a URL query — over-escaping a
/// handful of punctuation is cheaper than pulling in a dependency.
fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

async fn uds_request(
    path: &Path,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: Option<Vec<u8>>,
) -> Result<(u16, HyperBytes)> {
    let stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("connecting to unix socket {}", path.display()))?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("hyper handshake on unix socket")?;
    tokio::spawn(async move {
        if let Err(err) = conn.await {
            tracing::debug!(%err, "unix-socket connection closed");
        }
    });
    let mut builder = HyperRequest::builder()
        .method(method)
        .uri(target)
        .header("host", "localhost");
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    let req = builder
        .body(Full::<HyperBytes>::from(body.unwrap_or_default()))
        .context("building hyper request")?;
    let resp = sender
        .send_request(req)
        .await
        .context("sending hyper request on unix socket")?;
    let status: HyperStatus = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .context("reading response body from unix socket")?
        .to_bytes();
    Ok((status.as_u16(), bytes))
}

fn parse_transport(s: &str) -> Result<Transport> {
    match s {
        "https" => Ok(Transport::Https),
        "unix" => Ok(Transport::Unix),
        other => bail!("unknown transport `{other}` stored in DB"),
    }
}
