//! Parse `<aciddog-call>...</aciddog-call>` JSON tags out of assistant
//! text and render `<aciddog-response index="N">...</aciddog-response>`
//! blocks to feed back into the next turn.
//!
//! Used only by agents whose core has no shell tool available — see
//! `ClaudeCodeCore::turn` for the chain loop that drives this.

use crate::agent_ipc::{RequestCmd, Response};

/// One parsed `<aciddog-call>` tag. `parsed` is `Err` when the body is
/// not valid JSON or doesn't deserialize into a known `RequestCmd` — in
/// that case the dispatcher synthesizes an error `Response` rather than
/// aborting the chain, so the model can self-correct.
#[derive(Debug, Clone)]
pub struct TagCall {
    pub parsed: Result<RequestCmd, String>,
    pub raw_body: String,
}

/// Scan `text` for `<aciddog-call>...</aciddog-call>` blocks in
/// document order. Tags inside triple-backtick fenced code blocks are
/// ignored so the model can quote examples in its replies without
/// accidentally invoking them.
pub fn extract_calls(text: &str) -> Vec<TagCall> {
    const OPEN: &str = "<aciddog-call>";
    const CLOSE: &str = "</aciddog-call>";

    let fences = fenced_ranges(text);
    let in_fence = |pos: usize| fences.iter().any(|(a, b)| *a <= pos && pos < *b);

    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < text.len() {
        let Some(open_rel) = text[cursor..].find(OPEN) else {
            break;
        };
        let open_start = cursor + open_rel;
        if in_fence(open_start) {
            cursor = open_start + OPEN.len();
            continue;
        }
        let body_start = open_start + OPEN.len();
        let Some(close_rel) = text[body_start..].find(CLOSE) else {
            // Unterminated open tag — stop scanning.
            break;
        };
        let body_end = body_start + close_rel;
        let raw_body = text[body_start..body_end].trim().to_string();

        let parsed = serde_json::from_str::<RequestCmd>(&raw_body)
            .map_err(|e| format!("invalid aciddog-call JSON: {e}"));

        out.push(TagCall { parsed, raw_body });
        cursor = body_end + CLOSE.len();
    }
    out
}

/// Remove `<aciddog-call>…</aciddog-call>` and
/// `<aciddog-response …>…</aciddog-response>` blocks from an assistant
/// text buffer, leaving fenced code blocks untouched so the model can
/// still quote the protocol in documentation.
///
/// Used by the TUI to hide tag XML from the user — the core emits
/// semantic ToolUse / ToolResult events in its place, so the raw
/// markup would just be visual noise next to the dedicated chat
/// block.
pub fn strip_tags(text: &str) -> String {
    let fences = fenced_ranges(text);
    let in_fence = |pos: usize| fences.iter().any(|(a, b)| *a <= pos && pos < *b);

    const CALL_OPEN: &str = "<aciddog-call>";
    const CALL_CLOSE: &str = "</aciddog-call>";
    const RESP_OPEN_PREFIX: &str = "<aciddog-response";
    const RESP_CLOSE: &str = "</aciddog-response>";

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;

    while cursor < text.len() {
        let call_pos = text[cursor..].find(CALL_OPEN).map(|p| cursor + p);
        let resp_pos = text[cursor..].find(RESP_OPEN_PREFIX).map(|p| cursor + p);

        let (open_start, close_tag) = match (call_pos, resp_pos) {
            (None, None) => break,
            (Some(c), None) => (c, CALL_CLOSE),
            (None, Some(r)) => (r, RESP_CLOSE),
            (Some(c), Some(r)) => {
                if c <= r {
                    (c, CALL_CLOSE)
                } else {
                    (r, RESP_CLOSE)
                }
            }
        };

        if in_fence(open_start) {
            // Fenced example — preserve it verbatim. Advance by one
            // character so the next iteration looks past this match.
            let advance = next_char_boundary(text, open_start + 1);
            out.push_str(&text[cursor..advance]);
            cursor = advance;
            continue;
        }

        // Drop the whole tag and its body.
        out.push_str(&text[cursor..open_start]);
        let Some(close_rel) = text[open_start..].find(close_tag) else {
            // Unterminated tag: drop the remainder, matching
            // `extract_calls`'s behavior.
            return out;
        };
        cursor = open_start + close_rel + close_tag.len();
    }

    out.push_str(&text[cursor..]);
    out
}

fn next_char_boundary(text: &str, from: usize) -> usize {
    let mut i = from.min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Render a list of (index, response) pairs as the synthetic user-turn
/// payload for the next round of the tag chain.
pub fn render_responses(responses: &[(usize, Response)]) -> String {
    let mut out = String::new();
    for (i, (idx, resp)) in responses.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        let body = serde_json::to_string(resp).unwrap_or_else(|_| "{}".to_string());
        out.push_str(&format!(
            "<aciddog-response index=\"{idx}\">{body}</aciddog-response>"
        ));
    }
    out
}

/// Return byte ranges (half-open) in `text` that lie inside triple-
/// backtick fenced code blocks. The ranges cover only the *contents*
/// of the fence — the opening/closing fence lines themselves are
/// excluded. Backticks are ASCII so byte offsets are safe to compare
/// against UTF-8 char boundaries for presence checks.
fn fenced_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut i = 0usize;
    let mut fence_start: Option<(usize, usize)> = None;

    while i < bytes.len() {
        let at_line_start = i == 0 || bytes[i - 1] == b'\n';
        if at_line_start {
            let mut j = i;
            while j < bytes.len() && bytes[j] == b'`' {
                j += 1;
            }
            let run = j - i;
            if run >= 3 {
                // Skip to end of fence marker line.
                let mut line_end = j;
                while line_end < bytes.len() && bytes[line_end] != b'\n' {
                    line_end += 1;
                }
                match fence_start {
                    None => {
                        // Content starts after the newline (or at EOF).
                        let content_start = if line_end < bytes.len() {
                            line_end + 1
                        } else {
                            line_end
                        };
                        fence_start = Some((content_start, run));
                        i = content_start;
                        continue;
                    }
                    Some((start, open_len)) if run >= open_len => {
                        // Content ended at the start of this fence line (i).
                        ranges.push((start, i));
                        fence_start = None;
                        i = if line_end < bytes.len() {
                            line_end + 1
                        } else {
                            line_end
                        };
                        continue;
                    }
                    _ => {}
                }
            }
        }
        i += 1;
    }

    // Unterminated fence: everything from the open to EOF is fenced.
    if let Some((start, _)) = fence_start {
        ranges.push((start, bytes.len()));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_ipc::Response;

    #[test]
    fn extracts_a_single_well_formed_call() {
        let text = r#"Thinking out loud.
<aciddog-call>{"cmd":"send_message","to":"coder","body":"hi"}</aciddog-call>
Done."#;
        let calls = extract_calls(text);
        assert_eq!(calls.len(), 1);
        assert!(matches!(
            calls[0].parsed,
            Ok(RequestCmd::SendMessage { ref to, ref body })
                if to == "coder" && body == "hi"
        ));
    }

    #[test]
    fn extracts_multiple_calls_in_order() {
        let text = r#"
<aciddog-call>{"cmd":"test"}</aciddog-call>
mid
<aciddog-call>{"cmd":"worker_create","template":"jokester","task":"dad joke"}</aciddog-call>
"#;
        let calls = extract_calls(text);
        assert_eq!(calls.len(), 2);
        assert!(matches!(calls[0].parsed, Ok(RequestCmd::Test)));
        assert!(matches!(
            calls[1].parsed,
            Ok(RequestCmd::WorkerCreate { ref template, .. }) if template == "jokester"
        ));
    }

    #[test]
    fn malformed_json_becomes_err_without_aborting_scan() {
        let text = r#"<aciddog-call>{not json}</aciddog-call>
<aciddog-call>{"cmd":"test"}</aciddog-call>"#;
        let calls = extract_calls(text);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].parsed.is_err());
        assert!(matches!(calls[1].parsed, Ok(RequestCmd::Test)));
    }

    #[test]
    fn unknown_cmd_becomes_err() {
        let text = r#"<aciddog-call>{"cmd":"fly_to_mars"}</aciddog-call>"#;
        let calls = extract_calls(text);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].parsed.is_err());
    }

    #[test]
    fn tags_inside_fenced_code_block_are_ignored() {
        let text = "Example:\n```\n<aciddog-call>{\"cmd\":\"test\"}</aciddog-call>\n```\nreal call:\n<aciddog-call>{\"cmd\":\"test\"}</aciddog-call>\n";
        let calls = extract_calls(text);
        // Only the out-of-fence call should be extracted.
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].parsed, Ok(RequestCmd::Test)));
    }

    #[test]
    fn unterminated_open_tag_is_ignored() {
        let text = "<aciddog-call>{\"cmd\":\"test\"";
        let calls = extract_calls(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn no_tags_yields_empty() {
        let calls = extract_calls("just a regular reply, nothing special");
        assert!(calls.is_empty());
    }

    #[test]
    fn render_responses_produces_indexed_blocks() {
        let responses = vec![
            (0, Response::ok("queued")),
            (1, Response::err("unknown agent")),
        ];
        let rendered = render_responses(&responses);
        assert!(rendered.contains("<aciddog-response index=\"0\">"));
        assert!(rendered.contains("\"ok\":true"));
        assert!(rendered.contains("\"message\":\"queued\""));
        assert!(rendered.contains("<aciddog-response index=\"1\">"));
        assert!(rendered.contains("\"ok\":false"));
        assert!(rendered.contains("\"error\":\"unknown agent\""));
    }

    #[test]
    fn render_responses_handles_empty_list() {
        assert_eq!(render_responses(&[]), "");
    }

    #[test]
    fn strip_tags_removes_call_block() {
        let text = "before <aciddog-call>{\"cmd\":\"test\"}</aciddog-call> after";
        assert_eq!(strip_tags(text), "before  after");
    }

    #[test]
    fn strip_tags_removes_response_block() {
        let text = "prefix <aciddog-response index=\"0\">{\"ok\":true}</aciddog-response> suffix";
        assert_eq!(strip_tags(text), "prefix  suffix");
    }

    #[test]
    fn strip_tags_removes_multiple_interleaved_blocks() {
        let text = "a <aciddog-call>{\"cmd\":\"test\"}</aciddog-call> b <aciddog-response index=\"0\">{\"ok\":true}</aciddog-response> c";
        assert_eq!(strip_tags(text), "a  b  c");
    }

    #[test]
    fn strip_tags_preserves_fenced_example() {
        // Fenced examples must survive so the model can document the
        // protocol without having its examples eaten by the stripper.
        let text = "```\n<aciddog-call>{\"cmd\":\"test\"}</aciddog-call>\n```\nand a real call <aciddog-call>{\"cmd\":\"test\"}</aciddog-call> done";
        let stripped = strip_tags(text);
        // Fence content preserved.
        assert!(stripped.contains("```"));
        assert!(stripped.contains("<aciddog-call>"));
        // Real (out-of-fence) call removed.
        assert!(stripped.contains("and a real call  done"));
    }

    #[test]
    fn strip_tags_noop_when_no_tags() {
        let text = "just regular text";
        assert_eq!(strip_tags(text), text);
    }

    #[test]
    fn strip_tags_drops_remainder_on_unterminated_tag() {
        let text = "ok so far <aciddog-call>{\"cmd\":\"test\"";
        assert_eq!(strip_tags(text), "ok so far ");
    }

    #[test]
    fn tagged_code_fenced_response_example_does_not_confuse_parser() {
        // Models often quote the protocol when explaining. Make sure a
        // quoted <aciddog-response> doesn't accidentally match.
        let text = "```
<aciddog-response index=\"0\">{\"ok\":true}</aciddog-response>
```
<aciddog-call>{\"cmd\":\"test\"}</aciddog-call>";
        let calls = extract_calls(text);
        assert_eq!(calls.len(), 1);
    }
}
