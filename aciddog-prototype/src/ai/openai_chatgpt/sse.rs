//! SSE decoder for the OpenAI Responses API used by the
//! ChatGPT-subscription endpoint. Each event is a JSON object with a
//! `type` discriminator — the interesting ones are listed in
//! `ResponsesEvent::parse`.
//!
//! Frames are separated by a blank line (`\n\n` or `\r\n\r\n`); the
//! content is always `data: <json>` (the `event:` field is not used
//! by Codex responses).

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ResponsesEvent {
    pub event_type: String,
    pub raw: Value,
}

/// Buffered SSE decoder. Feed bytes in via `push`; drain events via
/// repeated calls to `next_event`.
pub struct ResponsesSseDecoder {
    buf: Vec<u8>,
}

impl Default for ResponsesSseDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponsesSseDecoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// Pull the next complete event out of the buffer, or `None` if
    /// we haven't yet seen a full `\n\n` / `\r\n\r\n` terminator.
    /// Returns `Some(Err)` if a frame arrived that isn't valid JSON
    /// so callers can surface it as an `AIEvent::Error` and bail.
    pub fn next_event(&mut self) -> Option<Result<ResponsesEvent, String>> {
        let term_rnrn = self
            .buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| (i, 4));
        let term_nn = self
            .buf
            .windows(2)
            .position(|w| w == b"\n\n")
            .map(|i| (i, 2));
        let (pos, len) = match (term_rnrn, term_nn) {
            (Some(a), Some(b)) => {
                if a.0 < b.0 {
                    a
                } else {
                    b
                }
            }
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => return None,
        };
        let frame = self.buf[..pos].to_vec();
        self.buf.drain(..pos + len);
        let text = match std::str::from_utf8(&frame) {
            Ok(s) => s,
            Err(_) => return Some(Err("sse frame is not utf-8".into())),
        };

        // Concatenate all `data:` lines — the spec allows multiple.
        let mut data = String::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.trim_start_matches(' '));
            }
        }
        if data.is_empty() || data == "[DONE]" {
            // Skip non-data frames (comments, `event:` lines, keep-alives).
            return self.next_event();
        }
        match serde_json::from_str::<Value>(&data) {
            Ok(v) => {
                let event_type = v
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                Some(Ok(ResponsesEvent { event_type, raw: v }))
            }
            Err(e) => Some(Err(format!(
                "failed to parse sse frame as JSON: {e}; raw={data}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_single_event() {
        let mut d = ResponsesSseDecoder::new();
        d.push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n");
        let ev = d.next_event().unwrap().unwrap();
        assert_eq!(ev.event_type, "response.output_text.delta");
        assert_eq!(ev.raw["delta"], "hi");
        assert!(d.next_event().is_none());
    }

    #[test]
    fn decodes_two_events_in_one_buffer() {
        let mut d = ResponsesSseDecoder::new();
        d.push(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n\
              data: {\"type\":\"response.output_text.delta\",\"delta\":\"there\"}\n\n",
        );
        let a = d.next_event().unwrap().unwrap();
        let b = d.next_event().unwrap().unwrap();
        assert_eq!(a.raw["delta"], "hi");
        assert_eq!(b.raw["delta"], "there");
    }

    #[test]
    fn handles_chunked_frames() {
        let mut d = ResponsesSseDecoder::new();
        d.push(b"data: {\"type\":\"response.output_text");
        assert!(d.next_event().is_none());
        d.push(b".delta\",\"delta\":\"hi\"}\n\n");
        let ev = d.next_event().unwrap().unwrap();
        assert_eq!(ev.event_type, "response.output_text.delta");
    }

    #[test]
    fn skips_done_marker() {
        let mut d = ResponsesSseDecoder::new();
        d.push(b"data: [DONE]\n\n");
        assert!(d.next_event().is_none());
    }

    #[test]
    fn skips_comment_only_frames() {
        let mut d = ResponsesSseDecoder::new();
        d.push(b": keep-alive\n\ndata: {\"type\":\"x\"}\n\n");
        let ev = d.next_event().unwrap().unwrap();
        assert_eq!(ev.event_type, "x");
    }
}
