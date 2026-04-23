//! SSE subscriber for the `:remote` automaton.
//!
//! Drives a long-lived connection to `/v1/events`, parses `data:` JSON
//! frames into [`InboundEvent`] values, and handles reconnection with
//! exponential backoff. Lagged frames from the server are logged as
//! stderr; keepalive frames are dropped silently.

use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::warn;

use super::http_client::Client;
use crate::ai::event::AIEvent;
use crate::webservice::InboundEvent;

/// Backoff schedule. Monotonic, capped at 60s. Reset to the first
/// entry after any successful connection.
const BACKOFF_MS: &[u64] = &[100, 250, 500, 1000, 2000, 5000, 10000, 30000, 60000];

/// Outcome returned by a single subscribe-and-read cycle, driving the
/// reconnect loop's next action.
enum CycleOutcome {
    /// Connection was opened successfully and then dropped (EOF or
    /// read error). Reset backoff.
    Reset,
    /// Transport / handshake failure — advance backoff.
    Retry,
    /// Fatal (e.g. 401/403). Stop retrying.
    Fatal(String),
    /// External cancel.
    Cancelled,
}

pub async fn run_sse_loop(
    idx: usize,
    automaton_id: String,
    client: Client,
    out_tx: mpsc::Sender<InboundEvent>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
    mut cancel_rx: mpsc::Receiver<()>,
) {
    let mut attempt: usize = 0;
    loop {
        tokio::select! {
            biased;
            _ = cancel_rx.recv() => return,
            outcome = subscribe_and_read(&automaton_id, &client, &out_tx, idx, &ai_tx) => {
                match outcome {
                    CycleOutcome::Reset => {
                        attempt = 0;
                        let _ = ai_tx
                            .send((
                                idx,
                                AIEvent::AutomatonStderr {
                                    line: format!(
                                        "remote `{automaton_id}` SSE dropped; reconnecting"
                                    ),
                                },
                            ))
                            .await;
                    }
                    CycleOutcome::Retry => {
                        let delay = backoff_delay(attempt);
                        attempt = attempt.saturating_add(1);
                        let _ = ai_tx
                            .send((
                                idx,
                                AIEvent::AutomatonStderr {
                                    line: format!(
                                        "remote `{automaton_id}` SSE connect failed; retrying in {}ms",
                                        delay.as_millis()
                                    ),
                                },
                            ))
                            .await;
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = cancel_rx.recv() => return,
                        }
                    }
                    CycleOutcome::Fatal(reason) => {
                        let _ = ai_tx
                            .send((
                                idx,
                                AIEvent::AutomatonStderr {
                                    line: format!(
                                        "remote `{automaton_id}` SSE fatal: {reason}; stopping reconnect"
                                    ),
                                },
                            ))
                            .await;
                        return;
                    }
                    CycleOutcome::Cancelled => return,
                }
            }
        }
    }
}

async fn subscribe_and_read(
    automaton_id: &str,
    client: &Client,
    out_tx: &mpsc::Sender<InboundEvent>,
    idx: usize,
    ai_tx: &mpsc::Sender<(usize, AIEvent)>,
) -> CycleOutcome {
    let mut stream = match client.sse_stream().await {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("HTTP 401") || msg.contains("HTTP 403") {
                return CycleOutcome::Fatal(msg);
            }
            return CycleOutcome::Retry;
        }
    };

    let _ = ai_tx
        .send((
            idx,
            AIEvent::AutomatonStdout {
                line: format!("remote `{automaton_id}` SSE connected"),
            },
        ))
        .await;

    let mut parser = FrameParser::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                for frame in parser.feed(&bytes) {
                    match frame {
                        Frame::Data(payload) => {
                            match serde_json::from_str::<InboundEvent>(&payload) {
                                Ok(ev) => {
                                    if out_tx.send(ev).await.is_err() {
                                        return CycleOutcome::Cancelled;
                                    }
                                }
                                Err(e) => {
                                    // Tolerate unknown payloads — the
                                    // server may add new event kinds.
                                    let _ = ai_tx
                                        .send((
                                            idx,
                                            AIEvent::AutomatonStderr {
                                                line: format!(
                                                    "remote `{automaton_id}` SSE parse skip: {e}: {payload}"
                                                ),
                                            },
                                        ))
                                        .await;
                                }
                            }
                        }
                        Frame::Lagged(n) => {
                            let _ = ai_tx
                                .send((
                                    idx,
                                    AIEvent::AutomatonStderr {
                                        line: format!(
                                            "remote `{automaton_id}` SSE lagged: dropped {n}"
                                        ),
                                    },
                                ))
                                .await;
                        }
                        Frame::Keepalive | Frame::Comment => {}
                    }
                }
            }
            Err(e) => {
                warn!(automaton = %automaton_id, error = %e, "sse read error");
                return CycleOutcome::Reset;
            }
        }
    }
    CycleOutcome::Reset
}

fn backoff_delay(attempt: usize) -> Duration {
    let idx = attempt.min(BACKOFF_MS.len() - 1);
    Duration::from_millis(BACKOFF_MS[idx])
}

/// Minimal parser for the subset of SSE we need: `data:` lines
/// (optionally following `event:` metadata), concatenated within a
/// single event, terminated by a blank line. Comments (`:` lines) and
/// unrecognised field names are skipped. The server's `keepalive`
/// frames use `data: keepalive` — we detect that by payload content.
#[derive(Debug)]
struct FrameParser {
    buffer: String,
    data_lines: Vec<String>,
    event_name: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Frame {
    Data(String),
    Lagged(u64),
    Keepalive,
    Comment,
}

impl FrameParser {
    fn new() -> Self {
        FrameParser {
            buffer: String::new(),
            data_lines: Vec::new(),
            event_name: None,
        }
    }

    /// Append a chunk of raw bytes; return any fully-formed frames.
    fn feed(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        let mut frames = Vec::new();
        while let Some(pos) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=pos).collect();
            let line = line.trim_end_matches('\n').trim_end_matches('\r');
            if line.is_empty() {
                // Dispatch the accumulated event.
                if let Some(frame) = self.dispatch() {
                    frames.push(frame);
                }
                continue;
            }
            if let Some(comment) = line.strip_prefix(':') {
                let _ = comment;
                frames.push(Frame::Comment);
                continue;
            }
            if let Some((field, value)) = split_field(line) {
                match field {
                    "data" => self.data_lines.push(value.to_string()),
                    "event" => self.event_name = Some(value.to_string()),
                    // Ignore `id:` and `retry:` — we don't implement
                    // client-side resume.
                    _ => {}
                }
            }
        }
        frames
    }

    fn dispatch(&mut self) -> Option<Frame> {
        let data_lines = std::mem::take(&mut self.data_lines);
        let event_name = self.event_name.take();
        if data_lines.is_empty() {
            return None;
        }
        let payload = data_lines.join("\n");
        if payload == "keepalive" {
            return Some(Frame::Keepalive);
        }
        if event_name.as_deref() == Some("lagged") {
            #[derive(serde::Deserialize)]
            struct Lagged {
                dropped: u64,
            }
            if let Ok(l) = serde_json::from_str::<Lagged>(&payload) {
                return Some(Frame::Lagged(l.dropped));
            }
        }
        Some(Frame::Data(payload))
    }
}

fn split_field(line: &str) -> Option<(&str, &str)> {
    let (field, rest) = line.split_once(':')?;
    let value = rest.strip_prefix(' ').unwrap_or(rest);
    Some((field, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(parser: &mut FrameParser, input: &str) -> Vec<Frame> {
        parser.feed(input.as_bytes())
    }

    #[test]
    fn parses_single_data_frame() {
        let mut p = FrameParser::new();
        let frames = run(
            &mut p,
            "data: {\"kind\":\"user_message\",\"body\":\"hi\",\"ts\":\"t\"}\n\n",
        );
        assert_eq!(frames.len(), 1);
        match &frames[0] {
            Frame::Data(d) => assert!(d.contains("user_message")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn handles_chunk_boundary_mid_line() {
        let mut p = FrameParser::new();
        let a = run(&mut p, "data: {\"kind\":\"user_");
        assert!(a.is_empty());
        let b = run(&mut p, "message\",\"body\":\"hi\",\"ts\":\"t\"}\n\n");
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn identifies_keepalive() {
        let mut p = FrameParser::new();
        let frames = run(&mut p, "data: keepalive\n\n");
        assert_eq!(frames, vec![Frame::Keepalive]);
    }

    #[test]
    fn identifies_lagged_event() {
        let mut p = FrameParser::new();
        let frames = run(
            &mut p,
            "event: lagged\ndata: {\"kind\":\"lagged\",\"dropped\":42}\n\n",
        );
        assert_eq!(frames, vec![Frame::Lagged(42)]);
    }

    #[test]
    fn backoff_monotonic_and_capped() {
        let mut last = Duration::from_millis(0);
        for attempt in 0..20 {
            let d = backoff_delay(attempt);
            assert!(d >= last, "attempt {attempt}: {d:?} < {last:?}");
            assert!(d <= Duration::from_secs(60));
            last = d;
        }
    }

    #[test]
    fn skips_comments() {
        let mut p = FrameParser::new();
        let frames = run(&mut p, ":hello\ndata: x\n\n");
        assert!(frames.iter().any(|f| matches!(f, Frame::Comment)));
        assert!(frames.iter().any(|f| matches!(f, Frame::Data(_))));
    }
}
