//! Transport layer for the `:remote` automaton.
//!
//! Handles two concrete transports:
//! * HTTPS — via `reqwest` with rustls, optional custom root CA, and a
//!   bearer-token `Authorization` header resolved from an env var.
//! * Unix socket — via `hyper` + `hyper-util` (`reqwest` does not
//!   support UDS). Each RPC dials a fresh `UnixStream`; SSE holds the
//!   stream open for the life of the subscription.
//!
//! Both variants expose a single `send_rpc` entry point and a
//! `sse_response` entry point that returns an async byte stream.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use futures::Stream;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request as HyperRequest, StatusCode as HyperStatus};
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

use crate::agent_ipc::{RequestCmd, Response as IpcResponse};
use crate::config::RemoteTransport;

/// A ready-to-use client that owns whatever handle the transport
/// needs (a `reqwest::Client` for HTTPS, or just the configured socket
/// path for UDS). Cloning is cheap; both variants are `Arc`-backed.
#[derive(Clone)]
pub struct Client {
    inner: Inner,
}

#[derive(Clone)]
enum Inner {
    Https {
        client: reqwest::Client,
        base_url: String,
        bearer: Arc<String>,
    },
    Unix {
        path: Arc<PathBuf>,
    },
}

impl Client {
    /// Build a client from a [`RemoteTransport`]. Resolves the bearer
    /// token env var and loads the optional CA bundle eagerly so a
    /// misconfigured automaton fails fast at startup rather than on
    /// first RPC.
    pub fn build(transport: &RemoteTransport) -> Result<Self> {
        match transport {
            RemoteTransport::Https {
                url,
                api_key_env,
                tls_ca,
            } => {
                let token = std::env::var(api_key_env).map_err(|_| {
                    anyhow!("api_key_env `{api_key_env}` is unset or empty; refusing to connect")
                })?;
                if token.is_empty() {
                    bail!("api_key_env `{api_key_env}` is empty");
                }
                let mut builder = reqwest::Client::builder();
                if let Some(ca_path) = tls_ca {
                    let pem = std::fs::read(ca_path).with_context(|| {
                        format!("reading tls_ca file {}", ca_path.display())
                    })?;
                    let cert = reqwest::Certificate::from_pem(&pem).with_context(|| {
                        format!("parsing tls_ca PEM at {}", ca_path.display())
                    })?;
                    builder = builder.add_root_certificate(cert);
                }
                let client = builder
                    .build()
                    .context("building reqwest client for :remote automaton")?;
                Ok(Client {
                    inner: Inner::Https {
                        client,
                        base_url: url.trim_end_matches('/').to_string(),
                        bearer: Arc::new(token),
                    },
                })
            }
            RemoteTransport::UnixSocket { path } => Ok(Client {
                inner: Inner::Unix {
                    path: Arc::new(path.clone()),
                },
            }),
        }
    }

    /// POST a single `RequestCmd` to `/v1/rpc` and return the parsed
    /// reply. On non-2xx responses, returns an `err` response
    /// containing the body text.
    pub async fn send_rpc(&self, cmd: &RequestCmd) -> Result<IpcResponse> {
        let body = serde_json::to_vec(cmd).context("serializing RequestCmd")?;
        match &self.inner {
            Inner::Https {
                client,
                base_url,
                bearer,
            } => {
                let url = format!("{base_url}/v1/rpc");
                let resp = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .bearer_auth(bearer.as_str())
                    .body(body)
                    .send()
                    .await
                    .with_context(|| format!("POST {url}"))?;
                let status = resp.status();
                let bytes = resp.bytes().await.context("reading /v1/rpc response body")?;
                parse_rpc_response(status.as_u16(), &bytes)
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

    /// Open a long-lived SSE subscription against `/v1/events`.
    /// Returns a stream of byte chunks — the caller is expected to run
    /// these through [`crate::remote_automaton::sse::parse_frames`].
    pub async fn sse_stream(
        &self,
    ) -> Result<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send + Unpin>> {
        match &self.inner {
            Inner::Https {
                client,
                base_url,
                bearer,
            } => {
                let url = format!("{base_url}/v1/events");
                let resp = client
                    .get(&url)
                    .header("accept", "text/event-stream")
                    .bearer_auth(bearer.as_str())
                    .send()
                    .await
                    .with_context(|| format!("GET {url}"))?;
                if !resp.status().is_success() {
                    bail!("SSE subscription failed: HTTP {}", resp.status());
                }
                // reqwest's bytes_stream returns reqwest::Error; map to io.
                let stream = futures::StreamExt::map(resp.bytes_stream(), |r| {
                    r.map_err(|e| std::io::Error::other(e.to_string()))
                });
                Ok(Box::new(Box::pin(stream)))
            }
            Inner::Unix { path } => {
                let stream = uds_sse_stream(path).await?;
                Ok(Box::new(stream))
            }
        }
    }
}

/// Short human-readable description of a configured transport, used
/// in pane log lines.
pub fn describe_transport(transport: &RemoteTransport) -> String {
    match transport {
        RemoteTransport::Https { url, .. } => url.clone(),
        RemoteTransport::UnixSocket { path } => format!("unix:{}", path.display()),
    }
}

fn parse_rpc_response(status: u16, bytes: &[u8]) -> Result<IpcResponse> {
    // Both the success and non-success code paths in the server
    // return a JSON `Response` body (see `src/webservice/http.rs`).
    // Try to parse regardless — surface anything that isn't JSON as
    // an anyhow error.
    if !(200..300).contains(&status) && status != 502 {
        // 401/403/404/503 etc. — surface as anyhow so the caller
        // treats them as transport-level failures, not as RPC errors.
        let text = String::from_utf8_lossy(bytes);
        bail!("HTTP {status}: {text}");
    }
    serde_json::from_slice::<IpcResponse>(bytes)
        .with_context(|| format!("parsing /v1/rpc response (HTTP {status})"))
}

/// Issue a single HTTP/1.1 request over a freshly-dialled UnixStream.
async fn uds_request(
    path: &Path,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: Option<Vec<u8>>,
) -> Result<(u16, Bytes)> {
    let stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("connecting to remote UDS {}", path.display()))?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .context("hyper http1 handshake")?;
    // Drive the connection in the background; it completes when the
    // server closes (or this task is aborted).
    let conn_handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut req_builder = HyperRequest::builder()
        .method(method)
        .uri(target)
        .header("host", "aciddog.local");
    for (k, v) in headers {
        req_builder = req_builder.header(*k, *v);
    }
    let body = match body {
        Some(b) => Full::new(Bytes::from(b)),
        None => Full::new(Bytes::new()),
    };
    let req = req_builder.body(body).context("building HTTP request")?;

    let resp = sender
        .send_request(req)
        .await
        .context("sending HTTP request over UDS")?;
    let status = resp.status();
    let collected = resp
        .into_body()
        .collect()
        .await
        .context("reading UDS response body")?;
    let bytes = collected.to_bytes();
    conn_handle.abort();
    Ok((status.as_u16(), bytes))
}

/// Open an SSE subscription over a UnixStream and return the response
/// body as a byte stream.
async fn uds_sse_stream(
    path: &Path,
) -> Result<impl Stream<Item = std::io::Result<Bytes>> + Send + Unpin> {
    let stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("connecting to remote UDS {}", path.display()))?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .context("hyper http1 handshake")?;
    // Keep the connection alive for the life of the SSE stream. The
    // handle is abandoned — aborting the outer task drops the reader
    // which closes the stream.
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = HyperRequest::builder()
        .method("GET")
        .uri("/v1/events")
        .header("host", "aciddog.local")
        .header("accept", "text/event-stream")
        .body(Full::new(Bytes::new()))
        .context("building SSE request")?;
    let resp = sender
        .send_request(req)
        .await
        .context("sending SSE request over UDS")?;
    if resp.status() != HyperStatus::OK {
        bail!("SSE subscription failed: HTTP {}", resp.status());
    }
    Ok(incoming_to_stream(resp.into_body()))
}

fn incoming_to_stream(body: Incoming) -> impl Stream<Item = std::io::Result<Bytes>> + Send + Unpin {
    use http_body_util::BodyStream;
    let s = BodyStream::new(body);
    let s = futures::StreamExt::filter_map(s, |frame_res| async move {
        match frame_res {
            Ok(frame) => match frame.into_data() {
                Ok(data) => Some(Ok(data)),
                Err(_trailers) => None,
            },
            Err(e) => Some(Err(std::io::Error::other(e.to_string()))),
        }
    });
    Box::pin(s)
}
