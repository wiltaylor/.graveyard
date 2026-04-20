use serde_json::Value;
use uuid::Uuid;

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
        raw: Value,
    },
    Error {
        message: String,
        raw: Option<Value>,
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
        _ => vec![AIEvent::Other(value)],
    }
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

    let model = raw
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);

    vec![AIEvent::SystemInit {
        session_id,
        tools,
        model,
        raw,
    }]
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
    vec![AIEvent::TurnResult {
        stop_reason,
        total_cost_usd,
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
                ..
            } => {
                assert_eq!(stop_reason.as_deref(), Some("end_turn"));
                assert_eq!(*total_cost_usd, Some(0.00042));
            }
            other => panic!("expected TurnResult, got {other:?}"),
        }
    }
}
