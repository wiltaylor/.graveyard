use serde_json::Value;
use uuid::Uuid;

// Re-exported so every `AIEvent::Error { kind, .. }` producer site can
// reach the classification enum through the same `use super::event::*` /
// `use crate::ai::event::*` import they already have for `AIEvent`.
// Defined in the codex core because it's the only core that emits
// non-`Unknown` variants today; the cross-core lift is a separate spike.
pub use crate::ai::codex_cli::CodexFailureKind;

/// Token usage in aciddog's canonical accounting shape.
///
/// `input_tokens` excludes cache reads. Providers that report cache
/// reads as a subset of input tokens, such as OpenAI/Codex, must
/// subtract that cached portion at the parser before constructing this
/// value. `cache_read_input_tokens` and `cache_creation_input_tokens`
/// remain separate because they still occupy context and are included
/// in [`TurnUsage::total_tokens`].
///
/// A core may emit this as a single-turn delta or as a cumulative
/// session total; consumers choose add-vs-replace based on the core's
/// documented reporting style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl TurnUsage {
    /// Total tokens that count against the context window for this
    /// turn. Cache reads are billed cheaper but still occupy context,
    /// so they're included in the displayed "used" figure.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_input_tokens)
            .saturating_add(self.cache_creation_input_tokens)
    }

    pub fn is_zero(&self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.cache_read_input_tokens == 0
            && self.cache_creation_input_tokens == 0
    }
}

/// Which reset window a [`AIEvent::RateLimit`] refers to. The Claude CLI
/// currently emits `five_hour` and `weekly`; any other string is treated
/// as unknown and the event falls back to [`AIEvent::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitKind {
    FiveHour,
    Weekly,
}

#[derive(Debug, Clone)]
pub enum AIEvent {
    SystemInit {
        session_id: Uuid,
        tools: Vec<String>,
        model: Option<String>,
        raw: Value,
    },
    AssistantText {
        text: String,
        raw: Value,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        raw: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
        is_error: bool,
        raw: Value,
    },
    TurnResult {
        stop_reason: Option<String>,
        total_cost_usd: Option<f64>,
        /// Token usage for this turn. `None` when the core can't tell us.
        /// For Claude Code CLI and Gemini CLI this is *cumulative* over the
        /// whole session; for the API cores it's the *delta* for this turn.
        usage: Option<TurnUsage>,
        raw: Value,
    },
    Error {
        /// Best-effort classification. Defaults to
        /// [`CodexFailureKind::Unknown`] at every producer site outside
        /// the codex CLI's two new detectors, preserving today's behaviour
        /// for consumers that don't discriminate on this field.
        kind: CodexFailureKind,
        message: String,
        raw: Option<Value>,
    },
    /// Emitted by the orchestrator (not the core itself) when an
    /// in-flight turn is cancelled via `/stop` or `/sleep`. Lets the
    /// TUI render a `[stopped]` marker and reset agent status to Ready.
    Stopped,
    /// Window reset announcement emitted by the `claude` CLI on every
    /// turn (stream-json mode). The Cores view uses these to display a
    /// reset countdown without ever touching the user's OAuth token.
    RateLimit {
        kind: RateLimitKind,
        resets_at_unix: Option<i64>,
        status: String,
        is_using_overage: bool,
        raw: Value,
    },
    /// Orchestrator-emitted: an automaton pulled a message off its
    /// priority queue and spawned a child process to handle it. The
    /// TUI flips the chat's status to `Working` on this event so the
    /// row icon reflects the in-flight state before the child has
    /// written any output.
    AutomatonStarted,
    /// Orchestrator-emitted: an automaton child process just printed a
    /// line on stdout. The TUI appends it as a plain-text block to the
    /// automaton's chat pane.
    AutomatonStdout {
        line: String,
    },
    /// Orchestrator-emitted: an automaton child printed a line on
    /// stderr. Styled distinctly from stdout in the TUI.
    AutomatonStderr {
        line: String,
    },
    /// Orchestrator-emitted: an automaton child has exited. `code` is
    /// `None` when the child was killed by a signal or by the
    /// `timeout_seconds` watchdog. `duration_ms` is wall-clock time
    /// from spawn to reap.
    AutomatonExit {
        code: Option<i32>,
        duration_ms: u128,
        timed_out: bool,
    },
    Other(Value),
}

/// Parse one line of claude stream-json output into zero or more `AIEvent`s.
///
/// A single `assistant` or `user` line may contain multiple content blocks
/// (text + tool_use, or several tool_results), which expand to multiple events.
pub fn parse_line(line: &str) -> Vec<AIEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return vec![AIEvent::Error {
                kind: CodexFailureKind::Unknown,
                message: format!("failed to parse stream line: {e}"),
                raw: None,
            }];
        }
    };

    let ty = value.get("type").and_then(Value::as_str).unwrap_or("");

    match ty {
        "system" => parse_system(value),
        "assistant" => parse_assistant(value),
        "user" => parse_user(value),
        "result" => parse_result(value),
        "rate_limit_event" => parse_rate_limit(value),
        _ => vec![AIEvent::Other(value)],
    }
}

/// Parse a `rate_limit_event` line from the claude CLI stream. Permissive:
/// an unrecognised `rateLimitType` or missing `rate_limit_info` object
/// falls back to [`AIEvent::Other`] so forward-compat windows don't
/// produce spurious `Error` events in the TUI.
fn parse_rate_limit(raw: Value) -> Vec<AIEvent> {
    let Some(info) = raw.get("rate_limit_info").and_then(Value::as_object) else {
        return vec![AIEvent::Other(raw)];
    };
    let kind = match info.get("rateLimitType").and_then(Value::as_str) {
        Some("five_hour") => RateLimitKind::FiveHour,
        Some("weekly") => RateLimitKind::Weekly,
        _ => return vec![AIEvent::Other(raw)],
    };
    let resets_at_unix = info.get("resetsAt").and_then(Value::as_i64);
    let status = info
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_using_overage = info
        .get("isUsingOverage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    vec![AIEvent::RateLimit {
        kind,
        resets_at_unix,
        status,
        is_using_overage,
        raw,
    }]
}

fn parse_system(raw: Value) -> Vec<AIEvent> {
    let subtype = raw.get("subtype").and_then(Value::as_str).unwrap_or("");
    if subtype != "init" {
        return vec![AIEvent::Other(raw)];
    }

    let session_id_str = raw.get("session_id").and_then(Value::as_str);
    let session_id = match session_id_str.and_then(|s| Uuid::parse_str(s).ok()) {
        Some(id) => id,
        None => {
            return vec![AIEvent::Error {
                kind: CodexFailureKind::Unknown,
                message: format!(
                    "system.init missing or invalid session_id: {:?}",
                    session_id_str
                ),
                raw: Some(raw),
            }];
        }
    };

    let tools = raw
        .get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let model = raw.get("model").and_then(Value::as_str).map(str::to_string);

    vec![AIEvent::SystemInit {
        session_id,
        tools,
        model,
        raw,
    }]
}

/// Pull a `TurnUsage` out of an Anthropic-style `usage` object. Returns
/// `None` if the value isn't an object or has none of the expected
/// fields. Used by the claude CLI's stream-json output (which mirrors
/// the Anthropic wire format) and by the Anthropic API core itself.
pub fn parse_usage_anthropic(usage: &Value) -> Option<TurnUsage> {
    let obj = usage.as_object()?;
    let mut u = TurnUsage::default();
    let mut any = false;
    if let Some(v) = obj.get("input_tokens").and_then(Value::as_u64) {
        u.input_tokens = v;
        any = true;
    }
    if let Some(v) = obj.get("output_tokens").and_then(Value::as_u64) {
        u.output_tokens = v;
        any = true;
    }
    if let Some(v) = obj.get("cache_read_input_tokens").and_then(Value::as_u64) {
        u.cache_read_input_tokens = v;
        any = true;
    }
    if let Some(v) = obj
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64)
    {
        u.cache_creation_input_tokens = v;
        any = true;
    }
    if any {
        Some(u)
    } else {
        None
    }
}

fn parse_assistant(raw: Value) -> Vec<AIEvent> {
    let Some(blocks) = raw.pointer("/message/content").and_then(Value::as_array) else {
        return vec![AIEvent::Other(raw)];
    };

    let mut events = Vec::with_capacity(blocks.len());
    for block in blocks {
        let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "text" => {
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                events.push(AIEvent::AssistantText {
                    text,
                    raw: block.clone(),
                });
            }
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                events.push(AIEvent::ToolUse {
                    id,
                    name,
                    input,
                    raw: block.clone(),
                });
            }
            _ => events.push(AIEvent::Other(block.clone())),
        }
    }
    events
}

fn parse_user(raw: Value) -> Vec<AIEvent> {
    let Some(blocks) = raw.pointer("/message/content").and_then(Value::as_array) else {
        return vec![AIEvent::Other(raw)];
    };

    let mut events = Vec::new();
    for block in blocks {
        let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
        if kind != "tool_result" {
            continue;
        }
        let tool_use_id = block
            .get("tool_use_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let content = block.get("content").cloned().unwrap_or(Value::Null);
        let is_error = block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        events.push(AIEvent::ToolResult {
            tool_use_id,
            content,
            is_error,
            raw: block.clone(),
        });
    }

    if events.is_empty() {
        vec![AIEvent::Other(raw)]
    } else {
        events
    }
}

fn parse_result(raw: Value) -> Vec<AIEvent> {
    let stop_reason = raw
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    let total_cost_usd = raw.get("total_cost_usd").and_then(Value::as_f64);
    // claude CLI: cumulative usage at the top level of the result event.
    let usage = raw.get("usage").and_then(parse_usage_anthropic);
    vec![AIEvent::TurnResult {
        stop_reason,
        total_cost_usd,
        usage,
        raw,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_line_produces_no_events() {
        assert!(parse_line("").is_empty());
        assert!(parse_line("   \n").is_empty());
    }

    #[test]
    fn malformed_json_becomes_error_event() {
        let evs = parse_line("{not json");
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Error { .. }));
    }

    #[test]
    fn unknown_type_becomes_other() {
        let evs = parse_line(r#"{"type":"mystery","x":1}"#);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn parses_system_init_with_session_id() {
        let line = r#"{"type":"system","subtype":"init","session_id":"550e8400-e29b-41d4-a716-446655440000","tools":["Bash","Edit"],"model":"claude-opus-4-7"}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::SystemInit {
                session_id,
                tools,
                model,
                ..
            } => {
                assert_eq!(
                    session_id.to_string(),
                    "550e8400-e29b-41d4-a716-446655440000"
                );
                assert_eq!(tools, &vec!["Bash".to_string(), "Edit".to_string()]);
                assert_eq!(model.as_deref(), Some("claude-opus-4-7"));
            }
            other => panic!("expected SystemInit, got {other:?}"),
        }
    }

    #[test]
    fn system_init_with_bad_session_id_becomes_error() {
        let line = r#"{"type":"system","subtype":"init","session_id":"not-a-uuid"}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Error { .. }));
    }

    #[test]
    fn parses_assistant_text_block() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::AssistantText { text, .. } => assert_eq!(text, "hello"),
            other => panic!("expected AssistantText, got {other:?}"),
        }
    }

    #[test]
    fn parses_assistant_tool_use_block() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolUse {
                id, name, input, ..
            } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "Bash");
                assert_eq!(input.get("command").and_then(Value::as_str), Some("ls"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn assistant_with_mixed_blocks_emits_multiple_events() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"text","text":"running"},
            {"type":"tool_use","id":"t1","name":"Bash","input":{}}
        ]}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], AIEvent::AssistantText { .. }));
        assert!(matches!(evs[1], AIEvent::ToolUse { .. }));
    }

    #[test]
    fn parses_user_tool_result() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"done","is_error":false}]}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::ToolResult {
                tool_use_id,
                is_error,
                ..
            } => {
                assert_eq!(tool_use_id, "t1");
                assert!(!is_error);
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parses_result_event() {
        let line = r#"{"type":"result","subtype":"success","stop_reason":"end_turn","total_cost_usd":0.00042}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::TurnResult {
                stop_reason,
                total_cost_usd,
                usage,
                ..
            } => {
                assert_eq!(stop_reason.as_deref(), Some("end_turn"));
                assert_eq!(*total_cost_usd, Some(0.00042));
                assert!(usage.is_none());
            }
            other => panic!("expected TurnResult, got {other:?}"),
        }
    }

    #[test]
    fn parses_result_event_with_usage() {
        let line = r#"{
            "type":"result",
            "subtype":"success",
            "stop_reason":"end_turn",
            "total_cost_usd":0.0042,
            "usage":{
                "input_tokens": 12,
                "output_tokens": 34,
                "cache_read_input_tokens": 100,
                "cache_creation_input_tokens": 50
            }
        }"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::TurnResult { usage, .. } => {
                let u = usage.expect("usage should be Some");
                assert_eq!(u.input_tokens, 12);
                assert_eq!(u.output_tokens, 34);
                assert_eq!(u.cache_read_input_tokens, 100);
                assert_eq!(u.cache_creation_input_tokens, 50);
                assert_eq!(u.total_tokens(), 12 + 34 + 100 + 50);
            }
            other => panic!("expected TurnResult, got {other:?}"),
        }
    }

    #[test]
    fn parses_rate_limit_event_five_hour() {
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1776924000,"rateLimitType":"five_hour","isUsingOverage":false}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::RateLimit {
                kind,
                resets_at_unix,
                status,
                is_using_overage,
                ..
            } => {
                assert_eq!(*kind, RateLimitKind::FiveHour);
                assert_eq!(*resets_at_unix, Some(1776924000));
                assert_eq!(status, "allowed");
                assert!(!is_using_overage);
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }
    }

    #[test]
    fn parses_rate_limit_event_weekly() {
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1777500000,"rateLimitType":"weekly","isUsingOverage":true}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AIEvent::RateLimit {
                kind,
                is_using_overage,
                ..
            } => {
                assert_eq!(*kind, RateLimitKind::Weekly);
                assert!(*is_using_overage);
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }
    }

    #[test]
    fn rate_limit_event_with_unknown_type_is_other() {
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1,"rateLimitType":"monthly"}}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn rate_limit_event_missing_info_is_other() {
        let line = r#"{"type":"rate_limit_event"}"#;
        let evs = parse_line(line);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AIEvent::Other(_)));
    }

    #[test]
    fn parse_usage_anthropic_returns_none_for_empty_object() {
        let v = serde_json::json!({});
        assert!(parse_usage_anthropic(&v).is_none());
    }

    #[test]
    fn parse_usage_anthropic_handles_partial_fields() {
        let v = serde_json::json!({"input_tokens": 5});
        let u = parse_usage_anthropic(&v).expect("Some");
        assert_eq!(u.input_tokens, 5);
        assert_eq!(u.output_tokens, 0);
    }
}
