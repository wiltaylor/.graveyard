//! Minimal SSE → Anthropic event parser. Given the raw byte stream
//! from an `/v1/messages?stream=true` response, yields a typed event
//! sequence that the core can fold into conversation state without
//! pulling in `eventsource-client`.
//!
//! Scope: this module handles framing and JSON parsing only. Mapping
//! events to `AIEvent` and to `ContentBlock`s lives in the core — this
//! keeps the parser side-effect-free and trivially unit-testable.

use serde_json::Value;

/// One parsed SSE event. `data` is the concatenated `data:` payload
/// already parsed as JSON (or `Null` if the server sent a bare event
/// with no data — valid per the SSE spec but unusual from Anthropic).
#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: String,
    pub data: Value,
}

/// Streaming SSE decoder. Feed it byte chunks as they arrive from the
/// HTTP body; each call returns the events that became complete during
/// that feed. Events that span chunk boundaries are held in the
/// internal buffer until the terminating blank line arrives.
#[derive(Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append a chunk and return any newly completed events. Parse
    /// errors on individual events are surfaced as `Err`; callers may
    /// choose to continue decoding or propagate upward.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Result<SseEvent, SseError>> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();

        loop {
            let Some(end) = find_event_boundary(&self.buf) else {
                break;
            };
            let frame: Vec<u8> = self.buf.drain(..end.end).collect();
            // `frame` ends with the blank-line terminator; slice it
            // off so the parser only sees the event body.
            let body = &frame[..end.body_len];
            match parse_event(body) {
                Ok(Some(ev)) => out.push(Ok(ev)),
                Ok(None) => {}
                Err(e) => out.push(Err(e)),
            }
        }

        out
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SseError {
    #[error("sse: invalid utf-8 in event body: {0}")]
    Utf8(std::str::Utf8Error),

    #[error("sse: invalid json in `data` field: {0}")]
    Json(serde_json::Error),
}

struct EventEnd {
    /// Length of the event body (excluding the trailing terminator).
    body_len: usize,
    /// Offset into the buffer just past the terminator.
    end: usize,
}

/// Find the end of the next SSE event in `buf`, returning both the
/// body length (for parsing) and the trailing-terminator-inclusive
/// offset (for draining). SSE events terminate on `\n\n` or `\r\n\r\n`.
fn find_event_boundary(buf: &[u8]) -> Option<EventEnd> {
    let mut lf_lf = None;
    let mut crlf_crlf = None;
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            lf_lf = Some(i);
            break;
        }
        if i + 3 < buf.len()
            && buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            crlf_crlf = Some(i);
            break;
        }
        i += 1;
    }
    match (lf_lf, crlf_crlf) {
        (Some(p), _) => Some(EventEnd {
            body_len: p,
            end: p + 2,
        }),
        (None, Some(p)) => Some(EventEnd {
            body_len: p,
            end: p + 4,
        }),
        (None, None) => None,
    }
}

fn parse_event(body: &[u8]) -> Result<Option<SseEvent>, SseError> {
    let text = std::str::from_utf8(body).map_err(SseError::Utf8)?;
    let mut event_name: Option<String> = None;
    let mut data_lines: Vec<&str> = Vec::new();

    for raw_line in text.split('\n') {
        // Trim trailing CR from \r\n endings; per SSE spec leading
        // colon marks a comment line that we simply ignore.
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if let Some(stripped) = line.strip_prefix(':') {
            let _ = stripped; // comment, skip
            continue;
        }
        if let Some((field, value)) = split_field(line) {
            match field {
                "event" => event_name = Some(value.to_string()),
                "data" => data_lines.push(value),
                "id" | "retry" => {} // irrelevant for our use
                _ => {}
            }
        }
    }

    let Some(name) = event_name else {
        // No `event:` field — not a message we care about.
        return Ok(None);
    };

    let data = if data_lines.is_empty() {
        Value::Null
    } else {
        let joined = data_lines.join("\n");
        serde_json::from_str(&joined).map_err(SseError::Json)?
    };

    Ok(Some(SseEvent { event: name, data }))
}

/// Split an SSE line into `(field, value)`. The value has one optional
/// leading space stripped per the spec.
fn split_field(line: &str) -> Option<(&str, &str)> {
    let colon = line.find(':')?;
    let field = &line[..colon];
    let rest = &line[colon + 1..];
    let value = rest.strip_prefix(' ').unwrap_or(rest);
    Some((field, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_one_event() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"event: message_start\ndata: {\"x\":1}\n\n");
        assert_eq!(evs.len(), 1);
        let ev = evs[0].as_ref().unwrap();
        assert_eq!(ev.event, "message_start");
        assert_eq!(ev.data, json!({"x": 1}));
    }

    #[test]
    fn handles_split_chunks() {
        let mut d = SseDecoder::new();
        let first = d.feed(b"event: content_block_delta\ndata: {\"tex");
        assert!(first.is_empty());
        let second = d.feed(b"t\":\"hi\"}\n\n");
        assert_eq!(second.len(), 1);
        let ev = second[0].as_ref().unwrap();
        assert_eq!(ev.event, "content_block_delta");
        assert_eq!(ev.data, json!({"text": "hi"}));
    }

    #[test]
    fn multiple_events_in_one_feed() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"event: a\ndata: 1\n\nevent: b\ndata: 2\n\nevent: c\ndata: 3\n\n");
        assert_eq!(evs.len(), 3);
        assert_eq!(evs[0].as_ref().unwrap().event, "a");
        assert_eq!(evs[1].as_ref().unwrap().event, "b");
        assert_eq!(evs[2].as_ref().unwrap().event, "c");
    }

    #[test]
    fn crlf_terminator_works() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"event: x\r\ndata: 7\r\n\r\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].as_ref().unwrap().event, "x");
    }

    #[test]
    fn multi_line_data_joins_with_newline() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"event: m\ndata: line1\ndata: line2\n\n");
        assert_eq!(evs.len(), 1);
        // The joined "line1\nline2" isn't valid JSON — expect a parse
        // error reported in-line, not a panic.
        assert!(evs[0].is_err());
    }

    #[test]
    fn ignores_comments_and_unknown_fields() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b": keep-alive\nevent: ping\ndata: {}\nid: 42\nretry: 1000\n\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].as_ref().unwrap().event, "ping");
    }

    #[test]
    fn event_with_no_data_field_yields_null() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"event: ping\n\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].as_ref().unwrap().data, Value::Null);
    }
}
