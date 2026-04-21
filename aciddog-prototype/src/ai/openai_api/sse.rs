//! Minimal SSE → OpenAI chunk parser. The OpenAI flavour of SSE omits
//! the `event:` field that Anthropic uses; every frame is just one or
//! more `data:` lines terminated by a blank line. A final
//! `data: [DONE]` frame signals end-of-stream without a JSON payload.
//!
//! Scope, like `anthropic_api::sse`: framing + JSON parsing only. The
//! caller maps each `Data(Value)` into assistant-text / tool-call
//! accumulator state.

use serde_json::Value;

/// One parsed SSE event. `[DONE]` becomes `Done`; every other frame
/// becomes `Data(value)` where `value` is the parsed JSON payload of
/// the concatenated `data:` lines.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenAiSseEvent {
    Data(Value),
    Done,
}

/// Streaming SSE decoder. Feed it byte chunks as they arrive from the
/// HTTP body; each call returns the events that became complete during
/// that feed. Events that span chunk boundaries are held in the
/// internal buffer until the terminating blank line arrives.
#[derive(Default)]
pub struct OpenAiSseDecoder {
    buf: Vec<u8>,
}

impl OpenAiSseDecoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Result<OpenAiSseEvent, SseError>> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();

        loop {
            let Some(end) = find_event_boundary(&self.buf) else {
                break;
            };
            let frame: Vec<u8> = self.buf.drain(..end.end).collect();
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
    body_len: usize,
    end: usize,
}

/// Find the end of the next SSE event. SSE terminates on `\n\n` or
/// `\r\n\r\n`. Ported verbatim from `anthropic_api::sse::find_event_boundary`.
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
        (Some(p), _) => Some(EventEnd { body_len: p, end: p + 2 }),
        (None, Some(p)) => Some(EventEnd { body_len: p, end: p + 4 }),
        (None, None) => None,
    }
}

fn parse_event(body: &[u8]) -> Result<Option<OpenAiSseEvent>, SseError> {
    let text = std::str::from_utf8(body).map_err(SseError::Utf8)?;
    let mut data_lines: Vec<&str> = Vec::new();

    for raw_line in text.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        // Comment lines (including OpenRouter's `: OPENROUTER PROCESSING`
        // keep-alives) are per spec ignored.
        if line.starts_with(':') {
            continue;
        }
        if let Some((field, value)) = split_field(line) {
            match field {
                "data" => data_lines.push(value),
                // OpenAI flavour never emits event/id/retry on the
                // chat-completions stream, but tolerate them.
                "event" | "id" | "retry" => {}
                _ => {}
            }
        }
    }

    if data_lines.is_empty() {
        return Ok(None);
    }

    let joined = data_lines.join("\n");
    if joined.trim() == "[DONE]" {
        return Ok(Some(OpenAiSseEvent::Done));
    }

    let value: Value = serde_json::from_str(&joined).map_err(SseError::Json)?;
    Ok(Some(OpenAiSseEvent::Data(value)))
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
    fn parses_one_data_event() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b"data: {\"x\":1}\n\n");
        assert_eq!(evs.len(), 1);
        match evs[0].as_ref().unwrap() {
            OpenAiSseEvent::Data(v) => assert_eq!(v, &json!({"x": 1})),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn done_sentinel_produces_done_event() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b"data: [DONE]\n\n");
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0].as_ref().unwrap(), OpenAiSseEvent::Done));
    }

    #[test]
    fn handles_split_chunks_mid_json() {
        let mut d = OpenAiSseDecoder::new();
        let first = d.feed(b"data: {\"tex");
        assert!(first.is_empty());
        let second = d.feed(b"t\":\"hi\"}\n\n");
        assert_eq!(second.len(), 1);
        match second[0].as_ref().unwrap() {
            OpenAiSseEvent::Data(v) => assert_eq!(v, &json!({"text": "hi"})),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn handles_split_chunks_mid_done() {
        let mut d = OpenAiSseDecoder::new();
        let first = d.feed(b"data: [DO");
        assert!(first.is_empty());
        let second = d.feed(b"NE]\n\n");
        assert_eq!(second.len(), 1);
        assert!(matches!(second[0].as_ref().unwrap(), OpenAiSseEvent::Done));
    }

    #[test]
    fn multiple_events_in_one_feed() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b"data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n");
        assert_eq!(evs.len(), 3);
        assert!(matches!(evs[0].as_ref().unwrap(), OpenAiSseEvent::Data(_)));
        assert!(matches!(evs[1].as_ref().unwrap(), OpenAiSseEvent::Data(_)));
        assert!(matches!(evs[2].as_ref().unwrap(), OpenAiSseEvent::Done));
    }

    #[test]
    fn crlf_terminator_works() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b"data: {\"x\":7}\r\n\r\n");
        assert_eq!(evs.len(), 1);
        match evs[0].as_ref().unwrap() {
            OpenAiSseEvent::Data(v) => assert_eq!(v, &json!({"x": 7})),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn keep_alive_comments_are_ignored() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b": OPENROUTER PROCESSING\n\n");
        // Comment-only frame yields no events.
        assert!(evs.is_empty());
    }

    #[test]
    fn keep_alive_before_data_does_not_break_parse() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b": OPENROUTER PROCESSING\ndata: {\"ok\":true}\n\n");
        assert_eq!(evs.len(), 1);
        match evs[0].as_ref().unwrap() {
            OpenAiSseEvent::Data(v) => assert_eq!(v, &json!({"ok": true})),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn empty_choices_array_parses_as_data() {
        // OpenRouter sometimes emits a leading usage/metadata chunk
        // with `choices: []` — decoder just hands it to the caller,
        // which is responsible for skipping it.
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b"data: {\"choices\":[]}\n\n");
        assert_eq!(evs.len(), 1);
        match evs[0].as_ref().unwrap() {
            OpenAiSseEvent::Data(v) => {
                assert_eq!(v.pointer("/choices").and_then(Value::as_array).map(Vec::len), Some(0));
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn multi_line_data_joins_and_fails_on_invalid_json() {
        let mut d = OpenAiSseDecoder::new();
        let evs = d.feed(b"data: not\ndata: json\n\n");
        assert_eq!(evs.len(), 1);
        assert!(evs[0].is_err());
    }
}
