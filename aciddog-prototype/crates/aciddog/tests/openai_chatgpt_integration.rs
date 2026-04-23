//! Integration test for `OpenAiChatgptCore` against a `wiremock` server.
//! Exercises the wire contract documented in the plan:
//!
//!   - correct base URL (`…/responses`)
//!   - mandatory headers: `Authorization: Bearer <access>`,
//!     `chatgpt-account-id`, `OpenAI-Beta: responses=experimental`,
//!     `originator: codex_cli_rs`
//!   - request body invariants: `store=false`, `stream=true`,
//!     `instructions` contains a bundled Codex prompt (not the user's
//!     system prompt), user system prompt lives in the first
//!     `input[]` item as `role: "developer"`
//!   - one complete turn: assistant text streamed via
//!     `response.output_text.delta`, terminated by
//!     `response.completed`, core emits `AssistantText` + `TurnResult`

use std::sync::{Arc, Mutex};

use chrono::Utc;
use futures::StreamExt;
use serde_json::Value;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use aciddog::ai::claude_code::PromptParts;
use aciddog::ai::core::AICore;
use aciddog::ai::event::AIEvent;
use aciddog::ai::openai_chatgpt::OpenAiChatgptCore;
use aciddog::openai_auth::AuthTokens;

fn sample_tokens() -> AuthTokens {
    AuthTokens {
        access_token: "TEST_ACCESS_TOKEN".into(),
        refresh_token: "TEST_REFRESH_TOKEN".into(),
        id_token: "TEST_ID_TOKEN".into(),
        account_id: "acc_test".into(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}

fn prompt_parts_with_instructions(s: &str) -> PromptParts {
    PromptParts {
        role: None,
        team: None,
        memory: None,
        instructions: s.to_string(),
        aciddog_cli: None,
    }
}

/// Minimal valid Responses-API SSE stream: one text delta, then
/// `response.completed` with usage. No tool calls, so the core's
/// native tool-use loop terminates after one round.
const OK_SSE_BODY: &str = "\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5.1-codex\"}}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n\n\
";

#[tokio::test]
async fn single_turn_round_trips_text_and_asserts_wire_contract() {
    let server = MockServer::start().await;

    // Capture the request so we can inspect headers/body after the
    // turn completes. Mount a Mock that both records the request and
    // streams back the canned SSE transcript.
    let captured = Arc::new(Mutex::new(Vec::<Captured>::new()));
    let cap = captured.clone();

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(move |req: &Request| {
            cap.lock().unwrap().push(Captured::from(req));
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(OK_SSE_BODY.as_bytes().to_vec(), "text/event-stream")
        })
        .mount(&server)
        .await;

    let session_id = Uuid::new_v4();
    let mut core = OpenAiChatgptCore::new(
        session_id,
        "personal",
        "gpt-5.1-codex",
        "medium",
        prompt_parts_with_instructions("answer concisely"),
        vec![],
        Some("user system prompt here".into()),
    )
    .with_base_url(server.uri())
    .with_tokens_for_tests(sample_tokens());

    let mut stream = core.turn("say OK").await.expect("turn starts");

    let mut events = Vec::new();
    while let Some(ev) = stream.next().await {
        events.push(ev);
    }

    // Collect expected events: SystemInit, AssistantText("OK"),
    // TurnResult with "completed". The native tool-use loop returns
    // after response.completed since no tool calls were seen.
    assert!(matches!(events.first(), Some(AIEvent::SystemInit { .. })));
    let text = events
        .iter()
        .find_map(|e| match e {
            AIEvent::AssistantText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .expect("expected AssistantText event");
    assert_eq!(text, "OK");
    let stop = events
        .iter()
        .find_map(|e| match e {
            AIEvent::TurnResult { stop_reason, .. } => stop_reason.clone(),
            _ => None,
        })
        .expect("expected TurnResult event");
    assert_eq!(stop, "completed");

    // Now inspect the captured HTTP request.
    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected exactly one /responses POST");
    let req = &captured[0];

    // Headers
    assert_eq!(
        req.header("authorization").unwrap(),
        "Bearer TEST_ACCESS_TOKEN"
    );
    assert_eq!(req.header("chatgpt-account-id").unwrap(), "acc_test");
    assert_eq!(req.header("openai-beta").unwrap(), "responses=experimental");
    assert_eq!(req.header("originator").unwrap(), "codex_cli_rs");
    assert_eq!(req.header("session_id").unwrap(), &session_id.to_string());
    assert_eq!(
        req.header("conversation_id").unwrap(),
        &session_id.to_string()
    );

    // Body invariants
    assert_eq!(req.body["store"], false);
    assert_eq!(req.body["stream"], true);
    assert_eq!(req.body["reasoning"]["effort"], "medium");
    assert_eq!(req.body["model"], "gpt-5.1-codex");

    // Instructions is the bundled Codex prompt, never the user's
    // system prompt. For gpt-5.1-codex the bundled file begins
    // "You are Codex".
    let instructions = req.body["instructions"]
        .as_str()
        .expect("instructions is string");
    assert!(
        instructions.starts_with("You are Codex"),
        "instructions must be the bundled Codex prompt, got: {}",
        &instructions[..50.min(instructions.len())]
    );
    assert!(!instructions.contains("user system prompt here"));
    assert!(!instructions.contains("answer concisely"));

    // User system prompt + aciddog preamble lives in the first
    // input[] item as role: "developer".
    let first_input = &req.body["input"][0];
    assert_eq!(first_input["type"], "message");
    assert_eq!(first_input["role"], "developer");
    let dev_text = first_input["content"][0]["text"]
        .as_str()
        .expect("developer text");
    assert!(
        dev_text.contains("user system prompt here"),
        "developer item should carry the user system prompt, got: {dev_text}"
    );

    // Second input item is the user message we passed to `turn`. It
    // is sanitised (no `id` at any depth) and carries the original
    // text + the first-turn preamble (because `started=false` initially).
    let second_input = &req.body["input"][1];
    assert_eq!(second_input["type"], "message");
    assert_eq!(second_input["role"], "user");
    assert!(second_input.get("id").is_none(), "item id must be stripped");
    let user_text = second_input["content"][0]["text"]
        .as_str()
        .expect("user text");
    assert!(user_text.contains("say OK"));
}

/// Minimal capture — we keep only the pieces we assert on to avoid
/// pulling in the full wiremock `Request` across `await` points.
struct Captured {
    headers: Vec<(String, String)>,
    body: Value,
}

impl Captured {
    fn from(req: &Request) -> Self {
        let headers = req
            .headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_ascii_lowercase(),
                    v.to_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        Self { headers, body }
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}
