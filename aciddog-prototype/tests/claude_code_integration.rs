//! Live integration against the real `claude` CLI.
//!
//! Ignored by default so `cargo test` passes on machines without claude on PATH.
//! Run explicitly with: `cargo test --test claude_code_integration -- --ignored`

use aciddog::ai::{AICore, AIEvent, ClaudeCodeCore};
use futures::StreamExt;
use uuid::Uuid;

#[tokio::test]
#[ignore]
async fn live_turn_streams_events_and_reuses_session() {
    let expected_session = Uuid::new_v4();
    let mut core = ClaudeCodeCore::new(
        expected_session,
        "Respond with a single word.".into(),
        vec!["Read".into()],
        None,
    );
    assert_eq!(core.session_id(), Some(expected_session));

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
    let mut stream2 = core
        .turn("What word did you just say?")
        .await
        .expect("second turn");
    let mut saw_second_init = false;
    while let Some(ev) = stream2.next().await {
        if let AIEvent::SystemInit { session_id, .. } = ev {
            assert_eq!(session_id, expected_session);
            saw_second_init = true;
        }
    }
    assert!(
        saw_second_init,
        "second turn should re-emit SystemInit with same session id"
    );
}

#[tokio::test]
async fn missing_binary_yields_spawn_error_event() {
    // Spawn failures now surface as AIEvent::Error inside the returned
    // stream (the refactor for tag-chain mode moved child spawn into
    // the background task). The error wording must still reference the
    // failed spawn path.
    let mut core = ClaudeCodeCore::new(Uuid::new_v4(), "i".into(), vec![], None)
        .with_binary("/no/such/path/claude");
    let mut stream = core
        .turn("hi")
        .await
        .expect("turn() now always returns Ok; errors arrive via stream");
    let mut err_msg = None;
    while let Some(ev) = stream.next().await {
        if let AIEvent::Error { message, .. } = ev {
            err_msg = Some(message);
            break;
        }
    }
    let msg = err_msg.expect("expected AIEvent::Error in stream");
    assert!(
        msg.contains("not found") || msg.contains("spawn") || msg.contains("No such file"),
        "unexpected error message: {msg}"
    );
}
