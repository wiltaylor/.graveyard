//! Live integration against the real `claude` CLI.
//!
//! Ignored by default so `cargo test` passes on machines without claude on PATH.
//! Run explicitly with: `cargo test --test claude_code_integration -- --ignored`

use aciddog::ai::{AICore, AIEvent, ClaudeCodeCore};
use futures::StreamExt;

#[tokio::test]
#[ignore]
async fn live_turn_streams_events_and_reuses_session() {
    let mut core = ClaudeCodeCore::new(
        "Respond with a single word.".into(),
        vec!["Read".into()],
    );
    let expected_session = core.session_id().expect("session id always present");

    let mut stream = core
        .turn("Reply with exactly the word READY and nothing else.")
        .await
        .expect("spawn claude");

    let mut saw_init = false;
    let mut saw_text = false;
    let mut saw_result = false;

    while let Some(ev) = stream.next().await {
        match ev {
            AIEvent::SystemInit { session_id, .. } => {
                assert_eq!(session_id, expected_session);
                saw_init = true;
            }
            AIEvent::AssistantText { .. } => saw_text = true,
            AIEvent::TurnResult { .. } => saw_result = true,
            AIEvent::Error { message, .. } => panic!("unexpected error event: {message}"),
            _ => {}
        }
    }

    assert!(saw_init, "expected a SystemInit event");
    assert!(saw_text, "expected at least one AssistantText event");
    assert!(saw_result, "expected a TurnResult event");

    // Second turn reuses the same session id.
    let mut stream2 = core.turn("What word did you just say?").await.expect("second turn");
    let mut saw_second_init = false;
    while let Some(ev) = stream2.next().await {
        if let AIEvent::SystemInit { session_id, .. } = ev {
            assert_eq!(session_id, expected_session);
            saw_second_init = true;
        }
    }
    assert!(saw_second_init, "second turn should re-emit SystemInit with same session id");
}

#[tokio::test]
async fn missing_binary_yields_spawn_error() {
    use aciddog::ai::AIError;

    let mut core = ClaudeCodeCore::new("i".into(), vec![])
        .with_binary("/no/such/path/claude");
    match core.turn("hi").await {
        Err(AIError::NotFound) | Err(AIError::Spawn(_)) => {}
        Err(other) => panic!("expected NotFound/Spawn, got {other:?}"),
        Ok(_) => panic!("expected spawn failure"),
    }
}
